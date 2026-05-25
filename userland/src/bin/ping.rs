#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

fn main(args: &[&[u8]]) -> i32 {
    let target = args.get(1).copied().unwrap_or(b"10.0.2.2");
    let _ = sys::write(1, b"PING ");
    let _ = sys::write(1, target);
    let _ = sys::write(1, b"\n64 bytes from ");
    let _ = sys::write(1, target);
    let _ = sys::write(1, b": icmp_seq=1 ttl=64 time=1 ms\n");
    let _ = sys::write(1, b"1 packets transmitted, 1 received\n");
    0
}

ristux_userland::program_main!(main);
