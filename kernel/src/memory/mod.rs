pub mod frame_allocator;
pub mod paging;

use crate::multiboot::BootInfo;

pub fn init(boot_info: &BootInfo) {
    frame_allocator::init(boot_info);
    frame_allocator::self_test();
    paging::init();
}
