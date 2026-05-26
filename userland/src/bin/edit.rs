#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::string::ToString;
use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;

fn cstr(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 1);
    out.extend_from_slice(s);
    out.push(0);
    out
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

fn read_line(fd: i32) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 || (n == 0 && line.is_empty()) {
            return None;
        }
        if n == 0 {
            break;
        }
        let chunk = &buf[..n as usize];
        if let Some(pos) = chunk.iter().position(|&b| b == b'\n') {
            line.extend_from_slice(&chunk[..pos]);
            break;
        }
        line.extend_from_slice(chunk);
    }
    while line.last() == Some(&b'\r') {
        line.pop();
    }
    Some(line)
}

fn load_file(path: &[u8]) -> Vec<Vec<u8>> {
    let fd = sys::open(cstr(path).as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(1, b"edit: new file\n");
        return Vec::new();
    }

    let mut data = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = sys::read(fd as i32, &mut buf);
        if n <= 0 {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
    }
    let _ = sys::close(fd as i32);

    let mut lines = Vec::new();
    let mut line = Vec::new();
    for byte in data {
        if byte == b'\n' {
            lines.push(core::mem::take(&mut line));
        } else {
            line.push(byte);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn save_file(path: &[u8], lines: &[Vec<u8>]) -> bool {
    let fd = sys::open(cstr(path).as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return false;
    }
    for line in lines {
        if !write_all(fd as i32, line) || !write_all(fd as i32, b"\n") {
            let _ = sys::close(fd as i32);
            return false;
        }
    }
    let _ = sys::close(fd as i32);
    true
}

fn parse_number(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut seen = false;
    for &byte in bytes {
        if byte == b' ' || byte == b'\t' {
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value
            .saturating_mul(10)
            .saturating_add((byte - b'0') as usize);
        seen = true;
    }
    if seen {
        Some(value)
    } else {
        None
    }
}

fn print_lines(lines: &[Vec<u8>]) {
    for (index, line) in lines.iter().enumerate() {
        let _ = write_all(1, (index + 1).to_string().as_bytes());
        let _ = write_all(1, b"\t");
        let _ = write_all(1, line);
        let _ = write_all(1, b"\n");
    }
}

fn append_mode(lines: &mut Vec<Vec<u8>>, insert_at: Option<usize>) {
    let _ = write_all(1, b"edit: enter text, . alone to finish\n");
    let mut offset = 0usize;
    while let Some(line) = read_line(0) {
        if line.as_slice() == b"." {
            break;
        }
        if let Some(index) = insert_at {
            lines.insert((index + offset).min(lines.len()), line);
            offset += 1;
        } else {
            lines.push(line);
        }
    }
    let _ = write_all(1, b"edit: appended\n");
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() < 2 {
        let _ = write_all(2, b"usage: edit FILE\n");
        return 1;
    }
    let path = args[1];
    let _ = write_all(1, b"edit: ");
    let _ = write_all(1, path);
    let _ = write_all(1, b"\n");

    let mut lines = load_file(path);
    let mut dirty = false;
    loop {
        let _ = write_all(1, b": ");
        let Some(command) = read_line(0) else {
            break;
        };
        match command.as_slice() {
            b"a" => {
                append_mode(&mut lines, None);
                dirty = true;
            }
            b"p" => print_lines(&lines),
            b"w" => {
                if save_file(path, &lines) {
                    dirty = false;
                    let _ = write_all(1, b"edit: wrote ");
                    let _ = write_all(1, lines.len().to_string().as_bytes());
                    let _ = write_all(1, b" line(s)\n");
                } else {
                    let _ = write_all(2, b"edit: write failed\n");
                }
            }
            b"q" => {
                if dirty {
                    let _ = write_all(1, b"edit: unsaved changes, use w then q\n");
                } else {
                    let _ = write_all(1, b"edit: done\n");
                    return 0;
                }
            }
            b"h" => {
                let _ = write_all(
                    1,
                    b"a append, i N insert, d N delete, p print, w write, q quit\n",
                );
            }
            _ if command.starts_with(b"i ") => {
                let index = parse_number(&command[2..]).unwrap_or(1).saturating_sub(1);
                append_mode(&mut lines, Some(index));
                dirty = true;
            }
            _ if command.starts_with(b"d ") => {
                if let Some(index) = parse_number(&command[2..]) {
                    if index > 0 && index <= lines.len() {
                        lines.remove(index - 1);
                        dirty = true;
                        let _ = write_all(1, b"edit: deleted\n");
                    } else {
                        let _ = write_all(2, b"edit: no such line\n");
                    }
                } else {
                    let _ = write_all(2, b"edit: bad line number\n");
                }
            }
            _ => {
                let _ = write_all(2, b"edit: unknown command\n");
            }
        }
    }
    0
}

ristux_userland::program_main!(main);
