#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

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

fn envp_slice(envp: *const *const u8) -> Vec<&'static [u8]> {
    let mut out = Vec::new();
    if envp.is_null() {
        return out;
    }
    for index in 0..128usize {
        unsafe {
            let p = *envp.add(index);
            if p.is_null() {
                break;
            }
            let mut len = 0usize;
            while *p.add(len) != 0 {
                len += 1;
                if len > 4096 {
                    break;
                }
            }
            out.push(core::slice::from_raw_parts(p, len));
        }
    }
    out
}

fn env_value<'a>(env: &'a [&[u8]], name: &[u8]) -> Option<&'a [u8]> {
    for entry in env {
        if entry.len() > name.len()
            && &entry[..name.len()] == name
            && entry.get(name.len()) == Some(&b'=')
        {
            return Some(&entry[name.len() + 1..]);
        }
    }
    None
}

fn has_slash(bytes: &[u8]) -> bool {
    bytes.iter().any(|byte| *byte == b'/')
}

fn join_path(dir: &[u8], command: &[u8]) -> Vec<u8> {
    let dir = if dir.is_empty() { b"." as &[u8] } else { dir };
    let mut out = Vec::with_capacity(dir.len() + command.len() + 1);
    out.extend_from_slice(dir);
    if !out.ends_with(b"/") {
        out.push(b'/');
    }
    out.extend_from_slice(command);
    out
}

fn stat_mode(path: &[u8]) -> Option<u32> {
    let path = cstr(path);
    let mut stat_buf = [0u8; 144];
    let rc = unsafe {
        sys::syscall2(
            sys::NR_STAT,
            path.as_ptr() as usize,
            stat_buf.as_mut_ptr() as usize,
        )
    };
    if rc < 0 {
        return None;
    }
    Some(u32::from_le_bytes([
        stat_buf[24],
        stat_buf[25],
        stat_buf[26],
        stat_buf[27],
    ]))
}

fn is_dir(path: &[u8]) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let mut buf = [0u8; 128];
    let rc = sys::getdents64(fd as i32, &mut buf);
    let _ = sys::close(fd as i32);
    rc >= 0
}

fn is_command(path: &[u8]) -> bool {
    stat_mode(path).is_some() && !is_dir(path)
}

fn print_path(path: &[u8]) -> bool {
    write_all(1, path) && write_all(1, b"\n")
}

fn find_command(command: &[u8], path: &[u8], all: bool) -> bool {
    if has_slash(command) {
        if is_command(command) {
            return print_path(command);
        }
        return false;
    }

    let mut found = false;
    let mut start = 0usize;
    for index in 0..=path.len() {
        if index < path.len() && path[index] != b':' {
            continue;
        }
        let candidate = join_path(&path[start..index], command);
        if is_command(&candidate) {
            if !print_path(&candidate) {
                return found;
            }
            found = true;
            if !all {
                return true;
            }
        }
        start = index + 1;
    }
    found
}

fn usage() {
    let _ = write_all(2, b"usage: which [-a] COMMAND...\n");
}

fn main(args: &[&[u8]], env: &[&[u8]]) -> i32 {
    let mut all = false;
    let mut index = 1usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--" {
            index += 1;
            break;
        }
        if *arg == b"-a" {
            all = true;
            index += 1;
            continue;
        }
        if arg.starts_with(b"-") && *arg != b"-" {
            usage();
            return 2;
        }
        break;
    }

    if index >= args.len() {
        usage();
        return 2;
    }

    let path = env_value(env, b"PATH").unwrap_or(b"/bin:/usr/bin");
    let mut status = 0;
    for command in &args[index..] {
        if !find_command(command, path, all) {
            status = 1;
        }
    }
    status
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(argc: i64, argv: *const *const u8, envp: *const *const u8) -> ! {
    let argc = if argc < 0 { 0 } else { argc as usize };
    let args = ristux_userland::argv_slice(argc, argv);
    let arg_refs: Vec<&[u8]> = args.iter().map(|arg| *arg).collect();
    let env = envp_slice(envp);
    let status = main(&arg_refs, &env);
    sys::exit(status);
}
