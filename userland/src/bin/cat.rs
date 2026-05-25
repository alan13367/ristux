#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

fn copy_fd(fd: i32) -> i32 {
    let mut buf = [0u8; 256];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return 1;
        }
        if n == 0 {
            return 0;
        }
        let mut remaining = &buf[..n as usize];
        while !remaining.is_empty() {
            let w = sys::write(1, remaining);
            if w <= 0 {
                return 1;
            }
            remaining = &remaining[w as usize..];
        }
    }
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() <= 1 {
        return copy_fd(0);
    }
    let mut rc = 0;
    for arg in args.iter().skip(1) {
        let mut path: Vec<u8> = Vec::with_capacity(arg.len() + 1);
        path.extend_from_slice(arg);
        path.push(0);
        let fd = sys::open(path.as_ptr(), 0, 0);
        if fd < 0 {
            let _ = sys::write(2, b"cat: cannot open ");
            let _ = sys::write(2, arg);
            let _ = sys::write(2, b"\n");
            rc = 1;
            continue;
        }
        let r = copy_fd(fd as i32);
        if r != 0 {
            rc = r;
        }
        let _ = sys::close(fd as i32);
    }
    rc
}

ristux_userland::program_main!(main);
