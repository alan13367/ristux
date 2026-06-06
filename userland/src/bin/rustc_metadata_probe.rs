#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::ptr;
use ristux_userland::sys;

const O_WRONLY: i32 = 1;
const O_RDONLY: i32 = 0;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const DIRENT64_HEADER: usize = 19;
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

fn write_usize(mut value: usize) {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    if value == 0 {
        let _ = write_all(1, b"0");
        return;
    }
    while value > 0 {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    while len > 0 {
        len -= 1;
        let _ = write_all(1, &digits[len..len + 1]);
    }
}

fn status_line(label: &[u8], value: i32) {
    let _ = write_all(1, b"rustc_metadata_probe: ");
    let _ = write_all(1, label);
    let _ = write_all(1, b" status ");
    if value < 0 {
        let _ = write_all(1, b"-");
        write_usize(value.unsigned_abs() as usize);
    } else {
        write_usize(value as usize);
    }
    let _ = write_all(1, b"\n");
}

fn value_line(label: &[u8], value: usize) {
    let _ = write_all(1, b"rustc_metadata_probe: ");
    let _ = write_all(1, label);
    let _ = write_all(1, b" ");
    write_usize(value);
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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd as i32, &mut buf);
        if n < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn is_decimal(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|byte| byte.is_ascii_digit())
}

fn read_proc_status(pid_name: &[u8]) -> Option<Vec<u8>> {
    let mut path = Vec::new();
    path.extend_from_slice(b"/proc/");
    path.extend_from_slice(pid_name);
    path.extend_from_slice(b"/status");
    read_file(&path)
}

fn count_processes_named(name: &[u8]) -> Option<usize> {
    let mut needle = Vec::new();
    needle.extend_from_slice(b"name: ");
    needle.extend_from_slice(name);
    needle.push(b'\n');

    let proc_path = cstr(b"/proc");
    let fd = sys::open(proc_path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut count = 0usize;
    let mut storage = [0u8; 1024];
    loop {
        let nread = sys::getdents64(fd as i32, &mut storage);
        if nread < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if nread == 0 {
            break;
        }
        let mut offset = 0usize;
        while offset + DIRENT64_HEADER <= nread as usize {
            let reclen = u16::from_le_bytes([storage[offset + 16], storage[offset + 17]]) as usize;
            if reclen == 0 || offset + reclen > nread as usize {
                let _ = sys::close(fd as i32);
                return None;
            }
            let name_start = offset + DIRENT64_HEADER;
            let name_end = storage[name_start..offset + reclen]
                .iter()
                .position(|byte| *byte == 0)
                .map(|pos| name_start + pos)
                .unwrap_or(offset + reclen);
            let pid_name = &storage[name_start..name_end];
            if is_decimal(pid_name)
                && read_proc_status(pid_name)
                    .as_ref()
                    .is_some_and(|status| contains(status, &needle))
            {
                count += 1;
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    Some(count)
}

fn free_ram_bytes() -> Option<usize> {
    let mut info = sys::SysInfo::default();
    if sys::sysinfo(&mut info as *mut sys::SysInfo) < 0 {
        return None;
    }
    Some(read_le_u64(&info.bytes, 40) as usize)
}

fn diagnostic_snapshot(label: &[u8]) {
    let mut free_label = Vec::new();
    free_label.extend_from_slice(label);
    free_label.extend_from_slice(b" free");
    value_line(&free_label, free_ram_bytes().unwrap_or(0));

    let mut rustc_label = Vec::new();
    rustc_label.extend_from_slice(label);
    rustc_label.extend_from_slice(b" rustc-procs");
    value_line(
        &rustc_label,
        count_processes_named(b"/bin/rustc").unwrap_or(usize::MAX),
    );
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

fn exec_and_wait(path: &[u8], args: &[&[u8]]) -> i32 {
    let pid = sys::fork();
    if pid < 0 {
        return pid as i32;
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
        let _ = write_all(2, b"rustc_metadata_probe: execve failed: ");
        let _ = write_all(2, path);
        let _ = write_all(2, b"\n");
        sys::exit(127);
    }

    let mut status = 0i32;
    let waited = sys::wait4(pid, &mut status as *mut i32, 0, 0);
    if waited < 0 {
        return waited as i32;
    }
    status
}

fn fork_sanity() -> bool {
    let pid = sys::fork();
    if pid < 0 {
        return false;
    }
    if pid == 0 {
        sys::exit(0);
    }

    let mut status = 0i32;
    sys::wait4(pid, &mut status as *mut i32, 0, 0) == pid && status == 0
}

fn main(_args: &[&[u8]]) -> i32 {
    let _ = sys::unlink(cstr(b"/tmp/rustc-metadata-probe.rs").as_ptr());
    let _ = sys::unlink(cstr(b"/tmp/rustc-metadata-probe.rmeta").as_ptr());
    let _ = sys::unlink(cstr(b"/tmp/rustc-codegen-probe.o").as_ptr());
    let _ = sys::unlink(cstr(b"/tmp/rustc-native-hello.rs").as_ptr());
    let _ = sys::unlink(cstr(b"/tmp/rustc-native-link.o").as_ptr());
    let _ = sys::unlink(cstr(b"/tmp/rustc-native-hello").as_ptr());

    if regular_file_size(b"/bin/rustc").unwrap_or(0) < 1024 * 1024 {
        return fail(b"official rustc presence");
    }
    line(b"rustc_metadata_probe: official rustc present");
    diagnostic_snapshot(b"initial");

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
    status_line(b"metadata", status);
    if status != 0 {
        return fail(b"rustc metadata");
    }

    if regular_file_size(b"/tmp/rustc-metadata-probe.rmeta").unwrap_or(0) == 0 {
        return fail(b"metadata output");
    }

    line(b"rustc_metadata_probe: metadata compile ok");
    diagnostic_snapshot(b"after-metadata");

    if !fork_sanity() {
        return fail(b"post-metadata fork");
    }
    line(b"rustc_metadata_probe: post-metadata fork ok");
    diagnostic_snapshot(b"after-fork-sanity");

    let status = exec_and_wait(
        b"/bin/rustc",
        &[
            b"--crate-name",
            b"ristux_codegen_probe",
            b"--crate-type",
            b"lib",
            b"--target",
            b"x86_64-unknown-ristux",
            b"--sysroot",
            b"/usr",
            b"--emit",
            b"obj",
            b"/tmp/rustc-metadata-probe.rs",
            b"-o",
            b"/tmp/rustc-codegen-probe.o",
        ],
    );
    status_line(b"object", status);
    let object_size = regular_file_size(b"/tmp/rustc-codegen-probe.o").unwrap_or(0);
    if object_size > 0 {
        let _ = write_all(1, b"rustc_metadata_probe: object bytes ");
        write_usize(object_size as usize);
        let _ = write_all(1, b"\n");
    }
    if status != 0 {
        return fail(b"rustc object");
    }

    if object_size == 0 {
        return fail(b"object output");
    }

    line(b"rustc_metadata_probe: object compile ok");

    if !fork_sanity() {
        return fail(b"post-object fork");
    }
    line(b"rustc_metadata_probe: post-object fork ok");

    if !write_file(
        b"/tmp/rustc-native-hello.rs",
        b"#![no_std]\n#[unsafe(no_mangle)]\npub extern \"C\" fn main() -> i32 { 0 }\n",
    ) {
        return fail(b"binary source write");
    }
    line(b"rustc_metadata_probe: binary source ready");

    let status = exec_and_wait(
        b"/bin/rustc",
        &[
            b"--crate-name",
            b"rustc_native_link",
            b"--crate-type",
            b"lib",
            b"--target",
            b"x86_64-unknown-ristux",
            b"--sysroot",
            b"/usr",
            b"--emit",
            b"obj",
            b"/tmp/rustc-native-hello.rs",
            b"-o",
            b"/tmp/rustc-native-link.o",
        ],
    );
    status_line(b"link-object", status);
    let link_object_size = regular_file_size(b"/tmp/rustc-native-link.o").unwrap_or(0);
    if link_object_size > 0 {
        let _ = write_all(1, b"rustc_metadata_probe: link-object bytes ");
        write_usize(link_object_size as usize);
        let _ = write_all(1, b"\n");
    }
    if status != 0 {
        return fail(b"rustc link object");
    }
    if link_object_size == 0 {
        return fail(b"link object output");
    }
    line(b"rustc_metadata_probe: link object compile ok");

    let status = exec_and_wait(
        b"/bin/ristux-ld",
        &[
            b"--ristux-crt0",
            b"-o",
            b"/tmp/rustc-native-hello",
            b"/tmp/rustc-native-link.o",
        ],
    );
    status_line(b"manual-link", status);
    let binary_size = regular_file_size(b"/tmp/rustc-native-hello").unwrap_or(0);
    if binary_size > 0 {
        let _ = write_all(1, b"rustc_metadata_probe: binary bytes ");
        write_usize(binary_size as usize);
        let _ = write_all(1, b"\n");
    }
    if status != 0 {
        return fail(b"manual link");
    }
    if binary_size == 0 {
        return fail(b"binary output");
    }
    line(b"rustc_metadata_probe: manual link ok");

    let Some(output) = spawn_capture(b"/tmp/rustc-native-hello", b"rustc-native-hello", &[]) else {
        return fail(b"binary run");
    };
    if !output.is_empty() {
        return fail(b"binary quiet output");
    }
    line(b"rustc_metadata_probe: binary run ok");

    0
}

ristux_userland::program_main!(main);
