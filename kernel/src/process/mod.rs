use alloc::{string::String, vec::Vec};
use core::{cmp, ptr, slice};

use crate::{
    arch::x86_64::fpu,
    fs,
    memory::{
        address_space::{AddressSpace, USER_MMAP_START, UserAccess, UserProtection},
        frame_allocator::{self, FRAME_SIZE},
        paging,
    },
    security::{Access, Credentials, FileMetadata},
    sync::spinlock::SpinLock,
    task::scheduler,
    userspace::elf,
};

static PROCESS_TABLE: SpinLock<Option<ProcessTable>> = SpinLock::new(None);
static CURRENT_PID: SpinLock<Option<Pid>> = SpinLock::new(None);

pub type Pid = u64;

const INIT_PID: Pid = 1;
pub const MAX_FDS: usize = 256;
pub const MAX_USER_ARGS: usize = 64;
pub const MAX_USER_ENVS: usize = 64;
pub const FD_CLOEXEC: u32 = 1;
const MAX_WAKE_PIDS: usize = MAX_FDS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked(BlockReason),
    Stopped(u8),
    Zombie(ExitReason),
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
    woken: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimedWaitError {
    NoCurrentProcess,
    DeadlineOverflow,
}

#[derive(Clone, Copy)]
struct SharedMapping {
    addr: usize,
    len: usize,
    file_offset: usize,
    vfs_fd: usize,
    writable: bool,
}

struct SharedMappingDiscard {
    addr: usize,
    range_end: usize,
    fragments: Vec<SharedMapping>,
    replacement: Vec<SharedMapping>,
}

impl SharedMappingDiscard {
    fn close_fragments(&self) {
        for fragment in &self.fragments {
            let _ = fs::close(fragment.vfs_fd);
        }
    }
}

struct WakeList {
    pids: [Pid; MAX_WAKE_PIDS],
    len: usize,
    overflow: bool,
}

impl WakeList {
    fn new() -> Self {
        Self {
            pids: [0; MAX_WAKE_PIDS],
            len: 0,
            overflow: false,
        }
    }

    fn push_unique(&mut self, pid: Pid) {
        if self.pids[..self.len].contains(&pid) {
            return;
        }
        if self.len == self.pids.len() {
            self.overflow = true;
            return;
        }
        self.pids[self.len] = pid;
        self.len += 1;
    }

    fn as_slice(&self) -> &[Pid] {
        &self.pids[..self.len]
    }

    fn extend(&mut self, other: WakeList) {
        for pid in other.as_slice() {
            self.push_unique(*pid);
        }
        if other.overflow {
            self.overflow = true;
        }
    }
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
    shared_mappings: Vec<SharedMapping>,
    timed_wait: Option<TimedWait>,
    next_fd: usize,
    pub entry: u64,
    pub stack_top: usize,
    pub argc: usize,
    pub argv_ptr: usize,
    pub envp_ptr: usize,
    exit_status: Option<i32>,
    stop_reported: bool,
    continued_report_pending: bool,
    waiters: Vec<Pid>,
    is_user: bool,
    saved_syscall: Option<SavedSyscallFrame>,
    interrupted_syscall: bool,
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
    Signaled(u8),
    Stopped(u8),
    Continued,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitSelector {
    Any,
    Pid(Pid),
    ProcessGroup(Pid),
}

impl WaitSelector {
    fn matches(self, parent: Pid, process: &Process) -> bool {
        if process.parent != Some(parent) {
            return false;
        }
        match self {
            Self::Any => true,
            Self::Pid(pid) => process.pid == pid,
            Self::ProcessGroup(pgrp) => process.pgrp == pgrp,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetPgidError {
    Invalid,
    NoSuchProcess,
    PermissionDenied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitReason {
    Exited(i32),
    Signaled(u8),
}

impl ExitReason {
    pub const fn legacy_status(self) -> i32 {
        match self {
            Self::Exited(status) => status,
            Self::Signaled(signal) => 128 + signal as i32,
        }
    }
}

fn signal_from_legacy_status(status: i32) -> Option<u8> {
    if status < 128 {
        return None;
    }
    let signal = status - 128;
    if (1..64).contains(&signal) {
        Some(signal as u8)
    } else {
        None
    }
}

fn is_uncatchable_signal(signal: u8) -> bool {
    signal == crate::signal::Signal::Kill.number() || signal == crate::signal::Signal::Stop.number()
}

fn stop_signal_from_status(status: i32) -> Option<u8> {
    let signal = signal_from_legacy_status(status).and_then(crate::signal::Signal::from_number)?;
    if signal.has_stop_default() {
        Some(signal.number())
    } else {
        None
    }
}

fn is_ignored_signal(process: &Process, status: i32) -> bool {
    let Some(signal) = signal_from_legacy_status(status) else {
        return false;
    };
    if is_uncatchable_signal(signal) {
        return false;
    }
    let handler = process
        .signal_handlers
        .get(signal as usize)
        .copied()
        .unwrap_or(crate::signal::DEFAULT_HANDLER);
    if handler == crate::signal::DEFAULT_HANDLER {
        if let Some(signal) = crate::signal::Signal::from_number(signal) {
            if signal.has_ignore_default() {
                return true;
            }
        }
    }
    handler == crate::signal::IGNORE_HANDLER
}

fn clear_pending_signal(process: &mut Process, signal: usize) {
    if signal < 64 {
        process.pending_signals &= !(1u64 << signal);
    }
    if signal < 32 && process.pending_signal_status == Some(128 + signal as i32) {
        process.pending_signal_status = None;
    }
}

fn discard_pending_signal_interactions(process: &mut Process, status: i32) {
    let Some(signal_number) = signal_from_legacy_status(status) else {
        return;
    };
    let Some(signal) = crate::signal::Signal::from_number(signal_number) else {
        return;
    };

    if signal == crate::signal::Signal::Cont {
        clear_pending_signal(process, crate::signal::Signal::Stop.number() as usize);
        clear_pending_signal(process, crate::signal::Signal::Tstp.number() as usize);
        clear_pending_signal(process, crate::signal::Signal::Ttin.number() as usize);
        clear_pending_signal(process, crate::signal::Signal::Ttou.number() as usize);
    } else if signal.has_stop_default() {
        clear_pending_signal(process, crate::signal::Signal::Cont.number() as usize);
    }
}

fn next_deliverable_pending_signal(process: &Process) -> Option<u8> {
    let mut pending = process.pending_signals;
    while pending != 0 {
        let signal = pending.trailing_zeros() as u8;
        let bit = 1u64 << signal;
        pending &= !bit;
        if signal == 0 {
            continue;
        }
        if !is_uncatchable_signal(signal) && process.signal_mask & bit != 0 {
            continue;
        }
        return Some(signal);
    }
    None
}

fn should_queue_signal(process: &Process, status: i32, current: Option<Pid>) -> bool {
    if current == Some(process.pid) {
        return true;
    }
    let Some(signal) = signal_from_legacy_status(status) else {
        return false;
    };
    if is_uncatchable_signal(signal) {
        return false;
    }
    let masked = signal < 64 && process.signal_mask & (1u64 << signal) != 0;
    let handler = process
        .signal_handlers
        .get(signal as usize)
        .copied()
        .unwrap_or(crate::signal::DEFAULT_HANDLER);
    masked || handler != crate::signal::DEFAULT_HANDLER
}

fn queue_signal(process: &mut Process, status: i32) {
    process.pending_signal_status = Some(status);
    if status >= 128 {
        let signal = (status - 128) as u64;
        if signal < 64 {
            process.pending_signals |= 1 << signal;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecError {
    NotFound,
    PermissionDenied,
    InvalidImage,
    TooManyArguments,
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForkError {
    NoSuchProcess,
    OutOfMemory,
    InvalidProcessState,
}

impl From<elf::ElfError> for ExecError {
    fn from(err: elf::ElfError) -> Self {
        match err {
            elf::ElfError::TooSmall | elf::ElfError::BadMagic | elf::ElfError::OutOfBounds => {
                Self::InvalidImage
            }
            elf::ElfError::Unsupported => Self::InvalidImage,
        }
    }
}

struct StackSetup {
    stack_top: usize,
    argc: usize,
    argv_ptr: usize,
    envp_ptr: usize,
}

struct PreparedExec {
    address_space: AddressSpace,
    entry: u64,
    stack: StackSetup,
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
            shared_mappings: Vec::new(),
            timed_wait: None,
            next_fd: 3,
            entry: 0,
            stack_top: paging::USER_STACK_TOP,
            argc: 0,
            argv_ptr: 0,
            envp_ptr: 0,
            exit_status: None,
            stop_reported: false,
            continued_report_pending: false,
            waiters: Vec::new(),
            is_user: true,
            saved_syscall: None,
            interrupted_syscall: false,
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
        let _ = self.set_fd_with_flags(user_fd, vfs_fd, 0, 0);
    }

    fn set_fd_with_flags(
        &mut self,
        user_fd: usize,
        vfs_fd: usize,
        status_flags: u32,
        fd_flags: u32,
    ) -> Result<(), FdTableError> {
        if user_fd >= self.fd_limit() {
            return Err(FdTableError::TooManyOpenFiles);
        }
        if let Some(entry) = self.fds.iter_mut().find(|e| e.user_fd == user_fd) {
            entry.vfs_fd = vfs_fd;
            entry.status_flags = status_flags;
            entry.fd_flags = fd_flags;
            return Ok(());
        }
        if self.fds.len() >= MAX_FDS {
            return Err(FdTableError::TooManyOpenFiles);
        }
        self.fds
            .try_reserve_exact(1)
            .map_err(|_| FdTableError::OutOfMemory)?;
        self.fds.push(FdEntry {
            user_fd,
            vfs_fd,
            status_flags,
            fd_flags,
        });
        if user_fd >= self.next_fd {
            self.next_fd = user_fd + 1;
        }
        Ok(())
    }

    fn lookup_fd(&self, user_fd: usize) -> Option<usize> {
        self.fds
            .iter()
            .find(|e| e.user_fd == user_fd)
            .map(|e| e.vfs_fd)
    }

    fn push_fd(&mut self, vfs_fd: usize) -> Result<usize, FdTableError> {
        self.push_fd_with_flags(vfs_fd, 0)
    }

    fn push_fd_with_flags(
        &mut self,
        vfs_fd: usize,
        status_flags: u32,
    ) -> Result<usize, FdTableError> {
        self.push_fd_with_all_flags(vfs_fd, status_flags, 0)
    }

    fn push_fd_with_all_flags(
        &mut self,
        vfs_fd: usize,
        status_flags: u32,
        fd_flags: u32,
    ) -> Result<usize, FdTableError> {
        let user_fd = self
            .lowest_available_fd()
            .ok_or(FdTableError::TooManyOpenFiles)?;
        self.set_fd_with_flags(user_fd, vfs_fd, status_flags, fd_flags)?;
        Ok(user_fd)
    }

    fn lowest_available_fd(&self) -> Option<usize> {
        let limit = self.fd_limit();
        let mut candidate = 0;
        while candidate < limit {
            if self.fds.iter().all(|entry| entry.user_fd != candidate) {
                return Some(candidate);
            }
            candidate += 1;
        }
        None
    }

    fn fd_limit(&self) -> usize {
        (self.rlimit_nofile_cur as usize).min(MAX_FDS)
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
    ) -> Result<Option<usize>, FdTableError> {
        if user_fd >= self.fd_limit() {
            return Err(FdTableError::TooManyOpenFiles);
        }
        if let Some(entry) = self.fds.iter_mut().find(|e| e.user_fd == user_fd) {
            let old = entry.vfs_fd;
            entry.vfs_fd = vfs_fd;
            entry.status_flags = status_flags;
            entry.fd_flags = fd_flags;
            return Ok(Some(old));
        }
        self.set_fd_with_flags(user_fd, vfs_fd, status_flags, fd_flags)?;
        Ok(None)
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

    fn close_on_exec_fds(&mut self) {
        let mut index = 0;
        while index < self.fds.len() {
            if self.fds[index].fd_flags & FD_CLOEXEC != 0 {
                let entry = self.fds.swap_remove(index);
                let _ = fs::close(entry.vfs_fd);
            } else {
                index += 1;
            }
        }
    }

    fn close_on_exec_socket_handles(&mut self) {
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
                self.socket_handles.swap_remove(index);
                let _ = crate::net::socket::with_sockets(|table| table.close(handle));
            } else {
                index += 1;
            }
        }
    }

    fn add_socket_handle(&mut self, handle: usize) -> Result<(), SocketInstallError> {
        if !self.socket_handles.contains(&handle) {
            self.socket_handles
                .try_reserve_exact(1)
                .map_err(|_| SocketInstallError::OutOfMemory)?;
            self.socket_handles.push(handle);
        }
        Ok(())
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

    fn timed_wait_deadline(
        &mut self,
        key: u64,
        timeout_ms: u64,
        now_ms: u64,
    ) -> Result<u64, TimedWaitError> {
        if let Some(wait) = self.timed_wait {
            if wait.key == key {
                return Ok(wait.deadline_ms);
            }
        }
        let deadline_ms = now_ms
            .checked_add(timeout_ms)
            .ok_or(TimedWaitError::DeadlineOverflow)?;
        self.timed_wait = Some(TimedWait {
            key,
            deadline_ms,
            woken: false,
        });
        Ok(deadline_ms)
    }

    fn clear_timed_wait(&mut self, key: u64) {
        if matches!(self.timed_wait, Some(wait) if wait.key == key) {
            self.timed_wait = None;
        }
    }

    fn take_timed_wait_wake(&mut self, key: u64) -> bool {
        if matches!(self.timed_wait, Some(wait) if wait.key == key && wait.woken) {
            self.timed_wait = None;
            return true;
        }
        false
    }

    fn allows_user_read(&self, addr: usize, len: usize) -> bool {
        self.address_space.allows_user(addr, len, UserAccess::Read)
    }

    fn allows_user_execute(&self, addr: usize, len: usize) -> bool {
        self.address_space
            .allows_user(addr, len, UserAccess::Execute)
    }

    fn prepare_user_write(&mut self, addr: usize, len: usize) -> bool {
        if !self.address_space.allows_user(addr, len, UserAccess::Write) {
            return false;
        }
        self.address_space.ensure_user_writable(addr, len).is_ok()
    }

    fn prepare_exec(
        &self,
        data: &[u8],
        args: &[&str],
        env: &[&str],
    ) -> Result<PreparedExec, ExecError> {
        let mut checked_segments = 0;
        let checked_entry = elf::for_each_load_segment(data, |_| {
            checked_segments += 1;
        })
        .map_err(ExecError::from)?;
        if checked_segments == 0 {
            return Err(ExecError::InvalidImage);
        }

        let mut new_space = AddressSpace::new_kernel_clone().map_err(|_| ExecError::OutOfMemory)?;
        new_space.activate();
        let mut segments = 0;
        let mut load_error = None;
        let map_result = elf::for_each_load_segment(data, |segment| {
            if load_error.is_some() {
                return;
            }
            match map_elf_segment(&mut new_space, segment) {
                Ok(()) => segments += 1,
                Err(err) => load_error = Some(err),
            }
        });
        if let Err(err) = map_result {
            self.address_space.activate();
            new_space.destroy();
            return Err(ExecError::from(err));
        }
        if let Some(err) = load_error {
            self.address_space.activate();
            new_space.destroy();
            return Err(err);
        }
        if segments != checked_segments || segments == 0 {
            self.address_space.activate();
            new_space.destroy();
            return Err(ExecError::OutOfMemory);
        }
        let Ok(entry) = usize::try_from(checked_entry) else {
            self.address_space.activate();
            new_space.destroy();
            return Err(ExecError::InvalidImage);
        };
        if !new_space.allows_user(entry, 1, UserAccess::Execute) {
            self.address_space.activate();
            new_space.destroy();
            return Err(ExecError::InvalidImage);
        }

        let stack = match Self::setup_stack_in_space(&mut new_space, args, env) {
            Ok(stack) => stack,
            Err(err) => {
                self.address_space.activate();
                new_space.destroy();
                return Err(err);
            }
        };

        self.address_space.activate();
        Ok(PreparedExec {
            address_space: new_space,
            entry: checked_entry,
            stack,
        })
    }

    fn commit_exec(&mut self, name: String, prepared: PreparedExec) {
        let PreparedExec {
            address_space,
            entry,
            stack,
        } = prepared;

        self.flush_and_close_shared_mappings();
        let old = core::mem::replace(&mut self.address_space, address_space);
        self.address_space.activate();
        old.destroy();
        self.timed_wait = None;
        self.fpu_state = fpu::initial_state();
        self.name = name;
        self.entry = entry;
        self.stack_top = stack.stack_top;
        self.argc = stack.argc;
        self.argv_ptr = stack.argv_ptr;
        self.envp_ptr = stack.envp_ptr;
        self.reset_caught_signal_handlers_for_exec();
    }

    fn reset_caught_signal_handlers_for_exec(&mut self) {
        for handler in &mut self.signal_handlers {
            if *handler != crate::signal::IGNORE_HANDLER {
                *handler = crate::signal::DEFAULT_HANDLER;
            }
        }
    }

    fn setup_stack_in_space(
        address_space: &mut AddressSpace,
        args: &[&str],
        env: &[&str],
    ) -> Result<StackSetup, ExecError> {
        if args.len() > MAX_USER_ARGS {
            return Err(ExecError::TooManyArguments);
        }
        if env.len() > MAX_USER_ENVS {
            return Err(ExecError::TooManyArguments);
        }
        let stack_top = paging::USER_STACK_TOP;
        let stack_bottom = stack_top - FRAME_SIZE;
        address_space
            .map_zero_page(stack_bottom)
            .map_err(|_| ExecError::OutOfMemory)?;
        address_space.stack_bottom = stack_bottom;
        address_space.stack_top = stack_top;

        let mut sp = stack_top;
        let mut arg_ptrs = [0usize; MAX_USER_ARGS];
        for (index, arg) in args.iter().enumerate() {
            arg_ptrs[index] = Self::push_stack_string(address_space, &mut sp, arg)?;
        }
        let mut env_ptrs = [0usize; MAX_USER_ENVS];
        for (index, entry) in env.iter().enumerate() {
            env_ptrs[index] = Self::push_stack_string(address_space, &mut sp, entry)?;
        }

        sp &= !0xf;
        let env_bytes = (env.len() + 1) * 8;
        sp = sp.checked_sub(env_bytes).ok_or(ExecError::OutOfMemory)?;
        Self::ensure_stack_mapping(address_space, sp, env_bytes)?;
        let envp_ptr = sp;
        unsafe {
            for index in 0..env.len() {
                *(envp_ptr as *mut u64).add(index) = env_ptrs[index] as u64;
            }
            *(envp_ptr as *mut u64).add(env.len()) = 0;
        }

        let argv_bytes = (args.len() + 1) * 8;
        sp = sp.checked_sub(argv_bytes).ok_or(ExecError::OutOfMemory)?;
        Self::ensure_stack_mapping(address_space, sp, argv_bytes)?;
        let argv_ptr = sp;
        unsafe {
            for index in 0..args.len() {
                *(argv_ptr as *mut u64).add(index) = arg_ptrs[index] as u64;
            }
            *(argv_ptr as *mut u64).add(args.len()) = 0;
        }

        sp = (sp & !0xf).checked_sub(8).ok_or(ExecError::OutOfMemory)?;
        Self::ensure_stack_mapping(address_space, sp, 8)?;
        Ok(StackSetup {
            stack_top: sp,
            argc: args.len(),
            argv_ptr,
            envp_ptr,
        })
    }

    fn push_stack_string(
        address_space: &mut AddressSpace,
        sp: &mut usize,
        text: &str,
    ) -> Result<usize, ExecError> {
        let bytes = text.as_bytes();
        *sp = sp
            .checked_sub(bytes.len().checked_add(1).ok_or(ExecError::OutOfMemory)?)
            .ok_or(ExecError::OutOfMemory)?;
        *sp &= !0xf;
        Self::ensure_stack_mapping(address_space, *sp, bytes.len() + 1)?;
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), *sp as *mut u8, bytes.len());
            *(*sp as *mut u8).add(bytes.len()) = 0;
        }
        Ok(*sp)
    }

    fn ensure_stack_mapping(
        address_space: &mut AddressSpace,
        addr: usize,
        len: usize,
    ) -> Result<(), ExecError> {
        let end = addr.checked_add(len.max(1)).ok_or(ExecError::OutOfMemory)?;
        let mut page = paging::align_down(addr, FRAME_SIZE);
        let page_end = paging::checked_align_up(end, FRAME_SIZE).ok_or(ExecError::OutOfMemory)?;
        while page < page_end {
            let page_limit = page.checked_add(FRAME_SIZE).ok_or(ExecError::OutOfMemory)?;
            if page <= paging::USER_STACK_GUARD || page_limit > paging::USER_STACK_TOP {
                return Err(ExecError::OutOfMemory);
            }
            if !address_space.is_user_mapped(page) {
                address_space
                    .map_zero_page(page)
                    .map_err(|_| ExecError::OutOfMemory)?;
            }
            if page < address_space.stack_bottom {
                address_space.stack_bottom = page;
            }
            page += FRAME_SIZE;
        }
        Ok(())
    }

    fn flush_shared_mappings_range(
        &mut self,
        addr: usize,
        len: usize,
    ) -> Result<(), fs::vfs::VfsError> {
        if len == 0 || current_pid() != Some(self.pid) {
            return Ok(());
        }
        let Some(range_end) = addr.checked_add(len) else {
            return Err(fs::vfs::VfsError::BadFd);
        };
        for mapping in &self.shared_mappings {
            self.flush_shared_mapping_range(mapping, addr, range_end)?;
        }
        Ok(())
    }

    fn flush_shared_mapping_range(
        &self,
        mapping: &SharedMapping,
        addr: usize,
        range_end: usize,
    ) -> Result<(), fs::vfs::VfsError> {
        if !mapping.writable {
            return Ok(());
        }
        let mapping_end = mapping.addr.saturating_add(mapping.len);
        let start = cmp::max(addr, mapping.addr);
        let end = cmp::min(range_end, mapping_end);
        if start >= end {
            return Ok(());
        }
        if !self.allows_user_read(start, end - start) {
            return Err(fs::vfs::VfsError::BadFd);
        }
        let file_offset = mapping.file_offset + (start - mapping.addr);
        fs::lseek(mapping.vfs_fd, file_offset as isize, 0)?;
        let bytes = unsafe { slice::from_raw_parts(start as *const u8, end - start) };
        let mut written = 0usize;
        while written < bytes.len() {
            let count = fs::write(mapping.vfs_fd, &bytes[written..])?;
            if count == 0 {
                return Err(fs::vfs::VfsError::BadFd);
            }
            written += count;
        }
        Ok(())
    }

    fn prepare_shared_mappings_discard(
        &self,
        addr: usize,
        len: usize,
    ) -> Result<SharedMappingDiscard, SharedMappingError> {
        let range_end = addr
            .checked_add(len)
            .ok_or(SharedMappingError::Vfs(fs::vfs::VfsError::BadFd))?;
        let mut fragment_count = 0usize;
        let mut replacement_count = 0usize;
        for mapping in &self.shared_mappings {
            let mapping_end = mapping.addr.saturating_add(mapping.len);
            let start = cmp::max(addr, mapping.addr);
            let end = cmp::min(range_end, mapping_end);
            if start >= end {
                replacement_count = replacement_count
                    .checked_add(1)
                    .ok_or(SharedMappingError::OutOfMemory)?;
                continue;
            }
            if mapping.addr < start {
                fragment_count = fragment_count
                    .checked_add(1)
                    .ok_or(SharedMappingError::OutOfMemory)?;
                replacement_count = replacement_count
                    .checked_add(1)
                    .ok_or(SharedMappingError::OutOfMemory)?;
            }
            if end < mapping_end {
                fragment_count = fragment_count
                    .checked_add(1)
                    .ok_or(SharedMappingError::OutOfMemory)?;
                replacement_count = replacement_count
                    .checked_add(1)
                    .ok_or(SharedMappingError::OutOfMemory)?;
            }
        }

        let mut discard = SharedMappingDiscard {
            addr,
            range_end,
            fragments: Vec::new(),
            replacement: Vec::new(),
        };
        discard
            .fragments
            .try_reserve_exact(fragment_count)
            .map_err(|_| SharedMappingError::OutOfMemory)?;
        discard
            .replacement
            .try_reserve_exact(replacement_count)
            .map_err(|_| SharedMappingError::OutOfMemory)?;

        for mapping in &self.shared_mappings {
            let mapping_end = mapping.addr.saturating_add(mapping.len);
            let start = cmp::max(addr, mapping.addr);
            let end = cmp::min(range_end, mapping_end);
            if start >= end {
                continue;
            }
            if mapping.addr < start {
                match fs::duplicate_fd(mapping.vfs_fd) {
                    Ok(dup) => discard.fragments.push(SharedMapping {
                        addr: mapping.addr,
                        len: start - mapping.addr,
                        file_offset: mapping.file_offset,
                        vfs_fd: dup,
                        writable: mapping.writable,
                    }),
                    Err(err) => {
                        discard.close_fragments();
                        return Err(SharedMappingError::Vfs(err));
                    }
                }
            }
            if end < mapping_end {
                match fs::duplicate_fd(mapping.vfs_fd) {
                    Ok(dup) => discard.fragments.push(SharedMapping {
                        addr: end,
                        len: mapping_end - end,
                        file_offset: mapping.file_offset + (end - mapping.addr),
                        vfs_fd: dup,
                        writable: mapping.writable,
                    }),
                    Err(err) => {
                        discard.close_fragments();
                        return Err(SharedMappingError::Vfs(err));
                    }
                }
            }
        }
        Ok(discard)
    }

    fn commit_shared_mappings_discard(&mut self, mut discard: SharedMappingDiscard) {
        let mut fragment_index = 0usize;
        for mapping in &self.shared_mappings {
            let mapping_end = mapping.addr.saturating_add(mapping.len);
            let start = cmp::max(discard.addr, mapping.addr);
            let end = cmp::min(discard.range_end, mapping_end);
            if start >= end {
                discard.replacement.push(*mapping);
                continue;
            }
            if mapping.addr < start {
                discard.replacement.push(discard.fragments[fragment_index]);
                fragment_index += 1;
            }
            if end < mapping_end {
                discard.replacement.push(discard.fragments[fragment_index]);
                fragment_index += 1;
            }
            let _ = fs::close(mapping.vfs_fd);
        }
        self.shared_mappings = discard.replacement;
    }

    fn register_shared_mapping(
        &mut self,
        addr: usize,
        len: usize,
        vfs_fd: usize,
        file_offset: usize,
        writable: bool,
    ) -> Result<(), SharedMappingError> {
        let _range_end = addr
            .checked_add(len)
            .ok_or(SharedMappingError::InvalidRange)?;
        let file_range_end = file_offset
            .checked_add(len)
            .ok_or(SharedMappingError::InvalidRange)?;
        if len == 0 || file_range_end > isize::MAX as usize {
            return Err(SharedMappingError::InvalidRange);
        }
        self.shared_mappings
            .try_reserve_exact(1)
            .map_err(|_| SharedMappingError::OutOfMemory)?;
        let dup = fs::duplicate_fd(vfs_fd).map_err(SharedMappingError::Vfs)?;
        self.shared_mappings.push(SharedMapping {
            addr,
            len,
            file_offset,
            vfs_fd: dup,
            writable,
        });
        Ok(())
    }

    fn flush_and_close_shared_mappings(&mut self) {
        let mappings = core::mem::take(&mut self.shared_mappings);
        if current_pid() == Some(self.pid) {
            for mapping in &mappings {
                if let Some(range_end) = mapping.addr.checked_add(mapping.len) {
                    let _ = self.flush_shared_mapping_range(mapping, mapping.addr, range_end);
                }
            }
        }
        for mapping in mappings {
            let _ = fs::close(mapping.vfs_fd);
        }
    }

    fn destroy(mut self) {
        self.flush_and_close_shared_mappings();
        for entry in &self.fds {
            let _ = fs::close(entry.vfs_fd);
        }
        self.fds.clear();
        self.close_socket_handles();
        self.address_space.destroy();
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

    fn fork(&mut self, parent: Pid) -> Result<Pid, ForkError> {
        self.processes
            .try_reserve_exact(1)
            .map_err(|_| ForkError::OutOfMemory)?;
        let parent_proc = self
            .get(parent)
            .ok_or(ForkError::NoSuchProcess)?
            .clone_process()?;
        let pid = self.next_pid;
        let mut child = parent_proc;
        child.pid = pid;
        child.parent = Some(parent);
        child.state = ProcessState::Ready;
        child.waiters.clear();
        child.exit_status = None;
        child.stop_reported = false;
        child.continued_report_pending = false;
        child.pending_signals = 0;
        child.pending_signal_status = None;
        self.processes.push(child);
        self.next_pid += 1;
        crate::sched::on_fork(pid);
        Ok(pid)
    }

    fn exec(&mut self, pid: Pid, path: &str, args: &[&str]) -> bool {
        let Some(data) = fs::read_file(path) else {
            return false;
        };
        let index = match self.processes.iter().position(|p| p.pid == pid) {
            Some(i) => i,
            None => return false,
        };
        let prepared = match self.processes[index].prepare_exec(&data, args, &[]) {
            Ok(prepared) => prepared,
            Err(_) => return false,
        };
        let name = match try_exec_string(path) {
            Ok(name) => name,
            Err(_) => return false,
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
        process.commit_exec(name, prepared);
        process.state = ProcessState::Ready;
        clear_current();
        true
    }

    fn exit(&mut self, pid: Pid, status: i32) -> WakeList {
        self.exit_with_reason(pid, ExitReason::Exited(status))
    }

    fn exit_signaled(&mut self, pid: Pid, signal: u8) -> WakeList {
        self.exit_with_reason(pid, ExitReason::Signaled(signal))
    }

    fn exit_with_reason(&mut self, pid: Pid, reason: ExitReason) -> WakeList {
        let was_current = current_pid() == Some(pid);
        let mut wake = WakeList::new();
        let parent_pid = self.get(pid).and_then(|p| p.parent);
        let waiters = {
            let process = match self.get_mut(pid) {
                Some(p) => p,
                None => return wake,
            };
            process.state = ProcessState::Zombie(reason);
            process.exit_status = Some(reason.legacy_status());
            if was_current {
                process.address_space.activate();
            }
            process.flush_and_close_shared_mappings();
            for entry in &process.fds {
                let _ = fs::close(entry.vfs_fd);
            }
            process.fds.clear();
            process.close_socket_handles();
            process.address_space.clear_user_pages();
            if was_current {
                clear_current();
            }
            core::mem::take(&mut process.waiters)
        };
        self.reparent_children_to_init(pid, &mut wake);
        for waiter in waiters {
            self.wake_waiter_for_child(waiter, pid, &mut wake);
        }
        if let Some(parent) = parent_pid {
            self.queue_sigchld(parent);
            self.wake_waiter_for_child(parent, pid, &mut wake);
        }
        wake
    }

    fn queue_sigchld(&mut self, parent: Pid) {
        let status = crate::signal::Signal::Child.default_status();
        let current = current_pid();
        let Some(process) = self.get_mut(parent) else {
            return;
        };
        if is_ignored_signal(process, status) {
            return;
        }
        if should_queue_signal(process, status, current) {
            queue_signal(process, status);
        }
    }

    fn wake_waiters_for(&mut self, pid: Pid) -> WakeList {
        let mut wake = WakeList::new();
        let waiters = self
            .get_mut(pid)
            .map(|process| core::mem::take(&mut process.waiters))
            .unwrap_or_default();
        for waiter in waiters {
            self.wake_waiter_for_child(waiter, pid, &mut wake);
        }
        if let Some(parent) = self.get(pid).and_then(|p| p.parent) {
            self.wake_waiter_for_child(parent, pid, &mut wake);
        }
        wake
    }

    fn reparent_children_to_init(&mut self, old_parent: Pid, wake: &mut WakeList) {
        if old_parent == INIT_PID {
            return;
        }
        let init_exists = self.get(INIT_PID).is_some();
        let new_parent = if init_exists { Some(INIT_PID) } else { None };
        let mut orphan_candidates = Vec::new();
        let mut index = 0;
        while index < self.processes.len() {
            let adopted_waitable = {
                let process = &mut self.processes[index];
                if process.parent != Some(old_parent) {
                    None
                } else {
                    if !orphan_candidates.contains(&process.pgrp)
                        && orphan_candidates.try_reserve_exact(1).is_ok()
                    {
                        orphan_candidates.push(process.pgrp);
                    }
                    process.parent = new_parent;
                    if init_exists
                        && matches!(
                            process.state,
                            ProcessState::Zombie(_) | ProcessState::Stopped(_)
                        )
                    {
                        Some(process.pid)
                    } else {
                        None
                    }
                }
            };
            if let Some(child) = adopted_waitable {
                self.wake_waiter_for_child(INIT_PID, child, wake);
            }
            index += 1;
        }
        self.notify_orphaned_stopped_groups(&orphan_candidates, wake);
    }

    fn notify_orphaned_stopped_groups(&mut self, candidates: &[Pid], wake: &mut WakeList) {
        for pgrp in candidates {
            if self.process_group_is_orphaned(*pgrp) && self.process_group_has_stopped_member(*pgrp)
            {
                self.signal_process_group(*pgrp, crate::signal::Signal::Hup, wake);
                self.signal_process_group(*pgrp, crate::signal::Signal::Cont, wake);
            }
        }
    }

    fn process_group_is_orphaned(&self, pgrp: Pid) -> bool {
        for process in self.processes.iter().filter(|process| {
            process.pgrp == pgrp && !matches!(process.state, ProcessState::Zombie(_))
        }) {
            let Some(parent) = process.parent.and_then(|parent| self.get(parent)) else {
                continue;
            };
            // Init adopts the child after parent death; it is not a job-control
            // parent that keeps the process group non-orphaned.
            if parent.pid == INIT_PID {
                continue;
            }
            if parent.sid == process.sid
                && parent.pgrp != process.pgrp
                && !matches!(parent.state, ProcessState::Zombie(_))
            {
                return false;
            }
        }
        true
    }

    fn process_group_has_stopped_member(&self, pgrp: Pid) -> bool {
        self.processes.iter().any(|process| {
            process.pgrp == pgrp && matches!(process.state, ProcessState::Stopped(_))
        })
    }

    fn process_group_members(&self, pgrp: Pid) -> Vec<Pid> {
        let mut members = Vec::new();
        for process in self.processes.iter().filter(|process| {
            process.pgrp == pgrp && !matches!(process.state, ProcessState::Zombie(_))
        }) {
            if members.try_reserve_exact(1).is_err() {
                break;
            }
            members.push(process.pid);
        }
        members
    }

    fn signal_process_group(
        &mut self,
        pgrp: Pid,
        signal: crate::signal::Signal,
        wake: &mut WakeList,
    ) {
        let members = self.process_group_members(pgrp);
        for pid in members {
            if signal == crate::signal::Signal::Cont {
                if let Some(continued_wake) = self.continue_process(pid) {
                    wake.extend(continued_wake);
                }
            }
            if let Some(signal_wake) = self.signal(pid, signal.default_status(), current_pid()) {
                wake.extend(signal_wake);
            }
        }
    }

    fn wake_waiter_for_child(&mut self, waiter: Pid, child: Pid, wake: &mut WakeList) {
        let Some(process) = self.get_mut(waiter) else {
            return;
        };
        let should_wake = matches!(
            process.state,
            ProcessState::Blocked(BlockReason::WaitChild(wait_child))
                if wait_child == child || wait_child == 0
        );
        if !should_wake {
            return;
        }
        process.state = ProcessState::Ready;
        wake.push_unique(waiter);
    }

    fn stop(&mut self, pid: Pid, signal: u8) -> Option<WakeList> {
        let parent = {
            let process = self.get_mut(pid)?;
            if matches!(process.state, ProcessState::Zombie(_)) {
                return Some(WakeList::new());
            }
            process.state = ProcessState::Stopped(signal);
            process.exit_status = None;
            process.stop_reported = false;
            process.continued_report_pending = false;
            process.parent
        };
        if current_pid() == Some(pid) {
            clear_current();
        }
        if let Some(parent) = parent {
            self.queue_sigchld(parent);
        }
        Some(self.wake_waiters_for(pid))
    }

    fn continue_process(&mut self, pid: Pid) -> Option<WakeList> {
        let parent = {
            let process = self.get_mut(pid)?;
            if matches!(process.state, ProcessState::Stopped(_)) {
                process.state = ProcessState::Ready;
                process.stop_reported = false;
                process.continued_report_pending = true;
                process.parent
            } else if matches!(process.state, ProcessState::Zombie(_)) {
                return None;
            } else {
                return Some(WakeList::new());
            }
        };
        let mut wake = WakeList::new();
        wake.push_unique(pid);
        if let Some(parent) = parent {
            self.queue_sigchld(parent);
            self.wake_waiter_for_child(parent, pid, &mut wake);
        }
        Some(wake)
    }

    fn wait(&mut self, parent: Pid, child: Pid) -> Option<i32> {
        let process = self.get(child)?;
        if process.parent != Some(parent) {
            return None;
        }
        let state = process.state;
        match state {
            ProcessState::Zombie(reason) => {
                self.reap(child);
                Some(reason.legacy_status())
            }
            _ => {
                self.get_mut(parent)?.state = ProcessState::Blocked(BlockReason::WaitChild(child));
                None
            }
        }
    }

    fn reap(&mut self, pid: Pid) {
        if let Some(index) = self.processes.iter().position(|p| p.pid == pid) {
            let process = self.processes.remove(index);
            process.destroy();
        }
    }

    fn signal(&mut self, pid: Pid, status: i32, current: Option<Pid>) -> Option<WakeList> {
        {
            let process = self.get_mut(pid)?;
            discard_pending_signal_interactions(process, status);
        }
        let process = self.get(pid)?;
        if is_ignored_signal(process, status) {
            return Some(WakeList::new());
        }
        if should_queue_signal(process, status, current) {
            let mut wake = WakeList::new();
            {
                let process = self.get_mut(pid)?;
                queue_signal(process, status);
                if next_deliverable_pending_signal(process).is_some()
                    && matches!(process.state, ProcessState::Blocked(_))
                {
                    process.interrupted_syscall = true;
                    process.state = ProcessState::Ready;
                    wake.push_unique(pid);
                }
            }
            Some(wake)
        } else {
            if let Some(signal) = stop_signal_from_status(status) {
                return self.stop(pid, signal);
            }
            if let Some(signal) = signal_from_legacy_status(status) {
                Some(self.exit_signaled(pid, signal))
            } else {
                Some(self.exit(pid, status))
            }
        }
    }
}

impl Process {
    fn duplicate_fd_entries(entries: &[FdEntry]) -> Result<Vec<FdEntry>, ForkError> {
        let mut duplicated = Vec::new();
        duplicated
            .try_reserve_exact(entries.len())
            .map_err(|_| ForkError::OutOfMemory)?;
        for entry in entries {
            let vfs_fd = match fs::duplicate_fd(entry.vfs_fd) {
                Ok(vfs_fd) => vfs_fd,
                Err(err) => {
                    Self::close_fd_entries(&duplicated);
                    return Err(map_fork_vfs_error(err));
                }
            };
            duplicated.push(FdEntry { vfs_fd, ..*entry });
        }
        Ok(duplicated)
    }

    fn close_fd_entries(entries: &[FdEntry]) {
        for entry in entries {
            let _ = fs::close(entry.vfs_fd);
        }
    }

    fn duplicate_socket_handles(handles: &[usize]) -> Result<Vec<usize>, ForkError> {
        let mut duplicated = Vec::new();
        duplicated
            .try_reserve_exact(handles.len())
            .map_err(|_| ForkError::OutOfMemory)?;
        for handle in handles {
            if let Err(err) = crate::net::socket::with_sockets(|table| table.duplicate(*handle)) {
                Self::close_socket_handle_list(&duplicated);
                return Err(map_fork_socket_error(err));
            }
            duplicated.push(*handle);
        }
        Ok(duplicated)
    }

    fn close_socket_handle_list(handles: &[usize]) {
        for handle in handles {
            let _ = crate::net::socket::with_sockets(|table| table.close(*handle));
        }
    }

    fn duplicate_shared_mappings(
        mappings: &[SharedMapping],
    ) -> Result<Vec<SharedMapping>, ForkError> {
        let mut duplicated = Vec::new();
        duplicated
            .try_reserve_exact(mappings.len())
            .map_err(|_| ForkError::OutOfMemory)?;
        for mapping in mappings {
            let vfs_fd = match fs::duplicate_fd(mapping.vfs_fd) {
                Ok(vfs_fd) => vfs_fd,
                Err(err) => {
                    Self::close_shared_mapping_fds(&duplicated);
                    return Err(map_fork_vfs_error(err));
                }
            };
            duplicated.push(SharedMapping { vfs_fd, ..*mapping });
        }
        Ok(duplicated)
    }

    fn close_shared_mapping_fds(mappings: &[SharedMapping]) {
        for mapping in mappings {
            let _ = fs::close(mapping.vfs_fd);
        }
    }

    fn clone_string(value: &String) -> Result<String, ForkError> {
        let mut cloned = String::new();
        cloned
            .try_reserve_exact(value.len())
            .map_err(|_| ForkError::OutOfMemory)?;
        cloned.push_str(value);
        Ok(cloned)
    }

    fn clone_process(&self) -> Result<Self, ForkError> {
        let name = Self::clone_string(&self.name)?;
        let cwd = Self::clone_string(&self.cwd)?;
        let address_space = self
            .address_space
            .clone_full_copy()
            .map_err(map_fork_paging_error)?;
        let fds = match Self::duplicate_fd_entries(&self.fds) {
            Ok(fds) => fds,
            Err(err) => {
                address_space.destroy();
                return Err(err);
            }
        };
        let socket_handles = match Self::duplicate_socket_handles(&self.socket_handles) {
            Ok(socket_handles) => socket_handles,
            Err(err) => {
                Self::close_fd_entries(&fds);
                address_space.destroy();
                return Err(err);
            }
        };
        let shared_mappings = match Self::duplicate_shared_mappings(&self.shared_mappings) {
            Ok(shared_mappings) => shared_mappings,
            Err(err) => {
                Self::close_socket_handle_list(&socket_handles);
                Self::close_fd_entries(&fds);
                address_space.destroy();
                return Err(err);
            }
        };
        Ok(Self {
            pid: self.pid,
            parent: self.parent,
            name,
            cwd,
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
            shared_mappings,
            timed_wait: None,
            next_fd: self.next_fd,
            entry: self.entry,
            stack_top: self.stack_top,
            argc: self.argc,
            argv_ptr: self.argv_ptr,
            envp_ptr: self.envp_ptr,
            exit_status: None,
            stop_reported: false,
            continued_report_pending: false,
            waiters: Vec::new(),
            is_user: self.is_user,
            saved_syscall: None,
            interrupted_syscall: false,
            fpu_state: self.fpu_state,
        })
    }
}

fn map_fork_vfs_error(err: fs::vfs::VfsError) -> ForkError {
    match err {
        fs::vfs::VfsError::OutOfMemory | fs::vfs::VfsError::TooManyOpenFiles => {
            ForkError::OutOfMemory
        }
        _ => ForkError::InvalidProcessState,
    }
}

fn map_fork_paging_error(err: paging::PagingError) -> ForkError {
    match err {
        paging::PagingError::OutOfFrames
        | paging::PagingError::RefcountOverflow
        | paging::PagingError::RefcountUnavailable => ForkError::OutOfMemory,
        _ => ForkError::InvalidProcessState,
    }
}

fn map_fork_socket_error(err: crate::net::socket::SocketError) -> ForkError {
    match err {
        crate::net::socket::SocketError::RefcountOverflow => ForkError::OutOfMemory,
        _ => ForkError::InvalidProcessState,
    }
}

pub fn init() {
    let mut table = ProcessTable::new();
    let init = table.spawn_init();
    crate::println!("Process table initialized with init pid {}.", init);
    *PROCESS_TABLE.lock() = Some(table);
    self_test();
}

pub fn fork(parent: Pid) -> Result<Pid, ForkError> {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FdTableError {
    TooManyOpenFiles,
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FdInstallError {
    Fault,
    TooManyOpenFiles,
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketInstallError {
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedMappingError {
    InvalidRange,
    OutOfMemory,
    Vfs(fs::vfs::VfsError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmapError {
    Invalid,
    OutOfMemory,
    Vfs(fs::vfs::VfsError),
}

const MAX_SHEBANG_DEPTH: usize = 4;

fn map_fd_table_vfs_error(err: FdTableError) -> fs::vfs::VfsError {
    match err {
        FdTableError::TooManyOpenFiles => fs::vfs::VfsError::TooManyOpenFiles,
        FdTableError::OutOfMemory => fs::vfs::VfsError::OutOfMemory,
    }
}

fn map_dup_target_error(err: FdTableError) -> fs::vfs::VfsError {
    match err {
        FdTableError::TooManyOpenFiles => fs::vfs::VfsError::BadFd,
        FdTableError::OutOfMemory => fs::vfs::VfsError::OutOfMemory,
    }
}

fn map_fd_install_error(err: FdTableError) -> FdInstallError {
    match err {
        FdTableError::TooManyOpenFiles => FdInstallError::TooManyOpenFiles,
        FdTableError::OutOfMemory => FdInstallError::OutOfMemory,
    }
}

fn trim_ascii_bytes(mut bytes: &[u8]) -> &[u8] {
    while bytes
        .first()
        .is_some_and(|byte| matches!(*byte, b' ' | b'\t'))
    {
        bytes = &bytes[1..];
    }
    while bytes
        .last()
        .is_some_and(|byte| matches!(*byte, b' ' | b'\t' | b'\r'))
    {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn try_exec_string(value: &str) -> Result<String, ExecError> {
    let mut string = String::new();
    string
        .try_reserve_exact(value.len())
        .map_err(|_| ExecError::OutOfMemory)?;
    string.push_str(value);
    Ok(string)
}

fn parse_shebang(
    path: &str,
    data: &[u8],
    args: &[&str],
) -> Result<Option<(String, Vec<String>)>, ExecError> {
    if !data.starts_with(b"#!") {
        return Ok(None);
    }
    let end = data[2..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|offset| offset + 2)
        .unwrap_or(data.len());
    let line = trim_ascii_bytes(&data[2..end]);
    if line.is_empty() {
        return Ok(None);
    }
    let split = line
        .iter()
        .position(|byte| matches!(*byte, b' ' | b'\t'))
        .unwrap_or(line.len());
    let interpreter = trim_ascii_bytes(&line[..split]);
    if interpreter.is_empty() {
        return Ok(None);
    }
    let interpreter = match core::str::from_utf8(interpreter) {
        Ok(interpreter) => interpreter,
        Err(_) => return Ok(None),
    };
    let optional = if split < line.len() {
        let optional = trim_ascii_bytes(&line[split..]);
        if optional.is_empty() {
            None
        } else {
            match core::str::from_utf8(optional) {
                Ok(optional) => Some(optional),
                Err(_) => return Ok(None),
            }
        }
    } else {
        None
    };
    let arg_count = 2usize
        .checked_add(usize::from(optional.is_some()))
        .and_then(|count| count.checked_add(args.len().saturating_sub(1)))
        .ok_or(ExecError::TooManyArguments)?;
    if arg_count > MAX_USER_ARGS {
        return Err(ExecError::TooManyArguments);
    }
    let mut script_args = Vec::new();
    script_args
        .try_reserve_exact(arg_count)
        .map_err(|_| ExecError::OutOfMemory)?;
    script_args.push(try_exec_string(interpreter)?);
    if let Some(optional) = optional {
        script_args.push(try_exec_string(optional)?);
    }
    script_args.push(try_exec_string(path)?);
    for arg in args.iter().skip(1) {
        script_args.push(try_exec_string(arg)?);
    }
    Ok(Some((try_exec_string(interpreter)?, script_args)))
}

fn exec_for_user_inner(
    table: &mut ProcessTable,
    pid: Pid,
    path: &str,
    args: &[&str],
    env: &[&str],
    depth: usize,
) -> Result<ExecInfo, ExecError> {
    if depth > MAX_SHEBANG_DEPTH {
        return Err(ExecError::InvalidImage);
    }
    let metadata = fs::stat(path).map_err(|_| ExecError::NotFound)?;
    let data = fs::read_file(path).ok_or(ExecError::NotFound)?;
    let index = table
        .processes
        .iter()
        .position(|p| p.pid == pid)
        .ok_or(ExecError::NotFound)?;
    let credentials = table.processes[index].credentials;
    let file = FileMetadata::new(metadata.owner, metadata.group, metadata.mode);
    if !file.can_access(credentials, Access::Execute) {
        return Err(ExecError::PermissionDenied);
    }

    if let Some((interpreter, script_args)) = parse_shebang(path, &data, args)? {
        let interpreter = if interpreter.starts_with('/') {
            interpreter
        } else {
            resolve_process_path(&table.processes[index], &interpreter)
                .map_err(|_| ExecError::NotFound)?
        };
        let mut script_arg_refs = Vec::new();
        script_arg_refs
            .try_reserve_exact(script_args.len())
            .map_err(|_| ExecError::OutOfMemory)?;
        for arg in &script_args {
            script_arg_refs.push(arg.as_str());
        }
        return exec_for_user_inner(table, pid, &interpreter, &script_arg_refs, env, depth + 1);
    }

    if data.get(0..4) != Some(b"\x7fELF") {
        return Err(ExecError::InvalidImage);
    }

    let prepared = table.processes[index].prepare_exec(&data, args, env)?;
    let name = try_exec_string(path)?;
    table.processes[index].commit_exec(name, prepared);
    table.processes[index].close_on_exec_fds();
    table.processes[index].close_on_exec_socket_handles();
    // Preserve fds across exec except descriptors marked FD_CLOEXEC.
    if metadata.mode & 0o4000 != 0 {
        table.processes[index].credentials.euid = metadata.owner;
    }
    if metadata.mode & 0o2000 != 0 {
        table.processes[index].credentials.egid = metadata.group;
    }
    table.processes[index].state = ProcessState::Running;
    let p = &table.processes[index];
    Ok(ExecInfo {
        entry: p.entry,
        stack_top: p.stack_top,
        argc: p.argc,
        argv_ptr: p.argv_ptr,
        envp_ptr: p.envp_ptr,
    })
}

/// Execve invoked from a running user process. Replaces the address space of
/// `pid` with the program at `path`, preserves the existing file descriptors,
/// and returns the entry/stack info so the syscall dispatcher can patch the
/// outgoing iretq frame.
pub fn exec_for_user(
    pid: Pid,
    path: &str,
    args: &[&str],
    env: &[&str],
) -> Result<ExecInfo, ExecError> {
    with_table(|table| exec_for_user_inner(table, pid, path, args, env, 0))
}

pub fn get_parent(pid: Pid) -> Option<Pid> {
    with_table(|table| table.get(pid).and_then(|p| p.parent))
}

pub fn install_pipe_fds_with_flags(
    pipefd: usize,
    read_vfs: usize,
    write_vfs: usize,
    status_flags: u32,
    fd_flags: u32,
) -> Result<(), FdInstallError> {
    let Some(parent) = current_pid() else {
        let _ = fs::close(read_vfs);
        let _ = fs::close(write_vfs);
        return Err(FdInstallError::Fault);
    };
    let (user_read, user_write) = with_table(|table| {
        let Some(process) = table.get_mut(parent) else {
            let _ = fs::close(read_vfs);
            let _ = fs::close(write_vfs);
            return Err(FdInstallError::Fault);
        };
        let user_read = match process.push_fd_with_all_flags(read_vfs, status_flags, fd_flags) {
            Ok(fd) => fd,
            Err(err) => {
                let _ = fs::close(read_vfs);
                let _ = fs::close(write_vfs);
                return Err(map_fd_install_error(err));
            }
        };
        let user_write = match process.push_fd_with_all_flags(write_vfs, status_flags, fd_flags) {
            Ok(fd) => fd,
            Err(err) => {
                if let Some(vfs_fd) = process.remove_fd(user_read) {
                    let _ = fs::close(vfs_fd);
                }
                let _ = fs::close(write_vfs);
                return Err(map_fd_install_error(err));
            }
        };
        Ok((user_read, user_write))
    })?;
    let Some(out) = write_user_buffer(pipefd, 8) else {
        let _ = user_close(user_read);
        let _ = user_close(user_write);
        return Err(FdInstallError::Fault);
    };
    out[0..4].copy_from_slice(&(user_read as u32).to_le_bytes());
    out[4..8].copy_from_slice(&(user_write as u32).to_le_bytes());
    Ok(())
}

pub fn exit(pid: Pid, status: i32) {
    let wake = with_table(|table| table.exit(pid, status));
    wake_io_waiters();
    wake_processes(wake);
}

pub fn exit_signaled(pid: Pid, signal: u8) {
    let wake = with_table(|table| table.exit_signaled(pid, signal));
    wake_io_waiters();
    wake_processes(wake);
}

pub fn wait(parent: Pid, child: Pid) -> Option<i32> {
    with_table(|table| table.wait(parent, child))
}

pub fn signal(pid: Pid, status: i32) -> bool {
    let current = current_pid();
    let wake = with_table(|table| table.signal(pid, status, current));
    if let Some(wake) = wake {
        wake_processes(wake);
        true
    } else {
        false
    }
}

pub fn stop_current_signal(pid: Pid, signal: u8) -> bool {
    let wake = with_table(|table| table.stop(pid, signal));
    if let Some(wake) = wake {
        wake_processes(wake);
        true
    } else {
        false
    }
}

pub fn continue_process(pid: Pid) -> bool {
    let wake = with_table(|table| table.continue_process(pid));
    if let Some(wake) = wake {
        wake_processes(wake);
        true
    } else {
        false
    }
}

fn wake_processes(wake: WakeList) {
    for pid in wake.as_slice() {
        scheduler::wake_blocked(*pid);
    }
    if wake.overflow {
        wake_all_runnable_processes();
    }
}

fn wake_all_runnable_processes() {
    let mut cursor = 0;
    while let Some(pid) = next_runnable_pid_after(cursor) {
        cursor = pid;
        scheduler::wake_blocked(pid);
    }
}

fn next_runnable_pid_after(after: Pid) -> Option<Pid> {
    with_table(|table| {
        table
            .processes
            .iter()
            .filter(|process| process.pid > after && matches!(process.state, ProcessState::Ready))
            .map(|process| process.pid)
            .min()
    })
}

pub fn take_pending_signal_current() -> Option<(Pid, usize, i32)> {
    let pid = current_pid()?;
    let status = with_table(|table| {
        let process = table.get_mut(pid)?;
        if let Some(signal) = next_deliverable_pending_signal(process) {
            clear_pending_signal(process, signal as usize);
            return Some(128 + signal as i32);
        }
        if process.pending_signals != 0 {
            return None;
        }
        process.pending_signal_status.take()
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

pub fn has_deliverable_signal_current() -> bool {
    with_current_read(|p| next_deliverable_pending_signal(p).is_some()).unwrap_or(false)
}

pub fn take_interrupted_syscall_current() -> bool {
    with_current(|p| {
        let interrupted = p.interrupted_syscall;
        p.interrupted_syscall = false;
        interrupted
    })
    .unwrap_or(false)
}

pub fn set_current_signal_mask(mask: u64) -> Option<u64> {
    const UNBLOCKABLE: u64 =
        (1 << crate::signal::Signal::Kill.number()) | (1 << crate::signal::Signal::Stop.number());
    with_current(|p| {
        let old = p.signal_mask;
        p.signal_mask = mask & !UNBLOCKABLE;
        old
    })
}

pub fn next_pid_in_pgrp_after(pgrp: Pid, after: Pid) -> Option<Pid> {
    with_table(|table| {
        table
            .processes
            .iter()
            .filter(|process| {
                process.pid > after
                    && process.pgrp == pgrp
                    && !matches!(process.state, ProcessState::Zombie(_))
            })
            .map(|process| process.pid)
            .min()
    })
}

pub fn next_process_pid_after(after: Pid) -> Option<Pid> {
    with_table(|table| {
        table
            .processes
            .iter()
            .filter(|process| process.pid > after)
            .map(|process| process.pid)
            .min()
    })
}

pub fn signal_pgrp(pgrp: Pid, status: i32) -> bool {
    let mut delivered = false;
    let mut cursor = 0;
    while let Some(pid) = next_pid_in_pgrp_after(pgrp, cursor) {
        cursor = pid;
        delivered |= signal(pid, status);
    }
    delivered
}

pub fn can_signal_current(target: Pid, signal: Option<u8>) -> Option<bool> {
    let caller = current_pid()?;
    with_table(|table| {
        let caller_proc = table.get(caller)?;
        let target_proc = table.get(target)?;
        if caller == target || caller_proc.credentials.is_superuser() {
            return Some(true);
        }
        if signal == Some(crate::signal::Signal::Cont.number())
            && caller_proc.sid == target_proc.sid
        {
            return Some(true);
        }
        let caller_uid = caller_proc.credentials.uid;
        let caller_euid = caller_proc.credentials.euid;
        let target_uid = target_proc.credentials.uid;
        let target_euid = target_proc.credentials.euid;
        let target_suid = target_proc.credentials.suid;
        Some(
            caller_uid == target_uid
                || caller_uid == target_euid
                || caller_uid == target_suid
                || caller_euid == target_uid
                || caller_euid == target_euid
                || caller_euid == target_suid,
        )
    })
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

pub fn set_pgid(pid: Pid, pgid: Pid) -> Result<(), SetPgidError> {
    let caller = current_pid().ok_or(SetPgidError::NoSuchProcess)?;
    with_table(|table| {
        let target_pid = if pid == 0 { caller } else { pid };
        if target_pid == 0 {
            return Err(SetPgidError::Invalid);
        }
        let target_pgid = if pgid == 0 { target_pid } else { pgid };
        if target_pgid == 0 {
            return Err(SetPgidError::Invalid);
        }
        let caller_sid = table.get(caller).ok_or(SetPgidError::NoSuchProcess)?.sid;
        let target = table.get(target_pid).ok_or(SetPgidError::NoSuchProcess)?;
        if target.pid != caller && target.parent != Some(caller) {
            return Err(SetPgidError::PermissionDenied);
        }
        if target.sid != caller_sid || target.sid == target.pid {
            return Err(SetPgidError::PermissionDenied);
        }
        if target_pgid != target_pid
            && !table
                .processes
                .iter()
                .any(|process| process.sid == caller_sid && process.pgrp == target_pgid)
        {
            return Err(SetPgidError::PermissionDenied);
        }
        table
            .get_mut(target_pid)
            .ok_or(SetPgidError::NoSuchProcess)?
            .pgrp = target_pgid;
        Ok(())
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
        if handler == crate::signal::IGNORE_HANDLER {
            clear_pending_signal(process, signal);
        }
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
    with_table(|table| {
        let process = table.get_mut(pid)?;
        Some(f(process))
    })
}

pub fn with_current_read<T>(f: impl FnOnce(&Process) -> T) -> Option<T> {
    let pid = current_pid()?;
    with_table(|table| {
        let process = table.get(pid)?;
        Some(f(process))
    })
}

pub fn read_user(addr: usize, len: usize) -> Option<&'static [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    with_current_read(|p| {
        if !p.allows_user_read(addr, len) {
            return None;
        }
        Some(unsafe { slice::from_raw_parts(addr as *const u8, len) })
    })?
}

pub fn is_user_executable(addr: usize, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    with_current_read(|p| p.allows_user_execute(addr, len)).unwrap_or(false)
}

pub fn is_user_readable(addr: usize, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    with_current_read(|p| p.allows_user_read(addr, len)).unwrap_or(false)
}

pub fn write_user_buffer(addr: usize, len: usize) -> Option<&'static mut [u8]> {
    if len == 0 {
        return Some(&mut []);
    }
    with_current(|p| {
        if !p.prepare_user_write(addr, len) {
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
            _ => {
                parts
                    .try_reserve_exact(1)
                    .map_err(|_| fs::vfs::VfsError::OutOfMemory)?;
                parts.push(part);
            }
        }
    }

    if parts.is_empty() {
        return try_string_from("/");
    }

    let mut len = 0usize;
    for part in &parts {
        len = len
            .checked_add(1)
            .and_then(|len| len.checked_add(part.len()))
            .ok_or(fs::vfs::VfsError::OutOfMemory)?;
    }
    let mut normalized = String::new();
    normalized
        .try_reserve_exact(len)
        .map_err(|_| fs::vfs::VfsError::OutOfMemory)?;
    for part in parts {
        normalized.push('/');
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
    let cwd = if process.cwd.is_empty() {
        "/"
    } else {
        process.cwd.as_str()
    };
    let needs_slash = !cwd.ends_with('/');
    let len = cwd
        .len()
        .checked_add(if needs_slash { 1 } else { 0 })
        .and_then(|len| len.checked_add(path.len()))
        .ok_or(fs::vfs::VfsError::OutOfMemory)?;
    let mut combined = String::new();
    combined
        .try_reserve_exact(len)
        .map_err(|_| fs::vfs::VfsError::OutOfMemory)?;
    combined.push_str(cwd);
    if needs_slash {
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
        Some(ControllingTty::Console) => Ok(path),
        Some(ControllingTty::Pty(pty)) => try_pts_path(pty),
        None => Err(fs::vfs::VfsError::NotFound),
    }
}

fn try_string_from(value: &str) -> Result<String, fs::vfs::VfsError> {
    let mut out = String::new();
    out.try_reserve_exact(value.len())
        .map_err(|_| fs::vfs::VfsError::OutOfMemory)?;
    out.push_str(value);
    Ok(out)
}

fn decimal_len(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}

fn push_usize_decimal(out: &mut String, mut value: usize) {
    let mut digits = [0u8; 39];
    let mut index = digits.len();
    loop {
        index -= 1;
        digits[index] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for digit in &digits[index..] {
        out.push(*digit as char);
    }
}

fn try_pts_path(pty: usize) -> Result<String, fs::vfs::VfsError> {
    const PREFIX: &str = "/dev/pts/";

    let len = PREFIX
        .len()
        .checked_add(decimal_len(pty))
        .ok_or(fs::vfs::VfsError::OutOfMemory)?;
    let mut out = String::new();
    out.try_reserve_exact(len)
        .map_err(|_| fs::vfs::VfsError::OutOfMemory)?;
    out.push_str(PREFIX);
    push_usize_decimal(&mut out, pty);
    Ok(out)
}

pub fn resolve_current_path(path: &str) -> Result<String, fs::vfs::VfsError> {
    with_current_read(|p| resolve_process_path(p, path)).unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_open(path: &str) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_open_path(p, path)?;
        let vfs_fd = fs::open_read_as(&path, p.credentials)?;
        match p.push_fd(vfs_fd) {
            Ok(fd) => Ok(fd),
            Err(err) => {
                let _ = fs::close(vfs_fd);
                Err(map_fd_table_vfs_error(err))
            }
        }
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
        match p.push_fd_with_flags(vfs_fd, status_flags) {
            Ok(fd) => Ok(fd),
            Err(err) => {
                let _ = fs::close(vfs_fd);
                Err(map_fd_table_vfs_error(err))
            }
        }
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_create(path: &str) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        let mode = 0o644 & !p.umask;
        let vfs_fd = fs::create_file_with_mode_as(&path, p.credentials, mode)?;
        match p.push_fd(vfs_fd) {
            Ok(fd) => Ok(fd),
            Err(err) => {
                let _ = fs::close(vfs_fd);
                Err(map_fd_table_vfs_error(err))
            }
        }
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_vfs_fd(user_fd: usize) -> Option<usize> {
    with_current_read(|p| p.lookup_fd(user_fd)).flatten()
}

pub fn user_close(user_fd: usize) -> Result<(), fs::vfs::VfsError> {
    let vfs_fd = with_current(|p| p.remove_fd(user_fd))
        .flatten()
        .ok_or(fs::vfs::VfsError::BadFd)?;
    let result = fs::close(vfs_fd);
    wake_io_waiters();
    result
}

pub fn install_socket_handle(handle: usize) -> Result<(), SocketInstallError> {
    with_current(|p| p.add_socket_handle(handle)).unwrap_or(Err(SocketInstallError::OutOfMemory))
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

pub fn timed_wait_deadline(key: u64, timeout_ms: u64, now_ms: u64) -> Result<u64, TimedWaitError> {
    with_current(|p| p.timed_wait_deadline(key, timeout_ms, now_ms))
        .unwrap_or(Err(TimedWaitError::NoCurrentProcess))
}

pub fn clear_timed_wait(key: u64) {
    let _ = with_current(|p| {
        p.clear_timed_wait(key);
    });
}

pub fn has_timed_wait(key: u64) -> bool {
    with_current_read(|p| matches!(p.timed_wait, Some(wait) if wait.key == key)).unwrap_or(false)
}

pub fn take_timed_wait_wake(key: u64) -> bool {
    with_current(|p| p.take_timed_wait_wake(key)).unwrap_or(false)
}

pub fn user_dup(user_fd: usize) -> Result<usize, fs::vfs::VfsError> {
    with_current(|p| {
        let status_flags = p.status_flags(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let vfs_fd = p.lookup_fd(user_fd).ok_or(fs::vfs::VfsError::BadFd)?;
        let dup = fs::duplicate_fd(vfs_fd)?;
        match p.push_fd_with_flags(dup, status_flags) {
            Ok(fd) => Ok(fd),
            Err(err) => {
                let _ = fs::close(dup);
                Err(map_fd_table_vfs_error(err))
            }
        }
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
        let old = match p.replace_fd_with_flags(target_fd, dup, status_flags, 0) {
            Ok(old) => old,
            Err(err) => {
                let _ = fs::close(dup);
                return Err(map_dup_target_error(err));
            }
        };
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
        let old = match p.replace_fd_with_flags(target_fd, dup, status_flags, fd_flags) {
            Ok(old) => old,
            Err(err) => {
                let _ = fs::close(dup);
                return Err(map_dup_target_error(err));
            }
        };
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

pub fn user_set_mtime(path: &str, mtime: u64) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let path = resolve_process_path(p, path)?;
        fs::set_mtime_as(&path, mtime, p.credentials)
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

    // Handle Copy-on-Write faults first, but only when the mapping is still
    // logically writable. Read-only and PROT_NONE mappings must fault.
    if present && write_fault && fault_addr < 0x8000_0000 {
        let cow_handled = with_current(|process| {
            process
                .address_space
                .ensure_user_writable(fault_addr, 1)
                .is_ok()
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
        } else {
            false
        }
    })
    .unwrap_or(false)
}

pub fn wake_io_waiters() {
    while let Some(pid) = wake_next_io_waiter(None) {
        crate::sched::wake_blocked(pid);
    }
}

pub fn wake_timed_waiters(key: u64, limit: usize) -> usize {
    let mut woken = 0;
    while woken < limit {
        let Some(pid) = wake_next_timed_waiter(key) else {
            break;
        };
        crate::sched::wake_blocked(pid);
        woken += 1;
    }
    woken
}

fn wake_next_timed_waiter(key: u64) -> Option<Pid> {
    let mut guard = PROCESS_TABLE.lock();
    let table = guard.as_mut()?;
    for process in &mut table.processes {
        let waiting_for_key = matches!(process.timed_wait, Some(wait) if wait.key == key);
        let blocked_for_io = matches!(
            process.state,
            ProcessState::Blocked(BlockReason::WaitIo | BlockReason::WaitIoUntil(_))
        );
        if waiting_for_key && blocked_for_io {
            if let Some(wait) = process.timed_wait.as_mut() {
                wait.woken = true;
            }
            process.state = ProcessState::Ready;
            return Some(process.pid);
        }
    }
    None
}

pub fn wake_expired_io_waiters(now_ms: u64) {
    while let Some(pid) = wake_next_io_waiter(Some(now_ms)) {
        crate::sched::wake_blocked(pid);
    }
}

fn wake_next_io_waiter(now_ms: Option<u64>) -> Option<Pid> {
    let mut guard = PROCESS_TABLE.lock();
    let table = guard.as_mut()?;
    for process in &mut table.processes {
        let should_wake = match (process.state, now_ms) {
            (ProcessState::Blocked(BlockReason::WaitIo), None)
            | (ProcessState::Blocked(BlockReason::WaitIoUntil(_)), None) => true,
            (ProcessState::Blocked(BlockReason::WaitIoUntil(deadline_ms)), Some(now_ms)) => {
                now_ms >= deadline_ms
            }
            _ => false,
        };
        if should_wake {
            process.state = ProcessState::Ready;
            return Some(process.pid);
        }
    }
    None
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

pub fn kill_current(status: i32) {
    if let Some(pid) = current_pid() {
        if let Some(signal) = signal_from_legacy_status(status) {
            exit_signaled(pid, signal);
        } else {
            exit(pid, status);
        }
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

fn map_elf_segment(
    address_space: &mut AddressSpace,
    segment: elf::SegmentView<'_>,
) -> Result<(), ExecError> {
    const PF_X: u32 = 0x1;
    const PF_W: u32 = 0x2;

    address_space.activate();
    let segment_start = segment.vaddr;
    let segment_end = segment_start
        .checked_add(segment.mem_size)
        .ok_or(ExecError::InvalidImage)?;
    if segment_start == segment_end {
        return Ok(());
    }

    let file_end = segment_start
        .checked_add(segment.file_bytes.len())
        .ok_or(ExecError::InvalidImage)?;
    let map_start = paging::align_down(segment_start, FRAME_SIZE);
    let map_end =
        paging::checked_align_up(segment_end, FRAME_SIZE).ok_or(ExecError::InvalidImage)?;
    if map_end > USER_MMAP_START {
        return Err(ExecError::InvalidImage);
    }
    let writable = segment.flags & PF_W != 0;
    let executable = segment.flags & PF_X != 0;
    if writable && executable {
        return Err(ExecError::InvalidImage);
    }
    let protection = if writable {
        UserProtection::ReadWrite
    } else if executable {
        UserProtection::ReadExecute
    } else {
        UserProtection::ReadOnly
    };
    let mut mapped = Vec::new();
    mapped
        .try_reserve_exact((map_end - map_start) / FRAME_SIZE)
        .map_err(|_| ExecError::OutOfMemory)?;

    for page in (map_start..map_end).step_by(FRAME_SIZE) {
        if address_space.is_user_mapped(page) {
            for mapped_page in mapped {
                let _ = address_space.unmap_user_page(mapped_page);
            }
            return Err(ExecError::InvalidImage);
        }
        let frame = frame_allocator::allocate_frame().ok_or(ExecError::OutOfMemory)?;
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
            if let Err(err) = address_space.map_owned_user_page(page, frame, protection) {
                frame_allocator::free_frame(frame);
                for mapped_page in mapped {
                    let _ = address_space.unmap_user_page(mapped_page);
                }
                return match err {
                    paging::PagingError::OutOfFrames
                    | paging::PagingError::RefcountOverflow
                    | paging::PagingError::RefcountUnavailable => Err(ExecError::OutOfMemory),
                    _ => Err(ExecError::InvalidImage),
                };
            }
        }
        mapped.push(page);
    }
    Ok(())
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

    let child = fork(parent).expect("reparent self-test child fork failed");
    let grandchild = fork(child).expect("reparent self-test grandchild fork failed");
    exit(child, 7);
    if wait(parent, child) != Some(7) {
        panic!("reparent self-test parent wait failed");
    }
    if get_parent(grandchild) != Some(parent) {
        panic!("reparent self-test did not adopt orphan to init");
    }
    exit(grandchild, 9);
    if wait(parent, grandchild) != Some(9) {
        panic!("init reaping self-test failed");
    }

    if fork(u64::MAX) != Err(ForkError::NoSuchProcess) {
        panic!("fork missing parent self-test failed");
    }
    set_current(u64::MAX);
    if with_current_read(|_| ()).is_some() || with_current(|_| ()).is_some() {
        panic!("stale current process self-test failed");
    }
    clear_current();
    crate::println!("Process model self-test passed: fork exec wait reparent.");
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

pub fn current_groups_snapshot() -> Option<([u32; 8], usize)> {
    with_current_read(|p| {
        (
            p.credentials.groups,
            p.credentials.group_count.min(p.credentials.groups.len()),
        )
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

pub fn user_cwd() -> Result<String, fs::vfs::VfsError> {
    with_current_read(|p| try_string_from(&p.cwd)).unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn user_chdir(path: &str) -> Result<(), fs::vfs::VfsError> {
    with_current(|p| {
        let resolved = resolve_process_path(p, path)?;
        let fd = fs::open_read_as(&resolved, p.credentials)?;
        let is_directory = fs::directory_entries(fd).is_ok();
        let _ = fs::close(fd);
        if !is_directory {
            return Err(fs::vfs::VfsError::NotFile);
        }
        p.cwd = resolved;
        Ok(())
    })
    .unwrap_or(Err(fs::vfs::VfsError::BadFd))
}

pub fn current_heap_break() -> usize {
    with_current_read(|p| p.address_space.heap_break).unwrap_or(0)
}

/// Find any waitable child matching `selector`. Zombies are only reported here;
/// callers must reap them after userspace status copy succeeds. Stopped
/// children remain waitable for a later `fg`.
pub fn peek_wait_any(
    parent: Pid,
    selector: WaitSelector,
    include_stopped: bool,
    include_continued: bool,
) -> Option<(Pid, WaitStatus)> {
    with_table(|table| {
        if include_stopped {
            let stopped = table
                .processes
                .iter()
                .find(|p| {
                    selector.matches(parent, p)
                        && !p.stop_reported
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

        if include_continued {
            if let Some(process) = table
                .processes
                .iter()
                .find(|p| selector.matches(parent, p) && p.continued_report_pending)
            {
                return Some((process.pid, WaitStatus::Continued));
            }
        }

        let zombie = table
            .processes
            .iter()
            .find(|p| selector.matches(parent, p) && matches!(p.state, ProcessState::Zombie(_)));
        if let Some(process) = zombie {
            let status = match process.state {
                ProcessState::Zombie(reason) => match reason {
                    ExitReason::Exited(status) => WaitStatus::Exited(status),
                    ExitReason::Signaled(signal) => WaitStatus::Signaled(signal),
                },
                _ => return None,
            };
            return Some((process.pid, status));
        }
        None
    })
}

pub fn mark_waited_stopped(parent: Pid, child: Pid) -> bool {
    with_table(|table| {
        let Some(process) = table.get_mut(child) else {
            return false;
        };
        if process.parent != Some(parent) || !matches!(process.state, ProcessState::Stopped(_)) {
            return false;
        }
        process.stop_reported = true;
        true
    })
}

pub fn mark_waited_continued(parent: Pid, child: Pid) -> bool {
    with_table(|table| {
        let Some(process) = table.get_mut(child) else {
            return false;
        };
        if process.parent != Some(parent) || !process.continued_report_pending {
            return false;
        }
        process.continued_report_pending = false;
        true
    })
}

pub fn reap_waited_zombie(parent: Pid, child: Pid) -> bool {
    with_table(|table| {
        let Some(process) = table.get(child) else {
            return false;
        };
        if process.parent != Some(parent) || !matches!(process.state, ProcessState::Zombie(_)) {
            return false;
        }
        table.reap(child);
        true
    })
}

pub fn has_wait_child(parent: Pid, selector: WaitSelector) -> bool {
    with_table(|table| {
        table
            .processes
            .iter()
            .any(|process| selector.matches(parent, process))
    })
}

pub fn has_child(parent: Pid, child: Pid) -> bool {
    let selector = if child == 0 {
        WaitSelector::Any
    } else {
        WaitSelector::Pid(child)
    };
    has_wait_child(parent, selector)
}

pub fn brk(new_break: usize) -> Result<usize, MmapError> {
    with_current(|p| {
        p.address_space
            .grow_heap(new_break)
            .map_err(map_paging_mmap_error)?;
        Ok(p.address_space.heap_break)
    })
    .ok_or(MmapError::Invalid)?
}

fn map_paging_mmap_error(err: paging::PagingError) -> MmapError {
    match err {
        paging::PagingError::OutOfFrames | paging::PagingError::RefcountOverflow => {
            MmapError::OutOfMemory
        }
        paging::PagingError::RefcountUnavailable => MmapError::OutOfMemory,
        _ => MmapError::Invalid,
    }
}

fn map_shared_mapping_mmap_error(err: SharedMappingError) -> MmapError {
    match err {
        SharedMappingError::InvalidRange => MmapError::Invalid,
        SharedMappingError::OutOfMemory => MmapError::OutOfMemory,
        SharedMappingError::Vfs(err) => MmapError::Vfs(err),
    }
}

pub fn mmap_anonymous(
    hint: usize,
    len: usize,
    protection: UserProtection,
) -> Result<usize, MmapError> {
    with_current(|p| {
        p.address_space
            .map_anonymous(hint, len, protection)
            .map_err(map_paging_mmap_error)
    })
    .ok_or(MmapError::Invalid)?
}

pub fn mmap_fixed(addr: usize, len: usize, protection: UserProtection) -> Result<usize, MmapError> {
    with_current(|p| {
        p.flush_shared_mappings_range(addr, len)
            .map_err(MmapError::Vfs)?;
        let discard = p
            .prepare_shared_mappings_discard(addr, len)
            .map_err(map_shared_mapping_mmap_error)?;
        if let Err(err) = p.address_space.map_fixed(addr, len, protection) {
            discard.close_fragments();
            return Err(map_paging_mmap_error(err));
        }
        p.commit_shared_mappings_discard(discard);
        Ok(addr)
    })
    .ok_or(MmapError::Invalid)?
}

pub fn munmap(addr: usize, len: usize) -> Result<(), MmapError> {
    with_current(|p| {
        p.flush_shared_mappings_range(addr, len)
            .map_err(MmapError::Vfs)?;
        let discard = p
            .prepare_shared_mappings_discard(addr, len)
            .map_err(map_shared_mapping_mmap_error)?;
        if let Err(err) = p.address_space.unmap_user_range(addr, len) {
            discard.close_fragments();
            return Err(map_paging_mmap_error(err));
        }
        p.commit_shared_mappings_discard(discard);
        Ok(())
    })
    .ok_or(MmapError::Invalid)?
}

pub fn register_shared_mapping(
    addr: usize,
    len: usize,
    vfs_fd: usize,
    file_offset: usize,
    writable: bool,
) -> Result<(), SharedMappingError> {
    with_current(|p| p.register_shared_mapping(addr, len, vfs_fd, file_offset, writable))
        .ok_or(SharedMappingError::Vfs(fs::vfs::VfsError::BadFd))?
}

pub fn msync(addr: usize, len: usize) -> Result<(), MmapError> {
    with_current(|p| {
        p.flush_shared_mappings_range(addr, len)
            .map_err(MmapError::Vfs)
    })
    .ok_or(MmapError::Invalid)?
}

pub fn mprotect(addr: usize, len: usize, protection: UserProtection) -> Result<(), MmapError> {
    with_current(|p| {
        p.address_space
            .protect_user_range(addr, len, protection)
            .map_err(map_paging_mmap_error)
    })
    .ok_or(MmapError::Invalid)?
}

pub fn get_process_info(pid: Pid) -> Option<(String, ProcessState, Option<Pid>, Option<i32>)> {
    with_table(|table| {
        let p = table.get(pid)?;
        Some((p.name.clone(), p.state, p.parent, p.exit_status))
    })
}

pub fn process_exists(pid: Pid) -> bool {
    with_table(|table| table.get(pid).is_some())
}
