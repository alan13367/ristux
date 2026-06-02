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
const SEEK_SET: usize = 0;

struct Options<'a> {
    input: Option<&'a [u8]>,
    output: Option<&'a [u8]>,
    block_size: usize,
    count: Option<usize>,
    skip: usize,
    seek: usize,
    notrunc: bool,
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

fn usage() {
    let _ = write_all(
        2,
        b"usage: dd [if=FILE] [of=FILE] [bs=N] [count=N] [skip=N] [seek=N] [conv=notrunc]\n",
    );
}

fn parse_number(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    let mut index = 0usize;
    let mut value = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        value = value
            .checked_mul(10)?
            .checked_add((bytes[index] - b'0') as usize)?;
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let multiplier = match &bytes[index..] {
        b"" => 1,
        b"c" => 1,
        b"w" => 2,
        b"b" => 512,
        b"k" | b"K" => 1024,
        b"m" | b"M" => 1024 * 1024,
        _ => return None,
    };
    value.checked_mul(multiplier)
}

fn parse_conv(bytes: &[u8]) -> Option<bool> {
    let mut notrunc = false;
    let mut start = 0usize;
    for index in 0..=bytes.len() {
        if index < bytes.len() && bytes[index] != b',' {
            continue;
        }
        match &bytes[start..index] {
            b"notrunc" => notrunc = true,
            b"" => {}
            _ => return None,
        }
        start = index + 1;
    }
    Some(notrunc)
}

fn parse_args<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    let mut options = Options {
        input: None,
        output: None,
        block_size: 512,
        count: None,
        skip: 0,
        seek: 0,
        notrunc: false,
    };

    for arg in &args[1..] {
        let eq = arg.iter().position(|byte| *byte == b'=')?;
        let key = &arg[..eq];
        let value = &arg[eq + 1..];
        match key {
            b"if" => options.input = Some(value),
            b"of" => options.output = Some(value),
            b"bs" => options.block_size = parse_number(value)?,
            b"count" => options.count = Some(parse_number(value)?),
            b"skip" => options.skip = parse_number(value)?,
            b"seek" => options.seek = parse_number(value)?,
            b"conv" => options.notrunc |= parse_conv(value)?,
            b"status" if value == b"none" || value == b"noxfer" => {}
            _ => return None,
        }
    }

    if options.block_size == 0 {
        return None;
    }
    Some(options)
}

fn seek_blocks(fd: i32, blocks: usize, block_size: usize) -> bool {
    if blocks == 0 {
        return true;
    }
    let Some(offset) = blocks.checked_mul(block_size) else {
        return false;
    };
    let rc = unsafe { sys::syscall3(sys::NR_LSEEK, fd as usize, offset, SEEK_SET) };
    rc >= 0
}

fn open_input(path: Option<&[u8]>) -> Option<i32> {
    let Some(path) = path else {
        return Some(0);
    };
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 { None } else { Some(fd as i32) }
}

fn open_output(path: Option<&[u8]>, notrunc: bool) -> Option<i32> {
    let Some(path) = path else {
        return Some(1);
    };
    let path = cstr(path);
    let flags = O_WRONLY | O_CREAT | if notrunc { 0 } else { O_TRUNC };
    let fd = sys::open(path.as_ptr(), flags, 0o644);
    if fd < 0 { None } else { Some(fd as i32) }
}

fn copy_blocks(input: i32, output: i32, block_size: usize, count: Option<usize>) -> i32 {
    let mut buf = Vec::new();
    buf.resize(block_size, 0);
    let mut copied = 0usize;

    loop {
        if count.is_some_and(|limit| copied >= limit) {
            return 0;
        }
        let n = sys::read(input, &mut buf);
        if n < 0 {
            let _ = write_all(2, b"dd: read failed\n");
            return 1;
        }
        if n == 0 {
            return 0;
        }
        if !write_all(output, &buf[..n as usize]) {
            let _ = write_all(2, b"dd: write failed\n");
            return 1;
        }
        copied += 1;
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(options) = parse_args(args) else {
        usage();
        return 2;
    };

    let Some(input) = open_input(options.input) else {
        let _ = write_all(2, b"dd: cannot open input\n");
        return 1;
    };
    let Some(output) = open_output(options.output, options.notrunc) else {
        if input != 0 {
            let _ = sys::close(input);
        }
        let _ = write_all(2, b"dd: cannot open output\n");
        return 1;
    };

    let rc = if !seek_blocks(input, options.skip, options.block_size) {
        let _ = write_all(2, b"dd: input seek failed\n");
        1
    } else if !seek_blocks(output, options.seek, options.block_size) {
        let _ = write_all(2, b"dd: output seek failed\n");
        1
    } else {
        copy_blocks(input, output, options.block_size, options.count)
    };

    if input != 0 {
        let _ = sys::close(input);
    }
    if output != 1 {
        let _ = sys::close(output);
    }
    rc
}

ristux_userland::program_main!(main);
