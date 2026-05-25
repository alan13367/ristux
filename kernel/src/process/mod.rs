use alloc::{string::String, vec::Vec};
use core::{cmp, ptr, slice};

use crate::{
    fs,
    memory::{
        address_space::AddressSpace,
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

const MAX_FDS: usize = 16;
const MAX_USER_ARGS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked(BlockReason),
    Zombie(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockReason {
    WaitChild(Pid),
    WaitIo,
}

#[derive(Clone, Copy)]
struct FdEntry {
    user_fd: usize,
    vfs_fd: usize,
}

pub struct Process {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub name: String,
    pub cwd: String,
    pub pgrp: Pid,
    pub sid: Pid,
    pub pending_signals: u64,
    pub signal_mask: u64,
    pub signal_handlers: [usize; 32],
    pub state: ProcessState,
    pub address_space: AddressSpace,
    pub credentials: Credentials,
    fd_count: usize,
    fds: [FdEntry; MAX_FDS],
    next_fd: usize,
    pub entry: u64,
    pub stack_top: usize,
    pub argc: usize,
    pub argv_ptr: usize,
    exit_status: Option<i32>,
    waiters: Vec<Pid>,
    is_user: bool,
    saved_syscall: Option<SavedSyscallFrame>,
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
            pending_signals: 0,
            signal_mask: 0,
            signal_handlers: [0; 32],
            state: ProcessState::Ready,
            address_space,
            credentials,
            fd_count: 0,
            fds: [FdEntry {
                user_fd: 0,
                vfs_fd: 0,
            }; MAX_FDS],
            next_fd: 3,
            entry: 0,
            stack_top: paging::USER_STACK_TOP,
            argc: 0,
            argv_ptr: 0,
            exit_status: None,
            waiters: Vec::new(),
            is_user: true,
            saved_syscall: None,
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
        if let Some(entry) = self.fds[..self.fd_count]
            .iter_mut()
            .find(|e| e.user_fd == user_fd)
        {
            entry.vfs_fd = vfs_fd;
            return;
        }
        if self.fd_count >= MAX_FDS {
            panic!("too many file descriptors");
        }
        self.fds[self.fd_count] = FdEntry { user_fd, vfs_fd };
        self.fd_count += 1;
        if user_fd >= self.next_fd {
            self.next_fd = user_fd + 1;
        }
    }

    fn lookup_fd(&self, user_fd: usize) -> Option<usize> {
        self.fds[..self.fd_count]
            .iter()
            .find(|e| e.user_fd == user_fd)
            .map(|e| e.vfs_fd)
    }

    fn push_fd(&mut self, vfs_fd: usize) -> usize {
        let user_fd = self.next_fd;
        self.next_fd += 1;
        self.set_fd(user_fd, vfs_fd);
        user_fd
    }

    fn remove_fd(&mut self, user_fd: usize) -> Option<usize> {
        let index = self.fds[..self.fd_count]
            .iter()
            .position(|e| e.user_fd == user_fd)?;
        let vfs_fd = self.fds[index].vfs_fd;
        self.fd_count -= 1;
        self.fds[index] = self.fds[self.fd_count];
        Some(vfs_fd)
    }

    fn replace_fd(&mut self, user_fd: usize, vfs_fd: usize) -> Option<usize> {
        if let Some(entry) = self.fds[..self.fd_count]
            .iter_mut()
            .find(|e| e.user_fd == user_fd)
        {
            let old = entry.vfs_fd;
            entry.vfs_fd = vfs_fd;
            return Some(old);
        }
        self.set_fd(user_fd, vfs_fd);
        None
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
        self.name = String::from(path);
        self.entry = entry;
        Ok(entry)
    }

    fn setup_stack(&mut self, args: &[&str]) {
        self.address_space.activate();
        if args.len() > MAX_USER_ARGS {
            panic!("too many user arguments");
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
            let bytes = arg.as_bytes();
            sp -= bytes.len() + 1;
            sp &= !0xf;
            let page = paging::align_down(sp, FRAME_SIZE);
            if !self.address_space.allows(page, FRAME_SIZE) {
                self.address_space
                    .map_zero_page(page)
                    .expect("argv stack map failed");
            }
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), sp as *mut u8, bytes.len());
                *(sp as *mut u8).add(bytes.len()) = 0;
            }
            arg_ptrs[index] = sp;
        }

        sp &= !0xf;
        sp -= (args.len() + 1) * 8;
        sp -= 8;
        unsafe {
            for index in 0..args.len() {
                *(sp as *mut u64).add(index) = arg_ptrs[index] as u64;
            }
            *(sp as *mut u64).add(args.len()) = 0;
        }

        self.stack_top = sp;
        self.argc = args.len();
        self.argv_ptr = sp;
    }

    fn destroy(&mut self) {
        for index in 0..self.fd_count {
            let _ = fs::close(self.fds[index].vfs_fd);
        }
        self.fd_count = 0;
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
            for i in 0..process.fd_count {
                let _ = fs::close(process.fds[i].vfs_fd);
            }
            process.fd_count = 0;
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
            for index in 0..process.fd_count {
                let _ = fs::close(process.fds[index].vfs_fd);
            }
            process.fd_count = 0;
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

    fn signal(&mut self, pid: Pid, status: i32) -> Option<Vec<Pid>> {
        if self.get(pid).is_some() {
            Some(self.exit(pid, status))
        } else {
            None
        }
    }
}

impl Process {
    fn clone_process(&self) -> Option<Self> {
        let address_space = self.address_space.clone_full_copy().ok()?;
        let mut fds = self.fds;
        let fd_count = self.fd_count;
        for i in 0..fd_count {
            if let Ok(dup) = fs::duplicate_fd(fds[i].vfs_fd) {
                fds[i].vfs_fd = dup;
            }
        }
        Some(Self {
            pid: self.pid,
            parent: self.parent,
            name: self.name.clone(),
            cwd: self.cwd.clone(),
            pgrp: self.pgrp,
            sid: self.sid,
            pending_signals: self.pending_signals,
            signal_mask: self.signal_mask,
            signal_handlers: self.signal_handlers,
            state: ProcessState::Ready,
            address_space,
            credentials: self.credentials,
            fd_count,
            fds,
            next_fd: self.next_fd,
            entry: self.entry,
            stack_top: self.stack_top,
            argc: self.argc,
            argv_ptr: self.argv_ptr,
            exit_status: None,
            waiters: Vec::new(),
            is_user: self.is_user,
            saved_syscall: None,
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
}

/// Execve invoked from a running user process. Replaces the address space of
/// `pid` with the program at `path`, preserves the existing file descriptors,
/// and returns the entry/stack info so the syscall dispatcher can patch the
/// outgoing iretq frame.
pub fn exec_for_user(pid: Pid, path: &str, args: &[&str]) -> Option<ExecInfo> {
    with_table(|table| {
        let metadata = fs::stat(path).ok()?;
        let data = fs::read_file(path)?;
        let index = table.processes.iter().position(|p| p.pid == pid)?;
        let credentials = table.processes[index].credentials;
        let file = FileMetadata::new(metadata.owner, metadata.group, metadata.mode);
        if !file.can_access(credentials, Access::Execute) {
            return None;
        }
        // Preserve fds across exec (semantically correct execve behaviour).
        if table.processes[index].load_elf(path, &data).is_err() {
            return None;
        }
        table.processes[index].setup_stack(args);
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
        })
    })
}

pub fn get_parent(pid: Pid) -> Option<Pid> {
    with_table(|table| table.get(pid).and_then(|p| p.parent))
}

pub fn install_pipe_fds(pipefd: usize, read_vfs: usize, write_vfs: usize) -> Result<(), ()> {
    let parent = current_pid().ok_or(())?;
    let (user_read, user_write) = with_table(|table| {
        let process = table.get_mut(parent).ok_or(())?;
        let user_read = process.push_fd(read_vfs);
        let user_write = process.push_fd(write_vfs);
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
    let wake = with_table(|table| table.signal(pid, status));
    if let Some(wake) = wake {
        for pid in wake {
            scheduler::wake_blocked(pid);
        }
        true
    } else {
        false
    }
}

pub fn signal_pgrp(pgrp: Pid, status: i32) -> bool {
    let pids = with_table(|table| {
        table
            .processes
            .iter()
            .filter(|process| {
                process.pgrp == pgrp && !matches!(process.state, ProcessState::Zombie(_))
            })
            .map(|process| process.pid)
            .collect::<Vec<_>>()
    });
    let mut delivered = false;
    for pid in pids {
        delivered |= signal(pid, status);
    }
    delivered
}

pub fn current_pgrp() -> Option<Pid> {
    with_current_read(|process| process.pgrp)
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

pub fn is_runnable(pid: Pid) -> bool {
    with_table(|table| {
        table
            .get(pid)
            .map(|p| matches!(p.state, ProcessState::Ready))
            .unwrap_or(false)
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

pub fn user_open(path: &str) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let vfs_fd = fs::open_read_as(path, p.credentials)?;
        Ok(p.push_fd(vfs_fd))
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_open_options(
    path: &str,
    read: bool,
    write: bool,
    create: bool,
    truncate: bool,
    append: bool,
) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let vfs_fd = if create {
            match fs::open_with_rights_as(path, p.credentials, read, write) {
                Ok(fd) if !truncate => fd,
                Ok(fd) => {
                    let _ = fs::close(fd);
                    fs::create_file_as(path, p.credentials)?
                }
                Err(fs::vfs::VfsError::NotFound) => fs::create_file_as(path, p.credentials)?,
                Err(err) => return Err(err),
            }
        } else if truncate {
            fs::create_file_as(path, p.credentials)?
        } else {
            fs::open_with_rights_as(path, p.credentials, read, write)?
        };
        if append {
            let _ = fs::lseek(vfs_fd, 0, 2);
        }
        Ok(p.push_fd(vfs_fd))
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_create(path: &str) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let vfs_fd = fs::create_file_as(path, p.credentials)?;
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

pub fn user_dup(user_fd: usize) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let vfs_fd = p.lookup_fd(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let dup = fs::duplicate_fd(vfs_fd)?;
        Ok(p.push_fd(dup))
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
        let vfs_fd = p.lookup_fd(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let dup = fs::duplicate_fd(vfs_fd)?;
        let old = p.replace_fd(target_fd, dup);
        if let Some(old) = old {
            fs::close(old)?;
        }
        Ok(target_fd)
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_mkdir(path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| fs::mkdir_as(path, p.credentials)).unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_unlink(path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| fs::unlink_as(path, p.credentials)).unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_chmod(path: &str, mode: u16) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| fs::chmod_as(path, mode, p.credentials))
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
        if fault_addr >= process.address_space.stack_bottom
            && fault_addr < process.address_space.stack_top
        {
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
    let woken: Vec<Pid> = with_table(|table| {
        let mut woken = Vec::new();
        for process in &mut table.processes {
            if matches!(process.state, ProcessState::Blocked(BlockReason::WaitIo)) {
                process.state = ProcessState::Ready;
                woken.push(process.pid);
            }
        }
        woken
    });
    for pid in woken {
        crate::sched::wake_blocked(pid);
    }
}

pub fn wake_io_waiters_for(pid: Pid) {
    let woke = with_table(|table| {
        if let Some(p) = table.get_mut(pid) {
            if matches!(p.state, ProcessState::Blocked(BlockReason::WaitIo)) {
                p.state = ProcessState::Ready;
                return true;
            }
        }
        false
    });
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
        fd_count: table.processes.iter().map(|p| p.fd_count).sum(),
        cwd_count: table.processes.iter().filter(|p| !p.cwd.is_empty()).count(),
        fd_path_checksum: table
            .processes
            .iter()
            .map(|p| p.fd_count + p.pid as usize)
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

pub fn set_current_resuid(ruid: Option<u32>, euid: Option<u32>, suid: Option<u32>) -> Result<(), ()> {
    with_current(|p| {
        let old = p.credentials;
        let allowed = |uid: u32| {
            old.is_superuser() || uid == old.uid || uid == old.euid || uid == old.suid
        };
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

pub fn user_cwd() -> Option<String> {
    with_current_read(|p| p.cwd.clone())
}

pub fn user_chdir(path: &str) -> Result<(), ()> {
    with_current(|p| {
        if path.is_empty() {
            return Err(());
        }
        let resolved = if path.starts_with('/') {
            String::from(path)
        } else {
            let mut combined = p.cwd.clone();
            if !combined.ends_with('/') {
                combined.push('/');
            }
            combined.push_str(path);
            combined
        };
        p.cwd = resolved;
        Ok(())
    })
    .unwrap_or(Err(()))
}

pub fn current_heap_break() -> usize {
    with_current_read(|p| p.address_space.heap_break).unwrap_or(0)
}

/// Wait for any child matching `child` (0 = any). Returns Some((pid, status)) when
/// a zombie child is reaped, None otherwise.
pub fn wait_any(parent: Pid, child: Pid) -> Option<(Pid, i32)> {
    with_table(|table| {
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
            return Some((zombie_pid, status));
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

pub fn get_process_info(pid: Pid) -> Option<(String, ProcessState, Option<Pid>, Option<i32>)> {
    with_table(|table| {
        let p = table.get(pid)?;
        Some((p.name.clone(), p.state, p.parent, p.exit_status))
    })
}
