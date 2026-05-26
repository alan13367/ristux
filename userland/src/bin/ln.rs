#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const NR_LINK: usize = 86;
const NR_SYMLINK: usize = 88;

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

fn link_path(old: &[u8], new: &[u8], symbolic: bool) -> bool {
    let old_c = cstr(old);
    let new_c = cstr(new);
    let nr = if symbolic { NR_SYMLINK } else { NR_LINK };
    unsafe { sys::syscall2(nr, old_c.as_ptr() as usize, new_c.as_ptr() as usize) >= 0 }
}

fn main(args: &[&[u8]]) -> i32 {
    let mut symbolic = false;
    let mut index = 1usize;
    if args.get(index).copied() == Some(b"-s".as_slice()) {
        symbolic = true;
        index += 1;
    }
    if args.len() - index != 2 {
        let _ = write_all(2, b"usage: ln [-s] TARGET LINK_NAME\n");
        return 2;
    }
    if link_path(args[index], args[index + 1], symbolic) {
        0
    } else {
        let _ = write_all(2, b"ln: failed\n");
        1
    }
}

ristux_userland::program_main!(main);
