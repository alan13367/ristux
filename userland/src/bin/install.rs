#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const NR_MKDIR: usize = 83;
const NR_CHMOD: usize = 90;
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

fn mkdir_path(path: &[u8], mode: u32) -> bool {
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

fn basename(path: &[u8]) -> &[u8] {
    let trimmed = trim_trailing_slashes(path);
    if trimmed.is_empty() {
        return b".";
    }
    if trimmed.iter().all(|byte| *byte == b'/') {
        return b"/";
    }
    trimmed
        .iter()
        .rposition(|byte| *byte == b'/')
        .map(|pos| &trimmed[pos + 1..])
        .unwrap_or(trimmed)
}

fn parent_path(path: &[u8]) -> Option<Vec<u8>> {
    let trimmed = trim_trailing_slashes(path);
    let slash = trimmed.iter().rposition(|byte| *byte == b'/')?;
    if slash == 0 {
        Some(b"/".to_vec())
    } else {
        Some(trimmed[..slash].to_vec())
    }
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

fn ensure_dir(path: &[u8], mode: u32) -> bool {
    if path.is_empty() || is_dir(path) {
        return true;
    }
    if let Some(parent) = parent_path(path) {
        if parent != path && !is_dir(&parent) && !ensure_dir(&parent, 0o755) {
            return false;
        }
    }
    if !mkdir_path(path, mode) && !is_dir(path) {
        return false;
    }
    chmod_path(path, mode)
}

fn install_file(src: &[u8], dest: &[u8], mode: u32, create_parent: bool) -> bool {
    if is_dir(src) {
        let _ = write_all(2, b"install: source is a directory\n");
        return false;
    }
    let final_dest = destination_path(src, dest);
    if create_parent {
        if let Some(parent) = parent_path(&final_dest) {
            if !ensure_dir(&parent, 0o755) {
                let _ = write_all(2, b"install: cannot create parent directory\n");
                return false;
            }
        }
    }

    let src_c = cstr(src);
    let input = sys::open(src_c.as_ptr(), O_RDONLY, 0);
    if input < 0 {
        let _ = write_all(2, b"install: cannot open source\n");
        return false;
    }

    let dest_c = cstr(&final_dest);
    let output = sys::open(dest_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, mode);
    if output < 0 {
        let _ = sys::close(input as i32);
        let _ = write_all(2, b"install: cannot open destination\n");
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
    ok && chmod_path(&final_dest, mode)
}

fn usage() {
    let _ = write_all(2, b"usage: install [-D] [-d] [-m MODE] SOURCE... DEST\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut create_dirs = false;
    let mut create_parent = false;
    let mut mode = 0o755u32;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-d" {
            create_dirs = true;
            index += 1;
        } else if *arg == b"-D" {
            create_parent = true;
            index += 1;
        } else if *arg == b"-m" {
            let Some(next) = args.get(index + 1) else {
                usage();
                return 2;
            };
            let Some(parsed) = parse_mode(next) else {
                usage();
                return 2;
            };
            mode = parsed;
            index += 2;
        } else if arg.starts_with(b"-") && arg.len() > 1 {
            usage();
            return 2;
        } else {
            break;
        }
    }

    if create_dirs {
        if index >= args.len() {
            usage();
            return 2;
        }
        let mut rc = 0;
        for path in &args[index..] {
            if !ensure_dir(path, mode) {
                let _ = write_all(2, b"install: cannot create directory ");
                let _ = write_all(2, path);
                let _ = write_all(2, b"\n");
                rc = 1;
            }
        }
        return rc;
    }

    if args.len() - index < 2 {
        usage();
        return 2;
    }
    let dest = args[args.len() - 1];
    let sources = &args[index..args.len() - 1];
    if sources.len() > 1 && !is_dir(dest) {
        let _ = write_all(2, b"install: target is not a directory\n");
        return 1;
    }

    let mut rc = 0;
    for src in sources {
        if !install_file(src, dest, mode, create_parent) {
            rc = 1;
        }
    }
    rc
}

ristux_userland::program_main!(main);
