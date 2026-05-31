//! Linux x86_64 syscall numbers and dispatch.
//!
//! Entry from `linux_syscall_entry` (assembly) constructs a
//! [`SyscallInterruptFrame`] on the kernel stack — identical to the int 0x80
//! path — and tail-calls [`linux_syscall_dispatch_frame`]. On return the entry
//! assembly issues `iretq`, unifying the resume mechanism with the Ristux ABI
//! so blocked Linux syscalls can be parked/woken via
//! [`crate::sched::yield_from_syscall`].

#![allow(non_upper_case_globals)]

use alloc::vec::Vec;

use crate::{
    fs,
    memory::{
        address_space::{USER_MMAP_END, USER_MMAP_START},
        frame_allocator::FRAME_SIZE,
    },
    process,
    syscall::SyscallInterruptFrame,
    tty,
};

pub const NR_read: u64 = 0;
pub const NR_write: u64 = 1;
pub const NR_open: u64 = 2;
pub const NR_close: u64 = 3;
pub const NR_stat: u64 = 4;
pub const NR_fstat: u64 = 5;
pub const NR_lstat: u64 = 6;
pub const NR_poll: u64 = 7;
pub const NR_lseek: u64 = 8;
pub const NR_mmap: u64 = 9;
pub const NR_mprotect: u64 = 10;
pub const NR_munmap: u64 = 11;
pub const NR_brk: u64 = 12;
pub const NR_rt_sigaction: u64 = 13;
pub const NR_rt_sigprocmask: u64 = 14;
pub const NR_rt_sigreturn: u64 = 15;
pub const NR_ioctl: u64 = 16;
pub const NR_writev: u64 = 20;
pub const NR_access: u64 = 21;
pub const NR_pipe: u64 = 22;
pub const NR_select: u64 = 23;
pub const NR_sched_yield: u64 = 24;
pub const NR_dup: u64 = 32;
pub const NR_dup2: u64 = 33;
pub const NR_nanosleep: u64 = 35;
pub const NR_getpid: u64 = 39;
pub const NR_socket: u64 = 41;
pub const NR_connect: u64 = 42;
pub const NR_accept: u64 = 43;
pub const NR_sendto: u64 = 44;
pub const NR_recvfrom: u64 = 45;
pub const NR_shutdown: u64 = 48;
pub const NR_bind: u64 = 49;
pub const NR_listen: u64 = 50;
pub const NR_getsockname: u64 = 51;
pub const NR_getpeername: u64 = 52;
pub const NR_setsockopt: u64 = 54;
pub const NR_getsockopt: u64 = 55;
pub const NR_fork: u64 = 57;
pub const NR_execve: u64 = 59;
pub const NR_exit: u64 = 60;
pub const NR_wait4: u64 = 61;
pub const NR_kill: u64 = 62;
pub const NR_fcntl: u64 = 72;
pub const NR_fsync: u64 = 74;
pub const NR_ftruncate: u64 = 77;
pub const NR_getdents: u64 = 78;
pub const NR_getcwd: u64 = 79;
pub const NR_chdir: u64 = 80;
pub const NR_rename: u64 = 82;
pub const NR_mkdir: u64 = 83;
pub const NR_rmdir: u64 = 84;
pub const NR_link: u64 = 86;
pub const NR_unlink: u64 = 87;
pub const NR_symlink: u64 = 88;
pub const NR_readlink: u64 = 89;
pub const NR_chmod: u64 = 90;
pub const NR_chown: u64 = 92;
pub const NR_umask: u64 = 95;
pub const NR_gettimeofday: u64 = 96;
pub const NR_getuid: u64 = 102;
pub const NR_getgid: u64 = 104;
pub const NR_setuid: u64 = 105;
pub const NR_setgid: u64 = 106;
pub const NR_geteuid: u64 = 107;
pub const NR_getegid: u64 = 108;
pub const NR_setpgid: u64 = 109;
pub const NR_getppid: u64 = 110;
pub const NR_getpgrp: u64 = 111;
pub const NR_setsid: u64 = 112;
pub const NR_setgroups: u64 = 116;
pub const NR_setresuid: u64 = 117;
pub const NR_setresgid: u64 = 119;
pub const NR_rt_sigpending: u64 = 127;
pub const NR_time: u64 = 201;
pub const NR_getdents64: u64 = 217;
pub const NR_clock_gettime: u64 = 228;
pub const NR_getrandom: u64 = 318;

const ESRCH: i64 = -3;
const EPERM: i64 = -1;
const EBADF: i64 = -9;
const ENOMEM: i64 = -12;
const EMFILE: i64 = -24;
const EFAULT: i64 = -14;
const ENOENT: i64 = -2;
const EAGAIN: i64 = -11;
const EACCES: i64 = -13;
const EEXIST: i64 = -17;
const ENOSYS: i64 = -38;
const EINVAL: i64 = -22;
const ENOTTY: i64 = -25;
const ECONNRESET: i64 = -104;
const ENOTCONN: i64 = -107;
const ETIMEDOUT: i64 = -110;
const CONTEXT_SWITCHED: i64 = i64::MIN;
const SOCKET_FD_BASE: usize = 1000;
const AF_INET: i32 = 2;
const SOCK_STREAM: i32 = 1;
const SOCK_DGRAM: i32 = 2;
const SOCK_RAW: i32 = 3;
const IPPROTO_ICMP: i32 = 1;
const SOL_SOCKET: i32 = 1;
const SO_REUSEADDR: i32 = 2;
const SO_ERROR: i32 = 4;
const SO_RCVTIMEO: i32 = 20;
const SO_SNDTIMEO: i32 = 21;
const IPPROTO_TCP: i32 = 6;
const TCP_NODELAY: i32 = 1;
const O_ACCMODE: u32 = 0o3;
const O_APPEND: u32 = 0o2000;
const O_NONBLOCK: u32 = 0o4000;
const SETTABLE_STATUS_FLAGS: u32 = O_APPEND | O_NONBLOCK;
const PROT_READ: i32 = 0x1;
const PROT_WRITE: i32 = 0x2;
const PROT_EXEC: i32 = 0x4;
const MAP_PRIVATE: i32 = 0x02;
const MAP_FIXED: i32 = 0x10;
const MAP_ANONYMOUS: i32 = 0x20;
const POLLIN: i16 = 0x001;
const POLLOUT: i16 = 0x004;
const POLLERR: i16 = 0x008;
const POLLHUP: i16 = 0x010;
const POLLNVAL: i16 = 0x020;
const MSG_DONTWAIT: i32 = 0x40;
const GRND_NONBLOCK: u32 = 0x0001;
const GRND_RANDOM: u32 = 0x0002;
const IOV_MAX: usize = 1024;

/// Entry from `linux_syscall_entry` assembly. The frame holds saved user
/// registers and the SYSV-style return state for `iretq`.
#[unsafe(no_mangle)]
pub extern "C" fn linux_syscall_dispatch_frame(frame: &mut SyscallInterruptFrame) {
    process::save_current_fpu();
    let nr = frame.rax;
    let a0 = frame.rdi;
    let a1 = frame.rsi;
    let a2 = frame.rdx;
    let a3 = frame.r10;
    let a4 = frame.r8;
    let a5 = frame.r9;

    if deliver_pending_signal(frame) {
        process::restore_current_fpu();
        return;
    }

    let result: Result<u64, i64> = match nr {
        NR_write => linux_write(frame, a0 as usize, a1 as usize, a2 as usize),
        NR_read => linux_read(frame, a0 as usize, a1 as usize, a2 as usize),
        NR_writev => linux_writev(a0 as usize, a1 as usize, a2 as usize),
        NR_open => linux_open(a0 as usize, a1 as i32, a2 as u32),
        NR_close => linux_close(a0 as usize),
        NR_poll => linux_poll(frame, a0 as usize, a1 as usize, a2 as i32),
        NR_access => linux_access(a0 as usize, a1 as i32),
        NR_pipe => linux_pipe(a0 as usize),
        NR_select => linux_select(
            frame,
            a0 as i32,
            a1 as usize,
            a2 as usize,
            a3 as usize,
            a4 as usize,
        ),
        NR_sched_yield => linux_sched_yield(frame),
        NR_dup => linux_dup(a0 as usize),
        NR_dup2 => linux_dup2(a0 as usize, a1 as usize),
        NR_nanosleep => linux_nanosleep(a0 as usize, a1 as usize),
        NR_getpid => Ok(process::current_pid().unwrap_or(0)),
        NR_socket => linux_socket(a0 as i32, a1 as i32, a2 as i32),
        NR_connect => linux_connect(a0 as usize, a1 as usize, a2 as usize),
        NR_accept => linux_accept(a0 as usize, a1 as usize, a2 as usize),
        NR_sendto => linux_sendto(
            a0 as usize,
            a1 as usize,
            a2 as usize,
            a3 as i32,
            a4 as usize,
            a5 as usize,
        ),
        NR_recvfrom => linux_recvfrom(
            frame,
            a0 as usize,
            a1 as usize,
            a2 as usize,
            a3 as i32,
            a4 as usize,
            a5 as usize,
        ),
        NR_shutdown => linux_shutdown(a0 as usize, a1 as i32),
        NR_bind => linux_bind(a0 as usize, a1 as usize, a2 as usize),
        NR_listen => linux_listen(a0 as usize, a1 as i32),
        NR_getsockname => linux_getsockname(a0 as usize, a1 as usize, a2 as usize),
        NR_getpeername => linux_getpeername(a0 as usize, a1 as usize, a2 as usize),
        NR_setsockopt => {
            linux_setsockopt(a0 as usize, a1 as i32, a2 as i32, a3 as usize, a4 as usize)
        }
        NR_getsockopt => {
            linux_getsockopt(a0 as usize, a1 as i32, a2 as i32, a3 as usize, a4 as usize)
        }
        NR_getppid => linux_getppid(),
        NR_fork => linux_fork(frame),
        NR_execve => linux_execve(frame, a0 as usize, a1 as usize, a2 as usize),
        NR_exit => {
            let status = a0 as i32;
            if let Some(pid) = process::current_pid() {
                process::exit(pid, status);
            }
            // exit removes us from the run-queue; finish this syscall by
            // looping into yield until somebody else runs.
            if !crate::syscall::yield_until_runnable(frame) {
                return;
            }
            Ok(0)
        }
        NR_wait4 => linux_wait4(frame, a0 as u64, a1 as usize, a2 as i32),
        NR_fcntl => linux_fcntl(a0 as usize, a1 as i32, a2 as u64),
        NR_fsync => linux_fsync(a0 as usize),
        NR_ftruncate => linux_ftruncate(a0 as usize, a1 as i64),
        NR_getdents | NR_getdents64 => linux_getdents64(a0 as usize, a1 as usize, a2 as usize),
        NR_setpgid => linux_setpgid(a0 as u64, a1 as u64),
        NR_getpgrp => Ok(process::current_pgrp().unwrap_or(0)),
        NR_setsid => linux_setsid(),
        NR_chdir => linux_chdir(a0 as usize),
        NR_getcwd => linux_getcwd(a0 as usize, a1 as usize),
        NR_rename => linux_rename(a0 as usize, a1 as usize),
        NR_mkdir => linux_mkdir(a0 as usize, a1 as u16),
        NR_rmdir => linux_rmdir(a0 as usize),
        NR_link => linux_link(a0 as usize, a1 as usize),
        NR_unlink => linux_unlink(a0 as usize),
        NR_symlink => linux_symlink(a0 as usize, a1 as usize),
        NR_readlink => linux_readlink(a0 as usize, a1 as usize, a2 as usize),
        NR_chmod => linux_chmod(a0 as usize, a1 as u16),
        NR_chown => linux_chown(a0 as usize, a1 as u32, a2 as u32),
        NR_umask => Ok(process::set_current_umask(a0 as u16) as u64),
        NR_brk => linux_brk(a0 as usize),
        NR_ioctl => linux_ioctl(a0 as usize, a1 as u64, a2 as usize),
        NR_lseek => linux_lseek(a0 as usize, a1 as i64, a2 as u32),
        NR_mmap => linux_mmap(
            a0 as usize,
            a1 as usize,
            a2 as i32,
            a3 as i32,
            a4 as i64,
            a5 as usize,
        ),
        NR_mprotect => linux_mprotect(a0 as usize, a1 as usize, a2 as i32),
        NR_munmap => linux_munmap(a0 as usize, a1 as usize),
        NR_stat => linux_stat(a0 as usize, a1 as usize),
        NR_fstat => linux_fstat(a0 as usize, a1 as usize),
        NR_lstat => linux_lstat(a0 as usize, a1 as usize),
        NR_kill => linux_kill(a0 as i64, a1 as u8),
        NR_getuid => Ok(process::current_uid() as u64),
        NR_geteuid => Ok(process::current_euid() as u64),
        NR_getgid => Ok(process::current_gid() as u64),
        NR_getegid => Ok(process::current_egid() as u64),
        NR_time => linux_time(a0 as usize),
        NR_gettimeofday => linux_gettimeofday(a0 as usize, a1 as usize),
        NR_clock_gettime => linux_clock_gettime(a0 as i32, a1 as usize),
        NR_getrandom => linux_getrandom(a0 as usize, a1 as usize, a2 as u32),
        NR_setuid => linux_setuid(a0 as u32),
        NR_setgid => linux_setgid(a0 as u32),
        NR_setresuid => linux_setresuid(a0, a1, a2),
        NR_setresgid => linux_setresgid(a0, a1, a2),
        NR_setgroups => linux_setgroups(a0 as usize, a1 as usize),
        NR_rt_sigaction => linux_rt_sigaction(a0 as usize, a1 as usize, a2 as usize),
        NR_rt_sigprocmask => {
            linux_rt_sigprocmask(a0 as i32, a1 as usize, a2 as usize, a3 as usize)
        }
        NR_rt_sigpending => linux_rt_sigpending(a0 as usize, a1 as usize),
        NR_rt_sigreturn => linux_rt_sigreturn(frame, a0 as usize),
        _ => {
            crate::println!("Unhandled Linux syscall {} (rip {:#x})", nr, frame.rip);
            Err(ENOSYS)
        }
    };

    frame.rax = match result {
        Ok(v) => v,
        Err(CONTEXT_SWITCHED) => {
            process::restore_current_fpu();
            return;
        }
        Err(e) => e as u64,
    };

    let _ = deliver_pending_signal(frame);
    process::restore_current_fpu();
}

fn deliver_pending_signal(frame: &mut SyscallInterruptFrame) -> bool {
    let Some((pid, signum, status)) = process::take_pending_signal_current() else {
        return false;
    };
    if signum == crate::signal::Signal::Tstp.number() as usize {
        let saved = saved_from_linux_frame(frame);
        process::save_syscall_frame(pid, &saved);
        process::stop_current_signal(pid, signum as u8);
        let _ = crate::syscall::yield_until_runnable(frame);
        return true;
    }
    if signum != 0 {
        if let Some(handler) = process::signal_handler(pid, signum) {
            if handler != 0 && deliver_signal_handler(frame, signum, handler).is_ok() {
                return true;
            }
        }
    }
    process::exit(pid, status);
    let _ = crate::syscall::yield_until_runnable(frame);
    true
}

fn deliver_signal_handler(
    frame: &mut SyscallInterruptFrame,
    signum: usize,
    handler: usize,
) -> Result<(), i64> {
    let saved = saved_from_linux_frame(frame);
    let frame_size = core::mem::size_of::<process::SavedSyscallFrame>();
    let new_rsp = (frame.rsp as usize).saturating_sub(frame_size) & !0xfusize;
    let out = process::write_user_buffer(new_rsp, frame_size).ok_or(EFAULT)?;
    let bytes = unsafe {
        core::slice::from_raw_parts(
            &saved as *const process::SavedSyscallFrame as *const u8,
            frame_size,
        )
    };
    out.copy_from_slice(bytes);
    frame.rip = handler as u64;
    frame.rsp = new_rsp as u64;
    frame.rdi = signum as u64;
    frame.rsi = new_rsp as u64;
    Ok(())
}

fn linux_rt_sigreturn(frame: &mut SyscallInterruptFrame, saved_ptr: usize) -> Result<u64, i64> {
    let frame_size = core::mem::size_of::<process::SavedSyscallFrame>();
    let bytes = process::read_user(saved_ptr, frame_size).ok_or(EFAULT)?;
    let mut saved = process::SavedSyscallFrame {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: 0,
        cs: 0,
        rflags: 0,
        rsp: 0,
        ss: 0,
    };
    let out = unsafe {
        core::slice::from_raw_parts_mut(
            &mut saved as *mut process::SavedSyscallFrame as *mut u8,
            frame_size,
        )
    };
    out.copy_from_slice(bytes);
    apply_linux_saved_frame(frame, &saved);
    Err(CONTEXT_SWITCHED)
}

fn saved_from_linux_frame(frame: &SyscallInterruptFrame) -> process::SavedSyscallFrame {
    process::SavedSyscallFrame {
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

fn apply_linux_saved_frame(frame: &mut SyscallInterruptFrame, saved: &process::SavedSyscallFrame) {
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

fn linux_write(
    frame: &mut SyscallInterruptFrame,
    fd: usize,
    buf: usize,
    len: usize,
) -> Result<u64, i64> {
    if let Some(vfs_fd) = process::user_vfs_fd(fd) {
        let nonblocking = process::user_fd_status_flags(fd)
            .map(|flags| flags & O_NONBLOCK != 0)
            .unwrap_or(false);
        let mut total = 0usize;
        loop {
            let ptr = buf.checked_add(total).ok_or(EFAULT)?;
            let bytes = process::read_user(ptr, len - total).ok_or(EFAULT)?;
            match fs::write(vfs_fd, bytes) {
                Ok(n) => {
                    total += n;
                    if total == len || n == 0 {
                        return Ok(total as u64);
                    }
                }
                Err(fs::vfs::VfsError::WouldBlock) => {
                    if nonblocking || total > 0 {
                        return if total > 0 {
                            Ok(total as u64)
                        } else {
                            Err(EAGAIN)
                        };
                    }
                    process::block_current(process::BlockReason::WaitIo);
                    if !crate::syscall::yield_blocked(frame) {
                        return Err(CONTEXT_SWITCHED);
                    }
                }
                Err(_) => return if total > 0 { Ok(total as u64) } else { Err(EBADF) },
            }
        }
    }

    let bytes = process::read_user(buf, len).ok_or(EFAULT)?;
    linux_write_bytes(fd, bytes).map(|written| written as u64)
}

fn linux_write_bytes(fd: usize, bytes: &[u8]) -> Result<usize, i64> {
    if let Some(vfs_fd) = process::user_vfs_fd(fd) {
        return match fs::write(vfs_fd, bytes) {
            Ok(n) => Ok(n),
            Err(fs::vfs::VfsError::WouldBlock) => Err(EAGAIN),
            Err(_) => Err(EBADF),
        };
    }
    if fd >= SOCKET_FD_BASE {
        let handle = socket_handle(fd)?;
        if bytes.is_empty() {
            crate::net::socket::with_sockets(|table| table.status_flags(handle))
                .map_err(map_socket_error)?;
            return Ok(0);
        }
        return crate::net::socket::with_sockets(|table| table.send(handle, bytes))
            .map_err(map_socket_error);
    }
    if fd == 1 || fd == 2 {
        write_console(bytes);
        return Ok(bytes.len());
    }
    Err(EBADF)
}

fn linux_writev(fd: usize, iov_ptr: usize, iovcnt: usize) -> Result<u64, i64> {
    if iovcnt > IOV_MAX {
        return Err(EINVAL);
    }
    if iovcnt == 0 {
        return Ok(0);
    }
    let iov_bytes = iovcnt.checked_mul(iovec_size()).ok_or(EINVAL)?;
    process::read_user(iov_ptr, iov_bytes).ok_or(EFAULT)?;

    let mut total = 0usize;
    for index in 0..iovcnt {
        let (base, len) = read_iovec(iov_ptr, index)?;
        if len == 0 {
            continue;
        }
        let bytes = match process::read_user(base, len) {
            Some(bytes) => bytes,
            None if total > 0 => return Ok(total as u64),
            None => return Err(EFAULT),
        };
        match linux_write_bytes(fd, bytes) {
            Ok(written) => {
                total = total.checked_add(written).ok_or(EINVAL)?;
                if written < len {
                    return Ok(total as u64);
                }
            }
            Err(_) if total > 0 => return Ok(total as u64),
            Err(err) => return Err(err),
        }
    }
    Ok(total as u64)
}

fn iovec_size() -> usize {
    core::mem::size_of::<usize>() * 2
}

fn read_iovec(iov_ptr: usize, index: usize) -> Result<(usize, usize), i64> {
    let word = core::mem::size_of::<usize>();
    let offset = index
        .checked_mul(iovec_size())
        .and_then(|offset| iov_ptr.checked_add(offset))
        .ok_or(EINVAL)?;
    let bytes = process::read_user(offset, iovec_size()).ok_or(EFAULT)?;
    let mut base = [0u8; core::mem::size_of::<usize>()];
    let mut len = [0u8; core::mem::size_of::<usize>()];
    base.copy_from_slice(&bytes[..word]);
    len.copy_from_slice(&bytes[word..word * 2]);
    Ok((usize::from_le_bytes(base), usize::from_le_bytes(len)))
}

fn write_console(bytes: &[u8]) {
    if let Ok(text) = core::str::from_utf8(bytes) {
        crate::log::write_str(text);
    } else {
        for byte in bytes {
            crate::print!("{:02x}", byte);
        }
    }
}

fn linux_read(
    frame: &mut SyscallInterruptFrame,
    fd: usize,
    buf: usize,
    len: usize,
) -> Result<u64, i64> {
    // Validate the user buffer up front; we'll re-acquire each iteration to
    // avoid holding the slice across yield points.
    {
        let _ = process::write_user_buffer(buf, len).ok_or(EFAULT)?;
    }
    let vfs_fd = match process::user_vfs_fd(fd) {
        Some(v) => v,
        None if fd >= SOCKET_FD_BASE => {
            return linux_recvfrom(frame, fd, buf, len, 0, 0, 0);
        }
        None => return Err(EBADF),
    };
    let nonblocking = process::user_fd_status_flags(fd)
        .map(|flags| flags & O_NONBLOCK != 0)
        .unwrap_or(false);

    if fs::is_kernel_tty_fd(vfs_fd) {
        loop {
            if let Some(data) = tty::try_read() {
                let out = process::write_user_buffer(buf, len).ok_or(EFAULT)?;
                let n = data.len().min(len);
                out[..n].copy_from_slice(&data[..n]);
                return Ok(n as u64);
            }
            if nonblocking {
                return Err(EAGAIN);
            }
            tty::park_current();
            if !crate::syscall::yield_blocked(frame) {
                return Err(CONTEXT_SWITCHED);
            }
        }
    }

    loop {
        let buffer = process::write_user_buffer(buf, len).ok_or(EFAULT)?;
        match fs::read(vfs_fd, buffer) {
            Ok(n) => return Ok(n as u64),
            Err(fs::vfs::VfsError::WouldBlock) => {
                if nonblocking {
                    return Err(EAGAIN);
                }
                process::block_current(process::BlockReason::WaitIo);
                if !crate::syscall::yield_blocked(frame) {
                    return Err(CONTEXT_SWITCHED);
                }
            }
            Err(_) => return Err(EBADF),
        }
    }
}

fn linux_open(path_ptr: usize, flags: i32, mode: u32) -> Result<u64, i64> {
    const O_WRONLY: i32 = 1;
    const O_RDWR: i32 = 2;
    const O_CREAT: i32 = 0o100;
    const O_EXCL: i32 = 0o200;
    const O_TRUNC: i32 = 0o1000;

    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let access = flags & 0b11;
    let write = access == O_WRONLY || access == O_RDWR;
    let read = access != O_WRONLY;
    let create = flags & O_CREAT != 0;
    let exclusive = flags & O_EXCL != 0;
    let truncate = flags & O_TRUNC != 0;
    let status_flags = (flags as u32) & (O_ACCMODE | SETTABLE_STATUS_FLAGS);
    let append = status_flags & O_APPEND != 0;
    process::user_open_options(
        &path,
        read,
        write,
        create,
        exclusive,
        truncate,
        append,
        status_flags,
        mode as u16,
    )
    .map(|fd| fd as u64)
    .map_err(map_vfs_error)
}

fn linux_fcntl(fd: usize, cmd: i32, arg: u64) -> Result<u64, i64> {
    const F_GETFD: i32 = 1;
    const F_SETFD: i32 = 2;
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;

    if fd >= SOCKET_FD_BASE {
        return linux_socket_fcntl(fd, cmd, arg);
    }

    match cmd {
        F_GETFD => process::user_fd_flags(fd)
            .map(|flags| flags as u64)
            .map_err(|_| EBADF),
        F_SETFD => {
            let flags = (arg as u32) & process::FD_CLOEXEC;
            process::user_set_fd_flags(fd, flags)
                .map(|_| 0)
                .map_err(|_| EBADF)
        }
        F_GETFL => process::user_fd_status_flags(fd)
            .map(|flags| flags as u64)
            .map_err(|_| EBADF),
        F_SETFL => {
            let current = process::user_fd_status_flags(fd).map_err(|_| EBADF)?;
            let flags = (current & !SETTABLE_STATUS_FLAGS) | ((arg as u32) & SETTABLE_STATUS_FLAGS);
            process::user_set_fd_status_flags(fd, flags)
                .map(|_| 0)
                .map_err(|_| EBADF)
        }
        _ => Err(EINVAL),
    }
}

fn linux_fsync(fd: usize) -> Result<u64, i64> {
    if process::user_vfs_fd(fd).is_some() {
        return Ok(0);
    }
    Err(EBADF)
}

fn linux_ftruncate(fd: usize, len: i64) -> Result<u64, i64> {
    if len < 0 {
        return Err(EINVAL);
    }
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    fs::truncate_fd(vfs_fd, len as usize)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_socket_fcntl(fd: usize, cmd: i32, arg: u64) -> Result<u64, i64> {
    const F_GETFD: i32 = 1;
    const F_SETFD: i32 = 2;
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;

    let handle = socket_handle(fd)?;
    crate::net::socket::with_sockets(|table| match cmd {
        F_GETFD => table.fd_flags(handle).map(|flags| flags as u64),
        F_SETFD => table
            .set_fd_flags(handle, (arg as u32) & process::FD_CLOEXEC)
            .map(|_| 0),
        F_GETFL => table.status_flags(handle).map(|flags| flags as u64),
        F_SETFL => {
            let current = table.status_flags(handle)?;
            let flags = (current & !SETTABLE_STATUS_FLAGS) | ((arg as u32) & SETTABLE_STATUS_FLAGS);
            table.set_status_flags(handle, flags).map(|_| 0)
        }
        _ => Err(crate::net::socket::SocketError::Invalid),
    })
    .map_err(map_socket_error)
}

fn linux_poll(
    frame: &mut SyscallInterruptFrame,
    fds_ptr: usize,
    nfds: usize,
    timeout_ms: i32,
) -> Result<u64, i64> {
    const POLLFD_SIZE: usize = 8;
    const MAX_POLL_FDS: usize = 64;

    if nfds > MAX_POLL_FDS {
        return Err(EINVAL);
    }
    if nfds > 0 {
        process::read_user(fds_ptr, nfds * POLLFD_SIZE).ok_or(EFAULT)?;
        process::write_user_buffer(fds_ptr, nfds * POLLFD_SIZE).ok_or(EFAULT)?;
    }

    let wait_key = if timeout_ms > 0 {
        Some(timed_wait_key(NR_poll, fds_ptr, nfds, timeout_ms as usize))
    } else {
        None
    };
    let deadline_ms = if let Some(key) = wait_key {
        let now_ms = crate::time::uptime_millis();
        Some(process::timed_wait_deadline(key, timeout_ms as u64, now_ms).ok_or(ESRCH)?)
    } else {
        None
    };
    loop {
        let ready = linux_poll_once(fds_ptr, nfds)?;
        if ready > 0 {
            if let Some(key) = wait_key {
                process::clear_timed_wait(key);
            }
            return Ok(ready as u64);
        }
        if timeout_ms == 0
            || deadline_ms.is_some_and(|deadline| crate::time::uptime_millis() >= deadline)
        {
            if let Some(key) = wait_key {
                process::clear_timed_wait(key);
            }
            return Ok(0);
        }
        block_for_io(frame, deadline_ms)?;
    }
}

fn linux_poll_once(fds_ptr: usize, nfds: usize) -> Result<usize, i64> {
    const POLLFD_SIZE: usize = 8;
    let mut ready_count = 0usize;
    for index in 0..nfds {
        let entry = fds_ptr + index * POLLFD_SIZE;
        let bytes = process::read_user(entry, POLLFD_SIZE).ok_or(EFAULT)?;
        let fd = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let events = i16::from_le_bytes([bytes[4], bytes[5]]);
        let revents = if fd < 0 {
            0
        } else {
            poll_revents(fd as usize, events)
        };
        let out = process::write_user_buffer(entry + 6, 2).ok_or(EFAULT)?;
        out.copy_from_slice(&revents.to_le_bytes());
        if revents != 0 {
            ready_count += 1;
        }
    }
    Ok(ready_count)
}

fn poll_revents(fd: usize, events: i16) -> i16 {
    let ready = if fd >= SOCKET_FD_BASE {
        let Ok(handle) = socket_handle(fd) else {
            return POLLNVAL;
        };
        match crate::net::socket::with_sockets(|table| table.poll(handle)) {
            Ok(ready) => PollSnapshot {
                read: ready.read,
                write: ready.write,
                error: ready.error,
                hangup: ready.hangup,
            },
            Err(_) => {
                return POLLNVAL;
            }
        }
    } else {
        let Some(vfs_fd) = process::user_vfs_fd(fd) else {
            return POLLNVAL;
        };
        match fs::poll(vfs_fd) {
            Ok(ready) => PollSnapshot {
                read: ready.read,
                write: ready.write,
                error: ready.error,
                hangup: ready.hangup,
            },
            Err(_) => {
                return POLLNVAL;
            }
        }
    };

    let mut revents = 0;
    if ready.read && events & POLLIN != 0 {
        revents |= POLLIN;
    }
    if ready.write && events & POLLOUT != 0 {
        revents |= POLLOUT;
    }
    if ready.error {
        revents |= POLLERR;
    }
    if ready.hangup {
        revents |= POLLHUP;
    }
    revents
}

struct PollSnapshot {
    read: bool,
    write: bool,
    error: bool,
    hangup: bool,
}

fn timed_wait_key(nr: u64, a0: usize, a1: usize, a2: usize) -> u64 {
    nr.rotate_left(48) ^ (a0 as u64).rotate_left(17) ^ (a1 as u64).rotate_left(7) ^ a2 as u64
}

fn block_for_io(frame: &mut SyscallInterruptFrame, deadline_ms: Option<u64>) -> Result<(), i64> {
    match deadline_ms {
        Some(deadline_ms) => process::block_current(process::BlockReason::WaitIoUntil(deadline_ms)),
        None => process::block_current(process::BlockReason::WaitIo),
    }
    if !crate::syscall::yield_blocked(frame) {
        return Err(CONTEXT_SWITCHED);
    }
    Ok(())
}

const SELECT_FD_SETSIZE: usize = 1024;
const SELECT_FDSET_BYTES: usize = SELECT_FD_SETSIZE / 8;

struct SelectInterest {
    read_ptr: usize,
    write_ptr: usize,
    except_ptr: usize,
    bytes: usize,
    read: [u8; SELECT_FDSET_BYTES],
    write: [u8; SELECT_FDSET_BYTES],
    except: [u8; SELECT_FDSET_BYTES],
}

struct SelectResult {
    count: usize,
    read: [u8; SELECT_FDSET_BYTES],
    write: [u8; SELECT_FDSET_BYTES],
    except: [u8; SELECT_FDSET_BYTES],
}

fn linux_select(
    frame: &mut SyscallInterruptFrame,
    nfds: i32,
    readfds: usize,
    writefds: usize,
    exceptfds: usize,
    timeout: usize,
) -> Result<u64, i64> {
    if nfds < 0 || nfds as usize > SELECT_FD_SETSIZE {
        return Err(EINVAL);
    }
    let nfds = nfds as usize;
    let interest = read_select_interest(nfds, readfds, writefds, exceptfds)?;
    let timeout_ms = read_select_timeout(timeout)?;
    let wait_key = match timeout_ms {
        Some(timeout_ms) if timeout_ms > 0 => {
            Some(timed_wait_key(NR_select, nfds, readfds, writefds))
        }
        _ => None,
    };
    let deadline_ms = if let (Some(key), Some(timeout_ms)) = (wait_key, timeout_ms) {
        let now_ms = crate::time::uptime_millis();
        Some(process::timed_wait_deadline(key, timeout_ms, now_ms).ok_or(ESRCH)?)
    } else {
        None
    };
    loop {
        let result = linux_select_once(nfds, &interest)?;
        if result.count > 0 || timeout_ms == Some(0) {
            if let Some(key) = wait_key {
                process::clear_timed_wait(key);
            }
            write_select_result(&interest, &result)?;
            return Ok(result.count as u64);
        }
        if deadline_ms.is_some_and(|deadline| crate::time::uptime_millis() >= deadline) {
            if let Some(key) = wait_key {
                process::clear_timed_wait(key);
            }
            write_select_result(&interest, &result)?;
            return Ok(0);
        }
        block_for_io(frame, deadline_ms)?;
    }
}

fn read_select_interest(
    nfds: usize,
    readfds: usize,
    writefds: usize,
    exceptfds: usize,
) -> Result<SelectInterest, i64> {
    let bytes = select_fdset_bytes(nfds);
    Ok(SelectInterest {
        read_ptr: readfds,
        write_ptr: writefds,
        except_ptr: exceptfds,
        bytes,
        read: read_fdset(readfds, bytes)?,
        write: read_fdset(writefds, bytes)?,
        except: read_fdset(exceptfds, bytes)?,
    })
}

fn read_fdset(ptr: usize, bytes: usize) -> Result<[u8; SELECT_FDSET_BYTES], i64> {
    let mut bits = [0u8; SELECT_FDSET_BYTES];
    if ptr == 0 || bytes == 0 {
        return Ok(bits);
    }
    let source = process::read_user(ptr, bytes).ok_or(EFAULT)?;
    process::write_user_buffer(ptr, bytes).ok_or(EFAULT)?;
    bits[..bytes].copy_from_slice(&source);
    Ok(bits)
}

fn read_select_timeout(timeout: usize) -> Result<Option<u64>, i64> {
    if timeout == 0 {
        return Ok(None);
    }
    let bytes = process::read_user(timeout, 16).ok_or(EFAULT)?;
    let sec = i64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    let usec = i64::from_le_bytes([
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ]);
    if sec < 0 || usec < 0 || usec >= 1_000_000 {
        return Err(EINVAL);
    }
    let millis = (sec as u64)
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add(((usec as u64) + 999) / 1000))
        .ok_or(EINVAL)?;
    Ok(Some(millis))
}

fn linux_select_once(nfds: usize, interest: &SelectInterest) -> Result<SelectResult, i64> {
    let mut result = SelectResult {
        count: 0,
        read: [0u8; SELECT_FDSET_BYTES],
        write: [0u8; SELECT_FDSET_BYTES],
        except: [0u8; SELECT_FDSET_BYTES],
    };
    for fd in 0..nfds {
        let want_read = fdset_isset(&interest.read, fd);
        let want_write = fdset_isset(&interest.write, fd);
        let want_except = fdset_isset(&interest.except, fd);
        if !want_read && !want_write && !want_except {
            continue;
        }
        let mut events = 0;
        if want_read {
            events |= POLLIN;
        }
        if want_write {
            events |= POLLOUT;
        }
        let revents = poll_revents(fd, events);
        if revents & POLLNVAL != 0 {
            return Err(EBADF);
        }
        if want_read && revents & (POLLIN | POLLERR | POLLHUP) != 0 {
            fdset_set(&mut result.read, fd);
            result.count += 1;
        }
        if want_write && revents & (POLLOUT | POLLERR | POLLHUP) != 0 {
            fdset_set(&mut result.write, fd);
            result.count += 1;
        }
        if want_except && revents & (POLLERR | POLLHUP) != 0 {
            fdset_set(&mut result.except, fd);
            result.count += 1;
        }
    }
    Ok(result)
}

fn write_select_result(interest: &SelectInterest, result: &SelectResult) -> Result<(), i64> {
    write_fdset(interest.read_ptr, interest.bytes, &result.read)?;
    write_fdset(interest.write_ptr, interest.bytes, &result.write)?;
    write_fdset(interest.except_ptr, interest.bytes, &result.except)
}

fn write_fdset(ptr: usize, bytes: usize, bits: &[u8; SELECT_FDSET_BYTES]) -> Result<(), i64> {
    if ptr == 0 || bytes == 0 {
        return Ok(());
    }
    let out = process::write_user_buffer(ptr, bytes).ok_or(EFAULT)?;
    out.copy_from_slice(&bits[..bytes]);
    Ok(())
}

fn select_fdset_bytes(nfds: usize) -> usize {
    (nfds + 7) / 8
}

fn fdset_isset(bits: &[u8; SELECT_FDSET_BYTES], fd: usize) -> bool {
    bits[fd / 8] & (1u8 << (fd % 8)) != 0
}

fn fdset_set(bits: &mut [u8; SELECT_FDSET_BYTES], fd: usize) {
    bits[fd / 8] |= 1u8 << (fd % 8);
}

fn linux_close(fd: usize) -> Result<u64, i64> {
    if fd >= SOCKET_FD_BASE {
        let handle = raw_socket_handle(fd)?;
        return process::user_close_socket_handle(handle)
            .map(|_| 0)
            .map_err(|_| EBADF);
    }
    process::user_close(fd).map(|_| 0).map_err(|_| EBADF)
}

fn linux_access(path_ptr: usize, mode: i32) -> Result<u64, i64> {
    const R_OK: i32 = 4;
    const W_OK: i32 = 2;
    const X_OK: i32 = 1;
    if mode & !(R_OK | W_OK | X_OK) != 0 {
        return Err(EINVAL);
    }
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_access(&path, mode & R_OK != 0, mode & W_OK != 0, mode & X_OK != 0)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_pipe(pipefd: usize) -> Result<u64, i64> {
    let (read_fd, write_fd) = fs::create_pipe(4096).map_err(|_| ENOMEM)?;
    process::install_pipe_fds(pipefd, read_fd, write_fd)
        .map(|_| 0)
        .map_err(|_| EFAULT)
}

fn linux_sched_yield(frame: &mut SyscallInterruptFrame) -> Result<u64, i64> {
    frame.rax = 0;
    if !crate::syscall::yield_current_process(frame) {
        return Err(CONTEXT_SWITCHED);
    }
    Ok(0)
}

fn linux_dup2(oldfd: usize, newfd: usize) -> Result<u64, i64> {
    process::user_dup2(oldfd, newfd)
        .map(|fd| fd as u64)
        .map_err(|_| EBADF)
}

fn linux_dup(fd: usize) -> Result<u64, i64> {
    process::user_dup(fd).map(|fd| fd as u64).map_err(|_| EBADF)
}

fn linux_socket(domain: i32, kind: i32, _protocol: i32) -> Result<u64, i64> {
    if domain != AF_INET {
        return Err(EINVAL);
    }
    let socket_type = match kind & 0xf {
        SOCK_STREAM => crate::net::socket::SocketType::Stream,
        SOCK_DGRAM => crate::net::socket::SocketType::Datagram,
        SOCK_RAW if _protocol == IPPROTO_ICMP => crate::net::socket::SocketType::RawIcmp,
        _ => return Err(EINVAL),
    };
    let handle = crate::net::socket::with_sockets(|table| {
        table.socket(crate::net::socket::SocketDomain::Inet, socket_type)
    })
    .ok_or(EINVAL)?;
    if process::install_socket_handle(handle).is_err() {
        let _ = crate::net::socket::with_sockets(|table| table.close(handle));
        return Err(EMFILE);
    }
    Ok((SOCKET_FD_BASE + handle) as u64)
}

fn linux_connect(fd: usize, addr: usize, addrlen: usize) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    let (ip, port) = read_sockaddr_in(addr, addrlen)?;
    crate::net::socket::with_sockets(|table| table.connect(handle, crate::net::Ipv4Addr(ip), port))
        .map(|_| 0)
        .map_err(map_socket_error)
}

fn linux_accept(fd: usize, addr: usize, addrlen_ptr: usize) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    let (accepted, peer) = crate::net::socket::with_sockets(|table| {
        let accepted = table.accept(handle)?;
        let peer = table.peer_addr(accepted)?;
        Ok((accepted, peer))
    })
    .map_err(map_socket_error)?;
    if process::install_socket_handle(accepted).is_err() {
        let _ = crate::net::socket::with_sockets(|table| table.close(accepted));
        return Err(EMFILE);
    }
    if addr != 0 {
        let peer = peer.unwrap_or(crate::net::socket::SocketAddress {
            ip: crate::net::Ipv4Addr([10, 0, 2, 2]),
            port: 8080,
        });
        write_sockaddr_in(addr, addrlen_ptr, peer.ip.0, peer.port)?;
    }
    Ok((SOCKET_FD_BASE + accepted) as u64)
}

fn linux_sendto(
    fd: usize,
    buf: usize,
    len: usize,
    _flags: i32,
    addr: usize,
    addrlen: usize,
) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    let target = if addr != 0 {
        let (ip, port) = read_sockaddr_in(addr, addrlen)?;
        Some(crate::net::socket::SocketAddress {
            ip: crate::net::Ipv4Addr(ip),
            port,
        })
    } else {
        None
    };
    let bytes = process::read_user(buf, len).ok_or(EFAULT)?;
    crate::net::socket::with_sockets(|table| table.send_to(handle, target, bytes))
        .map(|sent| sent as u64)
        .map_err(map_socket_error)
}

fn linux_recvfrom(
    frame: &mut SyscallInterruptFrame,
    fd: usize,
    buf: usize,
    len: usize,
    flags: i32,
    addr: usize,
    addrlen_ptr: usize,
) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    process::write_user_buffer(buf, len).ok_or(EFAULT)?;
    let nonblocking = crate::net::socket::with_sockets(|table| {
        table
            .status_flags(handle)
            .map(|flags| flags & O_NONBLOCK != 0)
    })
    .map_err(map_socket_error)?;
    let nonblocking = nonblocking || flags & MSG_DONTWAIT != 0;
    let timeout_ms = crate::net::socket::with_sockets(|table| table.recv_timeout_ms(handle))
        .map_err(map_socket_error)?;
    let wait_key =
        timeout_ms.map(|timeout_ms| timed_wait_key(NR_recvfrom, fd, handle, timeout_ms as usize));
    let deadline_ms = if let (Some(key), Some(timeout_ms)) = (wait_key, timeout_ms) {
        let now_ms = crate::time::uptime_millis();
        Some(process::timed_wait_deadline(key, timeout_ms, now_ms).ok_or(ESRCH)?)
    } else {
        None
    };
    loop {
        let output = process::write_user_buffer(buf, len).ok_or(EFAULT)?;
        match crate::net::socket::with_sockets(|table| table.recv_from(handle, output)) {
            Ok(recv) => {
                if let Some(key) = wait_key {
                    process::clear_timed_wait(key);
                }
                if addr != 0 {
                    let peer = recv.peer.unwrap_or(crate::net::socket::SocketAddress {
                        ip: crate::net::Ipv4Addr([10, 0, 2, 2]),
                        port: 80,
                    });
                    write_sockaddr_in(addr, addrlen_ptr, peer.ip.0, peer.port)?;
                }
                return Ok(recv.len as u64);
            }
            Err(crate::net::socket::SocketError::WouldBlock) => {
                if nonblocking {
                    return Err(EAGAIN);
                }
                if deadline_ms.is_some_and(|deadline| crate::time::uptime_millis() >= deadline) {
                    if let Some(key) = wait_key {
                        process::clear_timed_wait(key);
                    }
                    return Err(EAGAIN);
                }
                block_for_io(frame, deadline_ms)?;
            }
            Err(err) => return Err(map_socket_error(err)),
        }
    }
}

fn linux_bind(fd: usize, addr: usize, addrlen: usize) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    let (_ip, port) = read_sockaddr_in(addr, addrlen)?;
    crate::net::socket::with_sockets(|table| table.bind(handle, port))
        .map(|_| 0)
        .map_err(map_socket_error)
}

fn linux_listen(fd: usize, _backlog: i32) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    crate::net::socket::with_sockets(|table| table.listen(handle))
        .map(|_| 0)
        .map_err(map_socket_error)
}

fn linux_getsockname(fd: usize, addr: usize, addrlen_ptr: usize) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    let local = crate::net::socket::with_sockets(|table| table.local_addr(handle))
        .map_err(map_socket_error)?;
    write_sockaddr_in(addr, addrlen_ptr, local.ip.0, local.port)?;
    Ok(0)
}

fn linux_getpeername(fd: usize, addr: usize, addrlen_ptr: usize) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    let peer = crate::net::socket::with_sockets(|table| table.peer_addr(handle))
        .map_err(map_socket_error)?
        .ok_or(ENOTCONN)?;
    write_sockaddr_in(addr, addrlen_ptr, peer.ip.0, peer.port)?;
    Ok(0)
}

fn linux_shutdown(fd: usize, how: i32) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    crate::net::socket::with_sockets(|table| table.shutdown(handle, how))
        .map(|_| 0)
        .map_err(map_socket_error)?;
    Ok(0)
}

fn linux_setsockopt(
    fd: usize,
    level: i32,
    optname: i32,
    optval: usize,
    optlen: usize,
) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    match (level, optname) {
        (SOL_SOCKET, SO_REUSEADDR) => {
            let enabled = read_sockopt_int(optval, optlen)? != 0;
            crate::net::socket::with_sockets(|table| table.set_reuse_addr(handle, enabled))
                .map_err(map_socket_error)?;
        }
        (SOL_SOCKET, SO_RCVTIMEO) => {
            let timeout = read_sockopt_timeval(optval, optlen)?;
            crate::net::socket::with_sockets(|table| table.set_recv_timeout_ms(handle, timeout))
                .map_err(map_socket_error)?;
        }
        (SOL_SOCKET, SO_SNDTIMEO) => {
            let timeout = read_sockopt_timeval(optval, optlen)?;
            crate::net::socket::with_sockets(|table| table.set_send_timeout_ms(handle, timeout))
                .map_err(map_socket_error)?;
        }
        (IPPROTO_TCP, TCP_NODELAY) => {
            let enabled = read_sockopt_int(optval, optlen)? != 0;
            crate::net::socket::with_sockets(|table| table.set_tcp_nodelay(handle, enabled))
                .map_err(map_socket_error)?;
        }
        _ => return Err(EINVAL),
    }
    Ok(0)
}

fn linux_getsockopt(
    fd: usize,
    level: i32,
    optname: i32,
    optval: usize,
    optlen_ptr: usize,
) -> Result<u64, i64> {
    let handle = socket_handle(fd)?;
    match (level, optname) {
        (SOL_SOCKET, SO_REUSEADDR) => {
            let enabled = crate::net::socket::with_sockets(|table| table.reuse_addr(handle))
                .map_err(map_socket_error)?;
            write_sockopt_int(optval, optlen_ptr, enabled as i32)?;
        }
        (SOL_SOCKET, SO_ERROR) => {
            let error = crate::net::socket::with_sockets(|table| table.take_error(handle))
                .map_err(map_socket_error)?;
            write_sockopt_int(optval, optlen_ptr, error)?;
        }
        (SOL_SOCKET, SO_RCVTIMEO) => {
            let timeout = crate::net::socket::with_sockets(|table| table.recv_timeout_ms(handle))
                .map_err(map_socket_error)?;
            write_sockopt_timeval(optval, optlen_ptr, timeout)?;
        }
        (SOL_SOCKET, SO_SNDTIMEO) => {
            let timeout = crate::net::socket::with_sockets(|table| table.send_timeout_ms(handle))
                .map_err(map_socket_error)?;
            write_sockopt_timeval(optval, optlen_ptr, timeout)?;
        }
        (IPPROTO_TCP, TCP_NODELAY) => {
            let enabled = crate::net::socket::with_sockets(|table| table.tcp_nodelay(handle))
                .map_err(map_socket_error)?;
            write_sockopt_int(optval, optlen_ptr, enabled as i32)?;
        }
        _ => return Err(EINVAL),
    }
    Ok(0)
}

fn read_sockopt_int(ptr: usize, len: usize) -> Result<i32, i64> {
    if ptr == 0 || len < 4 {
        return Err(EINVAL);
    }
    let bytes = process::read_user(ptr, 4).ok_or(EFAULT)?;
    Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_sockopt_timeval(ptr: usize, len: usize) -> Result<Option<u64>, i64> {
    if ptr == 0 || len < 16 {
        return Err(EINVAL);
    }
    let bytes = process::read_user(ptr, 16).ok_or(EFAULT)?;
    let sec = i64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    let usec = i64::from_le_bytes([
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ]);
    if sec < 0 || usec < 0 || usec >= 1_000_000 {
        return Err(EINVAL);
    }
    if sec == 0 && usec == 0 {
        return Ok(None);
    }
    let millis = (sec as u64)
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add(((usec as u64) + 999) / 1000))
        .ok_or(EINVAL)?;
    Ok(Some(millis))
}

fn write_sockopt_int(ptr: usize, len_ptr: usize, value: i32) -> Result<(), i64> {
    let len = read_socklen(len_ptr)?;
    if ptr == 0 || len < 4 {
        return Err(EINVAL);
    }
    let out = process::write_user_buffer(ptr, 4).ok_or(EFAULT)?;
    out.copy_from_slice(&value.to_le_bytes());
    write_socklen(len_ptr, 4)
}

fn write_sockopt_timeval(ptr: usize, len_ptr: usize, timeout_ms: Option<u64>) -> Result<(), i64> {
    let len = read_socklen(len_ptr)?;
    if ptr == 0 || len < 16 {
        return Err(EINVAL);
    }
    let millis = timeout_ms.unwrap_or(0);
    let sec = (millis / 1000) as i64;
    let usec = ((millis % 1000) * 1000) as i64;
    let out = process::write_user_buffer(ptr, 16).ok_or(EFAULT)?;
    out[0..8].copy_from_slice(&sec.to_le_bytes());
    out[8..16].copy_from_slice(&usec.to_le_bytes());
    write_socklen(len_ptr, 16)
}

fn read_socklen(ptr: usize) -> Result<u32, i64> {
    if ptr == 0 {
        return Err(EFAULT);
    }
    let bytes = process::read_user(ptr, 4).ok_or(EFAULT)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn write_socklen(ptr: usize, len: u32) -> Result<(), i64> {
    if ptr == 0 {
        return Err(EFAULT);
    }
    let out = process::write_user_buffer(ptr, 4).ok_or(EFAULT)?;
    out.copy_from_slice(&len.to_le_bytes());
    Ok(())
}

fn socket_handle(fd: usize) -> Result<usize, i64> {
    let handle = raw_socket_handle(fd)?;
    if !process::owns_socket_handle(handle) {
        return Err(EBADF);
    }
    Ok(handle)
}

fn raw_socket_handle(fd: usize) -> Result<usize, i64> {
    if fd < SOCKET_FD_BASE {
        return Err(EBADF);
    }
    Ok(fd - SOCKET_FD_BASE)
}

fn map_socket_error(err: crate::net::socket::SocketError) -> i64 {
    match err {
        crate::net::socket::SocketError::BadFd => EBADF,
        crate::net::socket::SocketError::Invalid => EINVAL,
        crate::net::socket::SocketError::WouldBlock => EAGAIN,
        crate::net::socket::SocketError::ConnectionReset => ECONNRESET,
        crate::net::socket::SocketError::TimedOut => ETIMEDOUT,
    }
}

fn read_sockaddr_in(ptr: usize, len: usize) -> Result<([u8; 4], u16), i64> {
    if ptr == 0 || len < 16 {
        return Err(EINVAL);
    }
    let bytes = process::read_user(ptr, 16).ok_or(EFAULT)?;
    let family = u16::from_le_bytes([bytes[0], bytes[1]]) as i32;
    if family != AF_INET {
        return Err(EINVAL);
    }
    let port = u16::from_be_bytes([bytes[2], bytes[3]]);
    let ip = [bytes[4], bytes[5], bytes[6], bytes[7]];
    Ok((ip, port))
}

fn write_sockaddr_in(ptr: usize, len_ptr: usize, ip: [u8; 4], port: u16) -> Result<(), i64> {
    let out = process::write_user_buffer(ptr, 16).ok_or(EFAULT)?;
    for byte in out.iter_mut() {
        *byte = 0;
    }
    out[0..2].copy_from_slice(&(AF_INET as u16).to_le_bytes());
    out[2..4].copy_from_slice(&port.to_be_bytes());
    out[4..8].copy_from_slice(&ip);
    if len_ptr != 0 {
        let len_out = process::write_user_buffer(len_ptr, 4).ok_or(EFAULT)?;
        len_out.copy_from_slice(&(16u32).to_le_bytes());
    }
    Ok(())
}

fn linux_getppid() -> Result<u64, i64> {
    let pid = process::current_pid().ok_or(ESRCH)?;
    process::get_parent(pid)
        .map(|ppid| ppid as u64)
        .ok_or(ESRCH)
}

fn linux_fork(frame: &mut SyscallInterruptFrame) -> Result<u64, i64> {
    let parent = process::current_pid().ok_or(ESRCH)?;
    let child = process::fork(parent).ok_or(ENOMEM)?;
    // Seed the child's resumable syscall frame so the scheduler can iretq into
    // it. The child wakes up at the user instruction immediately after the
    // syscall (frame.rip is already past it), with rax = 0.
    let mut child_frame = crate::process::SavedSyscallFrame {
        rax: 0,
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
    };
    let _ = &mut child_frame;
    process::save_syscall_frame(child, &child_frame);
    Ok(child as u64)
}

fn linux_execve(
    frame: &mut SyscallInterruptFrame,
    path_ptr: usize,
    argv_ptr: usize,
    envp_ptr: usize,
) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let args = read_user_argv(argv_ptr).ok_or(EFAULT)?;
    let env = read_user_envp(envp_ptr).ok_or(EFAULT)?;
    let pid = process::current_pid().ok_or(ESRCH)?;
    let path = process::resolve_current_path(&path).map_err(map_vfs_error)?;
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let env_refs: Vec<&str> = env.iter().map(|s| s.as_str()).collect();
    let info = process::exec_for_user(pid, &path, &arg_refs, &env_refs).ok_or(ENOENT)?;
    // Patch the syscall frame so the imminent iretq lands at the new entry
    // point with the freshly built user stack. argc → rdi, argv → rsi (SysV
    // calling convention used by our _start glue).
    frame.rip = info.entry;
    frame.rsp = info.stack_top as u64;
    frame.rdi = info.argc as u64;
    frame.rsi = info.argv_ptr as u64;
    frame.rdx = info.envp_ptr as u64;
    frame.cs = 0x33; // USER_CODE
    frame.ss = 0x2B; // USER_DATA
    frame.rflags = 0x202;
    // rax (return value) will be overwritten by the iretq epilogue; set it
    // so user code that mistakenly inspects rax after execve sees 0.
    Ok(0)
}

fn linux_wait4(
    frame: &mut SyscallInterruptFrame,
    pid: u64,
    status_ptr: usize,
    options: i32,
) -> Result<u64, i64> {
    const WNOHANG: i32 = 1;
    const WUNTRACED: i32 = 2;
    let parent = process::current_pid().ok_or(ESRCH)?;
    let child = if pid == u64::MAX || pid as i64 == -1 {
        0 // wait for any child
    } else {
        pid
    };
    let include_stopped = options & WUNTRACED != 0;

    loop {
        match process::wait_any(parent, child, include_stopped) {
            Some((waited_pid, status)) => {
                if status_ptr != 0 {
                    if let Some(out) = process::write_user_buffer(status_ptr, 4) {
                        let encoded = match status {
                            process::WaitStatus::Exited(status) => (status & 0xff) << 8,
                            process::WaitStatus::Stopped(signal) => ((signal as i32) << 8) | 0x7f,
                        };
                        out.copy_from_slice(&(encoded as u32).to_le_bytes());
                    }
                }
                return Ok(waited_pid as u64);
            }
            None => {
                if !process::has_child(parent, child) {
                    return Err(ESRCH);
                }
                if options & WNOHANG != 0 {
                    return Ok(0);
                }
                process::block_current(process::BlockReason::WaitChild(child));
                if !crate::syscall::yield_blocked(frame) {
                    return Err(CONTEXT_SWITCHED);
                }
            }
        }
    }
}

fn linux_setpgid(pid: u64, pgid: u64) -> Result<u64, i64> {
    if process::set_pgid(pid, pgid) {
        Ok(0)
    } else {
        Err(ESRCH)
    }
}

fn linux_setsid() -> Result<u64, i64> {
    process::setsid_current().ok_or(EPERM)
}

fn linux_setuid(uid: u32) -> Result<u64, i64> {
    process::set_current_uid(uid).map(|_| 0).map_err(|_| EACCES)
}

fn linux_setgid(gid: u32) -> Result<u64, i64> {
    process::set_current_gid(gid).map(|_| 0).map_err(|_| EACCES)
}

fn linux_setresuid(ruid: u64, euid: u64, suid: u64) -> Result<u64, i64> {
    let is_no_change = |id: u64| id == u64::MAX || id == u32::MAX as u64;
    let ruid = if is_no_change(ruid) {
        None
    } else {
        Some(ruid as u32)
    };
    let euid = if is_no_change(euid) {
        None
    } else {
        Some(euid as u32)
    };
    let suid = if is_no_change(suid) {
        None
    } else {
        Some(suid as u32)
    };
    process::set_current_resuid(ruid, euid, suid)
        .map(|_| 0)
        .map_err(|_| EACCES)
}

fn linux_setresgid(rgid: u64, egid: u64, sgid: u64) -> Result<u64, i64> {
    let is_no_change = |id: u64| id == u64::MAX || id == u32::MAX as u64;
    let rgid = if is_no_change(rgid) {
        None
    } else {
        Some(rgid as u32)
    };
    let egid = if is_no_change(egid) {
        None
    } else {
        Some(egid as u32)
    };
    let sgid = if is_no_change(sgid) {
        None
    } else {
        Some(sgid as u32)
    };
    process::set_current_resgid(rgid, egid, sgid)
        .map(|_| 0)
        .map_err(|_| EACCES)
}

fn linux_setgroups(size: usize, list_ptr: usize) -> Result<u64, i64> {
    if size > 8 {
        return Err(EINVAL);
    }
    let mut groups = Vec::new();
    if size > 0 {
        let bytes = process::read_user(list_ptr, size * 4).ok_or(EFAULT)?;
        for chunk in bytes.chunks_exact(4) {
            groups.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
    }
    process::set_current_groups(&groups)
        .map(|_| 0)
        .map_err(|_| EACCES)
}

fn linux_chdir(path_ptr: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_chdir(&path).map(|_| 0).map_err(|_| ENOENT)
}

fn linux_getcwd(buf: usize, size: usize) -> Result<u64, i64> {
    let cwd = process::user_cwd().ok_or(ESRCH)?;
    let needed = cwd.len() + 1;
    if needed > size {
        return Err(EINVAL);
    }
    let out = process::write_user_buffer(buf, needed).ok_or(EFAULT)?;
    out[..cwd.len()].copy_from_slice(cwd.as_bytes());
    out[cwd.len()] = 0;
    Ok(buf as u64)
}

fn linux_mkdir(path_ptr: usize, mode: u16) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_mkdir_mode(&path, mode)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_rmdir(path_ptr: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_rmdir(&path).map(|_| 0).map_err(map_vfs_error)
}

fn linux_rename(old_ptr: usize, new_ptr: usize) -> Result<u64, i64> {
    let old_path = read_user_cstr(old_ptr).ok_or(EFAULT)?;
    let new_path = read_user_cstr(new_ptr).ok_or(EFAULT)?;
    process::user_rename(&old_path, &new_path)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_chmod(path_ptr: usize, mode: u16) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_chmod(&path, mode)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_chown(path_ptr: usize, uid: u32, gid: u32) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_chown(&path, uid, gid)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_unlink(path_ptr: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_unlink(&path)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_link(old_ptr: usize, new_ptr: usize) -> Result<u64, i64> {
    let old_path = read_user_cstr(old_ptr).ok_or(EFAULT)?;
    let new_path = read_user_cstr(new_ptr).ok_or(EFAULT)?;
    process::user_link(&old_path, &new_path)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_symlink(target_ptr: usize, link_ptr: usize) -> Result<u64, i64> {
    let target = read_user_cstr(target_ptr).ok_or(EFAULT)?;
    let link_path = read_user_cstr(link_ptr).ok_or(EFAULT)?;
    process::user_symlink(&target, &link_path)
        .map(|_| 0)
        .map_err(map_vfs_error)
}

fn linux_readlink(path_ptr: usize, buf: usize, len: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let path = process::resolve_current_path(&path).map_err(map_vfs_error)?;
    let target = fs::readlink(&path).map_err(map_vfs_error)?;
    let count = target.len().min(len);
    let out = process::write_user_buffer(buf, count).ok_or(EFAULT)?;
    out.copy_from_slice(&target[..count]);
    Ok(count as u64)
}

fn linux_brk(new_break: usize) -> Result<u64, i64> {
    if new_break == 0 {
        return Ok(process::current_heap_break() as u64);
    }
    match process::brk(new_break) {
        Ok(addr) => Ok(addr as u64),
        Err(_) => Ok(process::current_heap_break() as u64),
    }
}

fn linux_mmap(
    addr: usize,
    len: usize,
    prot: i32,
    flags: i32,
    fd: i64,
    offset: usize,
) -> Result<u64, i64> {
    let writable = mmap_writable(prot)?;
    if flags & MAP_FIXED != 0 || flags & MAP_PRIVATE == 0 {
        return Err(EINVAL);
    }
    let length = page_aligned_len(len)?;
    if length > USER_MMAP_END - USER_MMAP_START {
        return Err(ENOMEM);
    }

    let file_bytes = if flags & MAP_ANONYMOUS != 0 {
        None
    } else {
        if fd < 0 || offset % FRAME_SIZE != 0 {
            return Err(EINVAL);
        }
        Some(read_mmap_file(fd as usize, length, offset)?)
    };

    let mapped = process::mmap_anonymous(addr, length, true).map_err(|_| ENOMEM)?;
    if let Some(bytes) = file_bytes {
        if !bytes.is_empty() {
            let out = process::write_user_buffer(mapped, bytes.len()).ok_or(EFAULT)?;
            out.copy_from_slice(&bytes);
        }
    }
    if !writable {
        process::mprotect(mapped, length, false).map_err(|_| ENOMEM)?;
    }
    Ok(mapped as u64)
}

fn linux_mprotect(addr: usize, len: usize, prot: i32) -> Result<u64, i64> {
    if addr % FRAME_SIZE != 0 {
        return Err(EINVAL);
    }
    let writable = mmap_writable(prot)?;
    let length = page_aligned_len(len)?;
    process::mprotect(addr, length, writable)
        .map(|_| 0)
        .map_err(|_| EINVAL)
}

fn linux_munmap(addr: usize, len: usize) -> Result<u64, i64> {
    if addr % FRAME_SIZE != 0 {
        return Err(EINVAL);
    }
    let length = page_aligned_len(len)?;
    process::munmap(addr, length).map(|_| 0).map_err(|_| EINVAL)
}

fn mmap_writable(prot: i32) -> Result<bool, i64> {
    if prot == 0 || prot & !(PROT_READ | PROT_WRITE | PROT_EXEC) != 0 {
        return Err(EINVAL);
    }
    Ok(prot & PROT_WRITE != 0)
}

fn page_aligned_len(len: usize) -> Result<usize, i64> {
    if len == 0 {
        return Err(EINVAL);
    }
    len.checked_add(FRAME_SIZE - 1)
        .map(|value| value & !(FRAME_SIZE - 1))
        .ok_or(ENOMEM)
}

fn read_mmap_file(fd: usize, len: usize, offset: usize) -> Result<Vec<u8>, i64> {
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    let dup = fs::duplicate_fd(vfs_fd).map_err(map_vfs_error)?;
    let result = read_mmap_file_from_vfs(dup, len, offset);
    let _ = fs::close(dup);
    result
}

fn read_mmap_file_from_vfs(vfs_fd: usize, len: usize, offset: usize) -> Result<Vec<u8>, i64> {
    fs::lseek(vfs_fd, offset as isize, 0).map_err(map_vfs_error)?;
    let mut bytes = Vec::new();
    bytes.resize(len, 0);
    let mut read = 0usize;
    while read < len {
        match fs::read(vfs_fd, &mut bytes[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(fs::vfs::VfsError::WouldBlock) => break,
            Err(err) => return Err(map_vfs_error(err)),
        }
    }
    bytes.truncate(read);
    Ok(bytes)
}

fn linux_time(tloc: usize) -> Result<u64, i64> {
    let now = crate::time::unix_time();
    if tloc != 0 {
        let out = process::write_user_buffer(tloc, 8).ok_or(EFAULT)?;
        out.copy_from_slice(&(now as i64).to_le_bytes());
    }
    Ok(now)
}

fn linux_gettimeofday(tv: usize, tz: usize) -> Result<u64, i64> {
    let now = crate::time::unix_time() as i64;
    if tv != 0 {
        let out = process::write_user_buffer(tv, 16).ok_or(EFAULT)?;
        out[0..8].copy_from_slice(&now.to_le_bytes());
        out[8..16].copy_from_slice(&0i64.to_le_bytes());
    }
    if tz != 0 {
        let out = process::write_user_buffer(tz, 8).ok_or(EFAULT)?;
        out.fill(0);
    }
    Ok(0)
}

fn linux_clock_gettime(clock_id: i32, tp: usize) -> Result<u64, i64> {
    const CLOCK_REALTIME: i32 = 0;
    const CLOCK_MONOTONIC: i32 = 1;
    let hz = crate::config::PIT_TARGET_HZ as u64;
    let (sec, nsec) = match clock_id {
        CLOCK_REALTIME => {
            let ticks = crate::time::monotonic_ticks();
            let sec = crate::time::unix_time();
            let nsec = (ticks % hz).saturating_mul(1_000_000_000) / hz;
            (sec, nsec)
        }
        CLOCK_MONOTONIC => {
            let millis = crate::time::uptime_millis();
            (millis / 1000, (millis % 1000) * 1_000_000)
        }
        _ => return Err(EINVAL),
    };
    let out = process::write_user_buffer(tp, 16).ok_or(EFAULT)?;
    out[0..8].copy_from_slice(&(sec as i64).to_le_bytes());
    out[8..16].copy_from_slice(&(nsec as i64).to_le_bytes());
    Ok(0)
}

fn linux_getrandom(buf: usize, len: usize, flags: u32) -> Result<u64, i64> {
    if flags & !(GRND_NONBLOCK | GRND_RANDOM) != 0 {
        return Err(EINVAL);
    }
    let output = process::write_user_buffer(buf, len).ok_or(EFAULT)?;
    crate::entropy::fill_random(output);
    Ok(len as u64)
}

fn linux_nanosleep(req: usize, rem: usize) -> Result<u64, i64> {
    let bytes = process::read_user(req, 16).ok_or(EFAULT)?;
    let sec = i64::from_le_bytes(bytes[0..8].try_into().map_err(|_| EFAULT)?);
    let nsec = i64::from_le_bytes(bytes[8..16].try_into().map_err(|_| EFAULT)?);
    if sec < 0 || !(0..1_000_000_000).contains(&nsec) {
        return Err(EINVAL);
    }
    let hz = crate::config::PIT_TARGET_HZ as u64;
    let sec_ticks = (sec as u64).saturating_mul(hz);
    let nsec_ticks = ((nsec as u64).saturating_mul(hz) + 999_999_999) / 1_000_000_000;
    let target = crate::time::monotonic_ticks().saturating_add(sec_ticks + nsec_ticks);
    while crate::time::monotonic_ticks() < target {
        core::hint::spin_loop();
    }
    if rem != 0 {
        let out = process::write_user_buffer(rem, 16).ok_or(EFAULT)?;
        out.fill(0);
    }
    Ok(0)
}

fn linux_ioctl(fd: usize, request: u64, argp: usize) -> Result<u64, i64> {
    const TCGETS: u64 = 0x5401;
    const TCSETS: u64 = 0x5402;
    const TCSETSW: u64 = 0x5403;
    const TCSETSF: u64 = 0x5404;
    const TIOCSCTTY: u64 = 0x540e;
    const TIOCGPGRP: u64 = 0x540f;
    const TIOCSPGRP: u64 = 0x5410;
    const TIOCNOTTY: u64 = 0x5422;
    const TIOCGWINSZ: u64 = 0x5413;
    const TIOCSWINSZ: u64 = 0x5414;
    const TIOCGPTN: u64 = 0x8004_5430;
    const TIOCSPTLCK: u64 = 0x4004_5431;

    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    let is_tty = fs::is_tty_fd(vfs_fd);
    match request {
        TIOCGPTN => {
            let number = fs::pty_number(vfs_fd).ok_or(ENOTTY)?;
            let out = process::write_user_buffer(argp, 4).ok_or(EFAULT)?;
            out.copy_from_slice(&(number as u32).to_le_bytes());
            Ok(0)
        }
        TIOCSPTLCK => {
            if fs::pty_number(vfs_fd).is_none() {
                return Err(ENOTTY);
            }
            let input = process::read_user(argp, 4).ok_or(EFAULT)?;
            let locked = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) != 0;
            fs::set_pty_locked(vfs_fd, locked).map_err(|_| ENOTTY)?;
            Ok(0)
        }
        TIOCSCTTY => {
            let number = fs::pty_number(vfs_fd).ok_or(ENOTTY)?;
            if !process::set_current_controlling_pty(number) {
                return Err(ESRCH);
            }
            Ok(0)
        }
        TIOCNOTTY => {
            if !is_tty {
                return Err(ENOTTY);
            }
            if !process::detach_current_controlling_tty() {
                return Err(ESRCH);
            }
            Ok(0)
        }
        TIOCGPGRP => {
            if !is_tty {
                return Err(ENOTTY);
            }
            let out = process::write_user_buffer(argp, 4).ok_or(EFAULT)?;
            out.copy_from_slice(&(tty::foreground_pgrp() as u32).to_le_bytes());
            Ok(0)
        }
        TIOCSPGRP => {
            if !is_tty {
                return Err(ENOTTY);
            }
            let input = process::read_user(argp, 4).ok_or(EFAULT)?;
            let pgrp = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) as u64;
            tty::set_foreground_pgrp(pgrp);
            Ok(0)
        }
        TCGETS => {
            if !is_tty {
                return Err(ENOTTY);
            }
            let termios = tty::termios_bytes();
            let out = process::write_user_buffer(argp, tty::TERMIOS_SIZE).ok_or(EFAULT)?;
            out.copy_from_slice(&termios);
            Ok(0)
        }
        TCSETS | TCSETSW | TCSETSF => {
            if !is_tty {
                return Err(ENOTTY);
            }
            let input = process::read_user(argp, tty::TERMIOS_SIZE).ok_or(EFAULT)?;
            tty::set_termios_bytes(input).map_err(|_| EINVAL)?;
            Ok(0)
        }
        TIOCGWINSZ => {
            if !is_tty {
                return Err(ENOTTY);
            }
            let out = process::write_user_buffer(argp, 8).ok_or(EFAULT)?;
            out[0..2].copy_from_slice(&24u16.to_le_bytes());
            out[2..4].copy_from_slice(&80u16.to_le_bytes());
            out[4..8].fill(0);
            Ok(0)
        }
        TIOCSWINSZ => {
            if !is_tty {
                return Err(ENOTTY);
            }
            process::read_user(argp, 8).ok_or(EFAULT)?;
            Ok(0)
        }
        _ => Ok(0),
    }
}

fn linux_lseek(fd: usize, offset: i64, whence: u32) -> Result<u64, i64> {
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    match fs::lseek(vfs_fd, offset as isize, whence) {
        Ok(pos) => Ok(pos as u64),
        Err(_) => Err(EINVAL),
    }
}

fn linux_getdents64(fd: usize, dirp: usize, count: usize) -> Result<u64, i64> {
    const DT_CHR: u8 = 2;
    const DT_DIR: u8 = 4;
    const DT_REG: u8 = 8;
    const DT_LNK: u8 = 10;
    const DIRENT64_HEADER: usize = 19;

    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    let (entries, mut index) = fs::directory_entries(vfs_fd).map_err(map_vfs_error)?;
    let out = process::write_user_buffer(dirp, count).ok_or(EFAULT)?;
    let mut written = 0usize;
    while index < entries.len() {
        let entry = &entries[index];
        let reclen = align8(DIRENT64_HEADER + entry.name.len() + 1);
        if reclen > count {
            return Err(EINVAL);
        }
        if written + reclen > count {
            break;
        }
        let slot = &mut out[written..written + reclen];
        slot.fill(0);
        let ino = (index as u64) + 1;
        let next_off = (index as i64) + 1;
        slot[0..8].copy_from_slice(&ino.to_le_bytes());
        slot[8..16].copy_from_slice(&next_off.to_le_bytes());
        slot[16..18].copy_from_slice(&(reclen as u16).to_le_bytes());
        slot[18] = match entry.kind {
            fs::vfs::NodeKind::Directory => DT_DIR,
            fs::vfs::NodeKind::File => DT_REG,
            fs::vfs::NodeKind::Symlink => DT_LNK,
            fs::vfs::NodeKind::Device(_) => DT_CHR,
        };
        let name = entry.name.as_bytes();
        slot[DIRENT64_HEADER..DIRENT64_HEADER + name.len()].copy_from_slice(name);
        written += reclen;
        index += 1;
    }
    fs::set_directory_offset(vfs_fd, index).map_err(map_vfs_error)?;
    Ok(written as u64)
}

fn linux_stat(path_ptr: usize, buf_ptr: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let path = process::resolve_current_path(&path).map_err(|_| ENOENT)?;
    let meta = fs::stat(&path).map_err(|_| ENOENT)?;
    write_linux_stat(
        buf_ptr,
        meta.owner,
        meta.group,
        meta.size,
        meta.mode as u32,
        meta.nlink,
    )
}

fn linux_lstat(path_ptr: usize, buf_ptr: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let path = process::resolve_current_path(&path).map_err(|_| ENOENT)?;
    let meta = fs::lstat(&path).map_err(|_| ENOENT)?;
    write_linux_stat(
        buf_ptr,
        meta.owner,
        meta.group,
        meta.size,
        meta.mode as u32,
        meta.nlink,
    )
}

fn linux_fstat(fd: usize, buf_ptr: usize) -> Result<u64, i64> {
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    let meta = fs::fstat(vfs_fd).map_err(|_| EBADF)?;
    write_linux_stat(
        buf_ptr,
        meta.owner,
        meta.group,
        meta.size,
        meta.mode as u32,
        meta.nlink,
    )
}

fn write_linux_stat(
    buf_ptr: usize,
    owner: u32,
    group: u32,
    size: u64,
    mode: u32,
    nlink: u64,
) -> Result<u64, i64> {
    // struct stat is ~144 bytes on x86_64; we only fill the fields userland
    // typically reads (size, mode), zeroing the rest.
    const STAT_SIZE: usize = 144;
    let out = process::write_user_buffer(buf_ptr, STAT_SIZE).ok_or(EFAULT)?;
    for byte in out.iter_mut() {
        *byte = 0;
    }
    out[16..24].copy_from_slice(&nlink.to_le_bytes()); // st_nlink
    out[24..28].copy_from_slice(&mode.to_le_bytes()); // st_mode
    out[28..32].copy_from_slice(&owner.to_le_bytes()); // st_uid
    out[32..36].copy_from_slice(&group.to_le_bytes()); // st_gid
    out[48..56].copy_from_slice(&size.to_le_bytes()); // st_size
    Ok(0)
}

fn linux_kill(pid: i64, sig: u8) -> Result<u64, i64> {
    let signal = crate::signal::Signal::from_number(sig).ok_or(EINVAL)?;
    let delivered = if pid < 0 {
        crate::signal::send_pgrp((-pid) as u64, signal)
    } else if pid == 0 {
        process::current_pgrp()
            .map(|pgrp| crate::signal::send_pgrp(pgrp, signal))
            .unwrap_or(false)
    } else {
        crate::signal::send(pid as u64, signal)
    };
    if delivered { Ok(0) } else { Err(ESRCH) }
}

fn linux_rt_sigaction(signum: usize, act: usize, oldact: usize) -> Result<u64, i64> {
    if signum == 0 || signum >= 32 {
        return Err(EINVAL);
    }
    let pid = process::current_pid().ok_or(ESRCH)?;
    let new_handler = if act == 0 {
        None
    } else {
        let bytes = process::read_user(act, core::mem::size_of::<usize>()).ok_or(EFAULT)?;
        let mut raw = [0u8; core::mem::size_of::<usize>()];
        raw.copy_from_slice(bytes);
        Some(usize::from_le_bytes(raw))
    };
    let old = if let Some(handler) = new_handler {
        process::set_signal_handler(pid, signum, handler).ok_or(EINVAL)?
    } else {
        process::get_signal_handler(pid, signum).ok_or(EINVAL)?
    };
    if oldact != 0 {
        let out =
            process::write_user_buffer(oldact, core::mem::size_of::<usize>()).ok_or(EFAULT)?;
        out.copy_from_slice(&old.to_le_bytes());
    }
    Ok(0)
}

fn linux_rt_sigprocmask(
    how: i32,
    set_ptr: usize,
    oldset_ptr: usize,
    sigset_size: usize,
) -> Result<u64, i64> {
    const SIG_BLOCK: i32 = 0;
    const SIG_UNBLOCK: i32 = 1;
    const SIG_SETMASK: i32 = 2;

    if sigset_size != core::mem::size_of::<u64>() {
        return Err(EINVAL);
    }
    let old = process::current_signal_mask().ok_or(ESRCH)?;
    if oldset_ptr != 0 {
        let out = process::write_user_buffer(oldset_ptr, core::mem::size_of::<u64>())
            .ok_or(EFAULT)?;
        out.copy_from_slice(&old.to_le_bytes());
    }
    if set_ptr == 0 {
        return Ok(0);
    }
    let bytes = process::read_user(set_ptr, core::mem::size_of::<u64>()).ok_or(EFAULT)?;
    let mut raw = [0u8; core::mem::size_of::<u64>()];
    raw.copy_from_slice(bytes);
    let set = u64::from_le_bytes(raw);
    match how {
        SIG_BLOCK => process::set_current_signal_mask(old | set),
        SIG_UNBLOCK => process::set_current_signal_mask(old & !set),
        SIG_SETMASK => process::set_current_signal_mask(set),
        _ => return Err(EINVAL),
    }
    .ok_or(ESRCH)?;
    Ok(0)
}

fn linux_rt_sigpending(set_ptr: usize, sigset_size: usize) -> Result<u64, i64> {
    if sigset_size != core::mem::size_of::<u64>() {
        return Err(EINVAL);
    }
    let pending = process::current_pending_signals().ok_or(ESRCH)?;
    let out = process::write_user_buffer(set_ptr, core::mem::size_of::<u64>()).ok_or(EFAULT)?;
    out.copy_from_slice(&pending.to_le_bytes());
    Ok(0)
}

fn read_user_cstr(addr: usize) -> Option<alloc::string::String> {
    use alloc::string::String;
    let mut buf: Vec<u8> = Vec::new();
    let mut offset = 0usize;
    while offset < 4096 {
        let slice = process::read_user(addr + offset, 1)?;
        if slice[0] == 0 {
            break;
        }
        buf.push(slice[0]);
        offset += 1;
    }
    String::from_utf8(buf).ok()
}

fn map_vfs_error(err: fs::vfs::VfsError) -> i64 {
    match err {
        fs::vfs::VfsError::PermissionDenied => EACCES,
        fs::vfs::VfsError::NotFound => ENOENT,
        fs::vfs::VfsError::AlreadyExists => EEXIST,
        fs::vfs::VfsError::BadFd => EBADF,
        _ => EINVAL,
    }
}

fn align8(value: usize) -> usize {
    (value + 7) & !7
}

fn read_user_argv(argv_ptr: usize) -> Option<Vec<alloc::string::String>> {
    let mut args = Vec::new();
    let mut index = 0usize;
    loop {
        let ptr_bytes = process::read_user(argv_ptr + index * 8, 8)?;
        let arg_ptr = u64::from_le_bytes(ptr_bytes.try_into().ok()?) as usize;
        if arg_ptr == 0 {
            break;
        }
        args.push(read_user_cstr(arg_ptr)?);
        index += 1;
        if index > 32 {
            break;
        }
    }
    Some(args)
}

fn read_user_envp(envp_ptr: usize) -> Option<Vec<alloc::string::String>> {
    let mut env = Vec::new();
    if envp_ptr == 0 {
        return Some(env);
    }
    let mut index = 0usize;
    loop {
        let ptr_bytes = process::read_user(envp_ptr + index * 8, 8)?;
        let entry_ptr = u64::from_le_bytes(ptr_bytes.try_into().ok()?) as usize;
        if entry_ptr == 0 {
            break;
        }
        env.push(read_user_cstr(entry_ptr)?);
        index += 1;
        if index > 16 {
            break;
        }
    }
    Some(env)
}

pub fn self_test() {
    crate::println!(
        "Linux syscall ABI ready: fork={}, execve={}, pipe={}.",
        NR_fork,
        NR_execve,
        NR_pipe
    );
}
