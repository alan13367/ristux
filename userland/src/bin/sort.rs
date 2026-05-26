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

fn push_lines(bytes: &[u8], lines: &mut Vec<Vec<u8>>) {
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(bytes[start..index].to_vec());
            start = index + 1;
        }
    }
    if start < bytes.len() {
        lines.push(bytes[start..].to_vec());
    }
}

fn read_lines_fd(fd: i32, lines: &mut Vec<Vec<u8>>) -> i32 {
    let mut pending = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return 1;
        }
        if n == 0 {
            if !pending.is_empty() {
                lines.push(pending);
            }
            return 0;
        }

        pending.extend_from_slice(&buf[..n as usize]);
        let split_after = pending.iter().rposition(|byte| *byte == b'\n');
        if let Some(last_newline) = split_after {
            push_lines(&pending[..=last_newline], lines);
            let rest = pending[last_newline + 1..].to_vec();
            pending = rest;
        }
    }
}

fn read_lines_file(path: &[u8], lines: &mut Vec<Vec<u8>>) -> i32 {
    if path == b"-" {
        return read_lines_fd(0, lines);
    }

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"sort: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return 1;
    }
    let rc = read_lines_fd(fd as i32, lines);
    let _ = sys::close(fd as i32);
    rc
}

fn usage() {
    let _ = write_all(2, b"usage: sort [-r] [-u] [FILE...]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut reverse = false;
    let mut unique = false;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if !arg.starts_with(b"-") || *arg == b"-" {
            break;
        }
        for option in &arg[1..] {
            match *option {
                b'r' => reverse = true,
                b'u' => unique = true,
                _ => {
                    usage();
                    return 2;
                }
            }
        }
        index += 1;
    }

    let mut lines: Vec<Vec<u8>> = Vec::new();
    let mut rc = 0;
    if index >= args.len() {
        rc = read_lines_fd(0, &mut lines);
    } else {
        for path in &args[index..] {
            let file_rc = read_lines_file(path, &mut lines);
            if file_rc != 0 {
                rc = file_rc;
            }
        }
    }

    lines.sort_unstable();
    if unique {
        lines.dedup();
    }
    if reverse {
        lines.reverse();
    }

    for line in lines {
        if !write_all(1, &line) || !write_all(1, b"\n") {
            return 1;
        }
    }
    rc
}

ristux_userland::program_main!(main);
