#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const NR_RENAME: usize = 82;
const NR_UNLINK: usize = 87;
const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
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

fn basename(path: &[u8]) -> &[u8] {
    let mut trimmed = path;
    while trimmed.len() > 1 && trimmed.ends_with(b"/") {
        trimmed = &trimmed[..trimmed.len() - 1];
    }
    trimmed
        .iter()
        .rposition(|byte| *byte == b'/')
        .map(|pos| &trimmed[pos + 1..])
        .unwrap_or(trimmed)
}

fn destination_path(src: &[u8], dest: &[u8]) -> Vec<u8> {
    if !is_dir(dest) {
        return dest.to_vec();
    }
    let name = basename(src);
    let mut out = Vec::with_capacity(dest.len() + name.len() + 1);
    out.extend_from_slice(dest);
    if !out.ends_with(b"/") {
        out.push(b'/');
    }
    out.extend_from_slice(name);
    out
}

fn rename_path(src: &[u8], dest: &[u8]) -> bool {
    let src_c = cstr(src);
    let dest_c = cstr(dest);
    unsafe { sys::syscall2(NR_RENAME, src_c.as_ptr() as usize, dest_c.as_ptr() as usize) >= 0 }
}

fn unlink_path(path: &[u8]) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall1(NR_UNLINK, path_c.as_ptr() as usize) >= 0 }
}

fn copy_file(src: &[u8], dest: &[u8]) -> bool {
    if is_dir(src) {
        return false;
    }
    let src_c = cstr(src);
    let input = sys::open(src_c.as_ptr(), O_RDONLY, 0);
    if input < 0 {
        return false;
    }
    let mode = stat_mode(src).unwrap_or(0o100644) & 0o777;
    let dest_c = cstr(dest);
    let output = sys::open(dest_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, mode);
    if output < 0 {
        let _ = sys::close(input as i32);
        return false;
    }
    let mut ok = true;
    let mut buf = [0u8; 1024];
    loop {
        let n = sys::read(input as i32, &mut buf);
        if n < 0 {
            ok = false;
            break;
        }
        if n == 0 {
            break;
        }
        if !write_all(output as i32, &buf[..n as usize]) {
            ok = false;
            break;
        }
    }
    let _ = sys::close(input as i32);
    let _ = sys::close(output as i32);
    ok
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() != 3 {
        let _ = write_all(2, b"usage: mv SOURCE DEST\n");
        return 2;
    }
    let dest = destination_path(args[1], args[2]);
    if rename_path(args[1], &dest) {
        return 0;
    }
    if copy_file(args[1], &dest) && unlink_path(args[1]) {
        return 0;
    }
    let _ = write_all(2, b"mv: failed\n");
    1
}

ristux_userland::program_main!(main);
