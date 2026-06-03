#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    let _ = sys::write(1, b"ssh_banner: no SSH daemon installed in pure-Rust profile\n");
    0
}

ristux_userland::program_main!(main);
