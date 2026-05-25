pub mod ext2;
pub mod vfs;

use crate::{initrd::Initrd, security::Credentials};

pub fn init(initrd: &Initrd) {
    vfs::init(initrd);
    vfs::self_test();
}

pub fn open(path: &str) -> Result<usize, vfs::VfsError> {
    vfs::open(path)
}

pub fn open_read_as(path: &str, creds: Credentials) -> Result<usize, vfs::VfsError> {
    vfs::open_read_as(path, creds)
}

pub fn create_pipe(capacity: usize) -> Result<(usize, usize), vfs::VfsError> {
    vfs::create_pipe(capacity)
}

pub fn create_file_as(path: &str, creds: Credentials) -> Result<usize, vfs::VfsError> {
    vfs::create_file_as(path, creds)
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

pub fn chmod_as(path: &str, mode: u16, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::chmod_as(path, mode, creds)
}

pub fn mkdir_as(path: &str, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::mkdir_as(path, creds)
}

pub fn unlink_as(path: &str, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::unlink_as(path, creds)
}

pub fn read_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    vfs::read_file(path)
}

pub fn list_paths(prefix: &str) -> alloc::vec::Vec<alloc::string::String> {
    vfs::list_paths(prefix)
}

pub fn write_file(path: &str, data: &[u8]) {
    vfs::write_file(path, data);
}

pub use vfs::Stat;

pub fn lseek(fd: usize, offset: isize, whence: u32) -> Result<usize, vfs::VfsError> {
    vfs::lseek(fd, offset, whence)
}

pub fn mount(device: &str, mountpoint: &str, fstype: &str) -> Result<(), vfs::VfsError> {
    vfs::mount(device, mountpoint, fstype)
}

pub fn mount_hybrid_ext2() {
    vfs::mount_hybrid_ext2();
}

pub fn stat(path: &str) -> Result<Stat, vfs::VfsError> {
    vfs::stat(path)
}

pub fn fstat(fd: usize) -> Result<Stat, vfs::VfsError> {
    vfs::fstat(fd)
}

pub fn is_tty_fd(fd: usize) -> bool {
    vfs::is_tty_fd(fd)
}
