#[cfg(target_os = "ristux")]
mod ristux {
    use crate::alloc::{GlobalAlloc, Layout, System};
    use crate::ptr;
    use crate::sync::atomic::{AtomicUsize, Ordering};

    const NR_BRK: usize = 12;
    const SYSCALL_TRAMPOLINE_BASE: usize = 0x4000_0000;
    const SYSCALL_TRAMPOLINE_STRIDE: usize = 0x20;

    static HEAP_END: AtomicUsize = AtomicUsize::new(0);

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
    fn brk(addr: usize) -> usize {
        unsafe { syscall1(NR_BRK, addr) as usize }
    }

    #[inline]
    fn align_up(value: usize, align: usize) -> Option<usize> {
        let align = align.max(1);
        value.checked_add(align - 1).map(|v| v & !(align - 1))
    }

    #[stable(feature = "alloc_system_type", since = "1.28.0")]
    unsafe impl GlobalAlloc for System {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let size = layout.size().max(1);
            loop {
                let observed = HEAP_END.load(Ordering::Acquire);
                let current = if observed == 0 { brk(0) } else { observed };
                let Some(start) = align_up(current, layout.align()) else {
                    return ptr::null_mut();
                };
                let Some(end) = start.checked_add(size) else {
                    return ptr::null_mut();
                };
                let new_break = brk(end);
                if new_break < end {
                    return ptr::null_mut();
                }
                if HEAP_END
                    .compare_exchange(observed, end, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return ptr::with_exposed_provenance_mut::<u8>(start);
                }
            }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { self.alloc(layout) };
            if !ptr.is_null() {
                unsafe { ptr::write_bytes(ptr, 0, layout.size()) };
            }
            ptr
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let new_layout = match Layout::from_size_align(new_size, layout.align()) {
                Ok(layout) => layout,
                Err(_) => return ptr::null_mut(),
            };
            let new_ptr = unsafe { self.alloc(new_layout) };
            if !new_ptr.is_null() {
                unsafe {
                    ptr::copy_nonoverlapping(ptr, new_ptr, layout.size().min(new_size));
                }
            }
            new_ptr
        }
    }
}

#[cfg(not(target_os = "ristux"))]
include!("unix_upstream.rs");
