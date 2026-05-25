use alloc::vec::Vec;
use core::str;

use crate::multiboot::BootInfo;

const MAGIC: &[u8; 8] = b"RITRD001";

pub struct Initrd {
    files: Vec<InitrdFile>,
}

#[derive(Clone, Copy)]
pub struct InitrdFile {
    pub path: &'static str,
    pub data: &'static [u8],
}

impl Initrd {
    pub fn from_boot_info(boot_info: &BootInfo) -> Result<Self, &'static str> {
        let module = boot_info
            .modules()
            .find(|module| module.command_line.contains("initrd"))
            .ok_or("initrd module missing")?;
        Self::parse(module.bytes())
    }

    pub fn parse(bytes: &'static [u8]) -> Result<Self, &'static str> {
        if bytes.len() < 16 || &bytes[..8] != MAGIC {
            return Err("invalid initrd magic");
        }

        let count = read_u32(bytes, 8)? as usize;
        let mut offset = 16;
        let mut files = Vec::new();

        for _ in 0..count {
            let path_len = read_u16(bytes, offset)? as usize;
            let data_len = read_u32(bytes, offset + 4)? as usize;
            offset += 8;

            let path_end = offset.checked_add(path_len).ok_or("initrd path overflow")?;
            let path = bytes
                .get(offset..path_end)
                .ok_or("initrd path out of bounds")?;
            let path = str::from_utf8(path).map_err(|_| "initrd path is not utf-8")?;
            offset = align_up(path_end, 8);

            let data_end = offset.checked_add(data_len).ok_or("initrd data overflow")?;
            let data = bytes
                .get(offset..data_end)
                .ok_or("initrd data out of bounds")?;
            offset = align_up(data_end, 8);

            files.push(InitrdFile { path, data });
        }

        Ok(Self { files })
    }

    #[allow(dead_code)]
    pub fn files(&self) -> &[InitrdFile] {
        &self.files
    }

    pub fn print_summary(&self) {
        crate::println!("Initrd contains {} file(s):", self.files.len());
        for file in &self.files {
            crate::println!("  {} ({} bytes)", file.path, file.data.len());
        }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let data = bytes
        .get(offset..offset + 2)
        .ok_or("initrd u16 out of bounds")?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let data = bytes
        .get(offset..offset + 4)
        .ok_or("initrd u32 out of bounds")?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
