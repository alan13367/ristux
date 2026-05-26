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

fn envp_slice(envp: *const *const u8) -> Vec<Vec<u8>> {
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
            out.push(core::slice::from_raw_parts(p, len).to_vec());
        }
    }
    out
}

fn var_name_len(entry: &[u8]) -> Option<usize> {
    let eq = entry.iter().position(|byte| *byte == b'=')?;
    if eq == 0 {
        None
    } else {
        Some(eq)
    }
}

fn set_env(env: &mut Vec<Vec<u8>>, assignment: &[u8]) {
    let Some(name_len) = var_name_len(assignment) else {
        return;
    };
    if let Some(existing) = env
        .iter_mut()
        .find(|entry| entry.len() > name_len && entry[..name_len] == assignment[..name_len] && entry[name_len] == b'=')
    {
        existing.clear();
        existing.extend_from_slice(assignment);
        return;
    }
    env.push(assignment.to_vec());
}

fn has_slash(bytes: &[u8]) -> bool {
    bytes.iter().any(|byte| *byte == b'/')
}

fn command_path(command: &[u8]) -> Vec<u8> {
    if has_slash(command) {
        return command.to_vec();
    }
    let mut out = Vec::with_capacity(command.len() + 5);
    out.extend_from_slice(b"/bin/");
    out.extend_from_slice(command);
    out
}

fn print_env(env: &[Vec<u8>]) -> i32 {
    for entry in env {
        if !write_all(1, entry) || !write_all(1, b"\n") {
            return 1;
        }
    }
    0
}

fn usage() {
    let _ = write_all(2, b"usage: env [-i] [NAME=VALUE...] [COMMAND [ARG...]]\n");
}

fn main(args: &[&[u8]], inherited: &[Vec<u8>]) -> i32 {
    let mut env = inherited.to_vec();
    let mut index = 1usize;
    if args.get(index).is_some_and(|arg| *arg == b"-i") {
        env.clear();
        index += 1;
    } else if args
        .get(index)
        .is_some_and(|arg| arg.starts_with(b"-") && *arg != b"-")
    {
        usage();
        return 2;
    }

    while let Some(arg) = args.get(index) {
        if var_name_len(arg).is_none() {
            break;
        }
        set_env(&mut env, arg);
        index += 1;
    }

    if index >= args.len() {
        return print_env(&env);
    }

    let path = command_path(args[index]);
    let path_c = cstr(&path);
    let mut owned_args: Vec<Vec<u8>> = Vec::new();
    for arg in &args[index..] {
        owned_args.push(cstr(arg));
    }
    let mut argv_ptrs: Vec<*const u8> = owned_args.iter().map(|arg| arg.as_ptr()).collect();
    argv_ptrs.push(ptr::null());

    let owned_env: Vec<Vec<u8>> = env.iter().map(|entry| cstr(entry)).collect();
    let mut env_ptrs: Vec<*const u8> = owned_env.iter().map(|entry| entry.as_ptr()).collect();
    env_ptrs.push(ptr::null());

    let _ = sys::execve(path_c.as_ptr(), argv_ptrs.as_ptr(), env_ptrs.as_ptr());
    let _ = write_all(2, b"env: cannot execute ");
    let _ = write_all(2, args[index]);
    let _ = write_all(2, b"\n");
    127
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(argc: i64, argv: *const *const u8, envp: *const *const u8) -> ! {
    let argc = if argc < 0 { 0 } else { argc as usize };
    let args = ristux_userland::argv_slice(argc, argv);
    let arg_refs: Vec<&[u8]> = args.iter().map(|arg| *arg).collect();
    let inherited = envp_slice(envp);
    let status = main(&arg_refs, &inherited);
    sys::exit(status);
}
