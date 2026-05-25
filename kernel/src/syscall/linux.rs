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

use crate::{fs, process, syscall::SyscallInterruptFrame, tty};

pub const NR_read: u64 = 0;
pub const NR_write: u64 = 1;
pub const NR_open: u64 = 2;
pub const NR_close: u64 = 3;
pub const NR_stat: u64 = 4;
pub const NR_fstat: u64 = 5;
pub const NR_lseek: u64 = 8;
pub const NR_brk: u64 = 12;
pub const NR_rt_sigaction: u64 = 13;
pub const NR_rt_sigreturn: u64 = 15;
pub const NR_ioctl: u64 = 16;
pub const NR_pipe: u64 = 22;
pub const NR_dup2: u64 = 33;
pub const NR_getpid: u64 = 39;
pub const NR_fork: u64 = 57;
pub const NR_execve: u64 = 59;
pub const NR_exit: u64 = 60;
pub const NR_wait4: u64 = 61;
pub const NR_kill: u64 = 62;
pub const NR_getcwd: u64 = 79;
pub const NR_chdir: u64 = 80;
pub const NR_getuid: u64 = 102;
pub const NR_setuid: u64 = 105;
pub const NR_setpgid: u64 = 109;
pub const NR_getppid: u64 = 110;

const ESRCH: i64 = -3;
const EBADF: i64 = -9;
const ENOMEM: i64 = -12;
const EFAULT: i64 = -14;
const ENOENT: i64 = -2;
const ENOSYS: i64 = -38;
const EINVAL: i64 = -22;
const CONTEXT_SWITCHED: i64 = i64::MIN;

/// Entry from `linux_syscall_entry` assembly. The frame holds saved user
/// registers and the SYSV-style return state for `iretq`.
#[unsafe(no_mangle)]
pub extern "C" fn linux_syscall_dispatch_frame(frame: &mut SyscallInterruptFrame) {
    let nr = frame.rax;
    let a0 = frame.rdi;
    let a1 = frame.rsi;
    let a2 = frame.rdx;
    let _a3 = frame.r10;
    let _a4 = frame.r8;
    let _a5 = frame.r9;

    let result: Result<u64, i64> = match nr {
        NR_write => linux_write(a0 as usize, a1 as usize, a2 as usize),
        NR_read => linux_read(frame, a0 as usize, a1 as usize, a2 as usize),
        NR_open => linux_open(a0 as usize, a1 as i32, a2 as u32),
        NR_close => linux_close(a0 as usize),
        NR_pipe => linux_pipe(a0 as usize),
        NR_dup2 => linux_dup2(a0 as usize, a1 as usize),
        NR_getpid => Ok(process::current_pid().unwrap_or(0)),
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
        NR_setpgid => linux_setpgid(a0 as u64, a1 as u64),
        NR_chdir => linux_chdir(a0 as usize),
        NR_getcwd => linux_getcwd(a0 as usize, a1 as usize),
        NR_brk => linux_brk(a0 as usize),
        NR_ioctl => linux_ioctl(a0 as usize, a1 as u64, a2 as usize),
        NR_lseek => linux_lseek(a0 as usize, a1 as i64, a2 as u32),
        NR_stat => linux_stat(a0 as usize, a1 as usize),
        NR_fstat => linux_fstat(a0 as usize, a1 as usize),
        NR_kill => linux_kill(a0 as u64, a1 as u8),
        NR_getuid => Ok(process::current_uid() as u64),
        NR_setuid => Ok(0),
        NR_rt_sigaction | NR_rt_sigreturn => Ok(0),
        _ => {
            crate::println!("Unhandled Linux syscall {} (rip {:#x})", nr, frame.rip);
            Err(ENOSYS)
        }
    };

    frame.rax = match result {
        Ok(v) => v,
        Err(CONTEXT_SWITCHED) => return,
        Err(e) => e as u64,
    };
}

fn linux_write(fd: usize, buf: usize, len: usize) -> Result<u64, i64> {
    let bytes = process::read_user(buf, len).ok_or(EFAULT)?;
    if let Some(vfs_fd) = process::user_vfs_fd(fd) {
        return fs::write(vfs_fd, bytes)
            .map(|n| n as u64)
            .map_err(|_| EBADF);
    }
    if fd == 1 || fd == 2 {
        write_console(bytes);
        return Ok(len as u64);
    }
    Err(EBADF)
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
        None => return Err(EBADF),
    };

    if fs::is_tty_fd(vfs_fd) {
        loop {
            if let Some(data) = tty::try_read_line() {
                let out = process::write_user_buffer(buf, len).ok_or(EFAULT)?;
                let n = data.len().min(len);
                out[..n].copy_from_slice(&data[..n]);
                return Ok(n as u64);
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
                process::block_current(process::BlockReason::WaitIo);
                if !crate::syscall::yield_blocked(frame) {
                    return Err(CONTEXT_SWITCHED);
                }
            }
            Err(_) => return Err(EBADF),
        }
    }
}

fn linux_open(path_ptr: usize, _flags: i32, _mode: u32) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_open(&path)
        .map(|fd| fd as u64)
        .map_err(|_| ENOENT)
}

fn linux_close(fd: usize) -> Result<u64, i64> {
    process::user_close(fd).map(|_| 0).map_err(|_| EBADF)
}

fn linux_pipe(pipefd: usize) -> Result<u64, i64> {
    let (read_fd, write_fd) = fs::create_pipe(4096).map_err(|_| ENOMEM)?;
    process::install_pipe_fds(pipefd, read_fd, write_fd)
        .map(|_| 0)
        .map_err(|_| EFAULT)
}

fn linux_dup2(oldfd: usize, newfd: usize) -> Result<u64, i64> {
    process::user_dup2(oldfd, newfd)
        .map(|fd| fd as u64)
        .map_err(|_| EBADF)
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
    _envp_ptr: usize,
) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let args = read_user_argv(argv_ptr).ok_or(EFAULT)?;
    let pid = process::current_pid().ok_or(ESRCH)?;
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let info = process::exec_for_user(pid, &path, &arg_refs).ok_or(ENOENT)?;
    // Patch the syscall frame so the imminent iretq lands at the new entry
    // point with the freshly built user stack. argc → rdi, argv → rsi (SysV
    // calling convention used by our _start glue).
    frame.rip = info.entry;
    frame.rsp = info.stack_top as u64;
    frame.rdi = info.argc as u64;
    frame.rsi = info.argv_ptr as u64;
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
    _options: i32,
) -> Result<u64, i64> {
    let parent = process::current_pid().ok_or(ESRCH)?;
    let child = if pid == u64::MAX || pid as i64 == -1 {
        0 // wait for any child
    } else {
        pid
    };

    loop {
        match process::wait_any(parent, child) {
            Some((reaped_pid, status)) => {
                if status_ptr != 0 {
                    if let Some(out) = process::write_user_buffer(status_ptr, 4) {
                        let encoded = (status & 0xff) << 8;
                        out.copy_from_slice(&(encoded as u32).to_le_bytes());
                    }
                }
                return Ok(reaped_pid as u64);
            }
            None => {
                if !process::has_child(parent, child) {
                    return Err(ESRCH);
                }
                process::block_current(process::BlockReason::WaitChild(child));
                if !crate::syscall::yield_blocked(frame) {
                    return Err(CONTEXT_SWITCHED);
                }
            }
        }
    }
}

fn linux_setpgid(_pid: u64, _pgid: u64) -> Result<u64, i64> {
    Ok(0)
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

fn linux_brk(new_break: usize) -> Result<u64, i64> {
    if new_break == 0 {
        return Ok(process::current_heap_break() as u64);
    }
    match process::brk(new_break) {
        Ok(addr) => Ok(addr as u64),
        Err(_) => Ok(process::current_heap_break() as u64),
    }
}

fn linux_ioctl(fd: usize, _request: u64, _argp: usize) -> Result<u64, i64> {
    if process::user_vfs_fd(fd).is_none() {
        return Err(EBADF);
    }
    // Minimal stub: ack TIOCGPGRP/TIOCSPGRP-style requests so utilities
    // attempting basic terminal queries don't fail. Real signal/pgrp work
    // lands in phase D.
    Ok(0)
}

fn linux_lseek(fd: usize, offset: i64, whence: u32) -> Result<u64, i64> {
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    match fs::lseek(vfs_fd, offset as isize, whence) {
        Ok(pos) => Ok(pos as u64),
        Err(_) => Err(EINVAL),
    }
}

fn linux_stat(path_ptr: usize, buf_ptr: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let meta = fs::stat(&path).map_err(|_| ENOENT)?;
    write_linux_stat(buf_ptr, meta.size, meta.mode as u32)
}

fn linux_fstat(fd: usize, buf_ptr: usize) -> Result<u64, i64> {
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    let meta = fs::fstat(vfs_fd).map_err(|_| EBADF)?;
    write_linux_stat(buf_ptr, meta.size, meta.mode as u32)
}

fn write_linux_stat(buf_ptr: usize, size: u64, mode: u32) -> Result<u64, i64> {
    // struct stat is ~144 bytes on x86_64; we only fill the fields userland
    // typically reads (size, mode), zeroing the rest.
    const STAT_SIZE: usize = 144;
    let out = process::write_user_buffer(buf_ptr, STAT_SIZE).ok_or(EFAULT)?;
    for byte in out.iter_mut() {
        *byte = 0;
    }
    out[24..28].copy_from_slice(&mode.to_le_bytes()); // st_mode
    out[48..56].copy_from_slice(&size.to_le_bytes()); // st_size
    Ok(0)
}

fn linux_kill(pid: u64, sig: u8) -> Result<u64, i64> {
    let signal = crate::signal::Signal::from_number(sig).ok_or(EINVAL)?;
    if crate::signal::send(pid, signal) {
        Ok(0)
    } else {
        Err(ESRCH)
    }
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

pub fn self_test() {
    crate::println!(
        "Linux syscall ABI ready: fork={}, execve={}, pipe={}.",
        NR_fork,
        NR_execve,
        NR_pipe
    );
}
