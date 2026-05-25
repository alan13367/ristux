use alloc::{collections::VecDeque, string::String, vec::Vec};
use core::{fmt, str};

use crate::{
    drivers,
    initrd::Initrd,
    security::{Access, Credentials, FileMetadata},
    sync::spinlock::SpinLock,
};

static VFS: SpinLock<Option<Vfs>> = SpinLock::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    File,
    Directory,
    Device(DeviceKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceKind {
    Null,
    Zero,
    Console,
    Keyboard,
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

#[derive(Clone, Copy)]
enum OpenHandle {
    Node {
        node: usize,
        offset: usize,
        rights: OpenRights,
    },
    PipeRead { pipe: usize },
    PipeWrite { pipe: usize },
}

#[derive(Clone, Copy)]
struct OpenRights {
    read: bool,
    write: bool,
}

impl OpenRights {
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
}

impl PipeState {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            capacity,
        }
    }

    fn read(&mut self, output: &mut [u8]) -> usize {
        let mut read = 0;
        for byte in output {
            let Some(value) = self.buffer.pop_front() else {
                break;
            };
            *byte = value;
            read += 1;
        }
        read
    }

    fn write(&mut self, input: &[u8]) -> usize {
        let mut written = 0;
        for byte in input {
            if self.buffer.len() == self.capacity {
                break;
            }
            self.buffer.push_back(*byte);
            written += 1;
        }
        written
    }
}

pub struct Vfs {
    nodes: Vec<Node>,
    open_files: Vec<Option<OpenHandle>>,
    pipes: Vec<PipeState>,
}

impl Vfs {
    fn new() -> Self {
        let mut vfs = Self {
            nodes: Vec::new(),
            open_files: Vec::new(),
            pipes: Vec::new(),
        };

        vfs.add_directory("/");
        vfs.add_directory("/bin");
        vfs.add_directory("/etc");
        vfs.add_directory("/lib");
        vfs.add_directory("/dev");
        vfs.add_directory("/pkg");
        vfs.add_directory("/proc");
        vfs.add_directory("/tmp");
        vfs.chmod("/tmp", 0o777).expect("failed to make /tmp writable");
        vfs.add_device("/dev/null", DeviceKind::Null);
        vfs.add_device("/dev/zero", DeviceKind::Zero);
        vfs.add_device("/dev/console", DeviceKind::Console);
        vfs.add_device("/dev/keyboard", DeviceKind::Keyboard);
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
            let node = self.nodes.iter().position(|node| node.path == path).unwrap();
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
            metadata: FileMetadata::new(creds.uid, creds.gid, 0o644),
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
        let node = self
            .nodes
            .iter()
            .position(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if self.nodes[node].kind == NodeKind::Directory {
            return Err(VfsError::NotFile);
        }
        if rights.read && !self.nodes[node].metadata.can_access(creds, Access::Read) {
            crate::println!(
                "VFS permission denied: uid {} cannot read {}.",
                creds.uid,
                path
            );
            return Err(VfsError::PermissionDenied);
        }
        if rights.write && !self.nodes[node].metadata.can_access(creds, Access::Write) {
            crate::println!(
                "VFS permission denied: uid {} cannot write {}.",
                creds.uid,
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
            .and_then(|slot| *slot)
            .ok_or(VfsError::BadFd)?;
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
            metadata: FileMetadata::new(creds.uid, creds.gid, 0o755),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: Vec::new(),
        });
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
        if node.kind != NodeKind::File {
            return Err(VfsError::NotFile);
        }
        node.path.clear();
        Ok(())
    }

    fn chmod_as(&mut self, path: &str, mode: u16, creds: Credentials) -> Result<(), VfsError> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if !creds.is_superuser() && creds.uid != node.metadata.owner {
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
                            Ok(count)
                        }
                        NodeKind::Device(DeviceKind::Console) => Ok(0),
                        NodeKind::Device(DeviceKind::Framebuffer) => Ok(0),
                        NodeKind::Directory => Err(VfsError::NotFile),
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
                Ok(pipe.read(output))
            }
            OpenHandle::PipeWrite { .. } => Err(VfsError::BadFd),
        }
    }

    fn write(&mut self, fd: usize, input: &[u8]) -> Result<usize, VfsError> {
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
                            crate::print!("{}", text);
                            Ok(input.len())
                        }
                        NodeKind::Device(DeviceKind::Framebuffer) => {
                            Ok(drivers::framebuffer::write_bytes(input))
                        }
                        NodeKind::Device(DeviceKind::Zero | DeviceKind::Keyboard) => Ok(0),
                        NodeKind::Directory => Err(VfsError::NotFile),
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
                Ok(pipe.write(input))
            }
            OpenHandle::PipeRead { .. } => Err(VfsError::BadFd),
        }
    }

    fn close(&mut self, fd: usize) -> Result<(), VfsError> {
        let Some(slot) = self.open_files.get_mut(fd) else {
            return Err(VfsError::BadFd);
        };
        *slot = None;
        Ok(())
    }

    fn chmod(&mut self, path: &str, mode: u16) -> Result<(), VfsError> {
        self.chmod_as(path, mode, Credentials::root())
    }

    fn can_access(&self, path: &str, creds: Credentials, access: Access) -> Result<bool, VfsError> {
        let node = self
            .nodes
            .iter()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        Ok(node.metadata.can_access(creds, access))
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let node = self.nodes.iter().find(|node| node.path == path)?;
        if node.kind == NodeKind::File {
            Some(node.data.clone())
        } else {
            None
        }
    }

    fn with_file_data<T>(&self, path: &str, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
        let node = self.nodes.iter().find(|node| node.path == path)?;
        if node.kind == NodeKind::File {
            Some(f(&node.data))
        } else {
            None
        }
    }

    fn timestamps(&self, path: &str) -> Option<FileTimestamps> {
        self.nodes
            .iter()
            .find(|node| node.path == path)
            .map(|node| node.timestamps)
    }

    fn list_paths(&self, prefix: &str) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|node| node.path.starts_with(prefix))
            .map(|node| node.path.clone())
            .collect()
    }
}

pub fn init(initrd: &Initrd) {
    let mut vfs = Vfs::new();
    vfs.mount_initrd(initrd);
    crate::println!("VFS mounted initrd, devfs, procfs, and tmpfs.");
    *VFS.lock() = Some(vfs);
}

pub fn open(path: &str) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.open(path))
}

pub fn open_read_as(path: &str, creds: Credentials) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.open_as(path, creds, OpenRights::read_only()))
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

pub fn unlink(path: &str) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.unlink(path))
}

pub fn unlink_as(path: &str, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.unlink_as(path, creds))
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

pub fn with_file_data<T>(path: &str, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
    let guard = VFS.lock();
    guard.as_ref().and_then(|vfs| vfs.with_file_data(path, f))
}

pub fn write_file(path: &str, data: &[u8]) {
    with_vfs(|vfs| vfs.add_file(path, data));
}

pub fn timestamps(path: &str) -> Option<FileTimestamps> {
    let guard = VFS.lock();
    guard.as_ref().and_then(|vfs| vfs.timestamps(path))
}

pub fn list_paths(prefix: &str) -> Vec<String> {
    let guard = VFS.lock();
    guard
        .as_ref()
        .map(|vfs| vfs.list_paths(prefix))
        .unwrap_or_default()
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
