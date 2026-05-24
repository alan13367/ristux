use alloc::{boxed::Box, vec::Vec};
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

const HEAP_SIZE: usize = 128 * 1024;

#[repr(C, align(4096))]
struct HeapSpace(UnsafeCell<[u8; HEAP_SIZE]>);

unsafe impl Sync for HeapSpace {}

static HEAP_SPACE: HeapSpace = HeapSpace(UnsafeCell::new([0; HEAP_SIZE]));

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator::new();

pub struct BumpAllocator {
    start: AtomicUsize,
    next: AtomicUsize,
    end: AtomicUsize,
    initialized: AtomicBool,
}

impl BumpAllocator {
    pub const fn new() -> Self {
        Self {
            start: AtomicUsize::new(0),
            next: AtomicUsize::new(0),
            end: AtomicUsize::new(0),
            initialized: AtomicBool::new(false),
        }
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.initialized.load(Ordering::Acquire) {
            return ptr::null_mut();
        }

        let mut current = self.next.load(Ordering::Relaxed);
        loop {
            let alloc_start = align_up(current, layout.align());
            let Some(alloc_end) = alloc_start.checked_add(layout.size()) else {
                return ptr::null_mut();
            };

            if alloc_end > self.end.load(Ordering::Relaxed) {
                return ptr::null_mut();
            }

            match self.next.compare_exchange(
                current,
                alloc_end,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return alloc_start as *mut u8,
                Err(next) => current = next,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

pub fn init() {
    let start = HEAP_SPACE.0.get() as *mut u8 as usize;
    let end = start + HEAP_SIZE;

    ALLOCATOR.start.store(start, Ordering::Relaxed);
    ALLOCATOR.next.store(start, Ordering::Relaxed);
    ALLOCATOR.end.store(end, Ordering::Relaxed);
    ALLOCATOR.initialized.store(true, Ordering::Release);

    crate::println!("Kernel heap: {:#x}..{:#x}", start, end);
}

pub fn self_test() {
    let value = Box::new(42_u64);
    let mut list: Vec<Box<u64>> = Vec::new();
    list.push(value);

    if *list[0] != 42 {
        panic!("Box/Vec heap self-test returned unexpected value");
    }

    let mut bytes = Vec::new();
    for byte in 0_u8..32 {
        bytes.push(byte);
    }

    if bytes.len() != 32 || bytes[31] != 31 {
        panic!("Vec heap growth self-test failed");
    }

    crate::println!("Kernel heap self-test passed with Box and Vec.");
}

#[alloc_error_handler]
fn alloc_error(layout: Layout) -> ! {
    panic!(
        "kernel heap allocation failed: {} bytes aligned to {}",
        layout.size(),
        layout.align()
    );
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

