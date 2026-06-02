#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

#[derive(Clone, Copy, Default)]
struct Counts {
    lines: usize,
    words: usize,
    bytes: usize,
}

#[derive(Clone, Copy)]
struct Options {
    lines: bool,
    words: bool,
    bytes: bool,
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

fn count_fd(fd: i32) -> Option<Counts> {
    let mut counts = Counts::default();
    let mut in_word = false;
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 {
            break;
        }
        counts.bytes += n as usize;
        for byte in &buf[..n as usize] {
            if *byte == b'\n' {
                counts.lines += 1;
            }
            if byte.is_ascii_whitespace() {
                in_word = false;
            } else if !in_word {
                counts.words += 1;
                in_word = true;
            }
        }
    }
    Some(counts)
}

fn count_file(path: &[u8]) -> Option<Counts> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = write_all(2, b"wc: cannot open ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        return None;
    }
    let counts = count_fd(fd as i32);
    let _ = sys::close(fd as i32);
    counts
}

fn print_counts(counts: Counts, opts: Options, label: Option<&[u8]>) {
    let mut first = true;
    if opts.lines {
        print_number(counts.lines);
        first = false;
    }
    if opts.words {
        if !first {
            let _ = write_all(1, b" ");
        }
        print_number(counts.words);
        first = false;
    }
    if opts.bytes {
        if !first {
            let _ = write_all(1, b" ");
        }
        print_number(counts.bytes);
    }
    if let Some(label) = label {
        let _ = write_all(1, b" ");
        let _ = write_all(1, label);
    }
    let _ = write_all(1, b"\n");
}

fn parse_options(args: &[&[u8]]) -> Option<(Options, usize)> {
    let mut opts = Options {
        lines: false,
        words: false,
        bytes: false,
    };
    let mut index = 1usize;
    while index < args.len() && args[index].starts_with(b"-") {
        if args[index] == b"--" {
            index += 1;
            break;
        }
        for flag in &args[index][1..] {
            match *flag {
                b'l' => opts.lines = true,
                b'w' => opts.words = true,
                b'c' => opts.bytes = true,
                _ => return None,
            }
        }
        index += 1;
    }
    if !opts.lines && !opts.words && !opts.bytes {
        opts.lines = true;
        opts.words = true;
        opts.bytes = true;
    }
    Some((opts, index))
}

fn usage() {
    let _ = write_all(2, b"usage: wc [-lwc] [FILE...]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let Some((opts, start)) = parse_options(args) else {
        usage();
        return 2;
    };
    if start >= args.len() {
        if let Some(counts) = count_fd(0) {
            print_counts(counts, opts, None);
            return 0;
        }
        return 1;
    }

    let mut total = Counts::default();
    let mut ok = true;
    for path in &args[start..] {
        if let Some(counts) = count_file(path) {
            total.lines += counts.lines;
            total.words += counts.words;
            total.bytes += counts.bytes;
            print_counts(counts, opts, Some(path));
        } else {
            ok = false;
        }
    }
    if args.len() - start > 1 {
        print_counts(total, opts, Some(b"total"));
    }
    if ok { 0 } else { 1 }
}

ristux_userland::program_main!(main);
