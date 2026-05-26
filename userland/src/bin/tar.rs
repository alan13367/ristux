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
const BLOCK: usize = 512;

enum Mode {
    Create,
    Extract,
    List,
}

struct Options<'a> {
    mode: Mode,
    archive: &'a [u8],
    files: &'a [&'a [u8]],
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

fn print_err(prefix: &[u8], path: &[u8]) {
    let _ = sys::write(2, b"tar: ");
    let _ = sys::write(2, prefix);
    let _ = sys::write(2, path);
    let _ = sys::write(2, b"\n");
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd as i32, &mut buf);
        if n < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn write_octal(field: &mut [u8], value: usize) {
    field.fill(b'0');
    if field.is_empty() {
        return;
    }
    field[field.len() - 1] = 0;
    if field.len() == 1 {
        return;
    }
    let mut value = value;
    let mut pos = field.len() - 2;
    loop {
        field[pos] = b'0' + (value & 7) as u8;
        value >>= 3;
        if value == 0 || pos == 0 {
            break;
        }
        pos -= 1;
    }
}

fn write_checksum(field: &mut [u8], value: usize) {
    if field.len() != 8 {
        return;
    }
    field[..6].fill(b'0');
    let mut value = value;
    let mut pos = 5usize;
    loop {
        field[pos] = b'0' + (value & 7) as u8;
        value >>= 3;
        if value == 0 || pos == 0 {
            break;
        }
        pos -= 1;
    }
    field[6] = 0;
    field[7] = b' ';
}

fn parse_octal(field: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut saw_digit = false;
    for &byte in field {
        if byte == 0 || byte == b' ' {
            break;
        }
        if !(b'0'..=b'7').contains(&byte) {
            return None;
        }
        saw_digit = true;
        value = value.checked_mul(8)?.checked_add((byte - b'0') as usize)?;
    }
    if saw_digit {
        Some(value)
    } else {
        Some(0)
    }
}

fn archive_name(path: &[u8]) -> &[u8] {
    let mut name = path;
    while name.starts_with(b"./") {
        name = &name[2..];
    }
    while name.starts_with(b"/") {
        name = &name[1..];
    }
    name
}

fn safe_archive_path(path: &[u8]) -> bool {
    if path.is_empty() || path.starts_with(b"/") {
        return false;
    }
    for part in path.split(|byte| *byte == b'/') {
        if part.is_empty() || part == b"." || part == b".." {
            return false;
        }
    }
    true
}

fn trim_trailing_slash(mut path: &[u8]) -> &[u8] {
    while path.len() > 1 && path.ends_with(b"/") {
        path = &path[..path.len() - 1];
    }
    path
}

fn build_header(name: &[u8], size: usize, mode: usize, typeflag: u8) -> Option<[u8; BLOCK]> {
    if name.is_empty() || name.len() > 100 {
        return None;
    }
    let mut header = [0u8; BLOCK];
    header[..name.len()].copy_from_slice(name);
    write_octal(&mut header[100..108], mode);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(&mut header[124..136], size);
    write_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = typeflag;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let sum: usize = header.iter().map(|byte| *byte as usize).sum();
    write_checksum(&mut header[148..156], sum);
    Some(header)
}

fn name_from_header(header: &[u8; BLOCK]) -> Option<&[u8]> {
    let end = header[..100].iter().position(|byte| *byte == 0).unwrap_or(100);
    let name = trim_trailing_slash(&header[..end]);
    if safe_archive_path(name) {
        Some(name)
    } else {
        None
    }
}

fn valid_checksum(header: &[u8; BLOCK]) -> bool {
    let stored = parse_octal(&header[148..156]).unwrap_or(usize::MAX);
    let mut copy = *header;
    copy[148..156].fill(b' ');
    let actual: usize = copy.iter().map(|byte| *byte as usize).sum();
    stored == actual
}

fn padded_size(size: usize) -> Option<usize> {
    size.checked_add(BLOCK - 1).map(|value| value & !(BLOCK - 1))
}

fn create_archive(archive: &[u8], files: &[&[u8]]) -> i32 {
    if files.is_empty() {
        let _ = sys::write(2, b"tar: no input files\n");
        return 1;
    }
    let archive_c = cstr(archive);
    let out = sys::open(archive_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out < 0 {
        print_err(b"cannot create ", archive);
        return 1;
    }
    let zero = [0u8; BLOCK];
    for path in files {
        let Some(data) = read_file(path) else {
            print_err(b"cannot read ", path);
            let _ = sys::close(out as i32);
            return 1;
        };
        let name = archive_name(path);
        if !safe_archive_path(name) {
            print_err(b"unsafe path ", path);
            let _ = sys::close(out as i32);
            return 1;
        }
        let Some(header) = build_header(name, data.len(), 0o644, b'0') else {
            print_err(b"name too long ", path);
            let _ = sys::close(out as i32);
            return 1;
        };
        if !write_all(out as i32, &header) || !write_all(out as i32, &data) {
            print_err(b"write failed ", archive);
            let _ = sys::close(out as i32);
            return 1;
        }
        let pad = (BLOCK - (data.len() % BLOCK)) % BLOCK;
        if pad != 0 && !write_all(out as i32, &zero[..pad]) {
            print_err(b"write failed ", archive);
            let _ = sys::close(out as i32);
            return 1;
        }
    }
    let ok = write_all(out as i32, &zero) && write_all(out as i32, &zero);
    let _ = sys::close(out as i32);
    if !ok {
        print_err(b"write failed ", archive);
        return 1;
    }
    0
}

fn mkdir_path(path: &[u8], mode: u32) -> bool {
    let path_c = cstr(path);
    let r = unsafe { sys::syscall2(NR_MKDIR, path_c.as_ptr() as usize, mode as usize) };
    r >= 0 || r == -17
}

fn chmod_path(path: &[u8], mode: u32) {
    let path_c = cstr(path);
    let _ = unsafe { sys::syscall2(NR_CHMOD, path_c.as_ptr() as usize, mode as usize) };
}

fn ensure_parent_dirs(path: &[u8]) -> bool {
    let mut cur = Vec::new();
    for part in path.split(|byte| *byte == b'/').take_while(|part| !part.is_empty()) {
        if cur.is_empty() {
            cur.extend_from_slice(part);
        } else {
            cur.push(b'/');
            cur.extend_from_slice(part);
        }
        if cur.len() < path.len() && !mkdir_path(&cur, 0o755) {
            return false;
        }
    }
    true
}

fn read_exact(fd: i32, mut output: &mut [u8]) -> bool {
    while !output.is_empty() {
        let n = sys::read(fd, output);
        if n <= 0 {
            return false;
        }
        let read = n as usize;
        let rest = output;
        output = &mut rest[read..];
    }
    true
}

fn skip_bytes(fd: i32, mut count: usize) -> bool {
    let mut buf = [0u8; BLOCK];
    while count > 0 {
        let n = count.min(buf.len());
        if !read_exact(fd, &mut buf[..n]) {
            return false;
        }
        count -= n;
    }
    true
}

fn walk_archive(archive: &[u8], extract: bool) -> i32 {
    let archive_c = cstr(archive);
    let fd = sys::open(archive_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        print_err(b"cannot open ", archive);
        return 1;
    }
    let mut header = [0u8; BLOCK];
    loop {
        if !read_exact(fd as i32, &mut header) {
            print_err(b"truncated archive ", archive);
            let _ = sys::close(fd as i32);
            return 1;
        }
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        if !valid_checksum(&header) {
            print_err(b"bad checksum in ", archive);
            let _ = sys::close(fd as i32);
            return 1;
        }
        let Some(name) = name_from_header(&header) else {
            let _ = sys::write(2, b"tar: unsafe archive path\n");
            let _ = sys::close(fd as i32);
            return 1;
        };
        let size = parse_octal(&header[124..136]).unwrap_or(usize::MAX);
        let Some(padded) = padded_size(size) else {
            print_err(b"bad size in ", archive);
            let _ = sys::close(fd as i32);
            return 1;
        };
        let mode = parse_octal(&header[100..108]).unwrap_or(0o644) as u32;
        let typeflag = header[156];
        if !extract {
            let _ = sys::write(1, name);
            let _ = sys::write(1, b"\n");
            if !skip_bytes(fd as i32, padded) {
                print_err(b"truncated archive ", archive);
                let _ = sys::close(fd as i32);
                return 1;
            }
            continue;
        }
        if typeflag == b'5' {
            if !mkdir_path(name, mode) {
                print_err(b"cannot mkdir ", name);
                let _ = sys::close(fd as i32);
                return 1;
            }
            if !skip_bytes(fd as i32, padded) {
                print_err(b"truncated archive ", archive);
                let _ = sys::close(fd as i32);
                return 1;
            }
            continue;
        }
        if typeflag != 0 && typeflag != b'0' {
            if !skip_bytes(fd as i32, padded) {
                print_err(b"truncated archive ", archive);
                let _ = sys::close(fd as i32);
                return 1;
            }
            continue;
        }
        if !ensure_parent_dirs(name) {
            print_err(b"cannot create parent for ", name);
            let _ = sys::close(fd as i32);
            return 1;
        }
        let name_c = cstr(name);
        let out = sys::open(name_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, mode);
        if out < 0 {
            print_err(b"cannot extract ", name);
            let _ = sys::close(fd as i32);
            return 1;
        }
        let mut remaining = size;
        let mut buf = [0u8; BLOCK];
        while remaining > 0 {
            let n = remaining.min(buf.len());
            if !read_exact(fd as i32, &mut buf[..n]) || !write_all(out as i32, &buf[..n]) {
                print_err(b"extract failed ", name);
                let _ = sys::close(out as i32);
                let _ = sys::close(fd as i32);
                return 1;
            }
            remaining -= n;
        }
        let _ = sys::close(out as i32);
        chmod_path(name, mode);
        if padded > size && !skip_bytes(fd as i32, padded - size) {
            print_err(b"truncated archive ", archive);
            let _ = sys::close(fd as i32);
            return 1;
        }
    }
    let _ = sys::close(fd as i32);
    0
}

fn parse_options<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    if args.len() < 3 {
        return None;
    }
    let flags = args[1];
    let mode = if flags.contains(&b'c') {
        Mode::Create
    } else if flags.contains(&b'x') {
        Mode::Extract
    } else if flags.contains(&b't') {
        Mode::List
    } else {
        return None;
    };
    let mut archive_index = None;
    if flags.contains(&b'f') {
        archive_index = Some(2);
    } else if args.len() > 3 && args[2] == b"-f" {
        archive_index = Some(3);
    }
    let archive_index = archive_index?;
    if archive_index >= args.len() {
        return None;
    }
    let files_start = archive_index + 1;
    Some(Options {
        mode,
        archive: args[archive_index],
        files: if files_start < args.len() {
            &args[files_start..]
        } else {
            &[]
        },
    })
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(options) = parse_options(args) else {
        let _ = sys::write(2, b"usage: tar -cf archive file... | tar -tf archive | tar -xf archive\n");
        return 2;
    };
    match options.mode {
        Mode::Create => create_archive(options.archive, options.files),
        Mode::Extract => walk_archive(options.archive, true),
        Mode::List => walk_archive(options.archive, false),
    }
}

ristux_userland::program_main!(main);
