#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const DT_LNK: u8 = 10;
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_IFDIR: u32 = 0o040000;
const S_IFLNK: u32 = 0o120000;

#[derive(Clone, Copy)]
enum FileType {
    File,
    Directory,
    Symlink,
}

struct Expr<'a> {
    name: Option<&'a [u8]>,
    kind: Option<FileType>,
    maxdepth: Option<usize>,
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

fn parse_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0usize;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
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

fn file_type_from_mode(path: &[u8]) -> Option<FileType> {
    stat_mode(path).and_then(|mode| match mode & S_IFMT {
        S_IFREG => Some(FileType::File),
        S_IFDIR => Some(FileType::Directory),
        S_IFLNK => Some(FileType::Symlink),
        _ => None,
    })
}

fn path_type(path: &[u8]) -> Option<FileType> {
    if let Some(kind) = file_type_from_mode(path) {
        return Some(kind);
    }

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut storage = [0u8; 32];
    let nread = sys::getdents64(fd as i32, &mut storage);
    let _ = sys::close(fd as i32);
    if nread >= 0 {
        Some(FileType::Directory)
    } else {
        Some(FileType::File)
    }
}

fn file_type_from_dirent(dtype: u8, path: &[u8]) -> Option<FileType> {
    match dtype {
        DT_REG => Some(FileType::File),
        DT_DIR => Some(FileType::Directory),
        DT_LNK => Some(FileType::Symlink),
        _ => path_type(path),
    }
}

fn same_type(left: FileType, right: FileType) -> bool {
    matches!(
        (left, right),
        (FileType::File, FileType::File)
            | (FileType::Directory, FileType::Directory)
            | (FileType::Symlink, FileType::Symlink)
    )
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

fn join_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    if parent == b"." {
        let mut out = Vec::with_capacity(2 + name.len());
        out.extend_from_slice(b"./");
        out.extend_from_slice(name);
        return out;
    }
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

fn glob_match(pattern: &[u8], name: &[u8]) -> bool {
    let mut p = 0usize;
    let mut n = 0usize;
    let mut star: Option<usize> = None;
    let mut retry = 0usize;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = n;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            retry += 1;
            n = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn matches_expr(path: &[u8], kind: FileType, expr: &Expr) -> bool {
    if let Some(pattern) = expr.name {
        if !glob_match(pattern, basename(path)) {
            return false;
        }
    }
    if let Some(expected) = expr.kind {
        if !same_type(kind, expected) {
            return false;
        }
    }
    true
}

fn print_path(path: &[u8]) -> i32 {
    if write_all(1, path) && write_all(1, b"\n") {
        0
    } else {
        1
    }
}

fn read_dir_entries(path: &[u8]) -> Option<Vec<(Vec<u8>, u8)>> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }

    let mut out = Vec::new();
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
            let dtype = storage[offset + 18];
            let name_start = offset + 19;
            let name_end = storage[name_start..offset + reclen]
                .iter()
                .position(|&byte| byte == 0)
                .map(|pos| name_start + pos)
                .unwrap_or(offset + reclen);
            let name = &storage[name_start..name_end];
            if name != b"." && name != b".." && !name.is_empty() {
                out.push((name.to_vec(), dtype));
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    out.sort_by(|left, right| left.0.cmp(&right.0));
    Some(out)
}

fn walk(path: &[u8], depth: usize, kind: FileType, expr: &Expr) -> i32 {
    let mut rc = 0;
    if matches_expr(path, kind, expr) {
        rc = print_path(path);
    }
    if !same_type(kind, FileType::Directory) {
        return rc;
    }
    if expr.maxdepth.is_some_and(|maxdepth| depth >= maxdepth) {
        return rc;
    }

    let Some(entries) = read_dir_entries(path) else {
        let _ = write_all(2, b"find: cannot read directory ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return 1;
    };

    for (name, dtype) in entries {
        let child = join_path(path, &name);
        let Some(child_kind) = file_type_from_dirent(dtype, &child) else {
            continue;
        };
        let child_rc = walk(&child, depth + 1, child_kind, expr);
        if child_rc != 0 {
            rc = child_rc;
        }
    }
    rc
}

fn usage() {
    let _ = write_all(
        2,
        b"usage: find [PATH...] [-name PATTERN] [-type f|d|l] [-maxdepth N] [-print]\n",
    );
}

fn main(args: &[&[u8]]) -> i32 {
    let mut paths: Vec<&[u8]> = Vec::new();
    let mut expr = Expr {
        name: None,
        kind: None,
        maxdepth: None,
    };
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"-print" {
            index += 1;
        } else if *arg == b"-name" {
            let Some(pattern) = args.get(index + 1) else {
                usage();
                return 2;
            };
            expr.name = Some(pattern);
            index += 2;
        } else if *arg == b"-type" {
            let Some(kind) = args.get(index + 1) else {
                usage();
                return 2;
            };
            expr.kind = match *kind {
                b"f" => Some(FileType::File),
                b"d" => Some(FileType::Directory),
                b"l" => Some(FileType::Symlink),
                _ => {
                    usage();
                    return 2;
                }
            };
            index += 2;
        } else if *arg == b"-maxdepth" {
            let Some(depth) = args.get(index + 1).and_then(|value| parse_usize(value)) else {
                usage();
                return 2;
            };
            expr.maxdepth = Some(depth);
            index += 2;
        } else if arg.starts_with(b"-") {
            usage();
            return 2;
        } else {
            paths.push(arg);
            index += 1;
        }
    }

    if paths.is_empty() {
        paths.push(b".");
    }

    let mut rc = 0;
    for path in paths {
        let Some(kind) = path_type(path) else {
            let _ = write_all(2, b"find: cannot stat ");
            let _ = write_all(2, path);
            let _ = write_all(2, b"\n");
            rc = 1;
            continue;
        };
        let path_rc = walk(path, 0, kind, &expr);
        if path_rc != 0 {
            rc = path_rc;
        }
    }
    rc
}

ristux_userland::program_main!(main);
