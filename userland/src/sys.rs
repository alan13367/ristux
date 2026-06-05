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
pub const NR_LSTAT: usize = 6;
pub const NR_POLL: usize = 7;
pub const NR_LSEEK: usize = 8;
pub const NR_MMAP: usize = 9;
pub const NR_MPROTECT: usize = 10;
pub const NR_MUNMAP: usize = 11;
pub const NR_BRK: usize = 12;
pub const NR_RT_SIGACTION: usize = 13;
pub const NR_RT_SIGPROCMASK: usize = 14;
pub const NR_RT_SIGRETURN: usize = 15;
pub const NR_IOCTL: usize = 16;
pub const NR_PREAD64: usize = 17;
pub const NR_PWRITE64: usize = 18;
pub const NR_READV: usize = 19;
pub const NR_WRITEV: usize = 20;
pub const NR_ACCESS: usize = 21;
pub const NR_PIPE: usize = 22;
pub const NR_SCHED_YIELD: usize = 24;
pub const NR_MADVISE: usize = 28;
pub const NR_NANOSLEEP: usize = 35;
pub const NR_DUP: usize = 32;
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
pub const NR_UNAME: usize = 63;
pub const NR_FCNTL: usize = 72;
pub const NR_FSYNC: usize = 74;
pub const NR_TRUNCATE: usize = 76;
pub const NR_FTRUNCATE: usize = 77;
pub const NR_GETCWD: usize = 79;
pub const NR_CHDIR: usize = 80;
pub const NR_RENAME: usize = 82;
pub const NR_MKDIR: usize = 83;
pub const NR_RMDIR: usize = 84;
pub const NR_LINK: usize = 86;
pub const NR_UNLINK: usize = 87;
pub const NR_SYMLINK: usize = 88;
pub const NR_READLINK: usize = 89;
pub const NR_CHMOD: usize = 90;
pub const NR_FCHMOD: usize = 91;
pub const NR_CHOWN: usize = 92;
pub const NR_FCHOWN: usize = 93;
pub const NR_UMASK: usize = 95;
pub const NR_GETTIMEOFDAY: usize = 96;
pub const NR_GETRLIMIT: usize = 97;
pub const NR_GETRUSAGE: usize = 98;
pub const NR_SYSINFO: usize = 99;
pub const NR_TIMES: usize = 100;
pub const NR_STATFS: usize = 137;
pub const NR_FSTATFS: usize = 138;
pub const NR_ARCH_PRCTL: usize = 158;
pub const NR_SETRLIMIT: usize = 160;
pub const NR_MOUNT: usize = 165;
pub const NR_REBOOT: usize = 169;
pub const NR_TIME: usize = 201;
pub const NR_FUTEX: usize = 202;
pub const NR_SCHED_GETAFFINITY: usize = 204;
pub const NR_GETDENTS64: usize = 217;
pub const NR_SET_TID_ADDRESS: usize = 218;
pub const NR_FADVISE64: usize = 221;
pub const NR_CLOCK_GETTIME: usize = 228;
pub const NR_CLOCK_GETRES: usize = 229;
pub const NR_OPENAT: usize = 257;
pub const NR_MKDIRAT: usize = 258;
pub const NR_FCHOWNAT: usize = 260;
pub const NR_NEWFSTATAT: usize = 262;
pub const NR_UNLINKAT: usize = 263;
pub const NR_RENAMEAT: usize = 264;
pub const NR_LINKAT: usize = 265;
pub const NR_SYMLINKAT: usize = 266;
pub const NR_READLINKAT: usize = 267;
pub const NR_FCHMODAT: usize = 268;
pub const NR_FACCESSAT: usize = 269;
pub const NR_SET_ROBUST_LIST: usize = 273;
pub const NR_GET_ROBUST_LIST: usize = 274;
pub const NR_UTIMENSAT: usize = 280;
pub const NR_DUP3: usize = 292;
pub const NR_PIPE2: usize = 293;
pub const NR_PRLIMIT64: usize = 302;
pub const NR_GETCPU: usize = 309;
pub const NR_RENAMEAT2: usize = 316;
pub const NR_GETRANDOM: usize = 318;
pub const NR_COPY_FILE_RANGE: usize = 326;
pub const NR_STATX: usize = 332;
pub const NR_RISTUX_THREAD_CREATE: usize = 451;
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
pub const NR_SETHOSTNAME: usize = 170;
pub const NR_GETTID: usize = 186;

pub const LINUX_REBOOT_MAGIC1: usize = 0xfee1_dead;
pub const LINUX_REBOOT_MAGIC2: usize = 672_274_793;
pub const LINUX_REBOOT_CMD_RESTART: usize = 0x0123_4567;
pub const LINUX_REBOOT_CMD_HALT: usize = 0xcdef_0123;
pub const LINUX_REBOOT_CMD_POWER_OFF: usize = 0x4321_fedc;

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
pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;
pub const O_NONBLOCK: i32 = 0o4000;
pub const O_CLOEXEC: i32 = 0o2000000;
pub const F_OK: i32 = 0;
pub const X_OK: i32 = 1;
pub const W_OK: i32 = 2;
pub const R_OK: i32 = 4;
pub const AT_FDCWD: i32 = -100;
pub const AT_SYMLINK_NOFOLLOW: i32 = 0x100;
pub const AT_EACCESS: i32 = 0x200;
pub const AT_REMOVEDIR: i32 = 0x200;
pub const AT_SYMLINK_FOLLOW: i32 = 0x400;
pub const AT_NO_AUTOMOUNT: i32 = 0x800;
pub const AT_EMPTY_PATH: i32 = 0x1000;
pub const STATX_BASIC_STATS: u32 = 0x0000_07ff;
pub const RLIMIT_CORE: i32 = 4;
pub const RLIMIT_NOFILE: i32 = 7;
pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;
pub const RUSAGE_THREAD: i32 = 1;
pub const PROT_READ: i32 = 0x1;
pub const PROT_WRITE: i32 = 0x2;
pub const MAP_PRIVATE: i32 = 0x02;
pub const MAP_ANONYMOUS: i32 = 0x20;
pub const MADV_NORMAL: i32 = 0;
pub const MADV_RANDOM: i32 = 1;
pub const MADV_SEQUENTIAL: i32 = 2;
pub const MADV_WILLNEED: i32 = 3;
pub const MADV_DONTNEED: i32 = 4;
pub const POSIX_FADV_NORMAL: i32 = 0;
pub const POSIX_FADV_DONTNEED: i32 = 4;
pub const F_GETFD: i32 = 1;
pub const F_SETFD: i32 = 2;
pub const F_GETFL: i32 = 3;
pub const F_SETFL: i32 = 4;
pub const FD_CLOEXEC: i32 = 1;
pub const MSG_DONTWAIT: i32 = 0x40;
pub const POLLIN: i16 = 0x001;
pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;
pub const FUTEX_WAIT: i32 = 0;
pub const FUTEX_WAKE: i32 = 1;
pub const FUTEX_PRIVATE_FLAG: i32 = 128;
pub const SIG_BLOCK: i32 = 0;
pub const SIG_UNBLOCK: i32 = 1;
pub const SIG_SETMASK: i32 = 2;
pub const ARCH_SET_GS: i32 = 0x1001;
pub const ARCH_SET_FS: i32 = 0x1002;
pub const ARCH_GET_FS: i32 = 0x1003;
pub const ARCH_GET_GS: i32 = 0x1004;
pub const RENAME_NOREPLACE: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Iovec {
    pub base: *mut u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rlimit {
    pub cur: u64,
    pub max: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rusage {
    pub bytes: [u8; 144],
}

impl Default for Rusage {
    fn default() -> Self {
        Self { bytes: [0; 144] }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Tms {
    pub utime: i64,
    pub stime: i64,
    pub cutime: i64,
    pub cstime: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SysInfo {
    pub bytes: [u8; 112],
}

impl Default for SysInfo {
    fn default() -> Self {
        Self { bytes: [0; 112] }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddrIn {
    pub family: u16,
    pub port: u16,
    pub addr: [u8; 4],
    pub zero: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StatFs {
    pub f_type: u64,
    pub f_bsize: u64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    pub f_files: u64,
    pub f_ffree: u64,
    pub f_fsid: [i32; 2],
    pub f_namelen: u64,
    pub f_frsize: u64,
    pub f_flags: u64,
    pub f_spare: [u64; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UtsName {
    pub bytes: [u8; 65 * 6],
}

impl Default for UtsName {
    fn default() -> Self {
        Self { bytes: [0; 65 * 6] }
    }
}

impl UtsName {
    pub fn field(&self, index: usize) -> &[u8] {
        let start = index.saturating_mul(65);
        if start >= self.bytes.len() {
            return b"";
        }
        let field = &self.bytes[start..start + 65];
        let len = field.iter().position(|byte| *byte == 0).unwrap_or(65);
        &field[..len]
    }
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
pub unsafe fn syscall5(nr: usize, a: usize, b: usize, c: usize, d: usize, e: usize) -> isize {
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
pub fn openat(dirfd: i32, path: *const u8, flags: i32, mode: u32) -> isize {
    unsafe {
        syscall4(
            NR_OPENAT,
            dirfd as usize,
            path as usize,
            flags as usize,
            mode as usize,
        )
    }
}

#[inline]
pub fn close(fd: i32) -> isize {
    unsafe { syscall1(NR_CLOSE, fd as usize) }
}

#[inline]
pub fn stat(path: *const u8, buf: *mut u8) -> isize {
    unsafe { syscall2(NR_STAT, path as usize, buf as usize) }
}

#[inline]
pub fn fstat(fd: i32, buf: *mut u8) -> isize {
    unsafe { syscall2(NR_FSTAT, fd as usize, buf as usize) }
}

#[inline]
pub fn lstat(path: *const u8, buf: *mut u8) -> isize {
    unsafe { syscall2(NR_LSTAT, path as usize, buf as usize) }
}

#[inline]
pub fn newfstatat(dirfd: i32, path: *const u8, buf: *mut u8, flags: i32) -> isize {
    unsafe {
        syscall4(
            NR_NEWFSTATAT,
            dirfd as usize,
            path as usize,
            buf as usize,
            flags as usize,
        )
    }
}

#[inline]
pub fn statx(dirfd: i32, path: *const u8, flags: i32, mask: u32, buf: *mut u8) -> isize {
    unsafe {
        syscall5(
            NR_STATX,
            dirfd as usize,
            path as usize,
            flags as usize,
            mask as usize,
            buf as usize,
        )
    }
}

#[inline]
pub fn lseek(fd: i32, offset: usize, whence: usize) -> isize {
    unsafe { syscall3(NR_LSEEK, fd as usize, offset, whence) }
}

#[inline]
pub fn pread64(fd: i32, buf: &mut [u8], offset: i64) -> isize {
    unsafe {
        syscall4(
            NR_PREAD64,
            fd as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
            offset as usize,
        )
    }
}

#[inline]
pub fn pwrite64(fd: i32, buf: &[u8], offset: i64) -> isize {
    unsafe {
        syscall4(
            NR_PWRITE64,
            fd as usize,
            buf.as_ptr() as usize,
            buf.len(),
            offset as usize,
        )
    }
}

#[inline]
pub fn copy_file_range(
    fd_in: i32,
    off_in: *mut i64,
    fd_out: i32,
    off_out: *mut i64,
    len: usize,
    flags: u32,
) -> isize {
    unsafe {
        syscall6(
            NR_COPY_FILE_RANGE,
            fd_in as usize,
            off_in as usize,
            fd_out as usize,
            off_out as usize,
            len,
            flags as usize,
        )
    }
}

#[inline]
pub fn readv(fd: i32, iovecs: &mut [Iovec]) -> isize {
    unsafe {
        syscall3(
            NR_READV,
            fd as usize,
            iovecs.as_mut_ptr() as usize,
            iovecs.len(),
        )
    }
}

#[inline]
pub fn writev(fd: i32, iovecs: &[Iovec]) -> isize {
    unsafe {
        syscall3(
            NR_WRITEV,
            fd as usize,
            iovecs.as_ptr() as usize,
            iovecs.len(),
        )
    }
}

#[inline]
pub fn access(path: *const u8, mode: i32) -> isize {
    unsafe { syscall2(NR_ACCESS, path as usize, mode as usize) }
}

#[inline]
pub fn faccessat(dirfd: i32, path: *const u8, mode: i32, flags: i32) -> isize {
    unsafe {
        syscall4(
            NR_FACCESSAT,
            dirfd as usize,
            path as usize,
            mode as usize,
            flags as usize,
        )
    }
}

#[inline]
pub fn mmap(addr: usize, len: usize, prot: i32, flags: i32, fd: i32, offset: usize) -> isize {
    unsafe {
        syscall6(
            NR_MMAP,
            addr,
            len,
            prot as usize,
            flags as usize,
            fd as usize,
            offset,
        )
    }
}

#[inline]
pub fn mprotect(addr: usize, len: usize, prot: i32) -> isize {
    unsafe { syscall3(NR_MPROTECT, addr, len, prot as usize) }
}

#[inline]
pub fn madvise(addr: usize, len: usize, advice: i32) -> isize {
    unsafe { syscall3(NR_MADVISE, addr, len, advice as usize) }
}

#[inline]
pub fn posix_fadvise(fd: i32, offset: i64, len: usize, advice: i32) -> isize {
    unsafe {
        syscall4(
            NR_FADVISE64,
            fd as usize,
            offset as usize,
            len,
            advice as usize,
        )
    }
}

#[inline]
pub fn munmap(addr: usize, len: usize) -> isize {
    unsafe { syscall2(NR_MUNMAP, addr, len) }
}

#[inline]
pub fn dup(fd: i32) -> isize {
    unsafe { syscall1(NR_DUP, fd as usize) }
}

#[inline]
pub fn dup3(oldfd: i32, newfd: i32, flags: i32) -> isize {
    unsafe { syscall3(NR_DUP3, oldfd as usize, newfd as usize, flags as usize) }
}

#[inline]
pub fn pipe(pipefd: *mut i32) -> isize {
    unsafe { syscall1(NR_PIPE, pipefd as usize) }
}

#[inline]
pub fn pipe2(pipefd: *mut i32, flags: i32) -> isize {
    unsafe { syscall2(NR_PIPE2, pipefd as usize, flags as usize) }
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
pub fn sched_getaffinity(pid: isize, mask: &mut [u8]) -> isize {
    unsafe {
        syscall3(
            NR_SCHED_GETAFFINITY,
            pid as usize,
            mask.len(),
            mask.as_mut_ptr() as usize,
        )
    }
}

#[inline]
pub fn getcpu(cpu: *mut u32, node: *mut u32) -> isize {
    unsafe { syscall3(NR_GETCPU, cpu as usize, node as usize, 0) }
}

#[inline]
pub fn time() -> isize {
    unsafe { syscall1(NR_TIME, 0) }
}

#[inline]
pub fn gettimeofday(tv: *mut Timeval) -> isize {
    unsafe { syscall2(NR_GETTIMEOFDAY, tv as usize, 0) }
}

#[inline]
pub fn getrusage(who: i32, usage: *mut Rusage) -> isize {
    unsafe { syscall2(NR_GETRUSAGE, who as usize, usage as usize) }
}

#[inline]
pub fn sysinfo(info: *mut SysInfo) -> isize {
    unsafe { syscall1(NR_SYSINFO, info as usize) }
}

#[inline]
pub fn times(buf: *mut Tms) -> isize {
    unsafe { syscall1(NR_TIMES, buf as usize) }
}

#[inline]
pub fn clock_gettime(clock_id: i32, tp: *mut Timespec) -> isize {
    unsafe { syscall2(NR_CLOCK_GETTIME, clock_id as usize, tp as usize) }
}

#[inline]
pub fn clock_getres(clock_id: i32, tp: *mut Timespec) -> isize {
    unsafe { syscall2(NR_CLOCK_GETRES, clock_id as usize, tp as usize) }
}

#[inline]
pub fn nanosleep(req: &Timespec) -> isize {
    unsafe { syscall2(NR_NANOSLEEP, req as *const Timespec as usize, 0) }
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
pub fn fsync(fd: i32) -> isize {
    unsafe { syscall1(NR_FSYNC, fd as usize) }
}

#[inline]
pub fn truncate(path: *const u8, len: i64) -> isize {
    unsafe { syscall2(NR_TRUNCATE, path as usize, len as usize) }
}

#[inline]
pub fn ftruncate(fd: i32, len: i64) -> isize {
    unsafe { syscall2(NR_FTRUNCATE, fd as usize, len as usize) }
}

#[inline]
pub fn getrlimit(resource: i32, rlim: *mut Rlimit) -> isize {
    unsafe { syscall2(NR_GETRLIMIT, resource as usize, rlim as usize) }
}

#[inline]
pub fn setrlimit(resource: i32, rlim: *const Rlimit) -> isize {
    unsafe { syscall2(NR_SETRLIMIT, resource as usize, rlim as usize) }
}

#[inline]
pub fn prlimit64(
    pid: isize,
    resource: i32,
    new_rlim: *const Rlimit,
    old_rlim: *mut Rlimit,
) -> isize {
    unsafe {
        syscall4(
            NR_PRLIMIT64,
            pid as usize,
            resource as usize,
            new_rlim as usize,
            old_rlim as usize,
        )
    }
}

#[inline]
pub fn arch_prctl(code: i32, addr: usize) -> isize {
    unsafe { syscall2(NR_ARCH_PRCTL, code as usize, addr) }
}

#[inline]
pub fn set_tid_address(tidptr: *mut i32) -> isize {
    unsafe { syscall1(NR_SET_TID_ADDRESS, tidptr as usize) }
}

#[inline]
pub fn ristux_thread_create(
    entry: usize,
    arg: usize,
    child_stack: usize,
    tls: usize,
    clear_child_tid: *mut u32,
) -> isize {
    unsafe {
        syscall5(
            NR_RISTUX_THREAD_CREATE,
            entry,
            arg,
            child_stack,
            tls,
            clear_child_tid as usize,
        )
    }
}

#[inline]
pub fn set_robust_list(head: usize, len: usize) -> isize {
    unsafe { syscall2(NR_SET_ROBUST_LIST, head, len) }
}

#[inline]
pub fn get_robust_list(pid: isize, head: *mut usize, len: *mut usize) -> isize {
    unsafe {
        syscall3(
            NR_GET_ROBUST_LIST,
            pid as usize,
            head as usize,
            len as usize,
        )
    }
}

#[inline]
pub fn futex(uaddr: *mut u32, op: i32, val: i32, timeout: *const Timespec) -> isize {
    unsafe {
        syscall4(
            NR_FUTEX,
            uaddr as usize,
            op as usize,
            val as usize,
            timeout as usize,
        )
    }
}

#[inline]
pub fn chdir(path: *const u8) -> isize {
    unsafe { syscall1(NR_CHDIR, path as usize) }
}

#[inline]
pub fn mkdir(path: *const u8, mode: u32) -> isize {
    unsafe { syscall2(NR_MKDIR, path as usize, mode as usize) }
}

#[inline]
pub fn mkdirat(dirfd: i32, path: *const u8, mode: u32) -> isize {
    unsafe { syscall3(NR_MKDIRAT, dirfd as usize, path as usize, mode as usize) }
}

#[inline]
pub fn rmdir(path: *const u8) -> isize {
    unsafe { syscall1(NR_RMDIR, path as usize) }
}

#[inline]
pub fn rename(old_path: *const u8, new_path: *const u8) -> isize {
    unsafe { syscall2(NR_RENAME, old_path as usize, new_path as usize) }
}

#[inline]
pub fn renameat(old_dirfd: i32, old_path: *const u8, new_dirfd: i32, new_path: *const u8) -> isize {
    unsafe {
        syscall4(
            NR_RENAMEAT,
            old_dirfd as usize,
            old_path as usize,
            new_dirfd as usize,
            new_path as usize,
        )
    }
}

#[inline]
pub fn renameat2(
    old_dirfd: i32,
    old_path: *const u8,
    new_dirfd: i32,
    new_path: *const u8,
    flags: u32,
) -> isize {
    unsafe {
        syscall5(
            NR_RENAMEAT2,
            old_dirfd as usize,
            old_path as usize,
            new_dirfd as usize,
            new_path as usize,
            flags as usize,
        )
    }
}

#[inline]
pub fn link(old_path: *const u8, new_path: *const u8) -> isize {
    unsafe { syscall2(NR_LINK, old_path as usize, new_path as usize) }
}

#[inline]
pub fn linkat(
    old_dirfd: i32,
    old_path: *const u8,
    new_dirfd: i32,
    new_path: *const u8,
    flags: i32,
) -> isize {
    unsafe {
        syscall5(
            NR_LINKAT,
            old_dirfd as usize,
            old_path as usize,
            new_dirfd as usize,
            new_path as usize,
            flags as usize,
        )
    }
}

#[inline]
pub fn unlink(path: *const u8) -> isize {
    unsafe { syscall1(NR_UNLINK, path as usize) }
}

#[inline]
pub fn unlinkat(dirfd: i32, path: *const u8, flags: i32) -> isize {
    unsafe { syscall3(NR_UNLINKAT, dirfd as usize, path as usize, flags as usize) }
}

#[inline]
pub fn symlink(target: *const u8, link_path: *const u8) -> isize {
    unsafe { syscall2(NR_SYMLINK, target as usize, link_path as usize) }
}

#[inline]
pub fn symlinkat(target: *const u8, dirfd: i32, link_path: *const u8) -> isize {
    unsafe {
        syscall3(
            NR_SYMLINKAT,
            target as usize,
            dirfd as usize,
            link_path as usize,
        )
    }
}

#[inline]
pub fn readlink(path: *const u8, buf: &mut [u8]) -> isize {
    unsafe {
        syscall3(
            NR_READLINK,
            path as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
        )
    }
}

#[inline]
pub fn readlinkat(dirfd: i32, path: *const u8, buf: &mut [u8]) -> isize {
    unsafe {
        syscall4(
            NR_READLINKAT,
            dirfd as usize,
            path as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
        )
    }
}

#[inline]
pub fn chmod(path: *const u8, mode: u32) -> isize {
    unsafe { syscall2(NR_CHMOD, path as usize, mode as usize) }
}

#[inline]
pub fn fchmod(fd: i32, mode: u32) -> isize {
    unsafe { syscall2(NR_FCHMOD, fd as usize, mode as usize) }
}

#[inline]
pub fn fchmodat(dirfd: i32, path: *const u8, mode: u32) -> isize {
    unsafe { syscall3(NR_FCHMODAT, dirfd as usize, path as usize, mode as usize) }
}

#[inline]
pub fn chown(path: *const u8, uid: u32, gid: u32) -> isize {
    unsafe { syscall3(NR_CHOWN, path as usize, uid as usize, gid as usize) }
}

#[inline]
pub fn fchown(fd: i32, uid: u32, gid: u32) -> isize {
    unsafe { syscall3(NR_FCHOWN, fd as usize, uid as usize, gid as usize) }
}

#[inline]
pub fn fchownat(dirfd: i32, path: *const u8, uid: u32, gid: u32, flags: i32) -> isize {
    unsafe {
        syscall5(
            NR_FCHOWNAT,
            dirfd as usize,
            path as usize,
            uid as usize,
            gid as usize,
            flags as usize,
        )
    }
}

#[inline]
pub fn umask(mask: u32) -> isize {
    unsafe { syscall1(NR_UMASK, mask as usize) }
}

#[inline]
pub fn getcwd(buf: *mut u8, len: usize) -> isize {
    unsafe { syscall2(NR_GETCWD, buf as usize, len) }
}

#[inline]
pub fn getdents64(fd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        syscall3(
            NR_GETDENTS64,
            fd as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
        )
    }
}

#[inline]
pub fn statfs(path: *const u8, buf: *mut StatFs) -> isize {
    unsafe { syscall2(NR_STATFS, path as usize, buf as usize) }
}

#[inline]
pub fn fstatfs(fd: i32, buf: *mut StatFs) -> isize {
    unsafe { syscall2(NR_FSTATFS, fd as usize, buf as usize) }
}

#[inline]
pub fn utimensat(dirfd: i32, path: *const u8, times: *const Timespec, flags: i32) -> isize {
    unsafe {
        syscall4(
            NR_UTIMENSAT,
            dirfd as usize,
            path as usize,
            times as usize,
            flags as usize,
        )
    }
}

#[inline]
pub fn mount(source: *const u8, target: *const u8, fstype: *const u8) -> isize {
    unsafe { syscall3(NR_MOUNT, source as usize, target as usize, fstype as usize) }
}

#[inline]
pub fn uname(buf: *mut UtsName) -> isize {
    unsafe { syscall1(NR_UNAME, buf as usize) }
}

#[inline]
pub fn sethostname(name: *const u8, len: usize) -> isize {
    unsafe { syscall2(NR_SETHOSTNAME, name as usize, len) }
}

#[inline]
pub fn reboot(cmd: usize) -> isize {
    unsafe { syscall4(NR_REBOOT, LINUX_REBOOT_MAGIC1, LINUX_REBOOT_MAGIC2, cmd, 0) }
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
pub fn gettid() -> isize {
    unsafe { syscall0(NR_GETTID) }
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
pub fn getrandom(buf: &mut [u8], flags: u32) -> isize {
    unsafe {
        syscall3(
            NR_GETRANDOM,
            buf.as_mut_ptr() as usize,
            buf.len(),
            flags as usize,
        )
    }
}

#[inline]
pub fn rt_sigaction(signum: usize, handler: usize) -> isize {
    unsafe {
        syscall3(
            NR_RT_SIGACTION,
            signum,
            &handler as *const usize as usize,
            0,
        )
    }
}

#[inline]
pub fn rt_sigprocmask(how: i32, set: *const u64, oldset: *mut u64, sigset_size: usize) -> isize {
    unsafe {
        syscall4(
            NR_RT_SIGPROCMASK,
            how as usize,
            set as usize,
            oldset as usize,
            sigset_size,
        )
    }
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
