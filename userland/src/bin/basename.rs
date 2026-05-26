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

fn basename<'a>(path: &'a [u8], suffix: Option<&[u8]>) -> &'a [u8] {
    if path.is_empty() {
        return b".";
    }
    let trimmed = trim_trailing_slashes(path);
    if trimmed.iter().all(|byte| *byte == b'/') {
        return b"/";
    }

    let start = trimmed
        .iter()
        .rposition(|byte| *byte == b'/')
        .map(|index| index + 1)
        .unwrap_or(0);
    let mut base = &trimmed[start..];
    if let Some(suffix) = suffix {
        if !suffix.is_empty() && base.len() > suffix.len() && base.ends_with(suffix) {
            base = &base[..base.len() - suffix.len()];
        }
    }
    base
}

fn usage() {
    let _ = write_all(2, b"usage: basename NAME [SUFFIX]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() < 2 || args.len() > 3 {
        usage();
        return 2;
    }
    let out = basename(args[1], args.get(2).copied());
    if write_all(1, out) && write_all(1, b"\n") {
        0
    } else {
        1
    }
}

ristux_userland::program_main!(main);
