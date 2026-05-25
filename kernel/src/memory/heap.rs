use alloc::{boxed::Box, vec::Vec};
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::{config, sync::spinlock::SpinLock};

#[repr(C, align(4096))]
struct HeapSpace(UnsafeCell<[u8; config::KERNEL_HEAP_SIZE]>);

unsafe impl Sync for HeapSpace {}

static HEAP_SPACE: HeapSpace = HeapSpace(UnsafeCell::new([0; config::KERNEL_HEAP_SIZE]));

#[global_allocator]
static ALLOCATOR: LinkedListAllocator = LinkedListAllocator::new();

const HEADER_SIZE: usize = core::mem::size_of::<FreeBlock>();

#[repr(C)]
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

pub struct LinkedListAllocator {
    head: SpinLock<*mut FreeBlock>,
    start: AtomicUsize,
    end: AtomicUsize,
    initialized: AtomicBool,
}

unsafe impl Sync for LinkedListAllocator {}

impl LinkedListAllocator {
    pub const fn new() -> Self {
        Self {
            head: SpinLock::new(core::ptr::null_mut()),
            start: AtomicUsize::new(0),
            end: AtomicUsize::new(0),
            initialized: AtomicBool::new(false),
        }
    }

    unsafe fn init_free_list(&self, start: usize, size: usize) {
        let head = start as *mut FreeBlock;
        unsafe {
            (*head).size = size;
            (*head).next = ptr::null_mut();
        }
        *self.head.lock() = head;
    }

    unsafe fn alloc_from_free_list(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let size = layout.size();
            let align = layout.align();
            let total = align_up(HEADER_SIZE + size, align);

            let mut head_ptr = self.head.lock();
            let mut current = *head_ptr;
            let mut prev: *mut FreeBlock = ptr::null_mut();

            while !current.is_null() {
                let block_size = (*current).size;

                if block_size >= total {
                    let remainder = block_size - total;
                    if remainder >= HEADER_SIZE {
                        let remainder_ptr = (current as *mut u8).add(total) as *mut FreeBlock;
                        (*remainder_ptr).size = remainder;
                        (*remainder_ptr).next = (*current).next;
                        if prev.is_null() {
                            *head_ptr = remainder_ptr;
                        } else {
                            (*prev).next = remainder_ptr;
                        }
                    } else if prev.is_null() {
                        *head_ptr = (*current).next;
                    } else {
                        (*prev).next = (*current).next;
                    }

                    return (current as *mut u8).add(HEADER_SIZE);
                }

                prev = current;
                current = (*current).next;
            }

            ptr::null_mut()
        }
    }

    unsafe fn dealloc_to_free_list(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            let block = ptr.sub(HEADER_SIZE) as *mut FreeBlock;
            let block_size = align_up(HEADER_SIZE + layout.size(), layout.align());

            (*block).size = block_size;
            (*block).next = ptr::null_mut();

            let mut head_ptr = self.head.lock();
            if (*head_ptr).is_null() || block < *head_ptr {
                (*block).next = *head_ptr;
                *head_ptr = block;
            } else {
                let mut current = *head_ptr;
                while !(*current).next.is_null() && (*current).next < block {
                    current = (*current).next;
                }
                (*block).next = (*current).next;
                (*current).next = block;
            }
            *head_ptr = self.coalesce_head(*head_ptr);
        }
    }

    unsafe fn coalesce_head(&self, head: *mut FreeBlock) -> *mut FreeBlock {
        if head.is_null() {
            return head;
        }

        let mut current = head;
        while !unsafe { (*current).next.is_null() } {
            let end = current as usize + unsafe { (*current).size };
            let next = unsafe { (*current).next };
            if end == next as usize {
                unsafe {
                    (*current).size += (*next).size;
                    (*current).next = (*next).next;
                }
            } else {
                current = next;
            }
        }

        head
    }

    fn used_bytes(&self) -> usize {
        let start = self.start.load(Ordering::Relaxed);
        let end = self.end.load(Ordering::Relaxed);
        end.saturating_sub(start).saturating_sub(self.free_bytes())
    }

    fn free_bytes(&self) -> usize {
        let head = *self.head.lock();
        let mut current = head;
        let mut free = 0;

        while !current.is_null() {
            free += unsafe { (*current).size };
            current = unsafe { (*current).next };
        }

        free
    }
}

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.initialized.load(Ordering::Acquire) {
            return ptr::null_mut();
        }

        unsafe { self.alloc_from_free_list(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !self.initialized.load(Ordering::Acquire) || ptr.is_null() {
            return;
        }

        unsafe {
            self.dealloc_to_free_list(ptr, layout);
        }
    }
}

pub fn init() {
    let start = HEAP_SPACE.0.get() as *mut u8 as usize;
    let end = start + config::KERNEL_HEAP_SIZE;

    ALLOCATOR.start.store(start, Ordering::Relaxed);
    ALLOCATOR.end.store(end, Ordering::Relaxed);
    unsafe {
        ALLOCATOR.init_free_list(start, config::KERNEL_HEAP_SIZE);
    }
    ALLOCATOR.initialized.store(true, Ordering::Release);

    crate::println!("Kernel heap: {:#x}..{:#x}", start, end);
}

pub fn stats() -> HeapStats {
    HeapStats {
        start: ALLOCATOR.start.load(Ordering::Relaxed),
        end: ALLOCATOR.end.load(Ordering::Relaxed),
        used_bytes: ALLOCATOR.used_bytes(),
        free_bytes: ALLOCATOR.free_bytes(),
    }
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

    let layout = Layout::from_size_align(64, 8).unwrap();
    let first = unsafe { ALLOCATOR.alloc(layout) };
    if first.is_null() {
        panic!("heap reuse self-test alloc failed");
    }
    unsafe {
        ALLOCATOR.dealloc(first, layout);
    }
    let second = unsafe { ALLOCATOR.alloc(layout) };
    if second.is_null() {
        panic!("heap reuse self-test re-alloc failed");
    }
    unsafe {
        ALLOCATOR.dealloc(second, layout);
    }

    for _ in 0..64 {
        let mut v: Vec<u64> = Vec::new();
        for i in 0..32 {
            v.push(i);
        }
        drop(v);
    }

    crate::println!("Kernel heap self-test passed with Box, Vec, and free/reuse.");
}

#[derive(Clone, Copy)]
pub struct HeapStats {
    pub start: usize,
    pub end: usize,
    pub used_bytes: usize,
    pub free_bytes: usize,
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
