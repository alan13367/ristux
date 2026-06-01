#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

fn parse_target(arg: &[u8]) -> Option<([u8; 4], &[u8])> {
    let rest = arg.strip_prefix(b"http://").unwrap_or(arg);
    let slash = rest
        .iter()
        .position(|byte| *byte == b'/')
        .unwrap_or(rest.len());
    let host = &rest[..slash];
    let path = if slash < rest.len() {
        &rest[slash..]
    } else {
        b"/"
    };
    Some((parse_ipv4(host)?, path))
}

fn parse_ipv4(host: &[u8]) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut part = 0usize;
    let mut value = 0u16;
    let mut saw_digit = false;
    for &byte in host {
        if byte == b'.' {
            if !saw_digit || part >= 4 || value > 255 {
                return None;
            }
            out[part] = value as u8;
            part += 1;
            value = 0;
            saw_digit = false;
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u16::from(byte - b'0');
        saw_digit = true;
    }
    if !saw_digit || part != 3 || value > 255 {
        return None;
    }
    out[part] = value as u8;
    Some(out)
}

fn main(args: &[&[u8]]) -> i32 {
    let target = args.get(1).copied().unwrap_or(b"http://10.0.2.2/");
    let Some((ip, path)) = parse_target(target) else {
        let _ = sys::write(2, b"curl_lite: bad URL\n");
        return 1;
    };

    let fd = sys::socket(sys::AF_INET, sys::SOCK_STREAM, 0);
    if fd < 0 {
        let _ = sys::write(2, b"curl_lite: socket failed\n");
        return 1;
    }
    let addr = sys::SockAddrIn::new(ip, 80);
    if sys::connect(fd as i32, &addr) < 0 {
        let _ = sys::write(2, b"curl_lite: connect failed\n");
        return 1;
    }

    let mut request = Vec::new();
    request.extend_from_slice(b"GET ");
    request.extend_from_slice(path);
    request.extend_from_slice(b" HTTP/1.0\r\nHost: ");
    request.extend_from_slice(target);
    request.extend_from_slice(b"\r\n\r\n");
    if sys::sendto(fd as i32, &request, 0) < 0 {
        let _ = sys::write(2, b"curl_lite: send failed\n");
        return 1;
    }

    let mut buffer = [0u8; 256];
    let n = sys::recvfrom(fd as i32, &mut buffer, 0);
    if n < 0 {
        let _ = sys::write(2, b"curl_lite: recv failed\n");
        return 1;
    }
    let _ = sys::write(1, &buffer[..n as usize]);
    0
}

ristux_userland::program_main!(main);
