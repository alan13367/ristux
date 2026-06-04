use crate::{c_char, c_int, c_void, iovec, size_t, ssize_t};

const NR_READ: usize = 0;
const NR_WRITE: usize = 1;
const NR_CLOSE: usize = 3;
const NR_EXIT: usize = 60;
const NR_GETCWD: usize = 79;
const NR_READV: usize = 19;
const NR_WRITEV: usize = 20;
const SYSCALL_TRAMPOLINE_BASE: usize = 0x4000_0000;
const SYSCALL_TRAMPOLINE_STRIDE: usize = 0x20;

static mut ERRNO: c_int = 0;

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
        core::mem::transmute::<usize, Syscall1>(
            SYSCALL_TRAMPOLINE_BASE + SYSCALL_TRAMPOLINE_STRIDE,
        )
    };
    unsafe { f(nr, a0) }
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
fn cvt(ret: isize) -> ssize_t {
    if (-4095..0).contains(&ret) {
        unsafe {
            ERRNO = (-ret) as c_int;
        }
        -1
    } else {
        ret as ssize_t
    }
}

#[no_mangle]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t {
    cvt(unsafe { syscall3(NR_READ, fd as usize, buf as usize, count) })
}

#[no_mangle]
pub unsafe extern "C" fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t {
    cvt(unsafe { syscall3(NR_WRITE, fd as usize, buf as usize, count) })
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
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    cvt(unsafe { syscall1(NR_CLOSE, fd as usize) }) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn getcwd(buf: *mut c_char, size: size_t) -> *mut c_char {
    let ret = unsafe { syscall2(NR_GETCWD, buf as usize, size) };
    if (-4095..0).contains(&ret) {
        unsafe {
            ERRNO = (-ret) as c_int;
        }
        core::ptr::null_mut()
    } else {
        buf
    }
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
pub unsafe extern "C" fn _exit(status: c_int) -> ! {
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
