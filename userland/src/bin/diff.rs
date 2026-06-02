#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

#[derive(Clone, Copy)]
enum Op {
    Equal(usize),
    Delete(usize),
    Insert(usize),
}

struct Options {
    unified: bool,
    brief: bool,
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

fn push_usize(out: &mut Vec<u8>, mut value: usize) {
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
    if path == b"-" {
        return read_fd(0);
    }
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let bytes = read_fd(fd as i32);
    let _ = sys::close(fd as i32);
    bytes
}

fn split_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for index in 0..=bytes.len() {
        if index < bytes.len() && bytes[index] != b'\n' {
            continue;
        }
        if index == bytes.len() && start == bytes.len() {
            break;
        }
        let mut line = &bytes[start..index];
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        lines.push(line.to_vec());
        start = index + 1;
    }
    lines
}

fn lcs_ops(left: &[Vec<u8>], right: &[Vec<u8>]) -> Vec<Op> {
    let width = right.len() + 1;
    let mut dp = Vec::new();
    dp.resize((left.len() + 1) * width, 0usize);

    for i in (0..left.len()).rev() {
        for j in (0..right.len()).rev() {
            dp[i * width + j] = if left[i] == right[j] {
                dp[(i + 1) * width + j + 1] + 1
            } else {
                core::cmp::max(dp[(i + 1) * width + j], dp[i * width + j + 1])
            };
        }
    }

    let mut ops = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < left.len() || j < right.len() {
        if i < left.len() && j < right.len() && left[i] == right[j] {
            ops.push(Op::Equal(i));
            i += 1;
            j += 1;
        } else if j >= right.len()
            || (i < left.len() && dp[(i + 1) * width + j] >= dp[i * width + j + 1])
        {
            ops.push(Op::Delete(i));
            i += 1;
        } else {
            ops.push(Op::Insert(j));
            j += 1;
        }
    }
    ops
}

fn print_brief(left_path: &[u8], right_path: &[u8]) -> bool {
    write_all(1, b"Files ")
        && write_all(1, left_path)
        && write_all(1, b" and ")
        && write_all(1, right_path)
        && write_all(1, b" differ\n")
}

fn push_hunk_range(out: &mut Vec<u8>, sign: u8, start: usize, len: usize) {
    out.push(sign);
    push_usize(out, start);
    out.push(b',');
    push_usize(out, len);
}

fn print_unified(
    left_path: &[u8],
    right_path: &[u8],
    left: &[Vec<u8>],
    right: &[Vec<u8>],
    ops: &[Op],
) -> bool {
    let mut out = Vec::new();
    out.extend_from_slice(b"--- ");
    out.extend_from_slice(left_path);
    out.push(b'\n');
    out.extend_from_slice(b"+++ ");
    out.extend_from_slice(right_path);
    out.push(b'\n');
    out.extend_from_slice(b"@@ ");
    push_hunk_range(&mut out, b'-', 1, left.len());
    out.push(b' ');
    push_hunk_range(&mut out, b'+', 1, right.len());
    out.extend_from_slice(b" @@\n");

    for op in ops {
        match *op {
            Op::Equal(i) => {
                out.push(b' ');
                out.extend_from_slice(&left[i]);
            }
            Op::Delete(i) => {
                out.push(b'-');
                out.extend_from_slice(&left[i]);
            }
            Op::Insert(j) => {
                out.push(b'+');
                out.extend_from_slice(&right[j]);
            }
        }
        out.push(b'\n');
    }
    write_all(1, &out)
}

fn print_simple(left: &[Vec<u8>], right: &[Vec<u8>], ops: &[Op]) -> bool {
    let mut out = Vec::new();
    for op in ops {
        match *op {
            Op::Equal(_) => {}
            Op::Delete(i) => {
                out.extend_from_slice(b"< ");
                out.extend_from_slice(&left[i]);
                out.push(b'\n');
            }
            Op::Insert(j) => {
                out.extend_from_slice(b"> ");
                out.extend_from_slice(&right[j]);
                out.push(b'\n');
            }
        }
    }
    write_all(1, &out)
}

fn usage() {
    let _ = write_all(2, b"usage: diff [-q] [-u] FILE1 FILE2\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut options = Options {
        unified: false,
        brief: false,
    };
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-u" || *arg == b"-U" {
            options.unified = true;
            index += 1;
            continue;
        }
        if *arg == b"-q" {
            options.brief = true;
            index += 1;
            continue;
        }
        if arg.starts_with(b"-") && *arg != b"-" {
            usage();
            return 2;
        }
        break;
    }

    if args.len().saturating_sub(index) != 2 {
        usage();
        return 2;
    }
    let left_path = args[index];
    let right_path = args[index + 1];

    let Some(left_bytes) = read_file(left_path) else {
        let _ = write_all(2, b"diff: cannot read ");
        let _ = write_all(2, left_path);
        let _ = write_all(2, b"\n");
        return 2;
    };
    let Some(right_bytes) = read_file(right_path) else {
        let _ = write_all(2, b"diff: cannot read ");
        let _ = write_all(2, right_path);
        let _ = write_all(2, b"\n");
        return 2;
    };

    if left_bytes == right_bytes {
        return 0;
    }
    if options.brief {
        return if print_brief(left_path, right_path) {
            1
        } else {
            2
        };
    }

    let left = split_lines(&left_bytes);
    let right = split_lines(&right_bytes);
    let ops = lcs_ops(&left, &right);
    let ok = if options.unified {
        print_unified(left_path, right_path, &left, &right, &ops)
    } else {
        print_simple(&left, &right, &ops)
    };
    if ok { 1 } else { 2 }
}

ristux_userland::program_main!(main);
