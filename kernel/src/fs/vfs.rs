use alloc::{string::String, vec::Vec};
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
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::NotFile => f.write_str("not a file"),
            Self::BadFd => f.write_str("bad file descriptor"),
            Self::Utf8 => f.write_str("invalid utf-8"),
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

struct OpenFile {
    node: usize,
    offset: usize,
}

pub struct Vfs {
    nodes: Vec<Node>,
    open_files: Vec<Option<OpenFile>>,
}

impl Vfs {
    fn new() -> Self {
        let mut vfs = Self {
            nodes: Vec::new(),
            open_files: Vec::new(),
        };

        vfs.add_directory("/");
        vfs.add_directory("/bin");
        vfs.add_directory("/lib");
        vfs.add_directory("/dev");
        vfs.add_directory("/proc");
        vfs.add_directory("/tmp");
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
        let node = self
            .nodes
            .iter()
            .position(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if self.nodes[node].kind == NodeKind::Directory {
            return Err(VfsError::NotFile);
        }

        for (fd, slot) in self.open_files.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(OpenFile { node, offset: 0 });
                return Ok(fd);
            }
        }

        self.open_files.push(Some(OpenFile { node, offset: 0 }));
        Ok(self.open_files.len() - 1)
    }

    fn read(&mut self, fd: usize, output: &mut [u8]) -> Result<usize, VfsError> {
        let Some(open_file) = self.open_files.get_mut(fd).and_then(Option::as_mut) else {
            return Err(VfsError::BadFd);
        };
        let node = &self.nodes[open_file.node];

        match node.kind {
            NodeKind::File => {
                let remaining = node.data.len().saturating_sub(open_file.offset);
                let count = remaining.min(output.len());
                output[..count]
                    .copy_from_slice(&node.data[open_file.offset..open_file.offset + count]);
                open_file.offset += count;
                Ok(count)
            }
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
        }
    }

    fn write(&mut self, fd: usize, input: &[u8]) -> Result<usize, VfsError> {
        let Some(open_file) = self.open_files.get_mut(fd).and_then(Option::as_mut) else {
            return Err(VfsError::BadFd);
        };
        let node = &mut self.nodes[open_file.node];

        match node.kind {
            NodeKind::File => {
                if open_file.offset > node.data.len() {
                    node.data.resize(open_file.offset, 0);
                }
                let end = open_file.offset + input.len();
                if end > node.data.len() {
                    node.data.resize(end, 0);
                }
                node.data[open_file.offset..end].copy_from_slice(input);
                open_file.offset = end;
                node.timestamps.modified_at = crate::time::filesystem_timestamp();
                Ok(input.len())
            }
            NodeKind::Device(DeviceKind::Null) => Ok(input.len()),
            NodeKind::Device(DeviceKind::Console) => {
                let text = str::from_utf8(input).map_err(|_| VfsError::Utf8)?;
                crate::print!("{}", text);
                Ok(input.len())
            }
            NodeKind::Device(DeviceKind::Framebuffer) => Ok(drivers::framebuffer::write_bytes(input)),
            NodeKind::Device(DeviceKind::Zero | DeviceKind::Keyboard) => Ok(0),
            NodeKind::Directory => Err(VfsError::NotFile),
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
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        node.metadata.mode = crate::security::FileMode::new(mode);
        Ok(())
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
    let read = read(fd, &mut zeros).expect("read /dev/zero failed");
    close(fd).expect("close /dev/zero failed");
    if read != zeros.len() || zeros != [0; 8] {
        panic!("/dev/zero self-test failed");
    }

    let fd = open("/dev/null").expect("open /dev/null failed");
    let wrote = write(fd, b"discard me").expect("write /dev/null failed");
    close(fd).expect("close /dev/null failed");
    if wrote != b"discard me".len() {
        panic!("/dev/null self-test failed");
    }

    let fd = open("/dev/console").expect("open /dev/console failed");
    write(fd, b"console device online\n").expect("write /dev/console failed");
    close(fd).expect("close /dev/console failed");

    if read_file("/bin/init").is_none()
        || read_file("/lib/libc.so").is_none()
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
