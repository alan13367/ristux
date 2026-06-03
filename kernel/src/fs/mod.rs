pub mod ext2;
pub mod vfs;

use crate::{
    initrd::Initrd,
    security::{Access, Credentials},
};

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

pub fn open_with_rights_as(
    path: &str,
    creds: Credentials,
    read: bool,
    write: bool,
) -> Result<usize, vfs::VfsError> {
    vfs::open_with_rights_as(path, creds, read, write)
}

pub fn create_pipe(capacity: usize) -> Result<(usize, usize), vfs::VfsError> {
    vfs::create_pipe(capacity)
}

pub fn create_file_as(path: &str, creds: Credentials) -> Result<usize, vfs::VfsError> {
    vfs::create_file_as(path, creds)
}

pub fn create_file_with_mode_as(
    path: &str,
    creds: Credentials,
    mode: u16,
) -> Result<usize, vfs::VfsError> {
    vfs::create_file_with_mode_as(path, creds, mode)
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

pub fn truncate_fd(fd: usize, len: usize) -> Result<(), vfs::VfsError> {
    vfs::truncate_fd(fd, len)
}

pub fn close(fd: usize) -> Result<(), vfs::VfsError> {
    vfs::close(fd)
}

pub fn chmod_as(path: &str, mode: u16, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::chmod_as(path, mode, creds)
}

pub fn mkdir_with_mode_as(path: &str, creds: Credentials, mode: u16) -> Result<(), vfs::VfsError> {
    vfs::mkdir_with_mode_as(path, creds, mode)
}

pub fn rmdir_as(path: &str, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::rmdir_as(path, creds)
}

pub fn unlink_as(path: &str, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::unlink_as(path, creds)
}

pub fn rename_as(old_path: &str, new_path: &str, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::rename_as(old_path, new_path, creds)
}

pub fn symlink_as(target: &str, link_path: &str, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::symlink_as(target, link_path, creds)
}

pub fn link_as(old_path: &str, new_path: &str, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::link_as(old_path, new_path, creds)
}

pub fn readlink(path: &str) -> Result<alloc::vec::Vec<u8>, vfs::VfsError> {
    vfs::readlink(path)
}

pub fn chown_as(path: &str, uid: u32, gid: u32, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::chown_as(path, uid, gid, creds)
}

pub fn set_mtime_as(path: &str, mtime: u64, creds: Credentials) -> Result<(), vfs::VfsError> {
    vfs::set_mtime_as(path, mtime, creds)
}

pub fn can_access(path: &str, creds: Credentials, access: Access) -> Result<bool, vfs::VfsError> {
    vfs::can_access(path, creds, access)
}

pub fn read_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    vfs::read_file(path)
}

pub fn list_paths(prefix: &str) -> alloc::vec::Vec<alloc::string::String> {
    vfs::list_paths(prefix)
}

pub fn directory_entries(
    fd: usize,
) -> Result<(alloc::vec::Vec<vfs::DirectoryEntry>, usize), vfs::VfsError> {
    vfs::directory_entries(fd)
}

pub fn set_directory_offset(fd: usize, offset: usize) -> Result<(), vfs::VfsError> {
    vfs::set_directory_offset(fd, offset)
}

pub fn write_file(path: &str, data: &[u8]) {
    vfs::write_file(path, data);
}

pub use vfs::{FsStat, Stat};

pub fn lseek(fd: usize, offset: isize, whence: u32) -> Result<usize, vfs::VfsError> {
    vfs::lseek(fd, offset, whence)
}

pub fn mount(device: &str, mountpoint: &str, fstype: &str) -> Result<(), vfs::VfsError> {
    vfs::mount(device, mountpoint, fstype)
}

pub fn mount_hybrid_ext2() {
    vfs::mount_hybrid_ext2();
}

pub fn refresh_block_devices() {
    vfs::refresh_block_devices();
}

pub fn block_device_size(fd: usize) -> Option<u64> {
    vfs::block_device_size(fd)
}

pub fn stat(path: &str) -> Result<Stat, vfs::VfsError> {
    vfs::stat(path)
}

pub fn lstat(path: &str) -> Result<Stat, vfs::VfsError> {
    vfs::lstat(path)
}

pub fn fstat(fd: usize) -> Result<Stat, vfs::VfsError> {
    vfs::fstat(fd)
}

pub fn statfs(path: &str) -> Result<FsStat, vfs::VfsError> {
    vfs::statfs(path)
}

pub fn fd_path(fd: usize) -> Result<alloc::string::String, vfs::VfsError> {
    vfs::fd_path(fd)
}

pub fn poll(fd: usize) -> Result<vfs::PollReady, vfs::VfsError> {
    vfs::poll(fd)
}

pub fn fd_rights(fd: usize) -> Result<vfs::FdRights, vfs::VfsError> {
    vfs::fd_rights(fd)
}

pub fn is_tty_fd(fd: usize) -> bool {
    vfs::is_tty_fd(fd)
}

pub fn is_kernel_tty_fd(fd: usize) -> bool {
    vfs::is_kernel_tty_fd(fd)
}

pub fn pty_number(fd: usize) -> Option<usize> {
    vfs::pty_number(fd)
}

pub fn set_pty_locked(fd: usize, locked: bool) -> Result<(), vfs::VfsError> {
    vfs::set_pty_locked(fd, locked)
}

pub fn pty_termios_bytes(fd: usize) -> Result<[u8; crate::tty::TERMIOS_SIZE], vfs::VfsError> {
    vfs::pty_termios_bytes(fd)
}

pub fn set_pty_termios_bytes(fd: usize, bytes: &[u8]) -> Result<(), vfs::VfsError> {
    vfs::set_pty_termios_bytes(fd, bytes)
}

pub fn pty_winsize(fd: usize) -> Result<[u8; 8], vfs::VfsError> {
    vfs::pty_winsize(fd)
}

pub fn set_pty_winsize(fd: usize, bytes: &[u8]) -> Result<(), vfs::VfsError> {
    vfs::set_pty_winsize(fd, bytes)
}

pub fn pty_foreground_pgrp(fd: usize) -> Result<crate::process::Pid, vfs::VfsError> {
    vfs::pty_foreground_pgrp(fd)
}

pub fn set_pty_foreground_pgrp(fd: usize, pgrp: crate::process::Pid) -> Result<(), vfs::VfsError> {
    vfs::set_pty_foreground_pgrp(fd, pgrp)
}
