#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFCHR: u32 = 0o020000;
const S_IFLNK: u32 = 0o120000;

#[derive(Clone)]
struct Entry {
    name: Vec<u8>,
    meta: Metadata,
}

#[derive(Clone, Copy)]
struct Metadata {
    nlink: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    size: i64,
}

struct Options {
    all: bool,
    directory: bool,
    long: bool,
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

fn push_unsigned(out: &mut Vec<u8>, mut value: u64, min_width: usize) {
    let mut digits = [0u8; 32];
    let mut len = 0usize;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
        if value == 0 {
            break;
        }
    }
    while len < min_width {
        digits[len] = b' ';
        len += 1;
    }
    while len > 0 {
        len -= 1;
        out.push(digits[len]);
    }
}

fn push_signed(out: &mut Vec<u8>, value: i64, min_width: usize) {
    if value < 0 {
        out.push(b'-');
        push_unsigned(out, value.wrapping_neg() as u64, min_width);
    } else {
        push_unsigned(out, value as u64, min_width);
    }
}

fn join_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    if parent == b"/" {
        let mut out = Vec::with_capacity(name.len() + 1);
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

fn is_dir(meta: &Metadata) -> bool {
    meta.mode & S_IFMT == S_IFDIR
}

fn type_char(meta: &Metadata) -> u8 {
    match meta.mode & S_IFMT {
        S_IFDIR => b'd',
        S_IFCHR => b'c',
        S_IFLNK => b'l',
        _ => b'-',
    }
}

fn push_mode(out: &mut Vec<u8>, meta: &Metadata) {
    out.push(type_char(meta));
    for (read, write, exec) in [
        (0o400, 0o200, 0o100),
        (0o040, 0o020, 0o010),
        (0o004, 0o002, 0o001),
    ] {
        out.push(if meta.mode & read != 0 { b'r' } else { b'-' });
        out.push(if meta.mode & write != 0 { b'w' } else { b'-' });
        out.push(if meta.mode & exec != 0 { b'x' } else { b'-' });
    }
}

fn read_dir_entries(path: &[u8], options: &Options) -> Option<Vec<Entry>> {
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
            let name_start = offset + 19;
            let name_end = storage[name_start..offset + reclen]
                .iter()
                .position(|&byte| byte == 0)
                .map(|pos| name_start + pos)
                .unwrap_or(offset + reclen);
            let name = &storage[name_start..name_end];
            if !name.is_empty() && (options.all || !name.starts_with(b".")) {
                let entry_path = join_path(path, name);
                if let Some(meta) = stat_path(&entry_path) {
                    out.push(Entry {
                        name: name.to_vec(),
                        meta,
                    });
                }
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    out.sort_by(|left, right| left.name.cmp(&right.name));
    Some(out)
}

fn print_name(name: &[u8]) -> bool {
    write_all(1, name) && write_all(1, b"\n")
}

fn print_long(entry: &Entry) -> bool {
    let mut out = Vec::new();
    push_mode(&mut out, &entry.meta);
    out.push(b' ');
    push_unsigned(&mut out, entry.meta.nlink, 2);
    out.push(b' ');
    push_unsigned(&mut out, entry.meta.uid as u64, 0);
    out.push(b' ');
    push_unsigned(&mut out, entry.meta.gid as u64, 0);
    out.push(b' ');
    push_signed(&mut out, entry.meta.size, 6);
    out.push(b' ');
    out.extend_from_slice(&entry.name);
    out.push(b'\n');
    write_all(1, &out)
}

fn print_entries(entries: &[Entry], options: &Options) -> i32 {
    let mut status = 0;
    for entry in entries {
        let ok = if options.long {
            print_long(entry)
        } else {
            print_name(&entry.name)
        };
        if !ok {
            status = 1;
        }
    }
    status
}

fn print_path(path: &[u8], options: &Options) -> i32 {
    let Some(meta) = stat_path(path) else {
        let _ = write_all(2, b"ls: cannot access ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return 1;
    };

    if is_dir(&meta) && !options.directory {
        return match read_dir_entries(path, options) {
            Some(entries) => print_entries(&entries, options),
            None => {
                let _ = write_all(2, b"ls: cannot read directory ");
                let _ = write_all(2, path);
                let _ = write_all(2, b"\n");
                1
            }
        };
    }

    let entry = Entry {
        name: path.to_vec(),
        meta,
    };
    print_entries(&[entry], options)
}

fn usage() {
    let _ = write_all(2, b"usage: ls [-1adl] [FILE...]\n");
}

fn parse_options(args: &[&[u8]]) -> Option<Options> {
    let mut options = Options {
        all: false,
        directory: false,
        long: false,
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
                b'1' => {}
                b'a' => options.all = true,
                b'd' => options.directory = true,
                b'l' => options.long = true,
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
    let paths = if options.paths_start < args.len() {
        &args[options.paths_start..]
    } else {
        &[b".".as_slice()][..]
    };

    let mut status = 0;
    for (index, path) in paths.iter().enumerate() {
        if paths.len() > 1 && !options.directory {
            if index > 0 {
                let _ = write_all(1, b"\n");
            }
            let _ = write_all(1, path);
            let _ = write_all(1, b":\n");
        }
        if print_path(path, &options) != 0 {
            status = 1;
        }
    }
    status
}

ristux_userland::program_main!(main);
