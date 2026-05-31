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

fn usage(fd: i32) {
    let _ = write_all(fd, b"usage: hostname [-s] [NAME]\n");
}

fn print_error(message: &[u8], value: &[u8]) {
    let _ = write_all(2, b"hostname: ");
    let _ = write_all(2, message);
    let _ = write_all(2, value);
    let _ = write_all(2, b"\n");
}

fn current_hostname() -> Option<sys::UtsName> {
    let mut uts = sys::UtsName::default();
    if sys::uname(&mut uts as *mut sys::UtsName) < 0 {
        None
    } else {
        Some(uts)
    }
}

fn print_hostname(short: bool) -> i32 {
    let Some(uts) = current_hostname() else {
        let _ = write_all(2, b"hostname: uname failed\n");
        return 1;
    };
    let mut name = uts.field(1);
    if short {
        if let Some(dot) = name.iter().position(|byte| *byte == b'.') {
            name = &name[..dot];
        }
    }
    if write_all(1, name) && write_all(1, b"\n") {
        0
    } else {
        1
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let mut short = false;
    let mut name: Option<&[u8]> = None;
    let mut index = 1usize;

    while index < args.len() {
        let arg = args[index];
        if arg == b"--help" {
            usage(1);
            return 0;
        }
        if arg == b"--version" {
            let _ = write_all(1, b"hostname (Ristux userland) 0.1.0\n");
            return 0;
        }
        if arg == b"--" {
            index += 1;
            break;
        }
        if arg == b"-s" {
            short = true;
            index += 1;
            continue;
        }
        if arg.starts_with(b"-") && arg != b"-" {
            print_error(b"unrecognized option ", arg);
            usage(2);
            return 2;
        }
        break;
    }

    if index < args.len() {
        name = Some(args[index]);
        index += 1;
    }
    if index < args.len() {
        print_error(b"extra operand ", args[index]);
        usage(2);
        return 2;
    }
    if short && name.is_some() {
        let _ = write_all(2, b"hostname: -s is only valid when printing\n");
        usage(2);
        return 2;
    }

    if let Some(value) = name {
        if value.is_empty() || value.len() > 64 {
            let _ = write_all(2, b"hostname: invalid name length\n");
            return 1;
        }
        if sys::sethostname(value.as_ptr(), value.len()) < 0 {
            let _ = write_all(2, b"hostname: sethostname failed\n");
            return 1;
        }
        return 0;
    }

    print_hostname(short)
}

ristux_userland::program_main!(main);
