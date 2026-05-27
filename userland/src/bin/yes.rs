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

fn build_line(args: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    if args.len() <= 1 {
        out.extend_from_slice(b"y");
    } else {
        for (index, arg) in args[1..].iter().enumerate() {
            if index > 0 {
                out.push(b' ');
            }
            out.extend_from_slice(arg);
        }
    }
    out.push(b'\n');
    out
}

fn main(args: &[&[u8]]) -> i32 {
    let line = build_line(args);
    loop {
        if !write_all(1, &line) {
            return 1;
        }
    }
}

ristux_userland::program_main!(main);
