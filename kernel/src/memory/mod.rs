pub mod address_space;
pub mod frame_allocator;
pub mod heap;
pub mod paging;
pub mod refcount;

use crate::multiboot::BootInfo;

#[derive(Clone, Copy)]
pub struct MemoryStats {
    pub frames: frame_allocator::Stats,
    pub heap: heap::HeapStats,
}

pub fn init(boot_info: &BootInfo) {
    frame_allocator::init(boot_info);
    frame_allocator::self_test();
    paging::init();
    heap::init();
    refcount::init(frame_allocator::max_frame());
    heap::self_test();
    crate::sync::spinlock::self_test();
    address_space::self_test();
}

pub fn stats() -> MemoryStats {
    MemoryStats {
        frames: frame_allocator::stats(),
        heap: heap::stats(),
    }
}
