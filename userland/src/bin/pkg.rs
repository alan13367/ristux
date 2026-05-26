#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let written = sys::write(fd, bytes);
        if written <= 0 {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), 0, 0);
    if fd < 0 {
        return None;
    }

    let mut out = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let read = sys::read(fd as i32, &mut buf);
        if read < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn read_db_file(name: &[u8], file: &[u8]) -> Option<Vec<u8>> {
    if !safe_package_name(name) {
        return None;
    }
    let mut path = Vec::new();
    path.extend_from_slice(b"/pkg/db/");
    path.extend_from_slice(name);
    path.push(b'/');
    path.extend_from_slice(file);
    read_file(&path)
}

fn safe_package_name(name: &[u8]) -> bool {
    !name.is_empty()
        && !name.starts_with(b".")
        && name.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || *byte == b'-'
                || *byte == b'_'
                || *byte == b'.'
                || *byte == b'+'
        })
}

fn split_fields(line: &[u8]) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = None;
    for (index, byte) in line.iter().enumerate() {
        if byte.is_ascii_whitespace() {
            if let Some(field_start) = start.take() {
                fields.push(&line[field_start..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(field_start) = start {
        fields.push(&line[field_start..]);
    }
    fields
}

fn for_lines(bytes: &[u8], mut f: impl FnMut(&[u8])) {
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            let mut line = &bytes[start..index];
            if line.ends_with(b"\r") {
                line = &line[..line.len() - 1];
            }
            f(line);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        f(&bytes[start..]);
    }
}

fn print_line(bytes: &[u8]) {
    let _ = write_all(1, bytes);
    let _ = write_all(1, b"\n");
}

fn print_usage() {
    let _ = write_all(2, b"usage: pkg list | pkg info NAME | pkg files NAME | pkg deps NAME | pkg hook NAME\n");
}

fn list_packages() -> i32 {
    let Some(index) = read_file(b"/pkg/packages.txt") else {
        let _ = write_all(2, b"pkg: cannot read package index\n");
        return 1;
    };
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for_lines(&index, |line| {
        if line.is_empty() || line.starts_with(b"#") {
            return;
        }
        let fields = split_fields(line);
        if fields.len() < 2 || seen.iter().any(|name| name.as_slice() == fields[0]) {
            return;
        }
        seen.push(fields[0].to_vec());
        let _ = write_all(1, fields[0]);
        let _ = write_all(1, b" ");
        let _ = write_all(1, fields[1]);
        let _ = write_all(1, b"\n");
    });
    0
}

fn print_indented_lines(bytes: &[u8]) {
    for_lines(bytes, |line| {
        if line.is_empty() {
            return;
        }
        let _ = write_all(1, b"  ");
        print_line(line);
    });
}

fn first_line(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|byte| *byte == b'\n').unwrap_or(bytes.len());
    let line = &bytes[..end];
    if line.ends_with(b"\r") {
        &line[..line.len() - 1]
    } else {
        line
    }
}

fn info_package(name: &[u8]) -> i32 {
    let Some(version) = read_db_file(name, b"version") else {
        let _ = write_all(2, b"pkg: unknown package ");
        let _ = write_all(2, name);
        let _ = write_all(2, b"\n");
        return 1;
    };
    let files = read_db_file(name, b"files").unwrap_or_default();
    let deps = read_db_file(name, b"dependencies").unwrap_or_default();
    let hook = read_db_file(name, b"post-install").unwrap_or_default();

    let _ = write_all(1, b"name: ");
    print_line(name);
    let _ = write_all(1, b"version: ");
    print_line(first_line(&version));
    let _ = write_all(1, b"files:\n");
    print_indented_lines(&files);
    let _ = write_all(1, b"dependencies:\n");
    print_indented_lines(&deps);
    let _ = write_all(1, b"post-install:\n");
    print_indented_lines(&hook);
    0
}

fn print_db_file(name: &[u8], file: &[u8]) -> i32 {
    let Some(bytes) = read_db_file(name, file) else {
        let _ = write_all(2, b"pkg: unknown package ");
        let _ = write_all(2, name);
        let _ = write_all(2, b"\n");
        return 1;
    };
    let _ = write_all(1, &bytes);
    0
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() == 2 && args[1] == b"list" {
        return list_packages();
    }
    if args.len() == 3 && args[1] == b"info" {
        return info_package(args[2]);
    }
    if args.len() == 3 && args[1] == b"files" {
        return print_db_file(args[2], b"files");
    }
    if args.len() == 3 && args[1] == b"deps" {
        return print_db_file(args[2], b"dependencies");
    }
    if args.len() == 3 && args[1] == b"hook" {
        return print_db_file(args[2], b"post-install");
    }
    print_usage();
    2
}

ristux_userland::program_main!(main);
