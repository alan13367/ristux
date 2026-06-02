#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

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

fn usage() {
    let _ = write_all(2, b"usage: tr [-d] [-s] SET1 [SET2]\n");
}

fn push_range(out: &mut Vec<u8>, start: u8, end: u8) {
    let mut byte = start;
    loop {
        out.push(byte);
        if byte == end {
            break;
        }
        byte = byte.wrapping_add(1);
    }
}

fn push_class(out: &mut Vec<u8>, class: &[u8]) -> bool {
    match class {
        b"alnum" => {
            push_range(out, b'0', b'9');
            push_range(out, b'A', b'Z');
            push_range(out, b'a', b'z');
        }
        b"alpha" => {
            push_range(out, b'A', b'Z');
            push_range(out, b'a', b'z');
        }
        b"digit" => push_range(out, b'0', b'9'),
        b"lower" => push_range(out, b'a', b'z'),
        b"upper" => push_range(out, b'A', b'Z'),
        b"space" => out.extend_from_slice(b" \t\r\n\x0b\x0c"),
        _ => return false,
    }
    true
}

fn parse_class(bytes: &[u8], index: usize, out: &mut Vec<u8>) -> Option<usize> {
    if bytes.get(index) != Some(&b'[') || bytes.get(index + 1) != Some(&b':') {
        return None;
    }
    let mut end = index + 2;
    while end + 1 < bytes.len() {
        if bytes[end] == b':' && bytes[end + 1] == b']' {
            if push_class(out, &bytes[index + 2..end]) {
                return Some(end + 2 - index);
            }
            return None;
        }
        end += 1;
    }
    None
}

fn parse_item(bytes: &[u8], index: usize) -> Option<(u8, usize)> {
    let byte = *bytes.get(index)?;
    if byte != b'\\' {
        return Some((byte, 1));
    }
    let escaped = *bytes.get(index + 1)?;
    match escaped {
        b'n' => Some((b'\n', 2)),
        b'r' => Some((b'\r', 2)),
        b't' => Some((b'\t', 2)),
        b'\\' => Some((b'\\', 2)),
        b'0'..=b'7' => {
            let mut value = escaped - b'0';
            let mut consumed = 2usize;
            while consumed < 4 {
                let Some(next) = bytes.get(index + consumed).copied() else {
                    break;
                };
                if !(b'0'..=b'7').contains(&next) {
                    break;
                }
                value = value.saturating_mul(8).saturating_add(next - b'0');
                consumed += 1;
            }
            Some((value, consumed))
        }
        other => Some((other, 2)),
    }
}

fn expand_set(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if let Some(consumed) = parse_class(bytes, index, &mut out) {
            index += consumed;
            continue;
        }

        let (start, start_len) = parse_item(bytes, index)?;
        if index + start_len + 1 < bytes.len() && bytes[index + start_len] == b'-' {
            let range_end_index = index + start_len + 1;
            let (end, end_len) = parse_item(bytes, range_end_index)?;
            if start <= end {
                push_range(&mut out, start, end);
                index = range_end_index + end_len;
                continue;
            }
        }

        out.push(start);
        index += start_len;
    }
    Some(out)
}

fn set_contains(set: &[bool; 256], byte: u8) -> bool {
    set[byte as usize]
}

fn mark_set(bytes: &[u8]) -> [bool; 256] {
    let mut set = [false; 256];
    for byte in bytes {
        set[*byte as usize] = true;
    }
    set
}

fn build_translation(set1: &[u8], set2: &[u8]) -> ([bool; 256], [u8; 256]) {
    let mut present = [false; 256];
    let mut map = [0u8; 256];
    if set2.is_empty() {
        return (present, map);
    }
    for (index, byte) in set1.iter().enumerate() {
        let slot = *byte as usize;
        if present[slot] {
            continue;
        }
        let replacement = set2[core::cmp::min(index, set2.len() - 1)];
        present[slot] = true;
        map[slot] = replacement;
    }
    (present, map)
}

fn parse_options(args: &[&[u8]]) -> Option<(bool, bool, usize)> {
    let mut delete = false;
    let mut squeeze = false;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            return Some((delete, squeeze, index + 1));
        }
        if *arg == b"-" || !arg.starts_with(b"-") {
            break;
        }
        for option in &arg[1..] {
            match *option {
                b'd' => delete = true,
                b's' => squeeze = true,
                _ => return None,
            }
        }
        index += 1;
    }
    Some((delete, squeeze, index))
}

fn run(delete: bool, squeeze: bool, set1: &[u8], set2: Option<&[u8]>) -> i32 {
    let delete_set = if delete { mark_set(set1) } else { [false; 256] };
    let squeeze_source = if delete && squeeze {
        set2.unwrap_or(set1)
    } else if squeeze {
        set2.unwrap_or(set1)
    } else {
        b""
    };
    let squeeze_set = mark_set(squeeze_source);
    let translate = !delete && set2.is_some();
    let (translate_present, translate_map) = if let Some(set2) = set2 {
        build_translation(set1, set2)
    } else {
        ([false; 256], [0u8; 256])
    };

    let mut prev: Option<u8> = None;
    let mut input = [0u8; 512];
    let mut output = [0u8; 512];
    loop {
        let n = sys::read(0, &mut input);
        if n < 0 {
            return 1;
        }
        if n == 0 {
            return 0;
        }

        let mut out_len = 0usize;
        for byte in &input[..n as usize] {
            if set_contains(&delete_set, *byte) {
                continue;
            }

            let mut out = *byte;
            if translate && set_contains(&translate_present, *byte) {
                out = translate_map[*byte as usize];
            }

            if squeeze && prev == Some(out) && set_contains(&squeeze_set, out) {
                continue;
            }
            prev = Some(out);

            output[out_len] = out;
            out_len += 1;
            if out_len == output.len() {
                if !write_all(1, &output) {
                    return 1;
                }
                out_len = 0;
            }
        }
        if out_len > 0 && !write_all(1, &output[..out_len]) {
            return 1;
        }
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let Some((delete, squeeze, index)) = parse_options(args) else {
        usage();
        return 2;
    };
    let remaining = args.len().saturating_sub(index);
    if remaining == 0 || remaining > 2 || (!delete && !squeeze && remaining != 2) {
        usage();
        return 2;
    }
    if delete && !squeeze && remaining != 1 {
        usage();
        return 2;
    }

    let Some(set1) = expand_set(args[index]) else {
        usage();
        return 2;
    };
    let set2 = if remaining == 2 {
        let Some(expanded) = expand_set(args[index + 1]) else {
            usage();
            return 2;
        };
        if !delete && expanded.is_empty() {
            usage();
            return 2;
        }
        Some(expanded)
    } else {
        None
    };

    run(delete, squeeze, &set1, set2.as_deref())
}

ristux_userland::program_main!(main);
