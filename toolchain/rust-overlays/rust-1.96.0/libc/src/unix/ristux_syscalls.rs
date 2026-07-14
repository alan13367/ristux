use crate::{
    addrinfo, c_char, c_int, c_long, c_uint, c_ulong, c_void, clockid_t, dev_t, dirent, gid_t,
    iovec, mode_t, nfds_t, off_t, passwd, pid_t, pollfd, pthread_attr_t, pthread_cond_t,
    pthread_condattr_t, pthread_key_t, pthread_mutex_t, pthread_mutexattr_t, pthread_rwlock_t,
    pthread_rwlockattr_t, pthread_t, rusage, sigaction as sigaction_t, sighandler_t, sigset_t,
    size_t, sockaddr, socklen_t, ssize_t, stat as stat_t, statfs as statfs_t, termios, timespec,
    timeval, uid_t, utsname, Dl_info,
};
use core::sync::atomic::{AtomicBool, Ordering};

const NR_READ: usize = 0;
const NR_WRITE: usize = 1;
const NR_OPEN: usize = 2;
const NR_CLOSE: usize = 3;
const NR_STAT: usize = 4;
const NR_FSTAT: usize = 5;
const NR_LSTAT: usize = 6;
const NR_POLL: usize = 7;
const NR_LSEEK: usize = 8;
const NR_MMAP: usize = 9;
const NR_MPROTECT: usize = 10;
const NR_MUNMAP: usize = 11;
const NR_RT_SIGACTION: usize = 13;
const NR_RT_SIGPROCMASK: usize = 14;
const NR_IOCTL: usize = 16;
const NR_PREAD64: usize = 17;
const NR_PWRITE64: usize = 18;
const NR_READV: usize = 19;
const NR_WRITEV: usize = 20;
const NR_ACCESS: usize = 21;
const NR_PIPE: usize = 22;
const NR_SCHED_YIELD: usize = 24;
const NR_MADVISE: usize = 28;
const NR_NANOSLEEP: usize = 35;
const NR_DUP: usize = 32;
const NR_DUP2: usize = 33;
const NR_GETPID: usize = 39;
const NR_SOCKET: usize = 41;
const NR_CONNECT: usize = 42;
const NR_ACCEPT: usize = 43;
const NR_SENDTO: usize = 44;
const NR_RECVFROM: usize = 45;
const NR_SHUTDOWN: usize = 48;
const NR_BIND: usize = 49;
const NR_LISTEN: usize = 50;
const NR_GETSOCKNAME: usize = 51;
const NR_SETSOCKOPT: usize = 54;
const NR_GETSOCKOPT: usize = 55;
const NR_FORK: usize = 57;
const NR_EXECVE: usize = 59;
const NR_EXIT: usize = 60;
const NR_WAIT4: usize = 61;
const NR_KILL: usize = 62;
const NR_UNAME: usize = 63;
const NR_FCNTL: usize = 72;
const NR_FSYNC: usize = 74;
const NR_TRUNCATE: usize = 76;
const NR_FTRUNCATE: usize = 77;
const NR_GETCWD: usize = 79;
const NR_CHDIR: usize = 80;
const NR_RENAME: usize = 82;
const NR_MKDIR: usize = 83;
const NR_RMDIR: usize = 84;
const NR_LINK: usize = 86;
const NR_UNLINK: usize = 87;
const NR_SYMLINK: usize = 88;
const NR_READLINK: usize = 89;
const NR_CHMOD: usize = 90;
const NR_FCHMOD: usize = 91;
const NR_CHOWN: usize = 92;
const NR_FCHOWN: usize = 93;
const NR_UMASK: usize = 95;
const NR_GETTIMEOFDAY: usize = 96;
const NR_GETUID: usize = 102;
const NR_GETGID: usize = 104;
const NR_SETUID: usize = 105;
const NR_SETGID: usize = 106;
const NR_GETEUID: usize = 107;
const NR_GETEGID: usize = 108;
const NR_SETPGID: usize = 109;
const NR_GETPPID: usize = 110;
const NR_GETPGRP: usize = 111;
const NR_SETSID: usize = 112;
const NR_SETGROUPS: usize = 116;
const NR_FLOCK: usize = 73;
const NR_STATFS: usize = 137;
const NR_FSTATFS: usize = 138;
const NR_GETTID: usize = 186;
const NR_FUTEX: usize = 202;
const NR_GETDENTS64: usize = 217;
const NR_FADVISE64: usize = 221;
const NR_CLOCK_GETTIME: usize = 228;
const NR_CLOCK_GETRES: usize = 229;
const NR_EXIT_GROUP: usize = 231;
const NR_OPENAT: usize = 257;
const NR_MKDIRAT: usize = 258;
const NR_FCHOWNAT: usize = 260;
const NR_NEWFSTATAT: usize = 262;
const NR_UNLINKAT: usize = 263;
const NR_RENAMEAT: usize = 264;
const NR_LINKAT: usize = 265;
const NR_SYMLINKAT: usize = 266;
const NR_READLINKAT: usize = 267;
const NR_FCHMODAT: usize = 268;
const NR_FACCESSAT: usize = 269;
const NR_UTIMENSAT: usize = 280;
const NR_DUP3: usize = 292;
const NR_PIPE2: usize = 293;
const NR_RENAMEAT2: usize = 316;
const NR_GETRANDOM: usize = 318;
const NR_RISTUX_THREAD_CREATE: usize = 451;
const SYSCALL_TRAMPOLINE_BASE: usize = 0x4000_0000;
const SYSCALL_TRAMPOLINE_STRIDE: usize = 0x20;
const FUTEX_WAIT: usize = 0;
const FUTEX_PRIVATE_FLAG: usize = 0x80;
const EAGAIN: c_int = 11;
const EBADF: c_int = 9;
const EINVAL: c_int = 22;
const EMFILE: c_int = 24;
const ENOSYS: c_int = 38;
const ETIMEDOUT: c_int = 110;
const EAI_NONAME: c_int = -2;
const DIRENT64_HEADER: usize = 19;
const DIR_BUFFER_LEN: usize = 4096;
const MAX_OPEN_DIRS: usize = 16;
const AT_FDCWD: c_int = -100;
const AT_SYMLINK_NOFOLLOW: c_int = 0x100;
const AT_EMPTY_PATH: c_int = 0x1000;
const KERNEL_SIGNAL_FLAG_NOCLDSTOP: u32 = 1;
const KERNEL_SIGNAL_FLAG_RESTART: u32 = 2;
const LIBC_SA_SIGINFO: c_int = 0x0200_0000;
const LIBC_SA_RESTART: c_int = 0x0800_0000;
const LIBC_SA_NOCLDSTOP: c_int = 0x4000_0000;
const SC_CLK_TCK: c_int = 2;
const SC_OPEN_MAX: c_int = 4;
const SC_PAGESIZE: c_int = 30;
const SC_NPROCESSORS_CONF: c_int = 57;
const SC_NPROCESSORS_ONLN: c_int = 58;
const DEFAULT_PTHREAD_STACK_SIZE: size_t = 2 * 1024 * 1024;
const MIN_PTHREAD_STACK_SIZE: size_t = 4096;
const MAX_PTHREAD_KEYS: usize = 64;
const MAX_PTHREAD_THREADS: usize = 64;
const RISTUX_PROT_READ_WRITE: c_int = 0x4 | 0x2;
const RISTUX_MAP_PRIVATE_ANON: c_int = 0x02 | 0x20;

#[repr(C)]
struct RistuxDir {
    fd: c_int,
    pos: usize,
    len: usize,
    buf: [u8; DIR_BUFFER_LEN],
    ent: dirent,
}

impl core::marker::Copy for RistuxDir {}

impl core::clone::Clone for RistuxDir {
    fn clone(&self) -> Self {
        *self
    }
}

#[repr(C)]
struct KernelSigAction {
    handler: usize,
    mask: u64,
    flags: u32,
    _pad: u32,
}

#[repr(C)]
struct PthreadStart {
    start: extern "C" fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
    clear_tid: u32,
}

static mut ERRNO: c_int = 0;
static mut NEXT_PTHREAD_KEY: pthread_key_t = 1;
static mut PTHREAD_THREAD_IDS: [pid_t; MAX_PTHREAD_THREADS] = [0; MAX_PTHREAD_THREADS];
static mut PTHREAD_CLEAR_TIDS: [usize; MAX_PTHREAD_THREADS] = [0; MAX_PTHREAD_THREADS];
static mut PTHREAD_VALUES: [[*const c_void; MAX_PTHREAD_KEYS]; MAX_PTHREAD_THREADS] =
    [[core::ptr::null(); MAX_PTHREAD_KEYS]; MAX_PTHREAD_THREADS];
static PTHREAD_LOCK: AtomicBool = AtomicBool::new(false);
static DIR_LOCK: AtomicBool = AtomicBool::new(false);
static mut DIR_STATES: [RistuxDir; MAX_OPEN_DIRS] = [RistuxDir::closed(); MAX_OPEN_DIRS];

#[no_mangle]
pub static mut environ: *const *const c_char = core::ptr::null();

#[no_mangle]
pub extern "C" fn errno_location() -> *mut c_int {
    core::ptr::addr_of_mut!(ERRNO)
}

#[no_mangle]
pub extern "C" fn __errno() -> *mut c_int {
    errno_location()
}

#[no_mangle]
pub extern "C" fn __errno_location() -> *mut c_int {
    errno_location()
}

#[inline]
unsafe fn syscall1(nr: usize, a0: usize) -> isize {
    type Syscall1 = unsafe extern "C" fn(usize, usize) -> isize;
    let f = unsafe {
        core::mem::transmute::<usize, Syscall1>(SYSCALL_TRAMPOLINE_BASE + SYSCALL_TRAMPOLINE_STRIDE)
    };
    unsafe { f(nr, a0) }
}

#[inline]
unsafe fn syscall0(nr: usize) -> isize {
    type Syscall0 = unsafe extern "C" fn(usize) -> isize;
    let f = unsafe { core::mem::transmute::<usize, Syscall0>(SYSCALL_TRAMPOLINE_BASE) };
    unsafe { f(nr) }
}

#[inline]
unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> isize {
    type Syscall2 = unsafe extern "C" fn(usize, usize, usize) -> isize;
    let f = unsafe {
        core::mem::transmute::<usize, Syscall2>(
            SYSCALL_TRAMPOLINE_BASE + SYSCALL_TRAMPOLINE_STRIDE * 2,
        )
    };
    unsafe { f(nr, a0, a1) }
}

#[inline]
unsafe fn syscall3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    type Syscall3 = unsafe extern "C" fn(usize, usize, usize, usize) -> isize;
    let f = unsafe {
        core::mem::transmute::<usize, Syscall3>(
            SYSCALL_TRAMPOLINE_BASE + SYSCALL_TRAMPOLINE_STRIDE * 3,
        )
    };
    unsafe { f(nr, a0, a1, a2) }
}

#[inline]
unsafe fn syscall4(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    type Syscall4 = unsafe extern "C" fn(usize, usize, usize, usize, usize) -> isize;
    let f = unsafe {
        core::mem::transmute::<usize, Syscall4>(
            SYSCALL_TRAMPOLINE_BASE + SYSCALL_TRAMPOLINE_STRIDE * 4,
        )
    };
    unsafe { f(nr, a0, a1, a2, a3) }
}

#[inline]
unsafe fn syscall5(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    type Syscall5 = unsafe extern "C" fn(usize, usize, usize, usize, usize, usize) -> isize;
    let f = unsafe {
        core::mem::transmute::<usize, Syscall5>(
            SYSCALL_TRAMPOLINE_BASE + SYSCALL_TRAMPOLINE_STRIDE * 5,
        )
    };
    unsafe { f(nr, a0, a1, a2, a3, a4) }
}

#[inline]
unsafe fn syscall6(
    nr: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    type Syscall6 = unsafe extern "C" fn(usize, usize, usize, usize, usize, usize, usize) -> isize;
    let f = unsafe {
        core::mem::transmute::<usize, Syscall6>(
            SYSCALL_TRAMPOLINE_BASE + SYSCALL_TRAMPOLINE_STRIDE * 6,
        )
    };
    unsafe { f(nr, a0, a1, a2, a3, a4, a5) }
}

#[inline]
fn is_errno(ret: isize) -> bool {
    (-4095..0).contains(&ret)
}

#[inline]
fn set_errno_from(ret: isize) {
    unsafe {
        ERRNO = (-ret) as c_int;
    }
}

#[inline]
fn set_errno(errno: c_int) {
    unsafe {
        ERRNO = errno;
    }
}

#[inline]
fn cvt(ret: isize) -> ssize_t {
    if is_errno(ret) {
        set_errno_from(ret);
        -1
    } else {
        ret as ssize_t
    }
}

#[inline]
fn cvt_int(ret: isize) -> c_int {
    cvt(ret) as c_int
}

#[inline]
fn cvt_pid(ret: isize) -> pid_t {
    cvt(ret) as pid_t
}

#[inline]
fn cvt_off(ret: isize) -> off_t {
    cvt(ret) as off_t
}

#[inline]
fn cvt_ptr(ret: isize) -> *mut c_void {
    if is_errno(ret) {
        set_errno_from(ret);
        !0usize as *mut c_void
    } else {
        ret as usize as *mut c_void
    }
}

#[inline]
fn cvt_pthread(ret: isize) -> c_int {
    if is_errno(ret) {
        (-ret) as c_int
    } else {
        0
    }
}

#[inline]
unsafe fn zero_pthread_object<T>(ptr: *mut T) -> c_int {
    if ptr.is_null() {
        return EINVAL;
    }
    unsafe {
        core::ptr::write_bytes(ptr, 0, 1);
    }
    0
}

#[inline]
unsafe fn copy_c_string_unbounded(src: *const c_char, dst: *mut c_char) -> bool {
    if src.is_null() || dst.is_null() {
        return false;
    }
    let mut offset = 0usize;
    loop {
        let byte = unsafe { *src.add(offset) };
        unsafe {
            *dst.add(offset) = byte;
        }
        if byte == 0 {
            return true;
        }
        offset += 1;
    }
}

#[inline]
const fn empty_dirent() -> dirent {
    dirent {
        d_ino: 0,
        d_off: 0,
        d_reclen: 0,
        d_type: 0,
        d_name: [0; 256],
    }
}

impl RistuxDir {
    const fn closed() -> Self {
        Self {
            fd: -1,
            pos: 0,
            len: 0,
            buf: [0; DIR_BUFFER_LEN],
            ent: empty_dirent(),
        }
    }
}

#[inline]
fn lock_dir_pool() {
    while DIR_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

#[inline]
fn unlock_dir_pool() {
    DIR_LOCK.store(false, Ordering::Release);
}

#[inline]
fn lock_pthread_pool() {
    while PTHREAD_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

#[inline]
fn unlock_pthread_pool() {
    PTHREAD_LOCK.store(false, Ordering::Release);
}

unsafe fn remember_pthread_clear_tid(tid: pid_t, clear_tid: usize) {
    lock_pthread_pool();
    for index in 0..MAX_PTHREAD_THREADS {
        if unsafe { PTHREAD_THREAD_IDS[index] } == tid || unsafe { PTHREAD_THREAD_IDS[index] } == 0
        {
            unsafe {
                PTHREAD_THREAD_IDS[index] = tid;
                PTHREAD_CLEAR_TIDS[index] = clear_tid;
            }
            unlock_pthread_pool();
            return;
        }
    }
    unlock_pthread_pool();
}

unsafe fn lookup_pthread_clear_tid(tid: pid_t) -> usize {
    lock_pthread_pool();
    for index in 0..MAX_PTHREAD_THREADS {
        if unsafe { PTHREAD_THREAD_IDS[index] } == tid {
            let clear_tid = unsafe { PTHREAD_CLEAR_TIDS[index] };
            unlock_pthread_pool();
            return clear_tid;
        }
    }
    unlock_pthread_pool();
    0
}

unsafe fn retire_pthread_slot(tid: pid_t) {
    lock_pthread_pool();
    for index in 0..MAX_PTHREAD_THREADS {
        if unsafe { PTHREAD_THREAD_IDS[index] } == tid {
            unsafe {
                PTHREAD_THREAD_IDS[index] = 0;
                PTHREAD_CLEAR_TIDS[index] = 0;
                for key in 0..MAX_PTHREAD_KEYS {
                    PTHREAD_VALUES[index][key] = core::ptr::null();
                }
            }
            break;
        }
    }
    unlock_pthread_pool();
}

unsafe fn dir_state_from_ptr(
    dirp: *mut crate::DIR,
) -> core::option::Option<&'static mut RistuxDir> {
    for index in 0..MAX_OPEN_DIRS {
        let state = unsafe { core::ptr::addr_of_mut!(DIR_STATES[index]) };
        if state as *mut crate::DIR == dirp {
            return core::option::Option::Some(unsafe { &mut *state });
        }
    }
    core::option::Option::None
}

unsafe fn init_dir_state(fd: c_int) -> *mut crate::DIR {
    lock_dir_pool();
    for index in 0..MAX_OPEN_DIRS {
        let state = unsafe { &mut *core::ptr::addr_of_mut!(DIR_STATES[index]) };
        if state.fd < 0 {
            state.fd = fd;
            state.pos = 0;
            state.len = 0;
            state.buf.fill(0);
            state.ent = empty_dirent();
            let dirp = state as *mut RistuxDir as *mut crate::DIR;
            unlock_dir_pool();
            return dirp;
        }
    }
    unlock_dir_pool();
    let _ = unsafe { close(fd) };
    set_errno(EMFILE);
    core::ptr::null_mut()
}

unsafe fn pthread_thread_slot_locked() -> core::option::Option<usize> {
    let tid = unsafe { gettid() };
    if tid <= 0 {
        return core::option::Option::None;
    }
    for index in 0..MAX_PTHREAD_THREADS {
        let slot_tid = unsafe { PTHREAD_THREAD_IDS[index] };
        if slot_tid == tid {
            return core::option::Option::Some(index);
        }
    }
    for index in 0..MAX_PTHREAD_THREADS {
        if unsafe { PTHREAD_THREAD_IDS[index] } == 0 {
            unsafe {
                PTHREAD_THREAD_IDS[index] = tid;
            }
            return core::option::Option::Some(index);
        }
    }
    core::option::Option::None
}

unsafe fn pthread_attr_stack_size(attr: *const pthread_attr_t) -> size_t {
    if attr.is_null() {
        return DEFAULT_PTHREAD_STACK_SIZE;
    }
    let stored = unsafe { *(attr as *const size_t) };
    if stored == 0 {
        DEFAULT_PTHREAD_STACK_SIZE
    } else {
        stored
    }
}

unsafe fn pthread_attr_set_stack_size_raw(attr: *mut pthread_attr_t, stack_size: size_t) {
    unsafe {
        *(attr as *mut size_t) = stack_size;
    }
}

fn align_stack_size(stack_size: size_t) -> core::option::Option<size_t> {
    let stack_size = if stack_size < MIN_PTHREAD_STACK_SIZE {
        MIN_PTHREAD_STACK_SIZE
    } else {
        stack_size
    };
    stack_size
        .checked_add(MIN_PTHREAD_STACK_SIZE - 1)
        .map(|value| value & !(MIN_PTHREAD_STACK_SIZE - 1))
}

#[inline]
fn libc_signal_flags_to_kernel(flags: c_int) -> core::option::Option<u32> {
    let mut out = 0u32;
    let mut unsupported = flags;
    if unsupported & LIBC_SA_RESTART != 0 {
        out |= KERNEL_SIGNAL_FLAG_RESTART;
        unsupported &= !LIBC_SA_RESTART;
    }
    if unsupported & LIBC_SA_NOCLDSTOP != 0 {
        out |= KERNEL_SIGNAL_FLAG_NOCLDSTOP;
        unsupported &= !LIBC_SA_NOCLDSTOP;
    }
    if unsupported & LIBC_SA_SIGINFO != 0 {
        unsupported &= !LIBC_SA_SIGINFO;
    }
    if unsupported == 0 {
        core::option::Option::Some(out)
    } else {
        core::option::Option::None
    }
}

#[inline]
fn kernel_signal_flags_to_libc(flags: u32) -> c_int {
    let mut out = 0;
    if flags & KERNEL_SIGNAL_FLAG_RESTART != 0 {
        out |= LIBC_SA_RESTART;
    }
    if flags & KERNEL_SIGNAL_FLAG_NOCLDSTOP != 0 {
        out |= LIBC_SA_NOCLDSTOP;
    }
    out
}

#[no_mangle]
pub extern "sysv64" fn rust_psm_stack_direction() -> u8 {
    2
}

#[no_mangle]
pub extern "sysv64" fn rust_psm_stack_pointer() -> *mut u8 {
    let local = 0u8;
    (&local as *const u8) as *mut u8
}

#[no_mangle]
pub unsafe extern "sysv64" fn rust_psm_on_stack(
    data: usize,
    return_ptr: usize,
    callback: unsafe extern "sysv64" fn(usize, usize),
    _sp: *mut u8,
) {
    unsafe { callback(data, return_ptr) }
}

#[no_mangle]
pub unsafe extern "sysv64" fn rust_psm_replace_stack(
    data: usize,
    callback: unsafe extern "sysv64" fn(usize) -> !,
    _sp: *mut u8,
) -> ! {
    unsafe { callback(data) }
}

#[no_mangle]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t {
    cvt(unsafe { syscall3(NR_READ, fd as usize, buf as usize, count) })
}

#[no_mangle]
pub unsafe extern "C" fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t {
    cvt(unsafe { syscall3(NR_WRITE, fd as usize, buf as usize, count) })
}

#[inline]
fn ristux_open_syscall_flags(flags: c_int) -> usize {
    const KERNEL_O_ACCMODE: c_int = 0o3;
    const KERNEL_O_WRONLY: c_int = 1;
    const KERNEL_O_RDWR: c_int = 2;
    const KERNEL_O_CREAT: c_int = 0o100;
    const KERNEL_O_EXCL: c_int = 0o200;
    const KERNEL_O_TRUNC: c_int = 0o1000;
    const KERNEL_O_APPEND: c_int = 0o2000;
    const KERNEL_O_NONBLOCK: c_int = 0o4000;
    const KERNEL_O_CLOEXEC: c_int = 0o2000000;

    const REDOX_O_ACCMODE: c_int = 0x0003_0000;
    const REDOX_O_RDONLY: c_int = 0x0001_0000;
    const REDOX_O_WRONLY: c_int = 0x0002_0000;
    const REDOX_O_RDWR: c_int = 0x0003_0000;
    const REDOX_O_NONBLOCK: c_int = 0x0004_0000;
    const REDOX_O_APPEND: c_int = 0x0008_0000;
    const REDOX_O_CLOEXEC: c_int = 0x0100_0000;
    const REDOX_O_CREAT: c_int = 0x0200_0000;
    const REDOX_O_TRUNC: c_int = 0x0400_0000;
    const REDOX_O_EXCL: c_int = 0x0800_0000;
    const REDOX_O_DIRECTORY: c_int = 0x1000_0000;
    const REDOX_O_NOFOLLOW: c_int = c_int::MIN;
    const REDOX_KNOWN_FLAGS: c_int = REDOX_O_ACCMODE
        | REDOX_O_NONBLOCK
        | REDOX_O_APPEND
        | REDOX_O_CLOEXEC
        | REDOX_O_CREAT
        | REDOX_O_TRUNC
        | REDOX_O_EXCL
        | REDOX_O_DIRECTORY
        | REDOX_O_NOFOLLOW;

    if flags & REDOX_O_ACCMODE == 0 {
        return flags as usize;
    }

    let mut normalized = flags & !REDOX_KNOWN_FLAGS & !KERNEL_O_ACCMODE;
    normalized |= match flags & REDOX_O_ACCMODE {
        REDOX_O_WRONLY => KERNEL_O_WRONLY,
        REDOX_O_RDWR => KERNEL_O_RDWR,
        REDOX_O_RDONLY => 0,
        _ => flags & KERNEL_O_ACCMODE,
    };
    if flags & REDOX_O_NONBLOCK != 0 {
        normalized |= KERNEL_O_NONBLOCK;
    }
    if flags & REDOX_O_APPEND != 0 {
        normalized |= KERNEL_O_APPEND;
    }
    if flags & REDOX_O_CREAT != 0 {
        normalized |= KERNEL_O_CREAT;
    }
    if flags & REDOX_O_TRUNC != 0 {
        normalized |= KERNEL_O_TRUNC;
    }
    if flags & REDOX_O_EXCL != 0 {
        normalized |= KERNEL_O_EXCL;
    }
    if flags & REDOX_O_CLOEXEC != 0 {
        normalized |= KERNEL_O_CLOEXEC;
    }
    normalized as usize
}

#[no_mangle]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    cvt_int(unsafe {
        syscall3(
            NR_OPEN,
            path as usize,
            ristux_open_syscall_flags(flags),
            mode as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn openat(
    dirfd: c_int,
    path: *const c_char,
    flags: c_int,
    mode: mode_t,
) -> c_int {
    cvt_int(unsafe {
        syscall4(
            NR_OPENAT,
            dirfd as usize,
            path as usize,
            ristux_open_syscall_flags(flags),
            mode as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t {
    cvt(unsafe { syscall3(NR_READV, fd as usize, iov as usize, iovcnt as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t {
    cvt(unsafe { syscall3(NR_WRITEV, fd as usize, iov as usize, iovcnt as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn pread(
    fd: c_int,
    buf: *mut c_void,
    count: size_t,
    offset: off_t,
) -> ssize_t {
    cvt(unsafe {
        syscall4(
            NR_PREAD64,
            fd as usize,
            buf as usize,
            count,
            offset as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn pwrite(
    fd: c_int,
    buf: *const c_void,
    count: size_t,
    offset: off_t,
) -> ssize_t {
    cvt(unsafe {
        syscall4(
            NR_PWRITE64,
            fd as usize,
            buf as usize,
            count,
            offset as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    cvt(unsafe { syscall1(NR_CLOSE, fd as usize) }) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn stat(path: *const c_char, buf: *mut stat_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_STAT, path as usize, buf as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn fstat(fd: c_int, buf: *mut stat_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_FSTAT, fd as usize, buf as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn lstat(path: *const c_char, buf: *mut stat_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_LSTAT, path as usize, buf as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn fstatat(
    dirfd: c_int,
    path: *const c_char,
    buf: *mut stat_t,
    flags: c_int,
) -> c_int {
    cvt_int(unsafe {
        syscall4(
            NR_NEWFSTATAT,
            dirfd as usize,
            path as usize,
            buf as usize,
            flags as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn statfs(path: *const c_char, buf: *mut statfs_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_STATFS, path as usize, buf as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn fstatfs(fd: c_int, buf: *mut statfs_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_FSTATFS, fd as usize, buf as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t {
    cvt_off(unsafe { syscall3(NR_LSEEK, fd as usize, offset as usize, whence as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn truncate(path: *const c_char, len: off_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_TRUNCATE, path as usize, len as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn ftruncate(fd: c_int, len: off_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_FTRUNCATE, fd as usize, len as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn chdir(path: *const c_char) -> c_int {
    cvt_int(unsafe { syscall1(NR_CHDIR, path as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn chroot(_path: *const c_char) -> c_int {
    set_errno(ENOSYS);
    -1
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(path: *const c_char, mode: mode_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_MKDIR, path as usize, mode as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn mkfifo(_path: *const c_char, _mode: mode_t) -> c_int {
    set_errno(ENOSYS);
    -1
}

#[no_mangle]
pub unsafe extern "C" fn mkdirat(dirfd: c_int, path: *const c_char, mode: mode_t) -> c_int {
    cvt_int(unsafe { syscall3(NR_MKDIRAT, dirfd as usize, path as usize, mode as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn rmdir(path: *const c_char) -> c_int {
    cvt_int(unsafe { syscall1(NR_RMDIR, path as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn rename(old_path: *const c_char, new_path: *const c_char) -> c_int {
    cvt_int(unsafe { syscall2(NR_RENAME, old_path as usize, new_path as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn renameat(
    old_dirfd: c_int,
    old_path: *const c_char,
    new_dirfd: c_int,
    new_path: *const c_char,
) -> c_int {
    cvt_int(unsafe {
        syscall4(
            NR_RENAMEAT,
            old_dirfd as usize,
            old_path as usize,
            new_dirfd as usize,
            new_path as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn renameat2(
    old_dirfd: c_int,
    old_path: *const c_char,
    new_dirfd: c_int,
    new_path: *const c_char,
    flags: c_uint,
) -> c_int {
    cvt_int(unsafe {
        syscall5(
            NR_RENAMEAT2,
            old_dirfd as usize,
            old_path as usize,
            new_dirfd as usize,
            new_path as usize,
            flags as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn link(old_path: *const c_char, new_path: *const c_char) -> c_int {
    cvt_int(unsafe { syscall2(NR_LINK, old_path as usize, new_path as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn linkat(
    old_dirfd: c_int,
    old_path: *const c_char,
    new_dirfd: c_int,
    new_path: *const c_char,
    flags: c_int,
) -> c_int {
    cvt_int(unsafe {
        syscall5(
            NR_LINKAT,
            old_dirfd as usize,
            old_path as usize,
            new_dirfd as usize,
            new_path as usize,
            flags as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn unlink(path: *const c_char) -> c_int {
    cvt_int(unsafe { syscall1(NR_UNLINK, path as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn unlinkat(dirfd: c_int, path: *const c_char, flags: c_int) -> c_int {
    cvt_int(unsafe { syscall3(NR_UNLINKAT, dirfd as usize, path as usize, flags as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn symlink(target: *const c_char, link_path: *const c_char) -> c_int {
    cvt_int(unsafe { syscall2(NR_SYMLINK, target as usize, link_path as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn symlinkat(
    target: *const c_char,
    dirfd: c_int,
    link_path: *const c_char,
) -> c_int {
    cvt_int(unsafe {
        syscall3(
            NR_SYMLINKAT,
            target as usize,
            dirfd as usize,
            link_path as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn readlink(
    path: *const c_char,
    buf: *mut c_char,
    bufsiz: size_t,
) -> ssize_t {
    cvt(unsafe { syscall3(NR_READLINK, path as usize, buf as usize, bufsiz) })
}

#[no_mangle]
pub unsafe extern "C" fn readlinkat(
    dirfd: c_int,
    path: *const c_char,
    buf: *mut c_char,
    bufsiz: size_t,
) -> ssize_t {
    cvt(unsafe {
        syscall4(
            NR_READLINKAT,
            dirfd as usize,
            path as usize,
            buf as usize,
            bufsiz,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn realpath(pathname: *const c_char, resolved: *mut c_char) -> *mut c_char {
    if resolved.is_null() || !unsafe { copy_c_string_unbounded(pathname, resolved) } {
        set_errno(EINVAL);
        core::ptr::null_mut()
    } else {
        resolved
    }
}

#[no_mangle]
pub unsafe extern "C" fn chmod(path: *const c_char, mode: mode_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_CHMOD, path as usize, mode as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn fchmod(fd: c_int, mode: mode_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_FCHMOD, fd as usize, mode as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn fchmodat(
    dirfd: c_int,
    path: *const c_char,
    mode: mode_t,
    flags: c_int,
) -> c_int {
    if flags != 0 {
        set_errno(EINVAL);
        return -1;
    }
    cvt_int(unsafe { syscall3(NR_FCHMODAT, dirfd as usize, path as usize, mode as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn chown(path: *const c_char, uid: uid_t, gid: gid_t) -> c_int {
    cvt_int(unsafe { syscall3(NR_CHOWN, path as usize, uid as usize, gid as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn lchown(path: *const c_char, uid: uid_t, gid: gid_t) -> c_int {
    unsafe { fchownat(AT_FDCWD, path, uid, gid, AT_SYMLINK_NOFOLLOW) }
}

#[no_mangle]
pub unsafe extern "C" fn fchown(fd: c_int, uid: uid_t, gid: gid_t) -> c_int {
    cvt_int(unsafe { syscall3(NR_FCHOWN, fd as usize, uid as usize, gid as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn fchownat(
    dirfd: c_int,
    path: *const c_char,
    uid: uid_t,
    gid: gid_t,
    flags: c_int,
) -> c_int {
    cvt_int(unsafe {
        syscall5(
            NR_FCHOWNAT,
            dirfd as usize,
            path as usize,
            uid as usize,
            gid as usize,
            flags as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn umask(mask: mode_t) -> mode_t {
    cvt_int(unsafe { syscall1(NR_UMASK, mask as usize) }) as mode_t
}

#[no_mangle]
pub unsafe extern "C" fn utimensat(
    dirfd: c_int,
    pathname: *const c_char,
    times: *const timespec,
    flags: c_int,
) -> c_int {
    cvt_int(unsafe {
        syscall4(
            NR_UTIMENSAT,
            dirfd as usize,
            pathname as usize,
            times as usize,
            flags as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn futimens(fd: c_int, times: *const timespec) -> c_int {
    unsafe { utimensat(fd, core::ptr::null(), times, AT_EMPTY_PATH) }
}

#[no_mangle]
pub unsafe extern "C" fn access(path: *const c_char, mode: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_ACCESS, path as usize, mode as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn faccessat(
    dirfd: c_int,
    path: *const c_char,
    mode: c_int,
    flags: c_int,
) -> c_int {
    cvt_int(unsafe {
        syscall4(
            NR_FACCESSAT,
            dirfd as usize,
            path as usize,
            mode as usize,
            flags as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn fsync(fd: c_int) -> c_int {
    cvt_int(unsafe { syscall1(NR_FSYNC, fd as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn fdatasync(fd: c_int) -> c_int {
    unsafe { fsync(fd) }
}

#[no_mangle]
pub unsafe extern "C" fn sync() {}

#[no_mangle]
pub unsafe extern "C" fn fallocate(_fd: c_int, _mode: c_int, _offset: off_t, _len: off_t) -> c_int {
    set_errno(ENOSYS);
    -1
}

#[no_mangle]
pub unsafe extern "C" fn posix_fallocate(_fd: c_int, _offset: off_t, _len: off_t) -> c_int {
    ENOSYS
}

#[no_mangle]
pub unsafe extern "C" fn mknodat(
    _dirfd: c_int,
    _pathname: *const c_char,
    _mode: mode_t,
    _dev: dev_t,
) -> c_int {
    set_errno(ENOSYS);
    -1
}

#[no_mangle]
pub unsafe extern "C" fn seekdir(_dirp: *mut crate::DIR, _loc: c_long) {}

#[no_mangle]
pub unsafe extern "C" fn fcntl(fd: c_int, cmd: c_int, arg: usize) -> c_int {
    cvt_int(unsafe { syscall3(NR_FCNTL, fd as usize, cmd as usize, arg) })
}

#[no_mangle]
pub unsafe extern "C" fn flock(fd: c_int, operation: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_FLOCK, fd as usize, operation as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn ioctl(fd: c_int, request: c_ulong, arg: usize) -> c_int {
    cvt_int(unsafe { syscall3(NR_IOCTL, fd as usize, request as usize, arg) })
}

#[no_mangle]
pub unsafe extern "C" fn isatty(fd: c_int) -> c_int {
    const TCGETS: c_ulong = 0x5401;
    let mut storage = [0u8; 64];
    if unsafe { ioctl(fd, TCGETS, storage.as_mut_ptr() as usize) } == 0 {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn tcgetattr(fd: c_int, termios_p: *mut termios) -> c_int {
    const TCGETS: c_ulong = 0x5401;
    unsafe { ioctl(fd, TCGETS, termios_p as usize) }
}

#[no_mangle]
pub unsafe extern "C" fn tcsetattr(
    fd: c_int,
    optional_actions: c_int,
    termios_p: *const termios,
) -> c_int {
    const TCSETS: c_ulong = 0x5402;
    const TCSETSW: c_ulong = 0x5403;
    const TCSETSF: c_ulong = 0x5404;
    let request = match optional_actions {
        0 => TCSETS,
        1 => TCSETSW,
        2 => TCSETSF,
        _ => {
            set_errno(EINVAL);
            return -1;
        }
    };
    unsafe { ioctl(fd, request, termios_p as usize) }
}

#[no_mangle]
pub unsafe extern "C" fn opendir(dirname: *const c_char) -> *mut crate::DIR {
    let fd = unsafe { open(dirname, 0, 0) };
    if fd < 0 {
        core::ptr::null_mut()
    } else {
        unsafe { init_dir_state(fd) }
    }
}

#[no_mangle]
pub unsafe extern "C" fn fdopendir(fd: c_int) -> *mut crate::DIR {
    if fd < 0 {
        set_errno(EBADF);
        core::ptr::null_mut()
    } else {
        unsafe { init_dir_state(fd) }
    }
}

#[no_mangle]
pub unsafe extern "C" fn readdir(dirp: *mut crate::DIR) -> *mut dirent {
    if dirp.is_null() {
        set_errno(EINVAL);
        return core::ptr::null_mut();
    }
    let core::option::Option::Some(state) = (unsafe { dir_state_from_ptr(dirp) }) else {
        set_errno(EBADF);
        return core::ptr::null_mut();
    };
    if state.fd < 0 {
        set_errno(EBADF);
        return core::ptr::null_mut();
    }
    loop {
        if state.pos + DIRENT64_HEADER <= state.len {
            let reclen =
                u16::from_le_bytes([state.buf[state.pos + 16], state.buf[state.pos + 17]]) as usize;
            if reclen == 0 || state.pos + reclen > state.len {
                state.pos = state.len;
                continue;
            }
            let base = state.pos;
            state.pos += reclen;

            let ino = u64::from_le_bytes([
                state.buf[base],
                state.buf[base + 1],
                state.buf[base + 2],
                state.buf[base + 3],
                state.buf[base + 4],
                state.buf[base + 5],
                state.buf[base + 6],
                state.buf[base + 7],
            ]);
            let off = i64::from_le_bytes([
                state.buf[base + 8],
                state.buf[base + 9],
                state.buf[base + 10],
                state.buf[base + 11],
                state.buf[base + 12],
                state.buf[base + 13],
                state.buf[base + 14],
                state.buf[base + 15],
            ]);
            state.ent.d_ino = ino as _;
            state.ent.d_off = off as _;
            state.ent.d_reclen = core::mem::size_of::<dirent>() as _;
            state.ent.d_type = state.buf[base + 18] as _;
            state.ent.d_name.fill(0);
            let name_start = base + DIRENT64_HEADER;
            let mut name_end = base + reclen;
            let mut scan = name_start;
            while scan < base + reclen {
                if state.buf[scan] == 0 {
                    name_end = scan;
                    break;
                }
                scan += 1;
            }
            let max_name_len = state.ent.d_name.len() - 1;
            let raw_name_len = name_end - name_start;
            let name_len = if raw_name_len < max_name_len {
                raw_name_len
            } else {
                max_name_len
            };
            let mut index = 0usize;
            while index < name_len {
                state.ent.d_name[index] = state.buf[name_start + index] as c_char;
                index += 1;
            }
            return core::ptr::addr_of_mut!(state.ent);
        }

        let ret = unsafe {
            syscall3(
                NR_GETDENTS64,
                state.fd as usize,
                state.buf.as_mut_ptr() as usize,
                state.buf.len(),
            )
        };
        if is_errno(ret) {
            set_errno_from(ret);
            return core::ptr::null_mut();
        }
        if ret == 0 {
            return core::ptr::null_mut();
        }
        state.pos = 0;
        state.len = ret as usize;
    }
}

#[no_mangle]
pub unsafe extern "C" fn closedir(dirp: *mut crate::DIR) -> c_int {
    if dirp.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    lock_dir_pool();
    let core::option::Option::Some(state) = (unsafe { dir_state_from_ptr(dirp) }) else {
        unlock_dir_pool();
        set_errno(EBADF);
        return -1;
    };
    if state.fd < 0 {
        unlock_dir_pool();
        set_errno(EBADF);
        return -1;
    }
    let fd = state.fd;
    state.fd = -1;
    state.pos = 0;
    state.len = 0;
    state.buf.fill(0);
    state.ent = empty_dirent();
    unlock_dir_pool();
    unsafe { close(fd) }
}

#[no_mangle]
pub unsafe extern "C" fn dirfd(dirp: *mut crate::DIR) -> c_int {
    if dirp.is_null() {
        set_errno(EINVAL);
        -1
    } else if let core::option::Option::Some(state) = unsafe { dir_state_from_ptr(dirp) } {
        state.fd
    } else {
        set_errno(EBADF);
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int {
    cvt_int(unsafe { syscall3(NR_POLL, fds as usize, nfds as usize, timeout as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn pipe(fds: *mut c_int) -> c_int {
    cvt_int(unsafe { syscall1(NR_PIPE, fds as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn pipe2(fds: *mut c_int, flags: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_PIPE2, fds as usize, flags as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn dup(fd: c_int) -> c_int {
    cvt_int(unsafe { syscall1(NR_DUP, fd as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn dup2(oldfd: c_int, newfd: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_DUP2, oldfd as usize, newfd as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn dup3(oldfd: c_int, newfd: c_int, flags: c_int) -> c_int {
    cvt_int(unsafe { syscall3(NR_DUP3, oldfd as usize, newfd as usize, flags as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn mmap(
    addr: *mut c_void,
    len: size_t,
    prot: c_int,
    flags: c_int,
    fd: c_int,
    offset: off_t,
) -> *mut c_void {
    cvt_ptr(unsafe {
        syscall6(
            NR_MMAP,
            addr as usize,
            len,
            prot as usize,
            flags as usize,
            fd as usize,
            offset as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn posix_memalign(
    memptr: *mut *mut c_void,
    alignment: size_t,
    size: size_t,
) -> c_int {
    if memptr.is_null() || alignment < core::mem::size_of::<usize>() || !alignment.is_power_of_two()
    {
        return EINVAL;
    }
    let length = match size.checked_add(alignment) {
        core::option::Option::Some(length) => length,
        core::option::Option::None => return crate::ENOMEM,
    };
    let raw = unsafe {
        mmap(
            core::ptr::null_mut(),
            length,
            RISTUX_PROT_READ_WRITE,
            RISTUX_MAP_PRIVATE_ANON,
            -1,
            0,
        )
    };
    if raw as isize == -1 {
        return unsafe { *errno_location() };
    }
    let aligned = ((raw as usize) + alignment - 1) & !(alignment - 1);
    unsafe {
        *memptr = aligned as *mut c_void;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn mprotect(addr: *mut c_void, len: size_t, prot: c_int) -> c_int {
    cvt_int(unsafe { syscall3(NR_MPROTECT, addr as usize, len, prot as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn munmap(addr: *mut c_void, len: size_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_MUNMAP, addr as usize, len) })
}

#[no_mangle]
pub unsafe extern "C" fn madvise(addr: *mut c_void, len: size_t, advice: c_int) -> c_int {
    cvt_int(unsafe { syscall3(NR_MADVISE, addr as usize, len, advice as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn posix_fadvise(
    fd: c_int,
    offset: off_t,
    len: off_t,
    advice: c_int,
) -> c_int {
    cvt_int(unsafe {
        syscall4(
            NR_FADVISE64,
            fd as usize,
            offset as usize,
            len as usize,
            advice as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn gettimeofday(tp: *mut timeval, _tz: *mut c_void) -> c_int {
    cvt_int(unsafe { syscall2(NR_GETTIMEOFDAY, tp as usize, 0) })
}

#[inline]
fn ristux_clock_syscall_id(clk_id: clockid_t) -> usize {
    match clk_id as c_int {
        // libc currently inherits these constants from the Redox module for
        // target_os = "ristux"; the kernel ABI remains Linux-like.
        1 => 0,     // CLOCK_REALTIME
        2 | 4 => 1, // CLOCK_PROCESS_CPUTIME_ID and CLOCK_MONOTONIC
        other => other as usize,
    }
}

#[no_mangle]
pub unsafe extern "C" fn clock_gettime(clk_id: clockid_t, tp: *mut timespec) -> c_int {
    cvt_int(unsafe {
        syscall2(
            NR_CLOCK_GETTIME,
            ristux_clock_syscall_id(clk_id),
            tp as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn clock_getres(clk_id: clockid_t, tp: *mut timespec) -> c_int {
    cvt_int(unsafe {
        syscall2(
            NR_CLOCK_GETRES,
            ristux_clock_syscall_id(clk_id),
            tp as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn nanosleep(req: *const timespec, rem: *mut timespec) -> c_int {
    cvt_int(unsafe { syscall2(NR_NANOSLEEP, req as usize, rem as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn getrandom(buf: *mut c_void, buflen: size_t, flags: c_uint) -> ssize_t {
    cvt(unsafe { syscall3(NR_GETRANDOM, buf as usize, buflen, flags as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn sched_yield() -> c_int {
    cvt_int(unsafe { syscall0(NR_SCHED_YIELD) })
}

#[no_mangle]
pub unsafe extern "C" fn getcwd(buf: *mut c_char, size: size_t) -> *mut c_char {
    let ret = unsafe { syscall2(NR_GETCWD, buf as usize, size) };
    if is_errno(ret) {
        set_errno_from(ret);
        core::ptr::null_mut()
    } else {
        buf
    }
}

#[no_mangle]
pub unsafe extern "C" fn fork() -> pid_t {
    cvt_pid(unsafe { syscall0(NR_FORK) })
}

#[no_mangle]
pub unsafe extern "C" fn execve(
    path: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    cvt_int(unsafe { syscall3(NR_EXECVE, path as usize, argv as usize, envp as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn execvp(path: *const c_char, argv: *const *const c_char) -> c_int {
    unsafe { execve(path, argv, environ) }
}

#[no_mangle]
pub unsafe extern "C" fn waitpid(pid: pid_t, status: *mut c_int, options: c_int) -> pid_t {
    cvt_pid(unsafe { syscall4(NR_WAIT4, pid as usize, status as usize, options as usize, 0) })
}

#[no_mangle]
pub unsafe extern "C" fn wait4(
    pid: pid_t,
    status: *mut c_int,
    options: c_int,
    usage: *mut rusage,
) -> pid_t {
    cvt_pid(unsafe {
        syscall4(
            NR_WAIT4,
            pid as usize,
            status as usize,
            options as usize,
            usage as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn kill(pid: pid_t, sig: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_KILL, pid as usize, sig as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn killpg(pgrp: pid_t, sig: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_KILL, (-(pgrp as isize)) as usize, sig as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn getpid() -> pid_t {
    cvt_pid(unsafe { syscall0(NR_GETPID) })
}

#[no_mangle]
pub unsafe extern "C" fn gettid() -> pid_t {
    cvt_pid(unsafe { syscall0(NR_GETTID) })
}

#[no_mangle]
pub unsafe extern "C" fn getppid() -> pid_t {
    cvt_pid(unsafe { syscall0(NR_GETPPID) })
}

#[no_mangle]
pub unsafe extern "C" fn getpgrp() -> pid_t {
    cvt_pid(unsafe { syscall0(NR_GETPGRP) })
}

#[no_mangle]
pub unsafe extern "C" fn setpgid(pid: pid_t, pgid: pid_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_SETPGID, pid as usize, pgid as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn setsid() -> pid_t {
    cvt_pid(unsafe { syscall0(NR_SETSID) })
}

#[no_mangle]
pub unsafe extern "C" fn getuid() -> uid_t {
    cvt_int(unsafe { syscall0(NR_GETUID) }) as uid_t
}

#[no_mangle]
pub unsafe extern "C" fn geteuid() -> uid_t {
    cvt_int(unsafe { syscall0(NR_GETEUID) }) as uid_t
}

#[no_mangle]
pub unsafe extern "C" fn getgid() -> gid_t {
    cvt_int(unsafe { syscall0(NR_GETGID) }) as gid_t
}

#[no_mangle]
pub unsafe extern "C" fn getegid() -> gid_t {
    cvt_int(unsafe { syscall0(NR_GETEGID) }) as gid_t
}

#[no_mangle]
pub unsafe extern "C" fn setuid(uid: uid_t) -> c_int {
    cvt_int(unsafe { syscall1(NR_SETUID, uid as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn setgid(gid: gid_t) -> c_int {
    cvt_int(unsafe { syscall1(NR_SETGID, gid as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn setgroups(size: size_t, list: *const gid_t) -> c_int {
    cvt_int(unsafe { syscall2(NR_SETGROUPS, size, list as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn uname(name: *mut utsname) -> c_int {
    cvt_int(unsafe { syscall1(NR_UNAME, name as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn gethostname(name: *mut c_char, len: size_t) -> c_int {
    if name.is_null() || len == 0 {
        set_errno(EINVAL);
        return -1;
    }
    let hostname = b"ristux\0";
    let count = core::cmp::min(len, hostname.len());
    unsafe {
        core::ptr::copy_nonoverlapping(hostname.as_ptr() as *const c_char, name, count);
        *name.add(count - 1) = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn getpwnam(_name: *const c_char) -> *mut passwd {
    set_errno(ENOSYS);
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn getpwuid_r(
    _uid: uid_t,
    _pwd: *mut passwd,
    _buf: *mut c_char,
    _buflen: size_t,
    result: *mut *mut passwd,
) -> c_int {
    if !result.is_null() {
        unsafe {
            *result = core::ptr::null_mut();
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn log2f(value: f32) -> f32 {
    if value.is_nan() || value < 0.0 {
        return f32::NAN;
    }
    if value == 0.0 {
        return f32::NEG_INFINITY;
    }
    if value == f32::INFINITY {
        return f32::INFINITY;
    }

    let bits = value.to_bits();
    let exponent = ((bits >> 23) & 0xff) as i32 - 127;
    let mut normalized = f32::from_bits((bits & 0x7f_ff_ff) | (127 << 23));
    let mut fraction = 0.0f32;
    let mut bit = 0.5f32;
    let mut index = 0;
    while index < 23 {
        normalized *= normalized;
        if normalized >= 2.0 {
            normalized *= 0.5;
            fraction += bit;
        }
        bit *= 0.5;
        index += 1;
    }
    exponent as f32 + fraction
}

#[no_mangle]
pub unsafe extern "C" fn strerror_r(_errnum: c_int, buf: *mut c_char, buflen: size_t) -> c_int {
    let msg = b"Ristux error\0";
    if buf.is_null() || buflen == 0 {
        return -1;
    }
    let mut index = 0usize;
    while index + 1 < buflen && index < msg.len() {
        unsafe {
            *buf.add(index) = msg[index] as c_char;
        }
        if msg[index] == 0 {
            return 0;
        }
        index += 1;
    }
    unsafe {
        let nul_index = if index < buflen { index } else { buflen - 1 };
        *buf.add(nul_index) = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn free(_ptr: *mut c_void) {}

#[no_mangle]
pub unsafe extern "C" fn dlopen(_filename: *const c_char, _flag: c_int) -> *mut c_void {
    set_errno(ENOSYS);
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn dlerror() -> *mut c_char {
    b"Ristux dynamic loading unsupported\0".as_ptr() as *mut c_char
}

#[no_mangle]
pub unsafe extern "C" fn dlsym(_handle: *mut c_void, _symbol: *const c_char) -> *mut c_void {
    set_errno(ENOSYS);
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn dlclose(_handle: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn dladdr(_addr: *const c_void, info: *mut Dl_info) -> c_int {
    if !info.is_null() {
        unsafe {
            (*info).dli_fname = core::ptr::null();
            (*info).dli_fbase = core::ptr::null_mut();
            (*info).dli_sname = core::ptr::null();
            (*info).dli_saddr = core::ptr::null_mut();
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int {
    cvt_int(unsafe { syscall3(NR_SOCKET, domain as usize, kind as usize, protocol as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn connect(fd: c_int, address: *const sockaddr, length: socklen_t) -> c_int {
    cvt_int(unsafe { syscall3(NR_CONNECT, fd as usize, address as usize, length as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn bind(fd: c_int, address: *const sockaddr, length: socklen_t) -> c_int {
    cvt_int(unsafe { syscall3(NR_BIND, fd as usize, address as usize, length as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn listen(fd: c_int, backlog: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_LISTEN, fd as usize, backlog as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn accept(
    fd: c_int,
    address: *mut sockaddr,
    length: *mut socklen_t,
) -> c_int {
    cvt_int(unsafe { syscall3(NR_ACCEPT, fd as usize, address as usize, length as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn getsockname(
    fd: c_int,
    address: *mut sockaddr,
    length: *mut socklen_t,
) -> c_int {
    cvt_int(unsafe {
        syscall3(
            NR_GETSOCKNAME,
            fd as usize,
            address as usize,
            length as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn setsockopt(
    fd: c_int,
    level: c_int,
    option: c_int,
    value: *const c_void,
    length: socklen_t,
) -> c_int {
    cvt_int(unsafe {
        syscall5(
            NR_SETSOCKOPT,
            fd as usize,
            level as usize,
            option as usize,
            value as usize,
            length as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn getsockopt(
    fd: c_int,
    level: c_int,
    option: c_int,
    value: *mut c_void,
    length: *mut socklen_t,
) -> c_int {
    cvt_int(unsafe {
        syscall5(
            NR_GETSOCKOPT,
            fd as usize,
            level as usize,
            option as usize,
            value as usize,
            length as usize,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn send(
    fd: c_int,
    buffer: *const c_void,
    length: size_t,
    flags: c_int,
) -> ssize_t {
    cvt(unsafe {
        syscall6(
            NR_SENDTO,
            fd as usize,
            buffer as usize,
            length,
            flags as usize,
            0,
            0,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn recv(
    fd: c_int,
    buffer: *mut c_void,
    length: size_t,
    flags: c_int,
) -> ssize_t {
    cvt(unsafe {
        syscall6(
            NR_RECVFROM,
            fd as usize,
            buffer as usize,
            length,
            flags as usize,
            0,
            0,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn shutdown(fd: c_int, how: c_int) -> c_int {
    cvt_int(unsafe { syscall2(NR_SHUTDOWN, fd as usize, how as usize) })
}

#[no_mangle]
pub unsafe extern "C" fn getaddrinfo(
    _node: *const c_char,
    _service: *const c_char,
    _hints: *const addrinfo,
    res: *mut *mut addrinfo,
) -> c_int {
    if !res.is_null() {
        unsafe {
            *res = core::ptr::null_mut();
        }
    }
    EAI_NONAME
}

#[no_mangle]
pub unsafe extern "C" fn freeaddrinfo(_res: *mut addrinfo) {}

#[no_mangle]
pub unsafe extern "C" fn gai_strerror(_errcode: c_int) -> *const c_char {
    b"Ristux name resolution unavailable\0".as_ptr() as *const c_char
}

#[no_mangle]
pub unsafe extern "C" fn sysconf(name: c_int) -> c_long {
    match name {
        SC_CLK_TCK => 100,
        SC_OPEN_MAX => 256,
        SC_PAGESIZE => 4096,
        SC_NPROCESSORS_CONF | SC_NPROCESSORS_ONLN => 4,
        _ => {
            set_errno(EINVAL);
            -1
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sigaction(
    signum: c_int,
    act: *const sigaction_t,
    oldact: *mut sigaction_t,
) -> c_int {
    let mut kernel_new = KernelSigAction {
        handler: 0,
        mask: 0,
        flags: 0,
        _pad: 0,
    };
    let kernel_new_ptr = if act.is_null() {
        0
    } else {
        let act_ref = unsafe { &*act };
        let core::option::Option::Some(flags) = libc_signal_flags_to_kernel(act_ref.sa_flags)
        else {
            set_errno(EINVAL);
            return -1;
        };
        kernel_new.handler = act_ref.sa_sigaction as usize;
        kernel_new.mask = act_ref.sa_mask as u64;
        kernel_new.flags = flags;
        &kernel_new as *const KernelSigAction as usize
    };
    let mut kernel_old = KernelSigAction {
        handler: 0,
        mask: 0,
        flags: 0,
        _pad: 0,
    };
    let kernel_old_ptr = if oldact.is_null() {
        0
    } else {
        &mut kernel_old as *mut KernelSigAction as usize
    };
    let rc = cvt_int(unsafe {
        syscall3(
            NR_RT_SIGACTION,
            signum as usize,
            kernel_new_ptr,
            kernel_old_ptr,
        )
    });
    if rc == 0 && !oldact.is_null() {
        unsafe {
            (*oldact).sa_sigaction = kernel_old.handler as sighandler_t;
            (*oldact).sa_flags = kernel_signal_flags_to_libc(kernel_old.flags);
            (*oldact).sa_restorer = core::option::Option::None;
            (*oldact).sa_mask = kernel_old.mask as sigset_t;
        }
    }
    rc
}

#[no_mangle]
pub unsafe extern "C" fn signal(signum: c_int, handler: sighandler_t) -> sighandler_t {
    let act = sigaction_t {
        sa_sigaction: handler,
        sa_flags: 0,
        sa_restorer: core::option::Option::None,
        sa_mask: 0,
    };
    let mut oldact = sigaction_t {
        sa_sigaction: !0usize as sighandler_t,
        sa_flags: 0,
        sa_restorer: core::option::Option::None,
        sa_mask: 0,
    };
    if unsafe { sigaction(signum, &act, &mut oldact) } == 0 {
        oldact.sa_sigaction
    } else {
        !0usize as sighandler_t
    }
}

#[inline]
unsafe fn valid_env_name(name: *const c_char) -> bool {
    if name.is_null() {
        return false;
    }
    let mut offset = 0usize;
    loop {
        let byte = unsafe { *name.add(offset) };
        if byte == 0 {
            return offset != 0;
        }
        if byte == b'=' as c_char {
            return false;
        }
        offset += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn getenv(name: *const c_char) -> *mut c_char {
    if !unsafe { valid_env_name(name) } || unsafe { environ.is_null() } {
        return core::ptr::null_mut();
    }
    let mut entry = unsafe { environ };
    while !unsafe { (*entry).is_null() } {
        let value = unsafe { *entry };
        let mut offset = 0usize;
        while unsafe { *name.add(offset) } != 0
            && unsafe { *value.add(offset) } == unsafe { *name.add(offset) }
        {
            offset += 1;
        }
        if unsafe { *name.add(offset) } == 0 && unsafe { *value.add(offset) } == b'=' as c_char {
            return unsafe { value.add(offset + 1) as *mut c_char };
        }
        entry = unsafe { entry.add(1) };
    }
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn setenv(
    name: *const c_char,
    value: *const c_char,
    _overwrite: c_int,
) -> c_int {
    if value.is_null() || !unsafe { valid_env_name(name) } {
        unsafe {
            ERRNO = EINVAL;
        }
        return -1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn unsetenv(name: *const c_char) -> c_int {
    if !unsafe { valid_env_name(name) } {
        unsafe {
            ERRNO = EINVAL;
        }
        return -1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn clearenv() -> c_int {
    unsafe {
        environ = core::ptr::null();
    }
    0
}

extern "C" fn pthread_start(arg: *mut c_void) -> ! {
    let start = arg as *mut PthreadStart;
    let record = unsafe { core::ptr::read(start) };
    let _ = (record.start)(record.arg);
    unsafe { thread_exit(0) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_self() -> pthread_t {
    unsafe { gettid() as usize as pthread_t }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_equal(left: pthread_t, right: pthread_t) -> c_int {
    (left == right) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn pthread_attr_init(attr: *mut pthread_attr_t) -> c_int {
    let rc = unsafe { zero_pthread_object(attr) };
    if rc != 0 {
        return rc;
    }
    unsafe {
        pthread_attr_set_stack_size_raw(attr, DEFAULT_PTHREAD_STACK_SIZE);
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_attr_destroy(attr: *mut pthread_attr_t) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_attr_getstacksize(
    attr: *const pthread_attr_t,
    stacksize: *mut size_t,
) -> c_int {
    if attr.is_null() || stacksize.is_null() {
        return EINVAL;
    }
    unsafe {
        *stacksize = pthread_attr_stack_size(attr);
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setstacksize(
    attr: *mut pthread_attr_t,
    stack_size: size_t,
) -> c_int {
    let stack_size = match align_stack_size(stack_size) {
        core::option::Option::Some(stack_size) => stack_size,
        core::option::Option::None => return EINVAL,
    };
    if attr.is_null() {
        return EINVAL;
    }
    unsafe {
        pthread_attr_set_stack_size_raw(attr, stack_size);
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_attr_getguardsize(
    attr: *const pthread_attr_t,
    guardsize: *mut size_t,
) -> c_int {
    if attr.is_null() || guardsize.is_null() {
        return EINVAL;
    }
    unsafe {
        *guardsize = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setguardsize(
    attr: *mut pthread_attr_t,
    _guardsize: size_t,
) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setdetachstate(
    attr: *mut pthread_attr_t,
    _state: c_int,
) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_create(
    tid: *mut pthread_t,
    attr: *const pthread_attr_t,
    start: extern "C" fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
) -> c_int {
    if tid.is_null() {
        return EINVAL;
    }
    let stack_len = match align_stack_size(unsafe { pthread_attr_stack_size(attr) }) {
        core::option::Option::Some(stack_len) => stack_len,
        core::option::Option::None => return EINVAL,
    };
    let stack_base = cvt_ptr(unsafe {
        syscall6(
            NR_MMAP,
            0,
            stack_len,
            RISTUX_PROT_READ_WRITE as usize,
            RISTUX_MAP_PRIVATE_ANON as usize,
            usize::MAX,
            0,
        )
    });
    if stack_base == !0usize as *mut c_void {
        return unsafe { ERRNO };
    }

    let top = stack_base as usize + stack_len;
    let start_ptr = (top - core::mem::size_of::<PthreadStart>()) & !0xfusize;
    // Ristux enters the thread entrypoint directly instead of through a call
    // instruction. SysV x86_64 callees still expect the call-site return
    // address adjustment, so synthesize an entry stack with rsp % 16 == 8.
    let child_stack = start_ptr - core::mem::size_of::<usize>();
    let start_record = start_ptr as *mut PthreadStart;
    unsafe {
        core::ptr::write(
            start_record,
            PthreadStart {
                start,
                arg,
                clear_tid: 0,
            },
        );
    }
    let clear_tid = unsafe { core::ptr::addr_of_mut!((*start_record).clear_tid) } as usize;

    let pid = unsafe {
        syscall5(
            NR_RISTUX_THREAD_CREATE,
            pthread_start as usize,
            start_record as usize,
            child_stack,
            0,
            clear_tid,
        )
    };
    if is_errno(pid) {
        let errno = (-pid) as c_int;
        let _ = unsafe { munmap(stack_base, stack_len) };
        return errno;
    }
    unsafe {
        *tid = pid as usize as pthread_t;
        remember_pthread_clear_tid(pid as pid_t, clear_tid);
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_join(thread: pthread_t, value: *mut *mut c_void) -> c_int {
    if thread.is_null() {
        return EINVAL;
    }
    let mut status = 0;
    let pid = thread as usize as pid_t;
    let clear_tid = unsafe { lookup_pthread_clear_tid(pid) };
    if clear_tid != 0 {
        loop {
            let current = unsafe { *(clear_tid as *const u32) };
            if current == 0 {
                break;
            }
            let waited = unsafe {
                syscall4(
                    NR_FUTEX,
                    clear_tid,
                    FUTEX_WAIT | FUTEX_PRIVATE_FLAG,
                    current as usize,
                    0,
                )
            };
            if is_errno(waited) {
                let errno = (-waited) as c_int;
                if errno != EAGAIN {
                    return errno;
                }
            }
        }
    }
    let waited = unsafe { waitpid(pid, &mut status as *mut c_int, 0) };
    if waited < 0 {
        unsafe { ERRNO }
    } else {
        unsafe {
            retire_pthread_slot(pid);
        }
        if !value.is_null() {
            unsafe {
                *value = core::ptr::null_mut();
            }
        }
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_detach(_thread: pthread_t) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_exit(_value: *mut c_void) -> ! {
    unsafe { thread_exit(0) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_key_create(
    key: *mut pthread_key_t,
    _dtor: core::option::Option<unsafe extern "C" fn(*mut c_void)>,
) -> c_int {
    if key.is_null() {
        return EINVAL;
    }
    unsafe {
        let next = NEXT_PTHREAD_KEY;
        if next as usize >= MAX_PTHREAD_KEYS {
            return EAGAIN;
        }
        NEXT_PTHREAD_KEY = next + 1;
        *key = next;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_key_delete(key: pthread_key_t) -> c_int {
    if key as usize >= MAX_PTHREAD_KEYS {
        return EINVAL;
    }
    lock_pthread_pool();
    for slot in 0..MAX_PTHREAD_THREADS {
        unsafe {
            PTHREAD_VALUES[slot][key as usize] = core::ptr::null();
        }
    }
    unlock_pthread_pool();
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_getspecific(key: pthread_key_t) -> *mut c_void {
    if key as usize >= MAX_PTHREAD_KEYS {
        core::ptr::null_mut()
    } else {
        lock_pthread_pool();
        let slot = match unsafe { pthread_thread_slot_locked() } {
            core::option::Option::Some(slot) => slot,
            core::option::Option::None => {
                unlock_pthread_pool();
                return core::ptr::null_mut();
            }
        };
        let value = unsafe { PTHREAD_VALUES[slot][key as usize] as *mut c_void };
        unlock_pthread_pool();
        value
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_setspecific(key: pthread_key_t, value: *const c_void) -> c_int {
    if key as usize >= MAX_PTHREAD_KEYS {
        return EINVAL;
    }
    lock_pthread_pool();
    let slot = match unsafe { pthread_thread_slot_locked() } {
        core::option::Option::Some(slot) => slot,
        core::option::Option::None => {
            unlock_pthread_pool();
            return EAGAIN;
        }
    };
    unsafe {
        PTHREAD_VALUES[slot][key as usize] = value;
    }
    unlock_pthread_pool();
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_init(
    lock: *mut pthread_mutex_t,
    _attr: *const pthread_mutexattr_t,
) -> c_int {
    unsafe { zero_pthread_object(lock) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_destroy(lock: *mut pthread_mutex_t) -> c_int {
    if lock.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_lock(lock: *mut pthread_mutex_t) -> c_int {
    if lock.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int {
    unsafe { pthread_mutex_lock(lock) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int {
    if lock.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutexattr_init(attr: *mut pthread_mutexattr_t) -> c_int {
    unsafe { zero_pthread_object(attr) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutexattr_destroy(attr: *mut pthread_mutexattr_t) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutexattr_settype(
    attr: *mut pthread_mutexattr_t,
    _kind: c_int,
) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_init(
    cond: *mut pthread_cond_t,
    _attr: *const pthread_condattr_t,
) -> c_int {
    unsafe { zero_pthread_object(cond) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_wait(
    cond: *mut pthread_cond_t,
    lock: *mut pthread_mutex_t,
) -> c_int {
    if cond.is_null() || lock.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_timedwait(
    cond: *mut pthread_cond_t,
    lock: *mut pthread_mutex_t,
    _abstime: *const timespec,
) -> c_int {
    if cond.is_null() || lock.is_null() {
        EINVAL
    } else {
        ETIMEDOUT
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_signal(cond: *mut pthread_cond_t) -> c_int {
    if cond.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_broadcast(cond: *mut pthread_cond_t) -> c_int {
    unsafe { pthread_cond_signal(cond) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_destroy(cond: *mut pthread_cond_t) -> c_int {
    if cond.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_init(attr: *mut pthread_condattr_t) -> c_int {
    unsafe { zero_pthread_object(attr) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_destroy(attr: *mut pthread_condattr_t) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_setclock(
    attr: *mut pthread_condattr_t,
    _clock_id: clockid_t,
) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_init(
    lock: *mut pthread_rwlock_t,
    _attr: *const pthread_rwlockattr_t,
) -> c_int {
    unsafe { zero_pthread_object(lock) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_destroy(lock: *mut pthread_rwlock_t) -> c_int {
    if lock.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_rdlock(lock: *mut pthread_rwlock_t) -> c_int {
    if lock.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_tryrdlock(lock: *mut pthread_rwlock_t) -> c_int {
    unsafe { pthread_rwlock_rdlock(lock) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_wrlock(lock: *mut pthread_rwlock_t) -> c_int {
    unsafe { pthread_rwlock_rdlock(lock) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_trywrlock(lock: *mut pthread_rwlock_t) -> c_int {
    unsafe { pthread_rwlock_rdlock(lock) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_unlock(lock: *mut pthread_rwlock_t) -> c_int {
    if lock.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlockattr_init(attr: *mut pthread_rwlockattr_t) -> c_int {
    unsafe { zero_pthread_object(attr) }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_rwlockattr_destroy(attr: *mut pthread_rwlockattr_t) -> c_int {
    if attr.is_null() {
        EINVAL
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_sigmask(
    how: c_int,
    set: *const sigset_t,
    oldset: *mut sigset_t,
) -> c_int {
    cvt_pthread(unsafe {
        syscall4(
            NR_RT_SIGPROCMASK,
            how as usize,
            set as usize,
            oldset as usize,
            core::mem::size_of::<sigset_t>(),
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn pthread_atfork(
    _prepare: core::option::Option<unsafe extern "C" fn()>,
    _parent: core::option::Option<unsafe extern "C" fn()>,
    _child: core::option::Option<unsafe extern "C" fn()>,
) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cancel(_thread: pthread_t) -> c_int {
    ENOSYS
}

#[no_mangle]
pub unsafe extern "C" fn pthread_kill(_thread: pthread_t, _sig: c_int) -> c_int {
    ENOSYS
}

#[no_mangle]
pub unsafe extern "C" fn _exit(status: c_int) -> ! {
    let _ = unsafe { syscall1(NR_EXIT_GROUP, status as usize) };
    loop {
        core::hint::spin_loop();
    }
}

unsafe fn thread_exit(status: c_int) -> ! {
    let _ = unsafe { syscall1(NR_EXIT, status as usize) };
    loop {
        core::hint::spin_loop();
    }
}

#[no_mangle]
pub unsafe extern "C" fn exit(status: c_int) -> ! {
    unsafe { _exit(status) }
}

#[no_mangle]
pub unsafe extern "C" fn abort() -> ! {
    unsafe { _exit(134) }
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_Backtrace(_cb: *mut c_void, _arg: *mut c_void) -> c_int {
    5
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_DeleteException(_exception: *mut c_void) {}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_FindEnclosingFunction(_pc: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_GetCFA(_context: *mut c_void) -> usize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_GetDataRelBase(_context: *mut c_void) -> usize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_GetIP(_context: *mut c_void) -> usize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_GetIPInfo(
    _context: *mut c_void,
    ip_before_insn: *mut c_int,
) -> usize {
    if !ip_before_insn.is_null() {
        unsafe {
            *ip_before_insn = 0;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_GetLanguageSpecificData(_context: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_GetRegionStart(_context: *mut c_void) -> usize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_GetTextRelBase(_context: *mut c_void) -> usize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_RaiseException(_exception: *mut c_void) -> c_int {
    5
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_Resume(_exception: *mut c_void) -> ! {
    unsafe { abort() }
}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_SetGR(_context: *mut c_void, _index: c_int, _value: usize) {}

#[no_mangle]
pub unsafe extern "C" fn _Unwind_SetIP(_context: *mut c_void, _value: usize) {}
