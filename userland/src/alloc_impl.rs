//! Bump allocator backed by `brk`.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;

use crate::sys;

const HEAP_GROW: usize = 64 * 1024;

struct BumpAllocator {
    inner: UnsafeCell<BumpState>,
}

struct BumpState {
    base: usize,
    next: usize,
    end: usize,
    init: bool,
}

unsafe impl Sync for BumpAllocator {}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator {
    inner: UnsafeCell::new(BumpState {
        base: 0,
        next: 0,
        end: 0,
        init: false,
    }),
};

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let state = unsafe { &mut *self.inner.get() };
        if !state.init {
            let current = sys::brk(0);
            if current < 0 {
                return ptr::null_mut();
            }
            state.base = current as usize;
            state.next = current as usize;
            state.end = current as usize;
            state.init = true;
        }

        let align = layout.align().max(8);
        let size = layout.size();
        let aligned = (state.next + align - 1) & !(align - 1);
        let new_next = match aligned.checked_add(size) {
            Some(v) => v,
            None => return ptr::null_mut(),
        };

        if new_next > state.end {
            let grow = ((new_next - state.end + HEAP_GROW - 1) / HEAP_GROW) * HEAP_GROW;
            let new_end = state.end + grow;
            let res = sys::brk(new_end);
            if res < 0 || (res as usize) < new_end {
                return ptr::null_mut();
            }
            state.end = new_end;
        }

        state.next = new_next;
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: no individual frees.
    }
}
