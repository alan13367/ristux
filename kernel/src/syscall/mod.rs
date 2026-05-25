use core::{fmt, str};

use crate::userspace::{self, ProcessState, UserProcess};

pub const SYS_WRITE: u64 = 1;
pub const SYS_READ: u64 = 2;
pub const SYS_EXIT: u64 = 3;
pub const SYS_YIELD: u64 = 4;
pub const SYS_SLEEP: u64 = 5;
pub const SYS_GETPID: u64 = 6;
pub const SYS_TIME: u64 = 7;

const EBADF: i64 = -9;
const EFAULT: i64 = -14;
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
    crate::println!("Syscall ABI initialized on int 0x80.");
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
            frame.rax = EBADF as u64;
        }
        SYS_EXIT => {
            let status = frame.rdi as i32;
            let exit = userspace::finish_active_exit(status);
            crate::println!(
                "Ring 3 ELF process {} exited with status {} from rip {:#x}; unmapped {} page(s).",
                exit.name,
                status,
                frame.rip,
                exit.unmapped_pages
            );
            crate::println!("Ring 3 ELF init passed.");
            crate::arch::x86_64::instructions::enable_interrupts();
            crate::halt_loop();
        }
        SYS_YIELD | SYS_SLEEP => {
            frame.rax = 0;
        }
        SYS_GETPID => {
            frame.rax = userspace::active_user_pid();
        }
        SYS_TIME => {
            frame.rax = crate::time::unix_time();
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
    if fd != 1 && fd != 2 {
        return Err(SyscallError(EBADF));
    }

    let bytes = userspace::active_user_read(ptr, len).ok_or(SyscallError(EFAULT))?;
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
