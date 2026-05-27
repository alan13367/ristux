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

enum HunkLine<'a> {
    Context(&'a [u8]),
    Remove(&'a [u8]),
    Add(&'a [u8]),
}

struct Hunk<'a> {
    old_start: usize,
    old_count: usize,
    lines: Vec<HunkLine<'a>>,
}

struct FilePatch<'a> {
    old_path: Option<&'a [u8]>,
    new_path: Option<&'a [u8]>,
    hunks: Vec<Hunk<'a>>,
}

struct Options<'a> {
    strip: usize,
    input: Option<&'a [u8]>,
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

fn trim(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let bytes = read_fd(fd as i32);
    let _ = sys::close(fd as i32);
    bytes
}

fn write_file(path: &[u8], lines: &[Vec<u8>]) -> bool {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return false;
    }
    let mut ok = true;
    for line in lines {
        if !write_all(fd as i32, line) || !write_all(fd as i32, b"\n") {
            ok = false;
            break;
        }
    }
    let _ = sys::close(fd as i32);
    ok
}

fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
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
        lines.push(line);
        start = index + 1;
    }
    lines
}

fn split_owned_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    split_lines(bytes)
        .iter()
        .map(|line| line.to_vec())
        .collect()
}

fn parse_options<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    let mut strip = 0usize;
    let mut input = None;
    let mut index = 1usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"-p" {
            index += 1;
            strip = parse_usize(args.get(index)?)?;
            index += 1;
        } else if arg.len() > 2 && arg.starts_with(b"-p") {
            strip = parse_usize(&arg[2..])?;
            index += 1;
        } else if arg == b"-i" {
            index += 1;
            input = Some(*args.get(index)?);
            index += 1;
        } else if arg.starts_with(b"-") {
            return None;
        } else if input.is_none() {
            input = Some(arg);
            index += 1;
        } else {
            return None;
        }
    }
    Some(Options { strip, input })
}

fn header_path<'a>(line: &'a [u8], marker: &[u8]) -> Option<Option<&'a [u8]>> {
    if !line.starts_with(marker) {
        return None;
    }
    let rest = trim(&line[marker.len()..]);
    let end = rest
        .iter()
        .position(|byte| byte.is_ascii_whitespace())
        .unwrap_or(rest.len());
    let path = &rest[..end];
    if path == b"/dev/null" {
        Some(None)
    } else if path.is_empty() {
        None
    } else {
        Some(Some(path))
    }
}

fn strip_components(path: &[u8], strip: usize) -> Option<Vec<u8>> {
    let mut start = 0usize;
    for _ in 0..strip {
        let rel = path[start..].iter().position(|byte| *byte == b'/')?;
        start += rel + 1;
    }
    while start < path.len() && path[start] == b'/' {
        start += 1;
    }
    if start >= path.len() {
        None
    } else {
        Some(path[start..].to_vec())
    }
}

fn parse_range(bytes: &[u8], index: &mut usize, sign: u8) -> Option<(usize, usize)> {
    if bytes.get(*index).copied()? != sign {
        return None;
    }
    *index += 1;
    let start_index = *index;
    while bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
        *index += 1;
    }
    let start = parse_usize(&bytes[start_index..*index])?;
    let mut count = 1usize;
    if bytes.get(*index) == Some(&b',') {
        *index += 1;
        let count_index = *index;
        while bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
            *index += 1;
        }
        count = parse_usize(&bytes[count_index..*index])?;
    }
    Some((start, count))
}

fn parse_hunk_header(line: &[u8]) -> Option<(usize, usize, usize)> {
    if !line.starts_with(b"@@") {
        return None;
    }
    let mut index = 2usize;
    while line.get(index) == Some(&b' ') {
        index += 1;
    }
    let (old_start, old_count) = parse_range(line, &mut index, b'-')?;
    while line.get(index) == Some(&b' ') {
        index += 1;
    }
    let (_new_start, new_count) = parse_range(line, &mut index, b'+')?;
    Some((old_start, old_count, new_count))
}

fn parse_hunk<'a>(lines: &[&'a [u8]], index: &mut usize) -> Option<Hunk<'a>> {
    let (old_start, old_count, new_count) = parse_hunk_header(lines.get(*index)?)?;
    *index += 1;

    let mut hunk_lines = Vec::new();
    let mut seen_old = 0usize;
    let mut seen_new = 0usize;
    while let Some(line) = lines.get(*index).copied() {
        if line.starts_with(b"@@") || line.starts_with(b"--- ") {
            break;
        }
        *index += 1;
        if line.starts_with(b"\\") {
            continue;
        }
        let Some((&kind, content)) = line.split_first() else {
            return None;
        };
        match kind {
            b' ' => {
                hunk_lines.push(HunkLine::Context(content));
                seen_old += 1;
                seen_new += 1;
            }
            b'-' => {
                hunk_lines.push(HunkLine::Remove(content));
                seen_old += 1;
            }
            b'+' => {
                hunk_lines.push(HunkLine::Add(content));
                seen_new += 1;
            }
            _ => return None,
        }
    }

    if seen_old != old_count || seen_new != new_count {
        return None;
    }
    Some(Hunk {
        old_start,
        old_count,
        lines: hunk_lines,
    })
}

fn parse_patch(bytes: &[u8]) -> Option<Vec<FilePatch<'_>>> {
    let lines = split_lines(bytes);
    let mut patches = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        while index < lines.len() && !lines[index].starts_with(b"--- ") {
            index += 1;
        }
        if index >= lines.len() {
            break;
        }

        let old_path = header_path(lines[index], b"--- ")?;
        index += 1;
        let new_path = header_path(*lines.get(index)?, b"+++ ")?;
        index += 1;

        let mut hunks = Vec::new();
        while index < lines.len() {
            if lines[index].starts_with(b"--- ") {
                break;
            }
            if lines[index].starts_with(b"@@") {
                hunks.push(parse_hunk(&lines, &mut index)?);
            } else {
                index += 1;
            }
        }
        if hunks.is_empty() {
            return None;
        }
        patches.push(FilePatch {
            old_path,
            new_path,
            hunks,
        });
    }

    if patches.is_empty() {
        None
    } else {
        Some(patches)
    }
}

fn target_path(file_patch: &FilePatch, strip: usize) -> Option<Vec<u8>> {
    if let Some(path) = file_patch.new_path.or(file_patch.old_path) {
        strip_components(path, strip)
    } else {
        None
    }
}

fn hunk_base(hunk: &Hunk) -> usize {
    if hunk.old_count == 0 || hunk.old_start == 0 {
        hunk.old_start
    } else {
        hunk.old_start - 1
    }
}

fn apply_hunks(input: &[Vec<u8>], hunks: &[Hunk]) -> Option<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for hunk in hunks {
        let base = hunk_base(hunk);
        if base < cursor || base > input.len() {
            return None;
        }
        while cursor < base {
            out.push(input[cursor].clone());
            cursor += 1;
        }

        let mut source = base;
        for line in &hunk.lines {
            match line {
                HunkLine::Context(bytes) => {
                    if input.get(source).map(|line| line.as_slice()) != Some(*bytes) {
                        return None;
                    }
                    out.push((*bytes).to_vec());
                    source += 1;
                }
                HunkLine::Remove(bytes) => {
                    if input.get(source).map(|line| line.as_slice()) != Some(*bytes) {
                        return None;
                    }
                    source += 1;
                }
                HunkLine::Add(bytes) => out.push((*bytes).to_vec()),
            }
        }
        cursor = source;
    }
    while cursor < input.len() {
        out.push(input[cursor].clone());
        cursor += 1;
    }
    Some(out)
}

fn apply_file_patch(file_patch: &FilePatch, strip: usize) -> i32 {
    let Some(target) = target_path(file_patch, strip) else {
        let _ = write_all(2, b"patch: cannot determine target path\n");
        return 1;
    };

    let input = if file_patch.old_path.is_none() {
        Vec::new()
    } else {
        let Some(bytes) = read_file(&target) else {
            let _ = write_all(2, b"patch: cannot open ");
            let _ = write_all(2, &target);
            let _ = write_all(2, b"\n");
            return 1;
        };
        split_owned_lines(&bytes)
    };

    let Some(output) = apply_hunks(&input, &file_patch.hunks) else {
        let _ = write_all(2, b"patch: hunk failed for ");
        let _ = write_all(2, &target);
        let _ = write_all(2, b"\n");
        return 1;
    };

    let _ = write_all(1, b"patching file ");
    let _ = write_all(1, &target);
    let _ = write_all(1, b"\n");

    if write_file(&target, &output) {
        0
    } else {
        let _ = write_all(2, b"patch: cannot write ");
        let _ = write_all(2, &target);
        let _ = write_all(2, b"\n");
        1
    }
}

fn usage() {
    let _ = write_all(2, b"usage: patch [-pN] [-i PATCHFILE]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(opts) = parse_options(args) else {
        usage();
        return 2;
    };
    let patch_bytes = if let Some(input) = opts.input {
        let Some(bytes) = read_file(input) else {
            let _ = write_all(2, b"patch: cannot open patch file\n");
            return 1;
        };
        bytes
    } else {
        let Some(bytes) = read_fd(0) else {
            return 1;
        };
        bytes
    };

    let Some(patches) = parse_patch(&patch_bytes) else {
        let _ = write_all(2, b"patch: unsupported or malformed patch\n");
        return 2;
    };

    let mut rc = 0;
    for file_patch in &patches {
        let file_rc = apply_file_patch(file_patch, opts.strip);
        if file_rc != 0 {
            rc = file_rc;
            break;
        }
    }
    rc
}

ristux_userland::program_main!(main);
