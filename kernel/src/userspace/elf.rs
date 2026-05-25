use alloc::vec::Vec;
use core::fmt;

const PT_LOAD: u32 = 1;

pub struct LoadedElf {
    pub entry: u64,
    segments: Vec<LoadedSegment>,
}

pub struct LoadedSegment {
    pub vaddr: usize,
    pub flags: u32,
    pub bytes: Vec<u8>,
}

pub struct SegmentView<'a> {
    pub vaddr: usize,
    pub mem_size: usize,
    pub file_bytes: &'a [u8],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    Unsupported,
    OutOfBounds,
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall => f.write_str("ELF file is too small"),
            Self::BadMagic => f.write_str("bad ELF magic"),
            Self::Unsupported => f.write_str("unsupported ELF format"),
            Self::OutOfBounds => f.write_str("ELF table points outside file"),
        }
    }
}

impl LoadedElf {
    pub fn parse(data: &[u8]) -> Result<Self, ElfError> {
        if data.len() < 64 {
            return Err(ElfError::TooSmall);
        }

        if data.get(0..4) != Some(b"\x7fELF") {
            return Err(ElfError::BadMagic);
        }

        if data[4] != 2 || data[5] != 1 || read_u16(data, 18)? != 0x3e {
            return Err(ElfError::Unsupported);
        }

        let entry = read_u64(data, 24)?;
        let phoff = read_u64(data, 32)? as usize;
        let phentsize = read_u16(data, 54)? as usize;
        let phnum = read_u16(data, 56)? as usize;
        if phentsize < 56 {
            return Err(ElfError::Unsupported);
        }

        let mut segments = Vec::new();
        for index in 0..phnum {
            let offset = phoff
                .checked_add(index * phentsize)
                .ok_or(ElfError::OutOfBounds)?;
            let end = offset.checked_add(56).ok_or(ElfError::OutOfBounds)?;
            if end > data.len() {
                return Err(ElfError::OutOfBounds);
            }

            let typ = read_u32(data, offset)?;
            if typ != PT_LOAD {
                continue;
            }

            let flags = read_u32(data, offset + 4)?;
            let file_offset = read_u64(data, offset + 8)? as usize;
            let vaddr = read_u64(data, offset + 16)? as usize;
            let filesz = read_u64(data, offset + 32)? as usize;
            let memsz = read_u64(data, offset + 40)? as usize;
            let file_end = file_offset.checked_add(filesz).ok_or(ElfError::OutOfBounds)?;
            if file_end > data.len() || filesz > memsz {
                return Err(ElfError::OutOfBounds);
            }

            let mut bytes = Vec::new();
            bytes.extend_from_slice(&data[file_offset..file_end]);
            bytes.resize(memsz, 0);
            segments.push(LoadedSegment {
                vaddr,
                flags,
                bytes,
            });
        }

        if segments.is_empty() {
            return Err(ElfError::Unsupported);
        }

        crate::println!(
            "ELF loader: entry {:#x}, {} loadable segment(s)",
            entry,
            segments.len()
        );

        Ok(Self { entry, segments })
    }

    pub fn read_memory(&self, addr: usize, len: usize) -> Option<&[u8]> {
        let end = addr.checked_add(len)?;
        for segment in &self.segments {
            let start = segment.vaddr;
            let segment_end = start.checked_add(segment.bytes.len())?;
            if addr >= start && end <= segment_end {
                let offset = addr - start;
                return Some(&segment.bytes[offset..offset + len]);
            }
        }
        None
    }

    pub fn find_bytes(&self, needle: &[u8]) -> Option<usize> {
        for segment in &self.segments {
            if segment.flags & 0x4 == 0 {
                continue;
            }

            for (offset, window) in segment.bytes.windows(needle.len()).enumerate() {
                if window == needle {
                    return Some(segment.vaddr + offset);
                }
            }
        }

        None
    }
}

pub fn for_each_load_segment(
    data: &[u8],
    mut f: impl FnMut(SegmentView<'_>),
) -> Result<u64, ElfError> {
    let header = parse_header(data)?;

    for index in 0..header.phnum {
        let offset = header
            .phoff
            .checked_add(index * header.phentsize)
            .ok_or(ElfError::OutOfBounds)?;
        let end = offset.checked_add(56).ok_or(ElfError::OutOfBounds)?;
        if end > data.len() {
            return Err(ElfError::OutOfBounds);
        }

        let typ = read_u32(data, offset)?;
        if typ != PT_LOAD {
            continue;
        }

        let _flags = read_u32(data, offset + 4)?;
        let file_offset = read_u64(data, offset + 8)? as usize;
        let vaddr = read_u64(data, offset + 16)? as usize;
        let filesz = read_u64(data, offset + 32)? as usize;
        let memsz = read_u64(data, offset + 40)? as usize;
        let file_end = file_offset.checked_add(filesz).ok_or(ElfError::OutOfBounds)?;
        if file_end > data.len() || filesz > memsz {
            return Err(ElfError::OutOfBounds);
        }

        f(SegmentView {
            vaddr,
            mem_size: memsz,
            file_bytes: &data[file_offset..file_end],
        });
    }

    Ok(header.entry)
}

struct ElfHeader {
    entry: u64,
    phoff: usize,
    phentsize: usize,
    phnum: usize,
}

fn parse_header(data: &[u8]) -> Result<ElfHeader, ElfError> {
    if data.len() < 64 {
        return Err(ElfError::TooSmall);
    }

    if data.get(0..4) != Some(b"\x7fELF") {
        return Err(ElfError::BadMagic);
    }

    if data[4] != 2 || data[5] != 1 || read_u16(data, 18)? != 0x3e {
        return Err(ElfError::Unsupported);
    }

    let entry = read_u64(data, 24)?;
    let phoff = read_u64(data, 32)? as usize;
    let phentsize = read_u16(data, 54)? as usize;
    let phnum = read_u16(data, 56)? as usize;
    if phentsize < 56 {
        return Err(ElfError::Unsupported);
    }

    Ok(ElfHeader {
        entry,
        phoff,
        phentsize,
        phnum,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, ElfError> {
    let bytes = data.get(offset..offset + 2).ok_or(ElfError::OutOfBounds)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, ElfError> {
    let bytes = data.get(offset..offset + 4).ok_or(ElfError::OutOfBounds)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, ElfError> {
    let bytes = data.get(offset..offset + 8).ok_or(ElfError::OutOfBounds)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}
