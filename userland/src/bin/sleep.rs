#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    loop {
        let _ = sys::sched_yield();
    }
}

ristux_userland::program_main!(main);
