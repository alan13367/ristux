#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_IFDIR: u32 = 0o040000;
const S_IFCHR: u32 = 0o020000;

struct Metadata {
    nlink: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    size: i64,
}

struct Options<'a> {
    format: Option<&'a [u8]>,
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

fn stat_path(path: &[u8]) -> Option<Metadata> {
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
    Some(Metadata {
        nlink: u64::from_le_bytes([
            stat_buf[16],
            stat_buf[17],
            stat_buf[18],
            stat_buf[19],
            stat_buf[20],
            stat_buf[21],
            stat_buf[22],
            stat_buf[23],
        ]),
        mode: u32::from_le_bytes([stat_buf[24], stat_buf[25], stat_buf[26], stat_buf[27]]),
        uid: u32::from_le_bytes([stat_buf[28], stat_buf[29], stat_buf[30], stat_buf[31]]),
        gid: u32::from_le_bytes([stat_buf[32], stat_buf[33], stat_buf[34], stat_buf[35]]),
        size: i64::from_le_bytes([
            stat_buf[48],
            stat_buf[49],
            stat_buf[50],
            stat_buf[51],
            stat_buf[52],
            stat_buf[53],
            stat_buf[54],
            stat_buf[55],
        ]),
    })
}

fn path_is_dir(path: &[u8]) -> bool {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let mut buf = [0u8; 128];
    let rc = sys::getdents64(fd as i32, &mut buf);
    let _ = sys::close(fd as i32);
    rc >= 0
}

fn push_unsigned(out: &mut Vec<u8>, mut value: u64, base: u64, min_width: usize) {
    let mut digits = [0u8; 32];
    let mut len = 0usize;
    loop {
        let digit = (value % base) as u8;
        digits[len] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + (digit - 10)
        };
        value /= base;
        len += 1;
        if value == 0 {
            break;
        }
    }
    while len < min_width {
        digits[len] = b'0';
        len += 1;
    }
    while len > 0 {
        len -= 1;
        out.push(digits[len]);
    }
}

fn push_signed(out: &mut Vec<u8>, value: i64) {
    if value < 0 {
        out.push(b'-');
        push_unsigned(out, value.wrapping_neg() as u64, 10, 0);
    } else {
        push_unsigned(out, value as u64, 10, 0);
    }
}

fn type_name(path: &[u8], mode: u32) -> &'static [u8] {
    match mode & S_IFMT {
        S_IFREG => b"regular file",
        S_IFDIR => b"directory",
        S_IFCHR => b"character special file",
        _ if path_is_dir(path) => b"directory",
        _ => b"regular file",
    }
}

fn type_char(path: &[u8], mode: u32) -> u8 {
    match mode & S_IFMT {
        S_IFDIR => b'd',
        S_IFCHR => b'c',
        _ if path_is_dir(path) => b'd',
        _ => b'-',
    }
}

fn push_format(out: &mut Vec<u8>, format: &[u8], path: &[u8], meta: &Metadata) -> bool {
    let mut index = 0usize;
    while index < format.len() {
        let byte = format[index];
        if byte == b'\\' && index + 1 < format.len() {
            match format[index + 1] {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                other => out.push(other),
            }
            index += 2;
            continue;
        }
        if byte != b'%' {
            out.push(byte);
            index += 1;
            continue;
        }
        let Some(spec) = format.get(index + 1).copied() else {
            return false;
        };
        match spec {
            b'%' => out.push(b'%'),
            b'n' => out.extend_from_slice(path),
            b'N' => {
                out.push(b'\'');
                out.extend_from_slice(path);
                out.push(b'\'');
            }
            b's' => push_signed(out, meta.size),
            b'a' => push_unsigned(out, (meta.mode & 0o777) as u64, 8, 3),
            b'A' => push_mode_string(out, path, meta.mode),
            b'f' => push_unsigned(out, meta.mode as u64, 16, 0),
            b'F' => out.extend_from_slice(type_name(path, meta.mode)),
            b'u' => push_unsigned(out, meta.uid as u64, 10, 0),
            b'g' => push_unsigned(out, meta.gid as u64, 10, 0),
            b'h' => push_unsigned(out, meta.nlink, 10, 0),
            _ => return false,
        }
        index += 2;
    }
    true
}

fn push_mode_string(out: &mut Vec<u8>, path: &[u8], mode: u32) {
    out.push(type_char(path, mode));
    for (read, write, exec) in [
        (0o400, 0o200, 0o100),
        (0o040, 0o020, 0o010),
        (0o004, 0o002, 0o001),
    ] {
        out.push(if mode & read != 0 { b'r' } else { b'-' });
        out.push(if mode & write != 0 { b'w' } else { b'-' });
        out.push(if mode & exec != 0 { b'x' } else { b'-' });
    }
}

fn print_default(path: &[u8], meta: &Metadata) -> bool {
    let mut out = Vec::new();
    out.extend_from_slice(b"  File: ");
    out.extend_from_slice(path);
    out.extend_from_slice(b"\n  Size: ");
    push_signed(&mut out, meta.size);
    out.extend_from_slice(b"\n  Type: ");
    out.extend_from_slice(type_name(path, meta.mode));
    out.extend_from_slice(b"\n  Mode: ");
    push_unsigned(&mut out, (meta.mode & 0o777) as u64, 8, 4);
    out.extend_from_slice(b" (");
    push_mode_string(&mut out, path, meta.mode);
    out.extend_from_slice(b")\n  Links: ");
    push_unsigned(&mut out, meta.nlink, 10, 0);
    out.push(b'\n');
    write_all(1, &out)
}

fn parse_options<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    let mut format = None;
    let mut index = 1usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"-L" {
            index += 1;
        } else if arg == b"-c" || arg == b"--format" {
            index += 1;
            format = Some(*args.get(index)?);
            index += 1;
        } else if let Some(rest) = arg.strip_prefix(b"-c") {
            if rest.is_empty() {
                return None;
            }
            format = Some(rest);
            index += 1;
        } else if let Some(rest) = arg.strip_prefix(b"--format=") {
            format = Some(rest);
            index += 1;
        } else if arg.starts_with(b"-") {
            return None;
        } else {
            break;
        }
    }
    if index >= args.len() {
        return None;
    }
    Some(Options {
        format,
        files: &args[index..],
    })
}

fn usage() {
    let _ = write_all(2, b"usage: stat [-c FORMAT] FILE...\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(options) = parse_options(args) else {
        usage();
        return 2;
    };

    let mut rc = 0;
    for path in options.files {
        let Some(meta) = stat_path(path) else {
            let _ = write_all(2, b"stat: cannot stat ");
            let _ = write_all(2, path);
            let _ = write_all(2, b"\n");
            rc = 1;
            continue;
        };

        if let Some(format) = options.format {
            let mut out = Vec::new();
            if !push_format(&mut out, format, path, &meta) {
                usage();
                return 2;
            }
            out.push(b'\n');
            if !write_all(1, &out) {
                return 1;
            }
        } else if !print_default(path, &meta) {
            return 1;
        }
    }
    rc
}

ristux_userland::program_main!(main);
