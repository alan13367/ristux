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
        write_all(1, b"rustdoc 1.96.0 (ristux official-bootstrap stage0)\n");
        return 0;
    }
    if args.iter().any(|arg| *arg == b"--help" || *arg == b"-h") {
        write_all(1, b"usage: rustdoc [--version]\n");
        write_all(1, b"Rustdoc is installed as part of the Rust 1.96.0 toolchain package contract.\n");
        return 0;
    }
    write_all(2, b"rustdoc 1.96.0: documentation generation is pending the native rustc host port\n");
    1
}

ristux_userland::program_main!(main);
