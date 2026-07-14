#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::ptr;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const SEEK_SET: usize = 0;
const DIRENT64_HEADER: usize = 19;
const STAT_SIZE: usize = 144;
const STATX_SIZE: usize = 256;
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

fn fail(check: &[u8]) -> bool {
    let _ = write_all(2, b"rust_host_probe: ");
    let _ = write_all(2, check);
    let _ = write_all(2, b" failed\n");
    false
}

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn exists(path: &[u8]) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let _ = sys::close(fd as i32);
    true
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

fn stat_is_regular_nonempty(buf: &[u8; STAT_SIZE]) -> bool {
    let mode = read_le_u32(buf, 24);
    let size = read_le_u64(buf, 48);
    mode & S_IFMT == S_IFREG && size > 0
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

fn envp_has(envp: *const *const u8, key: &[u8]) -> bool {
    if envp.is_null() || key.is_empty() {
        return false;
    }
    for index in 0..128usize {
        unsafe {
            let entry = *envp.add(index);
            if entry.is_null() {
                break;
            }
            let mut len = 0usize;
            while *entry.add(len) != 0 && len < 4096 {
                len += 1;
            }
            let bytes = core::slice::from_raw_parts(entry, len);
            if bytes.len() > key.len() && bytes.starts_with(key) && bytes[key.len()] == b'=' {
                return true;
            }
        }
    }
    false
}

fn dir_contains(path: &[u8], wanted: &[u8]) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let mut storage = [0u8; 1024];
    loop {
        let nread = sys::getdents64(fd as i32, &mut storage);
        if nread < 0 {
            let _ = sys::close(fd as i32);
            return false;
        }
        if nread == 0 {
            break;
        }
        let mut offset = 0usize;
        while offset + 19 <= nread as usize {
            let reclen = u16::from_le_bytes([storage[offset + 16], storage[offset + 17]]) as usize;
            if reclen == 0 || offset + reclen > nread as usize {
                let _ = sys::close(fd as i32);
                return false;
            }
            let name_start = offset + 19;
            let name_end = storage[name_start..offset + reclen]
                .iter()
                .position(|byte| *byte == 0)
                .map(|pos| name_start + pos)
                .unwrap_or(offset + reclen);
            if &storage[name_start..name_end] == wanted {
                let _ = sys::close(fd as i32);
                return true;
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    false
}

fn dir_contains_matching(path: &[u8], prefix: &[u8], suffix: &[u8]) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let mut storage = [0u8; 1024];
    loop {
        let nread = sys::getdents64(fd as i32, &mut storage);
        if nread < 0 {
            let _ = sys::close(fd as i32);
            return false;
        }
        if nread == 0 {
            break;
        }
        let mut offset = 0usize;
        while offset + 19 <= nread as usize {
            let reclen = u16::from_le_bytes([storage[offset + 16], storage[offset + 17]]) as usize;
            if reclen == 0 || offset + reclen > nread as usize {
                let _ = sys::close(fd as i32);
                return false;
            }
            let name_start = offset + 19;
            let name_end = storage[name_start..offset + reclen]
                .iter()
                .position(|byte| *byte == 0)
                .map(|pos| name_start + pos)
                .unwrap_or(offset + reclen);
            let name = &storage[name_start..name_end];
            if name.starts_with(prefix) && name.ends_with(suffix) {
                let _ = sys::close(fd as i32);
                return true;
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);
    false
}

fn spawn_capture(
    path: &[u8],
    argv0: &[u8],
    args: &[&[u8]],
    envp: *const *const u8,
) -> Option<Vec<u8>> {
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
        let empty_envp = [ptr::null()];
        let child_envp = if envp.is_null() {
            empty_envp.as_ptr()
        } else {
            envp
        };
        let _ = sys::execve(path_c.as_ptr(), argv.as_ptr(), child_envp);
        let _ = write_all(2, b"rust_host_probe: exec failed\n");
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
        if output.len() > 4096 {
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

fn check_toolchain_files() -> bool {
    if !exists(b"/bin/rustc")
        || !exists(b"/bin/cargo")
        || !exists(b"/bin/rustdoc")
        || !exists(b"/bin/ristux-ld")
        || !exists(b"/usr/lib/rustlib/x86_64-unknown-ristux/bin/ristux-ld")
        || !exists(b"/usr/lib/rustlib/rust-1.96.0-manifest.toml")
        || !exists(b"/usr/lib/rustlib/src/ristux-overlays/README.md")
        || !exists(b"/usr/lib/rustlib/x86_64-unknown-ristux/target.json")
    {
        return fail(b"toolchain files");
    }
    line(b"rust_host_probe: toolchain files ok");
    true
}

fn check_manifest() -> bool {
    let Some(manifest) = read_file(b"/usr/lib/rustlib/rust-1.96.0-manifest.toml") else {
        return fail(b"manifest");
    };
    if !contains(&manifest, b"version = \"1.96.0\"")
        || !contains(&manifest, b"host = \"x86_64-unknown-ristux\"")
        || !contains(
            &manifest,
            b"target_libdir = \"/usr/lib/rustlib/x86_64-unknown-ristux/lib\"",
        )
        || !contains(
            &manifest,
            b"sysroot_libraries = \"core,alloc,std,panic_abort,compiler_builtins\"",
        )
        || !contains(
            &manifest,
            b"std = \"packaged Ristux overlay probe artifacts\"",
        )
        || !contains(
            &manifest,
            b"ristux_overlay_sources = \"/usr/lib/rustlib/src/ristux-overlays\"",
        )
        || !contains(
            &manifest,
            b"std_libdir = \"/usr/lib/rustlib/x86_64-unknown-ristux/lib\"",
        )
        || !contains(&manifest, b"std_probe = \"/bin/rust_std_probe\"")
        || !contains(&manifest, b"linker = \"/bin/ristux-ld\"")
        || !contains(&manifest, b"linker_version = \"0.3.0-bootstrap\"")
        || !contains(
            &manifest,
            b"official_stage1_std_bootstrap = \"proven by make rust-official-bootstrap-std\"",
        )
        || !contains(
            &manifest,
            b"official_stage2_static_driver_probe = \"builds static upstream Cargo with the pure-Rust Ristux-offline feature graph\"",
        )
        || !contains(
            &manifest,
            b"native_codegen = \"official stage2 rustc compile, ristux-ld link, and static ELF execution verified\"",
        )
        || !contains(
            &manifest,
            b"cargo_build_execution = \"upstream Cargo binary/library build, check, and run supported for editions 2015/2018/2021/2024 with recursive path dependencies, local file Git dependencies through gix, workspaces, and build scripts; network Git, registry HTTPS, and proc macros pending\"",
        )
    {
        return fail(b"manifest");
    }
    line(b"rust_host_probe: manifest ok");
    true
}

fn check_target_spec() -> bool {
    let Some(spec) = read_file(b"/usr/lib/rustlib/x86_64-unknown-ristux/target.json") else {
        return fail(b"target spec");
    };
    if !contains(&spec, b"\"os\": \"ristux\"")
        || !contains(&spec, b"\"target-family\": \"unix\"")
        || !contains(&spec, b"\"panic-strategy\": \"abort\"")
        || !contains(&spec, b"\"disable-redzone\": true")
        || !contains(&spec, b"\"linker\": \"ristux-ld\"")
    {
        return fail(b"target spec");
    }
    line(b"rust_host_probe: target spec ok");
    true
}

fn check_package_index() -> bool {
    let Some(index) = read_file(b"/pkg/packages.txt") else {
        return fail(b"package index");
    };
    if !contains(&index, b"rustc 1.96.0 /bin/rustc")
        || !contains(&index, b"cargo 1.96.0 /bin/cargo")
        || !contains(&index, b"rustdoc 1.96.0 /bin/rustdoc")
        || !contains(&index, b"ristux-ld 0.3.0-bootstrap /bin/ristux-ld")
        || !contains(
            &index,
            b"ristux-ld 0.3.0-bootstrap /usr/lib/rustlib/x86_64-unknown-ristux/bin/ristux-ld",
        )
        || !contains(&index, b"rust-host-probe 0.1.0 /bin/rust_host_probe")
        || !contains(
            &index,
            b"rustc-metadata-probe 0.1.0 /bin/rustc_metadata_probe",
        )
        || !contains(
            &index,
            b"rust-ristux-overlays 1.96.0 /usr/lib/rustlib/src/ristux-overlays/",
        )
        || !contains(
            &index,
            b"rust-core-libs 1.96.0 /usr/lib/rustlib/x86_64-unknown-ristux/lib/libcore-",
        )
        || !contains(
            &index,
            b"rust-core-libs 1.96.0 /usr/lib/rustlib/x86_64-unknown-ristux/lib/liballoc-",
        )
        || !contains(
            &index,
            b"rust-core-libs 1.96.0 /usr/lib/rustlib/x86_64-unknown-ristux/lib/libcompiler_builtins-",
        )
        || !contains(
            &index,
            b"rust-std-libs 1.96.0 /usr/lib/rustlib/x86_64-unknown-ristux/lib/libstd-",
        )
        || !contains(
            &index,
            b"rust-std-libs 1.96.0 /usr/lib/rustlib/x86_64-unknown-ristux/lib/libpanic_abort-",
        )
    {
        return fail(b"package index");
    }
    line(b"rust_host_probe: package index ok");
    true
}

fn check_environment(envp: *const *const u8) -> bool {
    if !envp_has(envp, b"PATH") || !envp_has(envp, b"HOME") {
        return fail(b"environment");
    }
    line(b"rust_host_probe: environment ok");
    true
}

fn check_file_io_and_fd_flags() -> bool {
    let path = cstr(b"/tmp/rust-host-probe.txt");
    let _ = sys::unlink(path.as_ptr());
    let fd = sys::open(path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return fail(b"file io");
    }
    if write_all(fd as i32, b"ristux-host-probe\n") {
        let flags = sys::fcntl(fd as i32, sys::F_GETFD, 0);
        if flags < 0 || sys::fcntl(fd as i32, sys::F_SETFD, flags | sys::FD_CLOEXEC as isize) < 0 {
            let _ = sys::close(fd as i32);
            return fail(b"fd flags");
        }
        let updated = sys::fcntl(fd as i32, sys::F_GETFD, 0);
        if updated < 0 || updated & sys::FD_CLOEXEC as isize == 0 {
            let _ = sys::close(fd as i32);
            return fail(b"fd flags");
        }
        line(b"rust_host_probe: fd flags ok");
    } else {
        let _ = sys::close(fd as i32);
        return fail(b"file io");
    }
    let _ = sys::close(fd as i32);

    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return fail(b"file io");
    }
    if sys::lseek(fd as i32, 7, SEEK_SET) < 0 {
        let _ = sys::close(fd as i32);
        return fail(b"file io");
    }
    let mut buf = [0u8; 4];
    let n = sys::read(fd as i32, &mut buf);
    let _ = sys::close(fd as i32);
    let _ = sys::unlink(path.as_ptr());
    if n != 4 || &buf != b"host" {
        return fail(b"file io");
    }
    line(b"rust_host_probe: file io ok");
    true
}

fn check_std_syscalls() -> bool {
    let rustc = cstr(b"/bin/rustc");
    let mut stat_buf = [0u8; STAT_SIZE];
    if sys::stat(rustc.as_ptr(), stat_buf.as_mut_ptr()) < 0 || !stat_is_regular_nonempty(&stat_buf)
    {
        return fail(b"std syscalls");
    }
    stat_buf.fill(0);
    if sys::lstat(rustc.as_ptr(), stat_buf.as_mut_ptr()) < 0 || !stat_is_regular_nonempty(&stat_buf)
    {
        return fail(b"std syscalls");
    }
    let rustc_fd = sys::open(rustc.as_ptr(), O_RDONLY, 0);
    if rustc_fd < 0 {
        return fail(b"std syscalls");
    }
    stat_buf.fill(0);
    if sys::fstat(rustc_fd as i32, stat_buf.as_mut_ptr()) < 0
        || !stat_is_regular_nonempty(&stat_buf)
    {
        let _ = sys::close(rustc_fd as i32);
        return fail(b"std syscalls");
    }
    let _ = sys::close(rustc_fd as i32);

    let libdir = cstr(b"/usr/lib/rustlib");
    let rel_target = cstr(b"x86_64-unknown-ristux/target.json");
    let dirfd = sys::openat(sys::AT_FDCWD, libdir.as_ptr(), O_RDONLY, 0);
    if dirfd < 0 {
        return fail(b"std syscalls");
    }
    let target_fd = sys::openat(dirfd as i32, rel_target.as_ptr(), O_RDONLY, 0);
    if target_fd < 0 {
        let _ = sys::close(dirfd as i32);
        return fail(b"std syscalls");
    }
    let _ = sys::close(target_fd as i32);
    stat_buf.fill(0);
    if sys::newfstatat(dirfd as i32, rel_target.as_ptr(), stat_buf.as_mut_ptr(), 0) < 0
        || !stat_is_regular_nonempty(&stat_buf)
    {
        let _ = sys::close(dirfd as i32);
        return fail(b"std syscalls");
    }
    if sys::faccessat(dirfd as i32, rel_target.as_ptr(), sys::R_OK, 0) < 0 {
        let _ = sys::close(dirfd as i32);
        return fail(b"std syscalls");
    }
    let _ = sys::close(dirfd as i32);
    if sys::access(rustc.as_ptr(), sys::F_OK | sys::R_OK) < 0 {
        return fail(b"std syscalls");
    }
    let mut statx_buf = [0u8; STATX_SIZE];
    if sys::statx(
        sys::AT_FDCWD,
        rustc.as_ptr(),
        sys::AT_NO_AUTOMOUNT,
        sys::STATX_BASIC_STATS,
        statx_buf.as_mut_ptr(),
    ) < 0
        || read_le_u32(&statx_buf, 0) & sys::STATX_BASIC_STATS != sys::STATX_BASIC_STATS
        || read_le_u32(&statx_buf, 28) & S_IFMT != S_IFREG
        || read_le_u64(&statx_buf, 40) == 0
    {
        return fail(b"std syscalls");
    }

    let io_path = cstr(b"/tmp/rust-std-syscalls.bin");
    let _ = sys::unlink(io_path.as_ptr());
    let fd = sys::open(io_path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return fail(b"std syscalls");
    }
    let first = b"rust";
    let second = b"-std";
    let iov = [
        sys::Iovec {
            base: first.as_ptr() as *mut u8,
            len: first.len(),
        },
        sys::Iovec {
            base: second.as_ptr() as *mut u8,
            len: second.len(),
        },
    ];
    if sys::writev(fd as i32, &iov) != 8 || sys::pwrite64(fd as i32, b"host", 0) != 4 {
        let _ = sys::close(fd as i32);
        let _ = sys::unlink(io_path.as_ptr());
        return fail(b"std syscalls");
    }
    if sys::utimensat(sys::AT_FDCWD, io_path.as_ptr(), ptr::null(), 0) < 0 {
        let _ = sys::close(fd as i32);
        let _ = sys::unlink(io_path.as_ptr());
        return fail(b"std syscalls");
    }
    let _ = sys::close(fd as i32);

    let fd = sys::open(io_path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        let _ = sys::unlink(io_path.as_ptr());
        return fail(b"std syscalls");
    }
    let mut pread_buf = [0u8; 8];
    if sys::pread64(fd as i32, &mut pread_buf, 0) != 8 || &pread_buf != b"host-std" {
        let _ = sys::close(fd as i32);
        let _ = sys::unlink(io_path.as_ptr());
        return fail(b"std syscalls");
    }
    if sys::lseek(fd as i32, 0, SEEK_SET) < 0 {
        let _ = sys::close(fd as i32);
        let _ = sys::unlink(io_path.as_ptr());
        return fail(b"std syscalls");
    }
    let mut left = [0u8; 4];
    let mut right = [0u8; 4];
    let mut iov = [
        sys::Iovec {
            base: left.as_mut_ptr(),
            len: left.len(),
        },
        sys::Iovec {
            base: right.as_mut_ptr(),
            len: right.len(),
        },
    ];
    if sys::readv(fd as i32, &mut iov) != 8 || &left != b"host" || &right != b"-std" {
        let _ = sys::close(fd as i32);
        let _ = sys::unlink(io_path.as_ptr());
        return fail(b"std syscalls");
    }
    let _ = sys::close(fd as i32);
    let _ = sys::unlink(io_path.as_ptr());

    let mut pipefd = [0i32; 2];
    if sys::pipe2(pipefd.as_mut_ptr(), sys::O_CLOEXEC) < 0 {
        return fail(b"std syscalls");
    }
    let pipe_flags = sys::fcntl(pipefd[0], sys::F_GETFD, 0);
    if pipe_flags < 0 || pipe_flags & sys::FD_CLOEXEC as isize == 0 {
        let _ = sys::close(pipefd[0]);
        let _ = sys::close(pipefd[1]);
        return fail(b"std syscalls");
    }
    let dupfd = 7;
    if sys::dup3(pipefd[0], dupfd, sys::O_CLOEXEC) != dupfd as isize {
        let _ = sys::close(pipefd[0]);
        let _ = sys::close(pipefd[1]);
        return fail(b"std syscalls");
    }
    let dup_flags = sys::fcntl(dupfd, sys::F_GETFD, 0);
    let _ = sys::close(dupfd);
    let _ = sys::close(pipefd[0]);
    let _ = sys::close(pipefd[1]);
    if dup_flags < 0 || dup_flags & sys::FD_CLOEXEC as isize == 0 {
        return fail(b"std syscalls");
    }

    let mut random = [0u8; 16];
    if sys::getrandom(&mut random, 0) != random.len() as isize {
        return fail(b"std syscalls");
    }
    let mut rlim = sys::Rlimit::default();
    if sys::getrlimit(sys::RLIMIT_NOFILE, &mut rlim as *mut sys::Rlimit) < 0
        || rlim.cur == 0
        || rlim.max < rlim.cur
        || sys::setrlimit(sys::RLIMIT_NOFILE, &rlim as *const sys::Rlimit) < 0
    {
        return fail(b"std syscalls");
    }
    let mut usage = sys::Rusage::default();
    let mut tms = sys::Tms::default();
    if sys::getrusage(sys::RUSAGE_SELF, &mut usage as *mut sys::Rusage) < 0
        || sys::times(&mut tms as *mut sys::Tms) < 0
    {
        return fail(b"std syscalls");
    }

    line(b"rust_host_probe: std syscalls ok");
    true
}

fn cleanup_cargo_fs_probe() {
    for path in [
        b"/tmp/rust-cargo-fs/source.txt".as_slice(),
        b"/tmp/rust-cargo-fs/copy.txt".as_slice(),
        b"/tmp/rust-cargo-fs/copy-moved.txt".as_slice(),
        b"/tmp/rust-cargo-fs/copy-final.txt".as_slice(),
        b"/tmp/rust-cargo-fs/renamed.txt".as_slice(),
        b"/tmp/rust-cargo-fs/hard-at.txt".as_slice(),
        b"/tmp/rust-cargo-fs/hard.txt".as_slice(),
        b"/tmp/rust-cargo-fs/hard-moved.txt".as_slice(),
        b"/tmp/rust-cargo-fs/sym.txt".as_slice(),
    ] {
        let path = cstr(path);
        let _ = sys::unlink(path.as_ptr());
    }
    for path in [
        b"/tmp/rust-cargo-fs/inner".as_slice(),
        b"/tmp/rust-cargo-fs".as_slice(),
    ] {
        let path = cstr(path);
        let _ = sys::rmdir(path.as_ptr());
    }
}

fn fail_cargo_fs(dirfd: i32) -> bool {
    if dirfd >= 0 {
        let _ = sys::close(dirfd);
    }
    cleanup_cargo_fs_probe();
    fail(b"cargo fs syscalls")
}

fn check_cargo_fs_syscalls() -> bool {
    cleanup_cargo_fs_probe();

    let root = cstr(b"/tmp/rust-cargo-fs");
    if sys::mkdir(root.as_ptr(), 0o755) < 0 {
        cleanup_cargo_fs_probe();
        return fail(b"cargo fs syscalls");
    }
    let dirfd = sys::openat(sys::AT_FDCWD, root.as_ptr(), O_RDONLY, 0);
    if dirfd < 0 {
        return fail_cargo_fs(-1);
    }

    let inner = cstr(b"inner");
    if sys::mkdirat(dirfd as i32, inner.as_ptr(), 0o755) < 0
        || sys::unlinkat(dirfd as i32, inner.as_ptr(), sys::AT_REMOVEDIR) < 0
    {
        return fail_cargo_fs(dirfd as i32);
    }

    let source = cstr(b"source.txt");
    let fd = sys::openat(
        dirfd as i32,
        source.as_ptr(),
        O_WRONLY | O_CREAT | O_TRUNC,
        0o644,
    );
    if fd < 0 {
        return fail_cargo_fs(dirfd as i32);
    }
    if !write_all(fd as i32, b"cargo-cache")
        || sys::fsync(fd as i32) < 0
        || sys::ftruncate(fd as i32, 5) < 0
        || sys::fchmod(fd as i32, 0o600) < 0
        || sys::fchown(fd as i32, 0, 0) < 0
    {
        let _ = sys::close(fd as i32);
        return fail_cargo_fs(dirfd as i32);
    }
    let _ = sys::close(fd as i32);

    let source_fd = sys::openat(dirfd as i32, source.as_ptr(), O_RDONLY, 0);
    let copy = cstr(b"copy.txt");
    let copy_fd = sys::openat(
        dirfd as i32,
        copy.as_ptr(),
        O_WRONLY | O_CREAT | O_TRUNC,
        0o644,
    );
    if source_fd < 0 || copy_fd < 0 {
        if source_fd >= 0 {
            let _ = sys::close(source_fd as i32);
        }
        if copy_fd >= 0 {
            let _ = sys::close(copy_fd as i32);
        }
        return fail_cargo_fs(dirfd as i32);
    }
    let mut in_off = 0i64;
    let mut out_off = 0i64;
    let copied = sys::copy_file_range(
        source_fd as i32,
        &mut in_off as *mut i64,
        copy_fd as i32,
        &mut out_off as *mut i64,
        5,
        0,
    );
    let _ = sys::close(source_fd as i32);
    let _ = sys::close(copy_fd as i32);
    if copied != 5 || in_off != 5 || out_off != 5 {
        return fail_cargo_fs(dirfd as i32);
    }

    let copy_moved = cstr(b"copy-moved.txt");
    let copy_final = cstr(b"copy-final.txt");
    if sys::renameat2(
        dirfd as i32,
        copy.as_ptr(),
        dirfd as i32,
        copy_moved.as_ptr(),
        0,
    ) < 0
        || sys::renameat2(
            dirfd as i32,
            copy_moved.as_ptr(),
            dirfd as i32,
            source.as_ptr(),
            sys::RENAME_NOREPLACE,
        ) >= 0
        || sys::renameat2(
            dirfd as i32,
            copy_moved.as_ptr(),
            dirfd as i32,
            copy_final.as_ptr(),
            sys::RENAME_NOREPLACE,
        ) < 0
    {
        return fail_cargo_fs(dirfd as i32);
    }

    let renamed = cstr(b"renamed.txt");
    let hard_at = cstr(b"hard-at.txt");
    let sym = cstr(b"sym.txt");
    let symlink_target = cstr(b"renamed.txt");
    if sys::renameat(
        dirfd as i32,
        source.as_ptr(),
        dirfd as i32,
        renamed.as_ptr(),
    ) < 0
        || sys::linkat(
            dirfd as i32,
            renamed.as_ptr(),
            dirfd as i32,
            hard_at.as_ptr(),
            0,
        ) < 0
        || sys::symlinkat(symlink_target.as_ptr(), dirfd as i32, sym.as_ptr()) < 0
        || sys::fchmodat(dirfd as i32, renamed.as_ptr(), 0o644) < 0
        || sys::fchownat(dirfd as i32, renamed.as_ptr(), 0, 0, 0) < 0
    {
        return fail_cargo_fs(dirfd as i32);
    }

    let mut link_target = [0u8; 32];
    let n = sys::readlinkat(dirfd as i32, sym.as_ptr(), &mut link_target);
    let symlink_target_len = symlink_target.len() - 1;
    if n != symlink_target_len as isize
        || &link_target[..n as usize] != &symlink_target[..symlink_target.len() - 1]
    {
        return fail_cargo_fs(dirfd as i32);
    }

    let renamed_abs = cstr(b"/tmp/rust-cargo-fs/renamed.txt");
    let hard_abs = cstr(b"/tmp/rust-cargo-fs/hard.txt");
    let hard_moved_abs = cstr(b"/tmp/rust-cargo-fs/hard-moved.txt");
    let sym_abs = cstr(b"/tmp/rust-cargo-fs/sym.txt");
    let mut link_target_abs = [0u8; 32];
    let n = sys::readlink(sym_abs.as_ptr(), &mut link_target_abs);
    if n != symlink_target_len as isize
        || &link_target_abs[..n as usize] != &symlink_target[..symlink_target.len() - 1]
        || sys::link(renamed_abs.as_ptr(), hard_abs.as_ptr()) < 0
        || sys::rename(hard_abs.as_ptr(), hard_moved_abs.as_ptr()) < 0
        || sys::truncate(renamed_abs.as_ptr(), 2) < 0
    {
        return fail_cargo_fs(dirfd as i32);
    }

    let fd = sys::open(renamed_abs.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return fail_cargo_fs(dirfd as i32);
    }
    let mut truncated = [0u8; 2];
    let n = sys::read(fd as i32, &mut truncated);
    let _ = sys::close(fd as i32);
    if n != 2 || &truncated != b"ca" {
        return fail_cargo_fs(dirfd as i32);
    }

    let old_umask = sys::umask(0o022);
    if old_umask < 0 {
        return fail_cargo_fs(dirfd as i32);
    }
    let _ = sys::umask(old_umask as u32);

    let _ = sys::close(dirfd as i32);
    cleanup_cargo_fs_probe();
    line(b"rust_host_probe: cargo fs syscalls ok");
    true
}

fn check_process_signal_syscalls() -> bool {
    let pid = sys::getpid();
    let tid = sys::gettid();
    if pid <= 0 || tid != pid {
        return fail(b"process signal syscalls");
    }
    let empty = 0u64;
    let mut old = 0u64;
    let size = core::mem::size_of::<u64>();
    if sys::rt_sigprocmask(
        sys::SIG_BLOCK,
        &empty as *const u64,
        &mut old as *mut u64,
        size,
    ) < 0
        || sys::rt_sigprocmask(sys::SIG_SETMASK, &old as *const u64, ptr::null_mut(), size) < 0
    {
        return fail(b"process signal syscalls");
    }
    line(b"rust_host_probe: process signal syscalls ok");
    true
}

fn check_tls_syscalls() -> bool {
    let mut old_fs = 0usize;
    if sys::arch_prctl(sys::ARCH_GET_FS, &mut old_fs as *mut usize as usize) < 0 {
        return fail(b"tls get fs");
    }
    let mut gs = usize::MAX;
    if sys::arch_prctl(sys::ARCH_GET_GS, &mut gs as *mut usize as usize) < 0 || gs != 0 {
        return fail(b"tls get gs");
    }

    let addr = sys::mmap(
        0,
        4096,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
        -1,
        0,
    );
    if addr < 0 {
        return fail(b"tls mmap");
    }
    let mut new_fs = 0usize;
    if sys::arch_prctl(sys::ARCH_SET_FS, addr as usize) < 0 {
        let _ = sys::arch_prctl(sys::ARCH_SET_FS, old_fs);
        let _ = sys::munmap(addr as usize, 4096);
        return fail(b"tls set fs");
    }
    if sys::arch_prctl(sys::ARCH_GET_FS, &mut new_fs as *mut usize as usize) < 0 {
        let _ = sys::arch_prctl(sys::ARCH_SET_FS, old_fs);
        let _ = sys::munmap(addr as usize, 4096);
        return fail(b"tls get new fs");
    }
    if new_fs != addr as usize {
        let _ = sys::arch_prctl(sys::ARCH_SET_FS, old_fs);
        let _ = sys::munmap(addr as usize, 4096);
        return fail(b"tls fs value");
    }
    if sys::arch_prctl(sys::ARCH_SET_FS, old_fs) < 0 {
        let _ = sys::munmap(addr as usize, 4096);
        return fail(b"tls restore fs");
    }
    if sys::munmap(addr as usize, 4096) < 0 {
        return fail(b"tls munmap");
    }
    line(b"rust_host_probe: tls syscalls ok");
    true
}

fn check_host_runtime_syscalls() -> bool {
    let pid = sys::getpid();
    let mut clear_tid = 0i32;
    if pid <= 0 || sys::set_tid_address(&mut clear_tid as *mut i32) != pid {
        return fail(b"host runtime syscalls");
    }

    let mut robust_head = [0usize; 3];
    if sys::set_robust_list(
        robust_head.as_mut_ptr() as usize,
        core::mem::size_of_val(&robust_head),
    ) < 0
    {
        return fail(b"host runtime syscalls");
    }
    let mut robust_ptr = usize::MAX;
    let mut robust_len = 0usize;
    if sys::get_robust_list(
        0,
        &mut robust_ptr as *mut usize,
        &mut robust_len as *mut usize,
    ) < 0
        || robust_ptr != 0
        || robust_len != core::mem::size_of_val(&robust_head)
    {
        return fail(b"host runtime syscalls");
    }

    let mut cpu = u32::MAX;
    let mut node = u32::MAX;
    if sys::getcpu(&mut cpu as *mut u32, &mut node as *mut u32) < 0 || cpu != 0 || node != 0 {
        return fail(b"host runtime syscalls");
    }

    let rustc = cstr(b"/bin/rustc");
    let fd = sys::open(rustc.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return fail(b"host runtime syscalls");
    }
    let advice = sys::posix_fadvise(fd as i32, 0, 0, sys::POSIX_FADV_DONTNEED);
    let _ = sys::close(fd as i32);
    if advice < 0 {
        return fail(b"host runtime syscalls");
    }

    line(b"rust_host_probe: host runtime syscalls ok");
    true
}

fn check_host_platform_syscalls() -> bool {
    let addr = sys::mmap(
        0,
        4096,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
        -1,
        0,
    );
    if addr < 0 {
        return fail(b"host platform syscalls");
    }
    if sys::madvise(addr as usize, 4096, sys::MADV_NORMAL) < 0
        || sys::madvise(addr as usize, 4096, sys::MADV_DONTNEED) < 0
        || sys::munmap(addr as usize, 4096) < 0
    {
        let _ = sys::munmap(addr as usize, 4096);
        return fail(b"host platform syscalls");
    }

    let mut res = sys::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if sys::clock_getres(sys::CLOCK_MONOTONIC, &mut res as *mut sys::Timespec) < 0
        || res.tv_sec != 0
        || res.tv_nsec <= 0
        || res.tv_nsec > 1_000_000_000
    {
        return fail(b"host platform syscalls");
    }

    let mut affinity = [0u8; 16];
    let affinity_len = sys::sched_getaffinity(0, &mut affinity);
    if affinity_len <= 0 || read_le_u64(&affinity, 0) == 0 {
        return fail(b"host platform syscalls");
    }

    let mut info = sys::SysInfo::default();
    if sys::sysinfo(&mut info as *mut sys::SysInfo) < 0 {
        return fail(b"host platform syscalls");
    }
    let total_ram = read_le_u64(&info.bytes, 32);
    let free_ram = read_le_u64(&info.bytes, 40);
    let mem_unit = read_le_u32(&info.bytes, 104);
    if total_ram == 0 || free_ram == 0 || free_ram > total_ram || mem_unit != 1 {
        return fail(b"host platform syscalls");
    }

    let mut limit = sys::Rlimit::default();
    if sys::prlimit64(
        0,
        sys::RLIMIT_NOFILE,
        ptr::null(),
        &mut limit as *mut sys::Rlimit,
    ) < 0
        || limit.cur == 0
        || limit.max < limit.cur
        || sys::prlimit64(
            0,
            sys::RLIMIT_NOFILE,
            &limit as *const sys::Rlimit,
            ptr::null_mut(),
        ) < 0
    {
        return fail(b"host platform syscalls");
    }

    line(b"rust_host_probe: host platform syscalls ok");
    true
}

fn check_dir_traversal() -> bool {
    if !dir_contains(b"/usr/lib/rustlib", b"rust-1.96.0-manifest.toml")
        || !dir_contains(b"/usr/lib/rustlib", b"x86_64-unknown-ristux")
    {
        return fail(b"dir traversal");
    }
    line(b"rust_host_probe: dir traversal ok");
    true
}

fn check_sysroot_libraries() -> bool {
    let libdir = b"/usr/lib/rustlib/x86_64-unknown-ristux/lib";
    if !dir_contains_matching(libdir, b"libcore-", b".rlib")
        || !dir_contains_matching(libdir, b"libcore-", b".rmeta")
        || !dir_contains_matching(libdir, b"liballoc-", b".rlib")
        || !dir_contains_matching(libdir, b"liballoc-", b".rmeta")
        || !dir_contains_matching(libdir, b"libcompiler_builtins-", b".rlib")
        || !dir_contains_matching(libdir, b"libcompiler_builtins-", b".rmeta")
        || !dir_contains_matching(libdir, b"libstd-", b".rlib")
        || !dir_contains_matching(libdir, b"libstd-", b".rmeta")
        || !dir_contains_matching(libdir, b"libpanic_abort-", b".rlib")
        || !dir_contains_matching(libdir, b"libpanic_abort-", b".rmeta")
        || !dir_contains_matching(libdir, b"liblibc-", b".rlib")
        || !dir_contains_matching(libdir, b"liblibc-", b".rmeta")
    {
        return fail(b"sysroot libs");
    }
    line(b"rust_host_probe: sysroot libs ok");
    true
}

fn check_overlay_sources() -> bool {
    let base = b"/usr/lib/rustlib/src/ristux-overlays";
    let Some(readme) = read_file(b"/usr/lib/rustlib/src/ristux-overlays/README.md") else {
        return fail(b"overlay sources");
    };
    let Some(probe) = read_file(b"/usr/lib/rustlib/src/ristux-overlays/probe/restricted_main.rs")
    else {
        return fail(b"overlay sources");
    };
    let Some(libc) =
        read_file(b"/usr/lib/rustlib/src/ristux-overlays/libc/src/unix/ristux_syscalls.rs")
    else {
        return fail(b"overlay sources");
    };
    let Some(futex) = read_file(
        b"/usr/lib/rustlib/src/ristux-overlays/rust-src/library/std/src/sys/pal/unix/futex.rs",
    ) else {
        return fail(b"overlay sources");
    };
    let Some(alloc) = read_file(
        b"/usr/lib/rustlib/src/ristux-overlays/rust-src/library/std/src/sys/alloc/unix.rs",
    ) else {
        return fail(b"overlay sources");
    };
    let Some(os_mod) = read_file(
        b"/usr/lib/rustlib/src/ristux-overlays/rust-src/library/std/src/os/ristux/mod.rs",
    ) else {
        return fail(b"overlay sources");
    };

    if !dir_contains(base, b"README.md")
        || !contains(&readme, b"Ristux Rust 1.96.0 Overlays")
        || !contains(&probe, b"hello from Ristux std")
        || !contains(&libc, b"pub unsafe extern \"C\" fn write")
        || !contains(&futex, b"NR_FUTEX: usize = 202")
        || !contains(&alloc, b"NR_BRK: usize = 12")
        || !contains(&os_mod, b"pub mod fs")
    {
        return fail(b"overlay sources");
    }

    line(b"rust_host_probe: overlay sources ok");
    true
}

fn check_process_capture(envp: *const *const u8) -> bool {
    let Some(rustc) = spawn_capture(b"/bin/rustc", b"rustc", &[b"--version"], envp) else {
        return fail(b"process capture");
    };
    let Some(cargo) = spawn_capture(b"/bin/cargo", b"cargo", &[b"--version"], envp) else {
        return fail(b"process capture");
    };
    let Some(rustdoc) = spawn_capture(b"/bin/rustdoc", b"rustdoc", &[b"--version"], envp) else {
        return fail(b"process capture");
    };
    let Some(linker) = spawn_capture(b"/bin/ristux-ld", b"ristux-ld", &[b"--version"], envp) else {
        return fail(b"process capture");
    };
    if !contains(&rustc, b"rustc 1.96.0")
        || !contains(&cargo, b"cargo 1.96.0")
        || !contains(&rustdoc, b"rustdoc 1.96.0")
        || !contains(&linker, b"ristux-ld 0.3.0-bootstrap")
    {
        return fail(b"process capture");
    }
    line(b"rust_host_probe: process capture ok");
    true
}

fn check_memory_map() -> bool {
    let addr = sys::mmap(
        0,
        4096,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
        -1,
        0,
    );
    if addr < 0 {
        return fail(b"memory map");
    }
    unsafe {
        let ptr = addr as *mut u8;
        *ptr = 0x52;
        if *ptr != 0x52 {
            let _ = sys::munmap(addr as usize, 4096);
            return fail(b"memory map");
        }
    }
    if sys::mprotect(addr as usize, 4096, sys::PROT_READ) < 0 {
        let _ = sys::munmap(addr as usize, 4096);
        return fail(b"memory map");
    }
    if sys::munmap(addr as usize, 4096) < 0 {
        return fail(b"memory map");
    }
    line(b"rust_host_probe: memory map ok");
    true
}

fn check_clocks() -> bool {
    let mut realtime = sys::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut monotonic = sys::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut timeval = sys::Timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    if sys::clock_gettime(sys::CLOCK_REALTIME, &mut realtime as *mut sys::Timespec) < 0
        || sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut monotonic as *mut sys::Timespec) < 0
        || sys::gettimeofday(&mut timeval as *mut sys::Timeval) < 0
        || realtime.tv_nsec < 0
        || realtime.tv_nsec >= 1_000_000_000
        || monotonic.tv_nsec < 0
        || monotonic.tv_nsec >= 1_000_000_000
        || timeval.tv_usec < 0
        || timeval.tv_usec >= 1_000_000
    {
        return fail(b"clocks");
    }
    line(b"rust_host_probe: clocks ok");
    true
}

#[repr(C)]
struct ClearTidProbe {
    clear_tid: u32,
    go: u32,
    child_seen: u32,
}

extern "C" fn clear_tid_thread_entry(arg: usize) -> ! {
    let probe = arg as *mut ClearTidProbe;
    unsafe {
        while ptr::read_volatile(&(*probe).go) == 0 {
            let _ = sys::sched_yield();
        }
        ptr::write_volatile(&mut (*probe).child_seen, 1);
    }
    sys::exit(0);
}

fn check_clear_child_tid_wake() -> bool {
    const STACK_LEN: usize = 64 * 1024;
    let stack = sys::mmap(
        0,
        STACK_LEN,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
        -1,
        0,
    );
    if stack < 0 {
        return fail(b"clear child tid mmap");
    }

    let stack_top = ((stack as usize + STACK_LEN) & !0xfusize) - core::mem::size_of::<usize>();
    let mut probe = ClearTidProbe {
        clear_tid: 0,
        go: 0,
        child_seen: 0,
    };
    let pid = sys::ristux_thread_create(
        clear_tid_thread_entry as *const () as usize,
        &mut probe as *mut ClearTidProbe as usize,
        stack_top,
        0,
        &mut probe.clear_tid as *mut u32,
    );
    if pid <= 0 {
        let _ = sys::munmap(stack as usize, STACK_LEN);
        return fail(b"clear child tid create");
    }

    let pid_word = pid as u32;
    if unsafe { ptr::read_volatile(&probe.clear_tid) } != pid_word {
        let _ = sys::munmap(stack as usize, STACK_LEN);
        return fail(b"clear child tid publish");
    }

    unsafe {
        ptr::write_volatile(&mut probe.go, 1);
    }
    let timeout = sys::Timespec {
        tv_sec: 0,
        tv_nsec: 50_000_000,
    };
    loop {
        let current = unsafe { ptr::read_volatile(&probe.clear_tid) };
        if current == 0 {
            break;
        }
        let rc = sys::futex(
            &mut probe.clear_tid as *mut u32,
            sys::FUTEX_WAIT | sys::FUTEX_PRIVATE_FLAG,
            current as i32,
            &timeout as *const sys::Timespec,
        );
        if rc < 0 {
            let after = unsafe { ptr::read_volatile(&probe.clear_tid) };
            if after != 0 {
                let _ = sys::munmap(stack as usize, STACK_LEN);
                return fail(b"clear child tid futex");
            }
        }
    }
    if unsafe { ptr::read_volatile(&probe.child_seen) } != 1 {
        let _ = sys::munmap(stack as usize, STACK_LEN);
        return fail(b"clear child tid child");
    }
    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) != pid || status != 0 {
        let _ = sys::munmap(stack as usize, STACK_LEN);
        return fail(b"clear child tid wait");
    }
    let _ = sys::munmap(stack as usize, STACK_LEN);
    line(b"rust_host_probe: clear child tid wake ok");
    true
}

#[repr(C)]
struct ExitGroupProbe {
    started: u32,
}

extern "C" fn exit_group_spin_thread(arg: usize) -> ! {
    let probe = arg as *mut ExitGroupProbe;
    unsafe {
        ptr::write_volatile(&mut (*probe).started, 1);
    }
    loop {
        let _ = sys::sched_yield();
    }
}

fn run_exit_group_child() -> ! {
    const STACK_LEN: usize = 64 * 1024;
    let stack = sys::mmap(
        0,
        STACK_LEN,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
        -1,
        0,
    );
    if stack < 0 {
        sys::exit_group(111);
    }
    let stack_top = ((stack as usize + STACK_LEN) & !0xfusize) - core::mem::size_of::<usize>();
    let mut probe = ExitGroupProbe { started: 0 };
    let tid = sys::ristux_thread_create(
        exit_group_spin_thread as *const () as usize,
        &mut probe as *mut ExitGroupProbe as usize,
        stack_top,
        0,
        ptr::null_mut(),
    );
    if tid <= 0 {
        let _ = sys::munmap(stack as usize, STACK_LEN);
        sys::exit_group(112);
    }
    for _ in 0..256 {
        if unsafe { ptr::read_volatile(&probe.started) } != 0 {
            sys::exit_group(0);
        }
        let _ = sys::sched_yield();
    }
    sys::exit_group(113);
}

fn check_exit_group_reaps_threads() -> bool {
    let Some(before) = count_processes_named(b"/bin/rust_host_probe") else {
        return fail(b"exit group proc baseline");
    };
    let pid = sys::fork();
    if pid < 0 {
        return fail(b"exit group fork");
    }
    if pid == 0 {
        run_exit_group_child();
    }

    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) != pid || status != 0 {
        return fail(b"exit group wait");
    }

    let nap = sys::Timespec {
        tv_sec: 0,
        tv_nsec: 1_000_000,
    };
    for _ in 0..16 {
        let _ = sys::nanosleep(&nap);
    }
    let Some(after) = count_processes_named(b"/bin/rust_host_probe") else {
        return fail(b"exit group proc after");
    };
    if after > before {
        return fail(b"exit group leaked thread");
    }
    line(b"rust_host_probe: exit group threads ok");
    true
}

fn check_synchronization() -> bool {
    let mut word = 0u32;
    let timeout = sys::Timespec {
        tv_sec: 0,
        tv_nsec: 1_000_000,
    };
    let mismatch = sys::futex(
        &mut word as *mut u32,
        sys::FUTEX_WAIT,
        1,
        &timeout as *const sys::Timespec,
    );
    let wake = sys::futex(&mut word as *mut u32, sys::FUTEX_WAKE, 1, ptr::null());
    if mismatch >= 0 || wake < 0 {
        return fail(b"synchronization");
    }
    line(b"rust_host_probe: synchronization ok");
    true
}

fn main(args: &[&[u8]], envp: *const *const u8) -> i32 {
    if args.iter().any(|arg| *arg == b"--help" || *arg == b"-h") {
        line(b"usage: rust_host_probe");
        line(b"checks the Ristux native Rust host runtime surface");
        return 0;
    }
    if args.iter().any(|arg| *arg == b"--version" || *arg == b"-V") {
        line(b"rust_host_probe 0.1.0");
        return 0;
    }
    if check_toolchain_files()
        && check_manifest()
        && check_target_spec()
        && check_package_index()
        && check_environment(envp)
        && check_file_io_and_fd_flags()
        && check_std_syscalls()
        && check_cargo_fs_syscalls()
        && check_process_signal_syscalls()
        && check_tls_syscalls()
        && check_host_runtime_syscalls()
        && check_host_platform_syscalls()
        && check_dir_traversal()
        && check_sysroot_libraries()
        && check_overlay_sources()
        && check_process_capture(envp)
        && check_memory_map()
        && check_clocks()
        && check_clear_child_tid_wake()
        && check_exit_group_reaps_threads()
        && check_synchronization()
    {
        line(b"rust_host_probe: done");
        return 0;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(argc: i64, argv: *const *const u8, envp: *const *const u8) -> ! {
    let argc = if argc < 0 { 0 } else { argc as usize };
    let args = ristux_userland::argv_slice(argc, argv);
    let arg_refs: Vec<&[u8]> = args.iter().map(|arg| *arg).collect();
    let status = main(&arg_refs, envp);
    sys::exit(status);
}
