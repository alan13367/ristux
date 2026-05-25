use alloc::{collections::VecDeque, format, string::String, vec::Vec};
use core::{fmt, str};

use crate::{
    drivers,
    initrd::Initrd,
    security::{Access, Credentials, FileMetadata},
    sync::spinlock::SpinLock,
};

use super::ext2;

static VFS: SpinLock<Option<Vfs>> = SpinLock::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    File,
    Directory,
    Symlink,
    Device(DeviceKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceKind {
    Null,
    Zero,
    Console,
    Keyboard,
    Tty,
    Framebuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileTimestamps {
    pub created_at: u64,
    pub modified_at: u64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum VfsError {
    NotFound,
    NotFile,
    BadFd,
    Utf8,
    AlreadyExists,
    PermissionDenied,
    WouldBlock,
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::NotFile => f.write_str("not a file"),
            Self::BadFd => f.write_str("bad file descriptor"),
            Self::Utf8 => f.write_str("invalid utf-8"),
            Self::AlreadyExists => f.write_str("already exists"),
            Self::PermissionDenied => f.write_str("permission denied"),
            Self::WouldBlock => f.write_str("would block"),
        }
    }
}

struct Node {
    path: String,
    kind: NodeKind,
    metadata: FileMetadata,
    timestamps: FileTimestamps,
    data: Vec<u8>,
}

#[derive(Clone)]
enum OpenHandle {
    Node {
        node: usize,
        offset: usize,
        rights: OpenRights,
    },
    PipeRead {
        pipe: usize,
    },
    PipeWrite {
        pipe: usize,
    },
    Ext2File {
        path: String,
        offset: usize,
        rights: OpenRights,
    },
    Ext2Dir {
        path: String,
        offset: usize,
    },
}

#[derive(Clone, Copy)]
struct OpenRights {
    read: bool,
    write: bool,
}

impl OpenRights {
    const fn new(read: bool, write: bool) -> Self {
        Self { read, write }
    }

    const fn read_only() -> Self {
        Self {
            read: true,
            write: false,
        }
    }

    const fn read_write() -> Self {
        Self {
            read: true,
            write: true,
        }
    }
}

struct PipeState {
    buffer: VecDeque<u8>,
    capacity: usize,
    readers: usize,
    writers: usize,
}

#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    pub name: String,
    pub kind: NodeKind,
}

impl PipeState {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            capacity,
            readers: 1,
            writers: 1,
        }
    }

    fn add_reader(&mut self) {
        self.readers += 1;
    }

    fn add_writer(&mut self) {
        self.writers += 1;
    }

    fn close_reader(&mut self) {
        self.readers = self.readers.saturating_sub(1);
    }

    fn close_writer(&mut self) {
        self.writers = self.writers.saturating_sub(1);
    }

    fn read(&mut self, output: &mut [u8]) -> Result<usize, VfsError> {
        let mut read = 0;
        for byte in output {
            let Some(value) = self.buffer.pop_front() else {
                break;
            };
            *byte = value;
            read += 1;
        }
        if read == 0 && self.writers > 0 {
            return Err(VfsError::WouldBlock);
        }
        Ok(read)
    }

    fn write(&mut self, input: &[u8]) -> Result<usize, VfsError> {
        if self.readers == 0 {
            return Err(VfsError::BadFd);
        }
        let mut written = 0;
        for byte in input {
            if self.buffer.len() == self.capacity {
                break;
            }
            self.buffer.push_back(*byte);
            written += 1;
        }
        if written == 0 && !input.is_empty() {
            return Err(VfsError::WouldBlock);
        }
        Ok(written)
    }
}

pub struct Vfs {
    nodes: Vec<Node>,
    open_files: Vec<Option<OpenHandle>>,
    pipes: Vec<PipeState>,
    mounts: Vec<MountPoint>,
}

#[derive(Clone)]
struct MountPoint {
    mountpoint: String,
    fstype: String,
    ext2: Option<ext2::Ext2Fs>,
}

impl Vfs {
    fn new() -> Self {
        let mut vfs = Self {
            nodes: Vec::new(),
            open_files: Vec::new(),
            pipes: Vec::new(),
            mounts: Vec::new(),
        };

        vfs.add_directory("/");
        vfs.add_directory("/bin");
        vfs.add_directory("/etc");
        vfs.add_directory("/lib");
        vfs.add_directory("/dev");
        vfs.add_directory("/pkg");
        vfs.add_directory("/proc");
        vfs.add_directory("/tmp");
        vfs.chmod("/tmp", 0o777)
            .expect("failed to make /tmp writable");
        vfs.add_device("/dev/null", DeviceKind::Null);
        vfs.add_device("/dev/zero", DeviceKind::Zero);
        vfs.add_device("/dev/console", DeviceKind::Console);
        vfs.add_device("/dev/keyboard", DeviceKind::Keyboard);
        vfs.add_device("/dev/tty", DeviceKind::Tty);
        vfs.add_device("/dev/fb0", DeviceKind::Framebuffer);
        vfs.add_file("/proc/version", b"ristux 0.1\n");
        vfs.add_file("/tmp/message.txt", b"hello from tmpfs\n");
        vfs
    }

    fn mount_initrd(&mut self, initrd: &Initrd) {
        for file in initrd.files() {
            self.add_file(file.path, file.data);
        }
    }

    fn mount(&mut self, device: &str, mountpoint: &str, fstype: &str) -> Result<(), VfsError> {
        let _ = device;
        if !self.nodes.iter().any(|node| node.path == mountpoint) {
            self.add_directory(mountpoint);
        }
        if fstype == "ext2" {
            let fs = ext2::Ext2Fs::mount().map_err(|_| VfsError::NotFound)?;
            self.mounts.push(MountPoint {
                mountpoint: String::from(mountpoint),
                fstype: String::from(fstype),
                ext2: Some(fs),
            });
            crate::println!("VFS mounted {} on {}.", fstype, mountpoint);
            return Ok(());
        }
        Err(VfsError::NotFound)
    }

    fn root_ext2(&self) -> Option<&ext2::Ext2Fs> {
        self.mounts
            .iter()
            .find(|mount| mount.mountpoint == "/")
            .and_then(|mount| mount.ext2.as_ref())
    }

    fn root_ext2_mut(&mut self) -> Option<&mut ext2::Ext2Fs> {
        self.mounts
            .iter_mut()
            .find(|mount| mount.mountpoint == "/")
            .and_then(|mount| mount.ext2.as_mut())
    }

    fn use_root_ext2(path: &str) -> bool {
        !(path == "/dev"
            || path.starts_with("/dev/")
            || path == "/proc"
            || path.starts_with("/proc/")
            || path == "/tmp"
            || path.starts_with("/tmp/")
            || path == "/initrd"
            || path.starts_with("/initrd/"))
    }

    fn resolve_mount_list_paths(&self, prefix: &str) -> Vec<String> {
        let mut paths = self
            .nodes
            .iter()
            .filter(|node| node.path.starts_with(prefix))
            .map(|node| node.path.clone())
            .collect::<Vec<_>>();
        for mount in &self.mounts {
            if let Some(ext2) = &mount.ext2 {
                let mount_prefix = if prefix.starts_with(&mount.mountpoint) {
                    prefix.strip_prefix(&mount.mountpoint).unwrap_or("/")
                } else if prefix == "/" || mount.mountpoint.starts_with(prefix) {
                    "/"
                } else {
                    continue;
                };
                if let Ok(entries) = ext2.list_dir(mount_prefix) {
                    for entry in entries {
                        let full = if mount.mountpoint == "/" {
                            format!("/{}", entry.trim_start_matches('/'))
                        } else {
                            format!("{}/{}", mount.mountpoint, entry.trim_start_matches('/'))
                        };
                        if full.starts_with(prefix) && !paths.iter().any(|p| p == &full) {
                            paths.push(full);
                        }
                    }
                }
            }
        }
        paths
    }

    fn add_directory(&mut self, path: &str) {
        let now = crate::time::filesystem_timestamp();
        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::Directory,
            metadata: FileMetadata::new(0, 0, 0o755),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: Vec::new(),
        });
    }

    fn add_file(&mut self, path: &str, data: &[u8]) {
        let now = crate::time::filesystem_timestamp();
        if let Some(node) = self.nodes.iter_mut().find(|node| node.path == path) {
            node.kind = NodeKind::File;
            node.metadata = FileMetadata::new(0, 0, 0o644);
            node.timestamps.modified_at = now;
            node.data.clear();
            node.data.extend_from_slice(data);
            return;
        }

        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::File,
            metadata: FileMetadata::new(0, 0, 0o644),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: Vec::from(data),
        });
    }

    fn create_file(&mut self, path: &str) -> Result<usize, VfsError> {
        self.create_file_as(path, Credentials::root())
    }

    fn create_file_as(&mut self, path: &str, creds: Credentials) -> Result<usize, VfsError> {
        if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2_mut() {
                match fs.metadata(path) {
                    Ok(meta) => {
                        if meta.kind != ext2::Ext2NodeKind::File {
                            return Err(VfsError::NotFile);
                        }
                        if !meta.metadata.can_access(creds, Access::Write) {
                            return Err(VfsError::PermissionDenied);
                        }
                        fs.truncate_file(path).map_err(map_ext2_error)?;
                    }
                    Err(ext2::Ext2Error::NotFound) => {
                        let parent_path = parent_path(path);
                        let parent = fs.metadata(&parent_path).map_err(map_ext2_error)?;
                        if parent.kind != ext2::Ext2NodeKind::Directory {
                            return Err(VfsError::NotFile);
                        }
                        if !parent.metadata.can_access(creds, Access::Write) {
                            return Err(VfsError::PermissionDenied);
                        }
                        fs.create_file(path, creds.euid, creds.egid, 0o644)
                            .map_err(map_ext2_error)?;
                    }
                    Err(err) => return Err(map_ext2_error(err)),
                }
                return self.push_open_handle(OpenHandle::Ext2File {
                    path: String::from(path),
                    offset: 0,
                    rights: OpenRights::read_write(),
                });
            }
        }

        self.ensure_parent_directory(path, Some((creds, Access::Write)))?;
        if let Some(node) = self.nodes.iter_mut().find(|node| node.path == path) {
            if node.kind != NodeKind::File {
                return Err(VfsError::NotFile);
            }
            if !node.metadata.can_access(creds, Access::Write) {
                return Err(VfsError::PermissionDenied);
            }
            node.data.clear();
            node.timestamps.modified_at = crate::time::filesystem_timestamp();
            let node = self
                .nodes
                .iter()
                .position(|node| node.path == path)
                .unwrap();
            return self.push_open_handle(OpenHandle::Node {
                node,
                offset: 0,
                rights: OpenRights::read_write(),
            });
        }

        let now = crate::time::filesystem_timestamp();
        let node = self.nodes.len();
        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::File,
            metadata: FileMetadata::new(creds.euid, creds.egid, 0o644),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: Vec::new(),
        });
        self.push_open_handle(OpenHandle::Node {
            node,
            offset: 0,
            rights: OpenRights::read_write(),
        })
    }

    fn add_device(&mut self, path: &str, kind: DeviceKind) {
        let now = crate::time::filesystem_timestamp();
        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::Device(kind),
            metadata: FileMetadata::new(0, 0, 0o666),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: Vec::new(),
        });
    }

    fn open(&mut self, path: &str) -> Result<usize, VfsError> {
        self.open_as(path, Credentials::root(), OpenRights::read_write())
    }

    fn open_as(
        &mut self,
        path: &str,
        creds: Credentials,
        rights: OpenRights,
    ) -> Result<usize, VfsError> {
        let resolved = self.resolve_symlink_path(path)?;
        let path = resolved.as_str();
        let node = if let Some(index) = self.nodes.iter().position(|node| node.path == path) {
            index
        } else if proc_status_pid(path).is_some() {
            self.nodes.push(Node {
                path: String::from(path),
                kind: NodeKind::File,
                metadata: FileMetadata::new(0, 0, 0o444),
                timestamps: FileTimestamps {
                    created_at: crate::time::filesystem_timestamp(),
                    modified_at: crate::time::filesystem_timestamp(),
                },
                data: Vec::new(),
            });
            self.nodes.len() - 1
        } else if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2() {
                let meta = fs.metadata(path).map_err(|_| VfsError::NotFound)?;
                if meta.kind == ext2::Ext2NodeKind::Directory {
                    if rights.write {
                        return Err(VfsError::NotFile);
                    }
                    if rights.read && !meta.metadata.can_access(creds, Access::Read) {
                        return Err(VfsError::PermissionDenied);
                    }
                    return self.push_open_handle(OpenHandle::Ext2Dir {
                        path: String::from(path),
                        offset: 0,
                    });
                }
                if rights.read && !meta.metadata.can_access(creds, Access::Read) {
                    return Err(VfsError::PermissionDenied);
                }
                if rights.write && !meta.metadata.can_access(creds, Access::Write) {
                    return Err(VfsError::PermissionDenied);
                }
                return self.push_open_handle(OpenHandle::Ext2File {
                    path: String::from(path),
                    offset: 0,
                    rights,
                });
            }
            return Err(VfsError::NotFound);
        } else {
            return Err(VfsError::NotFound);
        };
        if self.nodes[node].kind == NodeKind::Directory && rights.write {
            return Err(VfsError::NotFile);
        }
        if rights.read && !self.nodes[node].metadata.can_access(creds, Access::Read) {
            crate::println!(
                "VFS permission denied: uid {} cannot read {}.",
                creds.euid,
                path
            );
            return Err(VfsError::PermissionDenied);
        }
        if rights.write && !self.nodes[node].metadata.can_access(creds, Access::Write) {
            crate::println!(
                "VFS permission denied: uid {} cannot write {}.",
                creds.euid,
                path
            );
            return Err(VfsError::PermissionDenied);
        }

        self.push_open_handle(OpenHandle::Node {
            node,
            offset: 0,
            rights,
        })
    }

    fn push_open_handle(&mut self, handle: OpenHandle) -> Result<usize, VfsError> {
        for (fd, slot) in self.open_files.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(handle);
                return Ok(fd);
            }
        }

        self.open_files.push(Some(handle));
        Ok(self.open_files.len() - 1)
    }

    fn create_pipe(&mut self, capacity: usize) -> Result<(usize, usize), VfsError> {
        let pipe = self.pipes.len();
        self.pipes.push(PipeState::new(capacity));
        let read_fd = self.push_open_handle(OpenHandle::PipeRead { pipe })?;
        let write_fd = self.push_open_handle(OpenHandle::PipeWrite { pipe })?;
        Ok((read_fd, write_fd))
    }

    fn duplicate_fd(&mut self, fd: usize) -> Result<usize, VfsError> {
        let handle = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref().cloned())
            .ok_or(VfsError::BadFd)?;
        self.retain_handle(&handle)?;
        self.push_open_handle(handle)
    }

    fn mkdir(&mut self, path: &str) -> Result<(), VfsError> {
        self.mkdir_as(path, Credentials::root())
    }

    fn mkdir_as(&mut self, path: &str, creds: Credentials) -> Result<(), VfsError> {
        if self.nodes.iter().any(|node| node.path == path) {
            return Err(VfsError::AlreadyExists);
        }
        self.ensure_parent_directory(path, Some((creds, Access::Write)))?;
        let now = crate::time::filesystem_timestamp();
        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::Directory,
            metadata: FileMetadata::new(creds.euid, creds.egid, 0o755),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: Vec::new(),
        });
        Ok(())
    }

    fn rmdir_as(&mut self, path: &str, creds: Credentials) -> Result<(), VfsError> {
        if path == "/" {
            return Err(VfsError::PermissionDenied);
        }
        self.ensure_parent_directory(path, Some((creds, Access::Write)))?;
        if self.nodes.iter().any(|node| {
            !node.path.is_empty() && node.path != path && parent_path(&node.path) == path
        }) {
            return Err(VfsError::PermissionDenied);
        }
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if node.kind != NodeKind::Directory {
            return Err(VfsError::NotFile);
        }
        node.path.clear();
        Ok(())
    }

    fn unlink(&mut self, path: &str) -> Result<(), VfsError> {
        self.unlink_as(path, Credentials::root())
    }

    fn unlink_as(&mut self, path: &str, creds: Credentials) -> Result<(), VfsError> {
        self.ensure_parent_directory(path, Some((creds, Access::Write)))?;
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if matches!(node.kind, NodeKind::Directory | NodeKind::Device(_)) {
            return Err(VfsError::NotFile);
        }
        node.path.clear();
        Ok(())
    }

    fn rename_as(&mut self, old_path: &str, new_path: &str, creds: Credentials) -> Result<(), VfsError> {
        if old_path == "/" || new_path == "/" {
            return Err(VfsError::PermissionDenied);
        }
        self.ensure_parent_directory(old_path, Some((creds, Access::Write)))?;
        self.ensure_parent_directory(new_path, Some((creds, Access::Write)))?;
        let old_index = self
            .nodes
            .iter()
            .position(|node| node.path == old_path)
            .ok_or(VfsError::NotFound)?;
        if let Some(new_index) = self.nodes.iter().position(|node| node.path == new_path) {
            if new_index != old_index {
                self.nodes[new_index].path.clear();
            }
        }
        let old_prefix = format!("{}/", old_path.trim_end_matches('/'));
        let new_prefix = format!("{}/", new_path.trim_end_matches('/'));
        self.nodes[old_index].path = String::from(new_path);
        for node in &mut self.nodes {
            if node.path.starts_with(&old_prefix) {
                node.path = format!("{}{}", new_prefix, &node.path[old_prefix.len()..]);
            }
        }
        Ok(())
    }

    fn symlink_as(&mut self, target: &str, link_path: &str, creds: Credentials) -> Result<(), VfsError> {
        if self.nodes.iter().any(|node| node.path == link_path) {
            return Err(VfsError::AlreadyExists);
        }
        self.ensure_parent_directory(link_path, Some((creds, Access::Write)))?;
        let now = crate::time::filesystem_timestamp();
        self.nodes.push(Node {
            path: String::from(link_path),
            kind: NodeKind::Symlink,
            metadata: FileMetadata::new(creds.euid, creds.egid, 0o777),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: Vec::from(target.as_bytes()),
        });
        Ok(())
    }

    fn readlink(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let node = self
            .nodes
            .iter()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if node.kind != NodeKind::Symlink {
            return Err(VfsError::NotFile);
        }
        Ok(node.data.clone())
    }

    fn chown_as(&mut self, path: &str, uid: u32, gid: u32, creds: Credentials) -> Result<(), VfsError> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if !creds.is_superuser() {
            return Err(VfsError::PermissionDenied);
        }
        if uid != u32::MAX {
            node.metadata.owner = uid;
        }
        if gid != u32::MAX {
            node.metadata.group = gid;
        }
        Ok(())
    }

    fn chmod_as(&mut self, path: &str, mode: u16, creds: Credentials) -> Result<(), VfsError> {
        let resolved = self.resolve_symlink_path(path)?;
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.path == resolved)
            .ok_or(VfsError::NotFound)?;
        if !creds.is_superuser() && creds.euid != node.metadata.owner {
            return Err(VfsError::PermissionDenied);
        }
        node.metadata.mode = crate::security::FileMode::new(mode);
        Ok(())
    }

    fn ensure_parent_directory(
        &self,
        path: &str,
        required: Option<(Credentials, Access)>,
    ) -> Result<(), VfsError> {
        if !path.starts_with('/') || path == "/" {
            return Err(VfsError::NotFound);
        }

        let slash = path.rfind('/').ok_or(VfsError::NotFound)?;
        let parent = if slash == 0 { "/" } else { &path[..slash] };
        let parent = self
            .nodes
            .iter()
            .find(|node| node.path == parent)
            .ok_or(VfsError::NotFound)?;
        if parent.kind != NodeKind::Directory {
            return Err(VfsError::NotFile);
        }
        if let Some((creds, access)) = required {
            if !parent.metadata.can_access(creds, access) {
                return Err(VfsError::PermissionDenied);
            }
        }
        Ok(())
    }

    fn read(&mut self, fd: usize, output: &mut [u8]) -> Result<usize, VfsError> {
        if let Some((path, offset, rights)) = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref())
            .and_then(|handle| match handle {
                OpenHandle::Ext2File {
                    path,
                    offset,
                    rights,
                } => Some((path.clone(), *offset, *rights)),
                _ => None,
            })
        {
            if !rights.read {
                return Err(VfsError::BadFd);
            }
            let data = self
                .root_ext2()
                .ok_or(VfsError::NotFound)?
                .read_file(&path)
                .map_err(map_ext2_error)?;
            let remaining = data.len().saturating_sub(offset);
            let count = remaining.min(output.len());
            output[..count].copy_from_slice(&data[offset..offset + count]);
            if let Some(Some(OpenHandle::Ext2File {
                offset: cursor, ..
            })) = self.open_files.get_mut(fd)
            {
                *cursor += count;
            }
            return Ok(count);
        }

        let Some(handle) = self.open_files.get_mut(fd).and_then(Option::as_mut) else {
            return Err(VfsError::BadFd);
        };

        match handle {
            OpenHandle::Node {
                node,
                offset,
                rights,
            } => {
                if !rights.read {
                    return Err(VfsError::BadFd);
                }
                let path = self.nodes[*node].path.clone();
                if let Some(pid) = proc_status_pid(&path) {
                    let count = read_proc_status(pid, *offset, output)?;
                    *offset += count;
                    return Ok(count);
                }
                let node = &self.nodes[*node];
                if node.kind != NodeKind::File {
                    return match node.kind {
                        NodeKind::Device(DeviceKind::Null) => Ok(0),
                        NodeKind::Device(DeviceKind::Zero) => {
                            output.fill(0);
                            Ok(output.len())
                        }
                        NodeKind::Device(DeviceKind::Keyboard) => {
                            let mut count = 0;
                            for byte in output.iter_mut() {
                                let Some(scancode) = drivers::keyboard::pop_scancode() else {
                                    break;
                                };
                                *byte = scancode;
                                count += 1;
                            }
                            if count == 0 && !output.is_empty() {
                                return Err(VfsError::WouldBlock);
                            }
                            Ok(count)
                        }
                        NodeKind::Device(DeviceKind::Tty) => Ok(crate::tty::read(output)),
                        NodeKind::Device(DeviceKind::Console) => Ok(0),
                        NodeKind::Device(DeviceKind::Framebuffer) => Ok(0),
                        NodeKind::Directory => Err(VfsError::NotFile),
                        NodeKind::Symlink => Err(VfsError::NotFile),
                        NodeKind::File => unreachable!(),
                    };
                }

                let remaining = node.data.len().saturating_sub(*offset);
                let count = remaining.min(output.len());
                output[..count].copy_from_slice(&node.data[*offset..*offset + count]);
                *offset += count;
                Ok(count)
            }
            OpenHandle::PipeRead { pipe } => {
                let pipe = self.pipes.get_mut(*pipe).ok_or(VfsError::BadFd)?;
                pipe.read(output)
            }
            OpenHandle::Ext2File { .. } => Err(VfsError::BadFd),
            OpenHandle::Ext2Dir { .. } => Err(VfsError::NotFile),
            OpenHandle::PipeWrite { .. } => Err(VfsError::BadFd),
        }
    }

    fn write(&mut self, fd: usize, input: &[u8]) -> Result<usize, VfsError> {
        if let Some((path, offset, rights)) = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref())
            .and_then(|handle| match handle {
                OpenHandle::Ext2File {
                    path,
                    offset,
                    rights,
                } => Some((path.clone(), *offset, *rights)),
                _ => None,
            })
        {
            if !rights.write {
                return Err(VfsError::BadFd);
            }
            let mut data = self
                .root_ext2()
                .ok_or(VfsError::NotFound)?
                .read_file(&path)
                .map_err(map_ext2_error)?;
            if offset > data.len() {
                data.resize(offset, 0);
            }
            let end = offset + input.len();
            if end > data.len() {
                data.resize(end, 0);
            }
            data[offset..end].copy_from_slice(input);
            self.root_ext2_mut()
                .ok_or(VfsError::NotFound)?
                .write_file(&path, &data)
                .map_err(map_ext2_error)?;
            if let Some(Some(OpenHandle::Ext2File {
                offset: cursor, ..
            })) = self.open_files.get_mut(fd)
            {
                *cursor = end;
            }
            return Ok(input.len());
        }

        let Some(handle) = self.open_files.get_mut(fd).and_then(Option::as_mut) else {
            return Err(VfsError::BadFd);
        };

        match handle {
            OpenHandle::Node {
                node,
                offset,
                rights,
            } => {
                if !rights.write {
                    return Err(VfsError::BadFd);
                }
                let node = &mut self.nodes[*node];
                if node.kind != NodeKind::File {
                    return match node.kind {
                        NodeKind::Device(DeviceKind::Null) => Ok(input.len()),
                        NodeKind::Device(DeviceKind::Console) => {
                            let text = str::from_utf8(input).map_err(|_| VfsError::Utf8)?;
                            crate::log::write_str(text);
                            Ok(input.len())
                        }
                        NodeKind::Device(DeviceKind::Framebuffer) => {
                            Ok(drivers::framebuffer::write_bytes(input))
                        }
                        NodeKind::Device(DeviceKind::Tty) => {
                            let text = str::from_utf8(input).map_err(|_| VfsError::Utf8)?;
                            crate::log::write_str(text);
                            Ok(input.len())
                        }
                        NodeKind::Device(DeviceKind::Zero | DeviceKind::Keyboard) => Ok(0),
                        NodeKind::Directory => Err(VfsError::NotFile),
                        NodeKind::Symlink => Err(VfsError::NotFile),
                        NodeKind::File => unreachable!(),
                    };
                }

                if *offset > node.data.len() {
                    node.data.resize(*offset, 0);
                }
                let end = *offset + input.len();
                if end > node.data.len() {
                    node.data.resize(end, 0);
                }
                node.data[*offset..end].copy_from_slice(input);
                *offset = end;
                node.timestamps.modified_at = crate::time::filesystem_timestamp();
                Ok(input.len())
            }
            OpenHandle::PipeWrite { pipe } => {
                let pipe = self.pipes.get_mut(*pipe).ok_or(VfsError::BadFd)?;
                let written = pipe.write(input)?;
                if written > 0 {
                    crate::process::wake_io_waiters();
                }
                Ok(written)
            }
            OpenHandle::Ext2File { .. } | OpenHandle::Ext2Dir { .. } => Err(VfsError::BadFd),
            OpenHandle::PipeRead { .. } => Err(VfsError::BadFd),
        }
    }

    fn close(&mut self, fd: usize) -> Result<(), VfsError> {
        let Some(slot) = self.open_files.get_mut(fd) else {
            return Err(VfsError::BadFd);
        };
        let handle = slot.take().ok_or(VfsError::BadFd)?;
        self.release_handle(&handle)?;
        Ok(())
    }

    fn retain_handle(&mut self, handle: &OpenHandle) -> Result<(), VfsError> {
        match handle {
            OpenHandle::PipeRead { pipe } => self
                .pipes
                .get_mut(*pipe)
                .ok_or(VfsError::BadFd)?
                .add_reader(),
            OpenHandle::PipeWrite { pipe } => self
                .pipes
                .get_mut(*pipe)
                .ok_or(VfsError::BadFd)?
                .add_writer(),
            OpenHandle::Node { .. } | OpenHandle::Ext2File { .. } | OpenHandle::Ext2Dir { .. } => {}
        }
        Ok(())
    }

    fn release_handle(&mut self, handle: &OpenHandle) -> Result<(), VfsError> {
        match handle {
            OpenHandle::PipeRead { pipe } => self
                .pipes
                .get_mut(*pipe)
                .ok_or(VfsError::BadFd)?
                .close_reader(),
            OpenHandle::PipeWrite { pipe } => self
                .pipes
                .get_mut(*pipe)
                .ok_or(VfsError::BadFd)?
                .close_writer(),
            OpenHandle::Node { .. } | OpenHandle::Ext2File { .. } | OpenHandle::Ext2Dir { .. } => {}
        }
        Ok(())
    }

    fn lseek(&mut self, fd: usize, offset: isize, whence: u32) -> Result<usize, VfsError> {
        if let Some((path, cursor)) = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref())
            .and_then(|handle| match handle {
                OpenHandle::Ext2File { path, offset, .. } => Some((path.clone(), *offset)),
                _ => None,
            })
        {
            let size = self
                .root_ext2()
                .and_then(|fs| fs.metadata(&path).ok())
                .map(|meta| meta.size as usize)
                .ok_or(VfsError::NotFound)?;
            let new_offset = match whence {
                0 => offset,
                1 => cursor as isize + offset,
                2 => size as isize + offset,
                _ => return Err(VfsError::BadFd),
            };
            if new_offset < 0 {
                return Err(VfsError::BadFd);
            }
            if let Some(Some(OpenHandle::Ext2File {
                offset: cursor, ..
            })) = self.open_files.get_mut(fd)
            {
                *cursor = new_offset as usize;
            }
            return Ok(new_offset as usize);
        }

        if let Some((path, cursor)) = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref())
            .and_then(|handle| match handle {
                OpenHandle::Ext2Dir { path, offset } => Some((path.clone(), *offset)),
                _ => None,
            })
        {
            let size = self
                .root_ext2()
                .ok_or(VfsError::NotFound)?
                .list_dir(&path)
                .map_err(map_ext2_error)?
                .len();
            let new_offset = match whence {
                0 => offset,
                1 => cursor as isize + offset,
                2 => size as isize + offset,
                _ => return Err(VfsError::BadFd),
            };
            if new_offset < 0 {
                return Err(VfsError::BadFd);
            }
            if let Some(Some(OpenHandle::Ext2Dir {
                offset: cursor, ..
            })) = self.open_files.get_mut(fd)
            {
                *cursor = new_offset as usize;
            }
            return Ok(new_offset as usize);
        }

        let Some(handle) = self.open_files.get_mut(fd).and_then(Option::as_mut) else {
            return Err(VfsError::BadFd);
        };
        let (size, cursor) = match handle {
            OpenHandle::Node {
                node,
                offset: cursor,
                ..
            } => {
                let size = if let Some(pid) = proc_status_pid(&self.nodes[*node].path) {
                    format_proc_status(pid).len()
                } else {
                    self.nodes[*node].data.len()
                };
                (size, cursor)
            }
            OpenHandle::Ext2File { .. } | OpenHandle::Ext2Dir { .. } => {
                return Err(VfsError::BadFd);
            }
            OpenHandle::PipeRead { .. } | OpenHandle::PipeWrite { .. } => {
                return Err(VfsError::BadFd);
            }
        };
        let new_offset = match whence {
            0 => offset,
            1 => *cursor as isize + offset,
            2 => size as isize + offset,
            _ => return Err(VfsError::BadFd),
        };
        if new_offset < 0 {
            return Err(VfsError::BadFd);
        }
        *cursor = new_offset as usize;
        Ok(*cursor)
    }

    fn fstat(&self, fd: usize) -> Result<Stat, VfsError> {
        let handle = self
            .open_files
            .get(fd)
            .and_then(|h| h.as_ref())
            .ok_or(VfsError::BadFd)?;
        match handle {
            OpenHandle::Node { node, .. } => {
                let node = &self.nodes[*node];
                Ok(Stat {
                    owner: node.metadata.owner,
                    group: node.metadata.group,
                    mode: node.metadata.mode.0,
                    size: node.data.len() as u64,
                    mtime: node.timestamps.modified_at,
                })
            }
            OpenHandle::Ext2File { path, .. } => {
                let meta = self
                    .root_ext2()
                    .ok_or(VfsError::NotFound)?
                    .metadata(path)
                    .map_err(map_ext2_error)?;
                Ok(Stat {
                    owner: meta.metadata.owner,
                    group: meta.metadata.group,
                    mode: meta.metadata.mode.0,
                    size: meta.size,
                    mtime: crate::time::filesystem_timestamp(),
                })
            }
            OpenHandle::Ext2Dir { path, .. } => {
                let meta = self
                    .root_ext2()
                    .ok_or(VfsError::NotFound)?
                    .metadata(path)
                    .map_err(map_ext2_error)?;
                Ok(Stat {
                    owner: meta.metadata.owner,
                    group: meta.metadata.group,
                    mode: meta.metadata.mode.0,
                    size: meta.size,
                    mtime: crate::time::filesystem_timestamp(),
                })
            }
            OpenHandle::PipeRead { .. } | OpenHandle::PipeWrite { .. } => Err(VfsError::BadFd),
        }
    }

    fn is_tty_fd(&self, fd: usize) -> bool {
        let Some(Some(handle)) = self.open_files.get(fd) else {
            return false;
        };
        let OpenHandle::Node { node, .. } = handle else {
            return false;
        };
        matches!(
            self.nodes.get(*node).map(|n| n.kind),
            Some(NodeKind::Device(DeviceKind::Tty))
        )
    }

    fn stat(&self, path: &str) -> Result<Stat, VfsError> {
        self.stat_inner(path, true)
    }

    fn lstat(&self, path: &str) -> Result<Stat, VfsError> {
        self.stat_inner(path, false)
    }

    fn stat_inner(&self, path: &str, follow_symlink: bool) -> Result<Stat, VfsError> {
        let resolved;
        let path = if follow_symlink {
            resolved = self.resolve_symlink_path(path)?;
            resolved.as_str()
        } else {
            path
        };
        if let Some(pid) = proc_status_pid(path) {
            return Ok(Stat {
                owner: 0,
                group: 0,
                mode: 0o444,
                size: format_proc_status(pid).len() as u64,
                mtime: crate::time::filesystem_timestamp(),
            });
        }
        if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2() {
                if let Ok(meta) = fs.metadata(path) {
                    return Ok(Stat {
                        owner: meta.metadata.owner,
                        group: meta.metadata.group,
                        mode: meta.metadata.mode.0,
                        size: meta.size,
                        mtime: crate::time::filesystem_timestamp(),
                    });
                }
            }
        }
        let node = self
            .nodes
            .iter()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        Ok(Stat {
            owner: node.metadata.owner,
            group: node.metadata.group,
            mode: node.metadata.mode.0,
            size: node.data.len() as u64,
            mtime: node.timestamps.modified_at,
        })
    }

    fn chmod(&mut self, path: &str, mode: u16) -> Result<(), VfsError> {
        self.chmod_as(path, mode, Credentials::root())
    }

    fn can_access(&self, path: &str, creds: Credentials, access: Access) -> Result<bool, VfsError> {
        let resolved = self.resolve_symlink_path(path)?;
        let path = resolved.as_str();
        if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2() {
                if let Ok(meta) = fs.metadata(path) {
                    return Ok(meta.metadata.can_access(creds, access));
                }
            }
        }
        let node = self
            .nodes
            .iter()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        Ok(node.metadata.can_access(creds, access))
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let resolved = self.resolve_symlink_path(path).ok()?;
        let path = resolved.as_str();
        if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2() {
                if let Ok(data) = fs.read_file(path) {
                    return Some(data);
                }
            }
        }
        let node = self.nodes.iter().find(|node| node.path == path)?;
        if node.kind == NodeKind::File {
            Some(node.data.clone())
        } else {
            None
        }
    }

    fn resolve_symlink_path(&self, path: &str) -> Result<String, VfsError> {
        let mut current = String::from(path);
        for _ in 0..8 {
            let Some(node) = self.nodes.iter().find(|node| node.path == current) else {
                return Ok(current);
            };
            if node.kind != NodeKind::Symlink {
                return Ok(current);
            }
            let target = str::from_utf8(&node.data).map_err(|_| VfsError::Utf8)?;
            current = if target.starts_with('/') {
                String::from(target)
            } else {
                join_path(&parent_path(&current), target)
            };
        }
        Err(VfsError::NotFound)
    }

    fn timestamps(&self, path: &str) -> Option<FileTimestamps> {
        self.nodes
            .iter()
            .find(|node| node.path == path)
            .map(|node| node.timestamps)
    }

    fn directory_entries(&self, fd: usize) -> Result<(Vec<DirectoryEntry>, usize), VfsError> {
        let handle = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref())
            .ok_or(VfsError::BadFd)?;
        match handle {
            OpenHandle::Node { node, offset, .. } => {
                let node = self.nodes.get(*node).ok_or(VfsError::BadFd)?;
                if node.kind != NodeKind::Directory {
                    return Err(VfsError::NotFile);
                }
                Ok((self.node_directory_entries(&node.path), *offset))
            }
            OpenHandle::Ext2Dir { path, offset } => Ok((self.ext2_directory_entries(path)?, *offset)),
            _ => Err(VfsError::NotFile),
        }
    }

    fn set_directory_offset(&mut self, fd: usize, offset: usize) -> Result<(), VfsError> {
        let handle = self
            .open_files
            .get_mut(fd)
            .and_then(|slot| slot.as_mut())
            .ok_or(VfsError::BadFd)?;
        match handle {
            OpenHandle::Node {
                node,
                offset: cursor,
                ..
            } => {
                if self.nodes.get(*node).map(|node| node.kind) != Some(NodeKind::Directory) {
                    return Err(VfsError::NotFile);
                }
                *cursor = offset;
                Ok(())
            }
            OpenHandle::Ext2Dir { offset: cursor, .. } => {
                *cursor = offset;
                Ok(())
            }
            _ => Err(VfsError::NotFile),
        }
    }

    fn node_directory_entries(&self, path: &str) -> Vec<DirectoryEntry> {
        let mut entries = Vec::new();
        for child in &self.nodes {
            if child.path.is_empty() || child.path == path {
                continue;
            }
            if parent_path(&child.path) != path {
                continue;
            }
            entries.push(DirectoryEntry {
                name: file_name(&child.path),
                kind: child.kind,
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    fn ext2_directory_entries(&self, path: &str) -> Result<Vec<DirectoryEntry>, VfsError> {
        let fs = self.root_ext2().ok_or(VfsError::NotFound)?;
        let mut entries = Vec::new();
        for name in fs.list_dir(path).map_err(map_ext2_error)? {
            let full = join_path(path, &name);
            let kind = match fs.metadata(&full).map_err(map_ext2_error)?.kind {
                ext2::Ext2NodeKind::File => NodeKind::File,
                ext2::Ext2NodeKind::Directory => NodeKind::Directory,
            };
            entries.push(DirectoryEntry { name, kind });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn list_paths(&self, prefix: &str) -> Vec<String> {
        self.resolve_mount_list_paths(prefix)
    }
}

pub fn init(initrd: &Initrd) {
    let mut vfs = Vfs::new();
    vfs.mount_initrd(initrd);
    crate::println!("VFS mounted initrd, devfs, procfs, and tmpfs.");
    *VFS.lock() = Some(vfs);
}

pub fn mount(device: &str, mountpoint: &str, fstype: &str) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.mount(device, mountpoint, fstype))
}

pub fn mount_hybrid_ext2() {
    if mount("virtio0", "/", "ext2").is_ok() {
        crate::println!("Ext2 mounted as / with devfs, procfs, and tmpfs overlays.");
    }
}

pub fn open(path: &str) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.open(path))
}

pub fn open_read_as(path: &str, creds: Credentials) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.open_as(path, creds, OpenRights::read_only()))
}

pub fn open_with_rights_as(
    path: &str,
    creds: Credentials,
    read: bool,
    write: bool,
) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.open_as(path, creds, OpenRights::new(read, write)))
}

pub fn create_pipe(capacity: usize) -> Result<(usize, usize), VfsError> {
    with_vfs(|vfs| vfs.create_pipe(capacity))
}

pub fn create_file(path: &str) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.create_file(path))
}

pub fn create_file_as(path: &str, creds: Credentials) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.create_file_as(path, creds))
}

pub fn duplicate_fd(fd: usize) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.duplicate_fd(fd))
}

pub fn read(fd: usize, output: &mut [u8]) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.read(fd, output))
}

pub fn write(fd: usize, input: &[u8]) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.write(fd, input))
}

pub fn close(fd: usize) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.close(fd))
}

#[derive(Clone, Copy, Debug)]
pub struct Stat {
    pub owner: u32,
    pub group: u32,
    pub mode: u16,
    pub size: u64,
    pub mtime: u64,
}

pub fn lseek(fd: usize, offset: isize, whence: u32) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.lseek(fd, offset, whence))
}

pub fn stat(path: &str) -> Result<Stat, VfsError> {
    with_vfs(|vfs| vfs.stat(path))
}

pub fn lstat(path: &str) -> Result<Stat, VfsError> {
    with_vfs(|vfs| vfs.lstat(path))
}

pub fn fstat(fd: usize) -> Result<Stat, VfsError> {
    with_vfs(|vfs| vfs.fstat(fd))
}

pub fn is_tty_fd(fd: usize) -> bool {
    let guard = VFS.lock();
    guard.as_ref().map(|vfs| vfs.is_tty_fd(fd)).unwrap_or(false)
}

pub fn chmod(path: &str, mode: u16) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.chmod(path, mode))
}

pub fn chmod_as(path: &str, mode: u16, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.chmod_as(path, mode, creds))
}

pub fn mkdir(path: &str) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.mkdir(path))
}

pub fn mkdir_as(path: &str, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.mkdir_as(path, creds))
}

pub fn rmdir_as(path: &str, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.rmdir_as(path, creds))
}

pub fn unlink(path: &str) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.unlink(path))
}

pub fn unlink_as(path: &str, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.unlink_as(path, creds))
}

pub fn rename_as(old_path: &str, new_path: &str, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.rename_as(old_path, new_path, creds))
}

pub fn symlink_as(target: &str, link_path: &str, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.symlink_as(target, link_path, creds))
}

pub fn readlink(path: &str) -> Result<Vec<u8>, VfsError> {
    with_vfs(|vfs| vfs.readlink(path))
}

pub fn chown_as(path: &str, uid: u32, gid: u32, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.chown_as(path, uid, gid, creds))
}

pub fn can_access(path: &str, creds: Credentials, access: Access) -> Result<bool, VfsError> {
    let guard = VFS.lock();
    let vfs = guard.as_ref().expect("VFS used before initialization");
    vfs.can_access(path, creds, access)
}

pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let guard = VFS.lock();
    guard.as_ref().and_then(|vfs| vfs.read_file(path))
}

pub fn write_file(path: &str, data: &[u8]) {
    with_vfs(|vfs| vfs.add_file(path, data));
}

pub fn timestamps(path: &str) -> Option<FileTimestamps> {
    let guard = VFS.lock();
    guard.as_ref().and_then(|vfs| vfs.timestamps(path))
}

pub fn directory_entries(fd: usize) -> Result<(Vec<DirectoryEntry>, usize), VfsError> {
    with_vfs(|vfs| vfs.directory_entries(fd))
}

pub fn set_directory_offset(fd: usize, offset: usize) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.set_directory_offset(fd, offset))
}

pub fn list_paths(prefix: &str) -> Vec<String> {
    let guard = VFS.lock();
    guard
        .as_ref()
        .map(|vfs| vfs.list_paths(prefix))
        .unwrap_or_default()
}

fn map_ext2_error(err: ext2::Ext2Error) -> VfsError {
    match err {
        ext2::Ext2Error::NotFound => VfsError::NotFound,
        ext2::Ext2Error::NotDirectory | ext2::Ext2Error::NotFile => VfsError::NotFile,
        ext2::Ext2Error::AlreadyExists => VfsError::AlreadyExists,
        ext2::Ext2Error::InvalidSuperblock
        | ext2::Ext2Error::IoError
        | ext2::Ext2Error::Unsupported
        | ext2::Ext2Error::NoSpace
        | ext2::Ext2Error::DirectoryFull => VfsError::BadFd,
    }
}

fn parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => String::from("/"),
        Some(index) => String::from(&trimmed[..index]),
    }
}

fn file_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .into()
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    }
}

fn proc_status_pid(path: &str) -> Option<u64> {
    let rest = path.strip_prefix("/proc/")?;
    let pid_text = rest.strip_suffix("/status")?;
    pid_text.parse().ok()
}

fn format_proc_status(pid: u64) -> alloc::string::String {
    let Some((name, state, parent, exit_status)) = crate::process::get_process_info(pid) else {
        return alloc::format!("pid: {}\nstate: not found\n", pid);
    };
    let state_text = match state {
        crate::process::ProcessState::Ready => "ready",
        crate::process::ProcessState::Running => "running",
        crate::process::ProcessState::Blocked(_) => "blocked",
        crate::process::ProcessState::Zombie(_) => "zombie",
    };
    alloc::format!(
        "pid: {}\nname: {}\nstate: {}\nparent: {}\nexit: {}\n",
        pid,
        name,
        state_text,
        parent
            .map(|p| alloc::format!("{}", p))
            .unwrap_or_else(|| alloc::string::String::from("-")),
        exit_status
            .map(|status| alloc::format!("{}", status))
            .unwrap_or_else(|| alloc::string::String::from("-"))
    )
}

fn read_proc_status(pid: u64, offset: usize, output: &mut [u8]) -> Result<usize, VfsError> {
    let text = format_proc_status(pid);
    let bytes = text.as_bytes();
    let remaining = bytes.len().saturating_sub(offset);
    let count = remaining.min(output.len());
    output[..count].copy_from_slice(&bytes[offset..offset + count]);
    Ok(count)
}

pub fn self_test() {
    let fd = open("/dev/zero").expect("open /dev/zero failed");
    let mut zeros = [1u8; 8];
    let zero_read = read(fd, &mut zeros).expect("read /dev/zero failed");
    close(fd).expect("close /dev/zero failed");
    if zero_read != zeros.len() || zeros != [0; 8] {
        panic!("/dev/zero self-test failed");
    }

    let fd = open("/dev/null").expect("open /dev/null failed");
    let wrote = write(fd, b"discard me").expect("write /dev/null failed");
    close(fd).expect("close /dev/null failed");
    if wrote != b"discard me".len() {
        panic!("/dev/null self-test failed");
    }

    write_file("/tmp/dup.txt", b"dup ok");
    let fd = open("/tmp/dup.txt").expect("open dup test file failed");
    let duplicated = duplicate_fd(fd).expect("duplicate fd self-test failed");
    close(fd).expect("close original dup test fd failed");
    let mut dup_bytes = [0; 6];
    let dup_read = read(duplicated, &mut dup_bytes).expect("read duplicated fd failed");
    close(duplicated).expect("close duplicated fd failed");
    if dup_read != dup_bytes.len() || &dup_bytes != b"dup ok" {
        panic!("duplicated fd self-test read wrong data");
    }

    mkdir("/tmp/vfsdir").expect("mkdir self-test failed");
    let fd = create_file("/tmp/vfsdir/created.txt").expect("create file self-test failed");
    write(fd, b"created").expect("write created file self-test failed");
    close(fd).expect("close created file self-test failed");
    if read_file("/tmp/vfsdir/created.txt").as_deref() != Some(b"created") {
        panic!("created file self-test read wrong data");
    }
    unlink("/tmp/vfsdir/created.txt").expect("unlink self-test failed");
    if read_file("/tmp/vfsdir/created.txt").is_some() {
        panic!("unlink self-test left file reachable");
    }

    let fd = open("/dev/console").expect("open /dev/console failed");
    write(fd, b"console device online\n").expect("write /dev/console failed");
    close(fd).expect("close /dev/console failed");

    if read_file("/bin/init").is_none()
        || read_file("/lib/libc.so").is_none()
        || read_file("/etc/os-release").is_none()
        || read_file("/pkg/packages.txt").is_none()
        || read_file("/tmp/message.txt").is_none()
    {
        panic!("VFS path resolution self-test failed");
    }
    if list_paths("/dev").len() < 5 {
        panic!("devfs mount self-test failed");
    }
    let fb = open("/dev/fb0").expect("open /dev/fb0 failed");
    let fb_written = write(fb, &[0x40, 0x80, 0xff]).expect("write /dev/fb0 failed");
    close(fb).expect("close /dev/fb0 failed");
    if fb_written != 3 {
        panic!("/dev/fb0 self-test failed");
    }
    chmod("/tmp/message.txt", 0o600).expect("chmod self-test failed");
    let user = Credentials::user(1000, 1000);
    if can_access("/tmp/message.txt", user, Access::Read).expect("permission check failed") {
        panic!("VFS permission self-test allowed private read");
    }
    write_file("/tmp/timestamp.txt", b"before");
    let before = timestamps("/tmp/timestamp.txt").expect("missing timestamps before write");
    let fd = open("/tmp/timestamp.txt").expect("open timestamp test file failed");
    write(fd, b"timestamped").expect("timestamp test write failed");
    close(fd).expect("close timestamp test file failed");
    let after = timestamps("/tmp/timestamp.txt").expect("missing timestamps after write");
    if before.created_at != after.created_at || after.modified_at <= before.modified_at {
        panic!("VFS timestamp self-test failed");
    }

    crate::println!("VFS self-test passed.");
}

fn with_vfs<T>(f: impl FnOnce(&mut Vfs) -> T) -> T {
    let mut guard = VFS.lock();
    let vfs = guard.as_mut().expect("VFS used before initialization");
    f(vfs)
}
