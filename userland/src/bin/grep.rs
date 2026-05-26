#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

struct Options<'a> {
    invert: bool,
    line_numbers: bool,
    ignore_case: bool,
    quiet: bool,
    pattern: &'a [u8],
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

fn lower(byte: u8) -> u8 {
    if byte.is_ascii_uppercase() {
        byte + 32
    } else {
        byte
    }
}

fn byte_eq(left: u8, right: u8, ignore_case: bool) -> bool {
    if ignore_case {
        lower(left) == lower(right)
    } else {
        left == right
    }
}

fn contains(line: &[u8], needle: &[u8], ignore_case: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > line.len() {
        return false;
    }
    line.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.iter())
            .all(|(left, right)| byte_eq(*left, *right, ignore_case))
    })
}

fn print_number(mut value: usize) {
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    if value == 0 {
        index -= 1;
        buf[index] = b'0';
    }
    while value > 0 {
        index -= 1;
        buf[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    let _ = write_all(1, &buf[index..]);
}

fn print_line(line: &[u8], path: Option<&[u8]>, line_number: Option<usize>) {
    if let Some(path) = path {
        let _ = write_all(1, path);
        let _ = write_all(1, b":");
    }
    if let Some(number) = line_number {
        print_number(number);
        let _ = write_all(1, b":");
    }
    let _ = write_all(1, line);
    let _ = write_all(1, b"\n");
}

fn grep_bytes(bytes: &[u8], path: Option<&[u8]>, opts: &Options) -> bool {
    let mut matched_any = false;
    let mut line_number = 1usize;
    let mut start = 0usize;
    while start <= bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());
        let mut line = &bytes[start..end];
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        let matched = contains(line, opts.pattern, opts.ignore_case) ^ opts.invert;
        if matched {
            matched_any = true;
            if opts.quiet {
                return true;
            }
            print_line(
                line,
                path,
                if opts.line_numbers {
                    Some(line_number)
                } else {
                    None
                },
            );
        }
        if end == bytes.len() {
            break;
        }
        start = end + 1;
        line_number += 1;
    }
    matched_any
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
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    Some(out)
}

fn grep_file(path: &[u8], opts: &Options, print_path: bool) -> Option<bool> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"grep: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return None;
    }
    let bytes = read_fd(fd as i32);
    let _ = sys::close(fd as i32);
    bytes.map(|bytes| grep_bytes(&bytes, print_path.then_some(path), opts))
}

fn parse_args<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    let mut invert = false;
    let mut line_numbers = false;
    let mut ignore_case = false;
    let mut quiet = false;
    let mut index = 1usize;
    while index < args.len() {
        match args[index] {
            b"-v" => invert = true,
            b"-n" => line_numbers = true,
            b"-i" => ignore_case = true,
            b"-q" => quiet = true,
            arg if arg.starts_with(b"-") => return None,
            _ => break,
        }
        index += 1;
    }
    if index >= args.len() {
        return None;
    }
    let pattern = args[index];
    index += 1;
    Some(Options {
        invert,
        line_numbers,
        ignore_case,
        quiet,
        pattern,
        files: &args[index..],
    })
}

fn usage() {
    let _ = write_all(2, b"usage: grep [-inqv] PATTERN [FILE...]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(opts) = parse_args(args) else {
        usage();
        return 2;
    };

    let mut matched = false;
    let mut errors = false;
    if opts.files.is_empty() {
        if let Some(bytes) = read_fd(0) {
            matched = grep_bytes(&bytes, None, &opts);
        } else {
            errors = true;
        }
    } else {
        let print_path = opts.files.len() > 1;
        for file in opts.files {
            match grep_file(file, &opts, print_path) {
                Some(file_matched) => matched |= file_matched,
                None => errors = true,
            }
            if opts.quiet && matched {
                break;
            }
        }
    }

    if matched {
        0
    } else if errors {
        2
    } else {
        1
    }
}

ristux_userland::program_main!(main);
