#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

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

fn usage() {
    let _ = write_all(2, b"usage: seq [-s SEP] [FIRST [INCREMENT]] LAST\n");
}

fn parse_i64(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }
    let mut index = 0usize;
    let mut negative = false;
    if bytes[0] == b'-' {
        negative = true;
        index = 1;
    } else if bytes[0] == b'+' {
        index = 1;
    }
    if index == bytes.len() {
        return None;
    }

    let mut value = 0i64;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as i64)?;
        index += 1;
    }
    if negative { Some(-value) } else { Some(value) }
}

fn push_i64(out: &mut Vec<u8>, mut value: i64) {
    if value < 0 {
        out.push(b'-');
        value = -value;
    }
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

fn print_sequence(first: i64, increment: i64, last: i64, separator: &[u8]) -> i32 {
    if increment == 0 {
        let _ = write_all(2, b"seq: zero increment\n");
        return 2;
    }

    let mut value = first;
    let mut output = Vec::new();
    let mut printed = false;
    while if increment > 0 {
        value <= last
    } else {
        value >= last
    } {
        if printed {
            output.extend_from_slice(separator);
        }
        push_i64(&mut output, value);
        printed = true;
        let Some(next) = value.checked_add(increment) else {
            break;
        };
        value = next;
    }
    output.push(b'\n');
    if write_all(1, &output) { 0 } else { 1 }
}

fn main(args: &[&[u8]]) -> i32 {
    let mut separator = b"\n".as_slice();
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-s" {
            let Some(value) = args.get(index + 1) else {
                usage();
                return 2;
            };
            separator = value;
            index += 2;
            continue;
        }
        if arg.starts_with(b"-") && parse_i64(arg).is_none() {
            usage();
            return 2;
        }
        break;
    }

    let values = &args[index..];
    let (first, increment, last) = match values.len() {
        1 => {
            let Some(last) = parse_i64(values[0]) else {
                usage();
                return 2;
            };
            (1, 1, last)
        }
        2 => {
            let Some(first) = parse_i64(values[0]) else {
                usage();
                return 2;
            };
            let Some(last) = parse_i64(values[1]) else {
                usage();
                return 2;
            };
            (first, 1, last)
        }
        3 => {
            let Some(first) = parse_i64(values[0]) else {
                usage();
                return 2;
            };
            let Some(increment) = parse_i64(values[1]) else {
                usage();
                return 2;
            };
            let Some(last) = parse_i64(values[2]) else {
                usage();
                return 2;
            };
            (first, increment, last)
        }
        _ => {
            usage();
            return 2;
        }
    };

    print_sequence(first, increment, last, separator)
}

ristux_userland::program_main!(main);
