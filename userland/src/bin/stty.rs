#![no_std]
#![no_main]

extern crate ristux_userland;
extern crate alloc;

use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    let _ = sys::write(1, b"speed 38400 baud; rows 25; columns 80; isig icanon echo\n");
    0
}

ristux_userland::program_main!(main);
