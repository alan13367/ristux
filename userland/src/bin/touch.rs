#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;

fn main(args: &[&[u8]]) -> i32 {
    if args.len() <= 1 {
        let _ = sys::write(2, b"touch: missing operand\n");
        return 1;
    }

    let mut rc = 0;
    for arg in args.iter().skip(1) {
        let mut path: Vec<u8> = Vec::with_capacity(arg.len() + 1);
        path.extend_from_slice(arg);
        path.push(0);

        let fd = sys::open(path.as_ptr(), O_WRONLY | O_CREAT, 0o644);
        if fd < 0 {
            let _ = sys::write(2, b"touch: cannot touch ");
            let _ = sys::write(2, arg);
            let _ = sys::write(2, b"\n");
            rc = 1;
            continue;
        }
        let _ = sys::close(fd as i32);
    }

    rc
}

ristux_userland::program_main!(main);
