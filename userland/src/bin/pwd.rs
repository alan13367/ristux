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
    let _ = write_all(2, b"usage: pwd [-L|-P]\n");
}

fn parse_options(args: &[&[u8]]) -> bool {
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-L" || *arg == b"-P" {
            index += 1;
            continue;
        }
        return false;
    }
    index == args.len()
}

fn current_dir() -> Option<Vec<u8>> {
    let mut size = 128usize;
    while size <= 4096 {
        let mut buf = Vec::new();
        buf.resize(size, 0);
        let rc = sys::getcwd(buf.as_mut_ptr(), buf.len());
        if rc >= 0 {
            let len = buf
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(rc as usize);
            buf.truncate(len);
            return Some(buf);
        }
        size *= 2;
    }
    None
}

fn main(args: &[&[u8]]) -> i32 {
    if !parse_options(args) {
        usage();
        return 2;
    }
    let Some(path) = current_dir() else {
        let _ = write_all(2, b"pwd: failed\n");
        return 1;
    };
    if write_all(1, &path) && write_all(1, b"\n") {
        0
    } else {
        1
    }
}

ristux_userland::program_main!(main);
