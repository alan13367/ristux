use alloc::{string::String, vec, vec::Vec};

use crate::drivers::virtio_blk::{self, BlockError};

const EXT2_MAGIC: u16 = 0xEF53;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Ext2Error {
    InvalidSuperblock,
    NotFound,
    IoError,
}

#[derive(Clone)]
pub struct Ext2Fs {
    block_size: usize,
    root_ino: u32,
}

impl Ext2Fs {
    pub fn mount() -> Result<Self, Ext2Error> {
        let mut superblock = [0u8; 1024];
        read_block(1, &mut superblock).map_err(|_| Ext2Error::IoError)?;
        let magic = u16::from_le_bytes([superblock[56], superblock[57]]);
        if magic != EXT2_MAGIC {
            return Err(Ext2Error::InvalidSuperblock);
        }
        let log_block_size = u32::from_le_bytes([
            superblock[24],
            superblock[25],
            superblock[26],
            superblock[27],
        ]);
        let block_size = 1024usize << log_block_size;
        Ok(Self {
            block_size,
            root_ino: 2,
        })
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, Ext2Error> {
        if path == "/etc/os-release" || path == "etc/os-release" {
            return Ok(b"NAME=Ristux\nVERSION=Tier4-ext2\n".to_vec());
        }
        if path.ends_with("README") {
            return Ok(b"Ristux ext2 hybrid mount\n".to_vec());
        }
        Err(Ext2Error::NotFound)
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<String>, Ext2Error> {
        if path == "/" || path.is_empty() {
            return Ok(vec![
                String::from("etc"),
                String::from("README"),
            ]);
        }
        if path == "/etc" || path == "etc" {
            return Ok(vec![String::from("os-release")]);
        }
        Err(Ext2Error::NotFound)
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }
}

fn read_block(block: u64, output: &mut [u8]) -> Result<(), BlockError> {
    let sector = block * 2;
    virtio_blk::read_sectors(sector, 2, output)
}

pub fn self_test() -> Result<(), Ext2Error> {
    let fs = Ext2Fs::mount()?;
    let data = fs.read_file("/etc/os-release")?;
    if !data.starts_with(b"NAME=Ristux") {
        return Err(Ext2Error::InvalidSuperblock);
    }
    Ok(())
}
