#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::ptr;
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

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
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
    if value == 0 {
        None
    } else {
        Some(value)
    }
}

fn read_stdin_words() -> Option<Vec<Vec<u8>>> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(0, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n as usize]);
    }

    Some(
        bytes
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty())
            .map(|word| word.to_vec())
            .collect(),
    )
}

fn command_path(command: &[u8]) -> Vec<u8> {
    if command.iter().any(|byte| *byte == b'/') {
        return command.to_vec();
    }
    let mut out = Vec::with_capacity(command.len() + 5);
    out.extend_from_slice(b"/bin/");
    out.extend_from_slice(command);
    out
}

fn run_command(args: &[Vec<u8>]) -> i32 {
    if args.is_empty() {
        return 0;
    }

    let pid = sys::fork();
    if pid < 0 {
        let _ = write_all(2, b"xargs: fork failed\n");
        return 1;
    }
    if pid == 0 {
        let path = command_path(&args[0]);
        let path_c = cstr(&path);
        let mut owned_args: Vec<Vec<u8>> = Vec::with_capacity(args.len());
        for arg in args {
            owned_args.push(cstr(arg));
        }
        let mut argv_ptrs: Vec<*const u8> = owned_args.iter().map(|arg| arg.as_ptr()).collect();
        argv_ptrs.push(ptr::null());
        let envp = [ptr::null::<u8>()];
        let _ = sys::execve(path_c.as_ptr(), argv_ptrs.as_ptr(), envp.as_ptr());
        let _ = write_all(2, b"xargs: cannot execute ");
        let _ = write_all(2, &args[0]);
        let _ = write_all(2, b"\n");
        sys::exit(127);
    }

    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) < 0 {
        let _ = write_all(2, b"xargs: wait failed\n");
        return 1;
    }
    status
}

fn usage() {
    let _ = write_all(2, b"usage: xargs [-n COUNT] [COMMAND [ARG...]]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let mut batch_size: Option<usize> = None;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-n" {
            let Some(next) = args.get(index + 1).and_then(|value| parse_usize(value)) else {
                usage();
                return 2;
            };
            batch_size = Some(next);
            index += 2;
        } else if arg.starts_with(b"-") && arg.len() > 1 {
            usage();
            return 2;
        } else {
            break;
        }
    }

    let mut base: Vec<Vec<u8>> = Vec::new();
    if index < args.len() {
        for arg in &args[index..] {
            base.push(arg.to_vec());
        }
    } else {
        base.push(b"echo".to_vec());
    }

    let Some(words) = read_stdin_words() else {
        return 1;
    };
    if words.is_empty() {
        return run_command(&base);
    }

    let batch_size = batch_size.unwrap_or(words.len());
    let mut rc = 0;
    let mut offset = 0usize;
    while offset < words.len() {
        let end = (offset + batch_size).min(words.len());
        let mut command = base.clone();
        for word in &words[offset..end] {
            command.push(word.clone());
        }
        let status = run_command(&command);
        if status != 0 {
            rc = status;
        }
        offset = end;
    }
    rc
}

ristux_userland::program_main!(main);
