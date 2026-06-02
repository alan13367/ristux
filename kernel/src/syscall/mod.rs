use core::{fmt, str};

pub mod linux;

use crate::{
    fs::vfs::VfsError,
    process::{self, SavedSyscallFrame},
    sched,
    userspace::{self, ProcessState, UserProcess},
};

pub const SYS_WRITE: u64 = 1;
pub const SYS_READ: u64 = 2;
pub const SYS_EXIT: u64 = 3;
pub const SYS_YIELD: u64 = 4;
pub const SYS_SLEEP: u64 = 5;
pub const SYS_GETPID: u64 = 6;
pub const SYS_TIME: u64 = 7;
pub const SYS_OPEN: u64 = 8;
pub const SYS_CLOSE: u64 = 9;
pub const SYS_GETCWD: u64 = 10;
pub const SYS_LISTDIR: u64 = 11;
pub const SYS_DUP: u64 = 12;
pub const SYS_DUP2: u64 = 13;
pub const SYS_CREATE: u64 = 14;
pub const SYS_MKDIR: u64 = 15;
pub const SYS_UNLINK: u64 = 16;
pub const SYS_CHMOD: u64 = 17;
pub const SYS_KILL: u64 = 18;
pub const SYS_UDP_BIND: u64 = 19;
pub const SYS_UDP_SEND: u64 = 20;
pub const SYS_UDP_RECV: u64 = 21;
pub const SYS_WAITPID: u64 = 22;
pub const SYS_BRK: u64 = 23;
pub const SYS_LSEEK: u64 = 24;
pub const SYS_STAT: u64 = 25;
pub const SYS_SIGACTION: u64 = 26;
pub const SYS_SIGRETURN: u64 = 27;

const EAGAIN: i64 = -11;

const EACCES: i64 = -13;
const EBADF: i64 = -9;
const EFAULT: i64 = -14;
const EEXIST: i64 = -17;
const EMFILE: i64 = -24;
const ENOMEM: i64 = -12;
const EINVAL: i64 = -22;
const ENOENT: i64 = -2;
const ESRCH: i64 = -3;
const ENOSYS: i64 = -38;

#[repr(C)]
pub struct SyscallInterruptFrame {
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

pub type SyscallResult = Result<u64, SyscallError>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SyscallError(i64);

impl fmt::Display for SyscallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "syscall error {}", self.0)
    }
}

pub fn init() {
    crate::arch::x86_64::idt::install_syscall_gate();
    crate::arch::x86_64::idt::install_linux_syscall();
    linux::self_test();
    crate::println!("Syscall ABI initialized on int 0x80 and Linux syscall.");
}

#[unsafe(no_mangle)]
pub extern "C" fn syscall_interrupt_dispatch(frame: &mut SyscallInterruptFrame) {
    if frame.cs & 3 == 3 {
        dispatch_interrupt_syscall(frame);
        return;
    }

    crate::println!(
        "Unhandled int 0x80 syscall {:#x} from cs {:#x}",
        frame.rax,
        frame.cs
    );
    frame.rax = ENOSYS as u64;
}

fn dispatch_interrupt_syscall(frame: &mut SyscallInterruptFrame) {
    userspace::record_active_syscall();

    match frame.rax {
        SYS_WRITE => {
            frame.rax = match sys_write_active(
                frame.rdi as usize,
                frame.rsi as usize,
                frame.rdx as usize,
            ) {
                Ok(written) => written,
                Err(err) => err.0 as u64,
            };
        }
        SYS_READ => {
            frame.rax = match sys_read_active(
                frame,
                frame.rdi as usize,
                frame.rsi as usize,
                frame.rdx as usize,
            ) {
                Ok(read) => read,
                Err(err) => err.0 as u64,
            };
        }
        SYS_EXIT => {
            let status = frame.rdi as i32;
            let pid = process::current_pid().unwrap_or(0);
            process::exit(pid, status);
            let unmapped = process::finish_user_run(pid).1;
            crate::println!(
                "Ring 3 ELF process pid {} exited with status {} from rip {:#x}; unmapped {} page(s).",
                pid,
                status,
                frame.rip,
                unmapped
            );
            userspace::record_user_exit(pid, status, unmapped);
            let _ = yield_until_runnable(frame);
            return;
        }
        SYS_YIELD => {
            crate::task::yield_current();
            frame.rax = 0;
        }
        SYS_SLEEP => {
            let tick = crate::arch::x86_64::interrupts::timer_ticks();
            crate::task::sleep_current(tick, frame.rdi);
            frame.rax = 0;
        }
        SYS_GETPID => {
            frame.rax = userspace::active_user_pid();
        }
        SYS_TIME => {
            frame.rax = crate::time::unix_time();
        }
        SYS_OPEN => {
            frame.rax = match sys_open_active(frame.rdi as usize) {
                Ok(fd) => fd,
                Err(err) => err.0 as u64,
            };
        }
        SYS_CLOSE => {
            frame.rax = match sys_close_active(frame.rdi as usize) {
                Ok(status) => status,
                Err(err) => err.0 as u64,
            };
        }
        SYS_GETCWD => {
            frame.rax = match sys_getcwd_active(frame.rdi as usize, frame.rsi as usize) {
                Ok(written) => written,
                Err(err) => err.0 as u64,
            };
        }
        SYS_LISTDIR => {
            frame.rax = match sys_listdir_active(
                frame.rdi as usize,
                frame.rsi as usize,
                frame.rdx as usize,
            ) {
                Ok(written) => written,
                Err(err) => err.0 as u64,
            };
        }
        SYS_DUP => {
            frame.rax = match sys_dup_active(frame.rdi as usize) {
                Ok(fd) => fd,
                Err(err) => err.0 as u64,
            };
        }
        SYS_DUP2 => {
            frame.rax = match sys_dup2_active(frame.rdi as usize, frame.rsi as usize) {
                Ok(fd) => fd,
                Err(err) => err.0 as u64,
            };
        }
        SYS_CREATE => {
            frame.rax = match sys_create_active(frame.rdi as usize) {
                Ok(fd) => fd,
                Err(err) => err.0 as u64,
            };
        }
        SYS_MKDIR => {
            frame.rax = match sys_mkdir_active(frame.rdi as usize) {
                Ok(status) => status,
                Err(err) => err.0 as u64,
            };
        }
        SYS_UNLINK => {
            frame.rax = match sys_unlink_active(frame.rdi as usize) {
                Ok(status) => status,
                Err(err) => err.0 as u64,
            };
        }
        SYS_CHMOD => {
            frame.rax = match sys_chmod_active(frame.rdi as usize, frame.rsi as u16) {
                Ok(status) => status,
                Err(err) => err.0 as u64,
            };
        }
        SYS_KILL => {
            frame.rax = match sys_kill_active(frame.rdi as u64, frame.rsi as u8) {
                Ok(status) => status,
                Err(err) => err.0 as u64,
            };
        }
        SYS_UDP_BIND => {
            frame.rax = match sys_udp_bind_active(frame.rdi as u16) {
                Ok(socket) => socket,
                Err(err) => err.0 as u64,
            };
        }
        SYS_UDP_SEND => {
            frame.rax = match sys_udp_send_active(
                frame.rdi as usize,
                frame.rsi as u32,
                frame.rdx as u16,
                frame.r10 as usize,
                frame.r8 as usize,
            ) {
                Ok(written) => written,
                Err(err) => err.0 as u64,
            };
        }
        SYS_UDP_RECV => {
            frame.rax = match sys_udp_recv_active(
                frame,
                frame.rdi as usize,
                frame.rsi as usize,
                frame.rdx as usize,
            ) {
                Ok(read) => read,
                Err(err) => err.0 as u64,
            };
        }
        SYS_WAITPID => {
            let parent = process::current_pid().unwrap_or(0);
            let child = frame.rsi as process::Pid;
            frame.rax = sys_waitpid_active(frame, parent, child).unwrap_or(-1) as u64;
        }
        SYS_BRK => {
            frame.rax = match process::brk(frame.rdi as usize) {
                Ok(addr) => addr as u64,
                Err(_) => u64::MAX,
            };
        }
        SYS_LSEEK => {
            frame.rax =
                match sys_lseek_active(frame.rdi as usize, frame.rsi as isize, frame.rdx as u32) {
                    Ok(offset) => offset as u64,
                    Err(err) => err.0 as u64,
                };
        }
        SYS_STAT => {
            frame.rax = match sys_stat_active(frame.rdi as usize, frame.rsi as usize) {
                Ok(value) => value,
                Err(err) => err.0 as u64,
            };
        }
        SYS_SIGACTION | SYS_SIGRETURN => {
            frame.rax = 0;
        }
        _ => {
            crate::println!(
                "Unhandled ring 3 int 0x80 syscall {:#x} from rip {:#x}",
                frame.rax,
                frame.rip
            );
            frame.rax = ENOSYS as u64;
        }
    }
}

#[allow(dead_code)]
pub fn dispatch(process: &mut UserProcess, number: u64, args: [usize; 6]) -> SyscallResult {
    match number {
        SYS_WRITE => sys_write(process, args[0], args[1], args[2]),
        SYS_READ => sys_read(process, args[0], args[1], args[2]),
        SYS_EXIT => {
            let status = args[0] as i32;
            process.set_state(ProcessState::Exited(status));
            crate::println!(
                "process {} ({}) exited with status {}",
                process.pid(),
                process.name(),
                status
            );
            Ok(0)
        }
        SYS_YIELD => {
            crate::println!("process {} yielded", process.pid());
            Ok(0)
        }
        SYS_SLEEP => {
            crate::println!("process {} requested sleep({})", process.pid(), args[0]);
            Ok(0)
        }
        SYS_GETPID => {
            crate::println!("process {} getpid()", process.pid());
            Ok(process.pid())
        }
        SYS_TIME => {
            let now = crate::time::unix_time();
            crate::println!("process {} time() -> {}", process.pid(), now);
            Ok(now)
        }
        _ => Err(SyscallError(ENOSYS)),
    }
}

#[allow(dead_code)]
fn sys_write(process: &UserProcess, fd: usize, ptr: usize, len: usize) -> SyscallResult {
    if fd != 1 && fd != 2 {
        return Err(SyscallError(EBADF));
    }

    let bytes = process.read_memory(ptr, len).ok_or(SyscallError(EFAULT))?;
    match str::from_utf8(bytes) {
        Ok(text) => crate::print!("{}", text),
        Err(_) => {
            for byte in bytes {
                crate::print!("{:02x}", byte);
            }
            crate::println!();
        }
    }

    Ok(len as u64)
}

fn sys_write_active(fd: usize, ptr: usize, len: usize) -> SyscallResult {
    let bytes = userspace::active_user_read(ptr, len).ok_or(SyscallError(EFAULT))?;

    if let Some(vfs_fd) = userspace::active_user_vfs_fd(fd) {
        return crate::fs::write(vfs_fd, bytes)
            .map(|written| written as u64)
            .map_err(map_vfs_error);
    }

    if fd != 1 && fd != 2 {
        return Err(SyscallError(EBADF));
    }

    match str::from_utf8(bytes) {
        Ok(text) => crate::print!("{}", text),
        Err(_) => {
            for byte in bytes {
                crate::print!("{:02x}", byte);
            }
            crate::println!();
        }
    }

    Ok(len as u64)
}

fn sys_open_active(path_ptr: usize) -> SyscallResult {
    let mut path = [0u8; 128];
    let path = read_user_cstr(path_ptr, &mut path)?;
    userspace::active_user_open(path)
        .map(|fd| fd as u64)
        .map_err(map_vfs_error)
}

fn sys_create_active(path_ptr: usize) -> SyscallResult {
    let mut path = [0u8; 128];
    let path = read_user_cstr(path_ptr, &mut path)?;
    userspace::active_user_create(path)
        .map(|fd| fd as u64)
        .map_err(map_vfs_error)
}

fn sys_mkdir_active(path_ptr: usize) -> SyscallResult {
    let mut path = [0u8; 128];
    let path = read_user_cstr(path_ptr, &mut path)?;
    userspace::active_user_mkdir(path)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn sys_unlink_active(path_ptr: usize) -> SyscallResult {
    let mut path = [0u8; 128];
    let path = read_user_cstr(path_ptr, &mut path)?;
    userspace::active_user_unlink(path)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn sys_chmod_active(path_ptr: usize, mode: u16) -> SyscallResult {
    let mut path = [0u8; 128];
    let path = read_user_cstr(path_ptr, &mut path)?;
    userspace::active_user_chmod(path, mode)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn sys_kill_active(pid: u64, signal_number: u8) -> SyscallResult {
    let signal = crate::signal::Signal::from_number(signal_number).ok_or(SyscallError(EINVAL))?;
    if crate::signal::send(pid, signal) {
        Ok(0)
    } else {
        Err(SyscallError(ESRCH))
    }
}

fn sys_udp_bind_active(local_port: u16) -> SyscallResult {
    crate::net::udp_bind(local_port)
        .map(|socket| socket as u64)
        .ok_or(SyscallError(EINVAL))
}

fn sys_udp_send_active(
    socket: usize,
    dst_ip: u32,
    dst_port: u16,
    ptr: usize,
    len: usize,
) -> SyscallResult {
    let bytes = userspace::active_user_read(ptr, len).ok_or(SyscallError(EFAULT))?;
    if crate::net::udp_send(socket, dst_ip.to_be_bytes(), dst_port, bytes) {
        Ok(len as u64)
    } else {
        Err(SyscallError(EINVAL))
    }
}

fn sys_udp_recv_active(
    frame: &mut SyscallInterruptFrame,
    socket: usize,
    ptr: usize,
    len: usize,
) -> SyscallResult {
    let buffer = userspace::active_user_write_buffer(ptr, len).ok_or(SyscallError(EFAULT))?;
    loop {
        if let Some(read) = crate::net::udp_recv(socket, buffer) {
            return Ok(read as u64);
        }
        process::block_current(process::BlockReason::WaitIo);
        yield_while_blocked(frame);
    }
}

fn sys_read_active(
    frame: &mut SyscallInterruptFrame,
    fd: usize,
    ptr: usize,
    len: usize,
) -> SyscallResult {
    let vfs_fd = userspace::active_user_vfs_fd(fd).ok_or(SyscallError(EBADF))?;
    let buffer = userspace::active_user_write_buffer(ptr, len).ok_or(SyscallError(EFAULT))?;
    loop {
        match crate::fs::read(vfs_fd, buffer) {
            Ok(read) => return Ok(read as u64),
            Err(crate::fs::vfs::VfsError::WouldBlock) => {
                process::block_current(process::BlockReason::WaitIo);
                yield_while_blocked(frame);
            }
            Err(err) => return Err(map_vfs_error(err)),
        }
    }
}

fn sys_waitpid_active(
    frame: &mut SyscallInterruptFrame,
    parent: process::Pid,
    child: process::Pid,
) -> Option<i32> {
    loop {
        if let Some(status) = process::wait(parent, child) {
            return Some(status);
        }
        if !process::has_child(parent, child) {
            return None;
        }
        process::block_current(process::BlockReason::WaitChild(child));
        yield_while_blocked(frame);
    }
}

fn yield_while_blocked(frame: &mut SyscallInterruptFrame) -> bool {
    let original = process::current_pid();
    let mut saved = saved_from_frame(frame);
    let resumed = sched::yield_from_syscall(&mut saved);
    apply_saved_frame(frame, &saved);
    resumed == original
}

/// Public alias of [`yield_while_blocked`] used by the Linux ABI dispatcher.
pub fn yield_blocked(frame: &mut SyscallInterruptFrame) -> bool {
    yield_while_blocked(frame)
}

pub fn yield_current_process(frame: &mut SyscallInterruptFrame) -> bool {
    let original = process::current_pid();
    let mut saved = saved_from_frame(frame);
    let resumed = sched::yield_current_from_syscall(&mut saved);
    apply_saved_frame(frame, &saved);
    resumed == original
}

/// Yield until a runnable process is dispatched. Used after `exit` empties the
/// current process from the run queue so the kernel can fall back to another
/// task instead of immediately iretq'ing into a torn-down address space.
pub fn yield_until_runnable(frame: &mut SyscallInterruptFrame) -> bool {
    yield_while_blocked(frame)
}

fn saved_from_frame(frame: &SyscallInterruptFrame) -> SavedSyscallFrame {
    SavedSyscallFrame {
        rax: frame.rax,
        rbx: frame.rbx,
        rcx: frame.rcx,
        rdx: frame.rdx,
        rsi: frame.rsi,
        rdi: frame.rdi,
        rbp: frame.rbp,
        r8: frame.r8,
        r9: frame.r9,
        r10: frame.r10,
        r11: frame.r11,
        r12: frame.r12,
        r13: frame.r13,
        r14: frame.r14,
        r15: frame.r15,
        rip: frame.rip,
        cs: frame.cs,
        rflags: frame.rflags,
        rsp: frame.rsp,
        ss: frame.ss,
    }
}

fn apply_saved_frame(frame: &mut SyscallInterruptFrame, saved: &SavedSyscallFrame) {
    frame.rax = saved.rax;
    frame.rbx = saved.rbx;
    frame.rcx = saved.rcx;
    frame.rdx = saved.rdx;
    frame.rsi = saved.rsi;
    frame.rdi = saved.rdi;
    frame.rbp = saved.rbp;
    frame.r8 = saved.r8;
    frame.r9 = saved.r9;
    frame.r10 = saved.r10;
    frame.r11 = saved.r11;
    frame.r12 = saved.r12;
    frame.r13 = saved.r13;
    frame.r14 = saved.r14;
    frame.r15 = saved.r15;
    frame.rip = saved.rip;
    frame.cs = saved.cs;
    frame.rflags = saved.rflags;
    frame.rsp = saved.rsp;
    frame.ss = saved.ss;
}

fn sys_close_active(fd: usize) -> SyscallResult {
    userspace::active_user_close(fd)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn sys_dup_active(fd: usize) -> SyscallResult {
    userspace::active_user_dup(fd)
        .map(|fd| fd as u64)
        .map_err(map_vfs_error)
}

fn sys_dup2_active(fd: usize, target_fd: usize) -> SyscallResult {
    userspace::active_user_dup2(fd, target_fd)
        .map(|fd| fd as u64)
        .map_err(map_vfs_error)
}

fn sys_getcwd_active(ptr: usize, len: usize) -> SyscallResult {
    copy_bytes_to_user(b"/", ptr, len)
}

fn sys_listdir_active(prefix_ptr: usize, ptr: usize, len: usize) -> SyscallResult {
    let mut prefix = [0u8; 128];
    let prefix = read_user_cstr(prefix_ptr, &mut prefix)?;
    let paths = crate::fs::list_paths(prefix);
    let buffer = userspace::active_user_write_buffer(ptr, len).ok_or(SyscallError(EFAULT))?;
    let mut offset = 0;

    for path in paths {
        let bytes = path.as_bytes();
        let needed = bytes.len() + 1;
        if offset + needed > buffer.len() {
            break;
        }
        buffer[offset..offset + bytes.len()].copy_from_slice(bytes);
        offset += bytes.len();
        buffer[offset] = b'\n';
        offset += 1;
    }

    Ok(offset as u64)
}

fn copy_bytes_to_user(bytes: &[u8], ptr: usize, len: usize) -> SyscallResult {
    if bytes.len() > len {
        return Err(SyscallError(EFAULT));
    }

    let buffer = userspace::active_user_write_buffer(ptr, len).ok_or(SyscallError(EFAULT))?;
    buffer[..bytes.len()].copy_from_slice(bytes);
    Ok(bytes.len() as u64)
}

fn read_user_cstr<'a>(ptr: usize, buffer: &'a mut [u8]) -> Result<&'a str, SyscallError> {
    for index in 0..buffer.len() {
        let byte = userspace::active_user_read(ptr + index, 1)
            .and_then(|bytes| bytes.first().copied())
            .ok_or(SyscallError(EFAULT))?;
        if byte == 0 {
            return str::from_utf8(&buffer[..index]).map_err(|_| SyscallError(EFAULT));
        }
        buffer[index] = byte;
    }

    Err(SyscallError(EFAULT))
}

fn sys_lseek_active(fd: usize, offset: isize, whence: u32) -> SyscallResult {
    let vfs_fd = userspace::active_user_vfs_fd(fd).ok_or(SyscallError(EBADF))?;
    crate::fs::lseek(vfs_fd, offset, whence)
        .map(|offset| offset as u64)
        .map_err(map_vfs_error)
}

fn sys_stat_active(path_ptr: usize, stat_ptr: usize) -> SyscallResult {
    let mut path = [0u8; 128];
    let path = read_user_cstr(path_ptr, &mut path)?;
    let path = process::resolve_current_path(path).map_err(map_vfs_error)?;
    let stat = crate::fs::stat(&path).map_err(map_vfs_error)?;
    let buffer = userspace::active_user_write_buffer(stat_ptr, 18).ok_or(SyscallError(EFAULT))?;
    buffer[0..2].copy_from_slice(&stat.mode.to_le_bytes());
    buffer[2..10].copy_from_slice(&stat.size.to_le_bytes());
    buffer[10..18].copy_from_slice(&stat.mtime.to_le_bytes());
    Ok(0)
}

fn map_vfs_error(err: VfsError) -> SyscallError {
    match err {
        VfsError::NotFound => SyscallError(ENOENT),
        VfsError::BadFd => SyscallError(EBADF),
        VfsError::NotFile | VfsError::Utf8 => SyscallError(EFAULT),
        VfsError::AlreadyExists => SyscallError(EEXIST),
        VfsError::PermissionDenied => SyscallError(EACCES),
        VfsError::WouldBlock => SyscallError(EAGAIN),
        VfsError::TooManyOpenFiles => SyscallError(EMFILE),
        VfsError::OutOfMemory => SyscallError(ENOMEM),
    }
}

#[allow(dead_code)]
fn sys_read(_process: &mut UserProcess, fd: usize, _ptr: usize, _len: usize) -> SyscallResult {
    if fd != 0 {
        return Err(SyscallError(EBADF));
    }

    Ok(0)
}
