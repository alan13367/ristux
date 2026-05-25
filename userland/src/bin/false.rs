#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

fn main(_args: &[&[u8]]) -> i32 {
    1
}

ristux_userland::program_main!(main);
