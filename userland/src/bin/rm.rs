#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const NR_LSTAT: usize = 6;
const NR_RMDIR: usize = 84;
const NR_UNLINK: usize = 87;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Directory,
    Other,
}

struct Options {
    force: bool,
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

fn lstat_mode(path: &[u8]) -> Option<u32> {
    let path_c = cstr(path);
    let mut stat_buf = [0u8; 144];
    let rc = unsafe {
        sys::syscall2(
            NR_LSTAT,
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

fn path_kind(path: &[u8]) -> Option<Kind> {
    lstat_mode(path).map(|mode| {
        if mode & S_IFMT == S_IFDIR {
            Kind::Directory
        } else {
            Kind::Other
        }
    })
}

fn unlink_path(path: &[u8]) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall1(NR_UNLINK, path_c.as_ptr() as usize) >= 0 }
}

fn rmdir_path(path: &[u8]) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall1(NR_RMDIR, path_c.as_ptr() as usize) >= 0 }
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

fn print_error(prefix: &[u8], path: &[u8]) {
    let _ = write_all(2, b"rm: ");
    let _ = write_all(2, prefix);
    let _ = write_all(2, path);
    let _ = write_all(2, b"\n");
}

fn remove_path(path: &[u8], options: &Options) -> bool {
    let Some(kind) = path_kind(path) else {
        if !options.force {
            print_error(b"cannot remove ", path);
        }
        return options.force;
    };

    if kind != Kind::Directory {
        if unlink_path(path) {
            return true;
        }
        print_error(b"cannot remove ", path);
        return false;
    }

    if !options.recursive {
        print_error(b"is a directory: ", path);
        return false;
    }
    if path == b"/" {
        print_error(b"refusing to remove ", path);
        return false;
    }

    let Some(entries) = read_dir_entries(path) else {
        if rmdir_path(path) {
            return true;
        }
        print_error(b"cannot read directory ", path);
        return false;
    };

    let mut ok = true;
    for entry in entries {
        let child = join_path(path, &entry);
        if !remove_path(&child, options) {
            ok = false;
        }
    }
    if rmdir_path(path) {
        ok
    } else {
        print_error(b"cannot remove directory ", path);
        false
    }
}

fn usage() {
    let _ = write_all(2, b"usage: rm [-f] [-r|-R] FILE...\n");
}

fn parse_options(args: &[&[u8]]) -> Option<Options> {
    let mut options = Options {
        force: false,
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
                b'f' => options.force = true,
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
    if options.paths_start >= args.len() {
        if options.force {
            return 0;
        }
        usage();
        return 2;
    }

    let mut status = 0;
    for path in &args[options.paths_start..] {
        if !remove_path(path, &options) {
            status = 1;
        }
    }
    status
}

ristux_userland::program_main!(main);
