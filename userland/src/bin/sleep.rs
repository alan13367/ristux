#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use core::hint::spin_loop;
use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    loop {
        let _ = sys::getpid();
        for _ in 0..4096 {
            spin_loop();
        }
    }
}

ristux_userland::program_main!(main);
