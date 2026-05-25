pub mod vfs;

use crate::initrd::Initrd;

pub fn init(initrd: &Initrd) {
    vfs::init(initrd);
    vfs::self_test();
}

pub fn read_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    vfs::read_file(path)
}

pub fn with_file_data<T>(path: &str, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
    vfs::with_file_data(path, f)
}
