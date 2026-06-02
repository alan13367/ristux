#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

struct MemInfo {
    total_kb: u64,
    free_kb: u64,
    heap_used: u64,
    heap_free: u64,
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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let mut path_c = Vec::with_capacity(path.len() + 1);
    path_c.extend_from_slice(path);
    path_c.push(0);
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

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u64)?;
    }
    Some(value)
}

fn parse_line(line: &[u8], label: &[u8]) -> Option<u64> {
    let rest = line.strip_prefix(label)?;
    let rest = rest.strip_prefix(b":")?;
    let start = rest.iter().position(|byte| byte.is_ascii_digit())?;
    let digits = &rest[start..];
    let end = digits
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(digits.len());
    parse_u64(&digits[..end])
}

fn parse_meminfo(bytes: &[u8]) -> Option<MemInfo> {
    let mut info = MemInfo {
        total_kb: 0,
        free_kb: 0,
        heap_used: 0,
        heap_free: 0,
    };
    for_lines(bytes, |line| {
        if let Some(value) = parse_line(line, b"MemTotal") {
            info.total_kb = value;
        } else if let Some(value) = parse_line(line, b"MemFree") {
            info.free_kb = value;
        } else if let Some(value) = parse_line(line, b"HeapUsed") {
            info.heap_used = value;
        } else if let Some(value) = parse_line(line, b"HeapFree") {
            info.heap_free = value;
        }
    });
    if info.total_kb > 0 { Some(info) } else { None }
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

fn push_padded(out: &mut Vec<u8>, value: u64, width: usize) {
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

fn main(args: &[&[u8]]) -> i32 {
    if args.len() != 1 {
        let _ = write_all(2, b"usage: free\n");
        return 2;
    }
    let Some(bytes) = read_file(b"/proc/meminfo") else {
        let _ = write_all(2, b"free: cannot read /proc/meminfo\n");
        return 1;
    };
    let Some(info) = parse_meminfo(&bytes) else {
        let _ = write_all(2, b"free: invalid /proc/meminfo\n");
        return 1;
    };
    let used_kb = info.total_kb.saturating_sub(info.free_kb);
    let mut out = Vec::new();
    out.extend_from_slice(b"              total        used        free\n");
    out.extend_from_slice(b"Mem:   ");
    push_padded(&mut out, info.total_kb, 12);
    push_padded(&mut out, used_kb, 12);
    push_padded(&mut out, info.free_kb, 12);
    out.extend_from_slice(b"\nHeap:  ");
    push_padded(&mut out, info.heap_used + info.heap_free, 12);
    push_padded(&mut out, info.heap_used, 12);
    push_padded(&mut out, info.heap_free, 12);
    out.push(b'\n');
    if write_all(1, &out) { 0 } else { 1 }
}

ristux_userland::program_main!(main);
