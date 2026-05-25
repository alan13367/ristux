#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

fn main(args: &[&[u8]]) -> i32 {
    let mut first = true;
    for (idx, arg) in args.iter().enumerate() {
        if idx == 0 {
            continue;
        }
        if !first {
            let _ = sys::write(1, b" ");
        }
        let _ = sys::write(1, arg);
        first = false;
    }
    let _ = sys::write(1, b"\n");
    0
}

ristux_userland::program_main!(main);
