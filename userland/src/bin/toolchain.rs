#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::ptr;
use ristux_userland::sys;

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
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

fn basename(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

fn append_frontend_args(out: &mut Vec<Vec<u8>>, mode: &[u8], args: &[&[u8]]) -> bool {
    out.push(b"rustc".to_vec());
    match mode {
        b"ld" => out[0] = b"ristux-ld".to_vec(),
        b"rustc" => {}
        _ => return false,
    }
    for arg in args {
        out.push(arg.to_vec());
    }
    true
}

fn run_frontend(args: &[&[u8]], inherited_env: &[Vec<u8>]) -> i32 {
    let mode = args
        .first()
        .map(|arg| basename(arg))
        .unwrap_or(b"toolchain");
    let mut owned_args = Vec::new();
    if !append_frontend_args(&mut owned_args, mode, args.get(1..).unwrap_or(&[])) {
        write_all(2, b"toolchain: C frontends removed; use rustc or ristux-ld\n");
        return 2;
    }

    let program = owned_args
        .first()
        .map(|arg| arg.as_slice())
        .unwrap_or(b"rustc");
    let path = if program == b"ristux-ld" {
        cstr(b"/bin/ristux-ld")
    } else {
        cstr(b"/bin/rustc")
    };
    let c_args: Vec<Vec<u8>> = owned_args.iter().map(|arg| cstr(arg)).collect();
    let mut argv: Vec<*const u8> = c_args.iter().map(|arg| arg.as_ptr()).collect();
    argv.push(ptr::null());

    let c_env: Vec<Vec<u8>> = inherited_env.iter().map(|entry| cstr(entry)).collect();
    let mut envp: Vec<*const u8> = c_env.iter().map(|entry| entry.as_ptr()).collect();
    envp.push(ptr::null());

    let _ = sys::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
    write_all(2, b"toolchain: cannot execute Rust toolchain frontend\n");
    127
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(argc: i64, argv: *const *const u8, envp: *const *const u8) -> ! {
    let argc = if argc < 0 { 0 } else { argc as usize };
    let args = ristux_userland::argv_slice(argc, argv);
    let arg_refs: Vec<&[u8]> = args.iter().map(|arg| *arg).collect();
    let inherited_env = envp_slice(envp);
    let status = run_frontend(&arg_refs, &inherited_env);
    sys::exit(status);
}
