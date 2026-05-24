use core::{fmt, str};

use crate::userspace::{ProcessState, UserProcess};

pub const SYS_WRITE: u64 = 1;
pub const SYS_READ: u64 = 2;
pub const SYS_EXIT: u64 = 3;
pub const SYS_YIELD: u64 = 4;
pub const SYS_SLEEP: u64 = 5;
pub const SYS_GETPID: u64 = 6;

const EBADF: i64 = -9;
const EFAULT: i64 = -14;
const ENOSYS: i64 = -38;

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
    crate::println!("Syscall ABI initialized on int 0x80.");
}

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
        _ => Err(SyscallError(ENOSYS)),
    }
}

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

fn sys_read(_process: &mut UserProcess, fd: usize, _ptr: usize, _len: usize) -> SyscallResult {
    if fd != 0 {
        return Err(SyscallError(EBADF));
    }

    Ok(0)
}

