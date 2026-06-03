#![no_std]
#![no_main]

extern crate ristux_userland;
extern crate alloc;

use ristux_userland::sys;

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
}

fn main(args: &[&[u8]]) -> i32 {
    if args.iter().any(|arg| *arg == b"--version" || *arg == b"-V") {
        write_all(1, b"rustc 0.0.0-ristux-bootstrap (pure-rust package scaffold)\n");
        return 0;
    }
    if args
        .windows(2)
        .any(|pair| pair[0] == b"--print" && pair[1] == b"target-list")
    {
        write_all(1, b"x86_64-unknown-ristux\n");
        return 0;
    }
    if args.iter().any(|arg| *arg == b"--help" || *arg == b"-h") {
        write_all(1, b"usage: rustc [--version] [--print target-list]\n");
        write_all(1, b"native code generation is pending the Cranelift/std bootstrap\n");
        return 0;
    }
    write_all(2, b"rustc: native code generation pending Cranelift/std bootstrap\n");
    1
}

ristux_userland::program_main!(main);
