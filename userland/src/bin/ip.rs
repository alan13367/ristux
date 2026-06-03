#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let mut path_nul = Vec::with_capacity(path.len() + 1);
    path_nul.extend_from_slice(path);
    path_nul.push(0);

    let fd = sys::open(path_nul.as_ptr(), 0, 0); // O_RDONLY is 0
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = sys::read(fd as i32, &mut buf);
        if n < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn main(args: &[&[u8]]) -> i32 {
    let show_help = args.get(1).map(|&arg| arg == b"-h" || arg == b"--help").unwrap_or(false);
    if show_help {
        let _ = write_all(1, b"Usage: ip [a | addr]\n");
        return 0;
    }

    let Some(content) = read_file(b"/proc/netinfo") else {
        let _ = write_all(2, b"ip: cannot read /proc/netinfo (make sure procfs is mounted)\n");
        return 1;
    };

    let _ = write_all(1, &content);
    0
}

ristux_userland::program_main!(main);
