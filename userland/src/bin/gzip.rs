#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;

const FTEXT: u8 = 0x01;
const FHCRC: u8 = 0x02;
const FEXTRA: u8 = 0x04;
const FNAME: u8 = 0x08;
const FCOMMENT: u8 = 0x10;

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct Options<'a> {
    decompress: bool,
    stdout: bool,
    test: bool,
    files: &'a [&'a [u8]],
}

#[derive(Clone, Copy)]
struct HuffEntry {
    code: u16,
    len: u8,
    symbol: usize,
}

struct Huffman {
    entries: Vec<HuffEntry>,
    max_bits: u8,
}

struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize,
    bits: u32,
    bit_count: u8,
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return false;
        }
        bytes = &bytes[n as usize..];
    }
    true
}

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn read_fd(fd: i32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 {
            return Some(out);
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    if path == b"-" {
        return read_fd(0);
    }
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let bytes = read_fd(fd as i32);
    let _ = sys::close(fd as i32);
    bytes
}

fn write_file(path: &[u8], bytes: &[u8]) -> bool {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return false;
    }
    let ok = write_all(fd as i32, bytes);
    let _ = sys::close(fd as i32);
    ok
}

fn usage() {
    let _ = write_all(2, b"usage: gzip [-cdt] [FILE...]\n");
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    bytes.windows(needle.len()).any(|window| window == needle)
}

fn parse_options<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    let mut decompress = args.first().is_some_and(|arg| contains(arg, b"gunzip"));
    let mut stdout = false;
    let mut test = false;
    let mut index = 1usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"--" {
            index += 1;
            break;
        }
        if arg == b"--decompress" || arg == b"--uncompress" {
            decompress = true;
            index += 1;
            continue;
        }
        if arg == b"--stdout" || arg == b"--to-stdout" {
            stdout = true;
            index += 1;
            continue;
        }
        if arg.starts_with(b"-") && arg.len() > 1 {
            for byte in &arg[1..] {
                match *byte {
                    b'd' => decompress = true,
                    b'c' => stdout = true,
                    b't' => {
                        decompress = true;
                        test = true;
                    }
                    b'n' | b'f' | b'k' => {}
                    _ => return None,
                }
            }
            index += 1;
        } else {
            break;
        }
    }
    Some(Options {
        decompress,
        stdout,
        test,
        files: &args[index..],
    })
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn le16(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]))
}

fn le32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ]))
}

fn push_le16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_le32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            bits: 0,
            bit_count: 0,
        }
    }

    fn read_bits(&mut self, count: u8) -> Option<u32> {
        while self.bit_count < count {
            self.bits |= (*self.bytes.get(self.pos)? as u32) << self.bit_count;
            self.pos += 1;
            self.bit_count += 8;
        }
        let mask = if count == 32 {
            u32::MAX
        } else {
            (1u32 << count) - 1
        };
        let value = self.bits & mask;
        self.bits >>= count;
        self.bit_count -= count;
        Some(value)
    }

    fn align_byte(&mut self) {
        self.bits = 0;
        self.bit_count = 0;
    }

    fn read_aligned_byte(&mut self) -> Option<u8> {
        self.align_byte();
        let byte = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }
}

fn reverse_bits(mut value: u16, len: u8) -> u16 {
    let mut out = 0u16;
    for _ in 0..len {
        out = (out << 1) | (value & 1);
        value >>= 1;
    }
    out
}

fn build_huffman(lengths: &[u8]) -> Option<Huffman> {
    let mut counts = [0u16; 16];
    let mut max_bits = 0u8;
    for len in lengths {
        if *len > 15 {
            return None;
        }
        if *len != 0 {
            counts[*len as usize] += 1;
            max_bits = max_bits.max(*len);
        }
    }

    let mut next_code = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..=15 {
        code = (code + counts[bits - 1]) << 1;
        next_code[bits] = code;
    }

    let mut entries = Vec::new();
    for (symbol, len) in lengths.iter().enumerate() {
        if *len == 0 {
            continue;
        }
        let assigned = next_code[*len as usize];
        next_code[*len as usize] = next_code[*len as usize].checked_add(1)?;
        entries.push(HuffEntry {
            code: reverse_bits(assigned, *len),
            len: *len,
            symbol,
        });
    }

    Some(Huffman { entries, max_bits })
}

fn decode_symbol(reader: &mut BitReader, tree: &Huffman) -> Option<usize> {
    let mut code = 0u16;
    for len in 1..=tree.max_bits {
        code |= (reader.read_bits(1)? as u16) << (len - 1);
        for entry in &tree.entries {
            if entry.len == len && entry.code == code {
                return Some(entry.symbol);
            }
        }
    }
    None
}

fn fixed_trees() -> Option<(Huffman, Huffman)> {
    let mut lit_lengths = [0u8; 288];
    for item in lit_lengths.iter_mut().take(144) {
        *item = 8;
    }
    for item in lit_lengths.iter_mut().take(256).skip(144) {
        *item = 9;
    }
    for item in lit_lengths.iter_mut().take(280).skip(256) {
        *item = 7;
    }
    for item in lit_lengths.iter_mut().skip(280) {
        *item = 8;
    }
    let dist_lengths = [5u8; 32];
    Some((build_huffman(&lit_lengths)?, build_huffman(&dist_lengths)?))
}

fn dynamic_trees(reader: &mut BitReader) -> Option<(Huffman, Huffman)> {
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;
    let mut code_lengths = [0u8; 19];
    for slot in CL_ORDER.iter().take(hclen) {
        code_lengths[*slot] = reader.read_bits(3)? as u8;
    }
    let code_tree = build_huffman(&code_lengths)?;

    let total = hlit.checked_add(hdist)?;
    let mut lengths = Vec::new();
    while lengths.len() < total {
        let symbol = decode_symbol(reader, &code_tree)?;
        match symbol {
            0..=15 => lengths.push(symbol as u8),
            16 => {
                let prev = *lengths.last()?;
                let repeat = reader.read_bits(2)? as usize + 3;
                for _ in 0..repeat {
                    lengths.push(prev);
                }
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                for _ in 0..repeat {
                    lengths.push(0);
                }
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                for _ in 0..repeat {
                    lengths.push(0);
                }
            }
            _ => return None,
        }
    }
    if lengths.len() != total {
        return None;
    }

    let lit_tree = build_huffman(&lengths[..hlit])?;
    let dist_tree = build_huffman(&lengths[hlit..])?;
    Some((lit_tree, dist_tree))
}

fn copy_match(out: &mut Vec<u8>, distance: usize, length: usize) -> Option<()> {
    if distance == 0 || distance > out.len() {
        return None;
    }
    for _ in 0..length {
        let index = out.len() - distance;
        let byte = out[index];
        out.push(byte);
    }
    Some(())
}

fn decode_compressed_block(
    reader: &mut BitReader,
    lit_tree: &Huffman,
    dist_tree: &Huffman,
    out: &mut Vec<u8>,
) -> Option<()> {
    loop {
        let symbol = decode_symbol(reader, lit_tree)?;
        match symbol {
            0..=255 => out.push(symbol as u8),
            256 => return Some(()),
            257..=285 => {
                let index = symbol - 257;
                let mut length = *LENGTH_BASE.get(index)?;
                let extra = *LENGTH_EXTRA.get(index)?;
                if extra != 0 {
                    length += reader.read_bits(extra)? as usize;
                }
                let dist_symbol = decode_symbol(reader, dist_tree)?;
                if dist_symbol >= 30 {
                    return None;
                }
                let mut distance = DIST_BASE[dist_symbol];
                let extra = DIST_EXTRA[dist_symbol];
                if extra != 0 {
                    distance += reader.read_bits(extra)? as usize;
                }
                copy_match(out, distance, length)?;
            }
            _ => return None,
        }
    }
}

fn decode_stored_block(reader: &mut BitReader, out: &mut Vec<u8>) -> Option<()> {
    reader.align_byte();
    let len = u16::from_le_bytes([reader.read_aligned_byte()?, reader.read_aligned_byte()?]);
    let nlen = u16::from_le_bytes([reader.read_aligned_byte()?, reader.read_aligned_byte()?]);
    if len != !nlen {
        return None;
    }
    for _ in 0..len {
        out.push(reader.read_aligned_byte()?);
    }
    Some(())
}

fn inflate(bytes: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut reader = BitReader::new(bytes);
    let mut out = Vec::new();
    loop {
        let final_block = reader.read_bits(1)? != 0;
        let block_type = reader.read_bits(2)?;
        match block_type {
            0 => decode_stored_block(&mut reader, &mut out)?,
            1 => {
                let (lit_tree, dist_tree) = fixed_trees()?;
                decode_compressed_block(&mut reader, &lit_tree, &dist_tree, &mut out)?;
            }
            2 => {
                let (lit_tree, dist_tree) = dynamic_trees(&mut reader)?;
                decode_compressed_block(&mut reader, &lit_tree, &dist_tree, &mut out)?;
            }
            _ => return None,
        }
        if final_block {
            return Some((out, reader.pos));
        }
    }
}

fn skip_zero_terminated(bytes: &[u8], mut index: usize) -> Option<usize> {
    while index < bytes.len() {
        if bytes[index] == 0 {
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

fn gzip_header_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 10 || bytes[0] != 0x1f || bytes[1] != 0x8b || bytes[2] != 8 {
        return None;
    }
    let flags = bytes[3];
    if flags & !(FTEXT | FHCRC | FEXTRA | FNAME | FCOMMENT) != 0 {
        return None;
    }
    let mut index = 10usize;
    if flags & FEXTRA != 0 {
        let len = le16(bytes.get(index..index + 2)?)? as usize;
        index = index.checked_add(2)?.checked_add(len)?;
        if index > bytes.len() {
            return None;
        }
    }
    if flags & FNAME != 0 {
        index = skip_zero_terminated(bytes, index)?;
    }
    if flags & FCOMMENT != 0 {
        index = skip_zero_terminated(bytes, index)?;
    }
    if flags & FHCRC != 0 {
        index = index.checked_add(2)?;
        if index > bytes.len() {
            return None;
        }
    }
    Some(index)
}

fn gunzip(bytes: &[u8]) -> Option<Vec<u8>> {
    let start = gzip_header_len(bytes)?;
    let (out, consumed) = inflate(&bytes[start..])?;
    let trailer = start.checked_add(consumed)?;
    let crc = le32(bytes.get(trailer..trailer + 4)?)?;
    let isize = le32(bytes.get(trailer + 4..trailer + 8)?)?;
    if crc32(&out) != crc || out.len() as u32 != isize {
        return None;
    }
    Some(out)
}

fn gzip_store(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3]);

    let mut offset = 0usize;
    while offset < bytes.len() || bytes.is_empty() && offset == 0 {
        let remaining = bytes.len().saturating_sub(offset);
        let chunk = remaining.min(65_535);
        let final_block = offset + chunk >= bytes.len();
        out.push(if final_block { 1 } else { 0 });
        push_le16(&mut out, chunk as u16);
        push_le16(&mut out, !(chunk as u16));
        out.extend_from_slice(&bytes[offset..offset + chunk]);
        offset += chunk;
        if final_block {
            break;
        }
    }

    push_le32(&mut out, crc32(bytes));
    push_le32(&mut out, bytes.len() as u32);
    out
}

fn append_gz(path: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(path.len() + 3);
    out.extend_from_slice(path);
    out.extend_from_slice(b".gz");
    out
}

fn strip_gz(path: &[u8]) -> Option<Vec<u8>> {
    path.strip_suffix(b".gz").map(|base| base.to_vec())
}

fn output_path(path: &[u8], decompress: bool) -> Option<Vec<u8>> {
    if path == b"-" {
        return None;
    }
    if decompress {
        strip_gz(path)
    } else {
        Some(append_gz(path))
    }
}

fn process_bytes(bytes: &[u8], options: &Options) -> Option<Vec<u8>> {
    if options.decompress {
        gunzip(bytes)
    } else {
        Some(gzip_store(bytes))
    }
}

fn process_input(path: Option<&[u8]>, options: &Options) -> i32 {
    let input = match path {
        Some(path) => read_file(path),
        None => read_fd(0),
    };
    let Some(input) = input else {
        let _ = write_all(2, b"gzip: cannot read input\n");
        return 1;
    };
    let Some(output) = process_bytes(&input, options) else {
        let _ = write_all(2, b"gzip: invalid compressed data\n");
        return 1;
    };
    if options.test {
        return 0;
    }
    if options.stdout || path.is_none() {
        return if write_all(1, &output) { 0 } else { 1 };
    }
    let Some(path) = path else {
        return 1;
    };
    let Some(out_path) = output_path(path, options.decompress) else {
        let _ = write_all(2, b"gzip: cannot choose output path\n");
        return 1;
    };
    if write_file(&out_path, &output) {
        0
    } else {
        let _ = write_all(2, b"gzip: cannot write output\n");
        1
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(options) = parse_options(args) else {
        usage();
        return 2;
    };
    if options.files.is_empty() {
        return process_input(None, &options);
    }

    let mut rc = 0;
    for file in options.files {
        let file_rc = process_input(Some(file), &options);
        if file_rc != 0 {
            rc = file_rc;
        }
    }
    rc
}

ristux_userland::program_main!(main);
