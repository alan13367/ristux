use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::arch::x86_64::instructions;

pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let irq_enabled = instructions::interrupts_enabled();
        if irq_enabled {
            instructions::disable_interrupts();
        }

        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }

        SpinLockGuard {
            lock: self,
            restore_interrupts: irq_enabled,
        }
    }
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    restore_interrupts: bool,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        if self.restore_interrupts {
            instructions::enable_interrupts();
        }
    }
}

pub fn self_test() {
    static LOCK: SpinLock<u64> = SpinLock::new(0);
    {
        let mut guard = LOCK.lock();
        *guard = 42;
    }
    {
        let guard = LOCK.lock();
        if *guard != 42 {
            panic!("spinlock self-test failed");
        }
    }
    crate::println!("Interrupt-safe spinlock self-test passed.");
}
