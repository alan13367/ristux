//! Linux x86_64 syscall numbers and dispatch.

use crate::process;

pub const NR_read: u64 = 0;
pub const NR_write: u64 = 1;
pub const NR_open: u64 = 2;
pub const NR_close: u64 = 3;
pub const NR_pipe: u64 = 22;
pub const NR_dup2: u64 = 33;
pub const NR_getpid: u64 = 39;
pub const NR_fork: u64 = 57;
pub const NR_execve: u64 = 59;
pub const NR_exit: u64 = 60;
pub const NR_wait4: u64 = 61;
pub const NR_getppid: u64 = 110;
pub const NR_setpgid: u64 = 109;

const ESRCH: i64 = -3;
const EBADF: i64 = -9;
const ENOMEM: i64 = -12;
const EFAULT: i64 = -14;
const ENOENT: i64 = -2;
const ENOSYS: i64 = -38;

#[repr(C)]
struct LinuxSyscallFrame {
    nr: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
}

#[unsafe(no_mangle)]
pub extern "C" fn linux_syscall_dispatch(frame_ptr: *mut u64) -> i64 {
    if frame_ptr.is_null() {
        return ENOSYS;
    }
    let frame = unsafe { &*(frame_ptr as *const LinuxSyscallFrame) };
    let result = match frame.nr {
        NR_write => linux_write(frame.arg0 as usize, frame.arg1 as usize, frame.arg2 as usize),
        NR_read => linux_read(frame.arg0 as usize, frame.arg1 as usize, frame.arg2 as usize),
        NR_open => linux_open(frame.arg0 as usize, frame.arg1 as i32, frame.arg2 as u32),
        NR_close => linux_close(frame.arg0 as usize),
        NR_pipe => linux_pipe(frame.arg0 as usize),
        NR_dup2 => linux_dup2(frame.arg0 as usize, frame.arg1 as usize),
        NR_getpid => Ok(process::current_pid().unwrap_or(0)),
        NR_getppid => linux_getppid(),
        NR_fork => linux_fork(),
        NR_execve => linux_execve(frame.arg0 as usize, frame.arg1 as usize, frame.arg2 as usize),
        NR_exit => {
            let status = frame.arg0 as i32;
            if let Some(pid) = process::current_pid() {
                process::exit(pid, status);
            }
            Ok(0)
        }
        NR_wait4 => linux_wait4(frame.arg0 as u64, frame.arg1 as usize),
        NR_setpgid => linux_setpgid(frame.arg0 as u64, frame.arg1 as u64),
        _ => Err(ENOSYS),
    };
    match result {
        Ok(value) => value as i64,
        Err(err) => err,
    }
}

fn linux_write(fd: usize, buf: usize, len: usize) -> Result<u64, i64> {
    let bytes = process::read_user(buf, len).ok_or(EFAULT)?;
    if fd == 1 || fd == 2 {
        if let Ok(text) = core::str::from_utf8(bytes) {
            crate::print!("{}", text);
        } else {
            for byte in bytes {
                crate::print!("{:02x}", byte);
            }
        }
        return Ok(len as u64);
    }
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    crate::fs::write(vfs_fd, bytes)
        .map(|n| n as u64)
        .map_err(|_| EBADF)
}

fn linux_read(fd: usize, buf: usize, len: usize) -> Result<u64, i64> {
    let output = process::write_user_buffer(buf, len).ok_or(EFAULT)?;
    if fd == 0 {
        return Ok(0);
    }
    let vfs_fd = process::user_vfs_fd(fd).ok_or(EBADF)?;
    crate::fs::read(vfs_fd, output)
        .map(|n| n as u64)
        .map_err(|_| EBADF)
}

fn linux_open(path_ptr: usize, _flags: i32, _mode: u32) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    process::user_open(path)
        .map(|fd| fd as u64)
        .map_err(|_| ENOENT)
}

fn linux_close(fd: usize) -> Result<u64, i64> {
    process::user_close(fd).map(|_| 0).map_err(|_| EBADF)
}

fn linux_pipe(pipefd: usize) -> Result<u64, i64> {
    let (read_fd, write_fd) = crate::fs::create_pipe(4096).map_err(|_| ENOMEM)?;
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

fn linux_fork() -> Result<u64, i64> {
    let parent = process::current_pid().ok_or(ESRCH)?;
    process::fork(parent)
        .map(|child| child as u64)
        .ok_or(ENOMEM)
}

fn linux_execve(path_ptr: usize, argv_ptr: usize, _envp_ptr: usize) -> Result<u64, i64> {
    let path = read_user_cstr(path_ptr).ok_or(EFAULT)?;
    let args = read_user_argv(argv_ptr).ok_or(EFAULT)?;
    let pid = process::current_pid().ok_or(ESRCH)?;
    if process::exec_with_args(pid, path, &args) {
        Ok(0)
    } else {
        Err(ENOENT)
    }
}

fn linux_wait4(pid: u64, status_ptr: usize) -> Result<u64, i64> {
    let parent = process::current_pid().ok_or(ESRCH)?;
    let child = if pid == u64::MAX { 0 } else { pid };
    if let Some(status) = process::wait(parent, child) {
        if status_ptr != 0 {
            if let Some(out) = process::write_user_buffer(status_ptr, 4) {
                out.copy_from_slice(&(status << 8).to_le_bytes()[..4]);
            }
        }
        Ok(child as u64)
    } else {
        Err(ESRCH)
    }
}

fn linux_setpgid(_pid: u64, _pgid: u64) -> Result<u64, i64> {
    Ok(0)
}

fn read_user_cstr(addr: usize) -> Option<&'static str> {
    let mut len = 0usize;
    while len < 256 {
        let slice = process::read_user(addr + len, 1)?;
        if slice[0] == 0 {
            break;
        }
        len += 1;
    }
    let bytes = process::read_user(addr, len)?;
    core::str::from_utf8(bytes).ok()
}

fn read_user_argv(argv_ptr: usize) -> Option<alloc::vec::Vec<&'static str>> {
    use alloc::vec::Vec;
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
        if index > 16 {
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
