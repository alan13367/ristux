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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let mut path_c = Vec::with_capacity(path.len() + 1);
    path_c.extend_from_slice(path);
    path_c.push(0);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 128];
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

fn first_field(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[..end]
}

fn parse_seconds(field: &[u8]) -> Option<u64> {
    let whole = field.split(|byte| *byte == b'.').next()?;
    if whole.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for byte in whole {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u64)?;
    }
    Some(value)
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

fn main(args: &[&[u8]]) -> i32 {
    if args.len() != 1 {
        let _ = write_all(2, b"usage: uptime\n");
        return 2;
    }
    let Some(bytes) = read_file(b"/proc/uptime") else {
        let _ = write_all(2, b"uptime: cannot read /proc/uptime\n");
        return 1;
    };
    let Some(seconds) = parse_seconds(first_field(&bytes)) else {
        let _ = write_all(2, b"uptime: invalid /proc/uptime\n");
        return 1;
    };
    let mut out = Vec::new();
    out.extend_from_slice(b"up ");
    push_u64(&mut out, seconds);
    out.extend_from_slice(b" seconds\n");
    if write_all(1, &out) {
        0
    } else {
        1
    }
}

ristux_userland::program_main!(main);
