#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::{io, sys};

fn main(_args: &[&[u8]]) -> i32 {
    io::writeln(1, "ansi_demo: start");
    let _ = sys::write(
        1,
        b"\x1b[2J\x1b[Hansi_demo: clear-home\n\
          \x1b[31mansi_demo: red\x1b[0m\n\
          \x1b[2;8Hansi_demo: moved\n\
          \x1b[?1049hansi_demo: alt-screen\n\x1b[?1049l",
    );
    io::writeln(1, "ansi_demo: done");
    0
}

ristux_userland::program_main!(main);
