pub mod vfs;

use crate::initrd::Initrd;

pub fn init(initrd: &Initrd) {
    vfs::init(initrd);
    vfs::self_test();
}

pub fn read_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    vfs::read_file(path)
}
