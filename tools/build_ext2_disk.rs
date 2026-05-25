use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

const BLOCK_SIZE: usize = 1024;
const DISK_SIZE: usize = 64 * 1024 * 1024;
const BLOCKS_COUNT: u32 = (DISK_SIZE / BLOCK_SIZE) as u32;
const BLOCKS_PER_GROUP: u32 = 8192;
const INODES_PER_GROUP: u32 = 512;
const INODE_SIZE: usize = 128;
const GROUP_COUNT: u32 = BLOCKS_COUNT / BLOCKS_PER_GROUP;
const INODES_COUNT: u32 = GROUP_COUNT * INODES_PER_GROUP;
const INODE_TABLE_BLOCKS: u32 = (INODES_PER_GROUP * INODE_SIZE as u32) / BLOCK_SIZE as u32;
const ROOT_INODE: u32 = 2;
const FIRST_NORMAL_INODE: u32 = 11;

#[derive(Clone, Copy, Eq, PartialEq)]
enum EntryKind {
    File,
    Directory,
}

struct Entry {
    kind: EntryKind,
    data: Vec<u8>,
    mode: u16,
    uid: u16,
    gid: u16,
    inode: u32,
    data_blocks: Vec<u32>,
    indirect_block: Option<u32>,
}

impl Entry {
    fn directory(mode: u16, uid: u16, gid: u16) -> Self {
        Self {
            kind: EntryKind::Directory,
            data: Vec::new(),
            mode,
            uid,
            gid,
            inode: 0,
            data_blocks: Vec::new(),
            indirect_block: None,
        }
    }

    fn file(data: Vec<u8>, mode: u16, uid: u16, gid: u16) -> Self {
        Self {
            kind: EntryKind::File,
            data,
            mode,
            uid,
            gid,
            inode: 0,
            data_blocks: Vec::new(),
            indirect_block: None,
        }
    }
}

struct Builder {
    image: Vec<u8>,
    entries: BTreeMap<String, Entry>,
    block_used: Vec<bool>,
    inode_used: Vec<bool>,
}

impl Builder {
    fn new() -> Self {
        let mut builder = Self {
            image: vec![0; DISK_SIZE],
            entries: BTreeMap::new(),
            block_used: vec![false; BLOCKS_COUNT as usize],
            inode_used: vec![false; INODES_COUNT as usize],
        };
        builder.reserve_metadata();
        for inode in 1..FIRST_NORMAL_INODE {
            builder.mark_inode(inode);
        }
        builder
            .entries
            .insert(String::from("/"), Entry::directory(0o755, 0, 0));
        builder
    }

    fn reserve_metadata(&mut self) {
        self.mark_block(0);
        for group in 0..GROUP_COUNT {
            let first = group * BLOCKS_PER_GROUP;
            if group == 0 {
                self.mark_block(1);
                self.mark_block(2);
                self.mark_block(3);
                self.mark_block(4);
                for block in 5..5 + INODE_TABLE_BLOCKS {
                    self.mark_block(block);
                }
            } else {
                self.mark_block(first);
                self.mark_block(first + 1);
                for block in first + 2..first + 2 + INODE_TABLE_BLOCKS {
                    self.mark_block(block);
                }
            }
        }
    }

    fn mark_block(&mut self, block: u32) {
        self.block_used[block as usize] = true;
    }

    fn mark_inode(&mut self, inode: u32) {
        self.inode_used[(inode - 1) as usize] = true;
    }

    fn ensure_dir(&mut self, path: &str, mode: u16, uid: u16, gid: u16) {
        if path == "/" {
            return;
        }
        let parent = parent_path(path);
        self.ensure_dir(&parent, 0o755, 0, 0);
        self.entries
            .entry(String::from(path))
            .or_insert_with(|| Entry::directory(mode, uid, gid));
    }

    fn add_file(&mut self, path: &str, data: Vec<u8>, mode: u16, uid: u16, gid: u16) {
        let parent = parent_path(path);
        self.ensure_dir(&parent, 0o755, 0, 0);
        self.entries
            .insert(String::from(path), Entry::file(data, mode, uid, gid));
    }

    fn assign_inodes(&mut self) {
        if let Some(root) = self.entries.get_mut("/") {
            root.inode = ROOT_INODE;
        }
        self.mark_inode(ROOT_INODE);

        let paths = self
            .entries
            .keys()
            .filter(|path| path.as_str() != "/")
            .cloned()
            .collect::<Vec<_>>();
        let mut next = FIRST_NORMAL_INODE;
        for path in paths {
            while self.inode_used[(next - 1) as usize] {
                next += 1;
            }
            self.entries.get_mut(&path).expect("entry vanished").inode = next;
            self.mark_inode(next);
            next += 1;
        }
    }

    fn allocate_payload_blocks(&mut self) {
        let paths = self.entries.keys().cloned().collect::<Vec<_>>();
        for path in paths {
            let kind = self.entries[&path].kind;
            let data = if kind == EntryKind::Directory {
                self.directory_payload(&path)
            } else {
                self.entries[&path].data.clone()
            };
            let mut blocks = Vec::new();
            for chunk in data.chunks(BLOCK_SIZE) {
                let block = self.alloc_block();
                self.write_block_prefix(block, chunk);
                blocks.push(block);
            }
            if blocks.is_empty() {
                let block = self.alloc_block();
                blocks.push(block);
            }
            let indirect = if blocks.len() > 12 {
                let indirect = self.alloc_block();
                let mut block_data = [0u8; BLOCK_SIZE];
                for (index, block) in blocks.iter().enumerate() {
                    let offset = index * 4;
                    block_data[offset..offset + 4].copy_from_slice(&block.to_le_bytes());
                }
                self.write_block(indirect, &block_data);
                Some(indirect)
            } else {
                None
            };
            let entry = self.entries.get_mut(&path).expect("entry vanished");
            if kind == EntryKind::Directory {
                entry.data = data;
            }
            entry.data_blocks = blocks;
            entry.indirect_block = indirect;
        }
    }

    fn directory_payload(&self, path: &str) -> Vec<u8> {
        let mut records = Vec::new();
        let self_inode = self.entries[path].inode;
        let parent_inode = if path == "/" {
            ROOT_INODE
        } else {
            self.entries[&parent_path(path)].inode
        };
        records.push((self_inode, String::from("."), EntryKind::Directory));
        records.push((parent_inode, String::from(".."), EntryKind::Directory));

        for (child_path, child) in &self.entries {
            if child_path == path || child_path == "/" {
                continue;
            }
            if parent_path(child_path) == path {
                records.push((child.inode, file_name(child_path), child.kind));
            }
        }

        let mut data = Vec::new();
        for (index, (inode, name, kind)) in records.iter().enumerate() {
            let min_len = align4(8 + name.len());
            let rec_len = if index + 1 == records.len() {
                BLOCK_SIZE - data.len()
            } else {
                min_len
            };
            data.extend_from_slice(&inode.to_le_bytes());
            data.extend_from_slice(&(rec_len as u16).to_le_bytes());
            data.push(name.len() as u8);
            data.push(match kind {
                EntryKind::File => 1,
                EntryKind::Directory => 2,
            });
            data.extend_from_slice(name.as_bytes());
            while data.len() % 4 != 0 {
                data.push(0);
            }
            while data.len() % BLOCK_SIZE != 0 && data.len() % BLOCK_SIZE < rec_len {
                data.push(0);
            }
        }
        data.resize(BLOCK_SIZE, 0);
        data
    }

    fn alloc_block(&mut self) -> u32 {
        let block = self
            .block_used
            .iter()
            .position(|used| !*used)
            .expect("disk image ran out of blocks") as u32;
        self.mark_block(block);
        block
    }

    fn write_block_prefix(&mut self, block: u32, data: &[u8]) {
        let offset = block as usize * BLOCK_SIZE;
        self.image[offset..offset + data.len()].copy_from_slice(data);
    }

    fn write_block(&mut self, block: u32, data: &[u8; BLOCK_SIZE]) {
        let offset = block as usize * BLOCK_SIZE;
        self.image[offset..offset + BLOCK_SIZE].copy_from_slice(data);
    }

    fn write_metadata(&mut self) {
        self.write_superblock();
        self.write_group_descriptors();
        self.write_bitmaps();
        self.write_inodes();
    }

    fn write_superblock(&mut self) {
        let mut sb = [0u8; BLOCK_SIZE];
        let free_blocks = self.block_used.iter().filter(|used| !**used).count() as u32;
        let free_inodes = self.inode_used.iter().filter(|used| !**used).count() as u32;

        put_u32(&mut sb, 0, INODES_COUNT);
        put_u32(&mut sb, 4, BLOCKS_COUNT);
        put_u32(&mut sb, 8, 0);
        put_u32(&mut sb, 12, free_blocks);
        put_u32(&mut sb, 16, free_inodes);
        put_u32(&mut sb, 20, 1);
        put_u32(&mut sb, 24, 0);
        put_u32(&mut sb, 28, 0);
        put_u32(&mut sb, 32, BLOCKS_PER_GROUP);
        put_u32(&mut sb, 36, BLOCKS_PER_GROUP);
        put_u32(&mut sb, 40, INODES_PER_GROUP);
        put_u16(&mut sb, 52, 0);
        put_u16(&mut sb, 54, 0xffff);
        put_u16(&mut sb, 56, 0xef53);
        put_u16(&mut sb, 58, 1);
        put_u16(&mut sb, 60, 1);
        put_u32(&mut sb, 76, 1);
        put_u32(&mut sb, 84, FIRST_NORMAL_INODE);
        put_u16(&mut sb, 88, INODE_SIZE as u16);
        put_u32(&mut sb, 96, 0x2);
        put_u32(&mut sb, 100, 0);
        put_u32(&mut sb, 104, 0);
        self.write_block(1, &sb);
    }

    fn write_group_descriptors(&mut self) {
        let mut gdt = [0u8; BLOCK_SIZE];
        for group in 0..GROUP_COUNT {
            let desc = group as usize * 32;
            let first = group * BLOCKS_PER_GROUP;
            let (block_bitmap, inode_bitmap, inode_table) = if group == 0 {
                (3, 4, 5)
            } else {
                (first, first + 1, first + 2)
            };
            let block_start = first as usize;
            let block_end = block_start + BLOCKS_PER_GROUP as usize;
            let free_blocks = self.block_used[block_start..block_end]
                .iter()
                .filter(|used| !**used)
                .count() as u16;
            let inode_start = (group * INODES_PER_GROUP) as usize;
            let inode_end = inode_start + INODES_PER_GROUP as usize;
            let free_inodes = self.inode_used[inode_start..inode_end]
                .iter()
                .filter(|used| !**used)
                .count() as u16;
            let used_dirs = self
                .entries
                .values()
                .filter(|entry| {
                    entry.kind == EntryKind::Directory
                        && (entry.inode - 1) / INODES_PER_GROUP == group
                })
                .count() as u16;

            put_u32(&mut gdt, desc, block_bitmap);
            put_u32(&mut gdt, desc + 4, inode_bitmap);
            put_u32(&mut gdt, desc + 8, inode_table);
            put_u16(&mut gdt, desc + 12, free_blocks);
            put_u16(&mut gdt, desc + 14, free_inodes);
            put_u16(&mut gdt, desc + 16, used_dirs);
        }
        self.write_block(2, &gdt);
    }

    fn write_bitmaps(&mut self) {
        for group in 0..GROUP_COUNT {
            let first = group * BLOCKS_PER_GROUP;
            let (block_bitmap, inode_bitmap) = if group == 0 {
                (3, 4)
            } else {
                (first, first + 1)
            };

            let mut blocks = [0u8; BLOCK_SIZE];
            for index in 0..BLOCKS_PER_GROUP as usize {
                if self.block_used[first as usize + index] {
                    set_bitmap_bit(&mut blocks, index);
                }
            }
            self.write_block(block_bitmap, &blocks);

            let mut inodes = [0u8; BLOCK_SIZE];
            let inode_start = (group * INODES_PER_GROUP) as usize;
            for index in 0..INODES_PER_GROUP as usize {
                if self.inode_used[inode_start + index] {
                    set_bitmap_bit(&mut inodes, index);
                }
            }
            self.write_block(inode_bitmap, &inodes);
        }
    }

    fn write_inodes(&mut self) {
        for (path, entry) in &self.entries {
            let group = (entry.inode - 1) / INODES_PER_GROUP;
            let index = (entry.inode - 1) % INODES_PER_GROUP;
            let table_block = if group == 0 {
                5
            } else {
                group * BLOCKS_PER_GROUP + 2
            };
            let offset =
                table_block as usize * BLOCK_SIZE + index as usize * INODE_SIZE;
            let type_bits = match entry.kind {
                EntryKind::File => 0o100000,
                EntryKind::Directory => 0o040000,
            };
            let size = if entry.kind == EntryKind::Directory {
                BLOCK_SIZE as u32
            } else {
                entry.data.len() as u32
            };
            let links = match entry.kind {
                EntryKind::File => 1,
                EntryKind::Directory => 2 + self.immediate_subdir_count(path) as u16,
            };
            let sectors = ((entry.data_blocks.len() + entry.indirect_block.iter().count()) * 2)
                as u32;
            let mode = type_bits | entry.mode;
            let uid = entry.uid;
            let gid = entry.gid;
            let data_blocks = entry.data_blocks.clone();
            let indirect_block = entry.indirect_block;

            let inode = &mut self.image[offset..offset + INODE_SIZE];
            put_u16(inode, 0, mode);
            put_u16(inode, 2, uid);
            put_u32(inode, 4, size);
            put_u32(inode, 8, 1);
            put_u32(inode, 12, 1);
            put_u32(inode, 16, 1);
            put_u16(inode, 24, gid);
            put_u16(inode, 26, links);
            put_u32(inode, 28, sectors);
            for (i, block) in data_blocks.iter().take(12).enumerate() {
                put_u32(inode, 40 + i * 4, *block);
            }
            if let Some(indirect) = indirect_block {
                put_u32(inode, 40 + 12 * 4, indirect);
            }
        }
    }

    fn immediate_subdir_count(&self, path: &str) -> usize {
        self.entries
            .iter()
            .filter(|(child_path, child)| {
                child.kind == EntryKind::Directory
                    && child_path.as_str() != path
                    && child_path.as_str() != "/"
                    && parent_path(child_path) == path
            })
            .count()
    }

    fn finish(mut self, output: &Path) {
        self.assign_inodes();
        self.allocate_payload_blocks();
        self.write_metadata();
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).expect("create disk output directory");
        }
        let mut file = fs::File::create(output).expect("create ext2 disk image");
        file.write_all(&self.image).expect("write ext2 disk image");
    }
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: build_ext2_disk <output-disk> <manifest>");
        std::process::exit(2);
    }

    let output = PathBuf::from(&args[1]);
    let manifest_path = PathBuf::from(&args[2]);
    let manifest_dir = manifest_path.parent().unwrap_or(Path::new("."));
    let manifest = fs::read_to_string(&manifest_path).expect("read rootfs manifest");

    let mut builder = Builder::new();
    builder.ensure_dir("/bin", 0o755, 0, 0);
    builder.ensure_dir("/sbin", 0o755, 0, 0);
    builder.ensure_dir("/etc", 0o755, 0, 0);
    builder.ensure_dir("/home", 0o755, 0, 0);
    builder.ensure_dir("/home/alice", 0o755, 1000, 1000);
    builder.ensure_dir("/root", 0o700, 0, 0);
    builder.ensure_dir("/tmp", 0o777, 0, 0);
    builder.ensure_dir("/dev", 0o755, 0, 0);
    builder.ensure_dir("/proc", 0o755, 0, 0);
    builder.ensure_dir("/initrd", 0o755, 0, 0);

    let mut init_data = None;
    for (line_index, line) in manifest.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts = line.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            ["file", path, source] => {
                let source = manifest_dir.join(source);
                let data = fs::read(&source).unwrap_or_else(|err| {
                    panic!("read rootfs source {}: {}", source.display(), err)
                });
                if *path == "/bin/init" {
                    init_data = Some(data.clone());
                }
                let mode = if path.starts_with("/bin/")
                    || path.starts_with("/sbin/")
                    || path.starts_with("/lib/")
                {
                    0o755
                } else {
                    0o644
                };
                builder.add_file(path, data, mode, 0, 0);
            }
            ["package", ..] => {}
            _ => panic!("invalid rootfs manifest line {}: {}", line_index + 1, line),
        }
    }

    if let Some(data) = init_data {
        builder.add_file("/sbin/init", data, 0o755, 0, 0);
    }
    builder.add_file(
        "/etc/passwd",
        b"root:x:0:0:root:/root:/bin/sh\nalice:x:1000:1000:Alice:/home/alice:/bin/sh\n"
            .to_vec(),
        0o644,
        0,
        0,
    );
    builder.add_file(
        "/etc/group",
        b"root:x:0:\nalice:x:1000:\n".to_vec(),
        0o644,
        0,
        0,
    );
    builder.add_file(
        "/etc/shadow",
        b"root::0:0:99999:7:::\nalice::0:0:99999:7:::\n".to_vec(),
        0o600,
        0,
        0,
    );
    builder.add_file(
        "/home/alice/.profile",
        b"# Ristux user profile\n".to_vec(),
        0o644,
        1000,
        1000,
    );

    builder.finish(&output);
}

fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_bitmap_bit(buf: &mut [u8], index: usize) {
    buf[index / 8] |= 1 << (index % 8);
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

fn parent_path(path: &str) -> String {
    if path == "/" {
        return String::from("/");
    }
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
        .to_owned()
}
