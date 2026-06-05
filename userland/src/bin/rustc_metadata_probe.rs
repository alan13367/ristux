#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::ptr;
use ristux_userland::sys;

const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const STAT_SIZE: usize = 144;
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;

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

fn line(bytes: &[u8]) {
    let _ = write_all(1, bytes);
    let _ = write_all(1, b"\n");
}

fn fail(step: &[u8]) -> i32 {
    let _ = write_all(2, b"rustc_metadata_probe: ");
    let _ = write_all(2, step);
    let _ = write_all(2, b" failed\n");
    1
}

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn read_le_u32(bytes: &[u8], offset: usize) -> u32 {
    if offset + 4 > bytes.len() {
        return 0;
    }
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_le_u64(bytes: &[u8], offset: usize) -> u64 {
    if offset + 8 > bytes.len() {
        return 0;
    }
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

fn regular_file_size(path: &[u8]) -> Option<u64> {
    let path = cstr(path);
    let mut stat_buf = [0u8; STAT_SIZE];
    if sys::stat(path.as_ptr(), stat_buf.as_mut_ptr()) < 0 {
        return None;
    }
    let mode = read_le_u32(&stat_buf, 24);
    if mode & S_IFMT != S_IFREG {
        return None;
    }
    Some(read_le_u64(&stat_buf, 48))
}

fn write_file(path: &[u8], bytes: &[u8]) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return false;
    }
    let ok = write_all(fd as i32, bytes);
    let close_ok = sys::close(fd as i32) == 0;
    ok && close_ok
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn spawn_capture(path: &[u8], argv0: &[u8], args: &[&[u8]]) -> Option<Vec<u8>> {
    let mut pipefd = [0i32; 2];
    if sys::pipe(pipefd.as_mut_ptr()) < 0 {
        return None;
    }

    let pid = sys::fork();
    if pid < 0 {
        let _ = sys::close(pipefd[0]);
        let _ = sys::close(pipefd[1]);
        return None;
    }
    if pid == 0 {
        let _ = sys::close(pipefd[0]);
        let _ = sys::dup2(pipefd[1], 1);
        let _ = sys::dup2(pipefd[1], 2);
        let _ = sys::close(pipefd[1]);

        let path_c = cstr(path);
        let mut argv_storage = Vec::with_capacity(args.len() + 1);
        argv_storage.push(cstr(argv0));
        for arg in args {
            argv_storage.push(cstr(arg));
        }
        let mut argv: Vec<*const u8> = argv_storage.iter().map(|arg| arg.as_ptr()).collect();
        argv.push(ptr::null());

        let env_storage = [
            cstr(b"PATH=/bin:/usr/bin"),
            cstr(b"HOME=/root"),
            cstr(b"RUST_BACKTRACE=1"),
        ];
        let envp = [
            env_storage[0].as_ptr(),
            env_storage[1].as_ptr(),
            env_storage[2].as_ptr(),
            ptr::null(),
        ];

        let _ = sys::execve(path_c.as_ptr(), argv.as_ptr(), envp.as_ptr());
        let _ = write_all(2, b"rustc_metadata_probe: execve capture failed\n");
        sys::exit(127);
    }

    let _ = sys::close(pipefd[1]);
    let mut output = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = sys::read(pipefd[0], &mut buf);
        if n < 0 {
            let _ = sys::close(pipefd[0]);
            return None;
        }
        if n == 0 {
            break;
        }
        output.extend_from_slice(&buf[..n as usize]);
        if output.len() > 16 * 1024 {
            let _ = sys::close(pipefd[0]);
            return None;
        }
    }
    let _ = sys::close(pipefd[0]);

    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) < 0 || status != 0 {
        return None;
    }
    Some(output)
}

fn exec_and_wait(path: &[u8], args: &[&[u8]]) -> Option<i32> {
    let pid = sys::fork();
    if pid < 0 {
        return None;
    }
    if pid == 0 {
        let path_c = cstr(path);
        let mut argv_storage = Vec::with_capacity(args.len() + 1);
        argv_storage.push(cstr(path));
        for arg in args {
            argv_storage.push(cstr(arg));
        }
        let mut argv: Vec<*const u8> = argv_storage.iter().map(|arg| arg.as_ptr()).collect();
        argv.push(ptr::null());

        let env_storage = [
            cstr(b"PATH=/bin:/usr/bin"),
            cstr(b"HOME=/root"),
            cstr(b"RUST_BACKTRACE=1"),
        ];
        let envp = [
            env_storage[0].as_ptr(),
            env_storage[1].as_ptr(),
            env_storage[2].as_ptr(),
            ptr::null(),
        ];

        let _ = sys::execve(path_c.as_ptr(), argv.as_ptr(), envp.as_ptr());
        let _ = write_all(2, b"rustc_metadata_probe: execve /bin/rustc failed\n");
        sys::exit(127);
    }

    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) < 0 {
        return None;
    }
    Some(status)
}

fn main(_args: &[&[u8]]) -> i32 {
    let _ = sys::unlink(cstr(b"/tmp/rustc-metadata-probe.rs").as_ptr());
    let _ = sys::unlink(cstr(b"/tmp/rustc-metadata-probe.rmeta").as_ptr());

    if regular_file_size(b"/bin/rustc").unwrap_or(0) < 1024 * 1024 {
        return fail(b"official rustc presence");
    }
    line(b"rustc_metadata_probe: official rustc present");

    let Some(version) = spawn_capture(b"/bin/rustc", b"rustc", &[b"--version"]) else {
        return fail(b"rustc version");
    };
    if !contains(&version, b"rustc 1.96.0") {
        return fail(b"rustc version");
    }
    line(b"rustc_metadata_probe: version ok");

    if !write_file(
        b"/tmp/rustc-metadata-probe.rs",
        b"#![no_std]\n#[unsafe(no_mangle)]\npub extern \"C\" fn ristux_metadata_probe() -> i32 { 42 }\n",
    ) {
        return fail(b"source write");
    }
    line(b"rustc_metadata_probe: source ready");

    let status = exec_and_wait(
        b"/bin/rustc",
        &[
            b"--crate-name",
            b"ristux_metadata_probe",
            b"--crate-type",
            b"lib",
            b"--target",
            b"x86_64-unknown-ristux",
            b"--sysroot",
            b"/usr",
            b"--emit",
            b"metadata",
            b"/tmp/rustc-metadata-probe.rs",
            b"-o",
            b"/tmp/rustc-metadata-probe.rmeta",
        ],
    );
    if status != Some(0) {
        return fail(b"rustc metadata");
    }

    if regular_file_size(b"/tmp/rustc-metadata-probe.rmeta").unwrap_or(0) == 0 {
        return fail(b"metadata output");
    }

    line(b"rustc_metadata_probe: metadata compile ok");
    0
}

ristux_userland::program_main!(main);
