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
static PTY_SIGNALS: SpinLock<Vec<(crate::process::Pid, crate::signal::Signal)>> =
    SpinLock::new(Vec::new());

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
    Random,
    URandom,
    Console,
    Serial,
    Block(BlockDevice),
    Keyboard,
    Tty,
    Ptmx,
    PtySlave(usize),
    Framebuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockDevice {
    pub start_sector: u64,
    pub sector_count: u64,
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
    TooManyOpenFiles,
    TooManyLinks,
    NoSpace,
    OutOfMemory,
    BrokenPipe,
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
            Self::TooManyOpenFiles => f.write_str("too many open files"),
            Self::TooManyLinks => f.write_str("too many links"),
            Self::NoSpace => f.write_str("no space left on device"),
            Self::OutOfMemory => f.write_str("out of memory"),
            Self::BrokenPipe => f.write_str("broken pipe"),
        }
    }
}

struct Node {
    path: String,
    kind: NodeKind,
    metadata: FileMetadata,
    timestamps: FileTimestamps,
    data: FileData,
    link_target: Option<usize>,
}

enum FileData {
    Owned(Vec<u8>),
    Static(&'static [u8]),
}

impl FileData {
    fn empty() -> Self {
        Self::Owned(Vec::new())
    }

    fn from_slice(data: &[u8]) -> Self {
        Self::Owned(Vec::from(data))
    }

    fn from_static(data: &'static [u8]) -> Self {
        Self::Static(data)
    }

    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(data) => data.as_slice(),
            Self::Static(data) => data,
        }
    }

    fn make_mut(&mut self) -> Result<&mut Vec<u8>, VfsError> {
        if let Self::Static(data) = self {
            let mut owned = Vec::new();
            owned
                .try_reserve_exact(data.len())
                .map_err(|_| VfsError::NoSpace)?;
            owned.extend_from_slice(data);
            *self = Self::Owned(owned);
        }
        match self {
            Self::Owned(data) => Ok(data),
            Self::Static(_) => unreachable!(),
        }
    }

    fn clear(&mut self) {
        *self = Self::Owned(Vec::new());
    }

    fn resize(&mut self, len: usize) -> Result<(), VfsError> {
        resize_file_buffer(self.make_mut()?, len)
    }

    fn as_mut_slice(&mut self) -> Result<&mut [u8], VfsError> {
        Ok(self.make_mut()?.as_mut_slice())
    }
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
    PtyMaster {
        pty: usize,
        rights: OpenRights,
    },
    PtySlave {
        pty: usize,
        rights: OpenRights,
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

struct PtyState {
    master_to_slave: VecDeque<u8>,
    slave_to_master: VecDeque<u8>,
    input_line: Vec<u8>,
    capacity: usize,
    masters: usize,
    slaves: usize,
    locked: bool,
    termios: [u8; crate::tty::TERMIOS_SIZE],
    winsize: [u8; 8],
    foreground_pgrp: crate::process::Pid,
}

#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    pub name: String,
    pub kind: NodeKind,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PollReady {
    pub read: bool,
    pub write: bool,
    pub error: bool,
    pub hangup: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FdRights {
    pub read: bool,
    pub write: bool,
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
        if input.is_empty() {
            return Ok(0);
        }
        if self.readers == 0 {
            return Err(VfsError::BrokenPipe);
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

    fn poll_read(&self) -> bool {
        !self.buffer.is_empty() || self.writers == 0
    }

    fn poll_write(&self) -> bool {
        self.readers > 0 && self.buffer.len() < self.capacity
    }
}

impl PtyState {
    fn new(capacity: usize) -> Self {
        let mut winsize = [0u8; 8];
        winsize[0..2].copy_from_slice(&24u16.to_le_bytes());
        winsize[2..4].copy_from_slice(&80u16.to_le_bytes());
        Self {
            master_to_slave: VecDeque::new(),
            slave_to_master: VecDeque::new(),
            input_line: Vec::new(),
            capacity,
            masters: 1,
            slaves: 0,
            locked: true,
            termios: crate::tty::default_termios_bytes(),
            winsize,
            foreground_pgrp: 1,
        }
    }

    fn add_master(&mut self) {
        self.masters += 1;
    }

    fn add_slave(&mut self) {
        self.slaves += 1;
    }

    fn close_master(&mut self) {
        self.masters = self.masters.saturating_sub(1);
    }

    fn close_slave(&mut self) {
        self.slaves = self.slaves.saturating_sub(1);
    }

    fn read_master(&mut self, output: &mut [u8]) -> Result<usize, VfsError> {
        read_queue(&mut self.slave_to_master, self.slaves, output)
    }

    fn read_slave(&mut self, output: &mut [u8]) -> Result<usize, VfsError> {
        read_queue(&mut self.master_to_slave, self.masters, output)
    }

    fn write_master(&mut self, input: &[u8]) -> Result<usize, VfsError> {
        self.write_master_input(input)
    }

    fn write_slave(&mut self, input: &[u8]) -> Result<usize, VfsError> {
        self.write_slave_output(input)
    }

    fn write_slave_output(&mut self, input: &[u8]) -> Result<usize, VfsError> {
        const OFLAG_OPOST: u32 = 0x1;
        const OFLAG_ONLCR: u32 = 0x4;
        const TERMIOS_OFLAG: usize = 4;

        if input.is_empty() {
            return Ok(0);
        }
        if self.masters == 0 {
            return Err(VfsError::BadFd);
        }

        let oflag = u32::from_le_bytes([
            self.termios[TERMIOS_OFLAG],
            self.termios[TERMIOS_OFLAG + 1],
            self.termios[TERMIOS_OFLAG + 2],
            self.termios[TERMIOS_OFLAG + 3],
        ]);
        let translate_newline = oflag & (OFLAG_OPOST | OFLAG_ONLCR) == (OFLAG_OPOST | OFLAG_ONLCR);

        let mut written = 0usize;
        for byte in input {
            let required = if translate_newline && *byte == b'\n' {
                2
            } else {
                1
            };
            if self.slave_to_master.len().saturating_add(required) > self.capacity {
                break;
            }
            if translate_newline && *byte == b'\n' {
                self.slave_to_master.push_back(b'\r');
            }
            self.slave_to_master.push_back(*byte);
            written += 1;
        }
        if written == 0 {
            return Err(VfsError::WouldBlock);
        }
        Ok(written)
    }

    fn write_master_input(&mut self, input: &[u8]) -> Result<usize, VfsError> {
        const IFLAG_ICRNL: u32 = 0x100;
        const LFLAG_ISIG: u32 = 0x1;
        const LFLAG_ICANON: u32 = 0x2;
        const LFLAG_ECHO: u32 = 0x8;
        const VINTR: usize = 0;
        const VQUIT: usize = 1;
        const VERASE: usize = 2;
        const VEOF: usize = 4;
        const VSUSP: usize = 10;
        const TERMIOS_IFLAG: usize = 0;
        const TERMIOS_LFLAG: usize = 12;

        if input.is_empty() {
            return Ok(0);
        }
        if self.slaves == 0 {
            return Err(VfsError::BadFd);
        }

        let iflag = u32::from_le_bytes([
            self.termios[TERMIOS_IFLAG],
            self.termios[TERMIOS_IFLAG + 1],
            self.termios[TERMIOS_IFLAG + 2],
            self.termios[TERMIOS_IFLAG + 3],
        ]);
        let lflag = u32::from_le_bytes([
            self.termios[TERMIOS_LFLAG],
            self.termios[TERMIOS_LFLAG + 1],
            self.termios[TERMIOS_LFLAG + 2],
            self.termios[TERMIOS_LFLAG + 3],
        ]);
        let isig = lflag & LFLAG_ISIG != 0;
        let canonical = lflag & LFLAG_ICANON != 0;
        let echo = lflag & LFLAG_ECHO != 0;
        let mut written = 0;
        for byte in input {
            let mut byte = *byte;
            if byte == b'\r' && iflag & IFLAG_ICRNL != 0 {
                byte = b'\n';
            }
            let signal = if isig && Some(byte) == control_char(&self.termios, VINTR) {
                Some(crate::signal::Signal::Int)
            } else if isig && Some(byte) == control_char(&self.termios, VQUIT) {
                Some(crate::signal::Signal::Quit)
            } else if isig && Some(byte) == control_char(&self.termios, VSUSP) {
                Some(crate::signal::Signal::Tstp)
            } else {
                None
            };
            if let Some(signal) = signal {
                queue_pty_signal(self.foreground_pgrp, signal);
                written += 1;
                continue;
            }

            if canonical {
                let erase = Some(byte) == control_char(&self.termios, VERASE)
                    || matches!(byte, 0x08 | 0x7f);
                if erase {
                    if !self.input_line.is_empty() {
                        self.input_line.pop();
                        if echo {
                            let _ = push_pty_output(&mut self.slave_to_master, self.capacity, 0x08);
                            let _ = push_pty_output(&mut self.slave_to_master, self.capacity, b' ');
                            let _ = push_pty_output(&mut self.slave_to_master, self.capacity, 0x08);
                        }
                    }
                    written += 1;
                    continue;
                }
                if Some(byte) == control_char(&self.termios, VEOF) {
                    if !self.input_line.is_empty() {
                        commit_pty_line(
                            &mut self.master_to_slave,
                            &mut self.input_line,
                            self.capacity,
                        );
                    }
                    written += 1;
                    continue;
                }
                if self.input_line.len() == self.capacity {
                    break;
                }
                self.input_line.push(byte);
                if echo {
                    let _ = self.write_slave_output(core::slice::from_ref(&byte));
                }
                if byte == b'\n' {
                    commit_pty_line(
                        &mut self.master_to_slave,
                        &mut self.input_line,
                        self.capacity,
                    );
                }
            } else {
                if self.master_to_slave.len() == self.capacity {
                    break;
                }
                self.master_to_slave.push_back(byte);
            }
            written += 1;
        }
        if written == 0 {
            return Err(VfsError::WouldBlock);
        }
        Ok(written)
    }
}

fn push_pty_output(queue: &mut VecDeque<u8>, capacity: usize, byte: u8) -> Result<(), VfsError> {
    if queue.len() == capacity {
        return Err(VfsError::WouldBlock);
    }
    queue.push_back(byte);
    Ok(())
}

fn commit_pty_line(queue: &mut VecDeque<u8>, line: &mut Vec<u8>, capacity: usize) {
    while !line.is_empty() && queue.len() < capacity {
        queue.push_back(line.remove(0));
    }
    if line.is_empty() {
        return;
    }
    line.clear();
}

fn control_char(termios: &[u8; crate::tty::TERMIOS_SIZE], index: usize) -> Option<u8> {
    const TERMIOS_CC: usize = 17;
    let byte = *termios.get(TERMIOS_CC + index)?;
    if byte == 0 { None } else { Some(byte) }
}

fn queue_pty_signal(pgrp: crate::process::Pid, signal: crate::signal::Signal) {
    PTY_SIGNALS.lock().push((pgrp, signal));
}

fn deliver_pty_signals() {
    let signals = core::mem::take(&mut *PTY_SIGNALS.lock());
    for (pgrp, signal) in signals {
        let _ = crate::signal::send_pgrp(pgrp, signal);
    }
}

fn read_queue(
    queue: &mut VecDeque<u8>,
    writers: usize,
    output: &mut [u8],
) -> Result<usize, VfsError> {
    if output.is_empty() {
        return Ok(0);
    }
    let mut read = 0;
    for byte in output {
        let Some(value) = queue.pop_front() else {
            break;
        };
        *byte = value;
        read += 1;
    }
    if read == 0 && writers > 0 {
        return Err(VfsError::WouldBlock);
    }
    Ok(read)
}

fn resize_file_buffer(data: &mut Vec<u8>, len: usize) -> Result<(), VfsError> {
    if len <= data.len() {
        data.truncate(len);
        return Ok(());
    }
    data.try_reserve_exact(len - data.len())
        .map_err(|_| VfsError::NoSpace)?;
    data.resize(len, 0);
    Ok(())
}

fn checked_file_end(offset: usize, len: usize) -> Result<usize, VfsError> {
    let end = offset.checked_add(len).ok_or(VfsError::NoSpace)?;
    if end > isize::MAX as usize {
        return Err(VfsError::NoSpace);
    }
    Ok(end)
}

fn checked_seek_offset(base: usize, offset: isize) -> Result<usize, VfsError> {
    let target = if offset >= 0 {
        base.checked_add(offset as usize).ok_or(VfsError::BadFd)?
    } else {
        let delta = offset.checked_neg().ok_or(VfsError::BadFd)? as usize;
        base.checked_sub(delta).ok_or(VfsError::BadFd)?
    };
    if target > isize::MAX as usize {
        return Err(VfsError::BadFd);
    }
    Ok(target)
}

fn seek_target(cursor: usize, size: usize, offset: isize, whence: u32) -> Result<usize, VfsError> {
    match whence {
        0 => {
            if offset < 0 {
                Err(VfsError::BadFd)
            } else {
                Ok(offset as usize)
            }
        }
        1 => checked_seek_offset(cursor, offset),
        2 => checked_seek_offset(size, offset),
        _ => Err(VfsError::BadFd),
    }
}

pub struct Vfs {
    nodes: Vec<Node>,
    open_files: Vec<Option<OpenHandle>>,
    pipes: Vec<PipeState>,
    ptys: Vec<PtyState>,
    mounts: Vec<MountPoint>,
}

#[derive(Clone)]
struct MountPoint {
    mountpoint: String,
    ext2: Option<ext2::Ext2Fs>,
}

impl Vfs {
    fn new() -> Self {
        let mut vfs = Self {
            nodes: Vec::new(),
            open_files: Vec::new(),
            pipes: Vec::new(),
            ptys: Vec::new(),
            mounts: Vec::new(),
        };

        vfs.add_directory("/");
        vfs.add_directory("/bin");
        vfs.add_directory("/etc");
        vfs.add_directory("/lib");
        vfs.add_directory("/dev");
        vfs.add_directory("/dev/pts");
        vfs.add_directory("/pkg");
        vfs.add_directory("/proc");
        vfs.add_directory("/tmp");
        vfs.chmod("/tmp", 0o777)
            .expect("failed to make /tmp writable");
        vfs.add_device("/dev/null", DeviceKind::Null);
        vfs.add_device("/dev/zero", DeviceKind::Zero);
        vfs.add_device("/dev/random", DeviceKind::Random);
        vfs.add_device("/dev/urandom", DeviceKind::URandom);
        vfs.add_device("/dev/console", DeviceKind::Console);
        vfs.add_device("/dev/serial", DeviceKind::Serial);
        vfs.add_device(
            "/dev/vda",
            DeviceKind::Block(BlockDevice {
                start_sector: 0,
                sector_count: 0,
            }),
        );
        vfs.add_device("/dev/keyboard", DeviceKind::Keyboard);
        vfs.add_device("/dev/tty", DeviceKind::Tty);
        vfs.add_device("/dev/ptmx", DeviceKind::Ptmx);
        vfs.add_device("/dev/fb0", DeviceKind::Framebuffer);
        vfs.add_file("/proc/version", b"ristux 0.1\n");
        vfs.add_file("/tmp/message.txt", b"hello from tmpfs\n");
        vfs
    }

    fn mount_initrd(&mut self, initrd: &Initrd) {
        for file in initrd.files() {
            self.add_static_file(file.path, file.data);
        }
    }

    fn mount(&mut self, device: &str, mountpoint: &str, fstype: &str) -> Result<(), VfsError> {
        let mountpoint = normalize_path(mountpoint)?;
        let mountpoint = mountpoint.as_str();
        if !self.nodes.iter().any(|node| node.path == mountpoint) {
            self.add_directory(mountpoint);
        }
        if fstype == "ext2" {
            let start_sector = self
                .block_device_for_name(device)
                .map(|device| device.start_sector)
                .unwrap_or(0);
            let fs = ext2::Ext2Fs::mount_at(start_sector).map_err(|_| VfsError::NotFound)?;
            self.mounts.push(MountPoint {
                mountpoint: String::from(mountpoint),
                ext2: Some(fs),
            });
            crate::println!("VFS mounted {} on {}.", fstype, mountpoint);
            return Ok(());
        }
        Err(VfsError::NotFound)
    }

    fn refresh_block_devices(&mut self) {
        let sectors = drivers::virtio_blk::sector_count();
        self.upsert_device(
            "/dev/vda",
            DeviceKind::Block(BlockDevice {
                start_sector: 0,
                sector_count: sectors,
            }),
        );
        for index in 1..=4 {
            self.remove_node_path(&format!("/dev/vda{}", index));
        }
        let mut mbr = [0u8; 512];
        if sectors == 0 || drivers::virtio_blk::read_sectors(0, 1, &mut mbr).is_err() {
            return;
        }
        if mbr[510] != 0x55 || mbr[511] != 0xaa {
            return;
        }
        for index in 0..4 {
            let offset = 446 + index * 16;
            let part_type = mbr[offset + 4];
            let start = u32::from_le_bytes([
                mbr[offset + 8],
                mbr[offset + 9],
                mbr[offset + 10],
                mbr[offset + 11],
            ]) as u64;
            let count = u32::from_le_bytes([
                mbr[offset + 12],
                mbr[offset + 13],
                mbr[offset + 14],
                mbr[offset + 15],
            ]) as u64;
            if part_type == 0 || start == 0 || count == 0 {
                continue;
            }
            self.upsert_device(
                &format!("/dev/vda{}", index + 1),
                DeviceKind::Block(BlockDevice {
                    start_sector: start,
                    sector_count: count,
                }),
            );
        }
    }

    fn upsert_device(&mut self, path: &str, kind: DeviceKind) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.path == path) {
            node.kind = NodeKind::Device(kind);
            node.metadata = FileMetadata::new(0, 0, 0o666);
            return;
        }
        self.add_device(path, kind);
    }

    fn remove_node_path(&mut self, path: &str) {
        if let Some(index) = self.nodes.iter().position(|node| node.path == path) {
            self.nodes.remove(index);
        }
    }

    fn block_device_for_name(&self, name: &str) -> Option<BlockDevice> {
        let path = if name.starts_with("/dev/") {
            String::from(name)
        } else {
            format!("/dev/{}", name)
        };
        self.nodes.iter().find_map(|node| match node.kind {
            NodeKind::Device(DeviceKind::Block(device)) if node.path == path => Some(device),
            _ => None,
        })
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

    fn statfs(&self, path: &str) -> Result<FsStat, VfsError> {
        const EXT2_SUPER_MAGIC: u64 = 0xef53;
        const TMPFS_MAGIC: u64 = 0x0102_1994;
        const PROC_SUPER_MAGIC: u64 = 0x9fa0;
        const DEVFS_MAGIC: u64 = 0x1373;
        const INITRD_MAGIC: u64 = 0x8584_58f6;

        let normalized = normalize_path(path)?;
        let path = normalized.as_str();
        if Self::use_root_ext2(path) {
            let stats = self.root_ext2().ok_or(VfsError::NotFound)?.stats();
            return Ok(FsStat {
                fs_type: EXT2_SUPER_MAGIC,
                block_size: stats.block_size as u64,
                blocks: stats.blocks_count as u64,
                blocks_free: stats.free_blocks_count as u64,
                blocks_available: stats.free_blocks_count as u64,
                files: stats.inodes_count as u64,
                files_free: stats.free_inodes_count as u64,
                name_max: 255,
            });
        }
        let (fs_type, block_size, blocks, files) = if path == "/tmp" || path.starts_with("/tmp/") {
            (TMPFS_MAGIC, 1024, 1024, 1024)
        } else if path == "/proc" || path.starts_with("/proc/") {
            (PROC_SUPER_MAGIC, 1024, 0, 0)
        } else if path == "/dev" || path.starts_with("/dev/") {
            (DEVFS_MAGIC, 1024, 0, 0)
        } else if path == "/initrd" || path.starts_with("/initrd/") {
            (INITRD_MAGIC, 1024, 0, 0)
        } else {
            return Err(VfsError::NotFound);
        };
        Ok(FsStat {
            fs_type,
            block_size,
            blocks,
            blocks_free: blocks,
            blocks_available: blocks,
            files,
            files_free: files,
            name_max: 255,
        })
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
            data: FileData::empty(),
            link_target: None,
        });
    }

    fn add_file(&mut self, path: &str, data: &[u8]) {
        let now = crate::time::filesystem_timestamp();
        if let Some(node) = self.nodes.iter_mut().find(|node| node.path == path) {
            node.kind = NodeKind::File;
            node.metadata = FileMetadata::new(0, 0, 0o644);
            node.timestamps.modified_at = now;
            node.data = FileData::from_slice(data);
            node.link_target = None;
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
            data: FileData::from_slice(data),
            link_target: None,
        });
    }

    fn add_static_file(&mut self, path: &str, data: &'static [u8]) {
        let now = crate::time::filesystem_timestamp();
        if let Some(node) = self.nodes.iter_mut().find(|node| node.path == path) {
            node.kind = NodeKind::File;
            node.metadata = FileMetadata::new(0, 0, 0o644);
            node.timestamps.modified_at = now;
            node.data = FileData::from_static(data);
            node.link_target = None;
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
            data: FileData::from_static(data),
            link_target: None,
        });
    }

    fn create_file(&mut self, path: &str) -> Result<usize, VfsError> {
        self.create_file_as(path, Credentials::root())
    }

    fn create_file_as(&mut self, path: &str, creds: Credentials) -> Result<usize, VfsError> {
        self.create_file_with_mode_as(path, creds, 0o644)
    }

    fn create_file_with_mode_as(
        &mut self,
        path: &str,
        creds: Credentials,
        mode: u16,
    ) -> Result<usize, VfsError> {
        let normalized = normalize_path(path)?;
        let path = normalized.as_str();
        let mode = mode & 0o7777;
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
                        let parent_path = parent_path(path)?;
                        let parent = fs.metadata(&parent_path).map_err(map_ext2_error)?;
                        if parent.kind != ext2::Ext2NodeKind::Directory {
                            return Err(VfsError::NotFile);
                        }
                        if !parent.metadata.can_access(creds, Access::Write) {
                            return Err(VfsError::PermissionDenied);
                        }
                        fs.create_file(path, creds.euid, creds.egid, mode)
                            .map_err(map_ext2_error)?;
                    }
                    Err(err) => return Err(map_ext2_error(err)),
                }
                return self.push_open_handle(OpenHandle::Ext2File {
                    path: try_string_from(path)?,
                    offset: 0,
                    rights: OpenRights::read_write(),
                });
            }
        }

        self.ensure_parent_directory(path, Some((creds, Access::Write)))?;
        if let Some(node) = self.nodes.iter().position(|node| node.path == path) {
            let target = canonical_node_index(&self.nodes, node);
            if self.nodes[target].kind != NodeKind::File {
                return Err(VfsError::NotFile);
            }
            if !self.nodes[target].metadata.can_access(creds, Access::Write) {
                return Err(VfsError::PermissionDenied);
            }
            self.nodes[target].data.clear();
            self.nodes[target].timestamps.modified_at = crate::time::filesystem_timestamp();
            return self.push_open_handle(OpenHandle::Node {
                node,
                offset: 0,
                rights: OpenRights::read_write(),
            });
        }

        let now = crate::time::filesystem_timestamp();
        self.reserve_open_handle_slots(1)?;
        let node = self.push_node(Node {
            path: try_string_from(path)?,
            kind: NodeKind::File,
            metadata: FileMetadata::new(creds.euid, creds.egid, mode),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: FileData::empty(),
            link_target: None,
        })?;
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
            data: FileData::empty(),
            link_target: None,
        });
    }

    fn push_node(&mut self, node: Node) -> Result<usize, VfsError> {
        self.nodes
            .try_reserve_exact(1)
            .map_err(|_| VfsError::OutOfMemory)?;
        let index = self.nodes.len();
        self.nodes.push(node);
        Ok(index)
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
        } else if let Some(kind) = proc_virtual_kind(path) {
            self.reserve_open_handle_slots(1)?;
            self.push_node(Node {
                path: try_string_from(path)?,
                kind,
                metadata: FileMetadata::new(
                    0,
                    0,
                    if kind == NodeKind::Directory {
                        0o555
                    } else {
                        0o444
                    },
                ),
                timestamps: FileTimestamps {
                    created_at: crate::time::filesystem_timestamp(),
                    modified_at: crate::time::filesystem_timestamp(),
                },
                data: FileData::empty(),
                link_target: None,
            })?
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
                        path: try_string_from(path)?,
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
                    path: try_string_from(path)?,
                    offset: 0,
                    rights,
                });
            }
            return Err(VfsError::NotFound);
        } else {
            return Err(VfsError::NotFound);
        };
        let metadata_node = canonical_node_index(&self.nodes, node);
        if self.nodes[node].kind == NodeKind::Directory && rights.write {
            return Err(VfsError::NotFile);
        }
        if rights.read
            && !self.nodes[metadata_node]
                .metadata
                .can_access(creds, Access::Read)
        {
            crate::println!(
                "VFS permission denied: uid {} cannot read {}.",
                creds.euid,
                path
            );
            return Err(VfsError::PermissionDenied);
        }
        if rights.write
            && !self.nodes[metadata_node]
                .metadata
                .can_access(creds, Access::Write)
        {
            crate::println!(
                "VFS permission denied: uid {} cannot write {}.",
                creds.euid,
                path
            );
            return Err(VfsError::PermissionDenied);
        }

        match self.nodes[node].kind {
            NodeKind::Device(DeviceKind::Ptmx) => return self.open_pty_master(rights),
            NodeKind::Device(DeviceKind::PtySlave(pty)) => return self.open_pty_slave(pty, rights),
            _ => {}
        }

        self.push_open_handle(OpenHandle::Node {
            node,
            offset: 0,
            rights,
        })
    }

    fn push_open_handle(&mut self, handle: OpenHandle) -> Result<usize, VfsError> {
        self.reserve_open_handle_slots(1)?;
        for (fd, slot) in self.open_files.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(handle);
                return Ok(fd);
            }
        }

        self.open_files.push(Some(handle));
        Ok(self.open_files.len() - 1)
    }

    fn reserve_open_handle_slots(&mut self, count: usize) -> Result<(), VfsError> {
        let free_slots = self.open_files.iter().filter(|slot| slot.is_none()).count();
        if free_slots >= count {
            return Ok(());
        }
        self.open_files
            .try_reserve_exact(count - free_slots)
            .map_err(|_| VfsError::OutOfMemory)
    }

    fn create_pipe(&mut self, capacity: usize) -> Result<(usize, usize), VfsError> {
        self.reserve_open_handle_slots(2)?;
        self.pipes
            .try_reserve_exact(1)
            .map_err(|_| VfsError::OutOfMemory)?;
        let pipe = self.pipes.len();
        self.pipes.push(PipeState::new(capacity));
        let read_fd = self.push_open_handle(OpenHandle::PipeRead { pipe })?;
        let write_fd = self.push_open_handle(OpenHandle::PipeWrite { pipe })?;
        Ok((read_fd, write_fd))
    }

    fn open_pty_master(&mut self, rights: OpenRights) -> Result<usize, VfsError> {
        self.reserve_open_handle_slots(1)?;
        self.ptys
            .try_reserve_exact(1)
            .map_err(|_| VfsError::OutOfMemory)?;
        let pty = self.ptys.len();
        self.ptys.push(PtyState::new(4096));
        if let Err(err) = self.ensure_pty_slave_node(pty) {
            self.ptys.pop();
            return Err(err);
        }
        self.push_open_handle(OpenHandle::PtyMaster { pty, rights })
    }

    fn open_pty_slave(&mut self, pty: usize, rights: OpenRights) -> Result<usize, VfsError> {
        if self.ptys.get(pty).ok_or(VfsError::NotFound)?.locked {
            return Err(VfsError::PermissionDenied);
        }
        self.reserve_open_handle_slots(1)?;
        self.ptys
            .get_mut(pty)
            .ok_or(VfsError::NotFound)?
            .add_slave();
        self.push_open_handle(OpenHandle::PtySlave { pty, rights })
    }

    fn ensure_pty_slave_node(&mut self, pty: usize) -> Result<(), VfsError> {
        let path = try_pty_slave_path(pty)?;
        if self.nodes.iter().any(|node| node.path == path) {
            return Ok(());
        }
        self.nodes
            .try_reserve_exact(1)
            .map_err(|_| VfsError::OutOfMemory)?;
        let now = crate::time::filesystem_timestamp();
        self.nodes.push(Node {
            path,
            kind: NodeKind::Device(DeviceKind::PtySlave(pty)),
            metadata: FileMetadata::new(0, 0, 0o666),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: FileData::empty(),
            link_target: None,
        });
        Ok(())
    }

    fn duplicate_fd(&mut self, fd: usize) -> Result<usize, VfsError> {
        let handle = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref().cloned())
            .ok_or(VfsError::BadFd)?;
        self.reserve_open_handle_slots(1)?;
        self.retain_handle(&handle)?;
        self.push_open_handle(handle)
    }

    fn mkdir(&mut self, path: &str) -> Result<(), VfsError> {
        self.mkdir_as(path, Credentials::root())
    }

    fn mkdir_as(&mut self, path: &str, creds: Credentials) -> Result<(), VfsError> {
        self.mkdir_with_mode_as(path, creds, 0o755)
    }

    fn mkdir_with_mode_as(
        &mut self,
        path: &str,
        creds: Credentials,
        mode: u16,
    ) -> Result<(), VfsError> {
        let normalized = normalize_path(path)?;
        let path = normalized.as_str();
        if Self::use_root_ext2(path) {
            let parent_path = parent_path(path)?;
            if let Some(fs) = self.root_ext2_mut() {
                if fs.metadata(path).is_ok() {
                    return Err(VfsError::AlreadyExists);
                }
                let parent = fs.metadata(&parent_path).map_err(map_ext2_error)?;
                if parent.kind != ext2::Ext2NodeKind::Directory {
                    return Err(VfsError::NotFile);
                }
                if !parent.metadata.can_access(creds, Access::Write)
                    || !parent.metadata.can_access(creds, Access::Execute)
                {
                    return Err(VfsError::PermissionDenied);
                }
                return fs
                    .create_dir(path, creds.euid, creds.egid, mode)
                    .map_err(map_ext2_error);
            }
        }
        if self.nodes.iter().any(|node| node.path == path) {
            return Err(VfsError::AlreadyExists);
        }
        self.ensure_parent_directory(path, Some((creds, Access::Write)))?;
        let now = crate::time::filesystem_timestamp();
        self.push_node(Node {
            path: try_string_from(path)?,
            kind: NodeKind::Directory,
            metadata: FileMetadata::new(creds.euid, creds.egid, mode & 0o7777),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: FileData::empty(),
            link_target: None,
        })?;
        Ok(())
    }

    fn rmdir_as(&mut self, path: &str, creds: Credentials) -> Result<(), VfsError> {
        let normalized = normalize_path(path)?;
        let path = normalized.as_str();
        if path == "/" {
            return Err(VfsError::PermissionDenied);
        }
        if Self::use_root_ext2(path) {
            let parent_path = parent_path(path)?;
            if let Some(fs) = self.root_ext2_mut() {
                let parent = fs.metadata(&parent_path).map_err(map_ext2_error)?;
                if parent.kind != ext2::Ext2NodeKind::Directory {
                    return Err(VfsError::NotFile);
                }
                if !parent.metadata.can_access(creds, Access::Write)
                    || !parent.metadata.can_access(creds, Access::Execute)
                {
                    return Err(VfsError::PermissionDenied);
                }
                return fs.rmdir(path).map_err(map_ext2_error);
            }
        }
        self.ensure_parent_directory(path, Some((creds, Access::Write)))?;
        for node in &self.nodes {
            if node.path.is_empty() || node.path == path {
                continue;
            }
            if parent_path(&node.path)? == path {
                return Err(VfsError::PermissionDenied);
            }
        }
        let node = self
            .nodes
            .iter()
            .position(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        let node = canonical_node_index(&self.nodes, node);
        let node = &mut self.nodes[node];
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
        let normalized = normalize_path(path)?;
        let path = normalized.as_str();
        if Self::use_root_ext2(path) {
            let parent_path = parent_path(path)?;
            if let Some(fs) = self.root_ext2_mut() {
                let parent = fs.metadata(&parent_path).map_err(map_ext2_error)?;
                if parent.kind != ext2::Ext2NodeKind::Directory {
                    return Err(VfsError::NotFile);
                }
                if !parent.metadata.can_access(creds, Access::Write)
                    || !parent.metadata.can_access(creds, Access::Execute)
                {
                    return Err(VfsError::PermissionDenied);
                }
                return fs.unlink(path).map_err(map_ext2_error);
            }
        }
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

    fn rename_as(
        &mut self,
        old_path: &str,
        new_path: &str,
        creds: Credentials,
    ) -> Result<(), VfsError> {
        let normalized_old = normalize_path(old_path)?;
        let normalized_new = normalize_path(new_path)?;
        let old_path = normalized_old.as_str();
        let new_path = normalized_new.as_str();
        if old_path == "/" || new_path == "/" {
            return Err(VfsError::PermissionDenied);
        }
        if Self::use_root_ext2(old_path) || Self::use_root_ext2(new_path) {
            if !Self::use_root_ext2(old_path) || !Self::use_root_ext2(new_path) {
                return Err(VfsError::NotFound);
            }
            let old_parent_path = parent_path(old_path)?;
            let new_parent_path = parent_path(new_path)?;
            if let Some(fs) = self.root_ext2_mut() {
                let old_parent = fs.metadata(&old_parent_path).map_err(map_ext2_error)?;
                let new_parent = fs.metadata(&new_parent_path).map_err(map_ext2_error)?;
                if old_parent.kind != ext2::Ext2NodeKind::Directory
                    || new_parent.kind != ext2::Ext2NodeKind::Directory
                {
                    return Err(VfsError::NotFile);
                }
                if !old_parent.metadata.can_access(creds, Access::Write)
                    || !old_parent.metadata.can_access(creds, Access::Execute)
                    || !new_parent.metadata.can_access(creds, Access::Write)
                    || !new_parent.metadata.can_access(creds, Access::Execute)
                {
                    return Err(VfsError::PermissionDenied);
                }
                return fs.rename(old_path, new_path).map_err(map_ext2_error);
            }
        }
        self.ensure_parent_directory(old_path, Some((creds, Access::Write)))?;
        self.ensure_parent_directory(new_path, Some((creds, Access::Write)))?;
        let old_index = self
            .nodes
            .iter()
            .position(|node| node.path == old_path)
            .ok_or(VfsError::NotFound)?;
        let replaced_index = self
            .nodes
            .iter()
            .position(|node| node.path == new_path)
            .filter(|index| *index != old_index);
        let old_prefix = child_path_prefix(old_path)?;
        let new_prefix = child_path_prefix(new_path)?;
        let renamed_path = try_string_from(new_path)?;
        let mut child_rewrites: Vec<(usize, String)> = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if Some(index) == replaced_index {
                continue;
            }
            if node.path.starts_with(&old_prefix) {
                let rewritten = replace_path_prefix(&node.path, &old_prefix, &new_prefix)?;
                child_rewrites
                    .try_reserve_exact(1)
                    .map_err(|_| VfsError::OutOfMemory)?;
                child_rewrites.push((index, rewritten));
            }
        }
        if let Some(new_index) = replaced_index {
            self.nodes[new_index].path.clear();
        }
        self.nodes[old_index].path = renamed_path;
        for (index, rewritten) in child_rewrites {
            self.nodes[index].path = rewritten;
        }
        Ok(())
    }

    fn symlink_as(
        &mut self,
        target: &str,
        link_path: &str,
        creds: Credentials,
    ) -> Result<(), VfsError> {
        let normalized_link = normalize_path(link_path)?;
        let link_path = normalized_link.as_str();
        if Self::use_root_ext2(link_path) {
            let parent_path = parent_path(link_path)?;
            if let Some(fs) = self.root_ext2_mut() {
                if fs.lstat_metadata(link_path).is_ok() {
                    return Err(VfsError::AlreadyExists);
                }
                let parent = fs.metadata(&parent_path).map_err(map_ext2_error)?;
                if parent.kind != ext2::Ext2NodeKind::Directory {
                    return Err(VfsError::NotFile);
                }
                if !parent.metadata.can_access(creds, Access::Write)
                    || !parent.metadata.can_access(creds, Access::Execute)
                {
                    return Err(VfsError::PermissionDenied);
                }
                return fs
                    .symlink(target, link_path, creds.euid, creds.egid)
                    .map_err(map_ext2_error);
            }
        }
        if self.nodes.iter().any(|node| node.path == link_path) {
            return Err(VfsError::AlreadyExists);
        }
        self.ensure_parent_directory(link_path, Some((creds, Access::Write)))?;
        let now = crate::time::filesystem_timestamp();
        self.push_node(Node {
            path: try_string_from(link_path)?,
            kind: NodeKind::Symlink,
            metadata: FileMetadata::new(creds.euid, creds.egid, 0o777),
            timestamps: FileTimestamps {
                created_at: now,
                modified_at: now,
            },
            data: FileData::Owned(try_vec_from_bytes(target.as_bytes())?),
            link_target: None,
        })?;
        Ok(())
    }

    fn link_as(
        &mut self,
        old_path: &str,
        new_path: &str,
        creds: Credentials,
    ) -> Result<(), VfsError> {
        let resolved_old = self.resolve_symlink_path(old_path)?;
        let normalized_new = normalize_path(new_path)?;
        let old_path = resolved_old.as_str();
        let new_path = normalized_new.as_str();

        if Self::use_root_ext2(old_path) || Self::use_root_ext2(new_path) {
            if !Self::use_root_ext2(old_path) || !Self::use_root_ext2(new_path) {
                return Err(VfsError::NotFound);
            }
            let parent_path = parent_path(new_path)?;
            let fs = self.root_ext2_mut().ok_or(VfsError::NotFound)?;
            let parent = fs.metadata(&parent_path).map_err(map_ext2_error)?;
            if parent.kind != ext2::Ext2NodeKind::Directory {
                return Err(VfsError::NotFile);
            }
            if !parent.metadata.can_access(creds, Access::Write) {
                return Err(VfsError::PermissionDenied);
            }
            return fs.link(old_path, new_path).map_err(map_ext2_error);
        }

        if self.nodes.iter().any(|node| node.path == new_path) {
            return Err(VfsError::AlreadyExists);
        }
        self.ensure_parent_directory(new_path, Some((creds, Access::Write)))?;
        let source = self
            .nodes
            .iter()
            .position(|node| node.path == old_path)
            .ok_or(VfsError::NotFound)?;
        let target = canonical_node_index(&self.nodes, source);
        if self.nodes[target].kind != NodeKind::File {
            return Err(VfsError::NotFile);
        }
        let now = crate::time::filesystem_timestamp();
        let metadata = self.nodes[target].metadata;
        let modified_at = self.nodes[target].timestamps.modified_at;
        self.push_node(Node {
            path: try_string_from(new_path)?,
            kind: NodeKind::File,
            metadata,
            timestamps: FileTimestamps {
                created_at: now,
                modified_at,
            },
            data: FileData::empty(),
            link_target: Some(target),
        })?;
        Ok(())
    }

    fn readlink(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let normalized = normalize_path(path)?;
        let path = normalized.as_str();
        if Self::use_root_ext2(path) {
            return self
                .root_ext2()
                .ok_or(VfsError::NotFound)?
                .readlink(path)
                .map_err(map_ext2_error);
        }
        let node = self
            .nodes
            .iter()
            .find(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        if node.kind != NodeKind::Symlink {
            return Err(VfsError::NotFile);
        }
        try_vec_from_bytes(node.data.as_slice())
    }

    fn chown_as(
        &mut self,
        path: &str,
        uid: u32,
        gid: u32,
        creds: Credentials,
    ) -> Result<(), VfsError> {
        let normalized = normalize_path(path)?;
        let path = normalized.as_str();
        if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2_mut() {
                if !creds.is_superuser() {
                    return Err(VfsError::PermissionDenied);
                }
                return fs.chown(path, uid, gid).map_err(map_ext2_error);
            }
        }
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
        if Self::use_root_ext2(resolved.as_str()) {
            if let Some(fs) = self.root_ext2_mut() {
                let meta = fs.metadata(resolved.as_str()).map_err(map_ext2_error)?;
                if !creds.is_superuser() && creds.euid != meta.metadata.owner {
                    return Err(VfsError::PermissionDenied);
                }
                return fs.chmod(resolved.as_str(), mode).map_err(map_ext2_error);
            }
        }
        let node = self
            .nodes
            .iter()
            .position(|node| node.path == resolved)
            .ok_or(VfsError::NotFound)?;
        let node = canonical_node_index(&self.nodes, node);
        let node = &mut self.nodes[node];
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
            let count = self
                .root_ext2()
                .ok_or(VfsError::NotFound)?
                .read_file_range(&path, offset, output)
                .map_err(map_ext2_error)?;
            if let Some(Some(OpenHandle::Ext2File { offset: cursor, .. })) =
                self.open_files.get_mut(fd)
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
                if let Some(count) = read_proc_virtual(&path, *offset, output) {
                    *offset += count;
                    return Ok(count);
                }
                let kind = self.nodes[*node].kind;
                if kind != NodeKind::File {
                    return match kind {
                        NodeKind::Device(DeviceKind::Null) => Ok(0),
                        NodeKind::Device(DeviceKind::Zero) => {
                            output.fill(0);
                            Ok(output.len())
                        }
                        NodeKind::Device(DeviceKind::Random | DeviceKind::URandom) => {
                            fill_random(output);
                            Ok(output.len())
                        }
                        NodeKind::Device(DeviceKind::Block(device)) => {
                            let count = block_device_read(device, *offset as u64, output)?;
                            *offset += count;
                            Ok(count)
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
                        NodeKind::Device(DeviceKind::Ptmx | DeviceKind::PtySlave(_)) => {
                            Err(VfsError::BadFd)
                        }
                        NodeKind::Device(DeviceKind::Console | DeviceKind::Serial) => Ok(0),
                        NodeKind::Device(DeviceKind::Framebuffer) => Ok(0),
                        NodeKind::Directory => Err(VfsError::NotFile),
                        NodeKind::Symlink => Err(VfsError::NotFile),
                        NodeKind::File => unreachable!(),
                    };
                }

                let data_node = canonical_node_index(&self.nodes, *node);
                let data = self.nodes[data_node].data.as_slice();
                if *offset >= data.len() {
                    return Ok(0);
                }
                let remaining = data.len() - *offset;
                let count = remaining.min(output.len());
                output[..count].copy_from_slice(&data[*offset..*offset + count]);
                *offset += count;
                Ok(count)
            }
            OpenHandle::PipeRead { pipe } => {
                let pipe = self.pipes.get_mut(*pipe).ok_or(VfsError::BadFd)?;
                let read = pipe.read(output)?;
                if read > 0 {
                    crate::process::wake_io_waiters();
                }
                Ok(read)
            }
            OpenHandle::PtyMaster { pty, rights } => {
                if !rights.read {
                    return Err(VfsError::BadFd);
                }
                let pty = self.ptys.get_mut(*pty).ok_or(VfsError::BadFd)?;
                let read = pty.read_master(output)?;
                if read > 0 {
                    crate::process::wake_io_waiters();
                }
                Ok(read)
            }
            OpenHandle::PtySlave { pty, rights } => {
                if !rights.read {
                    return Err(VfsError::BadFd);
                }
                let pty = self.ptys.get_mut(*pty).ok_or(VfsError::BadFd)?;
                let read = pty.read_slave(output)?;
                if read > 0 {
                    crate::process::wake_io_waiters();
                }
                Ok(read)
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
                resize_file_buffer(&mut data, offset)?;
            }
            let end = checked_file_end(offset, input.len())?;
            if end > data.len() {
                resize_file_buffer(&mut data, end)?;
            }
            data[offset..end].copy_from_slice(input);
            self.root_ext2_mut()
                .ok_or(VfsError::NotFound)?
                .write_file(&path, &data)
                .map_err(map_ext2_error)?;
            if let Some(Some(OpenHandle::Ext2File { offset: cursor, .. })) =
                self.open_files.get_mut(fd)
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
                let kind = self.nodes[*node].kind;
                if kind != NodeKind::File {
                    return match kind {
                        NodeKind::Device(DeviceKind::Null) => Ok(input.len()),
                        NodeKind::Device(DeviceKind::Console) => {
                            let text = str::from_utf8(input).map_err(|_| VfsError::Utf8)?;
                            crate::log::write_str(text);
                            Ok(input.len())
                        }
                        NodeKind::Device(DeviceKind::Serial) => {
                            let text = str::from_utf8(input).map_err(|_| VfsError::Utf8)?;
                            crate::log::serial_print(format_args!("{}", text));
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
                        NodeKind::Device(
                            DeviceKind::Zero
                            | DeviceKind::Random
                            | DeviceKind::URandom
                            | DeviceKind::Block(_)
                            | DeviceKind::Keyboard,
                        ) => {
                            if let NodeKind::Device(DeviceKind::Block(device)) = kind {
                                let count = block_device_write(device, *offset as u64, input)?;
                                *offset += count;
                                Ok(count)
                            } else {
                                Ok(input.len())
                            }
                        }
                        NodeKind::Device(DeviceKind::Ptmx | DeviceKind::PtySlave(_)) => {
                            Err(VfsError::BadFd)
                        }
                        NodeKind::Directory => Err(VfsError::NotFile),
                        NodeKind::Symlink => Err(VfsError::NotFile),
                        NodeKind::File => unreachable!(),
                    };
                }

                let data_node = canonical_node_index(&self.nodes, *node);
                let node = &mut self.nodes[data_node];
                if *offset > node.data.len() {
                    node.data.resize(*offset)?;
                }
                let end = checked_file_end(*offset, input.len())?;
                if end > node.data.len() {
                    node.data.resize(end)?;
                }
                node.data.as_mut_slice()?[*offset..end].copy_from_slice(input);
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
            OpenHandle::PtyMaster { pty, rights } => {
                if !rights.write {
                    return Err(VfsError::BadFd);
                }
                let pty = self.ptys.get_mut(*pty).ok_or(VfsError::BadFd)?;
                let written = pty.write_master(input)?;
                if written > 0 {
                    crate::process::wake_io_waiters();
                }
                Ok(written)
            }
            OpenHandle::PtySlave { pty, rights } => {
                if !rights.write {
                    return Err(VfsError::BadFd);
                }
                let pty = self.ptys.get_mut(*pty).ok_or(VfsError::BadFd)?;
                let written = pty.write_slave(input)?;
                if written > 0 {
                    crate::process::wake_io_waiters();
                }
                Ok(written)
            }
            OpenHandle::Ext2File { .. } | OpenHandle::Ext2Dir { .. } => Err(VfsError::BadFd),
            OpenHandle::PipeRead { .. } => Err(VfsError::BadFd),
        }
    }

    fn truncate_fd(&mut self, fd: usize, len: usize) -> Result<(), VfsError> {
        if let Some((path, rights)) = self
            .open_files
            .get(fd)
            .and_then(|slot| slot.as_ref())
            .and_then(|handle| match handle {
                OpenHandle::Ext2File { path, rights, .. } => Some((path.clone(), *rights)),
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
            resize_file_buffer(&mut data, len)?;
            self.root_ext2_mut()
                .ok_or(VfsError::NotFound)?
                .write_file(&path, &data)
                .map_err(map_ext2_error)?;
            return Ok(());
        }

        let Some(handle) = self.open_files.get(fd).and_then(Option::as_ref) else {
            return Err(VfsError::BadFd);
        };
        let OpenHandle::Node { node, rights, .. } = handle else {
            return Err(VfsError::BadFd);
        };
        if !rights.write {
            return Err(VfsError::BadFd);
        }
        let data_node = canonical_node_index(&self.nodes, *node);
        if self.nodes[data_node].kind != NodeKind::File {
            return Err(VfsError::NotFile);
        }
        self.nodes[data_node].data.resize(len)?;
        self.nodes[data_node].timestamps.modified_at = crate::time::filesystem_timestamp();
        Ok(())
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
            OpenHandle::PtyMaster { pty, .. } => {
                self.ptys.get_mut(*pty).ok_or(VfsError::BadFd)?.add_master()
            }
            OpenHandle::PtySlave { pty, .. } => {
                self.ptys.get_mut(*pty).ok_or(VfsError::BadFd)?.add_slave()
            }
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
            OpenHandle::PtyMaster { pty, .. } => self
                .ptys
                .get_mut(*pty)
                .ok_or(VfsError::BadFd)?
                .close_master(),
            OpenHandle::PtySlave { pty, .. } => self
                .ptys
                .get_mut(*pty)
                .ok_or(VfsError::BadFd)?
                .close_slave(),
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
            let new_offset = seek_target(cursor, size, offset, whence)?;
            if let Some(Some(OpenHandle::Ext2File { offset: cursor, .. })) =
                self.open_files.get_mut(fd)
            {
                *cursor = new_offset;
            }
            return Ok(new_offset);
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
            let new_offset = seek_target(cursor, size, offset, whence)?;
            if let Some(Some(OpenHandle::Ext2Dir { offset: cursor, .. })) =
                self.open_files.get_mut(fd)
            {
                *cursor = new_offset;
            }
            return Ok(new_offset);
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
                let size = match self.nodes[*node].kind {
                    NodeKind::Device(DeviceKind::Block(device)) => {
                        block_device_len(device) as usize
                    }
                    _ => {
                        if let Some(text) = format_proc_virtual(&self.nodes[*node].path) {
                            text.len()
                        } else {
                            let data_node = canonical_node_index(&self.nodes, *node);
                            self.nodes[data_node].data.len()
                        }
                    }
                };
                (size, cursor)
            }
            OpenHandle::Ext2File { .. } | OpenHandle::Ext2Dir { .. } => {
                return Err(VfsError::BadFd);
            }
            OpenHandle::PipeRead { .. }
            | OpenHandle::PipeWrite { .. }
            | OpenHandle::PtyMaster { .. }
            | OpenHandle::PtySlave { .. } => {
                return Err(VfsError::BadFd);
            }
        };
        let new_offset = seek_target(*cursor, size, offset, whence)?;
        *cursor = new_offset;
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
                let visible_node = &self.nodes[*node];
                let data_node = canonical_node_index(&self.nodes, *node);
                let node = &self.nodes[data_node];
                let size = format_proc_virtual(&visible_node.path)
                    .map(|text| text.len() as u64)
                    .unwrap_or_else(|| match node.kind {
                        NodeKind::Device(DeviceKind::Block(device)) => block_device_len(device),
                        _ => node.data.len() as u64,
                    });
                Ok(Stat {
                    kind: stat_kind_from_node(node.kind),
                    owner: node.metadata.owner,
                    group: node.metadata.group,
                    mode: node.metadata.mode.0,
                    size,
                    nlink: self.node_link_count(data_node),
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
                    kind: stat_kind_from_ext2(meta.kind),
                    owner: meta.metadata.owner,
                    group: meta.metadata.group,
                    mode: meta.metadata.mode.0,
                    size: meta.size,
                    nlink: u64::from(meta.links),
                    mtime: meta.mtime,
                })
            }
            OpenHandle::Ext2Dir { path, .. } => {
                let meta = self
                    .root_ext2()
                    .ok_or(VfsError::NotFound)?
                    .metadata(path)
                    .map_err(map_ext2_error)?;
                Ok(Stat {
                    kind: stat_kind_from_ext2(meta.kind),
                    owner: meta.metadata.owner,
                    group: meta.metadata.group,
                    mode: meta.metadata.mode.0,
                    size: meta.size,
                    nlink: u64::from(meta.links),
                    mtime: meta.mtime,
                })
            }
            OpenHandle::PtyMaster { .. } | OpenHandle::PtySlave { .. } => Ok(Stat {
                kind: StatKind::CharDevice,
                owner: 0,
                group: 0,
                mode: 0o666,
                size: 0,
                nlink: 1,
                mtime: crate::time::filesystem_timestamp(),
            }),
            OpenHandle::PipeRead { .. } | OpenHandle::PipeWrite { .. } => Ok(Stat {
                kind: StatKind::Fifo,
                owner: 0,
                group: 0,
                mode: 0o600,
                size: 0,
                nlink: 1,
                mtime: crate::time::filesystem_timestamp(),
            }),
        }
    }

    fn fd_path(&self, fd: usize) -> Result<String, VfsError> {
        let handle = self
            .open_files
            .get(fd)
            .and_then(|h| h.as_ref())
            .ok_or(VfsError::BadFd)?;
        match handle {
            OpenHandle::Node { node, .. } => self
                .nodes
                .get(*node)
                .map(|node| node.path.clone())
                .ok_or(VfsError::BadFd),
            OpenHandle::Ext2File { path, .. } | OpenHandle::Ext2Dir { path, .. } => {
                Ok(path.clone())
            }
            OpenHandle::PtyMaster { .. }
            | OpenHandle::PtySlave { .. }
            | OpenHandle::PipeRead { .. }
            | OpenHandle::PipeWrite { .. } => Err(VfsError::BadFd),
        }
    }

    fn poll(&self, fd: usize) -> Result<PollReady, VfsError> {
        let handle = self
            .open_files
            .get(fd)
            .and_then(|h| h.as_ref())
            .ok_or(VfsError::BadFd)?;
        match handle {
            OpenHandle::Node { node, rights, .. } => {
                let node = self.nodes.get(*node).ok_or(VfsError::BadFd)?;
                let ready = match node.kind {
                    NodeKind::File | NodeKind::Symlink => PollReady {
                        read: rights.read,
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Directory => PollReady {
                        read: rights.read,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Null) => PollReady {
                        read: true,
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Zero) => PollReady {
                        read: rights.read,
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Random | DeviceKind::URandom) => PollReady {
                        read: rights.read,
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Block(_)) => PollReady {
                        read: rights.read,
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Console | DeviceKind::Serial) => PollReady {
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Keyboard) => PollReady::default(),
                    NodeKind::Device(DeviceKind::Tty) => PollReady {
                        read: crate::tty::has_data(),
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Ptmx | DeviceKind::PtySlave(_)) => PollReady {
                        read: false,
                        write: rights.write,
                        ..PollReady::default()
                    },
                    NodeKind::Device(DeviceKind::Framebuffer) => PollReady {
                        write: rights.write,
                        ..PollReady::default()
                    },
                };
                Ok(ready)
            }
            OpenHandle::Ext2File { rights, .. } => Ok(PollReady {
                read: rights.read,
                write: rights.write,
                ..PollReady::default()
            }),
            OpenHandle::Ext2Dir { .. } => Ok(PollReady {
                read: true,
                ..PollReady::default()
            }),
            OpenHandle::PipeRead { pipe } => {
                let pipe = self.pipes.get(*pipe).ok_or(VfsError::BadFd)?;
                Ok(PollReady {
                    read: pipe.poll_read(),
                    hangup: pipe.writers == 0 && pipe.buffer.is_empty(),
                    ..PollReady::default()
                })
            }
            OpenHandle::PipeWrite { pipe } => {
                let pipe = self.pipes.get(*pipe).ok_or(VfsError::BadFd)?;
                Ok(PollReady {
                    write: pipe.poll_write(),
                    error: pipe.readers == 0,
                    hangup: pipe.readers == 0,
                    ..PollReady::default()
                })
            }
            OpenHandle::PtyMaster { pty, rights } => {
                let pty = self.ptys.get(*pty).ok_or(VfsError::BadFd)?;
                Ok(PollReady {
                    read: rights.read && (!pty.slave_to_master.is_empty() || pty.slaves == 0),
                    write: rights.write
                        && pty.slaves > 0
                        && pty.master_to_slave.len() < pty.capacity,
                    error: pty.slaves == 0,
                    hangup: pty.slaves == 0 && pty.slave_to_master.is_empty(),
                })
            }
            OpenHandle::PtySlave { pty, rights } => {
                let pty = self.ptys.get(*pty).ok_or(VfsError::BadFd)?;
                Ok(PollReady {
                    read: rights.read && (!pty.master_to_slave.is_empty() || pty.masters == 0),
                    write: rights.write
                        && pty.masters > 0
                        && pty.slave_to_master.len() < pty.capacity,
                    error: pty.masters == 0,
                    hangup: pty.masters == 0 && pty.master_to_slave.is_empty(),
                })
            }
        }
    }

    fn fd_rights(&self, fd: usize) -> Result<FdRights, VfsError> {
        let handle = self
            .open_files
            .get(fd)
            .and_then(|h| h.as_ref())
            .ok_or(VfsError::BadFd)?;
        match handle {
            OpenHandle::Node { rights, .. }
            | OpenHandle::PtyMaster { rights, .. }
            | OpenHandle::PtySlave { rights, .. }
            | OpenHandle::Ext2File { rights, .. } => Ok(FdRights {
                read: rights.read,
                write: rights.write,
            }),
            OpenHandle::Ext2Dir { .. } | OpenHandle::PipeRead { .. } => Ok(FdRights {
                read: true,
                write: false,
            }),
            OpenHandle::PipeWrite { .. } => Ok(FdRights {
                read: false,
                write: true,
            }),
        }
    }

    fn is_tty_fd(&self, fd: usize) -> bool {
        let Some(Some(handle)) = self.open_files.get(fd) else {
            return false;
        };
        match handle {
            OpenHandle::Node { node, .. } => matches!(
                self.nodes.get(*node).map(|n| n.kind),
                Some(NodeKind::Device(DeviceKind::Tty))
            ),
            OpenHandle::PtyMaster { .. } | OpenHandle::PtySlave { .. } => true,
            _ => false,
        }
    }

    fn is_kernel_tty_fd(&self, fd: usize) -> bool {
        let Some(Some(OpenHandle::Node { node, .. })) = self.open_files.get(fd) else {
            return false;
        };
        matches!(
            self.nodes.get(*node).map(|n| n.kind),
            Some(NodeKind::Device(DeviceKind::Tty))
        )
    }

    fn pty_number(&self, fd: usize) -> Option<usize> {
        match self.open_files.get(fd).and_then(|slot| slot.as_ref())? {
            OpenHandle::PtyMaster { pty, .. } | OpenHandle::PtySlave { pty, .. } => Some(*pty),
            _ => None,
        }
    }

    fn block_device_size(&self, fd: usize) -> Option<u64> {
        match self.open_files.get(fd).and_then(|slot| slot.as_ref())? {
            OpenHandle::Node { node, .. } => match self.nodes.get(*node)?.kind {
                NodeKind::Device(DeviceKind::Block(device)) => Some(block_device_len(device)),
                _ => None,
            },
            _ => None,
        }
    }

    fn set_pty_locked(&mut self, fd: usize, locked: bool) -> Result<(), VfsError> {
        let pty = self.pty_number(fd).ok_or(VfsError::BadFd)?;
        let state = self.ptys.get_mut(pty).ok_or(VfsError::BadFd)?;
        state.locked = locked;
        Ok(())
    }

    fn pty_termios_bytes(&self, fd: usize) -> Result<[u8; crate::tty::TERMIOS_SIZE], VfsError> {
        let pty = self.pty_number(fd).ok_or(VfsError::BadFd)?;
        Ok(self.ptys.get(pty).ok_or(VfsError::BadFd)?.termios)
    }

    fn set_pty_termios_bytes(&mut self, fd: usize, bytes: &[u8]) -> Result<(), VfsError> {
        if bytes.len() < crate::tty::TERMIOS_SIZE {
            return Err(VfsError::BadFd);
        }
        let pty = self.pty_number(fd).ok_or(VfsError::BadFd)?;
        let state = self.ptys.get_mut(pty).ok_or(VfsError::BadFd)?;
        state
            .termios
            .copy_from_slice(&bytes[..crate::tty::TERMIOS_SIZE]);
        Ok(())
    }

    fn pty_winsize(&self, fd: usize) -> Result<[u8; 8], VfsError> {
        let pty = self.pty_number(fd).ok_or(VfsError::BadFd)?;
        Ok(self.ptys.get(pty).ok_or(VfsError::BadFd)?.winsize)
    }

    fn set_pty_winsize(&mut self, fd: usize, bytes: &[u8]) -> Result<(), VfsError> {
        if bytes.len() < 8 {
            return Err(VfsError::BadFd);
        }
        let pty = self.pty_number(fd).ok_or(VfsError::BadFd)?;
        let state = self.ptys.get_mut(pty).ok_or(VfsError::BadFd)?;
        state.winsize.copy_from_slice(&bytes[..8]);
        Ok(())
    }

    fn pty_foreground_pgrp(&self, fd: usize) -> Result<crate::process::Pid, VfsError> {
        let pty = self.pty_number(fd).ok_or(VfsError::BadFd)?;
        Ok(self.ptys.get(pty).ok_or(VfsError::BadFd)?.foreground_pgrp)
    }

    fn set_pty_foreground_pgrp(
        &mut self,
        fd: usize,
        pgrp: crate::process::Pid,
    ) -> Result<(), VfsError> {
        let pty = self.pty_number(fd).ok_or(VfsError::BadFd)?;
        let state = self.ptys.get_mut(pty).ok_or(VfsError::BadFd)?;
        state.foreground_pgrp = pgrp;
        Ok(())
    }

    fn stat(&self, path: &str) -> Result<Stat, VfsError> {
        self.stat_inner(path, true)
    }

    fn lstat(&self, path: &str) -> Result<Stat, VfsError> {
        self.stat_inner(path, false)
    }

    fn stat_inner(&self, path: &str, follow_symlink: bool) -> Result<Stat, VfsError> {
        let resolved;
        let normalized = normalize_path(path)?;
        let path = if follow_symlink {
            resolved = self.resolve_symlink_path(normalized.as_str())?;
            resolved.as_str()
        } else {
            normalized.as_str()
        };
        if let Some(kind) = proc_virtual_kind(path) {
            let size = format_proc_virtual(path)
                .map(|text| text.len() as u64)
                .unwrap_or(0);
            return Ok(Stat {
                kind: if kind == NodeKind::Directory {
                    StatKind::Directory
                } else {
                    StatKind::File
                },
                owner: 0,
                group: 0,
                mode: if kind == NodeKind::Directory {
                    0o555
                } else {
                    0o444
                },
                size,
                nlink: 1,
                mtime: crate::time::filesystem_timestamp(),
            });
        }
        if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2() {
                let meta = if follow_symlink {
                    fs.metadata(path)
                } else {
                    fs.lstat_metadata(path)
                };
                if let Ok(meta) = meta {
                    return Ok(Stat {
                        kind: stat_kind_from_ext2(meta.kind),
                        owner: meta.metadata.owner,
                        group: meta.metadata.group,
                        mode: meta.metadata.mode.0,
                        size: meta.size,
                        nlink: u64::from(meta.links),
                        mtime: meta.mtime,
                    });
                }
            }
        }
        let node_index = self
            .nodes
            .iter()
            .position(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        let data_node = canonical_node_index(&self.nodes, node_index);
        let node = &self.nodes[data_node];
        Ok(Stat {
            kind: stat_kind_from_node(node.kind),
            owner: node.metadata.owner,
            group: node.metadata.group,
            mode: node.metadata.mode.0,
            size: match node.kind {
                NodeKind::Device(DeviceKind::Block(device)) => block_device_len(device),
                _ => node.data.len() as u64,
            },
            nlink: self.node_link_count(data_node),
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
            .position(|node| node.path == path)
            .ok_or(VfsError::NotFound)?;
        let node = &self.nodes[canonical_node_index(&self.nodes, node)];
        Ok(node.metadata.can_access(creds, access))
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let resolved = self.resolve_symlink_path(path).ok()?;
        let path = resolved.as_str();
        if let Some(text) = format_proc_virtual(path) {
            return Some(Vec::from(text.as_bytes()));
        }
        if Self::use_root_ext2(path) {
            if let Some(fs) = self.root_ext2() {
                if let Ok(data) = fs.read_file(path) {
                    return Some(data);
                }
            }
        }
        let node = self.nodes.iter().position(|node| node.path == path)?;
        let data_node = canonical_node_index(&self.nodes, node);
        if self.nodes[data_node].kind == NodeKind::File {
            try_vec_from_bytes(self.nodes[data_node].data.as_slice()).ok()
        } else {
            None
        }
    }

    fn node_link_count(&self, target: usize) -> u64 {
        let target = canonical_node_index(&self.nodes, target);
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.path.is_empty() && node.kind == NodeKind::File)
            .filter(|(index, _)| canonical_node_index(&self.nodes, *index) == target)
            .count()
            .max(1) as u64
    }

    fn resolve_symlink_path(&self, path: &str) -> Result<String, VfsError> {
        let mut current = normalize_path(path)?;
        for _ in 0..8 {
            let Some(node) = self.nodes.iter().find(|node| node.path == current) else {
                return Ok(current);
            };
            if node.kind != NodeKind::Symlink {
                return Ok(current);
            }
            let target = str::from_utf8(node.data.as_slice()).map_err(|_| VfsError::Utf8)?;
            let next = if target.starts_with('/') {
                normalize_path(target)?
            } else {
                let parent = parent_path(&current)?;
                join_path(&parent, target)?
            };
            current = next;
        }
        Err(VfsError::NotFound)
    }

    fn timestamps(&self, path: &str) -> Option<FileTimestamps> {
        let path = normalize_path(path).ok()?;
        if Self::use_root_ext2(path.as_str()) {
            let meta = self.root_ext2()?.metadata(path.as_str()).ok()?;
            return Some(FileTimestamps {
                created_at: meta.mtime,
                modified_at: meta.mtime,
            });
        }
        let node = self
            .nodes
            .iter()
            .position(|node| node.path == path.as_str())?;
        Some(self.nodes[canonical_node_index(&self.nodes, node)].timestamps)
    }

    fn set_mtime_as(&mut self, path: &str, mtime: u64, creds: Credentials) -> Result<(), VfsError> {
        let resolved = self.resolve_symlink_path(path)?;
        if Self::use_root_ext2(resolved.as_str()) {
            if let Some(fs) = self.root_ext2_mut() {
                let meta = fs.metadata(resolved.as_str()).map_err(map_ext2_error)?;
                if !creds.is_superuser() && creds.euid != meta.metadata.owner {
                    return Err(VfsError::PermissionDenied);
                }
                return fs
                    .set_mtime(resolved.as_str(), mtime)
                    .map_err(map_ext2_error);
            }
        }
        let node = self
            .nodes
            .iter()
            .position(|node| node.path == resolved)
            .ok_or(VfsError::NotFound)?;
        let node = canonical_node_index(&self.nodes, node);
        let node = &mut self.nodes[node];
        if !creds.is_superuser() && creds.euid != node.metadata.owner {
            return Err(VfsError::PermissionDenied);
        }
        node.timestamps.modified_at = mtime;
        Ok(())
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
                Ok((self.node_directory_entries(&node.path)?, *offset))
            }
            OpenHandle::Ext2Dir { path, offset } => {
                Ok((self.ext2_directory_entries(path)?, *offset))
            }
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

    fn node_directory_entries(&self, path: &str) -> Result<Vec<DirectoryEntry>, VfsError> {
        let mut entries = Vec::new();
        for child in &self.nodes {
            if child.path.is_empty() || child.path == path {
                continue;
            }
            if parent_path(&child.path)? != path {
                continue;
            }
            push_entry(&mut entries, file_name(&child.path)?, child.kind)?;
        }
        if path == "/proc" {
            push_unique_entry(&mut entries, "meminfo", NodeKind::File)?;
            push_unique_entry(&mut entries, "mounts", NodeKind::File)?;
            push_unique_entry(&mut entries, "netinfo", NodeKind::File)?;
            push_unique_entry(&mut entries, "self", NodeKind::Directory)?;
            push_unique_entry(&mut entries, "stat", NodeKind::File)?;
            push_unique_entry(&mut entries, "uptime", NodeKind::File)?;
            let mut cursor = 0;
            while let Some(pid) = crate::process::next_process_pid_after(cursor) {
                cursor = pid;
                push_unique_entry_owned(&mut entries, try_u64_string(pid)?, NodeKind::Directory)?;
            }
        } else if proc_existing_process_dir_pid(path).is_some() {
            push_unique_entry(&mut entries, "status", NodeKind::File)?;
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn ext2_directory_entries(&self, path: &str) -> Result<Vec<DirectoryEntry>, VfsError> {
        let fs = self.root_ext2().ok_or(VfsError::NotFound)?;
        let mut entries = Vec::new();
        for name in fs.list_dir(path).map_err(map_ext2_error)? {
            let full = join_path(path, &name)?;
            let kind = match fs.lstat_metadata(&full).map_err(map_ext2_error)?.kind {
                ext2::Ext2NodeKind::File => NodeKind::File,
                ext2::Ext2NodeKind::Directory => NodeKind::Directory,
                ext2::Ext2NodeKind::Symlink => NodeKind::Symlink,
            };
            push_entry(&mut entries, name, kind)?;
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn list_paths(&self, prefix: &str) -> Vec<String> {
        let prefix = normalize_path(prefix).unwrap_or_else(|_| String::from("/"));
        self.resolve_mount_list_paths(&prefix)
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
    let root_device = crate::boot_config::value("root").unwrap_or("/dev/vda");
    if mount(root_device, "/", "ext2").is_ok() {
        crate::println!(
            "Ext2 mounted from {} as / with devfs, procfs, and tmpfs overlays.",
            root_device
        );
    } else if root_device != "/dev/vda" && mount("/dev/vda", "/", "ext2").is_ok() {
        crate::println!("Ext2 mounted from /dev/vda as / with devfs, procfs, and tmpfs overlays.");
    }
}

pub fn refresh_block_devices() {
    with_vfs(|vfs| vfs.refresh_block_devices());
}

pub fn block_device_size(fd: usize) -> Option<u64> {
    let guard = VFS.lock();
    guard.as_ref().and_then(|vfs| vfs.block_device_size(fd))
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

pub fn create_file_with_mode_as(
    path: &str,
    creds: Credentials,
    mode: u16,
) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.create_file_with_mode_as(path, creds, mode))
}

pub fn duplicate_fd(fd: usize) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.duplicate_fd(fd))
}

pub fn read(fd: usize, output: &mut [u8]) -> Result<usize, VfsError> {
    with_vfs(|vfs| vfs.read(fd, output))
}

pub fn write(fd: usize, input: &[u8]) -> Result<usize, VfsError> {
    let result = with_vfs(|vfs| vfs.write(fd, input));
    deliver_pty_signals();
    result
}

pub fn truncate_fd(fd: usize, len: usize) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.truncate_fd(fd, len))
}

pub fn close(fd: usize) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.close(fd))
}

#[derive(Clone, Copy, Debug)]
pub struct Stat {
    pub kind: StatKind,
    pub owner: u32,
    pub group: u32,
    pub mode: u16,
    pub size: u64,
    pub nlink: u64,
    pub mtime: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatKind {
    File,
    Directory,
    Symlink,
    CharDevice,
    Fifo,
}

fn stat_kind_from_node(kind: NodeKind) -> StatKind {
    match kind {
        NodeKind::File => StatKind::File,
        NodeKind::Directory => StatKind::Directory,
        NodeKind::Symlink => StatKind::Symlink,
        NodeKind::Device(_) => StatKind::CharDevice,
    }
}

fn stat_kind_from_ext2(kind: ext2::Ext2NodeKind) -> StatKind {
    match kind {
        ext2::Ext2NodeKind::File => StatKind::File,
        ext2::Ext2NodeKind::Directory => StatKind::Directory,
        ext2::Ext2NodeKind::Symlink => StatKind::Symlink,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FsStat {
    pub fs_type: u64,
    pub block_size: u64,
    pub blocks: u64,
    pub blocks_free: u64,
    pub blocks_available: u64,
    pub files: u64,
    pub files_free: u64,
    pub name_max: u64,
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

pub fn statfs(path: &str) -> Result<FsStat, VfsError> {
    with_vfs(|vfs| vfs.statfs(path))
}

pub fn fd_path(fd: usize) -> Result<String, VfsError> {
    with_vfs(|vfs| vfs.fd_path(fd))
}

pub fn poll(fd: usize) -> Result<PollReady, VfsError> {
    let guard = VFS.lock();
    let vfs = guard.as_ref().expect("VFS used before initialization");
    vfs.poll(fd)
}

pub fn fd_rights(fd: usize) -> Result<FdRights, VfsError> {
    let guard = VFS.lock();
    let vfs = guard.as_ref().expect("VFS used before initialization");
    vfs.fd_rights(fd)
}

pub fn is_tty_fd(fd: usize) -> bool {
    let guard = VFS.lock();
    guard.as_ref().map(|vfs| vfs.is_tty_fd(fd)).unwrap_or(false)
}

pub fn is_kernel_tty_fd(fd: usize) -> bool {
    let guard = VFS.lock();
    guard
        .as_ref()
        .map(|vfs| vfs.is_kernel_tty_fd(fd))
        .unwrap_or(false)
}

pub fn pty_number(fd: usize) -> Option<usize> {
    let guard = VFS.lock();
    guard.as_ref().and_then(|vfs| vfs.pty_number(fd))
}

pub fn set_pty_locked(fd: usize, locked: bool) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.set_pty_locked(fd, locked))
}

pub fn pty_termios_bytes(fd: usize) -> Result<[u8; crate::tty::TERMIOS_SIZE], VfsError> {
    let guard = VFS.lock();
    guard
        .as_ref()
        .expect("VFS used before initialization")
        .pty_termios_bytes(fd)
}

pub fn set_pty_termios_bytes(fd: usize, bytes: &[u8]) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.set_pty_termios_bytes(fd, bytes))
}

pub fn pty_winsize(fd: usize) -> Result<[u8; 8], VfsError> {
    let guard = VFS.lock();
    guard
        .as_ref()
        .expect("VFS used before initialization")
        .pty_winsize(fd)
}

pub fn set_pty_winsize(fd: usize, bytes: &[u8]) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.set_pty_winsize(fd, bytes))
}

pub fn pty_foreground_pgrp(fd: usize) -> Result<crate::process::Pid, VfsError> {
    let guard = VFS.lock();
    guard
        .as_ref()
        .expect("VFS used before initialization")
        .pty_foreground_pgrp(fd)
}

pub fn set_pty_foreground_pgrp(fd: usize, pgrp: crate::process::Pid) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.set_pty_foreground_pgrp(fd, pgrp))
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

pub fn mkdir_with_mode_as(path: &str, creds: Credentials, mode: u16) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.mkdir_with_mode_as(path, creds, mode))
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

pub fn link_as(old_path: &str, new_path: &str, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.link_as(old_path, new_path, creds))
}

pub fn readlink(path: &str) -> Result<Vec<u8>, VfsError> {
    with_vfs(|vfs| vfs.readlink(path))
}

pub fn chown_as(path: &str, uid: u32, gid: u32, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.chown_as(path, uid, gid, creds))
}

pub fn set_mtime_as(path: &str, mtime: u64, creds: Credentials) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.set_mtime_as(path, mtime, creds))
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
        ext2::Ext2Error::TooManyLinks => VfsError::TooManyLinks,
        ext2::Ext2Error::NoSpace | ext2::Ext2Error::DirectoryFull => VfsError::NoSpace,
        ext2::Ext2Error::OutOfMemory => VfsError::OutOfMemory,
        ext2::Ext2Error::InvalidSuperblock
        | ext2::Ext2Error::IoError
        | ext2::Ext2Error::Unsupported => VfsError::BadFd,
    }
}

fn canonical_node_index(nodes: &[Node], index: usize) -> usize {
    let mut current = index;
    for _ in 0..8 {
        let Some(next) = nodes.get(current).and_then(|node| node.link_target) else {
            break;
        };
        current = next;
    }
    current
}

fn parent_path(path: &str) -> Result<String, VfsError> {
    let normalized = normalize_path(path)?;
    let path = normalized.as_str();
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => try_string_from("/"),
        Some(index) => try_string_from(&trimmed[..index]),
    }
}

fn file_name(path: &str) -> Result<String, VfsError> {
    let normalized = normalize_path(path)?;
    let path = normalized.as_str();
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    try_string_from(name)
}

fn join_path(parent: &str, name: &str) -> Result<String, VfsError> {
    if name.starts_with('/') {
        return normalize_path(name);
    }
    let parent = parent.trim_end_matches('/');
    let len = if parent.is_empty() || parent == "/" {
        name.len().checked_add(1).ok_or(VfsError::OutOfMemory)?
    } else {
        parent
            .len()
            .checked_add(1)
            .and_then(|len| len.checked_add(name.len()))
            .ok_or(VfsError::OutOfMemory)?
    };
    let mut joined = String::new();
    joined
        .try_reserve_exact(len)
        .map_err(|_| VfsError::OutOfMemory)?;
    if parent.is_empty() || parent == "/" {
        joined.push('/');
    } else {
        joined.push_str(parent);
        joined.push('/');
    }
    joined.push_str(name);
    normalize_path(&joined)
}

fn child_path_prefix(path: &str) -> Result<String, VfsError> {
    let trimmed = path.trim_end_matches('/');
    let len = trimmed.len().checked_add(1).ok_or(VfsError::OutOfMemory)?;
    let mut prefix = String::new();
    prefix
        .try_reserve_exact(len)
        .map_err(|_| VfsError::OutOfMemory)?;
    prefix.push_str(trimmed);
    prefix.push('/');
    Ok(prefix)
}

fn replace_path_prefix(path: &str, old_prefix: &str, new_prefix: &str) -> Result<String, VfsError> {
    let suffix = path.get(old_prefix.len()..).ok_or(VfsError::NotFound)?;
    let len = new_prefix
        .len()
        .checked_add(suffix.len())
        .ok_or(VfsError::OutOfMemory)?;
    let mut rewritten = String::new();
    rewritten
        .try_reserve_exact(len)
        .map_err(|_| VfsError::OutOfMemory)?;
    rewritten.push_str(new_prefix);
    rewritten.push_str(suffix);
    Ok(rewritten)
}

fn try_pty_slave_path(pty: usize) -> Result<String, VfsError> {
    const PREFIX: &str = "/dev/pts/";
    let suffix = try_u64_string(pty as u64)?;
    let len = PREFIX
        .len()
        .checked_add(suffix.len())
        .ok_or(VfsError::OutOfMemory)?;
    let mut path = String::new();
    path.try_reserve_exact(len)
        .map_err(|_| VfsError::OutOfMemory)?;
    path.push_str(PREFIX);
    path.push_str(&suffix);
    Ok(path)
}

fn fill_random(output: &mut [u8]) {
    crate::entropy::fill_random(output);
}

fn block_device_len(device: BlockDevice) -> u64 {
    let sectors = if device.sector_count == 0 {
        drivers::virtio_blk::sector_count().saturating_sub(device.start_sector)
    } else {
        device.sector_count
    };
    sectors.saturating_mul(512)
}

fn block_device_read(
    device: BlockDevice,
    offset: u64,
    output: &mut [u8],
) -> Result<usize, VfsError> {
    let len = block_device_len(device);
    if offset >= len {
        return Ok(0);
    }
    let count = output.len().min((len - offset) as usize);
    let absolute = device
        .start_sector
        .saturating_mul(512)
        .saturating_add(offset);
    drivers::virtio_blk::read_bytes(absolute, &mut output[..count]).map_err(|_| VfsError::BadFd)
}

fn block_device_write(device: BlockDevice, offset: u64, input: &[u8]) -> Result<usize, VfsError> {
    let len = block_device_len(device);
    if offset >= len {
        return Ok(0);
    }
    let count = input.len().min((len - offset) as usize);
    let absolute = device
        .start_sector
        .saturating_mul(512)
        .saturating_add(offset);
    drivers::virtio_blk::write_bytes(absolute, &input[..count]).map_err(|_| VfsError::BadFd)
}

fn normalize_path(path: &str) -> Result<String, VfsError> {
    if !path.starts_with('/') {
        return Err(VfsError::NotFound);
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => {
                parts
                    .try_reserve_exact(1)
                    .map_err(|_| VfsError::OutOfMemory)?;
                parts.push(part);
            }
        }
    }
    if parts.is_empty() {
        return try_string_from("/");
    }
    let mut len = 0usize;
    for part in &parts {
        len = len
            .checked_add(1)
            .and_then(|len| len.checked_add(part.len()))
            .ok_or(VfsError::OutOfMemory)?;
    }
    let mut normalized = String::new();
    normalized
        .try_reserve_exact(len)
        .map_err(|_| VfsError::OutOfMemory)?;
    for part in parts {
        normalized.push('/');
        normalized.push_str(part);
    }
    Ok(normalized)
}

fn try_string_from(value: &str) -> Result<String, VfsError> {
    let mut out = String::new();
    out.try_reserve_exact(value.len())
        .map_err(|_| VfsError::OutOfMemory)?;
    out.push_str(value);
    Ok(out)
}

fn try_vec_from_bytes(bytes: &[u8]) -> Result<Vec<u8>, VfsError> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len())
        .map_err(|_| VfsError::OutOfMemory)?;
    out.extend_from_slice(bytes);
    Ok(out)
}

fn proc_status_pid(path: &str) -> Option<u64> {
    let rest = path.strip_prefix("/proc/")?;
    let pid_text = rest.strip_suffix("/status")?;
    if pid_text == "self" {
        return crate::process::current_pid();
    }
    pid_text.parse().ok()
}

fn proc_process_dir_pid(path: &str) -> Option<u64> {
    let pid_text = path.strip_prefix("/proc/")?;
    if pid_text == "self" {
        return crate::process::current_pid();
    }
    pid_text.parse().ok()
}

fn proc_existing_process_dir_pid(path: &str) -> Option<u64> {
    let pid = proc_process_dir_pid(path)?;
    crate::process::get_process_info(pid).map(|_| pid)
}

fn proc_virtual_kind(path: &str) -> Option<NodeKind> {
    if matches!(
        path,
        "/proc/cmdline"
            | "/proc/meminfo"
            | "/proc/mounts"
            | "/proc/stat"
            | "/proc/uptime"
            | "/proc/netinfo"
    ) || proc_status_pid(path).is_some()
    {
        return Some(NodeKind::File);
    }
    if proc_process_dir_pid(path).is_some() {
        return Some(NodeKind::Directory);
    }
    None
}

fn format_proc_virtual(path: &str) -> Option<String> {
    match path {
        "/proc/meminfo" => {
            let stats = crate::memory::stats();
            Some(format!(
                "MemTotal: {} kB\nMemFree: {} kB\nHeapUsed: {} bytes\nHeapFree: {} bytes\n",
                stats.frames.total_frames * 4,
                stats.frames.free_frames * 4,
                stats.heap.used_bytes,
                stats.heap.free_bytes
            ))
        }
        "/proc/mounts" => Some(String::from(
            "ext2 / ext2 rw 0 0\n\
             devfs /dev devfs rw 0 0\n\
             procfs /proc procfs ro 0 0\n\
             tmpfs /tmp tmpfs rw 0 0\n",
        )),
        "/proc/cmdline" => Some(format!(
            "{}\n",
            crate::boot_config::command_line().unwrap_or("")
        )),
        "/proc/stat" => {
            let stats = crate::process::stats();
            Some(format!(
                "cpu  0 0 0 {}\nprocesses {}\nprocs_running {}\nfd_count {}\n",
                crate::time::monotonic_ticks(),
                stats.process_count,
                stats.process_count,
                stats.fd_count
            ))
        }
        "/proc/uptime" => {
            let millis = crate::time::uptime_millis();
            Some(format!(
                "{}.{:02} 0.00\n",
                millis / 1000,
                (millis % 1000) / 10
            ))
        }
        "/proc/netinfo" => {
            let ip = crate::net::local_ip();
            let mac = crate::net::local_mac();
            let stats = crate::net::stats();
            let dhcp = crate::net::dhcp_status();

            let subnet = crate::net::subnet_mask()
                .map(|m| format!("{}.{}.{}.{}", m.0[0], m.0[1], m.0[2], m.0[3]))
                .unwrap_or_else(|| String::from("none"));

            let gateway = crate::net::gateway()
                .map(|g| format!("{}.{}.{}.{}", g.0[0], g.0[1], g.0[2], g.0[3]))
                .unwrap_or_else(|| String::from("none"));

            let dns = crate::net::dns_server()
                .map(|d| format!("{}.{}.{}.{}", d.0[0], d.0[1], d.0[2], d.0[3]))
                .unwrap_or_else(|| String::from("none"));

            Some(format!(
                "Interface: eth0\n\
                 MAC Address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n\
                 IP Address: {}.{}.{}.{}\n\
                 Subnet Mask: {}\n\
                 Default Gateway: {}\n\
                 DNS Server: {}\n\
                 DHCP Status: {}\n\
                 Rx Frames: {}\n\
                 Tx Frames: {}\n\
                 Arp Cache Entries: {}\n",
                mac.0[0],
                mac.0[1],
                mac.0[2],
                mac.0[3],
                mac.0[4],
                mac.0[5],
                ip.0[0],
                ip.0[1],
                ip.0[2],
                ip.0[3],
                subnet,
                gateway,
                dns,
                dhcp,
                stats.rx_frames,
                stats.tx_frames,
                stats.arp_entries
            ))
        }
        _ => proc_status_pid(path).map(format_proc_status),
    }
}

fn format_proc_status(pid: u64) -> alloc::string::String {
    let Some((name, state, parent, exit_status)) = crate::process::get_process_info(pid) else {
        return alloc::format!("pid: {}\nstate: not found\n", pid);
    };
    let state_text = match state {
        crate::process::ProcessState::Ready => "ready",
        crate::process::ProcessState::Running => "running",
        crate::process::ProcessState::Blocked(_) => "blocked",
        crate::process::ProcessState::Stopped(_) => "stopped",
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

fn read_proc_virtual(path: &str, offset: usize, output: &mut [u8]) -> Option<usize> {
    let text = format_proc_virtual(path)?;
    read_proc_text(&text, offset, output).ok()
}

fn read_proc_text(text: &String, offset: usize, output: &mut [u8]) -> Result<usize, VfsError> {
    let bytes = text.as_bytes();
    let remaining = bytes.len().saturating_sub(offset);
    let count = remaining.min(output.len());
    output[..count].copy_from_slice(&bytes[offset..offset + count]);
    Ok(count)
}

fn push_entry(
    entries: &mut Vec<DirectoryEntry>,
    name: String,
    kind: NodeKind,
) -> Result<(), VfsError> {
    entries
        .try_reserve_exact(1)
        .map_err(|_| VfsError::OutOfMemory)?;
    entries.push(DirectoryEntry { name, kind });
    Ok(())
}

fn push_unique_entry(
    entries: &mut Vec<DirectoryEntry>,
    name: &str,
    kind: NodeKind,
) -> Result<(), VfsError> {
    if entries
        .iter()
        .any(|entry| entry.name.as_bytes() == name.as_bytes())
    {
        return Ok(());
    }
    push_entry(entries, try_string_from(name)?, kind)
}

fn push_unique_entry_owned(
    entries: &mut Vec<DirectoryEntry>,
    name: String,
    kind: NodeKind,
) -> Result<(), VfsError> {
    if entries
        .iter()
        .any(|entry| entry.name.as_bytes() == name.as_bytes())
    {
        return Ok(());
    }
    push_entry(entries, name, kind)
}

fn try_u64_string(value: u64) -> Result<String, VfsError> {
    let mut digits = [0u8; 20];
    let mut pos = digits.len();
    let mut remaining = value;
    loop {
        pos -= 1;
        digits[pos] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    let text = str::from_utf8(&digits[pos..]).map_err(|_| VfsError::Utf8)?;
    try_string_from(text)
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

    let master = open("/dev/ptmx").expect("open /dev/ptmx failed");
    let pty = pty_number(master).expect("pty number missing");
    set_pty_locked(master, false).expect("unlock pty failed");
    let slave_path = format!("/dev/pts/{}", pty);
    let slave = open(&slave_path).expect("open pty slave failed");
    let mut raw_termios = crate::tty::default_termios_bytes();
    let mut lflag = u32::from_le_bytes([
        raw_termios[12],
        raw_termios[13],
        raw_termios[14],
        raw_termios[15],
    ]);
    lflag &= !(0x1 | 0x2 | 0x8 | 0x8000);
    raw_termios[12..16].copy_from_slice(&lflag.to_le_bytes());
    set_pty_termios_bytes(master, &raw_termios).expect("set pty raw termios failed");
    write(master, b"abc").expect("write pty master failed");
    let mut slave_bytes = [0; 3];
    read(slave, &mut slave_bytes).expect("read pty slave failed");
    write(slave, b"xyz").expect("write pty slave failed");
    let mut master_bytes = [0; 3];
    read(master, &mut master_bytes).expect("read pty master failed");
    close(slave).expect("close pty slave failed");
    close(master).expect("close pty master failed");
    if &slave_bytes != b"abc" || &master_bytes != b"xyz" {
        panic!("PTY self-test failed");
    }

    if read_file("/pkg/packages.txt").is_none() || read_file("/tmp/message.txt").is_none() {
        panic!("VFS path resolution self-test failed");
    }
    let root_device = crate::boot_config::value("root").unwrap_or("/dev/vda");
    let full_initrd_root =
        root_device == "/dev/vda" && !crate::boot_config::contains("ristux.mode=install");
    if full_initrd_root {
        if read_file("/bin/init").is_none()
            || read_file("/bin/rustc").is_none()
            || read_file("/bin/cargo").is_none()
            || read_file("/bin/rustdoc").is_none()
            || read_file("/usr/lib/rustlib/rust-1.96.0-manifest.toml").is_none()
            || read_file("/usr/lib/rustlib/x86_64-unknown-ristux/target.json").is_none()
            || read_file("/etc/os-release").is_none()
        {
            panic!("VFS full initrd path resolution self-test failed");
        }
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
