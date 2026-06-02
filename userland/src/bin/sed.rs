#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

enum Command<'a> {
    Substitute {
        pattern: &'a [u8],
        replacement: &'a [u8],
        global: bool,
    },
    Print {
        address: Option<&'a [u8]>,
    },
    Delete {
        address: Option<&'a [u8]>,
    },
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

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn parse_addressed_command(bytes: &[u8], command: u8) -> Option<Option<&[u8]>> {
    if bytes == [command] {
        return Some(None);
    }
    if bytes.first().copied() != Some(b'/') || bytes.last().copied() != Some(command) {
        return None;
    }
    let end = bytes[1..].iter().position(|byte| *byte == b'/')? + 1;
    if end + 1 != bytes.len() - 1 {
        return None;
    }
    Some(Some(&bytes[1..end]))
}

fn parse_substitute(bytes: &[u8]) -> Option<Command<'_>> {
    if bytes.len() < 4 || bytes[0] != b's' {
        return None;
    }
    let delimiter = bytes[1];
    let rest = &bytes[2..];
    let first = rest.iter().position(|byte| *byte == delimiter)?;
    let pattern = &rest[..first];
    let rest = &rest[first + 1..];
    let second = rest.iter().position(|byte| *byte == delimiter)?;
    let replacement = &rest[..second];
    let flags = &rest[second + 1..];
    if !flags.iter().all(|byte| *byte == b'g') {
        return None;
    }
    Some(Command::Substitute {
        pattern,
        replacement,
        global: flags.contains(&b'g'),
    })
}

fn parse_command(bytes: &[u8]) -> Option<Command<'_>> {
    if let Some(command) = parse_substitute(bytes) {
        return Some(command);
    }
    if let Some(address) = parse_addressed_command(bytes, b'p') {
        return Some(Command::Print { address });
    }
    if let Some(address) = parse_addressed_command(bytes, b'd') {
        return Some(Command::Delete { address });
    }
    None
}

fn substitute_line(line: &[u8], pattern: &[u8], replacement: &[u8], global: bool) -> Vec<u8> {
    if pattern.is_empty() {
        return line.to_vec();
    }

    let mut out = Vec::new();
    let mut index = 0usize;
    let mut replaced = false;
    while index < line.len() {
        if index + pattern.len() <= line.len() && &line[index..index + pattern.len()] == pattern {
            out.extend_from_slice(replacement);
            index += pattern.len();
            replaced = true;
            if !global {
                out.extend_from_slice(&line[index..]);
                return out;
            }
        } else {
            out.push(line[index]);
            index += 1;
        }
    }
    if replaced { out } else { line.to_vec() }
}

fn address_matches(line: &[u8], address: Option<&[u8]>) -> bool {
    address.is_none_or(|pattern| contains(line, pattern))
}

fn process_line(line: &[u8], command: &Command, suppress_default: bool) -> i32 {
    match command {
        Command::Substitute {
            pattern,
            replacement,
            global,
        } => {
            let out = substitute_line(line, pattern, replacement, *global);
            if suppress_default || write_all(1, &out) && write_all(1, b"\n") {
                0
            } else {
                1
            }
        }
        Command::Print { address } => {
            let matched = address_matches(line, *address);
            if !suppress_default {
                if !write_all(1, line) || !write_all(1, b"\n") {
                    return 1;
                }
            }
            if matched {
                if !write_all(1, line) || !write_all(1, b"\n") {
                    return 1;
                }
            }
            0
        }
        Command::Delete { address } => {
            if address_matches(line, *address) {
                return 0;
            }
            if suppress_default || write_all(1, line) && write_all(1, b"\n") {
                0
            } else {
                1
            }
        }
    }
}

fn process_bytes(bytes: &[u8], command: &Command, suppress_default: bool) -> i32 {
    let mut start = 0usize;
    for index in 0..=bytes.len() {
        if index != bytes.len() && bytes[index] != b'\n' {
            continue;
        }
        if index == bytes.len() && start == bytes.len() {
            break;
        }
        let mut line = &bytes[start..index];
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        let rc = process_line(line, command, suppress_default);
        if rc != 0 {
            return rc;
        }
        start = index + 1;
    }
    0
}

fn read_fd(fd: i32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 {
            return Some(out);
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}

fn process_file(path: &[u8], command: &Command, suppress_default: bool) -> i32 {
    if path == b"-" {
        let Some(bytes) = read_fd(0) else {
            return 1;
        };
        return process_bytes(&bytes, command, suppress_default);
    }

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"sed: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return 1;
    }
    let bytes = read_fd(fd as i32);
    let _ = sys::close(fd as i32);
    let Some(bytes) = bytes else {
        return 1;
    };
    process_bytes(&bytes, command, suppress_default)
}

fn usage() {
    let _ = write_all(2, b"usage: sed [-n] SCRIPT [FILE...]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut suppress_default = false;
    let mut index = 1usize;
    if args.get(index).is_some_and(|arg| *arg == b"-n") {
        suppress_default = true;
        index += 1;
    }

    let Some(script) = args.get(index) else {
        usage();
        return 2;
    };
    let Some(command) = parse_command(script) else {
        usage();
        return 2;
    };
    index += 1;

    if index >= args.len() {
        let Some(bytes) = read_fd(0) else {
            return 1;
        };
        return process_bytes(&bytes, &command, suppress_default);
    }

    let mut rc = 0;
    for path in &args[index..] {
        let file_rc = process_file(path, &command, suppress_default);
        if file_rc != 0 {
            rc = file_rc;
        }
    }
    rc
}

ristux_userland::program_main!(main);
