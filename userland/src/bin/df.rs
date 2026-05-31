#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = sys::read(fd as i32, &mut buf);
        if n < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn for_lines(bytes: &[u8], mut f: impl FnMut(&[u8])) {
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            f(&bytes[start..index]);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        f(&bytes[start..]);
    }
}

fn next_field<'a>(line: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    while *cursor < line.len() && line[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
    if *cursor >= line.len() {
        return None;
    }
    let start = *cursor;
    while *cursor < line.len() && !line[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
    Some(&line[start..*cursor])
}

fn default_targets() -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut targets = Vec::new();
    if let Some(bytes) = read_file(b"/proc/mounts") {
        for_lines(&bytes, |line| {
            let mut cursor = 0usize;
            let Some(source) = next_field(line, &mut cursor) else {
                return;
            };
            let Some(mountpoint) = next_field(line, &mut cursor) else {
                return;
            };
            targets.push((source.to_vec(), mountpoint.to_vec()));
        });
    }
    if targets.is_empty() {
        targets.push((b"ext2".to_vec(), b"/".to_vec()));
    }
    targets
}

fn statfs(path: &[u8]) -> Option<sys::StatFs> {
    let path_c = cstr(path);
    let mut stats = sys::StatFs::default();
    if sys::statfs(path_c.as_ptr(), &mut stats as *mut sys::StatFs) < 0 {
        None
    } else {
        Some(stats)
    }
}

fn blocks_1k(value: u64, block_size: u64) -> u64 {
    value.saturating_mul(block_size).saturating_add(1023) / 1024
}

fn push_u64(out: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
        if value == 0 {
            break;
        }
    }
    while len > 0 {
        len -= 1;
        out.push(digits[len]);
    }
}

fn push_padded_u64(out: &mut Vec<u8>, value: u64, width: usize) {
    let start = out.len();
    push_u64(out, value);
    let len = out.len() - start;
    if len < width {
        let pad = width - len;
        out.resize(out.len() + pad, b' ');
        out.copy_within(start..start + len, start + pad);
        for byte in &mut out[start..start + pad] {
            *byte = b' ';
        }
    }
}

fn push_padded_bytes(out: &mut Vec<u8>, bytes: &[u8], width: usize) {
    out.extend_from_slice(bytes);
    if bytes.len() < width {
        out.resize(out.len() + width - bytes.len(), b' ');
    }
}

fn append_row(out: &mut Vec<u8>, source: &[u8], mountpoint: &[u8], stats: sys::StatFs) {
    let block_size = if stats.f_frsize != 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let total = blocks_1k(stats.f_blocks, block_size);
    let free = blocks_1k(stats.f_bavail, block_size);
    let used = blocks_1k(stats.f_blocks.saturating_sub(stats.f_bfree), block_size);
    let pct = if total == 0 {
        0
    } else {
        used.saturating_mul(100).saturating_add(total - 1) / total
    };

    push_padded_bytes(out, source, 12);
    push_padded_u64(out, total, 11);
    push_padded_u64(out, used, 11);
    push_padded_u64(out, free, 12);
    out.push(b' ');
    push_padded_u64(out, pct, 3);
    out.push(b'%');
    out.push(b' ');
    out.extend_from_slice(mountpoint);
    out.push(b'\n');
}

fn main(args: &[&[u8]]) -> i32 {
    let targets = if args.len() == 1 {
        default_targets()
    } else {
        args[1..]
            .iter()
            .map(|arg| ((*arg).to_vec(), (*arg).to_vec()))
            .collect()
    };

    let mut out = Vec::new();
    out.extend_from_slice(b"Filesystem    1K-blocks       Used   Available Use% Mounted on\n");
    let mut status = 0;
    for (source, mountpoint) in targets {
        if let Some(stats) = statfs(&mountpoint) {
            append_row(&mut out, &source, &mountpoint, stats);
        } else {
            let _ = write_all(2, b"df: cannot stat ");
            let _ = write_all(2, &mountpoint);
            let _ = write_all(2, b"\n");
            status = 1;
        }
    }
    if !write_all(1, &out) {
        return 1;
    }
    status
}

ristux_userland::program_main!(main);
