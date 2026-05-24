pub mod frame_allocator;

use crate::multiboot::BootInfo;

pub fn init(boot_info: &BootInfo) {
    frame_allocator::init(boot_info);
    frame_allocator::self_test();
}

