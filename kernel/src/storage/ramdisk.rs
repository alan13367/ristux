use alloc::{string::String, vec, vec::Vec};

use crate::sync::spinlock::SpinLock;

const RAMDISK_SIZE: usize = 16 * 1024;

static STORAGE: SpinLock<Option<RamDiskFs>> = SpinLock::new(None);

pub struct RamDisk {
    bytes: Vec<u8>,
}

impl RamDisk {
    fn new(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }
}

pub struct RamDiskFs {
    disk: RamDisk,
    files: Vec<DiskFile>,
}

struct DiskFile {
    path: String,
    data: Vec<u8>,
}

impl RamDiskFs {
    fn new() -> Self {
        Self {
            disk: RamDisk::new(RAMDISK_SIZE),
            files: Vec::new(),
        }
    }

    fn create_file(&mut self, path: &str, data: &[u8]) {
        if let Some(file) = self.files.iter_mut().find(|file| file.path == path) {
            file.data.clear();
            file.data.extend_from_slice(data);
        } else {
            self.files.push(DiskFile {
                path: String::from(path),
                data: Vec::from(data),
            });
        }
        self.flush_catalog();
    }

    fn read_file(&self, path: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.data.as_slice())
    }

    fn remount(&self) -> Self {
        let mut remounted = Self {
            disk: RamDisk {
                bytes: self.disk.bytes.clone(),
            },
            files: Vec::new(),
        };
        remounted.load_catalog();
        remounted
    }

    fn flush_catalog(&mut self) {
        self.disk.bytes.fill(0);
        self.disk.bytes[0..8].copy_from_slice(b"RDSK001\0");
        self.disk.bytes[8..12].copy_from_slice(&(self.files.len() as u32).to_le_bytes());
        let mut offset = 12;
        for file in &self.files {
            let path = file.path.as_bytes();
            let needed = 4 + path.len() + 4 + file.data.len();
            if offset + needed > self.disk.bytes.len() {
                break;
            }
            self.disk.bytes[offset..offset + 2].copy_from_slice(&(path.len() as u16).to_le_bytes());
            self.disk.bytes[offset + 2..offset + 4]
                .copy_from_slice(&(file.data.len() as u16).to_le_bytes());
            offset += 4;
            self.disk.bytes[offset..offset + path.len()].copy_from_slice(path);
            offset += path.len();
            self.disk.bytes[offset..offset + file.data.len()].copy_from_slice(&file.data);
            offset += file.data.len();
        }
    }

    fn load_catalog(&mut self) {
        if self.disk.bytes.get(0..8) != Some(b"RDSK001\0") {
            return;
        }

        let count = u32::from_le_bytes([
            self.disk.bytes[8],
            self.disk.bytes[9],
            self.disk.bytes[10],
            self.disk.bytes[11],
        ]) as usize;
        let mut offset = 12;
        for _ in 0..count {
            if offset + 4 > self.disk.bytes.len() {
                break;
            }
            let path_len =
                u16::from_le_bytes([self.disk.bytes[offset], self.disk.bytes[offset + 1]]) as usize;
            let data_len =
                u16::from_le_bytes([self.disk.bytes[offset + 2], self.disk.bytes[offset + 3]])
                    as usize;
            offset += 4;
            if offset + path_len + data_len > self.disk.bytes.len() {
                break;
            }
            let path = core::str::from_utf8(&self.disk.bytes[offset..offset + path_len])
                .unwrap_or("/lost+found");
            offset += path_len;
            let data = Vec::from(&self.disk.bytes[offset..offset + data_len]);
            offset += data_len;
            self.files.push(DiskFile {
                path: String::from(path),
                data,
            });
        }
    }

    fn stats(&self) -> StorageStats {
        StorageStats {
            bytes: self.disk.bytes.len(),
            files: self.files.len(),
        }
    }
}

pub fn init() {
    *STORAGE.lock() = Some(RamDiskFs::new());
    crate::fs::vfs::write_file("/disk", b"");
    crate::println!("RAM disk initialized: {} bytes.", RAMDISK_SIZE);
}

pub fn self_test() {
    let mut guard = STORAGE.lock();
    let fs = guard.as_mut().expect("ramdisk used before initialization");
    fs.create_file("/disk/persist.txt", b"survived remount\n");
    let remounted = fs.remount();
    let data = remounted
        .read_file("/disk/persist.txt")
        .expect("ramdisk remount lost file");
    if data != b"survived remount\n" {
        panic!("ramdisk persistence self-test read wrong data");
    }
    crate::fs::vfs::write_file("/disk/persist.txt", data);
    *fs = remounted;
    crate::println!("RAM disk persistence self-test passed.");
}

pub fn stats() -> StorageStats {
    STORAGE
        .lock()
        .as_ref()
        .map(RamDiskFs::stats)
        .unwrap_or(StorageStats { bytes: 0, files: 0 })
}

#[derive(Clone, Copy)]
pub struct StorageStats {
    pub bytes: usize,
    pub files: usize,
}
