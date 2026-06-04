#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

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

fn has_arg(args: &[&[u8]], needle: &[u8]) -> bool {
    args.iter().any(|arg| *arg == needle)
}

fn main(args: &[&[u8]]) -> i32 {
    if has_arg(args, b"--version") || has_arg(args, b"-V") {
        write_all(1, b"cargo 1.96.0 (ristux official-bootstrap stage0)\n");
        return 0;
    }
    if has_arg(args, b"--help") || has_arg(args, b"-h") {
        write_all(1, b"usage: cargo [--version] build\n");
        write_all(
            1,
            b"Cargo is installed as part of the Rust 1.96.0 toolchain package contract.\n",
        );
        write_all(
            1,
            b"Build execution is pending native rustc code generation and Ristux std support.\n",
        );
        return 0;
    }
    if args
        .iter()
        .any(|arg| *arg == b"build" || *arg == b"check" || *arg == b"run")
    {
        write_all(
            2,
            b"cargo 1.96.0: build execution is not available yet on this Ristux image\n",
        );
        write_all(
            2,
            b"cargo 1.96.0: pending native rustc 1.96.0 code generation and Ristux std support\n",
        );
        return 1;
    }
    write_all(
        2,
        b"cargo 1.96.0: unsupported command in bootstrap stage0\n",
    );
    1
}

ristux_userland::program_main!(main);
