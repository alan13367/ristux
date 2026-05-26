#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

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

fn escape(byte: u8) -> Option<u8> {
    match byte {
        b'a' => Some(7),
        b'b' => Some(8),
        b'f' => Some(12),
        b'n' => Some(b'\n'),
        b'r' => Some(b'\r'),
        b't' => Some(b'\t'),
        b'v' => Some(11),
        b'\\' => Some(b'\\'),
        _ => None,
    }
}

fn print_format(format: &[u8], args: &[&[u8]]) -> bool {
    let mut arg_index = 0usize;
    let mut index = 0usize;
    while index < format.len() {
        match format[index] {
            b'\\' if index + 1 < format.len() => {
                if let Some(byte) = escape(format[index + 1]) {
                    if !write_all(1, &[byte]) {
                        return false;
                    }
                    index += 2;
                } else {
                    if !write_all(1, &format[index + 1..index + 2]) {
                        return false;
                    }
                    index += 2;
                }
            }
            b'%' if index + 1 < format.len() => {
                let spec = format[index + 1];
                match spec {
                    b'%' => {
                        if !write_all(1, b"%") {
                            return false;
                        }
                    }
                    b's' | b'd' | b'i' | b'u' | b'c' => {
                        let arg = args.get(arg_index).copied().unwrap_or(b"");
                        if spec == b'c' {
                            if let Some(first) = arg.first() {
                                if !write_all(1, &[*first]) {
                                    return false;
                                }
                            }
                        } else if !write_all(1, arg) {
                            return false;
                        }
                        arg_index += 1;
                    }
                    _ => {
                        if !write_all(1, &format[index..index + 2]) {
                            return false;
                        }
                    }
                }
                index += 2;
            }
            byte => {
                if !write_all(1, &[byte]) {
                    return false;
                }
                index += 1;
            }
        }
    }
    true
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() < 2 {
        return 0;
    }
    if print_format(args[1], &args[2..]) {
        0
    } else {
        1
    }
}

ristux_userland::program_main!(main);
