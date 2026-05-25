#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    let _ = sys::write(1, b"ext2 on / type ext2 (rw)\n");
    let _ = sys::write(1, b"tmpfs on /tmp type tmpfs (rw)\n");
    let _ = sys::write(1, b"devfs on /dev type devfs (rw)\n");
    let _ = sys::write(1, b"procfs on /proc type procfs (ro)\n");
    let _ = sys::write(1, b"initrd on /initrd type initrd (ro)\n");
    0
}

ristux_userland::program_main!(main);
