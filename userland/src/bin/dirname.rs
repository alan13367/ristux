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

fn trim_trailing_slashes(path: &[u8]) -> &[u8] {
    let mut end = path.len();
    while end > 1 && path[end - 1] == b'/' {
        end -= 1;
    }
    &path[..end]
}

fn dirname(path: &[u8]) -> &[u8] {
    if path.is_empty() {
        return b".";
    }
    let trimmed = trim_trailing_slashes(path);
    if trimmed.iter().all(|byte| *byte == b'/') {
        return b"/";
    }

    let Some(last_slash) = trimmed.iter().rposition(|byte| *byte == b'/') else {
        return b".";
    };
    if last_slash == 0 {
        return b"/";
    }

    let mut end = last_slash;
    while end > 1 && trimmed[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 { b"/" } else { &trimmed[..end] }
}

fn usage() {
    let _ = write_all(2, b"usage: dirname NAME\n");
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() != 2 {
        usage();
        return 2;
    }
    let out = dirname(args[1]);
    if write_all(1, out) && write_all(1, b"\n") {
        0
    } else {
        1
    }
}

ristux_userland::program_main!(main);
