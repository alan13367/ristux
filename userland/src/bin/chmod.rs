#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const NR_CHMOD: usize = 90;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

#[derive(Clone, Copy)]
enum ModeSpec<'a> {
    Absolute(u32),
    Symbolic(&'a [u8]),
}

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

fn parse_octal(bytes: &[u8]) -> Option<u32> {
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

fn chmod_path(path: &[u8], mode: u32) -> bool {
    let path_c = cstr(path);
    unsafe { sys::syscall2(NR_CHMOD, path_c.as_ptr() as usize, mode as usize) >= 0 }
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

fn class_mask(classes: u8, perm: u32) -> u32 {
    let mut mask = 0u32;
    if classes & 0b001 != 0 {
        mask |= perm << 6;
    }
    if classes & 0b010 != 0 {
        mask |= perm << 3;
    }
    if classes & 0b100 != 0 {
        mask |= perm;
    }
    mask
}

fn symbolic_mode(spec: &[u8], current: u32) -> Option<u32> {
    let mut mode = current & 0o7777;
    let mut start = 0usize;
    while start <= spec.len() {
        let end = spec[start..]
            .iter()
            .position(|byte| *byte == b',')
            .map(|offset| start + offset)
            .unwrap_or(spec.len());
        let clause = &spec[start..end];
        if clause.is_empty() {
            return None;
        }

        let mut index = 0usize;
        let mut classes = 0u8;
        while index < clause.len() {
            match clause[index] {
                b'u' => classes |= 0b001,
                b'g' => classes |= 0b010,
                b'o' => classes |= 0b100,
                b'a' => classes |= 0b111,
                _ => break,
            }
            index += 1;
        }
        if classes == 0 {
            classes = 0b111;
        }
        let op = *clause.get(index)?;
        if !matches!(op, b'+' | b'-' | b'=') {
            return None;
        }
        index += 1;
        let mut perms = 0u32;
        while index < clause.len() {
            match clause[index] {
                b'r' => perms |= 0o4,
                b'w' => perms |= 0o2,
                b'x' => perms |= 0o1,
                _ => return None,
            }
            index += 1;
        }
        let mask = class_mask(classes, perms);
        let class_all = class_mask(classes, 0o7);
        match op {
            b'+' => mode |= mask,
            b'-' => mode &= !mask,
            b'=' => mode = (mode & !class_all) | mask,
            _ => return None,
        }

        if end == spec.len() {
            break;
        }
        start = end + 1;
    }
    Some(mode)
}

fn target_mode(path: &[u8], spec: ModeSpec<'_>) -> Option<u32> {
    match spec {
        ModeSpec::Absolute(mode) => Some(mode),
        ModeSpec::Symbolic(spec) => symbolic_mode(spec, stat_mode(path)?),
    }
}

fn print_error(prefix: &[u8], path: &[u8]) {
    let _ = write_all(2, b"chmod: ");
    let _ = write_all(2, prefix);
    let _ = write_all(2, path);
    let _ = write_all(2, b"\n");
}

fn apply_path(path: &[u8], spec: ModeSpec<'_>, recursive: bool) -> bool {
    let Some(mode) = target_mode(path, spec) else {
        print_error(b"cannot stat ", path);
        return false;
    };
    let mut ok = chmod_path(path, mode);
    if !ok {
        print_error(b"cannot change mode of ", path);
    }
    if recursive && is_dir(path) {
        let Some(entries) = read_dir_entries(path) else {
            print_error(b"cannot read directory ", path);
            return false;
        };
        for entry in entries {
            let child = join_path(path, &entry);
            if !apply_path(&child, spec, true) {
                ok = false;
            }
        }
    }
    ok
}

fn usage() {
    let _ = write_all(2, b"usage: chmod [-R] MODE FILE...\n");
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
                b'R' => options.recursive = true,
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
    if options.paths_start + 1 >= args.len() {
        usage();
        return 2;
    }

    let mode_arg = args[options.paths_start];
    let spec = parse_octal(mode_arg)
        .map(ModeSpec::Absolute)
        .unwrap_or(ModeSpec::Symbolic(mode_arg));
    let mut status = 0;
    for path in &args[options.paths_start + 1..] {
        if !apply_path(path, spec, options.recursive) {
            status = 1;
        }
    }
    status
}

ristux_userland::program_main!(main);
