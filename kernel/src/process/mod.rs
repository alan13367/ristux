use alloc::{format, string::String, vec::Vec};
use core::{cmp, ptr, slice};

use crate::{
    arch::x86_64::fpu,
    fs,
    memory::{
        address_space::{AddressSpace, UserProtection},
        frame_allocator::{self, FRAME_SIZE},
        paging::{self, PageFlags},
    },
    security::{Access, Credentials, FileMetadata},
    sync::spinlock::SpinLock,
    task::scheduler,
    userspace::elf,
};

static PROCESS_TABLE: SpinLock<Option<ProcessTable>> = SpinLock::new(None);
static CURRENT_PID: SpinLock<Option<Pid>> = SpinLock::new(None);

pub type Pid = u64;

pub const MAX_FDS: usize = 256;
const MAX_USER_ARGS: usize = 32;
const MAX_USER_ENVS: usize = 16;
pub const FD_CLOEXEC: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked(BlockReason),
    Stopped(u8),
    Zombie(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockReason {
    WaitChild(Pid),
    WaitIo,
    WaitIoUntil(u64),
}

#[derive(Clone, Copy)]
struct FdEntry {
    user_fd: usize,
    vfs_fd: usize,
    status_flags: u32,
    fd_flags: u32,
}

#[derive(Clone, Copy)]
struct TimedWait {
    key: u64,
    deadline_ms: u64,
}

pub struct Process {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub name: String,
    pub cwd: String,
    pub pgrp: Pid,
    pub sid: Pid,
    controlling_tty: Option<ControllingTty>,
    pub pending_signals: u64,
    pending_signal_status: Option<i32>,
    pub signal_mask: u64,
    pub signal_handlers: [usize; 32],
    pub state: ProcessState,
    pub address_space: AddressSpace,
    pub credentials: Credentials,
    pub umask: u16,
    rlimit_nofile_cur: u64,
    rlimit_nofile_max: u64,
    fds: Vec<FdEntry>,
    socket_handles: Vec<usize>,
    timed_wait: Option<TimedWait>,
    next_fd: usize,
    pub entry: u64,
    pub stack_top: usize,
    pub argc: usize,
    pub argv_ptr: usize,
    pub envp_ptr: usize,
    exit_status: Option<i32>,
    waiters: Vec<Pid>,
    is_user: bool,
    saved_syscall: Option<SavedSyscallFrame>,
    fpu_state: fpu::FpuState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControllingTty {
    Console,
    Pty(usize),
}

/// Saved register frame for resuming a blocked syscall (mirrors SyscallInterruptFrame).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SavedSyscallFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

pub struct ProcessTable {
    processes: Vec<Process>,
    next_pid: Pid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitStatus {
    Exited(i32),
    Stopped(u8),
}

impl Process {
    fn new_user(pid: Pid, parent: Option<Pid>, name: &str, credentials: Credentials) -> Self {
        let address_space =
            AddressSpace::new_kernel_clone().expect("failed to create user address space");
        Self {
            pid,
            parent,
            name: String::from(name),
            cwd: String::from("/"),
            pgrp: pid,
            sid: parent.unwrap_or(pid),
            controlling_tty: Some(ControllingTty::Console),
            pending_signals: 0,
            pending_signal_status: None,
            signal_mask: 0,
            signal_handlers: [0; 32],
            state: ProcessState::Ready,
            address_space,
            credentials,
            umask: 0o022,
            rlimit_nofile_cur: MAX_FDS as u64,
            rlimit_nofile_max: MAX_FDS as u64,
            fds: Vec::new(),
            socket_handles: Vec::new(),
            timed_wait: None,
            next_fd: 3,
            entry: 0,
            stack_top: paging::USER_STACK_TOP,
            argc: 0,
            argv_ptr: 0,
            envp_ptr: 0,
            exit_status: None,
            waiters: Vec::new(),
            is_user: true,
            saved_syscall: None,
            fpu_state: fpu::initial_state(),
        }
    }

    fn init_stdio(&mut self) {
        let tty = fs::open("/dev/tty").unwrap_or(0);
        let console = fs::open("/dev/console").unwrap_or(1);
        self.set_fd(0, tty);
        self.set_fd(1, console);
        self.set_fd(2, console);
    }

    fn set_fd(&mut self, user_fd: usize, vfs_fd: usize) {
        self.set_fd_with_flags(user_fd, vfs_fd, 0, 0);
    }

    fn set_fd_with_flags(
        &mut self,
        user_fd: usize,
        vfs_fd: usize,
        status_flags: u32,
        fd_flags: u32,
    ) {
        if let Some(entry) = self.fds.iter_mut().find(|e| e.user_fd == user_fd) {
            entry.vfs_fd = vfs_fd;
            entry.status_flags = status_flags;
            entry.fd_flags = fd_flags;
            return;
        }
        if self.fds.len() >= MAX_FDS {
            panic!("too many file descriptors");
        }
        self.fds.push(FdEntry {
            user_fd,
            vfs_fd,
            status_flags,
            fd_flags,
        });
        if user_fd >= self.next_fd {
            self.next_fd = user_fd + 1;
        }
    }

    fn lookup_fd(&self, user_fd: usize) -> Option<usize> {
        self.fds
            .iter()
            .find(|e| e.user_fd == user_fd)
            .map(|e| e.vfs_fd)
    }

    fn push_fd(&mut self, vfs_fd: usize) -> usize {
        self.push_fd_with_flags(vfs_fd, 0)
    }

    fn push_fd_with_flags(&mut self, vfs_fd: usize, status_flags: u32) -> usize {
        self.push_fd_with_all_flags(vfs_fd, status_flags, 0)
    }

    fn push_fd_with_all_flags(&mut self, vfs_fd: usize, status_flags: u32, fd_flags: u32) -> usize {
        let user_fd = self.lowest_available_fd();
        self.set_fd_with_flags(user_fd, vfs_fd, status_flags, fd_flags);
        user_fd
    }

    fn lowest_available_fd(&self) -> usize {
        let mut candidate = 0;
        loop {
            if self.fds.iter().all(|entry| entry.user_fd != candidate) {
                return candidate;
            }
            candidate += 1;
        }
    }

    fn remove_fd(&mut self, user_fd: usize) -> Option<usize> {
        let index = self.fds.iter().position(|e| e.user_fd == user_fd)?;
        Some(self.fds.swap_remove(index).vfs_fd)
    }

    fn replace_fd_with_flags(
        &mut self,
        user_fd: usize,
        vfs_fd: usize,
        status_flags: u32,
        fd_flags: u32,
    ) -> Option<usize> {
        if let Some(entry) = self.fds.iter_mut().find(|e| e.user_fd == user_fd) {
            let old = entry.vfs_fd;
            entry.vfs_fd = vfs_fd;
            entry.status_flags = status_flags;
            entry.fd_flags = fd_flags;
            return Some(old);
        }
        self.set_fd_with_flags(user_fd, vfs_fd, status_flags, fd_flags);
        None
    }

    fn status_flags(&self, user_fd: usize) -> Option<u32> {
        self.fds
            .iter()
            .find(|e| e.user_fd == user_fd)
            .map(|e| e.status_flags)
    }

    fn set_status_flags(&mut self, user_fd: usize, flags: u32) -> bool {
        if let Some(entry) = self.fds.iter_mut().find(|e| e.user_fd == user_fd) {
            entry.status_flags = flags;
            return true;
        }
        false
    }

    fn fd_flags(&self, user_fd: usize) -> Option<u32> {
        self.fds
            .iter()
            .find(|e| e.user_fd == user_fd)
            .map(|e| e.fd_flags)
    }

    fn set_fd_flags(&mut self, user_fd: usize, flags: u32) -> bool {
        if let Some(entry) = self.fds.iter_mut().find(|e| e.user_fd == user_fd) {
            entry.fd_flags = flags;
            return true;
        }
        false
    }

    fn close_on_exec_fds(&mut self) -> Vec<usize> {
        let mut closed = Vec::new();
        let mut index = 0;
        while index < self.fds.len() {
            if self.fds[index].fd_flags & FD_CLOEXEC != 0 {
                closed.push(self.fds[index].vfs_fd);
                self.fds.swap_remove(index);
            } else {
                index += 1;
            }
        }
        closed
    }

    fn close_on_exec_socket_handles(&mut self) -> Vec<usize> {
        let mut closed = Vec::new();
        let mut index = 0;
        while index < self.socket_handles.len() {
            let handle = self.socket_handles[index];
            let cloexec = crate::net::socket::with_sockets(|table| {
                table
                    .fd_flags(handle)
                    .map(|flags| flags & FD_CLOEXEC != 0)
                    .unwrap_or(false)
            });
            if cloexec {
                closed.push(handle);
                self.socket_handles.swap_remove(index);
            } else {
                index += 1;
            }
        }
        closed
    }

    fn add_socket_handle(&mut self, handle: usize) {
        if !self.socket_handles.contains(&handle) {
            self.socket_handles.push(handle);
        }
    }

    fn remove_socket_handle(&mut self, handle: usize) -> bool {
        let Some(index) = self
            .socket_handles
            .iter()
            .position(|entry| *entry == handle)
        else {
            return false;
        };
        self.socket_handles.swap_remove(index);
        true
    }

    fn owns_socket_handle(&self, handle: usize) -> bool {
        self.socket_handles.contains(&handle)
    }

    fn close_socket_handles(&mut self) {
        let handles = core::mem::take(&mut self.socket_handles);
        for handle in handles {
            let _ = crate::net::socket::with_sockets(|table| table.close(handle));
        }
    }

    fn timed_wait_deadline(&mut self, key: u64, timeout_ms: u64, now_ms: u64) -> u64 {
        if let Some(wait) = self.timed_wait {
            if wait.key == key {
                return wait.deadline_ms;
            }
        }
        let deadline_ms = now_ms.saturating_add(timeout_ms);
        self.timed_wait = Some(TimedWait { key, deadline_ms });
        deadline_ms
    }

    fn clear_timed_wait(&mut self, key: u64) {
        if matches!(self.timed_wait, Some(wait) if wait.key == key) {
            self.timed_wait = None;
        }
    }

    fn allows(&self, addr: usize, len: usize) -> bool {
        self.address_space.allows(addr, len)
    }

    fn load_elf(&mut self, path: &str, data: &[u8]) -> Result<u64, elf::ElfError> {
        let old = core::mem::replace(
            &mut self.address_space,
            AddressSpace::new_kernel_clone().map_err(|_| elf::ElfError::Unsupported)?,
        );
        old.destroy();
        self.address_space.activate();
        let mut segments = 0;
        let entry = elf::for_each_load_segment(data, |segment| {
            map_elf_segment(&mut self.address_space, segment);
            segments += 1;
        })?;
        if segments == 0 {
            return Err(elf::ElfError::Unsupported);
        }
        self.timed_wait = None;
        self.fpu_state = fpu::initial_state();
        self.name = String::from(path);
        self.entry = entry;
        Ok(entry)
    }

    fn setup_stack(&mut self, args: &[&str]) {
        self.setup_stack_with_env(args, &[]);
    }

    fn setup_stack_with_env(&mut self, args: &[&str], env: &[&str]) {
        self.address_space.activate();
        if args.len() > MAX_USER_ARGS {
            panic!("too many user arguments");
        }
        if env.len() > MAX_USER_ENVS {
            panic!("too many user environment entries");
        }
        let stack_top = paging::USER_STACK_TOP;
        let stack_bottom = stack_top - FRAME_SIZE;
        self.address_space
            .map_zero_page(stack_bottom)
            .expect("user stack map failed");
        self.address_space.stack_bottom = stack_bottom;
        self.address_space.stack_top = stack_top;

        let mut sp = stack_top;
        let mut arg_ptrs = [0usize; MAX_USER_ARGS];
        for (index, arg) in args.iter().enumerate() {
            arg_ptrs[index] = self.push_stack_string(&mut sp, arg);
        }
        let mut env_ptrs = [0usize; MAX_USER_ENVS];
        for (index, entry) in env.iter().enumerate() {
            env_ptrs[index] = self.push_stack_string(&mut sp, entry);
        }

        sp &= !0xf;
        let env_bytes = (env.len() + 1) * 8;
        sp -= env_bytes;
        self.ensure_stack_mapping(sp, env_bytes);
        let envp_ptr = sp;
        unsafe {
            for index in 0..env.len() {
                *(envp_ptr as *mut u64).add(index) = env_ptrs[index] as u64;
            }
            *(envp_ptr as *mut u64).add(env.len()) = 0;
        }

        let argv_bytes = (args.len() + 1) * 8;
        sp -= argv_bytes;
        self.ensure_stack_mapping(sp, argv_bytes);
        let argv_ptr = sp;
        unsafe {
            for index in 0..args.len() {
                *(argv_ptr as *mut u64).add(index) = arg_ptrs[index] as u64;
            }
            *(argv_ptr as *mut u64).add(args.len()) = 0;
        }

        sp = (sp & !0xf).saturating_sub(8);
        self.ensure_stack_mapping(sp, 8);
        self.stack_top = sp;
        self.argc = args.len();
        self.argv_ptr = argv_ptr;
        self.envp_ptr = envp_ptr;
    }

    fn push_stack_string(&mut self, sp: &mut usize, text: &str) -> usize {
        let bytes = text.as_bytes();
        *sp -= bytes.len() + 1;
        *sp &= !0xf;
        self.ensure_stack_mapping(*sp, bytes.len() + 1);
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), *sp as *mut u8, bytes.len());
            *(*sp as *mut u8).add(bytes.len()) = 0;
        }
        *sp
    }

    fn ensure_stack_mapping(&mut self, addr: usize, len: usize) {
        let end = addr.saturating_add(len.max(1));
        let mut page = paging::align_down(addr, FRAME_SIZE);
        let page_end = paging::align_up(end, FRAME_SIZE);
        while page < page_end {
            if page <= paging::USER_STACK_GUARD || page + FRAME_SIZE > paging::USER_STACK_TOP {
                panic!("user stack exhausted");
            }
            if !self.address_space.allows(page, FRAME_SIZE) {
                self.address_space
                    .map_zero_page(page)
                    .expect("user stack map failed");
            }
            if page < self.address_space.stack_bottom {
                self.address_space.stack_bottom = page;
            }
            page += FRAME_SIZE;
        }
    }

    fn destroy(&mut self) {
        for entry in &self.fds {
            let _ = fs::close(entry.vfs_fd);
        }
        self.fds.clear();
        self.close_socket_handles();
        let old = core::mem::replace(
            &mut self.address_space,
            AddressSpace::new_kernel_clone().expect("address space"),
        );
        old.destroy();
    }
}

impl ProcessTable {
    fn new() -> Self {
        Self {
            processes: Vec::new(),
            next_pid: 1,
        }
    }

    fn spawn_init(&mut self) -> Pid {
        let pid = self.next_pid;
        self.next_pid += 1;
        let mut process = Process::new_user(pid, None, "init", Credentials::root());
        process.init_stdio();
        self.processes.push(process);
        pid
    }

    fn get_mut(&mut self, pid: Pid) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.pid == pid)
    }

    fn get(&self, pid: Pid) -> Option<&Process> {
        self.processes.iter().find(|p| p.pid == pid)
    }

    fn fork(&mut self, parent: Pid) -> Option<Pid> {
        let parent_proc = self.get(parent)?.clone_process()?;
        let pid = self.next_pid;
        self.next_pid += 1;
        let mut child = parent_proc;
        child.pid = pid;
        child.parent = Some(parent);
        child.state = ProcessState::Ready;
        child.waiters.clear();
        child.exit_status = None;
        child.pending_signals = 0;
        child.pending_signal_status = None;
        self.processes.push(child);
        crate::sched::on_fork(pid);
        Some(pid)
    }

    fn exec(&mut self, pid: Pid, path: &str, args: &[&str]) -> bool {
        let Some(data) = fs::read_file(path) else {
            return false;
        };
        let index = match self.processes.iter().position(|p| p.pid == pid) {
            Some(i) => i,
            None => return false,
        };
        {
            let process = &mut self.processes[index];
            for entry in &process.fds {
                let _ = fs::close(entry.vfs_fd);
            }
            process.fds.clear();
            process.next_fd = 3;
        }
        let process = &mut self.processes[index];
        process.init_stdio();
        if process.load_elf(path, &data).is_err() {
            return false;
        }
        process.setup_stack(args);
        process.state = ProcessState::Ready;
        clear_current();
        true
    }

    fn exit(&mut self, pid: Pid, status: i32) -> Vec<Pid> {
        let was_current = current_pid() == Some(pid);
        let mut wake = Vec::new();
        let waiters = {
            let process = match self.get_mut(pid) {
                Some(p) => p,
                None => return wake,
            };
            process.state = ProcessState::Zombie(status);
            process.exit_status = Some(status);
            if was_current {
                process.address_space.activate();
            }
            for entry in &process.fds {
                let _ = fs::close(entry.vfs_fd);
            }
            process.fds.clear();
            process.close_socket_handles();
            let mut old = AddressSpace::new_kernel_clone().expect("address space");
            core::mem::swap(&mut process.address_space, &mut old);
            old.destroy();
            if was_current {
                clear_current();
            }
            core::mem::take(&mut process.waiters)
        };
        for waiter in waiters {
            if let Some(parent) = self.get_mut(waiter) {
                if matches!(parent.state, ProcessState::Blocked(BlockReason::WaitChild(child)) if child == pid || child == 0)
                {
                    parent.state = ProcessState::Ready;
                    wake.push(waiter);
                }
            }
        }
        if let Some(parent) = self.get(pid).and_then(|p| p.parent) {
            if let Some(parent_proc) = self.get_mut(parent) {
                if matches!(parent_proc.state, ProcessState::Blocked(BlockReason::WaitChild(child)) if child == pid || child == 0)
                {
                    parent_proc.state = ProcessState::Ready;
                    wake.push(parent);
                }
            }
        }
        wake
    }

    fn wake_waiters_for(&mut self, pid: Pid) -> Vec<Pid> {
        let mut wake = Vec::new();
        let waiters = self
            .get_mut(pid)
            .map(|process| core::mem::take(&mut process.waiters))
            .unwrap_or_default();
        for waiter in waiters {
            if let Some(parent) = self.get_mut(waiter) {
                if matches!(parent.state, ProcessState::Blocked(BlockReason::WaitChild(child)) if child == pid || child == 0)
                {
                    parent.state = ProcessState::Ready;
                    wake.push(waiter);
                }
            }
        }
        if let Some(parent) = self.get(pid).and_then(|p| p.parent) {
            if let Some(parent_proc) = self.get_mut(parent) {
                if matches!(parent_proc.state, ProcessState::Blocked(BlockReason::WaitChild(child)) if child == pid || child == 0)
                {
                    parent_proc.state = ProcessState::Ready;
                    wake.push(parent);
                }
            }
        }
        wake
    }

    fn stop(&mut self, pid: Pid, signal: u8) -> Option<Vec<Pid>> {
        let process = self.get_mut(pid)?;
        if matches!(process.state, ProcessState::Zombie(_)) {
            return Some(Vec::new());
        }
        process.state = ProcessState::Stopped(signal);
        process.exit_status = None;
        if current_pid() == Some(pid) {
            clear_current();
        }
        Some(self.wake_waiters_for(pid))
    }

    fn continue_process(&mut self, pid: Pid) -> bool {
        let Some(process) = self.get_mut(pid) else {
            return false;
        };
        if matches!(process.state, ProcessState::Stopped(_)) {
            process.state = ProcessState::Ready;
            return true;
        }
        !matches!(process.state, ProcessState::Zombie(_))
    }

    fn wait(&mut self, parent: Pid, child: Pid) -> Option<i32> {
        let state = self.get(child).map(|p| p.state);
        match state {
            Some(ProcessState::Zombie(status)) => {
                if self.get(child).map(|p| p.parent) == Some(Some(parent)) {
                    self.reap(child);
                    Some(status)
                } else {
                    None
                }
            }
            Some(_) => {
                self.get_mut(parent)?.state = ProcessState::Blocked(BlockReason::WaitChild(child));
                None
            }
            None => None,
        }
    }

    fn reap(&mut self, pid: Pid) {
        if let Some(index) = self.processes.iter().position(|p| p.pid == pid) {
            let mut process = self.processes.remove(index);
            process.destroy();
        }
    }

    fn signal(&mut self, pid: Pid, status: i32, current: Option<Pid>) -> Option<Vec<Pid>> {
        if current == Some(pid) {
            let process = self.get_mut(pid)?;
            process.pending_signal_status = Some(status);
            if status >= 128 {
                let signal = (status - 128) as u64;
                if signal < 64 {
                    process.pending_signals |= 1 << signal;
                }
            }
            Some(Vec::new())
        } else if self.get(pid).is_some() {
            if status == crate::signal::Signal::Tstp.default_status() {
                return self.stop(pid, crate::signal::Signal::Tstp.number());
            }
            Some(self.exit(pid, status))
        } else {
            None
        }
    }
}

impl Process {
    fn clone_process(&self) -> Option<Self> {
        let address_space = self.address_space.clone_full_copy().ok()?;
        let mut fds = self.fds.clone();
        for entry in &mut fds {
            if let Ok(dup) = fs::duplicate_fd(entry.vfs_fd) {
                entry.vfs_fd = dup;
            }
        }
        let socket_handles = self.socket_handles.clone();
        for handle in &socket_handles {
            crate::net::socket::with_sockets(|table| {
                let _ = table.duplicate(*handle);
            });
        }
        Some(Self {
            pid: self.pid,
            parent: self.parent,
            name: self.name.clone(),
            cwd: self.cwd.clone(),
            pgrp: self.pgrp,
            sid: self.sid,
            controlling_tty: self.controlling_tty,
            pending_signals: self.pending_signals,
            pending_signal_status: None,
            signal_mask: self.signal_mask,
            signal_handlers: self.signal_handlers,
            state: ProcessState::Ready,
            address_space,
            credentials: self.credentials,
            umask: self.umask,
            rlimit_nofile_cur: self.rlimit_nofile_cur,
            rlimit_nofile_max: self.rlimit_nofile_max,
            fds,
            socket_handles,
            timed_wait: None,
            next_fd: self.next_fd,
            entry: self.entry,
            stack_top: self.stack_top,
            argc: self.argc,
            argv_ptr: self.argv_ptr,
            envp_ptr: self.envp_ptr,
            exit_status: None,
            waiters: Vec::new(),
            is_user: self.is_user,
            saved_syscall: None,
            fpu_state: self.fpu_state,
        })
    }
}

pub fn init() {
    let mut table = ProcessTable::new();
    let init = table.spawn_init();
    crate::println!("Process table initialized with init pid {}.", init);
    *PROCESS_TABLE.lock() = Some(table);
    self_test();
}

pub fn fork(parent: Pid) -> Option<Pid> {
    with_table(|table| table.fork(parent))
}

pub fn exec(pid: Pid, path: &str) -> bool {
    exec_with_args(pid, path, &[path])
}

pub fn exec_with_args(pid: Pid, path: &str, args: &[&str]) -> bool {
    with_table(|table| table.exec(pid, path, args))
}

pub struct ExecInfo {
    pub entry: u64,
    pub stack_top: usize,
    pub argc: usize,
    pub argv_ptr: usize,
    pub envp_ptr: usize,
}

/// Execve invoked from a running user process. Replaces the address space of
/// `pid` with the program at `path`, preserves the existing file descriptors,
/// and returns the entry/stack info so the syscall dispatcher can patch the
/// outgoing iretq frame.
pub fn exec_for_user(pid: Pid, path: &str, args: &[&str], env: &[&str]) -> Option<ExecInfo> {
    with_table(|table| {
        let metadata = fs::stat(path).ok()?;
        let data = fs::read_file(path)?;
        let index = table.processes.iter().position(|p| p.pid == pid)?;
        let credentials = table.processes[index].credentials;
        let file = FileMetadata::new(metadata.owner, metadata.group, metadata.mode);
        if !file.can_access(credentials, Access::Execute) {
            return None;
        }
        let close_on_exec = table.processes[index].close_on_exec_fds();
        for fd in close_on_exec {
            let _ = fs::close(fd);
        }
        let close_on_exec_sockets = table.processes[index].close_on_exec_socket_handles();
        for handle in close_on_exec_sockets {
            let _ = crate::net::socket::with_sockets(|socket_table| socket_table.close(handle));
        }
        // Preserve fds across exec except descriptors marked FD_CLOEXEC.
        if table.processes[index].load_elf(path, &data).is_err() {
            return None;
        }
        table.processes[index].setup_stack_with_env(args, env);
        if metadata.mode & 0o4000 != 0 {
            table.processes[index].credentials.euid = metadata.owner;
        }
        if metadata.mode & 0o2000 != 0 {
            table.processes[index].credentials.egid = metadata.group;
        }
        table.processes[index].state = ProcessState::Running;
        let p = &table.processes[index];
        Some(ExecInfo {
            entry: p.entry,
            stack_top: p.stack_top,
            argc: p.argc,
            argv_ptr: p.argv_ptr,
            envp_ptr: p.envp_ptr,
        })
    })
}

pub fn get_parent(pid: Pid) -> Option<Pid> {
    with_table(|table| table.get(pid).and_then(|p| p.parent))
}

pub fn install_pipe_fds(pipefd: usize, read_vfs: usize, write_vfs: usize) -> Result<(), ()> {
    install_pipe_fds_with_flags(pipefd, read_vfs, write_vfs, 0, 0)
}

pub fn install_pipe_fds_with_flags(
    pipefd: usize,
    read_vfs: usize,
    write_vfs: usize,
    status_flags: u32,
    fd_flags: u32,
) -> Result<(), ()> {
    let parent = current_pid().ok_or(())?;
    let (user_read, user_write) = with_table(|table| {
        let process = table.get_mut(parent).ok_or(())?;
        let user_read = process.push_fd_with_all_flags(read_vfs, status_flags, fd_flags);
        let user_write = process.push_fd_with_all_flags(write_vfs, status_flags, fd_flags);
        Ok((user_read, user_write))
    })?;
    let Some(out) = write_user_buffer(pipefd, 8) else {
        let _ = user_close(user_read);
        let _ = user_close(user_write);
        return Err(());
    };
    out[0..4].copy_from_slice(&(user_read as u32).to_le_bytes());
    out[4..8].copy_from_slice(&(user_write as u32).to_le_bytes());
    Ok(())
}

pub fn exit(pid: Pid, status: i32) {
    let wake = with_table(|table| table.exit(pid, status));
    for pid in wake {
        scheduler::wake_blocked(pid);
    }
}

pub fn wait(parent: Pid, child: Pid) -> Option<i32> {
    with_table(|table| table.wait(parent, child))
}

pub fn signal(pid: Pid, status: i32) -> bool {
    let current = current_pid();
    let wake = with_table(|table| table.signal(pid, status, current));
    if let Some(wake) = wake {
        for pid in wake {
            scheduler::wake_blocked(pid);
        }
        true
    } else {
        false
    }
}

pub fn stop_current_signal(pid: Pid, signal: u8) -> bool {
    let wake = with_table(|table| table.stop(pid, signal));
    if let Some(wake) = wake {
        for pid in wake {
            scheduler::wake_blocked(pid);
        }
        true
    } else {
        false
    }
}

pub fn continue_process(pid: Pid) -> bool {
    let continued = with_table(|table| table.continue_process(pid));
    if continued {
        scheduler::wake_blocked(pid);
    }
    continued
}

pub fn take_pending_signal_current() -> Option<(Pid, usize, i32)> {
    let pid = current_pid()?;
    let status = with_table(|table| {
        let process = table.get_mut(pid)?;
        let status = process.pending_signal_status?;
        if status >= 128 {
            let signal = (status - 128) as u64;
            if signal < 64 && process.signal_mask & (1 << signal) != 0 {
                return None;
            }
        }
        let status = process.pending_signal_status.take()?;
        process.pending_signals = 0;
        Some(status)
    })?;
    let signum = if status >= 128 {
        (status - 128) as usize
    } else {
        0
    };
    Some((pid, signum, status))
}

pub fn current_signal_mask() -> Option<u64> {
    with_current_read(|p| p.signal_mask)
}

pub fn current_pending_signals() -> Option<u64> {
    with_current_read(|p| p.pending_signals)
}

pub fn set_current_signal_mask(mask: u64) -> Option<u64> {
    const UNBLOCKABLE: u64 = 1 << crate::signal::Signal::Kill.number();
    with_current(|p| {
        let old = p.signal_mask;
        p.signal_mask = mask & !UNBLOCKABLE;
        old
    })
}

pub fn pids_in_pgrp(pgrp: Pid) -> Vec<Pid> {
    with_table(|table| {
        table
            .processes
            .iter()
            .filter(|process| {
                process.pgrp == pgrp && !matches!(process.state, ProcessState::Zombie(_))
            })
            .map(|process| process.pid)
            .collect::<Vec<_>>()
    })
}

pub fn signal_pgrp(pgrp: Pid, status: i32) -> bool {
    let pids = pids_in_pgrp(pgrp);
    let mut delivered = false;
    for pid in pids {
        delivered |= signal(pid, status);
    }
    delivered
}

pub fn current_pgrp() -> Option<Pid> {
    with_current_read(|process| process.pgrp)
}

pub fn setsid_current() -> Option<Pid> {
    let pid = current_pid()?;
    with_table(|table| {
        let process = table.get_mut(pid)?;
        if process.pgrp == process.pid {
            return None;
        }
        process.sid = process.pid;
        process.pgrp = process.pid;
        process.controlling_tty = None;
        Some(process.sid)
    })
}

pub fn detach_current_controlling_tty() -> bool {
    with_current(|process| {
        process.controlling_tty = None;
        true
    })
    .unwrap_or(false)
}

pub fn set_current_controlling_pty(pty: usize) -> bool {
    with_current(|process| {
        process.controlling_tty = Some(ControllingTty::Pty(pty));
        true
    })
    .unwrap_or(false)
}

pub fn set_pgid(pid: Pid, pgid: Pid) -> bool {
    let caller = current_pid();
    with_table(|table| {
        let target_pid = if pid == 0 { caller.unwrap_or(0) } else { pid };
        if target_pid == 0 {
            return false;
        }
        let target_pgid = if pgid == 0 { target_pid } else { pgid };
        let Some(process) = table.get_mut(target_pid) else {
            return false;
        };
        process.pgrp = target_pgid;
        true
    })
}

pub fn set_signal_handler(pid: Pid, signal: usize, handler: usize) -> Option<usize> {
    with_table(|table| {
        let process = table.get_mut(pid)?;
        if signal >= process.signal_handlers.len() {
            return None;
        }
        let old = process.signal_handlers[signal];
        process.signal_handlers[signal] = handler;
        Some(old)
    })
}

pub fn get_signal_handler(pid: Pid, signal: usize) -> Option<usize> {
    with_table(|table| {
        let process = table.get(pid)?;
        process.signal_handlers.get(signal).copied()
    })
}

pub fn signal_handler(pid: Pid, signal: usize) -> Option<usize> {
    with_table(|table| {
        let process = table.get(pid)?;
        process.signal_handlers.get(signal).copied()
    })
}

pub fn set_current(pid: Pid) {
    *CURRENT_PID.lock() = Some(pid);
    let p4 = with_table(|table| {
        for process in &mut table.processes {
            if process.pid == pid {
                process.state = ProcessState::Running;
            } else if matches!(process.state, ProcessState::Running) {
                process.state = ProcessState::Ready;
            }
        }
        table.get(pid).map(|p| p.address_space.p4_phys())
    });
    if let Some(p4) = p4 {
        unsafe {
            paging::switch_cr3(p4);
        }
    }
}

pub fn clear_current() {
    *CURRENT_PID.lock() = None;
    unsafe {
        paging::switch_cr3(paging::boot_root_table() as usize);
    }
}

pub fn current_pid() -> Option<Pid> {
    *CURRENT_PID.lock()
}

pub fn save_current_fpu() {
    let Some(pid) = current_pid() else {
        return;
    };
    with_table(|table| {
        if let Some(process) = table.get_mut(pid) {
            fpu::save(&mut process.fpu_state);
        }
    });
}

pub fn restore_current_fpu() {
    let Some(pid) = current_pid() else {
        return;
    };
    with_table(|table| {
        if let Some(process) = table.get(pid) {
            fpu::restore(&process.fpu_state);
        }
    });
}

pub fn is_runnable(pid: Pid) -> bool {
    with_table(|table| {
        table
            .get(pid)
            .map(|p| matches!(p.state, ProcessState::Ready))
            .unwrap_or(false)
    })
}

pub fn mark_ready(pid: Pid) -> bool {
    with_table(|table| {
        let Some(process) = table.get_mut(pid) else {
            return false;
        };
        if matches!(
            process.state,
            ProcessState::Zombie(_) | ProcessState::Stopped(_)
        ) {
            return false;
        }
        process.state = ProcessState::Ready;
        true
    })
}

pub fn with_current<T>(f: impl FnOnce(&mut Process) -> T) -> Option<T> {
    let pid = current_pid()?;
    Some(with_table(|table| {
        let process = table.get_mut(pid).expect("current process missing");
        f(process)
    }))
}

pub fn with_current_read<T>(f: impl FnOnce(&Process) -> T) -> Option<T> {
    let pid = current_pid()?;
    Some(with_table(|table| {
        let process = table.get(pid).expect("current process missing");
        f(process)
    }))
}

pub fn read_user(addr: usize, len: usize) -> Option<&'static [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    with_current_read(|p| {
        if !p.allows(addr, len) {
            return None;
        }
        Some(unsafe { slice::from_raw_parts(addr as *const u8, len) })
    })?
}

pub fn write_user_buffer(addr: usize, len: usize) -> Option<&'static mut [u8]> {
    if len == 0 {
        return Some(&mut []);
    }
    with_current(|p| {
        if !p.allows(addr, len) {
            return None;
        }
        Some(unsafe { slice::from_raw_parts_mut(addr as *mut u8, len) })
    })?
}

fn normalize_process_path(path: &str) -> Result<String, fs::vfs::VfsError> {
    if !path.starts_with('/') {
        return Err(fs::vfs::VfsError::NotFound);
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                let _ = parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let mut normalized = String::from("/");
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            normalized.push('/');
        }
        normalized.push_str(part);
    }
    Ok(normalized)
}

fn resolve_process_path(process: &Process, path: &str) -> Result<String, fs::vfs::VfsError> {
    if path.is_empty() {
        return Err(fs::vfs::VfsError::NotFound);
    }
    if path.starts_with('/') {
        return normalize_process_path(path);
    }
    let mut combined = if process.cwd.is_empty() {
        String::from("/")
    } else {
        process.cwd.clone()
    };
    if !combined.ends_with('/') {
        combined.push('/');
    }
    combined.push_str(path);
    normalize_process_path(&combined)
}

fn resolve_open_path(process: &Process, path: &str) -> Result<String, fs::vfs::VfsError> {
    let path = resolve_process_path(process, path)?;
    if path != "/dev/tty" {
        return Ok(path);
    }
    match process.controlling_tty {
        Some(ControllingTty::Console) => Ok(String::from("/dev/tty")),
        Some(ControllingTty::Pty(pty)) => Ok(format!("/dev/pts/{}", pty)),
        None => Err(fs::vfs::VfsError::NotFound),
    }
}

pub fn resolve_current_path(path: &str) -> Result<String, fs::vfs::VfsError> {
    with_current_read(|p| resolve_process_path(p, path)).unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_open(path: &str) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_open_path(p, path)?;
        let vfs_fd = fs::open_read_as(&path, p.credentials)?;
        Ok(p.push_fd(vfs_fd))
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_open_options(
    path: &str,
    read: bool,
    write: bool,
    create: bool,
    exclusive: bool,
    truncate: bool,
    append: bool,
    status_flags: u32,
    create_mode: u16,
) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_open_path(p, path)?;
        let create_mode = create_mode & !p.umask & 0o7777;
        let vfs_fd = if create {
            match fs::open_with_rights_as(&path, p.credentials, read, write) {
                Ok(fd) if exclusive => {
                    let _ = fs::close(fd);
                    return Err(fs::vfs::VfsError::AlreadyExists);
                }
                Ok(fd) if !truncate => fd,
                Ok(fd) => {
                    let _ = fs::close(fd);
                    fs::create_file_with_mode_as(&path, p.credentials, create_mode)?
                }
                Err(fs::vfs::VfsError::NotFound) => {
                    fs::create_file_with_mode_as(&path, p.credentials, create_mode)?
                }
                Err(err) => return Err(err),
            }
        } else if truncate {
            let fd = fs::open_with_rights_as(&path, p.credentials, read, write)?;
            let _ = fs::close(fd);
            fs::create_file_as(&path, p.credentials)?
        } else {
            fs::open_with_rights_as(&path, p.credentials, read, write)?
        };
        if append {
            let _ = fs::lseek(vfs_fd, 0, 2);
        }
        Ok(p.push_fd_with_flags(vfs_fd, status_flags))
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_create(path: &str) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        let mode = 0o644 & !p.umask;
        let vfs_fd = fs::create_file_with_mode_as(&path, p.credentials, mode)?;
        Ok(p.push_fd(vfs_fd))
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_vfs_fd(user_fd: usize) -> Option<usize> {
    with_current_read(|p| p.lookup_fd(user_fd)).flatten()
}

pub fn user_close(user_fd: usize) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let Some(vfs_fd) = p.remove_fd(user_fd) else {
            return Err(fs::vfs::VfsError::BadFd);
        };
        fs::close(vfs_fd)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn install_socket_handle(handle: usize) -> Result<(), ()> {
    with_current(|p| {
        p.add_socket_handle(handle);
        Ok(())
    })
    .unwrap_or(Err(()))
}

pub fn owns_socket_handle(handle: usize) -> bool {
    with_current_read(|p| p.owns_socket_handle(handle)).unwrap_or(false)
}

pub fn user_close_socket_handle(handle: usize) -> Result<(), ()> {
    let removed = with_current(|p| {
        if !p.remove_socket_handle(handle) {
            return false;
        }
        true
    })
    .unwrap_or(false);
    if !removed {
        return Err(());
    }
    crate::net::socket::with_sockets(|table| table.close(handle)).map_err(|_| ())
}

pub fn timed_wait_deadline(key: u64, timeout_ms: u64, now_ms: u64) -> Option<u64> {
    with_current(|p| p.timed_wait_deadline(key, timeout_ms, now_ms))
}

pub fn clear_timed_wait(key: u64) {
    let _ = with_current(|p| {
        p.clear_timed_wait(key);
    });
}

pub fn has_timed_wait(key: u64) -> bool {
    with_current_read(|p| matches!(p.timed_wait, Some(wait) if wait.key == key)).unwrap_or(false)
}

pub fn user_dup(user_fd: usize) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let status_flags = p.status_flags(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let vfs_fd = p.lookup_fd(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let dup = fs::duplicate_fd(vfs_fd)?;
        Ok(p.push_fd_with_flags(dup, status_flags))
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_dup2(user_fd: usize, target_fd: usize) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        if user_fd == target_fd {
            if p.lookup_fd(user_fd).is_some() {
                return Ok(target_fd);
            }
            return Err(fs::vfs::VfsError::BadFd);
        }
        let status_flags = p.status_flags(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let vfs_fd = p.lookup_fd(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let dup = fs::duplicate_fd(vfs_fd)?;
        let old = p.replace_fd_with_flags(target_fd, dup, status_flags, 0);
        if let Some(old) = old {
            fs::close(old)?;
        }
        Ok(target_fd)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_dup3(
    user_fd: usize,
    target_fd: usize,
    fd_flags: u32,
) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        if user_fd == target_fd {
            return Err(fs::vfs::VfsError::BadFd);
        }
        let status_flags = p.status_flags(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let vfs_fd = p.lookup_fd(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let dup = fs::duplicate_fd(vfs_fd)?;
        let old = p.replace_fd_with_flags(target_fd, dup, status_flags, fd_flags);
        if let Some(old) = old {
            fs::close(old)?;
        }
        Ok(target_fd)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_fd_status_flags(user_fd: usize) -> Result<u32, fs::vfs::VfsError> {
    with_current_read(|p| p.status_flags(user_fd).ok_or(fs::vfs::VfsError::BadFd))
        .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_set_fd_status_flags(user_fd: usize, flags: u32) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        if p.set_status_flags(user_fd, flags) {
            Ok(())
        } else {
            Err(fs::vfs::VfsError::BadFd)
        }
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_fd_flags(user_fd: usize) -> Result<u32, fs::vfs::VfsError> {
    with_current_read(|p| p.fd_flags(user_fd).ok_or(fs::vfs::VfsError::BadFd))
        .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_set_fd_flags(user_fd: usize, flags: u32) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        if p.set_fd_flags(user_fd, flags) {
            Ok(())
        } else {
            Err(fs::vfs::VfsError::BadFd)
        }
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_mkdir(path: &str) -> Result<(), fs::vfs::VfsError> {
    user_mkdir_mode(path, 0o755)
}

pub fn user_mkdir_mode(path: &str, mode: u16) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        let mode = mode & !p.umask & 0o7777;
        fs::mkdir_with_mode_as(&path, p.credentials, mode)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_rmdir(path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        fs::rmdir_as(&path, p.credentials)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_unlink(path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        fs::unlink_as(&path, p.credentials)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_rename(old_path: &str, new_path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let old_path = resolve_process_path(p, old_path)?;
        let new_path = resolve_process_path(p, new_path)?;
        fs::rename_as(&old_path, &new_path, p.credentials)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_symlink(target: &str, link_path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let link_path = resolve_process_path(p, link_path)?;
        fs::symlink_as(target, &link_path, p.credentials)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_link(old_path: &str, new_path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let old_path = resolve_process_path(p, old_path)?;
        let new_path = resolve_process_path(p, new_path)?;
        fs::link_as(&old_path, &new_path, p.credentials)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_chown(path: &str, uid: u32, gid: u32) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        fs::chown_as(&path, uid, gid, p.credentials)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_chmod(path: &str, mode: u16) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        fs::chmod_as(&path, mode, p.credentials)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_access(
    path: &str,
    read: bool,
    write: bool,
    execute: bool,
) -> Result<(), fs::vfs::VfsError> {
    with_current_read(|p| {
        let path = resolve_process_path(p, path)?;
        fs::stat(&path)?;
        if read && !fs::can_access(&path, p.credentials, Access::Read)? {
            return Err(fs::vfs::VfsError::PermissionDenied);
        }
        if write && !fs::can_access(&path, p.credentials, Access::Write)? {
            return Err(fs::vfs::VfsError::PermissionDenied);
        }
        if execute && !fs::can_access(&path, p.credentials, Access::Execute)? {
            return Err(fs::vfs::VfsError::PermissionDenied);
        }
        Ok(())
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn handle_page_fault(fault_addr: usize, error_code: u64) -> bool {
    let user_fault = error_code & 0x4 != 0;
    let present = error_code & 0x1 != 0;
    let write_fault = error_code & 0x2 != 0;

    // Handle Copy-on-Write faults first
    if present && write_fault && fault_addr < 0x8000_0000 {
        let cow_handled = with_current(|process| {
            unsafe {
                if let Some(pte) = paging::get_pte_mut(process.address_space.p4, fault_addr) {
                    if *pte & paging::COW_FLAG != 0 {
                        let old_frame_phys = (*pte & paging::ADDR_MASK) as usize;
                        let ref_count = crate::memory::refcount::get(old_frame_phys);
                        if ref_count == 1 {
                            let mut flags = *pte & !paging::ADDR_MASK;
                            flags |= paging::WRITABLE_FLAG;
                            flags &= !paging::COW_FLAG;
                            *pte = (*pte & paging::ADDR_MASK) | flags;
                            paging::flush(fault_addr);
                            crate::smp::send_tlb_shootdown();
                            return true;
                        } else {
                            if let Some(new_frame) = frame_allocator::allocate_frame() {
                                let fault_page_addr = paging::align_down(fault_addr, FRAME_SIZE);
                                ptr::copy_nonoverlapping(
                                    fault_page_addr as *const u8,
                                    new_frame.start as *mut u8,
                                    FRAME_SIZE,
                                );
                                crate::memory::refcount::decrement(old_frame_phys);
                                let mut flags = *pte & !paging::ADDR_MASK;
                                flags |= paging::WRITABLE_FLAG;
                                flags &= !paging::COW_FLAG;
                                *pte = new_frame.start as u64 | flags;
                                paging::flush(fault_addr);
                                crate::smp::send_tlb_shootdown();

                                if let Some(pos) = process
                                    .address_space
                                    .user_mappings
                                    .iter()
                                    .position(|(v, _)| *v == fault_page_addr)
                                {
                                    process.address_space.user_mappings[pos] =
                                        (fault_page_addr, new_frame);
                                }
                                return true;
                            }
                        }
                    }
                }
            }
            false
        })
        .unwrap_or(false);

        if cow_handled {
            return true;
        }
    }

    if !user_fault {
        return false;
    }

    with_current(|process| {
        if present {
            return false;
        }
        if process.address_space.can_grow_stack(fault_addr) {
            process.address_space.grow_stack(fault_addr).is_ok()
        } else if fault_addr >= paging::USER_HEAP_START
            && fault_addr < process.address_space.heap_break + FRAME_SIZE
        {
            process
                .address_space
                .grow_heap(process.address_space.heap_break + FRAME_SIZE)
                .is_ok()
        } else {
            false
        }
    })
    .unwrap_or(false)
}

pub fn wake_io_waiters() {
    let woken: Vec<Pid> = {
        let mut guard = PROCESS_TABLE.lock();
        let Some(table) = guard.as_mut() else {
            return;
        };
        let mut woken = Vec::new();
        for process in &mut table.processes {
            if matches!(
                process.state,
                ProcessState::Blocked(BlockReason::WaitIo | BlockReason::WaitIoUntil(_))
            ) {
                process.state = ProcessState::Ready;
                woken.push(process.pid);
            }
        }
        woken
    };
    for pid in woken {
        crate::sched::wake_blocked(pid);
    }
}

pub fn wake_expired_io_waiters(now_ms: u64) {
    let woken: Vec<Pid> = {
        let mut guard = PROCESS_TABLE.lock();
        let Some(table) = guard.as_mut() else {
            return;
        };
        let mut woken = Vec::new();
        for process in &mut table.processes {
            if matches!(
                process.state,
                ProcessState::Blocked(BlockReason::WaitIoUntil(deadline_ms))
                    if now_ms >= deadline_ms
            ) {
                process.state = ProcessState::Ready;
                woken.push(process.pid);
            }
        }
        woken
    };
    for pid in woken {
        crate::sched::wake_blocked(pid);
    }
}

pub fn wake_io_waiters_for(pid: Pid) {
    let woke = {
        let mut guard = PROCESS_TABLE.lock();
        let Some(table) = guard.as_mut() else {
            return;
        };
        if let Some(p) = table.get_mut(pid) {
            if matches!(
                p.state,
                ProcessState::Blocked(BlockReason::WaitIo | BlockReason::WaitIoUntil(_))
            ) {
                p.state = ProcessState::Ready;
                true
            } else {
                false
            }
        } else {
            false
        }
    };
    if woke {
        crate::sched::wake_blocked(pid);
    }
}

pub fn block_current(reason: BlockReason) {
    if let Some(pid) = current_pid() {
        with_table(|table| {
            if let Some(process) = table.get_mut(pid) {
                process.state = ProcessState::Blocked(reason);
            }
        });
    }
}

pub fn save_syscall_frame(pid: Pid, frame: &SavedSyscallFrame) {
    with_table(|table| {
        if let Some(process) = table.get_mut(pid) {
            process.saved_syscall = Some(*frame);
        }
    });
}

pub fn restore_syscall_frame(pid: Pid, frame: &mut SavedSyscallFrame) -> bool {
    with_table(|table| {
        if let Some(process) = table.get_mut(pid) {
            if let Some(saved) = process.saved_syscall.take() {
                *frame = saved;
                return true;
            }
        }
        false
    })
}

pub fn wait_pid_blocking(parent: Pid, child: Pid) -> Option<i32> {
    loop {
        if let Some(status) = wait(parent, child) {
            return Some(status);
        }
        block_current(BlockReason::WaitChild(child));
        crate::arch::x86_64::instructions::halt();
    }
}

pub fn kill_current(status: i32) {
    if let Some(pid) = current_pid() {
        exit(pid, status);
    }
}

pub fn stats() -> ProcessStats {
    with_table(|table| ProcessStats {
        process_count: table.processes.len(),
        fd_count: table.processes.iter().map(|p| p.fds.len()).sum(),
        cwd_count: table.processes.iter().filter(|p| !p.cwd.is_empty()).count(),
        fd_path_checksum: table
            .processes
            .iter()
            .map(|p| p.fds.len() + p.pid as usize)
            .sum(),
    })
}

fn map_elf_segment(address_space: &mut AddressSpace, segment: elf::SegmentView<'_>) {
    address_space.activate();
    let segment_start = segment.vaddr;
    let segment_end = segment_start
        .checked_add(segment.mem_size)
        .expect("ELF segment end overflow");
    if segment_start == segment_end {
        return;
    }

    let file_end = segment_start
        .checked_add(segment.file_bytes.len())
        .expect("ELF file segment end overflow");
    let map_start = paging::align_down(segment_start, FRAME_SIZE);
    let map_end = paging::align_up(segment_end, FRAME_SIZE);

    for page in (map_start..map_end).step_by(FRAME_SIZE) {
        let frame = frame_allocator::allocate_frame().expect("ELF segment frame allocation failed");
        unsafe {
            ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
            let page_end = page + FRAME_SIZE;
            let copy_start = cmp::max(page, segment_start);
            let copy_end = cmp::min(page_end, file_end);
            if copy_start < copy_end {
                let source_offset = copy_start - segment_start;
                let target_offset = copy_start - page;
                ptr::copy_nonoverlapping(
                    segment.file_bytes.as_ptr().add(source_offset),
                    (frame.start + target_offset) as *mut u8,
                    copy_end - copy_start,
                );
            }
            address_space
                .map_owned_user_page(page, frame, PageFlags::USER_WRITABLE)
                .expect("ELF segment map failed");
        }
    }
}

fn self_test() {
    let parent = 1;
    let child = fork(parent).expect("fork self-test failed");
    if !exec(child, "/bin/echo") {
        panic!("exec self-test failed");
    }
    clear_current();
    exit(child, 0);
    if wait(parent, child) != Some(0) {
        panic!("wait self-test failed");
    }
    clear_current();
    crate::println!("Process model self-test passed: fork exec wait.");
}

pub fn with_table<T>(f: impl FnOnce(&mut ProcessTable) -> T) -> T {
    let mut guard = PROCESS_TABLE.lock();
    let table = guard
        .as_mut()
        .expect("process table used before initialization");
    f(table)
}

#[derive(Clone, Copy)]
pub struct ProcessStats {
    pub process_count: usize,
    pub fd_count: usize,
    pub cwd_count: usize,
    pub fd_path_checksum: usize,
}

// Re-export for userspace integration
pub fn prepare_user_run(
    pid: Pid,
    path: &str,
    args: &[&str],
    stdin_vfs_fd: Option<usize>,
    stdout_vfs_fd: Option<usize>,
    credentials: Credentials,
) -> Option<(u64, usize, usize, usize)> {
    with_table(|table| {
        if !table.exec(pid, path, args) {
            return None;
        }
        let process = table.get_mut(pid)?;
        process.credentials = credentials;
        if let Some(fd) = stdin_vfs_fd {
            process.set_fd(0, fd);
        }
        if let Some(fd) = stdout_vfs_fd {
            process.set_fd(1, fd);
            process.set_fd(2, fd);
        }
        Some((
            process.entry,
            process.stack_top,
            process.argc,
            process.argv_ptr,
        ))
    })
}

pub fn finish_user_run(pid: Pid) -> (i32, usize) {
    let unmapped = with_table(|table| {
        table
            .get(pid)
            .map(|p| p.address_space.mapping_count())
            .unwrap_or(0)
    });
    clear_current();
    (0, unmapped)
}

pub fn current_uid() -> u32 {
    with_current_read(|p| p.credentials.uid).unwrap_or(0)
}

pub fn current_euid() -> u32 {
    with_current_read(|p| p.credentials.euid).unwrap_or(0)
}

pub fn current_gid() -> u32 {
    with_current_read(|p| p.credentials.gid).unwrap_or(0)
}

pub fn current_egid() -> u32 {
    with_current_read(|p| p.credentials.egid).unwrap_or(0)
}

pub fn current_resuid() -> Option<(u32, u32, u32)> {
    with_current_read(|p| (p.credentials.uid, p.credentials.euid, p.credentials.suid))
}

pub fn current_resgid() -> Option<(u32, u32, u32)> {
    with_current_read(|p| (p.credentials.gid, p.credentials.egid, p.credentials.sgid))
}

pub fn set_current_uid(uid: u32) -> Result<(), ()> {
    with_current(|p| {
        if p.credentials.is_superuser() {
            p.credentials.uid = uid;
            p.credentials.euid = uid;
            p.credentials.suid = uid;
            return Ok(());
        }
        if uid == p.credentials.uid || uid == p.credentials.suid {
            p.credentials.euid = uid;
            return Ok(());
        }
        Err(())
    })
    .unwrap_or(Err(()))
}

pub fn set_current_gid(gid: u32) -> Result<(), ()> {
    with_current(|p| {
        if p.credentials.is_superuser() {
            p.credentials.gid = gid;
            p.credentials.egid = gid;
            p.credentials.sgid = gid;
            p.credentials.groups[0] = gid;
            p.credentials.group_count = 1;
            return Ok(());
        }
        if gid == p.credentials.gid || gid == p.credentials.sgid {
            p.credentials.egid = gid;
            return Ok(());
        }
        Err(())
    })
    .unwrap_or(Err(()))
}

pub fn set_current_resuid(
    ruid: Option<u32>,
    euid: Option<u32>,
    suid: Option<u32>,
) -> Result<(), ()> {
    with_current(|p| {
        let old = p.credentials;
        let allowed =
            |uid: u32| old.is_superuser() || uid == old.uid || uid == old.euid || uid == old.suid;
        if let Some(uid) = ruid {
            if !allowed(uid) {
                return Err(());
            }
        }
        if let Some(uid) = euid {
            if !allowed(uid) {
                return Err(());
            }
        }
        if let Some(uid) = suid {
            if !allowed(uid) {
                return Err(());
            }
        }
        if let Some(uid) = ruid {
            p.credentials.uid = uid;
        }
        if let Some(uid) = euid {
            p.credentials.euid = uid;
        }
        if let Some(uid) = suid {
            p.credentials.suid = uid;
        }
        Ok(())
    })
    .unwrap_or(Err(()))
}

pub fn set_current_resgid(
    rgid: Option<u32>,
    egid: Option<u32>,
    sgid: Option<u32>,
) -> Result<(), ()> {
    with_current(|p| {
        let old = p.credentials;
        let allowed =
            |gid: u32| old.is_superuser() || gid == old.gid || gid == old.egid || gid == old.sgid;
        if let Some(gid) = rgid {
            if !allowed(gid) {
                return Err(());
            }
        }
        if let Some(gid) = egid {
            if !allowed(gid) {
                return Err(());
            }
        }
        if let Some(gid) = sgid {
            if !allowed(gid) {
                return Err(());
            }
        }
        if let Some(gid) = rgid {
            p.credentials.gid = gid;
        }
        if let Some(gid) = egid {
            p.credentials.egid = gid;
        }
        if let Some(gid) = sgid {
            p.credentials.sgid = gid;
        }
        Ok(())
    })
    .unwrap_or(Err(()))
}

pub fn set_current_groups(groups: &[u32]) -> Result<(), ()> {
    with_current(|p| {
        if !p.credentials.is_superuser() || groups.len() > p.credentials.groups.len() {
            return Err(());
        }
        p.credentials.groups = [0; 8];
        for (index, group) in groups.iter().copied().enumerate() {
            p.credentials.groups[index] = group;
        }
        p.credentials.group_count = groups.len();
        Ok(())
    })
    .unwrap_or(Err(()))
}

pub fn current_groups() -> Option<Vec<u32>> {
    with_current_read(|p| {
        p.credentials.groups[..p.credentials.group_count.min(p.credentials.groups.len())].to_vec()
    })
}

pub fn set_current_umask(mask: u16) -> u16 {
    with_current(|p| {
        let old = p.umask;
        p.umask = mask & 0o777;
        old
    })
    .unwrap_or(0o022)
}

pub fn current_nofile_limit() -> Option<(u64, u64)> {
    with_current_read(|p| (p.rlimit_nofile_cur, p.rlimit_nofile_max))
}

pub fn set_current_nofile_limit(cur: u64, max: u64) -> Result<(), ()> {
    with_current(|p| {
        if cur > max || max > MAX_FDS as u64 {
            return Err(());
        }
        p.rlimit_nofile_cur = cur;
        p.rlimit_nofile_max = max;
        Ok(())
    })
    .unwrap_or(Err(()))
}

pub fn user_cwd() -> Option<String> {
    with_current_read(|p| p.cwd.clone())
}

pub fn user_chdir(path: &str) -> Result<(), ()> {
    with_current(|p| {
        let resolved = resolve_process_path(p, path).map_err(|_| ())?;
        let fd = fs::open_read_as(&resolved, p.credentials).map_err(|_| ())?;
        let is_directory = fs::directory_entries(fd).is_ok();
        let _ = fs::close(fd);
        if !is_directory {
            return Err(());
        }
        p.cwd = resolved;
        Ok(())
    })
    .unwrap_or(Err(()))
}

pub fn current_heap_break() -> usize {
    with_current_read(|p| p.address_space.heap_break).unwrap_or(0)
}

/// Wait for any child matching `child` (0 = any). Zombies are reaped; stopped
/// children are reported when requested and left waitable for a later `fg`.
pub fn wait_any(parent: Pid, child: Pid, include_stopped: bool) -> Option<(Pid, WaitStatus)> {
    with_table(|table| {
        if include_stopped {
            let stopped = table
                .processes
                .iter()
                .find(|p| {
                    p.parent == Some(parent)
                        && (child == 0 || p.pid == child)
                        && matches!(p.state, ProcessState::Stopped(_))
                })
                .map(|p| {
                    let ProcessState::Stopped(signal) = p.state else {
                        unreachable!();
                    };
                    (p.pid, signal)
                });
            if let Some((pid, signal)) = stopped {
                return Some((pid, WaitStatus::Stopped(signal)));
            }
        }

        let candidates: Vec<Pid> = table
            .processes
            .iter()
            .filter(|p| {
                p.parent == Some(parent)
                    && (child == 0 || p.pid == child)
                    && matches!(p.state, ProcessState::Zombie(_))
            })
            .map(|p| p.pid)
            .collect();
        if let Some(&zombie_pid) = candidates.first() {
            let status = match table.get(zombie_pid)?.state {
                ProcessState::Zombie(status) => status,
                _ => return None,
            };
            table.reap(zombie_pid);
            return Some((zombie_pid, WaitStatus::Exited(status)));
        }
        None
    })
}

pub fn has_child(parent: Pid, child: Pid) -> bool {
    with_table(|table| {
        table
            .processes
            .iter()
            .any(|p| p.parent == Some(parent) && (child == 0 || p.pid == child))
    })
}

pub fn brk(new_break: usize) -> Result<usize, ()> {
    with_current(|p| {
        p.address_space.grow_heap(new_break).map_err(|_| ())?;
        Ok(p.address_space.heap_break)
    })
    .ok_or(())?
}

pub fn mmap_anonymous(hint: usize, len: usize, protection: UserProtection) -> Result<usize, ()> {
    with_current(|p| {
        p.address_space
            .map_anonymous(hint, len, protection.page_flags())
            .map_err(|_| ())
    })
    .ok_or(())?
}

pub fn mmap_fixed(addr: usize, len: usize, protection: UserProtection) -> Result<usize, ()> {
    with_current(|p| {
        p.address_space
            .map_fixed(addr, len, protection.page_flags())
            .map_err(|_| ())
    })
    .ok_or(())?
}

pub fn munmap(addr: usize, len: usize) -> Result<(), ()> {
    with_current(|p| p.address_space.unmap_user_range(addr, len).map_err(|_| ())).ok_or(())?
}

pub fn mprotect(addr: usize, len: usize, protection: UserProtection) -> Result<(), ()> {
    with_current(|p| {
        p.address_space
            .protect_user_range(addr, len, protection)
            .map_err(|_| ())
    })
    .ok_or(())?
}

pub fn get_process_info(pid: Pid) -> Option<(String, ProcessState, Option<Pid>, Option<i32>)> {
    with_table(|table| {
        let p = table.get(pid)?;
        Some((p.name.clone(), p.state, p.parent, p.exit_status))
    })
}

pub fn list_process_ids() -> Vec<Pid> {
    with_table(|table| table.processes.iter().map(|p| p.pid).collect())
}
