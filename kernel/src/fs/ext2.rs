use alloc::{string::String, vec, vec::Vec};

use crate::drivers::virtio_blk;

const EXT2_MAGIC: u16 = 0xEF53;
const EXT2_ROOT_INO: u32 = 2;
const EXT2_N_BLOCKS: usize = 15;
const EXT2_S_IFMT: u16 = 0o170000;
const EXT2_S_IFREG: u16 = 0o100000;
const EXT2_S_IFDIR: u16 = 0o040000;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Ext2Error {
    InvalidSuperblock,
    NotFound,
    NotDirectory,
    NotFile,
    IoError,
    Unsupported,
}

#[derive(Clone)]
pub struct Ext2Fs {
    block_size: usize,
    blocks_count: u32,
    inodes_count: u32,
    blocks_per_group: u32,
    inodes_per_group: u32,
    inode_size: usize,
    groups: Vec<BlockGroup>,
    root_ino: u32,
}

#[derive(Clone, Copy)]
struct BlockGroup {
    block_bitmap: u32,
    inode_bitmap: u32,
    inode_table: u32,
}

#[derive(Clone)]
struct Inode {
    mode: u16,
    uid: u32,
    gid: u32,
    size: u64,
    links: u16,
    blocks: [u32; EXT2_N_BLOCKS],
}

#[derive(Clone)]
struct DirEntry {
    ino: u32,
    name: String,
    file_type: u8,
}

impl Ext2Fs {
    pub fn mount() -> Result<Self, Ext2Error> {
        let mut superblock = [0u8; 1024];
        read_block_sized(1, 1024, &mut superblock)?;
        let magic = le_u16(&superblock, 56);
        if magic != EXT2_MAGIC {
            crate::println!("Ext2 superblock magic read {:#x}.", magic);
            return Err(Ext2Error::InvalidSuperblock);
        }

        let inodes_count = le_u32(&superblock, 0);
        let blocks_count = le_u32(&superblock, 4);
        let log_block_size = le_u32(&superblock, 24);
        if log_block_size > 2 {
            return Err(Ext2Error::Unsupported);
        }
        let block_size = 1024usize << log_block_size;
        let blocks_per_group = le_u32(&superblock, 32);
        let inodes_per_group = le_u32(&superblock, 40);
        if blocks_per_group == 0 || inodes_per_group == 0 {
            crate::println!(
                "Ext2 superblock invalid groups: blocks/group {}, inodes/group {}.",
                blocks_per_group,
                inodes_per_group
            );
            return Err(Ext2Error::InvalidSuperblock);
        }
        let rev_level = le_u32(&superblock, 76);
        let inode_size = if rev_level >= 1 {
            le_u16(&superblock, 88).max(128) as usize
        } else {
            128
        };

        let group_count = blocks_count.div_ceil(blocks_per_group) as usize;
        let desc_table_block = if block_size == 1024 { 2 } else { 1 };
        let desc_bytes = group_count * 32;
        let desc_blocks = desc_bytes.div_ceil(block_size);
        let mut desc_data = Vec::new();
        for block in 0..desc_blocks {
            let mut bytes = vec![0u8; block_size];
            read_block_sized(
                desc_table_block as u64 + block as u64,
                block_size,
                &mut bytes,
            )?;
            desc_data.extend_from_slice(&bytes);
        }

        let mut groups = Vec::with_capacity(group_count);
        for group in 0..group_count {
            let offset = group * 32;
            groups.push(BlockGroup {
                block_bitmap: le_u32(&desc_data, offset),
                inode_bitmap: le_u32(&desc_data, offset + 4),
                inode_table: le_u32(&desc_data, offset + 8),
            });
        }

        Ok(Self {
            block_size,
            blocks_count,
            inodes_count,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            groups,
            root_ino: EXT2_ROOT_INO,
        })
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, Ext2Error> {
        let (_ino, inode) = self.lookup_path(path)?;
        if !inode.is_file() {
            return Err(Ext2Error::NotFile);
        }
        self.read_inode_data(&inode)
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<String>, Ext2Error> {
        let (_ino, inode) = self.lookup_path(path)?;
        if !inode.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }
        Ok(self
            .read_dir_entries(&inode)?
            .into_iter()
            .filter(|entry| entry.name != "." && entry.name != "..")
            .map(|entry| entry.name)
            .collect())
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn stats(&self) -> Ext2Stats {
        Ext2Stats {
            block_size: self.block_size,
            blocks_count: self.blocks_count,
            inodes_count: self.inodes_count,
            groups: self.groups.len(),
        }
    }

    fn lookup_path(&self, path: &str) -> Result<(u32, Inode), Ext2Error> {
        let mut ino = self.root_ino;
        let mut inode = self.read_inode(ino)?;
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok((ino, inode));
        }

        for component in trimmed.split('/').filter(|part| !part.is_empty()) {
            if component == "." {
                continue;
            }
            if !inode.is_dir() {
                return Err(Ext2Error::NotDirectory);
            }
            let entries = self.read_dir_entries(&inode)?;
            let next = entries
                .iter()
                .find(|entry| entry.name == component)
                .map(|entry| entry.ino)
                .ok_or(Ext2Error::NotFound)?;
            ino = next;
            inode = self.read_inode(ino)?;
        }

        Ok((ino, inode))
    }

    fn read_inode(&self, ino: u32) -> Result<Inode, Ext2Error> {
        if ino == 0 || ino > self.inodes_count {
            return Err(Ext2Error::NotFound);
        }
        let index = ino - 1;
        let group_index = (index / self.inodes_per_group) as usize;
        let group = self.groups.get(group_index).ok_or(Ext2Error::NotFound)?;
        let local_index = (index % self.inodes_per_group) as usize;
        let byte_offset = local_index * self.inode_size;
        let block = group.inode_table as u64 + (byte_offset / self.block_size) as u64;
        let offset = byte_offset % self.block_size;
        let mut data = vec![0u8; self.block_size];
        read_block_sized(block, self.block_size, &mut data)?;
        if offset + 128 > data.len() {
            return Err(Ext2Error::IoError);
        }
        let raw = &data[offset..offset + 128];

        let mut blocks = [0u32; EXT2_N_BLOCKS];
        for (i, slot) in blocks.iter_mut().enumerate() {
            *slot = le_u32(raw, 40 + i * 4);
        }

        let size_low = le_u32(raw, 4) as u64;
        let size_high = if (le_u16(raw, 0) & EXT2_S_IFMT) == EXT2_S_IFREG {
            le_u32(raw, 108) as u64
        } else {
            0
        };
        let uid = u32::from(le_u16(raw, 2)) | (u32::from(le_u16(raw, 120)) << 16);
        let gid = u32::from(le_u16(raw, 24)) | (u32::from(le_u16(raw, 122)) << 16);

        Ok(Inode {
            mode: le_u16(raw, 0),
            uid,
            gid,
            size: size_low | (size_high << 32),
            links: le_u16(raw, 26),
            blocks,
        })
    }

    fn read_inode_data(&self, inode: &Inode) -> Result<Vec<u8>, Ext2Error> {
        let mut out = Vec::new();
        let mut remaining = inode.size as usize;
        if remaining == 0 {
            return Ok(out);
        }

        for block in inode.blocks[..12].iter().copied().filter(|block| *block != 0) {
            self.append_data_block(block, &mut remaining, &mut out)?;
            if remaining == 0 {
                return Ok(out);
            }
        }

        if remaining > 0 && inode.blocks[12] != 0 {
            let mut indirect = vec![0u8; self.block_size];
            read_block_sized(inode.blocks[12] as u64, self.block_size, &mut indirect)?;
            for offset in (0..self.block_size).step_by(4) {
                let block = le_u32(&indirect, offset);
                if block == 0 {
                    continue;
                }
                self.append_data_block(block, &mut remaining, &mut out)?;
                if remaining == 0 {
                    return Ok(out);
                }
            }
        }

        if remaining == 0 {
            Ok(out)
        } else {
            Err(Ext2Error::Unsupported)
        }
    }

    fn append_data_block(
        &self,
        block: u32,
        remaining: &mut usize,
        out: &mut Vec<u8>,
    ) -> Result<(), Ext2Error> {
        let mut data = vec![0u8; self.block_size];
        read_block_sized(block as u64, self.block_size, &mut data)?;
        let count = (*remaining).min(self.block_size);
        out.extend_from_slice(&data[..count]);
        *remaining -= count;
        Ok(())
    }

    fn read_dir_entries(&self, inode: &Inode) -> Result<Vec<DirEntry>, Ext2Error> {
        let data = self.read_inode_data(inode)?;
        let mut entries = Vec::new();
        let mut offset = 0usize;
        let limit = data.len().min(inode.size as usize);
        while offset + 8 <= limit {
            let ino = le_u32(&data, offset);
            let rec_len = le_u16(&data, offset + 4) as usize;
            let name_len = data[offset + 6] as usize;
            let file_type = data[offset + 7];
            if rec_len < 8 || offset + rec_len > limit {
                break;
            }
            if ino != 0 && name_len > 0 && name_len <= rec_len.saturating_sub(8) {
                let name_bytes = &data[offset + 8..offset + 8 + name_len];
                if let Ok(name) = core::str::from_utf8(name_bytes) {
                    entries.push(DirEntry {
                        ino,
                        name: String::from(name),
                        file_type,
                    });
                }
            }
            offset += rec_len;
        }
        Ok(entries)
    }
}

impl Inode {
    fn is_file(&self) -> bool {
        self.mode & EXT2_S_IFMT == EXT2_S_IFREG
    }

    fn is_dir(&self) -> bool {
        self.mode & EXT2_S_IFMT == EXT2_S_IFDIR
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ext2Stats {
    pub block_size: usize,
    pub blocks_count: u32,
    pub inodes_count: u32,
    pub groups: usize,
}

fn read_block_sized(block: u64, block_size: usize, output: &mut [u8]) -> Result<(), Ext2Error> {
    if output.len() < block_size || block_size != 1024 {
        return Err(Ext2Error::IoError);
    }
    let sector = block * (block_size / 512) as u64;
    let mut bounce = [0u8; 1024];
    virtio_blk::read_sectors(sector, 1, &mut bounce[..512]).map_err(|_| Ext2Error::IoError)?;
    virtio_blk::read_sectors(sector + 1, 1, &mut bounce[512..])
        .map_err(|_| Ext2Error::IoError)?;
    output[..block_size].copy_from_slice(&bounce[..block_size]);
    Ok(())
}

fn le_u16(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

fn le_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

pub fn self_test() -> Result<(), Ext2Error> {
    let fs = Ext2Fs::mount().inspect_err(|err| {
        crate::println!("Ext2 self-test mount failed: {:?}", err);
    })?;
    let stats = fs.stats();
    if stats.block_size != 1024 || stats.groups == 0 {
        crate::println!(
            "Ext2 self-test invalid stats: block {}, groups {}.",
            stats.block_size,
            stats.groups
        );
        return Err(Ext2Error::InvalidSuperblock);
    }
    let data = fs.read_file("/etc/os-release").inspect_err(|err| {
        crate::println!("Ext2 self-test read /etc/os-release failed: {:?}", err);
    })?;
    if !data.starts_with(b"NAME=ristux") {
        crate::println!("Ext2 self-test read unexpected os-release contents.");
        return Err(Ext2Error::InvalidSuperblock);
    }
    let root = fs.list_dir("/").inspect_err(|err| {
        crate::println!("Ext2 self-test list / failed: {:?}", err);
    })?;
    if !root.iter().any(|entry| entry == "bin") || !root.iter().any(|entry| entry == "home") {
        crate::println!("Ext2 self-test root entries missing bin/home.");
        return Err(Ext2Error::NotFound);
    }
    Ok(())
}
