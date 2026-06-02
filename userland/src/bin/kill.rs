#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

struct Options {
    signal: u8,
    list: bool,
    pids_start: usize,
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

fn parse_i64(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }
    let mut index = 0usize;
    let mut sign = 1i64;
    if bytes[0] == b'-' {
        sign = -1;
        index = 1;
    } else if bytes[0] == b'+' {
        index = 1;
    }
    if index == bytes.len() {
        return None;
    }
    let mut value = 0i64;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as i64)?;
        index += 1;
    }
    Some(value.saturating_mul(sign))
}

fn trim_sig_prefix(name: &[u8]) -> &[u8] {
    if name.len() > 3
        && name[0].eq_ignore_ascii_case(&b's')
        && name[1].eq_ignore_ascii_case(&b'i')
        && name[2].eq_ignore_ascii_case(&b'g')
    {
        &name[3..]
    } else {
        name
    }
}

fn eq_ignore_ascii_case(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn signal_by_name(name: &[u8]) -> Option<u8> {
    let name = trim_sig_prefix(name);
    if let Some(value) = parse_i64(name) {
        if (0..=255).contains(&value) {
            return Some(value as u8);
        }
        return None;
    }
    let signals: &[(&[u8], u8)] = &[
        (b"HUP", 1),
        (b"INT", 2),
        (b"QUIT", 3),
        (b"KILL", 9),
        (b"USR1", 10),
        (b"TERM", 15),
        (b"CHLD", 17),
        (b"CONT", 18),
        (b"TSTP", 20),
    ];
    signals
        .iter()
        .find(|(candidate, _)| eq_ignore_ascii_case(name, candidate))
        .map(|(_, signal)| *signal)
}

fn usage() {
    let _ = write_all(
        2,
        b"usage: kill [-SIGNAL | -s SIGNAL] PID...\n       kill -l\n",
    );
}

fn parse_options(args: &[&[u8]]) -> Option<Options> {
    let mut options = Options {
        signal: 15,
        list: false,
        pids_start: 1,
    };
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-l" || *arg == b"--list" {
            options.list = true;
            index += 1;
            continue;
        }
        if *arg == b"-s" {
            index += 1;
            options.signal = signal_by_name(*args.get(index)?)?;
            index += 1;
            continue;
        }
        if let Some(rest) = arg.strip_prefix(b"--signal=") {
            options.signal = signal_by_name(rest)?;
            index += 1;
            continue;
        }
        if arg.starts_with(b"-") && arg.len() > 1 && signal_by_name(&arg[1..]).is_some() {
            options.signal = signal_by_name(&arg[1..])?;
            index += 1;
            continue;
        }
        break;
    }
    options.pids_start = index;
    Some(options)
}

fn print_signal_list() -> i32 {
    let _ = write_all(1, b"HUP INT QUIT KILL USR1 TERM CHLD CONT TSTP\n");
    0
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(options) = parse_options(args) else {
        usage();
        return 2;
    };
    if options.list {
        return print_signal_list();
    }
    if options.pids_start >= args.len() {
        usage();
        return 2;
    }

    let mut status = 0;
    for pid_arg in &args[options.pids_start..] {
        let Some(pid) = parse_i64(pid_arg) else {
            let _ = write_all(2, b"kill: invalid pid: ");
            let _ = write_all(2, pid_arg);
            let _ = write_all(2, b"\n");
            status = 1;
            continue;
        };
        if sys::kill(pid as isize, options.signal) < 0 {
            let _ = write_all(2, b"kill: failed: ");
            let _ = write_all(2, pid_arg);
            let _ = write_all(2, b"\n");
            status = 1;
        }
    }
    status
}

ristux_userland::program_main!(main);
