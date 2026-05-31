#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const NR_MKDIR: usize = 83;
const NR_CHMOD: usize = 90;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

struct Options {
    recursive: bool,
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

fn mkdir_path(path: &[u8], mode: u32) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall2(NR_MKDIR, path_c.as_ptr() as usize, mode as usize) >= 0 }
}

fn chmod_path(path: &[u8], mode: u32) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall2(NR_CHMOD, path_c.as_ptr() as usize, mode as usize) >= 0 }
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

fn join_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    if parent == b"/" {
        let mut out = Vec::with_capacity(1 + name.len());
        out.push(b'/');
        out.extend_from_slice(name);
        return out;
    }
    let mut out = Vec::with_capacity(parent.len() + name.len() + 1);
    out.extend_from_slice(parent);
    if !out.ends_with(b"/") {
        out.push(b'/');
    }
    out.extend_from_slice(name);
    out
}

fn read_dir_entries(path: &[u8]) -> Option<Vec<Vec<u8>>> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }

    let mut entries = Vec::new();
    let mut storage = [0u8; 1024];
    loop {
        let nread = sys::getdents64(fd as i32, &mut storage);
        if nread < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if nread == 0 {
            break;
        }
        let mut offset = 0usize;
        while offset + 19 <= nread as usize {
            let reclen = u16::from_le_bytes([storage[offset + 16], storage[offset + 17]]) as usize;
            if reclen == 0 || offset + reclen > nread as usize {
                break;
            }
            let name_start = offset + 19;
            let name_end = storage[name_start..offset + reclen]
                .iter()
                .position(|byte| *byte == 0)
                .map(|pos| name_start + pos)
                .unwrap_or(offset + reclen);
            let name = &storage[name_start..name_end];
            if name != b"." && name != b".." && !name.is_empty() {
                entries.push(name.to_vec());
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    entries.sort();
    Some(entries)
}

fn copy_file(src: &[u8], dest: &[u8]) -> i32 {
    if is_dir(src) {
        let _ = write_all(2, b"cp: refusing to copy directory\n");
        return 1;
    }
    let src_c = cstr(src);
    let input = sys::open(src_c.as_ptr(), O_RDONLY, 0);
    if input < 0 {
        let _ = write_all(2, b"cp: cannot open source\n");
        return 1;
    }

    let mode = stat_mode(src).unwrap_or(0o100644) & 0o777;
    let dest_c = cstr(dest);
    let output = sys::open(dest_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, mode);
    if output < 0 {
        let _ = sys::close(input as i32);
        let _ = write_all(2, b"cp: cannot open destination\n");
        return 1;
    }

    let mut status = 0;
    let mut buf = [0u8; 1024];
    loop {
        let n = sys::read(input as i32, &mut buf);
        if n < 0 {
            status = 1;
            break;
        }
        if n == 0 {
            break;
        }
        if !write_all(output as i32, &buf[..n as usize]) {
            status = 1;
            break;
        }
    }
    let _ = sys::close(input as i32);
    let _ = sys::close(output as i32);
    if status != 0 {
        let _ = write_all(2, b"cp: copy failed\n");
    }
    status
}

fn copy_tree(src: &[u8], dest: &[u8]) -> i32 {
    let mode = stat_mode(src).unwrap_or(0o040755) & 0o777;
    if !is_dir(dest) && !mkdir_path(dest, mode) && !is_dir(dest) {
        let _ = write_all(2, b"cp: cannot create destination directory\n");
        return 1;
    }

    let Some(entries) = read_dir_entries(src) else {
        let _ = write_all(2, b"cp: cannot read directory\n");
        return 1;
    };

    let mut status = 0;
    for entry in entries {
        let child_src = join_path(src, &entry);
        let child_dest = join_path(dest, &entry);
        let rc = if is_dir(&child_src) {
            copy_tree(&child_src, &child_dest)
        } else {
            copy_file(&child_src, &child_dest)
        };
        if rc != 0 {
            status = rc;
        }
    }
    if !chmod_path(dest, mode) && status == 0 {
        status = 1;
    }
    status
}

fn copy_path(src: &[u8], dest: &[u8], options: &Options) -> i32 {
    if is_dir(src) {
        if !options.recursive {
            let _ = write_all(2, b"cp: refusing to copy directory\n");
            return 1;
        }
        copy_tree(src, dest)
    } else {
        copy_file(src, dest)
    }
}

fn usage() {
    let _ = write_all(2, b"usage: cp [-r|-R] SOURCE... DEST\n");
}

fn parse_options(args: &[&[u8]]) -> Option<Options> {
    let mut options = Options {
        recursive: false,
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
                b'r' | b'R' => options.recursive = true,
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
        let _ = write_all(2, b"cp: destination must be a directory\n");
        return 1;
    }

    let mut status = 0;
    for src in &operands[..operands.len() - 1] {
        let dest = destination_path(src, dest_root);
        let rc = copy_path(src, &dest, &options);
        if rc != 0 {
            status = rc;
        }
    }
    status
}

ristux_userland::program_main!(main);
