#![no_std]
#![no_main]

extern crate ristux_userland;
extern crate alloc;

use ristux_userland::sys;

fn main(args: &[&[u8]]) -> i32 {
    if args.iter().any(|arg| *arg == b"--version" || *arg == b"-v") {
        let _ = sys::write(1, b"ristux-ld 0.0.0-bootstrap\n");
        return 0;
    }
    let _ = sys::write(2, b"ristux-ld: static ELF linker bootstrap pending\n");
    1
}

ristux_userland::program_main!(main);
