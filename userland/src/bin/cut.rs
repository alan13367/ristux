#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

#[derive(Clone, Copy)]
struct Range {
    start: Option<usize>,
    end: Option<usize>,
}

enum Mode {
    Fields,
    Chars,
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
    if value == 0 { None } else { Some(value) }
}

fn parse_ranges(spec: &[u8]) -> Option<Vec<Range>> {
    let mut ranges = Vec::new();
    for part in spec.split(|byte| *byte == b',') {
        if part.is_empty() {
            return None;
        }
        if let Some(dash) = part.iter().position(|byte| *byte == b'-') {
            let start = parse_usize(&part[..dash]);
            let end = parse_usize(&part[dash + 1..]);
            if start.is_none() && end.is_none() {
                return None;
            }
            if let (Some(start), Some(end)) = (start, end) {
                if end < start {
                    return None;
                }
            }
            ranges.push(Range { start, end });
        } else {
            let value = parse_usize(part)?;
            ranges.push(Range {
                start: Some(value),
                end: Some(value),
            });
        }
    }
    Some(ranges)
}

fn selected(index: usize, ranges: &[Range]) -> bool {
    ranges.iter().any(|range| {
        let after_start = range.start.is_none_or(|start| index >= start);
        let before_end = range.end.is_none_or(|end| index <= end);
        after_start && before_end
    })
}

fn read_all(fd: i32) -> Option<Vec<u8>> {
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

fn cut_fields(line: &[u8], delimiter: u8, ranges: &[Range]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut field = 1usize;
    let mut start = 0usize;
    let mut first = true;
    for index in 0..=line.len() {
        if index == line.len() || line[index] == delimiter {
            if selected(field, ranges) {
                if !first {
                    out.push(delimiter);
                }
                out.extend_from_slice(&line[start..index]);
                first = false;
            }
            field += 1;
            start = index + 1;
        }
    }
    out
}

fn cut_chars(line: &[u8], ranges: &[Range]) -> Vec<u8> {
    let mut out = Vec::new();
    for (offset, byte) in line.iter().enumerate() {
        if selected(offset + 1, ranges) {
            out.push(*byte);
        }
    }
    out
}

fn process_bytes(bytes: &[u8], mode: &Mode, delimiter: u8, ranges: &[Range]) -> i32 {
    let mut start = 0usize;
    for index in 0..=bytes.len() {
        if index != bytes.len() && bytes[index] != b'\n' {
            continue;
        }
        if index == bytes.len() && start == bytes.len() {
            break;
        }
        let line = &bytes[start..index];
        let out = match mode {
            Mode::Fields => cut_fields(line, delimiter, ranges),
            Mode::Chars => cut_chars(line, ranges),
        };
        if !write_all(1, &out) || !write_all(1, b"\n") {
            return 1;
        }
        start = index + 1;
    }
    0
}

fn process_file(path: &[u8], mode: &Mode, delimiter: u8, ranges: &[Range]) -> i32 {
    if path == b"-" {
        let Some(bytes) = read_all(0) else {
            return 1;
        };
        return process_bytes(&bytes, mode, delimiter, ranges);
    }

    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"cut: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return 1;
    }
    let bytes = read_all(fd as i32);
    let _ = sys::close(fd as i32);
    let Some(bytes) = bytes else {
        return 1;
    };
    process_bytes(&bytes, mode, delimiter, ranges)
}

fn usage() {
    let _ = write_all(
        2,
        b"usage: cut -f LIST [-d DELIM] [FILE...]\n       cut -c LIST [FILE...]\n",
    );
}

fn main(args: &[&[u8]]) -> i32 {
    let mut delimiter = b'\t';
    let mut mode: Option<Mode> = None;
    let mut ranges: Option<Vec<Range>> = None;
    let mut index = 1usize;

    while let Some(arg) = args.get(index) {
        match *arg {
            b"-d" => {
                let Some(value) = args.get(index + 1) else {
                    usage();
                    return 2;
                };
                if value.is_empty() {
                    usage();
                    return 2;
                }
                delimiter = value[0];
                index += 2;
            }
            b"-f" => {
                let Some(value) = args.get(index + 1) else {
                    usage();
                    return 2;
                };
                ranges = parse_ranges(value);
                mode = Some(Mode::Fields);
                index += 2;
            }
            b"-c" => {
                let Some(value) = args.get(index + 1) else {
                    usage();
                    return 2;
                };
                ranges = parse_ranges(value);
                mode = Some(Mode::Chars);
                index += 2;
            }
            b"--" => {
                index += 1;
                break;
            }
            _ if arg.starts_with(b"-") && arg.len() > 1 => {
                usage();
                return 2;
            }
            _ => break,
        }
    }

    let Some(mode) = mode else {
        usage();
        return 2;
    };
    let Some(ranges) = ranges else {
        usage();
        return 2;
    };

    if index >= args.len() {
        let Some(bytes) = read_all(0) else {
            return 1;
        };
        return process_bytes(&bytes, &mode, delimiter, &ranges);
    }

    let mut rc = 0;
    for path in &args[index..] {
        let file_rc = process_file(path, &mode, delimiter, &ranges);
        if file_rc != 0 {
            rc = file_rc;
        }
    }
    rc
}

ristux_userland::program_main!(main);
