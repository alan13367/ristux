#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const DT_DIR: u8 = 4;
const DIRENT64_HEADER: usize = 19;

#[derive(Default)]
struct ProcStatus {
    pid: u64,
    parent: Option<u64>,
    name: Vec<u8>,
    state: Vec<u8>,
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
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
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

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u64)?;
    }
    Some(value)
}

fn is_numeric(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|byte| byte.is_ascii_digit())
}

fn for_lines(bytes: &[u8], mut f: impl FnMut(&[u8])) {
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            f(&bytes[start..index]);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        f(&bytes[start..]);
    }
}

fn field<'a>(line: &'a [u8], label: &[u8]) -> Option<&'a [u8]> {
    let rest = line.strip_prefix(label)?;
    let rest = rest.strip_prefix(b":")?;
    let start = rest.iter().position(|byte| !byte.is_ascii_whitespace())?;
    Some(&rest[start..])
}

fn parse_status(bytes: &[u8]) -> Option<ProcStatus> {
    let mut status = ProcStatus::default();
    for_lines(bytes, |line| {
        if let Some(value) = field(line, b"pid") {
            status.pid = parse_u64(value).unwrap_or(0);
        } else if let Some(value) = field(line, b"name") {
            status.name.clear();
            status.name.extend_from_slice(value);
        } else if let Some(value) = field(line, b"state") {
            status.state.clear();
            status.state.extend_from_slice(value);
        } else if let Some(value) = field(line, b"parent") {
            status.parent = parse_u64(value);
        }
    });
    if status.pid == 0 || status.name.is_empty() || status.state.is_empty() {
        None
    } else {
        Some(status)
    }
}

fn read_proc_status(pid_name: &[u8]) -> Option<ProcStatus> {
    let mut path = Vec::new();
    path.extend_from_slice(b"/proc/");
    path.extend_from_slice(pid_name);
    path.extend_from_slice(b"/status");
    read_file(&path).and_then(|bytes| parse_status(&bytes))
}

fn push_u64(out: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
        if value == 0 {
            break;
        }
    }
    while len > 0 {
        len -= 1;
        out.push(digits[len]);
    }
}

fn push_padded_u64(out: &mut Vec<u8>, value: u64, width: usize) {
    let start = out.len();
    push_u64(out, value);
    let len = out.len() - start;
    if len < width {
        let pad = width - len;
        out.resize(out.len() + pad, b' ');
        out.copy_within(start..start + len, start + pad);
        for byte in &mut out[start..start + pad] {
            *byte = b' ';
        }
    }
}

fn push_padded_bytes(out: &mut Vec<u8>, bytes: &[u8], width: usize) {
    out.extend_from_slice(bytes);
    if bytes.len() < width {
        out.resize(out.len() + width - bytes.len(), b' ');
    }
}

fn append_status(out: &mut Vec<u8>, status: &ProcStatus) {
    push_padded_u64(out, status.pid, 5);
    out.push(b' ');
    push_padded_u64(out, status.parent.unwrap_or(0), 5);
    out.push(b' ');
    push_padded_bytes(out, &status.state, 8);
    out.push(b' ');
    out.extend_from_slice(&status.name);
    out.push(b'\n');
}

fn collect_statuses() -> Vec<ProcStatus> {
    let proc_c = cstr(b"/proc");
    let fd = sys::open(proc_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return Vec::new();
    }
    let mut statuses = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::getdents64(fd as i32, &mut buf);
        if n < 0 {
            statuses.clear();
            break;
        }
        if n == 0 {
            break;
        }
        let mut offset = 0usize;
        while offset + DIRENT64_HEADER <= n as usize {
            let reclen = u16::from_le_bytes([buf[offset + 16], buf[offset + 17]]) as usize;
            if reclen == 0 || offset + reclen > n as usize {
                break;
            }
            let dtype = buf[offset + 18];
            let name_start = offset + DIRENT64_HEADER;
            let name_end = (name_start..offset + reclen)
                .find(|index| buf[*index] == 0)
                .unwrap_or(offset + reclen);
            let name = &buf[name_start..name_end];
            if dtype == DT_DIR && is_numeric(name) {
                if let Some(status) = read_proc_status(name) {
                    statuses.push(status);
                }
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    statuses
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() != 1 {
        let _ = write_all(2, b"usage: ps\n");
        return 2;
    }
    let mut statuses = collect_statuses();
    if statuses.is_empty() {
        let _ = write_all(2, b"ps: cannot read /proc\n");
        return 1;
    }
    statuses.sort_by(|a, b| a.pid.cmp(&b.pid));

    let mut out = Vec::new();
    out.extend_from_slice(b"  PID  PPID STATE    COMMAND\n");
    for status in &statuses {
        append_status(&mut out, status);
    }
    if write_all(1, &out) { 0 } else { 1 }
}

ristux_userland::program_main!(main);
