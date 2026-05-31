#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use core::ptr;
use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    let addr = sys::SockAddrIn::new([127, 0, 0, 1], 18181);

    let listener = sys::socket(sys::AF_INET, sys::SOCK_STREAM, 0);
    if listener < 0 {
        let _ = sys::write(2, b"loopback_check: listener socket failed\n");
        return 1;
    }
    if sys::bind(listener as i32, &addr) < 0 || sys::listen(listener as i32, 1) < 0 {
        let _ = sys::write(2, b"loopback_check: listen failed\n");
        return 1;
    }

    let client = sys::socket(sys::AF_INET, sys::SOCK_STREAM, 0);
    if client < 0 {
        let _ = sys::write(2, b"loopback_check: client socket failed\n");
        return 1;
    }
    if sys::connect(client as i32, &addr) < 0 {
        let _ = sys::write(2, b"loopback_check: connect failed\n");
        return 1;
    }

    let server = sys::accept(listener as i32, ptr::null_mut(), ptr::null_mut());
    if server < 0 {
        let _ = sys::write(2, b"loopback_check: accept failed\n");
        return 1;
    }

    if sys::sendto(client as i32, b"loop-ping", 0) != 9 {
        let _ = sys::write(2, b"loopback_check: client send failed\n");
        return 1;
    }
    let mut buffer = [0u8; 16];
    let read = sys::recvfrom(server as i32, &mut buffer, 0);
    if read != 9 || &buffer[..9] != b"loop-ping" {
        let _ = sys::write(2, b"loopback_check: server recv failed\n");
        return 1;
    }
    let _ = sys::write(1, b"loopback_check: server received\n");

    if sys::sendto(server as i32, b"loop-pong", 0) != 9 {
        let _ = sys::write(2, b"loopback_check: server send failed\n");
        return 1;
    }
    let read = sys::recvfrom(client as i32, &mut buffer, 0);
    if read != 9 || &buffer[..9] != b"loop-pong" {
        let _ = sys::write(2, b"loopback_check: client recv failed\n");
        return 1;
    }
    let _ = sys::write(1, b"loopback_check: client received\n");
    let _ = sys::close(server as i32);
    let _ = sys::close(client as i32);
    let _ = sys::close(listener as i32);
    let _ = sys::write(1, b"loopback_check: done\n");
    0
}

ristux_userland::program_main!(main);
