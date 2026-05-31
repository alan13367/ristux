#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const NR_CHOWN: usize = 92;
const O_RDONLY: i32 = 0;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

struct OwnerSpec {
    uid: u32,
    gid: u32,
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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 256];
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

fn for_lines(bytes: &[u8], mut f: impl FnMut(&[u8])) {
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            let mut line = &bytes[start..index];
            if line.ends_with(b"\r") {
                line = &line[..line.len() - 1];
            }
            f(line);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        f(&bytes[start..]);
    }
}

fn field<'a>(line: &'a [u8], wanted: usize) -> Option<&'a [u8]> {
    let mut start = 0usize;
    let mut index = 0usize;
    for (pos, byte) in line.iter().enumerate() {
        if *byte == b':' {
            if index == wanted {
                return Some(&line[start..pos]);
            }
            index += 1;
            start = pos + 1;
        }
    }
    if index == wanted {
        Some(&line[start..])
    } else {
        None
    }
}

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

fn lookup_passwd(name: &[u8]) -> Option<(u32, u32)> {
    let bytes = read_file(b"/etc/passwd")?;
    let mut out = None;
    for_lines(&bytes, |line| {
        if out.is_some() || field(line, 0) != Some(name) {
            return;
        }
        let Some(uid) = field(line, 2).and_then(parse_u32) else {
            return;
        };
        let Some(gid) = field(line, 3).and_then(parse_u32) else {
            return;
        };
        out = Some((uid, gid));
    });
    out
}

fn lookup_group(name: &[u8]) -> Option<u32> {
    let bytes = read_file(b"/etc/group")?;
    let mut out = None;
    for_lines(&bytes, |line| {
        if out.is_some() || field(line, 0) != Some(name) {
            return;
        }
        out = field(line, 2).and_then(parse_u32);
    });
    out
}

fn parse_user(bytes: &[u8]) -> Option<(u32, Option<u32>)> {
    if let Some(uid) = parse_u32(bytes) {
        return Some((uid, None));
    }
    lookup_passwd(bytes).map(|(uid, gid)| (uid, Some(gid)))
}

fn parse_group(bytes: &[u8]) -> Option<u32> {
    parse_u32(bytes).or_else(|| lookup_group(bytes))
}

fn parse_owner_spec(spec: &[u8]) -> Option<OwnerSpec> {
    let split = spec.iter().position(|byte| *byte == b':' || *byte == b'.');
    match split {
        Some(0) => Some(OwnerSpec {
            uid: u32::MAX,
            gid: parse_group(&spec[1..])?,
        }),
        Some(pos) => {
            let (uid, default_gid) = parse_user(&spec[..pos])?;
            let group = &spec[pos + 1..];
            let gid = if group.is_empty() {
                default_gid.unwrap_or(u32::MAX)
            } else {
                parse_group(group)?
            };
            Some(OwnerSpec { uid, gid })
        }
        None => {
            let (uid, _) = parse_user(spec)?;
            Some(OwnerSpec { uid, gid: u32::MAX })
        }
    }
}

fn chown_path(path: &[u8], spec: &OwnerSpec) -> bool {
    let path_c = cstr(path);
    unsafe {
        sys::syscall3(
            NR_CHOWN,
            path_c.as_ptr() as usize,
            spec.uid as usize,
            spec.gid as usize,
        ) >= 0
    }
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

fn read_dir_entries(path: &[u8]) -> Option<Vec<Vec<u8>>> {
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
            if name != b"." && name != b".." && !name.is_empty() {
                out.push(name.to_vec());
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    out.sort();
    Some(out)
}

fn chown_recursive(path: &[u8], spec: &OwnerSpec) -> bool {
    let mut ok = chown_path(path, spec);
    if !is_dir(path) {
        return ok;
    }
    let Some(entries) = read_dir_entries(path) else {
        return false;
    };
    for name in entries {
        let child = join_path(path, &name);
        if !chown_recursive(&child, spec) {
            ok = false;
        }
    }
    ok
}

fn usage() {
    let _ = write_all(2, b"usage: chown [-R] OWNER[:GROUP] FILE...\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut recursive = false;
    let mut index = 1usize;
    while index < args.len() && args[index].starts_with(b"-") {
        if args[index] == b"-R" {
            recursive = true;
            index += 1;
        } else {
            usage();
            return 2;
        }
    }
    if args.len().saturating_sub(index) < 2 {
        usage();
        return 2;
    }
    let Some(spec) = parse_owner_spec(args[index]) else {
        let _ = write_all(2, b"chown: invalid owner\n");
        return 1;
    };
    index += 1;

    let mut status = 0;
    while index < args.len() {
        let ok = if recursive {
            chown_recursive(args[index], &spec)
        } else {
            chown_path(args[index], &spec)
        };
        if !ok {
            let _ = write_all(2, b"chown: cannot change ownership of ");
            let _ = write_all(2, args[index]);
            let _ = write_all(2, b"\n");
            status = 1;
        }
        index += 1;
    }
    status
}

ristux_userland::program_main!(main);
