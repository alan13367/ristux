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

fn parse_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0usize;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
}

fn head_fd(fd: i32, max_lines: usize) -> i32 {
    if max_lines == 0 {
        return 0;
    }
    let mut lines = 0usize;
    let mut buf = [0u8; 256];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return 1;
        }
        if n == 0 {
            return 0;
        }
        let mut end = n as usize;
        for (index, byte) in buf[..n as usize].iter().enumerate() {
            if *byte == b'\n' {
                lines += 1;
                if lines == max_lines {
                    end = index + 1;
                    break;
                }
            }
        }
        if !write_all(1, &buf[..end]) {
            return 1;
        }
        if lines >= max_lines {
            return 0;
        }
    }
}

fn head_file(path: &[u8], max_lines: usize) -> i32 {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"head: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return 1;
    }
    let rc = head_fd(fd as i32, max_lines);
    let _ = sys::close(fd as i32);
    rc
}

fn usage() {
    let _ = write_all(2, b"usage: head [-n LINES] [FILE...]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut lines = 10usize;
    let mut index = 1usize;
    if let Some(arg) = args.get(index) {
        if *arg == b"-n" {
            if let Some(value) = args.get(index + 1).and_then(|arg| parse_usize(arg)) {
                lines = value;
                index += 2;
            } else {
                usage();
                return 2;
            }
        } else if arg.starts_with(b"-") && arg.len() > 1 {
            if let Some(value) = parse_usize(&arg[1..]) {
                lines = value;
                index += 1;
            } else {
                usage();
                return 2;
            }
        }
    }

    if index >= args.len() {
        return head_fd(0, lines);
    }

    let mut rc = 0;
    for path in &args[index..] {
        let file_rc = head_file(path, lines);
        if file_rc != 0 {
            rc = file_rc;
        }
    }
    rc
}

ristux_userland::program_main!(main);
