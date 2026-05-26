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
const ARMAG: &[u8] = b"!<arch>\n";
const HEADER_LEN: usize = 60;

#[derive(Clone)]
struct Member {
    name: Vec<u8>,
    data: Vec<u8>,
    mode: usize,
}

enum Command {
    Replace,
    List,
    Extract,
}

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let written = sys::write(fd, bytes);
        if written <= 0 {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
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
        let read = sys::read(fd as i32, &mut buf);
        if read < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn print_err(prefix: &[u8], value: &[u8]) {
    let _ = sys::write(2, b"ar: ");
    let _ = sys::write(2, prefix);
    let _ = sys::write(2, value);
    let _ = sys::write(2, b"\n");
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn parse_number(field: &[u8], base: usize) -> Option<usize> {
    let mut value = 0usize;
    let mut seen = false;
    for &byte in trim_ascii(field) {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as usize,
            _ => return None,
        };
        if digit >= base {
            return None;
        }
        seen = true;
        value = value.checked_mul(base)?.checked_add(digit)?;
    }
    if seen { Some(value) } else { Some(0) }
}

fn member_name(header: &[u8]) -> Option<Vec<u8>> {
    let mut name = trim_ascii(&header[..16]);
    if name == b"/" || name == b"//" {
        return Some(Vec::new());
    }
    if name.starts_with(b"#1/") {
        return None;
    }
    if name.ends_with(b"/") {
        name = &name[..name.len() - 1];
    }
    if !safe_name(name) {
        return None;
    }
    Some(name.to_vec())
}

fn safe_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name != b"."
        && name != b".."
        && !name.contains(&b'/')
        && !name.contains(&0)
}

fn basename(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|byte| *byte == b'/')
        .map(|index| &path[index + 1..])
        .unwrap_or(path)
}

fn parse_archive(bytes: &[u8]) -> Option<Vec<Member>> {
    if !bytes.starts_with(ARMAG) {
        return None;
    }
    let mut members = Vec::new();
    let mut offset = ARMAG.len();
    while offset + HEADER_LEN <= bytes.len() {
        let header = &bytes[offset..offset + HEADER_LEN];
        if &header[58..60] != b"`\n" {
            return None;
        }
        let name = member_name(header)?;
        let mode = parse_number(&header[40..48], 8)?;
        let size = parse_number(&header[48..58], 10)?;
        offset += HEADER_LEN;
        let end = offset.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        if !name.is_empty() {
            members.push(Member {
                name,
                data: bytes[offset..end].to_vec(),
                mode,
            });
        }
        offset = end + (size & 1);
    }
    Some(members)
}

fn write_field(field: &mut [u8], value: &[u8]) -> bool {
    if value.len() > field.len() {
        return false;
    }
    field.fill(b' ');
    field[..value.len()].copy_from_slice(value);
    true
}

fn write_usize_field(field: &mut [u8], mut value: usize, base: usize) -> bool {
    let mut buf = [0u8; 32];
    let mut pos = buf.len();
    loop {
        pos -= 1;
        buf[pos] = b'0' + (value % base) as u8;
        value /= base;
        if value == 0 {
            break;
        }
    }
    write_field(field, &buf[pos..])
}

fn append_member(archive: &mut Vec<u8>, member: &Member) -> bool {
    if member.name.len() + 1 > 16 || !safe_name(&member.name) {
        return false;
    }
    let mut header = [b' '; HEADER_LEN];
    let mut name = member.name.clone();
    name.push(b'/');
    if !write_field(&mut header[0..16], &name)
        || !write_usize_field(&mut header[16..28], 0, 10)
        || !write_usize_field(&mut header[28..34], 0, 10)
        || !write_usize_field(&mut header[34..40], 0, 10)
        || !write_usize_field(&mut header[40..48], member.mode, 8)
        || !write_usize_field(&mut header[48..58], member.data.len(), 10)
    {
        return false;
    }
    header[58] = b'`';
    header[59] = b'\n';
    archive.extend_from_slice(&header);
    archive.extend_from_slice(&member.data);
    if member.data.len() & 1 != 0 {
        archive.push(b'\n');
    }
    true
}

fn write_archive(path: &[u8], members: &[Member]) -> bool {
    let mut archive = Vec::new();
    archive.extend_from_slice(ARMAG);
    for member in members {
        if !append_member(&mut archive, member) {
            print_err(b"unsupported member name ", &member.name);
            return false;
        }
    }
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return false;
    }
    let ok = write_all(fd as i32, &archive);
    let _ = sys::close(fd as i32);
    ok
}

fn load_archive(path: &[u8]) -> Vec<Member> {
    read_file(path)
        .and_then(|bytes| parse_archive(&bytes))
        .unwrap_or_default()
}

fn replace_members(archive: &[u8], files: &[&[u8]]) -> i32 {
    if files.is_empty() {
        let _ = sys::write(2, b"ar: no input files\n");
        return 1;
    }
    let mut members = load_archive(archive);
    for file in files {
        let name = basename(file);
        if name.len() + 1 > 16 || !safe_name(name) {
            print_err(b"unsupported member name ", name);
            return 1;
        }
        let Some(data) = read_file(file) else {
            print_err(b"cannot read ", file);
            return 1;
        };
        let member = Member {
            name: name.to_vec(),
            data,
            mode: 0o100644,
        };
        if let Some(existing) = members.iter_mut().find(|entry| entry.name == member.name) {
            *existing = member;
        } else {
            members.push(member);
        }
    }
    if write_archive(archive, &members) {
        0
    } else {
        print_err(b"cannot write ", archive);
        1
    }
}

fn list_members(archive: &[u8]) -> i32 {
    let Some(bytes) = read_file(archive) else {
        print_err(b"cannot open ", archive);
        return 1;
    };
    let Some(members) = parse_archive(&bytes) else {
        print_err(b"invalid archive ", archive);
        return 1;
    };
    for member in members {
        let _ = write_all(1, &member.name);
        let _ = write_all(1, b"\n");
    }
    0
}

fn extract_members(archive: &[u8]) -> i32 {
    let Some(bytes) = read_file(archive) else {
        print_err(b"cannot open ", archive);
        return 1;
    };
    let Some(members) = parse_archive(&bytes) else {
        print_err(b"invalid archive ", archive);
        return 1;
    };
    for member in members {
        let name_c = cstr(&member.name);
        let fd = sys::open(name_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
        if fd < 0 {
            print_err(b"cannot extract ", &member.name);
            return 1;
        }
        let ok = write_all(fd as i32, &member.data);
        let _ = sys::close(fd as i32);
        if !ok {
            print_err(b"cannot write ", &member.name);
            return 1;
        }
    }
    0
}

fn parse_command(flags: &[u8]) -> Option<Command> {
    let flags = flags.strip_prefix(b"-").unwrap_or(flags);
    if flags.contains(&b'r') {
        Some(Command::Replace)
    } else if flags.contains(&b't') {
        Some(Command::List)
    } else if flags.contains(&b'x') {
        Some(Command::Extract)
    } else {
        None
    }
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() < 3 {
        let _ = sys::write(2, b"usage: ar rcs ARCHIVE FILE... | ar t ARCHIVE | ar x ARCHIVE\n");
        return 2;
    }
    let Some(command) = parse_command(args[1]) else {
        let _ = sys::write(2, b"ar: unsupported command\n");
        return 2;
    };
    match command {
        Command::Replace => replace_members(args[2], &args[3..]),
        Command::List => list_members(args[2]),
        Command::Extract => extract_members(args[2]),
    }
}

ristux_userland::program_main!(main);
