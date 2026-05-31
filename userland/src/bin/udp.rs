#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return false;
        }
        bytes = &bytes[n as usize..];
    }
    true
}

fn parse_u16(bytes: &[u8]) -> Option<u16> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
        if value > u16::MAX as u32 {
            return None;
        }
    }
    Some(value as u16)
}

fn parse_ipv4(bytes: &[u8]) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut part = 0usize;
    let mut value = 0u16;
    let mut digits = 0usize;
    for byte in bytes {
        match *byte {
            b'0'..=b'9' => {
                value = value.checked_mul(10)?.checked_add((byte - b'0') as u16)?;
                if value > 255 {
                    return None;
                }
                digits += 1;
            }
            b'.' => {
                if digits == 0 || part >= 3 {
                    return None;
                }
                out[part] = value as u8;
                part += 1;
                value = 0;
                digits = 0;
            }
            _ => return None,
        }
    }
    if digits == 0 || part != 3 {
        return None;
    }
    out[part] = value as u8;
    Some(out)
}

fn usage() {
    let _ = write_all(2, b"usage: udp [HOST PORT MESSAGE [LOCAL_PORT]]\n");
}

fn sendto_addr(fd: i32, packet: &[u8], addr: &sys::SockAddrIn) -> isize {
    unsafe {
        sys::syscall6(
            sys::NR_SENDTO,
            fd as usize,
            packet.as_ptr() as usize,
            packet.len(),
            0,
            addr as *const sys::SockAddrIn as usize,
            core::mem::size_of::<sys::SockAddrIn>(),
        )
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let (host, port, payload, local_port) = match args.len() {
        1 => ([10, 0, 2, 2], 9001, b"ristux".as_slice(), 9000),
        4 | 5 => {
            let Some(host) = parse_ipv4(args[1]) else {
                usage();
                return 2;
            };
            let Some(port) = parse_u16(args[2]) else {
                usage();
                return 2;
            };
            let local_port = if args.len() == 5 {
                let Some(local_port) = parse_u16(args[4]) else {
                    usage();
                    return 2;
                };
                local_port
            } else {
                9000
            };
            (host, port, args[3], local_port)
        }
        _ => {
            usage();
            return 2;
        }
    };

    let fd = sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0);
    if fd < 0 {
        let _ = write_all(2, b"udp: socket failed\n");
        return 1;
    }
    let local = sys::SockAddrIn::new([0, 0, 0, 0], local_port);
    if sys::bind(fd as i32, &local) < 0 {
        let _ = write_all(2, b"udp: bind failed\n");
        let _ = sys::close(fd as i32);
        return 1;
    }
    let remote = sys::SockAddrIn::new(host, port);
    if sendto_addr(fd as i32, payload, &remote) != payload.len() as isize {
        let _ = write_all(2, b"udp: send failed\n");
        let _ = sys::close(fd as i32);
        return 1;
    }

    let mut pollfd = sys::PollFd {
        fd: fd as i32,
        events: sys::POLLIN,
        revents: 0,
    };
    if sys::poll(&mut pollfd as *mut sys::PollFd, 1, 1000) <= 0 {
        let _ = write_all(2, b"udp: receive timed out\n");
        let _ = sys::close(fd as i32);
        return 1;
    }
    let mut buffer = [0u8; 512];
    let nread = sys::recvfrom(fd as i32, &mut buffer, 0);
    let _ = sys::close(fd as i32);
    if nread <= 0 {
        let _ = write_all(2, b"udp: receive failed\n");
        return 1;
    }
    let _ = write_all(1, b"udp recv: ");
    let _ = write_all(1, &buffer[..nread as usize]);
    let _ = write_all(1, b"\n");
    0
}

ristux_userland::program_main!(main);
