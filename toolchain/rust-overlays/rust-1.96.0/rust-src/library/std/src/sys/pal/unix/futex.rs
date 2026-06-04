#[cfg(target_os = "ristux")]
mod ristux {
    use crate::ptr::null;
    use crate::sync::atomic::{Atomic, Ordering::Relaxed};
    use crate::time::Duration;

    pub type Futex = Atomic<Primitive>;
    pub type Primitive = u32;
    pub type SmallFutex = Atomic<SmallPrimitive>;
    pub type SmallPrimitive = u32;

    const NR_FUTEX: usize = 202;
    const FUTEX_WAIT: usize = 0;
    const FUTEX_WAKE: usize = 1;
    const FUTEX_PRIVATE_FLAG: usize = 0x80;
    const ETIMEDOUT: isize = 110;
    const SYSCALL_TRAMPOLINE_BASE: usize = 0x4000_0000;
    const SYSCALL_TRAMPOLINE_STRIDE: usize = 0x20;

    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
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

    fn timeout_timespec(timeout: Option<Duration>) -> Option<Timespec> {
        let duration = timeout?;
        if duration.as_secs() > i64::MAX as u64 {
            return None;
        }
        Some(Timespec {
            tv_sec: duration.as_secs() as i64,
            tv_nsec: duration.subsec_nanos() as i64,
        })
    }

    pub fn futex_wait(futex: &Atomic<u32>, expected: u32, timeout: Option<Duration>) -> bool {
        if futex.load(Relaxed) != expected {
            return true;
        }
        let timespec = timeout_timespec(timeout);
        let timeout_ptr = timespec.as_ref().map_or(null(), |t| t as *const Timespec);
        let rc = unsafe {
            syscall4(
                NR_FUTEX,
                futex.as_ptr() as usize,
                FUTEX_WAIT | FUTEX_PRIVATE_FLAG,
                expected as usize,
                timeout_ptr as usize,
            )
        };
        rc != -ETIMEDOUT
    }

    pub fn futex_wake(futex: &Atomic<u32>) -> bool {
        unsafe {
            syscall4(
                NR_FUTEX,
                futex.as_ptr() as usize,
                FUTEX_WAKE | FUTEX_PRIVATE_FLAG,
                1,
                0,
            ) > 0
        }
    }

    pub fn futex_wake_all(futex: &Atomic<u32>) {
        unsafe {
            let _ = syscall4(
                NR_FUTEX,
                futex.as_ptr() as usize,
                FUTEX_WAKE | FUTEX_PRIVATE_FLAG,
                i32::MAX as usize,
                0,
            );
        }
    }
}

#[cfg(target_os = "ristux")]
pub use ristux::*;

#[cfg(all(
    not(target_os = "ristux"),
    any(
        target_os = "linux",
        target_os = "android",
        all(target_os = "emscripten", target_feature = "atomics"),
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "dragonfly",
        target_os = "fuchsia",
    )
))]
mod upstream {
    include!("futex_upstream.rs");
}

#[cfg(all(
    not(target_os = "ristux"),
    any(
        target_os = "linux",
        target_os = "android",
        all(target_os = "emscripten", target_feature = "atomics"),
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "dragonfly",
        target_os = "fuchsia",
    )
))]
pub use upstream::*;
