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

const XZ_MAGIC: &[u8; 6] = b"\xfd7zXZ\0";
const XZ_FOOTER_MAGIC: &[u8; 2] = b"YZ";
const CHECK_CRC32: u8 = 1;
const FILTER_LZMA2: u64 = 0x21;
const LZMA2_PROPS_8M: u8 = 22;
const MAX_LZMA2_STORED_CHUNK: usize = 65_536;

struct Options<'a> {
    decompress: bool,
    stdout: bool,
    test: bool,
    files: &'a [&'a [u8]],
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

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    bytes.windows(needle.len()).any(|window| window == needle)
}

fn usage() {
    let _ = write_all(2, b"usage: xz [-cdt] [FILE...]\n");
}

fn parse_options<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    let mut decompress = args
        .first()
        .is_some_and(|arg| contains(arg, b"unxz") || contains(arg, b"xzcat"));
    let mut stdout = args.first().is_some_and(|arg| contains(arg, b"xzcat"));
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
                    b'k' | b'f' | b'0'..=b'9' => {}
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

fn push_le32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_le32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn push_vli(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn read_vli(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for _ in 0..9 {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

fn pad4(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn xz_header(out: &mut Vec<u8>) {
    out.extend_from_slice(XZ_MAGIC);
    let flags = [0u8, CHECK_CRC32];
    out.extend_from_slice(&flags);
    push_le32(out, crc32(&flags));
}

fn encode_lzma2_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let mut first = true;
    while offset < data.len() {
        let remaining = data.len() - offset;
        let chunk_len = remaining.min(MAX_LZMA2_STORED_CHUNK);
        let encoded_len = (chunk_len - 1) as u16;
        out.push(if first { 0x01 } else { 0x02 });
        out.push((encoded_len >> 8) as u8);
        out.push(encoded_len as u8);
        out.extend_from_slice(&data[offset..offset + chunk_len]);
        offset += chunk_len;
        first = false;
    }
    out.push(0x00);
    out
}

fn block_header(compressed_size: usize, uncompressed_size: usize) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(0);
    body.push(0xc0);
    push_vli(&mut body, compressed_size as u64);
    push_vli(&mut body, uncompressed_size as u64);
    push_vli(&mut body, FILTER_LZMA2);
    push_vli(&mut body, 1);
    body.push(LZMA2_PROPS_8M);
    while (body.len() + 4) % 4 != 0 {
        body.push(0);
    }
    body[0] = ((body.len() + 4) / 4 - 1) as u8;
    let crc = crc32(&body);
    push_le32(&mut body, crc);
    body
}

fn xz_encode_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    xz_header(&mut out);

    let compressed = encode_lzma2_stored(data);
    let header = block_header(compressed.len(), data.len());
    let unpadded_size = header.len() + compressed.len() + 4;
    out.extend_from_slice(&header);
    out.extend_from_slice(&compressed);
    pad4(&mut out);
    push_le32(&mut out, crc32(data));

    let index_start = out.len();
    out.push(0);
    push_vli(&mut out, 1);
    push_vli(&mut out, unpadded_size as u64);
    push_vli(&mut out, data.len() as u64);
    pad4(&mut out);
    let index_crc = crc32(&out[index_start..]);
    push_le32(&mut out, index_crc);
    let index_size = out.len() - index_start;

    let backward_size = (index_size / 4 - 1) as u32;
    let footer_start = out.len();
    out.extend_from_slice(&[0; 4]);
    push_le32(&mut out, backward_size);
    out.extend_from_slice(&[0, CHECK_CRC32]);
    let footer_crc = crc32(&out[footer_start + 4..footer_start + 10]);
    out[footer_start..footer_start + 4].copy_from_slice(&footer_crc.to_le_bytes());
    out.extend_from_slice(XZ_FOOTER_MAGIC);
    out
}

fn decode_lzma2_stored(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    loop {
        let control = *bytes.get(pos)?;
        pos += 1;
        match control {
            0x00 => {
                if pos == bytes.len() {
                    return Some(out);
                }
                return None;
            }
            0x01 | 0x02 => {
                let high = *bytes.get(pos)? as usize;
                let low = *bytes.get(pos + 1)? as usize;
                pos += 2;
                let len = ((high << 8) | low) + 1;
                let chunk = bytes.get(pos..pos + len)?;
                out.extend_from_slice(chunk);
                pos += len;
            }
            _ => return None,
        }
    }
}

fn xz_decode_stored(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 12 + 12 || &bytes[..6] != XZ_MAGIC {
        return None;
    }
    let stream_flags = bytes.get(6..8)?;
    if stream_flags != [0, CHECK_CRC32] || read_le32(bytes, 8)? != crc32(stream_flags) {
        return None;
    }
    if bytes.get(bytes.len() - 2..)? != XZ_FOOTER_MAGIC {
        return None;
    }
    let footer_flags = bytes.get(bytes.len() - 4..bytes.len() - 2)?;
    if footer_flags != stream_flags {
        return None;
    }
    let footer_crc = read_le32(bytes, bytes.len() - 12)?;
    if footer_crc != crc32(&bytes[bytes.len() - 8..bytes.len() - 2]) {
        return None;
    }
    let backward_size = read_le32(bytes, bytes.len() - 8)? as usize;
    let index_size = (backward_size + 1) * 4;
    if bytes.len() < 12 + index_size + 12 {
        return None;
    }
    let index_start = bytes.len() - 12 - index_size;
    let index_crc_stored = read_le32(bytes, bytes.len() - 16)?;
    if index_crc_stored != crc32(&bytes[index_start..bytes.len() - 16]) {
        return None;
    }

    let mut pos = 12usize;
    let header_words = *bytes.get(pos)? as usize + 1;
    let header_len = header_words * 4;
    let header = bytes.get(pos..pos + header_len)?;
    if read_le32(header, header_len - 4)? != crc32(&header[..header_len - 4]) {
        return None;
    }
    let flags = *header.get(1)?;
    if flags != 0xc0 {
        return None;
    }
    let mut header_pos = 2usize;
    let compressed_size = read_vli(header, &mut header_pos)? as usize;
    let uncompressed_size = read_vli(header, &mut header_pos)? as usize;
    let filter = read_vli(header, &mut header_pos)?;
    let props_size = read_vli(header, &mut header_pos)? as usize;
    let _props = header.get(header_pos..header_pos + props_size)?;
    if filter != FILTER_LZMA2 || props_size != 1 {
        return None;
    }
    pos += header_len;
    let compressed = bytes.get(pos..pos + compressed_size)?;
    let out = decode_lzma2_stored(compressed)?;
    if out.len() != uncompressed_size {
        return None;
    }
    pos += compressed_size;
    while pos % 4 != 0 {
        if *bytes.get(pos)? != 0 {
            return None;
        }
        pos += 1;
    }
    if read_le32(bytes, pos)? != crc32(&out) {
        return None;
    }
    Some(out)
}

fn output_compressed_path(path: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(path.len() + 4);
    out.extend_from_slice(path);
    out.extend_from_slice(b".xz");
    out
}

fn output_decompressed_path(path: &[u8]) -> Vec<u8> {
    if path.ends_with(b".xz") {
        path[..path.len() - 3].to_vec()
    } else {
        let mut out = Vec::with_capacity(path.len() + 4);
        out.extend_from_slice(path);
        out.extend_from_slice(b".out");
        out
    }
}

fn process_file(opts: &Options<'_>, path: &[u8]) -> bool {
    let input = match read_file(path) {
        Some(bytes) => bytes,
        None => {
            let _ = write_all(2, b"xz: cannot read ");
            let _ = write_all(2, path);
            let _ = write_all(2, b"\n");
            return false;
        }
    };
    let output = if opts.decompress {
        match xz_decode_stored(&input) {
            Some(bytes) => bytes,
            None => {
                let _ = write_all(2, b"xz: unsupported or corrupt stream\n");
                return false;
            }
        }
    } else {
        xz_encode_stored(&input)
    };
    if opts.test {
        return true;
    }
    if opts.stdout || path == b"-" {
        write_all(1, &output)
    } else {
        let out_path = if opts.decompress {
            output_decompressed_path(path)
        } else {
            output_compressed_path(path)
        };
        write_file(&out_path, &output)
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let opts = match parse_options(args) {
        Some(opts) => opts,
        None => {
            usage();
            return 2;
        }
    };
    let files = if opts.files.is_empty() {
        &[b"-".as_ref()][..]
    } else {
        opts.files
    };
    let mut ok = true;
    for path in files {
        ok &= process_file(&opts, path);
    }
    if ok { 0 } else { 1 }
}

ristux_userland::program_main!(main);
