#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

const KERNEL_NAME: &[u8] = b"Ristux";
const NODENAME: &[u8] = b"ristux";
const KERNEL_RELEASE: &[u8] = b"0.1.0";
const KERNEL_VERSION: &[u8] = b"#1";
const MACHINE: &[u8] = b"x86_64";
const PROCESSOR: &[u8] = b"x86_64";
const HARDWARE_PLATFORM: &[u8] = b"x86_64";
const OPERATING_SYSTEM: &[u8] = b"Ristux";

const FIELD_KERNEL_NAME: usize = 0;
const FIELD_NODENAME: usize = 1;
const FIELD_KERNEL_RELEASE: usize = 2;
const FIELD_KERNEL_VERSION: usize = 3;
const FIELD_MACHINE: usize = 4;
const FIELD_PROCESSOR: usize = 5;
const FIELD_HARDWARE_PLATFORM: usize = 6;
const FIELD_OPERATING_SYSTEM: usize = 7;
const FIELD_COUNT: usize = 8;

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

fn field_value(index: usize) -> &'static [u8] {
    match index {
        FIELD_KERNEL_NAME => KERNEL_NAME,
        FIELD_NODENAME => NODENAME,
        FIELD_KERNEL_RELEASE => KERNEL_RELEASE,
        FIELD_KERNEL_VERSION => KERNEL_VERSION,
        FIELD_MACHINE => MACHINE,
        FIELD_PROCESSOR => PROCESSOR,
        FIELD_HARDWARE_PLATFORM => HARDWARE_PLATFORM,
        FIELD_OPERATING_SYSTEM => OPERATING_SYSTEM,
        _ => b"",
    }
}

fn enable_all(fields: &mut [bool; FIELD_COUNT]) {
    for field in fields.iter_mut() {
        *field = true;
    }
}

fn set_short_option(fields: &mut [bool; FIELD_COUNT], byte: u8) -> bool {
    match byte {
        b'a' => enable_all(fields),
        b's' => fields[FIELD_KERNEL_NAME] = true,
        b'n' => fields[FIELD_NODENAME] = true,
        b'r' => fields[FIELD_KERNEL_RELEASE] = true,
        b'v' => fields[FIELD_KERNEL_VERSION] = true,
        b'm' => fields[FIELD_MACHINE] = true,
        b'p' => fields[FIELD_PROCESSOR] = true,
        b'i' => fields[FIELD_HARDWARE_PLATFORM] = true,
        b'o' => fields[FIELD_OPERATING_SYSTEM] = true,
        _ => return false,
    }
    true
}

fn set_long_option(fields: &mut [bool; FIELD_COUNT], option: &[u8]) -> Option<bool> {
    match option {
        b"--all" => enable_all(fields),
        b"--kernel-name" => fields[FIELD_KERNEL_NAME] = true,
        b"--nodename" => fields[FIELD_NODENAME] = true,
        b"--kernel-release" => fields[FIELD_KERNEL_RELEASE] = true,
        b"--kernel-version" => fields[FIELD_KERNEL_VERSION] = true,
        b"--machine" => fields[FIELD_MACHINE] = true,
        b"--processor" => fields[FIELD_PROCESSOR] = true,
        b"--hardware-platform" => fields[FIELD_HARDWARE_PLATFORM] = true,
        b"--operating-system" => fields[FIELD_OPERATING_SYSTEM] = true,
        b"--help" => {
            print_usage(1);
            return Some(true);
        }
        b"--version" => {
            let _ = write_all(1, b"uname (Ristux userland) 0.1.0\n");
            return Some(true);
        }
        _ => return None,
    }
    Some(false)
}

fn print_usage(fd: i32) {
    let _ = write_all(
        fd,
        b"usage: uname [-asnrvmpio] [--all] [--kernel-name] [--nodename] [--kernel-release] [--kernel-version] [--machine] [--processor] [--hardware-platform] [--operating-system]\n",
    );
}

fn print_error(prefix: &[u8], value: &[u8]) {
    let _ = write_all(2, b"uname: ");
    let _ = write_all(2, prefix);
    let _ = write_all(2, value);
    let _ = write_all(2, b"\n");
}

fn print_fields(fields: &[bool; FIELD_COUNT]) -> i32 {
    let mut printed = false;
    for index in 0..FIELD_COUNT {
        if !fields[index] {
            continue;
        }
        if printed && !write_all(1, b" ") {
            return 1;
        }
        if !write_all(1, field_value(index)) {
            return 1;
        }
        printed = true;
    }
    if write_all(1, b"\n") {
        0
    } else {
        1
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let mut fields = [false; FIELD_COUNT];
    let mut index = 1usize;

    while index < args.len() {
        let arg = args[index];
        if arg == b"--" {
            index += 1;
            if index < args.len() {
                print_error(b"extra operand ", args[index]);
                print_usage(2);
                return 2;
            }
            break;
        }
        if arg == b"-" || !arg.starts_with(b"-") {
            print_error(b"extra operand ", arg);
            print_usage(2);
            return 2;
        }

        if arg.starts_with(b"--") {
            match set_long_option(&mut fields, arg) {
                Some(true) => return 0,
                Some(false) => {
                    index += 1;
                    continue;
                }
                None => {
                    print_error(b"unrecognized option ", arg);
                    print_usage(2);
                    return 2;
                }
            }
        }

        for byte in &arg[1..] {
            if !set_short_option(&mut fields, *byte) {
                let bad = [b'-', *byte];
                print_error(b"invalid option ", &bad);
                print_usage(2);
                return 2;
            }
        }
        index += 1;
    }

    if fields.iter().all(|field| !*field) {
        fields[FIELD_KERNEL_NAME] = true;
    }

    print_fields(&fields)
}

ristux_userland::program_main!(main);
