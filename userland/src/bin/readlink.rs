#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const NR_READLINK: usize = 89;

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

fn main(args: &[&[u8]]) -> i32 {
    if args.len() != 2 {
        let _ = write_all(2, b"usage: readlink PATH\n");
        return 2;
    }
    let path = cstr(args[1]);
    let mut buf = [0u8; 512];
    let n = unsafe {
        sys::syscall3(
            NR_READLINK,
            path.as_ptr() as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
        )
    };
    if n < 0 {
        let _ = write_all(2, b"readlink: failed\n");
        return 1;
    }
    let _ = write_all(1, &buf[..n as usize]);
    let _ = write_all(1, b"\n");
    0
}

ristux_userland::program_main!(main);
