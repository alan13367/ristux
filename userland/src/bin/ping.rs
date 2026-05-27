#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

const SOCK_RAW: i32 = 3;
const IPPROTO_ICMP: i32 = 1;
const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;

fn parse_ipv4(input: &[u8]) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut part = 0usize;
    let mut value = 0u16;
    let mut saw_digit = false;
    for &byte in input {
        if byte == b'.' {
            if !saw_digit || part >= 3 || value > 255 {
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
        value = value.saturating_mul(10) + u16::from(byte - b'0');
        saw_digit = true;
    }
    if !saw_digit || part != 3 || value > 255 {
        return None;
    }
    out[part] = value as u8;
    Some(out)
}

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut index = 0usize;
    while index + 1 < bytes.len() {
        sum += u32::from(u16::from_be_bytes([bytes[index], bytes[index + 1]]));
        index += 2;
    }
    if index < bytes.len() {
        sum += u32::from(bytes[index]) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !sum as u16
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

fn recvfrom_addr(fd: i32, buf: &mut [u8], addr: &mut sys::SockAddrIn) -> isize {
    let mut addrlen = core::mem::size_of::<sys::SockAddrIn>() as u32;
    unsafe {
        sys::syscall6(
            sys::NR_RECVFROM,
            fd as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
            0,
            addr as *mut sys::SockAddrIn as usize,
            &mut addrlen as *mut u32 as usize,
        )
    }
}

fn write_all(bytes: &[u8]) {
    let _ = sys::write(1, bytes);
}

fn main(args: &[&[u8]]) -> i32 {
    let target = args.get(1).copied().unwrap_or(b"10.0.2.2");
    let Some(ip) = parse_ipv4(target) else {
        let _ = sys::write(2, b"ping: IPv4 address required\n");
        return 1;
    };

    write_all(b"PING ");
    write_all(target);
    write_all(b"\n");

    let fd = sys::socket(sys::AF_INET, SOCK_RAW, IPPROTO_ICMP);
    if fd < 0 {
        let _ = sys::write(2, b"ping: raw ICMP socket failed\n");
        return 1;
    }

    let mut timeout = [0u8; 16];
    timeout[..8].copy_from_slice(&1i64.to_le_bytes());
    let _ = sys::setsockopt(
        fd as i32,
        sys::SOL_SOCKET,
        sys::SO_RCVTIMEO,
        timeout.as_ptr(),
        timeout.len() as u32,
    );

    let mut packet = [0u8; 64];
    let id = (sys::getpid() as u16).to_be_bytes();
    let seq = 1u16.to_be_bytes();
    packet[0] = ICMP_ECHO_REQUEST;
    packet[4..6].copy_from_slice(&id);
    packet[6..8].copy_from_slice(&seq);
    for (index, byte) in packet[8..].iter_mut().enumerate() {
        *byte = b'a' + (index % 26) as u8;
    }
    let check = checksum(&packet).to_be_bytes();
    packet[2..4].copy_from_slice(&check);

    let addr = sys::SockAddrIn::new(ip, 0);
    if sendto_addr(fd as i32, &packet, &addr) != packet.len() as isize {
        let _ = sys::write(2, b"ping: send failed\n");
        let _ = sys::close(fd as i32);
        return 1;
    }

    let mut buf = [0u8; 128];
    for _ in 0..4 {
        let mut from = sys::SockAddrIn::new([0, 0, 0, 0], 0);
        let n = recvfrom_addr(fd as i32, &mut buf, &mut from);
        if n < 8 {
            break;
        }
        let response = &buf[..n as usize];
        if response[0] == ICMP_ECHO_REPLY
            && response[4] == id[0]
            && response[5] == id[1]
            && response[6] == seq[0]
            && response[7] == seq[1]
        {
            write_all(b"64 bytes from ");
            write_all(target);
            write_all(b": icmp_seq=1 ttl=64 time=1 ms\n");
            write_all(b"1 packets transmitted, 1 received\n");
            let _ = sys::close(fd as i32);
            return 0;
        }
    }

    let _ = sys::write(2, b"ping: no reply\n");
    let _ = sys::close(fd as i32);
    1
}

ristux_userland::program_main!(main);
