#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;

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

fn write_usize(fd: i32, mut value: usize) -> bool {
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    if value == 0 {
        index -= 1;
        buf[index] = b'0';
    } else {
        while value > 0 {
            index -= 1;
            buf[index] = b'0' + (value % 10) as u8;
            value /= 10;
        }
    }
    write_all(fd, &buf[index..])
}

fn emit_line(fd: i32, line: &[u8], count: usize, show_count: bool) -> bool {
    if show_count && (!write_usize(fd, count) || !write_all(fd, b" ")) {
        return false;
    }
    write_all(fd, line) && write_all(fd, b"\n")
}

fn open_input(path: Option<&[u8]>) -> Result<(i32, bool), ()> {
    let Some(path) = path else {
        return Ok((0, false));
    };
    if path == b"-" {
        return Ok((0, false));
    }

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"uniq: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        Err(())
    } else {
        Ok((fd as i32, true))
    }
}

fn open_output(path: Option<&[u8]>) -> Result<(i32, bool), ()> {
    let Some(path) = path else {
        return Ok((1, false));
    };

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        let _ = write_all(2, b"uniq: cannot open output ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        Err(())
    } else {
        Ok((fd as i32, true))
    }
}

fn usage() {
    let _ = write_all(2, b"usage: uniq [-c] [INPUT [OUTPUT]]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut show_count = false;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-c" {
            show_count = true;
            index += 1;
            continue;
        }
        if arg.starts_with(b"-") && *arg != b"-" {
            usage();
            return 2;
        }
        break;
    }

    if args.len() - index > 2 {
        usage();
        return 2;
    }

    let (input, close_input) = match open_input(args.get(index).copied()) {
        Ok(fd) => fd,
        Err(()) => return 1,
    };
    let (output, close_output) = match open_output(args.get(index + 1).copied()) {
        Ok(fd) => fd,
        Err(()) => {
            if close_input {
                let _ = sys::close(input);
            }
            return 1;
        }
    };

    let bytes = match read_all(input) {
        Ok(bytes) => bytes,
        Err(()) => {
            if close_input {
                let _ = sys::close(input);
            }
            if close_output {
                let _ = sys::close(output);
            }
            return 1;
        }
    };
    if close_input {
        let _ = sys::close(input);
    }

    let mut lines = Vec::new();
    push_lines(&bytes, &mut lines);

    let mut current: Option<Vec<u8>> = None;
    let mut count = 0usize;
    for line in lines {
        if current.as_ref().is_some_and(|prev| prev == &line) {
            count += 1;
            continue;
        }
        if let Some(prev) = current.take() {
            if !emit_line(output, &prev, count, show_count) {
                return 1;
            }
        }
        current = Some(line);
        count = 1;
    }
    if let Some(prev) = current {
        if !emit_line(output, &prev, count, show_count) {
            return 1;
        }
    }

    if close_output {
        let _ = sys::close(output);
    }
    0
}

ristux_userland::program_main!(main);
