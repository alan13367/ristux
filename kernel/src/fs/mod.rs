pub mod vfs;

use crate::initrd::Initrd;

pub fn init(initrd: &Initrd) {
    vfs::init(initrd);
    vfs::self_test();
}

pub fn open(path: &str) -> Result<usize, vfs::VfsError> {
    vfs::open(path)
}

pub fn create_pipe(capacity: usize) -> Result<(usize, usize), vfs::VfsError> {
    vfs::create_pipe(capacity)
}

pub fn create_file(path: &str) -> Result<usize, vfs::VfsError> {
    vfs::create_file(path)
}

pub fn duplicate_fd(fd: usize) -> Result<usize, vfs::VfsError> {
    vfs::duplicate_fd(fd)
}

pub fn read(fd: usize, output: &mut [u8]) -> Result<usize, vfs::VfsError> {
    vfs::read(fd, output)
}

pub fn write(fd: usize, input: &[u8]) -> Result<usize, vfs::VfsError> {
    vfs::write(fd, input)
}

pub fn close(fd: usize) -> Result<(), vfs::VfsError> {
    vfs::close(fd)
}

pub fn mkdir(path: &str) -> Result<(), vfs::VfsError> {
    vfs::mkdir(path)
}

pub fn unlink(path: &str) -> Result<(), vfs::VfsError> {
    vfs::unlink(path)
}

pub fn read_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    vfs::read_file(path)
}

pub fn with_file_data<T>(path: &str, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
    vfs::with_file_data(path, f)
}

pub fn list_paths(prefix: &str) -> alloc::vec::Vec<alloc::string::String> {
    vfs::list_paths(prefix)
}

pub fn write_file(path: &str, data: &[u8]) {
    vfs::write_file(path, data);
}
