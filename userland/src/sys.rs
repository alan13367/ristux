//! Raw Linux x86_64 syscall wrappers.
//!
//! Uses the `syscall` instruction. All wrappers return `isize` where negative
//! values are `-errno`.

use core::arch::asm;

pub const NR_READ: usize = 0;
pub const NR_WRITE: usize = 1;
pub const NR_OPEN: usize = 2;
pub const NR_CLOSE: usize = 3;
pub const NR_STAT: usize = 4;
pub const NR_FSTAT: usize = 5;
pub const NR_LSEEK: usize = 8;
pub const NR_POLL: usize = 7;
pub const NR_BRK: usize = 12;
pub const NR_IOCTL: usize = 16;
pub const NR_PIPE: usize = 22;
pub const NR_SCHED_YIELD: usize = 24;
pub const NR_DUP2: usize = 33;
pub const NR_GETPID: usize = 39;
pub const NR_SOCKET: usize = 41;
pub const NR_CONNECT: usize = 42;
pub const NR_ACCEPT: usize = 43;
pub const NR_SENDTO: usize = 44;
pub const NR_RECVFROM: usize = 45;
pub const NR_SHUTDOWN: usize = 48;
pub const NR_BIND: usize = 49;
pub const NR_LISTEN: usize = 50;
pub const NR_GETSOCKNAME: usize = 51;
pub const NR_GETPEERNAME: usize = 52;
pub const NR_SETSOCKOPT: usize = 54;
pub const NR_GETSOCKOPT: usize = 55;
pub const NR_FORK: usize = 57;
pub const NR_EXECVE: usize = 59;
pub const NR_EXIT: usize = 60;
pub const NR_WAIT4: usize = 61;
pub const NR_KILL: usize = 62;
pub const NR_FCNTL: usize = 72;
pub const NR_GETCWD: usize = 79;
pub const NR_GETDENTS64: usize = 217;
pub const NR_CHDIR: usize = 80;
pub const NR_GETUID: usize = 102;
pub const NR_GETGID: usize = 104;
pub const NR_SETUID: usize = 105;
pub const NR_SETGID: usize = 106;
pub const NR_GETEUID: usize = 107;
pub const NR_SETPGID: usize = 109;
pub const NR_GETPPID: usize = 110;
pub const NR_SETSID: usize = 112;
pub const NR_SETGROUPS: usize = 116;
pub const NR_SETRESUID: usize = 117;
pub const NR_RT_SIGACTION: usize = 13;
pub const NR_RT_SIGRETURN: usize = 15;

pub const AF_INET: i32 = 2;
pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;
pub const SOL_SOCKET: i32 = 1;
pub const SO_REUSEADDR: i32 = 2;
pub const SO_ERROR: i32 = 4;
pub const SO_RCVTIMEO: i32 = 20;
pub const SO_SNDTIMEO: i32 = 21;
pub const IPPROTO_TCP: i32 = 6;
pub const TCP_NODELAY: i32 = 1;
pub const O_NONBLOCK: i32 = 0o4000;
pub const F_GETFL: i32 = 3;
pub const F_SETFL: i32 = 4;
pub const MSG_DONTWAIT: i32 = 0x40;
pub const POLLIN: i16 = 0x001;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddrIn {
    pub family: u16,
    pub port: u16,
    pub addr: [u8; 4],
    pub zero: [u8; 8],
}

impl SockAddrIn {
    pub const fn new(addr: [u8; 4], port: u16) -> Self {
        Self {
            family: AF_INET as u16,
            port: port.to_be(),
            addr,
            zero: [0; 8],
        }
    }
}

#[inline]
pub unsafe fn syscall0(nr: usize) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline]
pub unsafe fn syscall1(nr: usize, a: usize) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline]
pub unsafe fn syscall2(nr: usize, a: usize, b: usize) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a,
            in("rsi") b,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline]
pub unsafe fn syscall3(nr: usize, a: usize, b: usize, c: usize) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline]
pub unsafe fn syscall4(nr: usize, a: usize, b: usize, c: usize, d: usize) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("r10") d,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline]
pub unsafe fn syscall5(
    nr: usize,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("r10") d,
            in("r8") e,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline]
pub unsafe fn syscall6(
    nr: usize,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("r10") d,
            in("r8") e,
            in("r9") f,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline]
pub fn write(fd: i32, buf: &[u8]) -> isize {
    unsafe { syscall3(NR_WRITE, fd as usize, buf.as_ptr() as usize, buf.len()) }
}

#[inline]
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe { syscall3(NR_READ, fd as usize, buf.as_mut_ptr() as usize, buf.len()) }
}

#[inline]
pub fn open(path: *const u8, flags: i32, mode: u32) -> isize {
    unsafe { syscall3(NR_OPEN, path as usize, flags as usize, mode as usize) }
}

#[inline]
pub fn close(fd: i32) -> isize {
    unsafe { syscall1(NR_CLOSE, fd as usize) }
}

#[inline]
pub fn pipe(pipefd: *mut i32) -> isize {
    unsafe { syscall1(NR_PIPE, pipefd as usize) }
}

#[inline]
pub fn socket(domain: i32, kind: i32, protocol: i32) -> isize {
    unsafe { syscall3(NR_SOCKET, domain as usize, kind as usize, protocol as usize) }
}

#[inline]
pub fn connect(fd: i32, addr: &SockAddrIn) -> isize {
    unsafe {
        syscall3(
            NR_CONNECT,
            fd as usize,
            addr as *const SockAddrIn as usize,
            core::mem::size_of::<SockAddrIn>(),
        )
    }
}

#[inline]
pub fn sendto(fd: i32, buf: &[u8], flags: i32) -> isize {
    unsafe {
        syscall6(
            NR_SENDTO,
            fd as usize,
            buf.as_ptr() as usize,
            buf.len(),
            flags as usize,
            0,
            0,
        )
    }
}

#[inline]
pub fn recvfrom(fd: i32, buf: &mut [u8], flags: i32) -> isize {
    unsafe {
        syscall6(
            NR_RECVFROM,
            fd as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
            flags as usize,
            0,
            0,
        )
    }
}

#[inline]
pub fn poll(fds: *mut PollFd, nfds: usize, timeout_ms: i32) -> isize {
    unsafe { syscall3(NR_POLL, fds as usize, nfds, timeout_ms as usize) }
}

#[inline]
pub fn bind(fd: i32, addr: &SockAddrIn) -> isize {
    unsafe {
        syscall3(
            NR_BIND,
            fd as usize,
            addr as *const SockAddrIn as usize,
            core::mem::size_of::<SockAddrIn>(),
        )
    }
}

#[inline]
pub fn listen(fd: i32, backlog: i32) -> isize {
    unsafe { syscall2(NR_LISTEN, fd as usize, backlog as usize) }
}

#[inline]
pub fn accept(fd: i32, addr: *mut SockAddrIn, addrlen: *mut u32) -> isize {
    unsafe { syscall3(NR_ACCEPT, fd as usize, addr as usize, addrlen as usize) }
}

#[inline]
pub fn getsockname(fd: i32, addr: *mut SockAddrIn, addrlen: *mut u32) -> isize {
    unsafe { syscall3(NR_GETSOCKNAME, fd as usize, addr as usize, addrlen as usize) }
}

#[inline]
pub fn setsockopt(fd: i32, level: i32, optname: i32, optval: *const u8, optlen: u32) -> isize {
    unsafe {
        syscall5(
            NR_SETSOCKOPT,
            fd as usize,
            level as usize,
            optname as usize,
            optval as usize,
            optlen as usize,
        )
    }
}

#[inline]
pub fn getsockopt(fd: i32, level: i32, optname: i32, optval: *mut u8, optlen: *mut u32) -> isize {
    unsafe {
        syscall5(
            NR_GETSOCKOPT,
            fd as usize,
            level as usize,
            optname as usize,
            optval as usize,
            optlen as usize,
        )
    }
}

#[inline]
pub fn sched_yield() -> isize {
    unsafe { syscall0(NR_SCHED_YIELD) }
}

#[inline]
pub fn dup2(oldfd: i32, newfd: i32) -> isize {
    unsafe { syscall2(NR_DUP2, oldfd as usize, newfd as usize) }
}

#[inline]
pub fn fork() -> isize {
    unsafe { syscall0(NR_FORK) }
}

#[inline]
pub fn execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> isize {
    unsafe { syscall3(NR_EXECVE, path as usize, argv as usize, envp as usize) }
}

#[inline]
pub fn wait4(pid: isize, status: *mut i32, options: i32, rusage: usize) -> isize {
    unsafe {
        syscall4(
            NR_WAIT4,
            pid as usize,
            status as usize,
            options as usize,
            rusage,
        )
    }
}

#[inline]
pub fn fcntl(fd: i32, cmd: i32, arg: isize) -> isize {
    unsafe { syscall3(NR_FCNTL, fd as usize, cmd as usize, arg as usize) }
}

#[inline]
pub fn chdir(path: *const u8) -> isize {
    unsafe { syscall1(NR_CHDIR, path as usize) }
}

#[inline]
pub fn getcwd(buf: *mut u8, len: usize) -> isize {
    unsafe { syscall2(NR_GETCWD, buf as usize, len) }
}

#[inline]
pub fn getdents64(fd: i32, buf: &mut [u8]) -> isize {
    unsafe { syscall3(NR_GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len()) }
}

#[inline]
pub fn brk(addr: usize) -> isize {
    unsafe { syscall1(NR_BRK, addr) }
}

#[inline]
pub fn getpid() -> isize {
    unsafe { syscall0(NR_GETPID) }
}

#[inline]
pub fn getpgrp() -> isize {
    unsafe { syscall0(111) }
}

#[inline]
pub fn setsid() -> isize {
    unsafe { syscall0(NR_SETSID) }
}

#[inline]
pub fn setpgid(pid: usize, pgid: usize) -> isize {
    unsafe { syscall2(NR_SETPGID, pid, pgid) }
}

#[inline]
pub fn ioctl(fd: i32, request: usize, argp: usize) -> isize {
    unsafe { syscall3(NR_IOCTL, fd as usize, request, argp) }
}

#[inline]
pub fn kill(pid: isize, sig: u8) -> isize {
    unsafe { syscall2(NR_KILL, pid as usize, sig as usize) }
}

#[inline]
pub fn getuid() -> isize {
    unsafe { syscall0(NR_GETUID) }
}

#[inline]
pub fn geteuid() -> isize {
    unsafe { syscall0(NR_GETEUID) }
}

#[inline]
pub fn getgid() -> isize {
    unsafe { syscall0(NR_GETGID) }
}

#[inline]
pub fn setuid(uid: u32) -> isize {
    unsafe { syscall1(NR_SETUID, uid as usize) }
}

#[inline]
pub fn setgid(gid: u32) -> isize {
    unsafe { syscall1(NR_SETGID, gid as usize) }
}

#[inline]
pub fn setresuid(ruid: u32, euid: u32, suid: u32) -> isize {
    unsafe { syscall3(NR_SETRESUID, ruid as usize, euid as usize, suid as usize) }
}

#[inline]
pub fn setgroups(groups: &[u32]) -> isize {
    unsafe { syscall2(NR_SETGROUPS, groups.len(), groups.as_ptr() as usize) }
}

#[inline]
pub fn rt_sigaction(signum: usize, handler: usize) -> isize {
    unsafe { syscall3(NR_RT_SIGACTION, signum, &handler as *const usize as usize, 0) }
}

#[inline]
pub fn rt_sigreturn(frame: usize) -> isize {
    unsafe { syscall1(NR_RT_SIGRETURN, frame) }
}

#[inline]
pub fn exit(status: i32) -> ! {
    unsafe { syscall1(NR_EXIT, status as usize) };
    loop {}
}
