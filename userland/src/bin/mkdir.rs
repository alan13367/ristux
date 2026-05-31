#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const NR_MKDIR: usize = 83;
const NR_CHMOD: usize = 90;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

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

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn parse_mode(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut mode = 0u32;
    for byte in bytes {
        if !(b'0'..=b'7').contains(byte) {
            return None;
        }
        mode = mode.checked_mul(8)?.checked_add((byte - b'0') as u32)?;
    }
    Some(mode)
}

fn stat_mode(path: &[u8]) -> Option<u32> {
    let path_c = cstr(path);
    let mut stat_buf = [0u8; 144];
    let rc = unsafe {
        sys::syscall2(
            sys::NR_STAT,
            path_c.as_ptr() as usize,
            stat_buf.as_mut_ptr() as usize,
        )
    };
    if rc < 0 {
        return None;
    }
    Some(u32::from_le_bytes([
        stat_buf[24],
        stat_buf[25],
        stat_buf[26],
        stat_buf[27],
    ]))
}

fn is_dir(path: &[u8]) -> bool {
    stat_mode(path).is_some_and(|mode| mode & S_IFMT == S_IFDIR)
}

fn mkdir_one(path: &[u8], mode: u32) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall2(NR_MKDIR, path_c.as_ptr() as usize, mode as usize) >= 0 }
}

fn chmod_path(path: &[u8], mode: u32) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall2(NR_CHMOD, path_c.as_ptr() as usize, mode as usize) >= 0 }
}

fn trim_trailing_slashes(path: &[u8]) -> &[u8] {
    let mut end = path.len();
    while end > 1 && path[end - 1] == b'/' {
        end -= 1;
    }
    &path[..end]
}

fn ensure_dir(path: &[u8], mode: u32, parents: bool) -> bool {
    let path = trim_trailing_slashes(path);
    if path.is_empty() || is_dir(path) {
        return true;
    }
    if !parents {
        return mkdir_one(path, mode);
    }

    let mut cursor = 0usize;
    if path.starts_with(b"/") {
        cursor = 1;
    }
    while cursor <= path.len() {
        let next = path[cursor..]
            .iter()
            .position(|byte| *byte == b'/')
            .map(|offset| cursor + offset)
            .unwrap_or(path.len());
        if next > 0 {
            let component = &path[..next];
            if !component.is_empty()
                && !is_dir(component)
                && !mkdir_one(component, if next == path.len() { mode } else { 0o755 })
                && !is_dir(component)
            {
                return false;
            }
        }
        if next == path.len() {
            break;
        }
        cursor = next + 1;
    }
    chmod_path(path, mode)
}

fn usage() {
    let _ = write_all(2, b"usage: mkdir [-p] [-m MODE] DIR...\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut parents = false;
    let mut mode = 0o755u32;
    let mut paths: Vec<&[u8]> = Vec::new();
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            paths.extend_from_slice(&args[index + 1..]);
            break;
        } else if *arg == b"-p" {
            parents = true;
            index += 1;
        } else if *arg == b"-m" {
            let Some(next) = args.get(index + 1).and_then(|bytes| parse_mode(bytes)) else {
                usage();
                return 2;
            };
            mode = next;
            index += 2;
        } else if let Some(rest) = arg.strip_prefix(b"-m") {
            let Some(parsed) = parse_mode(rest) else {
                usage();
                return 2;
            };
            mode = parsed;
            index += 1;
        } else if arg.starts_with(b"-") {
            usage();
            return 2;
        } else {
            paths.push(arg);
            index += 1;
        }
    }

    if paths.is_empty() {
        usage();
        return 2;
    }

    let mut status = 0;
    for path in paths {
        if !ensure_dir(path, mode, parents) {
            let _ = write_all(2, b"mkdir: cannot create directory ");
            let _ = write_all(2, path);
            let _ = write_all(2, b"\n");
            status = 1;
        }
    }
    status
}

ristux_userland::program_main!(main);
