use alloc::{string::String, vec::Vec};
use core::{fmt, str};

use crate::{drivers, initrd::Initrd, sync::spinlock::SpinLock};

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
        vfs.add_directory("/dev");
        vfs.add_directory("/proc");
        vfs.add_directory("/tmp");
        vfs.add_device("/dev/null", DeviceKind::Null);
        vfs.add_device("/dev/zero", DeviceKind::Zero);
        vfs.add_device("/dev/console", DeviceKind::Console);
        vfs.add_device("/dev/keyboard", DeviceKind::Keyboard);
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
        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::Directory,
            data: Vec::new(),
        });
    }

    fn add_file(&mut self, path: &str, data: &[u8]) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.path == path) {
            node.kind = NodeKind::File;
            node.data.clear();
            node.data.extend_from_slice(data);
            return;
        }

        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::File,
            data: Vec::from(data),
        });
    }

    fn add_device(&mut self, path: &str, kind: DeviceKind) {
        self.nodes.push(Node {
            path: String::from(path),
            kind: NodeKind::Device(kind),
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
                output[..count].copy_from_slice(&node.data[open_file.offset..open_file.offset + count]);
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
                Ok(input.len())
            }
            NodeKind::Device(DeviceKind::Null) => Ok(input.len()),
            NodeKind::Device(DeviceKind::Console) => {
                let text = str::from_utf8(input).map_err(|_| VfsError::Utf8)?;
                crate::print!("{}", text);
                Ok(input.len())
            }
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

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let node = self.nodes.iter().find(|node| node.path == path)?;
        if node.kind == NodeKind::File {
            Some(node.data.clone())
        } else {
            None
        }
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

pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let guard = VFS.lock();
    guard.as_ref().and_then(|vfs| vfs.read_file(path))
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

    if read_file("/bin/init").is_none() || read_file("/tmp/message.txt").is_none() {
        panic!("VFS path resolution self-test failed");
    }
    if list_paths("/dev").len() < 4 {
        panic!("devfs mount self-test failed");
    }

    crate::println!("VFS self-test passed.");
}

fn with_vfs<T>(f: impl FnOnce(&mut Vfs) -> T) -> T {
    let mut guard = VFS.lock();
    let vfs = guard.as_mut().expect("VFS used before initialization");
    f(vfs)
}
