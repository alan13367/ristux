#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

struct Input {
    name: Vec<u8>,
    bytes: Vec<u8>,
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

fn push_decimal(out: &mut Vec<u8>, mut value: usize) {
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

fn read_input(path: &[u8]) -> Option<Input> {
    if path == b"-" {
        return Some(Input {
            name: b"-".to_vec(),
            bytes: read_fd(0)?,
        });
    }

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let bytes = read_fd(fd as i32);
    let _ = sys::close(fd as i32);
    Some(Input {
        name: path.to_vec(),
        bytes: bytes?,
    })
}

fn line_at(bytes: &[u8], end: usize) -> usize {
    let mut line = 1usize;
    for byte in &bytes[..end] {
        if *byte == b'\n' {
            line += 1;
        }
    }
    line
}

fn first_difference(left: &[u8], right: &[u8]) -> Option<(usize, usize)> {
    let min_len = core::cmp::min(left.len(), right.len());
    for index in 0..min_len {
        if left[index] != right[index] {
            return Some((index + 1, line_at(left, index)));
        }
    }
    if left.len() != right.len() {
        Some((min_len + 1, line_at(left, min_len)))
    } else {
        None
    }
}

fn print_difference(left: &Input, right: &Input, byte: usize, line: usize) -> bool {
    let mut out = Vec::new();
    out.extend_from_slice(&left.name);
    out.push(b' ');
    out.extend_from_slice(&right.name);
    out.extend_from_slice(b" differ: byte ");
    push_decimal(&mut out, byte);
    out.extend_from_slice(b", line ");
    push_decimal(&mut out, line);
    out.push(b'\n');
    write_all(1, &out)
}

fn print_eof(input: &Input) -> bool {
    let mut out = Vec::new();
    out.extend_from_slice(b"cmp: EOF on ");
    out.extend_from_slice(&input.name);
    out.push(b'\n');
    write_all(1, &out)
}

fn usage() {
    let _ = write_all(2, b"usage: cmp [-s] FILE1 FILE2\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut silent = false;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-s" {
            silent = true;
            index += 1;
            continue;
        }
        if arg.starts_with(b"-") && *arg != b"-" {
            usage();
            return 2;
        }
        break;
    }

    if args.len().saturating_sub(index) != 2 {
        usage();
        return 2;
    }

    let Some(left) = read_input(args[index]) else {
        let _ = write_all(2, b"cmp: cannot read ");
        let _ = write_all(2, args[index]);
        let _ = write_all(2, b"\n");
        return 2;
    };
    let Some(right) = read_input(args[index + 1]) else {
        let _ = write_all(2, b"cmp: cannot read ");
        let _ = write_all(2, args[index + 1]);
        let _ = write_all(2, b"\n");
        return 2;
    };

    let Some((byte, line)) = first_difference(&left.bytes, &right.bytes) else {
        return 0;
    };

    if !silent {
        let shorter = if left.bytes.len() < right.bytes.len() {
            Some(&left)
        } else if right.bytes.len() < left.bytes.len() {
            Some(&right)
        } else {
            None
        };
        if let Some(input) = shorter {
            if byte > input.bytes.len() && !print_eof(input) {
                return 2;
            }
        }
        if !print_difference(&left, &right, byte, line) {
            return 2;
        }
    }
    1
}

ristux_userland::program_main!(main);
