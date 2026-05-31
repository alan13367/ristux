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

struct Options {
    force: bool,
    paths_start: usize,
}

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

fn move_path(src: &[u8], dest: &[u8], options: &Options) -> bool {
    if rename_path(src, dest) {
        return true;
    }
    if options.force && !is_dir(dest) {
        let _ = unlink_path(dest);
        if rename_path(src, dest) {
            return true;
        }
    }
    if copy_file(src, dest) && unlink_path(src) {
        return true;
    }
    false
}

fn usage() {
    let _ = write_all(2, b"usage: mv [-f] SOURCE... DEST\n");
}

fn parse_options(args: &[&[u8]]) -> Option<Options> {
    let mut options = Options {
        force: false,
        paths_start: 1,
    };
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-" || !arg.starts_with(b"-") {
            break;
        }
        for flag in &arg[1..] {
            match *flag {
                b'f' => options.force = true,
                _ => return None,
            }
        }
        index += 1;
    }
    options.paths_start = index;
    Some(options)
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(options) = parse_options(args) else {
        usage();
        return 2;
    };
    let operands = &args[options.paths_start..];
    if operands.len() < 2 {
        usage();
        return 2;
    }
    let dest_root = operands[operands.len() - 1];
    if operands.len() > 2 && !is_dir(dest_root) {
        let _ = write_all(2, b"mv: destination must be a directory\n");
        return 1;
    }

    let mut status = 0;
    for src in &operands[..operands.len() - 1] {
        let dest = destination_path(src, dest_root);
        if !move_path(src, &dest, &options) {
            let _ = write_all(2, b"mv: failed: ");
            let _ = write_all(2, src);
            let _ = write_all(2, b"\n");
            status = 1;
        }
    }
    status
}

ristux_userland::program_main!(main);
