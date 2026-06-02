#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

struct Metadata {
    size: i64,
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

fn stat(path: &[u8]) -> Option<Metadata> {
    let path_c = cstr(path);
    let mut stat_buf = [0u8; 144];
    let rc = unsafe {
        sys::syscall2(
            sys::NR_STAT,
            path_c.as_ptr() as usize,
            stat_buf.as_mut_ptr() as usize,
        )
    };
    if rc < 0 {
        return None;
    }
    let size = i64::from_le_bytes([
        stat_buf[48],
        stat_buf[49],
        stat_buf[50],
        stat_buf[51],
        stat_buf[52],
        stat_buf[53],
        stat_buf[54],
        stat_buf[55],
    ]);
    Some(Metadata { size })
}

fn is_dir(path: &[u8]) -> bool {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let mut buf = [0u8; 128];
    let rc = sys::getdents64(fd as i32, &mut buf);
    let _ = sys::close(fd as i32);
    rc >= 0
}

fn parse_int(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }
    let mut index = 0usize;
    let mut negative = false;
    if bytes[0] == b'-' {
        negative = true;
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

fn eval_unary(op: &[u8], value: &[u8]) -> Option<bool> {
    match op {
        b"-n" => Some(!value.is_empty()),
        b"-z" => Some(value.is_empty()),
        b"-e" => Some(stat(value).is_some()),
        b"-f" => Some(stat(value).is_some() && !is_dir(value)),
        b"-d" => Some(is_dir(value)),
        b"-s" => Some(stat(value).is_some_and(|meta| meta.size > 0)),
        _ => None,
    }
}

fn eval_binary(left: &[u8], op: &[u8], right: &[u8]) -> Option<bool> {
    match op {
        b"=" | b"==" => Some(left == right),
        b"!=" => Some(left != right),
        b"-eq" | b"-ne" | b"-gt" | b"-ge" | b"-lt" | b"-le" => {
            let left = parse_int(left)?;
            let right = parse_int(right)?;
            match op {
                b"-eq" => Some(left == right),
                b"-ne" => Some(left != right),
                b"-gt" => Some(left > right),
                b"-ge" => Some(left >= right),
                b"-lt" => Some(left < right),
                b"-le" => Some(left <= right),
                _ => None,
            }
        }
        _ => None,
    }
}

fn eval(args: &[&[u8]]) -> Option<bool> {
    match args.len() {
        0 => Some(false),
        1 => Some(!args[0].is_empty()),
        2 if args[0] == b"!" => eval(&args[1..]).map(|value| !value),
        2 => eval_unary(args[0], args[1]),
        3 => eval_binary(args[0], args[1], args[2]),
        _ => None,
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let mut expr = &args[1..];
    if args.first().is_some_and(|arg| *arg == b"[") {
        if expr.last().copied() != Some(b"]".as_slice()) {
            let _ = write_all(2, b"[: missing ]\n");
            return 2;
        }
        expr = &expr[..expr.len() - 1];
    }
    match eval(expr) {
        Some(true) => 0,
        Some(false) => 1,
        None => {
            let _ = write_all(2, b"test: unsupported expression\n");
            2
        }
    }
}

ristux_userland::program_main!(main);
