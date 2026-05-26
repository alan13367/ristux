use alloc::{string::String, vec, vec::Vec};

use crate::{
    drivers::virtio_blk,
    security::{FileMetadata, FileMode},
};

const EXT2_MAGIC: u16 = 0xEF53;
const EXT2_ROOT_INO: u32 = 2;
const EXT2_N_BLOCKS: usize = 15;
const EXT2_S_IFMT: u16 = 0o170000;
const EXT2_S_IFREG: u16 = 0o100000;
const EXT2_S_IFDIR: u16 = 0o040000;
const EXT2_FIRST_NORMAL_INO: u32 = 11;
const EXT2_FT_REG_FILE: u8 = 1;
const EXT2_FT_DIR: u8 = 2;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Ext2Error {
    InvalidSuperblock,
    NotFound,
    NotDirectory,
    NotFile,
    AlreadyExists,
    NoSpace,
    DirectoryFull,
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

impl Inode {
    fn empty_file(uid: u32, gid: u32, mode: u16) -> Self {
        Self {
            mode: EXT2_S_IFREG | (mode & 0o7777),
            uid,
            gid,
            size: 0,
            links: 1,
            blocks: [0; EXT2_N_BLOCKS],
        }
    }

    fn empty_dir(uid: u32, gid: u32, mode: u16, block: u32) -> Self {
        let mut blocks = [0; EXT2_N_BLOCKS];
        blocks[0] = block;
        Self {
            mode: EXT2_S_IFDIR | (mode & 0o7777),
            uid,
            gid,
            size: 1024,
            links: 2,
            blocks,
        }
    }
}

#[derive(Clone)]
struct DirEntry {
    ino: u32,
    name: String,
    file_type: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ext2NodeKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug)]
pub struct Ext2Metadata {
    pub kind: Ext2NodeKind,
    pub metadata: FileMetadata,
    pub size: u64,
    pub links: u16,
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

    pub fn metadata(&self, path: &str) -> Result<Ext2Metadata, Ext2Error> {
        let (_ino, inode) = self.lookup_path(path)?;
        let kind = if inode.is_dir() {
            Ext2NodeKind::Directory
        } else if inode.is_file() {
            Ext2NodeKind::File
        } else {
            return Err(Ext2Error::Unsupported);
        };
        Ok(Ext2Metadata {
            kind,
            metadata: FileMetadata {
                owner: inode.uid,
                group: inode.gid,
                mode: FileMode::new(inode.mode & 0o7777),
            },
            size: inode.size,
            links: inode.links,
        })
    }

    pub fn create_file(
        &mut self,
        path: &str,
        uid: u32,
        gid: u32,
        mode: u16,
    ) -> Result<(), Ext2Error> {
        if self.lookup_path(path).is_ok() {
            return Err(Ext2Error::AlreadyExists);
        }
        let (parent_path, name) = split_parent_name(path)?;
        let (parent_ino, parent) = self.lookup_path(&parent_path)?;
        if !parent.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }

        let ino = self.allocate_inode()?;
        let inode = Inode::empty_file(uid, gid, mode);
        self.write_inode(ino, &inode)?;
        if let Err(err) = self.insert_dir_entry(parent_ino, &parent, ino, &name, EXT2_FT_REG_FILE)
        {
            let _ = self.free_inode(ino);
            return Err(err);
        }
        Ok(())
    }

    pub fn create_dir(
        &mut self,
        path: &str,
        uid: u32,
        gid: u32,
        mode: u16,
    ) -> Result<(), Ext2Error> {
        if self.lookup_path(path).is_ok() {
            return Err(Ext2Error::AlreadyExists);
        }
        let (parent_path, name) = split_parent_name(path)?;
        let (parent_ino, mut parent) = self.lookup_path(&parent_path)?;
        if !parent.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }

        let ino = self.allocate_inode()?;
        let block = match self.allocate_block() {
            Ok(block) => block,
            Err(err) => {
                let _ = self.free_inode(ino);
                return Err(err);
            }
        };
        if let Err(err) = self.write_dir_block(block, ino, parent_ino) {
            let _ = self.free_block(block);
            let _ = self.free_inode(ino);
            return Err(err);
        }
        let inode = Inode::empty_dir(uid, gid, mode, block);
        if let Err(err) = self.write_inode(ino, &inode) {
            let _ = self.free_block(block);
            let _ = self.free_inode(ino);
            return Err(err);
        }
        if let Err(err) = self.insert_dir_entry(parent_ino, &parent, ino, &name, EXT2_FT_DIR) {
            let _ = self.free_inode_blocks(&inode);
            let _ = self.free_inode(ino);
            return Err(err);
        }
        parent.links = parent.links.saturating_add(1);
        self.write_inode(parent_ino, &parent)
    }

    pub fn truncate_file(&mut self, path: &str) -> Result<(), Ext2Error> {
        self.write_file(path, &[])
    }

    pub fn link(&mut self, old_path: &str, new_path: &str) -> Result<(), Ext2Error> {
        if self.lookup_path(new_path).is_ok() {
            return Err(Ext2Error::AlreadyExists);
        }
        let (source_ino, mut source) = self.lookup_path(old_path)?;
        if !source.is_file() {
            return Err(Ext2Error::NotFile);
        }
        let (parent_path, name) = split_parent_name(new_path)?;
        let (parent_ino, parent) = self.lookup_path(&parent_path)?;
        if !parent.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }
        self.insert_dir_entry(parent_ino, &parent, source_ino, &name, EXT2_FT_REG_FILE)?;
        source.links = source.links.saturating_add(1);
        self.write_inode(source_ino, &source)
    }

    pub fn unlink(&mut self, path: &str) -> Result<(), Ext2Error> {
        let (parent_path, name) = split_parent_name(path)?;
        let (_ino, mut inode) = self.lookup_path(path)?;
        if !inode.is_file() {
            return Err(Ext2Error::NotFile);
        }
        let (_parent_ino, parent) = self.lookup_path(&parent_path)?;
        if !parent.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }
        let removed_ino = self.remove_dir_entry(&parent, &name)?;
        if inode.links > 1 {
            inode.links -= 1;
            self.write_inode(removed_ino, &inode)
        } else {
            self.free_inode_blocks(&inode)?;
            self.free_inode(removed_ino)
        }
    }

    pub fn rmdir(&mut self, path: &str) -> Result<(), Ext2Error> {
        if path == "/" {
            return Err(Ext2Error::Unsupported);
        }
        let (parent_path, name) = split_parent_name(path)?;
        let (ino, inode) = self.lookup_path(path)?;
        if !inode.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }
        if !self.is_empty_dir(&inode)? {
            return Err(Ext2Error::Unsupported);
        }
        let (parent_ino, mut parent) = self.lookup_path(&parent_path)?;
        if !parent.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }
        let removed_ino = self.remove_dir_entry(&parent, &name)?;
        if removed_ino != ino {
            return Err(Ext2Error::IoError);
        }
        self.free_inode_blocks(&inode)?;
        self.free_inode(ino)?;
        parent.links = parent.links.saturating_sub(1).max(2);
        self.write_inode(parent_ino, &parent)
    }

    pub fn rename(&mut self, old_path: &str, new_path: &str) -> Result<(), Ext2Error> {
        if old_path == new_path {
            return Ok(());
        }
        if old_path == "/" || new_path == "/" {
            return Err(Ext2Error::Unsupported);
        }
        if self.lookup_path(new_path).is_ok() {
            let (old_ino, _old_inode) = self.lookup_path(old_path)?;
            let (new_ino, new_inode) = self.lookup_path(new_path)?;
            if new_ino == old_ino {
                let (old_parent_path, old_name) = split_parent_name(old_path)?;
                let (_old_parent_ino, old_parent) = self.lookup_path(&old_parent_path)?;
                let _ = self.remove_dir_entry(&old_parent, &old_name)?;
                return Ok(());
            }
            if !new_inode.is_file() {
                return Err(Ext2Error::NotFile);
            }
            self.unlink(new_path)?;
        }

        let (old_parent_path, old_name) = split_parent_name(old_path)?;
        let (new_parent_path, new_name) = split_parent_name(new_path)?;
        let (old_ino, old_inode) = self.lookup_path(old_path)?;
        if old_inode.is_dir() && old_parent_path != new_parent_path {
            return Err(Ext2Error::Unsupported);
        }
        let (_old_parent_ino, old_parent) = self.lookup_path(&old_parent_path)?;
        let (new_parent_ino, new_parent) = self.lookup_path(&new_parent_path)?;
        if !old_parent.is_dir() || !new_parent.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }
        let file_type = if old_inode.is_dir() {
            EXT2_FT_DIR
        } else if old_inode.is_file() {
            EXT2_FT_REG_FILE
        } else {
            return Err(Ext2Error::Unsupported);
        };
        self.insert_dir_entry(new_parent_ino, &new_parent, old_ino, &new_name, file_type)?;
        let removed_ino = self.remove_dir_entry(&old_parent, &old_name)?;
        if removed_ino == old_ino {
            Ok(())
        } else {
            Err(Ext2Error::IoError)
        }
    }

    pub fn chmod(&mut self, path: &str, mode: u16) -> Result<(), Ext2Error> {
        let (ino, mut inode) = self.lookup_path(path)?;
        inode.mode = (inode.mode & EXT2_S_IFMT) | (mode & 0o7777);
        self.write_inode(ino, &inode)
    }

    pub fn chown(&mut self, path: &str, uid: u32, gid: u32) -> Result<(), Ext2Error> {
        let (ino, mut inode) = self.lookup_path(path)?;
        if uid != u32::MAX {
            inode.uid = uid;
        }
        if gid != u32::MAX {
            inode.gid = gid;
        }
        self.write_inode(ino, &inode)
    }

    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), Ext2Error> {
        let (ino, mut inode) = self.lookup_path(path)?;
        if !inode.is_file() {
            return Err(Ext2Error::NotFile);
        }
        self.free_inode_blocks(&inode)?;

        let blocks_needed = data.len().div_ceil(self.block_size);
        if blocks_needed > 12 + self.block_size / 4 {
            return Err(Ext2Error::Unsupported);
        }

        let mut data_blocks = Vec::with_capacity(blocks_needed);
        for index in 0..blocks_needed {
            let block = self.allocate_block()?;
            let start = index * self.block_size;
            let end = (start + self.block_size).min(data.len());
            self.write_data_block(block, &data[start..end])?;
            data_blocks.push(block);
        }

        inode.blocks = [0; EXT2_N_BLOCKS];
        for (index, block) in data_blocks.iter().take(12).enumerate() {
            inode.blocks[index] = *block;
        }
        if data_blocks.len() > 12 {
            let indirect = self.allocate_block()?;
            let mut indirect_data = [0u8; 1024];
            for (index, block) in data_blocks.iter().skip(12).enumerate() {
                put_u32(&mut indirect_data, index * 4, *block);
            }
            write_block_sized(indirect as u64, self.block_size, &indirect_data)?;
            inode.blocks[12] = indirect;
        }
        inode.size = data.len() as u64;
        self.write_inode(ino, &inode)
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

    fn is_empty_dir(&self, inode: &Inode) -> Result<bool, Ext2Error> {
        if !inode.is_dir() {
            return Err(Ext2Error::NotDirectory);
        }
        Ok(self
            .read_dir_entries(inode)?
            .into_iter()
            .all(|entry| entry.name == "." || entry.name == ".."))
    }

    fn write_inode(&self, ino: u32, inode: &Inode) -> Result<(), Ext2Error> {
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
        let mut data = [0u8; 1024];
        read_block_sized(block, self.block_size, &mut data)?;
        {
            let raw = &mut data[offset..offset + 128];
            raw.fill(0);
            put_u16(raw, 0, inode.mode);
            put_u16(raw, 2, inode.uid as u16);
            put_u32(raw, 4, inode.size as u32);
            put_u32(raw, 8, 1);
            put_u32(raw, 12, 1);
            put_u32(raw, 16, 1);
            put_u16(raw, 24, inode.gid as u16);
            put_u16(raw, 26, inode.links);
            put_u32(raw, 28, self.inode_sector_count(inode));
            for (i, block) in inode.blocks.iter().enumerate() {
                put_u32(raw, 40 + i * 4, *block);
            }
            put_u32(raw, 108, (inode.size >> 32) as u32);
            put_u16(raw, 120, (inode.uid >> 16) as u16);
            put_u16(raw, 122, (inode.gid >> 16) as u16);
        }
        write_block_sized(block, self.block_size, &data)
    }

    fn inode_sector_count(&self, inode: &Inode) -> u32 {
        let mut blocks = inode.blocks[..12]
            .iter()
            .filter(|block| **block != 0)
            .count();
        if inode.blocks[12] != 0 {
            blocks += 1;
            if let Ok(indirect) = self.read_indirect_blocks(inode.blocks[12]) {
                blocks += indirect.len();
            }
        }
        (blocks * (self.block_size / 512)) as u32
    }

    fn read_indirect_blocks(&self, block: u32) -> Result<Vec<u32>, Ext2Error> {
        let mut indirect = [0u8; 1024];
        read_block_sized(block as u64, self.block_size, &mut indirect)?;
        let mut blocks = Vec::new();
        for offset in (0..self.block_size).step_by(4) {
            let block = le_u32(&indirect, offset);
            if block != 0 {
                blocks.push(block);
            }
        }
        Ok(blocks)
    }

    fn free_inode_blocks(&mut self, inode: &Inode) -> Result<(), Ext2Error> {
        for block in inode.blocks[..12].iter().copied().filter(|block| *block != 0) {
            self.free_block(block)?;
        }
        if inode.blocks[12] != 0 {
            for block in self.read_indirect_blocks(inode.blocks[12])? {
                self.free_block(block)?;
            }
            self.free_block(inode.blocks[12])?;
        }
        Ok(())
    }

    fn insert_dir_entry(
        &mut self,
        _parent_ino: u32,
        parent: &Inode,
        child_ino: u32,
        name: &str,
        file_type: u8,
    ) -> Result<(), Ext2Error> {
        if name.is_empty() || name.len() > 255 {
            return Err(Ext2Error::Unsupported);
        }
        let block = parent.blocks[0];
        if block == 0 {
            return Err(Ext2Error::Unsupported);
        }
        let mut data = [0u8; 1024];
        read_block_sized(block as u64, self.block_size, &mut data)?;
        let needed = align4(8 + name.len());
        let mut offset = 0usize;
        while offset + 8 <= self.block_size {
            let rec_len = le_u16(&data, offset + 4) as usize;
            let name_len = data[offset + 6] as usize;
            if rec_len < 8 || offset + rec_len > self.block_size {
                break;
            }
            let actual = align4(8 + name_len);
            if rec_len >= actual + needed {
                put_u16(&mut data, offset + 4, actual as u16);
                let new_offset = offset + actual;
                let new_rec_len = rec_len - actual;
                put_u32(&mut data, new_offset, child_ino);
                put_u16(&mut data, new_offset + 4, new_rec_len as u16);
                data[new_offset + 6] = name.len() as u8;
                data[new_offset + 7] = file_type;
                data[new_offset + 8..new_offset + 8 + name.len()]
                    .copy_from_slice(name.as_bytes());
                write_block_sized(block as u64, self.block_size, &data)?;
                return Ok(());
            }
            offset += rec_len;
        }
        Err(Ext2Error::DirectoryFull)
    }

    fn remove_dir_entry(&mut self, parent: &Inode, name: &str) -> Result<u32, Ext2Error> {
        if name.is_empty() || name == "." || name == ".." {
            return Err(Ext2Error::Unsupported);
        }
        let block = parent.blocks[0];
        if block == 0 {
            return Err(Ext2Error::Unsupported);
        }
        let mut data = [0u8; 1024];
        read_block_sized(block as u64, self.block_size, &mut data)?;
        let mut offset = 0usize;
        let mut previous_offset = None;
        while offset + 8 <= self.block_size {
            let ino = le_u32(&data, offset);
            let rec_len = le_u16(&data, offset + 4) as usize;
            let name_len = data[offset + 6] as usize;
            if rec_len < 8 || offset + rec_len > self.block_size {
                break;
            }
            if ino != 0 && name_len > 0 && name_len <= rec_len.saturating_sub(8) {
                let name_bytes = &data[offset + 8..offset + 8 + name_len];
                if name_bytes == name.as_bytes() {
                    if let Some(previous_offset) = previous_offset {
                        let previous_len = le_u16(&data, previous_offset + 4) as usize;
                        put_u16(
                            &mut data,
                            previous_offset + 4,
                            (previous_len + rec_len) as u16,
                        );
                    } else {
                        put_u32(&mut data, offset, 0);
                    }
                    write_block_sized(block as u64, self.block_size, &data)?;
                    return Ok(ino);
                }
            }
            previous_offset = Some(offset);
            offset += rec_len;
        }
        Err(Ext2Error::NotFound)
    }

    fn write_dir_block(
        &self,
        block: u32,
        self_ino: u32,
        parent_ino: u32,
    ) -> Result<(), Ext2Error> {
        let mut data = [0u8; 1024];
        write_dir_record(&mut data, 0, self_ino, 12, ".", EXT2_FT_DIR);
        write_dir_record(
            &mut data,
            12,
            parent_ino,
            (self.block_size - 12) as u16,
            "..",
            EXT2_FT_DIR,
        );
        write_block_sized(block as u64, self.block_size, &data)
    }

    fn allocate_inode(&mut self) -> Result<u32, Ext2Error> {
        for (group_index, group) in self.groups.iter().enumerate() {
            let mut bitmap = [0u8; 1024];
            read_block_sized(group.inode_bitmap as u64, self.block_size, &mut bitmap)?;
            let start = if group_index == 0 {
                (EXT2_FIRST_NORMAL_INO - 1) as usize
            } else {
                0
            };
            for bit in start..self.inodes_per_group as usize {
                let ino = group_index as u32 * self.inodes_per_group + bit as u32 + 1;
                if ino > self.inodes_count {
                    return Err(Ext2Error::NoSpace);
                }
                if !bitmap_bit(&bitmap, bit) {
                    set_bitmap_bit(&mut bitmap, bit, true);
                    write_block_sized(group.inode_bitmap as u64, self.block_size, &bitmap)?;
                    return Ok(ino);
                }
            }
        }
        Err(Ext2Error::NoSpace)
    }

    fn free_inode(&mut self, ino: u32) -> Result<(), Ext2Error> {
        if ino == 0 || ino > self.inodes_count {
            return Err(Ext2Error::NotFound);
        }
        let index = ino - 1;
        let group_index = (index / self.inodes_per_group) as usize;
        let group = self.groups.get(group_index).ok_or(Ext2Error::NotFound)?;
        let bit = (index % self.inodes_per_group) as usize;
        let mut bitmap = [0u8; 1024];
        read_block_sized(group.inode_bitmap as u64, self.block_size, &mut bitmap)?;
        set_bitmap_bit(&mut bitmap, bit, false);
        write_block_sized(group.inode_bitmap as u64, self.block_size, &bitmap)
    }

    fn allocate_block(&mut self) -> Result<u32, Ext2Error> {
        for (group_index, group) in self.groups.iter().enumerate() {
            let mut bitmap = [0u8; 1024];
            read_block_sized(group.block_bitmap as u64, self.block_size, &mut bitmap)?;
            for bit in 0..self.blocks_per_group as usize {
                let block = group_index as u32 * self.blocks_per_group + bit as u32;
                if block >= self.blocks_count {
                    return Err(Ext2Error::NoSpace);
                }
                if !bitmap_bit(&bitmap, bit) {
                    set_bitmap_bit(&mut bitmap, bit, true);
                    write_block_sized(group.block_bitmap as u64, self.block_size, &bitmap)?;
                    return Ok(block);
                }
            }
        }
        Err(Ext2Error::NoSpace)
    }

    fn free_block(&mut self, block: u32) -> Result<(), Ext2Error> {
        if block >= self.blocks_count {
            return Err(Ext2Error::NotFound);
        }
        let group_index = (block / self.blocks_per_group) as usize;
        let group = self.groups.get(group_index).ok_or(Ext2Error::NotFound)?;
        let bit = (block % self.blocks_per_group) as usize;
        let mut bitmap = [0u8; 1024];
        read_block_sized(group.block_bitmap as u64, self.block_size, &mut bitmap)?;
        set_bitmap_bit(&mut bitmap, bit, false);
        write_block_sized(group.block_bitmap as u64, self.block_size, &bitmap)
    }

    fn write_data_block(&self, block: u32, data: &[u8]) -> Result<(), Ext2Error> {
        let mut full = [0u8; 1024];
        let count = data.len().min(self.block_size);
        full[..count].copy_from_slice(&data[..count]);
        write_block_sized(block as u64, self.block_size, &full)
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

fn write_block_sized(block: u64, block_size: usize, input: &[u8]) -> Result<(), Ext2Error> {
    if input.len() < block_size || block_size != 1024 {
        return Err(Ext2Error::IoError);
    }
    let sector = block * (block_size / 512) as u64;
    virtio_blk::write_sectors(sector, 1, &input[..512]).map_err(|_| Ext2Error::IoError)?;
    virtio_blk::write_sectors(sector + 1, 1, &input[512..1024])
        .map_err(|_| Ext2Error::IoError)?;
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

fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_dir_record(
    buf: &mut [u8],
    offset: usize,
    ino: u32,
    rec_len: u16,
    name: &str,
    file_type: u8,
) {
    put_u32(buf, offset, ino);
    put_u16(buf, offset + 4, rec_len);
    buf[offset + 6] = name.len() as u8;
    buf[offset + 7] = file_type;
    buf[offset + 8..offset + 8 + name.len()].copy_from_slice(name.as_bytes());
}

fn bitmap_bit(buf: &[u8], bit: usize) -> bool {
    buf[bit / 8] & (1 << (bit % 8)) != 0
}

fn set_bitmap_bit(buf: &mut [u8], bit: usize, used: bool) {
    if used {
        buf[bit / 8] |= 1 << (bit % 8);
    } else {
        buf[bit / 8] &= !(1 << (bit % 8));
    }
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

fn split_parent_name(path: &str) -> Result<(String, String), Ext2Error> {
    let trimmed = path.trim_end_matches('/');
    if !trimmed.starts_with('/') || trimmed == "/" {
        return Err(Ext2Error::NotFound);
    }
    let slash = trimmed.rfind('/').ok_or(Ext2Error::NotFound)?;
    let parent = if slash == 0 { "/" } else { &trimmed[..slash] };
    let name = &trimmed[slash + 1..];
    if name.is_empty() {
        return Err(Ext2Error::NotFound);
    }
    Ok((String::from(parent), String::from(name)))
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
