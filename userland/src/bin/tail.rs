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

fn read_all(fd: i32) -> Result<Vec<u8>, ()> {
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return Err(());
        }
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}

fn tail_start(bytes: &[u8], max_lines: usize) -> usize {
    if max_lines == 0 {
        return bytes.len();
    }
    if bytes.is_empty() {
        return 0;
    }

    let mut index = bytes.len();
    if bytes[index - 1] == b'\n' {
        index -= 1;
    }

    let mut lines = 0usize;
    while index > 0 {
        index -= 1;
        if bytes[index] == b'\n' {
            lines += 1;
            if lines == max_lines {
                return index + 1;
            }
        }
    }
    0
}

fn tail_fd(fd: i32, max_lines: usize) -> i32 {
    let bytes = match read_all(fd) {
        Ok(bytes) => bytes,
        Err(()) => return 1,
    };
    let start = tail_start(&bytes, max_lines);
    if write_all(1, &bytes[start..]) { 0 } else { 1 }
}

fn tail_file(path: &[u8], max_lines: usize) -> i32 {
    if path == b"-" {
        return tail_fd(0, max_lines);
    }

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"tail: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return 1;
    }
    let rc = tail_fd(fd as i32, max_lines);
    let _ = sys::close(fd as i32);
    rc
}

fn write_header(path: &[u8], first: bool) -> bool {
    if !first && !write_all(1, b"\n") {
        return false;
    }
    let name = if path == b"-" {
        b"standard input"
    } else {
        path
    };
    write_all(1, b"==> ") && write_all(1, name) && write_all(1, b" <==\n")
}

fn usage() {
    let _ = write_all(2, b"usage: tail [-n LINES] [FILE...]\n");
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
        return tail_fd(0, lines);
    }

    let multiple = args.len() - index > 1;
    let mut rc = 0;
    let mut first = true;
    for path in &args[index..] {
        if multiple && !write_header(path, first) {
            return 1;
        }
        first = false;
        let file_rc = tail_file(path, lines);
        if file_rc != 0 {
            rc = file_rc;
        }
    }
    rc
}

ristux_userland::program_main!(main);
