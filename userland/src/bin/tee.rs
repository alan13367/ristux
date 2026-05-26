#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const O_APPEND: i32 = 0o2000;

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

fn open_output(path: &[u8], append: bool) -> i32 {
    let path_c = cstr(path);
    let flags = O_WRONLY | O_CREAT | if append { O_APPEND } else { O_TRUNC };
    let fd = sys::open(path_c.as_ptr(), flags, 0o644);
    if fd < 0 {
        let _ = write_all(2, b"tee: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
    }
    fd as i32
}

fn usage() {
    let _ = write_all(2, b"usage: tee [-a] [FILE...]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut append = false;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"-a" {
            append = true;
            index += 1;
        } else if *arg == b"--" {
            index += 1;
            break;
        } else if arg.starts_with(b"-") && arg.len() > 1 {
            usage();
            return 2;
        } else {
            break;
        }
    }

    let mut status = 0;
    let mut outputs = Vec::new();
    for path in &args[index..] {
        let fd = open_output(path, append);
        if fd < 0 {
            status = 1;
        } else {
            outputs.push(fd);
        }
    }

    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(0, &mut buf);
        if n < 0 {
            status = 1;
            break;
        }
        if n == 0 {
            break;
        }

        let bytes = &buf[..n as usize];
        if !write_all(1, bytes) {
            status = 1;
        }
        for fd in &outputs {
            if !write_all(*fd, bytes) {
                status = 1;
            }
        }
    }

    for fd in outputs {
        let _ = sys::close(fd);
    }
    status
}

ristux_userland::program_main!(main);
