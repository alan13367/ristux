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
const NR_MKDIR: usize = 83;
const BASE_INDEX: &[u8] = b"/pkg/packages.txt";
const LOCAL_INDEX: &[u8] = b"/var/pkg/packages.txt";
const BASE_DB: &[u8] = b"/pkg/db/";
const LOCAL_DB: &[u8] = b"/var/pkg/db/";

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let written = sys::write(fd, bytes);
        if written <= 0 {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), 0, 0);
    if fd < 0 {
        return None;
    }

    let mut out = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let read = sys::read(fd as i32, &mut buf);
        if read < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn write_file(path: &[u8], bytes: &[u8], mode: u32) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, mode);
    if fd < 0 {
        return false;
    }
    let ok = write_all(fd as i32, bytes);
    let _ = sys::close(fd as i32);
    ok
}

fn mkdir(path: &[u8], mode: u32) -> bool {
    let path = cstr(path);
    let result = unsafe { sys::syscall2(NR_MKDIR, path.as_ptr() as usize, mode as usize) };
    result >= 0 || result == -17
}

fn db_path(base: &[u8], name: &[u8], file: &[u8]) -> Vec<u8> {
    let mut path = Vec::new();
    path.extend_from_slice(base);
    path.extend_from_slice(name);
    path.push(b'/');
    path.extend_from_slice(file);
    path
}

fn read_db_file(name: &[u8], file: &[u8]) -> Option<Vec<u8>> {
    if !safe_package_name(name) {
        return None;
    }
    read_file(&db_path(LOCAL_DB, name, file)).or_else(|| read_file(&db_path(BASE_DB, name, file)))
}

fn safe_package_name(name: &[u8]) -> bool {
    !name.is_empty()
        && !name.starts_with(b".")
        && name.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || *byte == b'-'
                || *byte == b'_'
                || *byte == b'.'
                || *byte == b'+'
        })
}

fn split_fields(line: &[u8]) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = None;
    for (index, byte) in line.iter().enumerate() {
        if byte.is_ascii_whitespace() {
            if let Some(field_start) = start.take() {
                fields.push(&line[field_start..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(field_start) = start {
        fields.push(&line[field_start..]);
    }
    fields
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn for_lines(bytes: &[u8], mut f: impl FnMut(&[u8])) {
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            let mut line = &bytes[start..index];
            if line.ends_with(b"\r") {
                line = &line[..line.len() - 1];
            }
            f(line);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        f(&bytes[start..]);
    }
}

fn print_line(bytes: &[u8]) {
    let _ = write_all(1, bytes);
    let _ = write_all(1, b"\n");
}

fn print_usage() {
    let _ = write_all(2, b"usage: pkg list | pkg info NAME | pkg files NAME | pkg deps NAME | pkg hook NAME | pkg run-hook NAME | pkg verify NAME | pkg register NAME VERSION FILELIST [DEP...]\n");
}

fn list_packages() -> i32 {
    let base_index = read_file(BASE_INDEX);
    let local_index = read_file(LOCAL_INDEX);
    if base_index.is_none() && local_index.is_none() {
        let _ = write_all(2, b"pkg: cannot read package index\n");
        return 1;
    }
    let mut seen: Vec<Vec<u8>> = Vec::new();
    if let Some(index) = local_index {
        print_package_index(&index, &mut seen);
    }
    if let Some(index) = base_index {
        print_package_index(&index, &mut seen);
    }
    0
}

fn print_package_index(index: &[u8], seen: &mut Vec<Vec<u8>>) {
    for_lines(index, |line| {
        if line.is_empty() || line.starts_with(b"#") {
            return;
        }
        let fields = split_fields(line);
        if fields.len() < 2 || seen.iter().any(|name| name.as_slice() == fields[0]) {
            return;
        }
        seen.push(fields[0].to_vec());
        let _ = write_all(1, fields[0]);
        let _ = write_all(1, b" ");
        let _ = write_all(1, fields[1]);
        let _ = write_all(1, b"\n");
    });
}

fn print_indented_lines(bytes: &[u8]) {
    for_lines(bytes, |line| {
        if line.is_empty() {
            return;
        }
        let _ = write_all(1, b"  ");
        print_line(line);
    });
}

fn first_line(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap_or(bytes.len());
    let line = &bytes[..end];
    if line.ends_with(b"\r") {
        &line[..line.len() - 1]
    } else {
        line
    }
}

fn info_package(name: &[u8]) -> i32 {
    let Some(version) = read_db_file(name, b"version") else {
        let _ = write_all(2, b"pkg: unknown package ");
        let _ = write_all(2, name);
        let _ = write_all(2, b"\n");
        return 1;
    };
    let files = read_db_file(name, b"files").unwrap_or_default();
    let deps = read_db_file(name, b"dependencies").unwrap_or_default();
    let hook = read_db_file(name, b"post-install").unwrap_or_default();

    let _ = write_all(1, b"name: ");
    print_line(name);
    let _ = write_all(1, b"version: ");
    print_line(first_line(&version));
    let _ = write_all(1, b"files:\n");
    print_indented_lines(&files);
    let _ = write_all(1, b"dependencies:\n");
    print_indented_lines(&deps);
    let _ = write_all(1, b"post-install:\n");
    print_indented_lines(&hook);
    0
}

fn print_db_file(name: &[u8], file: &[u8]) -> i32 {
    let Some(bytes) = read_db_file(name, file) else {
        let _ = write_all(2, b"pkg: unknown package ");
        let _ = write_all(2, name);
        let _ = write_all(2, b"\n");
        return 1;
    };
    let _ = write_all(1, &bytes);
    0
}

fn wait_exit_status(status: i32) -> i32 {
    if (status & 0xff) == 0 {
        (status >> 8) & 0xff
    } else {
        128 + (status & 0x7f)
    }
}

fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn append_hex_u64(out: &mut Vec<u8>, value: u64) {
    for shift in (0..16).rev() {
        let nibble = ((value >> (shift * 4)) & 0xf) as u8;
        out.push(if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + (nibble - 10)
        });
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_hex_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > 16 {
        return None;
    }
    let mut value = 0u64;
    for byte in bytes {
        value = (value << 4) | hex_value(*byte)? as u64;
    }
    Some(value)
}

fn valid_installed_path(path: &[u8]) -> bool {
    path.starts_with(b"/")
        && !path.contains(&0)
        && !path.windows(4).any(|window| window == b"/../")
        && !path.ends_with(b"/..")
}

fn read_file_list(path: &[u8]) -> Option<Vec<Vec<u8>>> {
    let bytes = read_file(path)?;
    let mut files = Vec::new();
    let mut ok = true;
    for_lines(&bytes, |line| {
        let line = trim_ascii(line);
        if line.is_empty() || line.starts_with(b"#") {
            return;
        }
        if !valid_installed_path(line) {
            ok = false;
            return;
        }
        files.push(line.to_vec());
    });
    if ok && !files.is_empty() {
        files.sort();
        files.dedup();
        Some(files)
    } else {
        None
    }
}

fn write_db_file(name: &[u8], file: &[u8], bytes: &[u8]) -> bool {
    write_file(&db_path(LOCAL_DB, name, file), bytes, 0o644)
}

fn rebuild_package_index(name: &[u8], version: &[u8], files: &[Vec<u8>]) -> Option<Vec<u8>> {
    let existing = read_file(LOCAL_INDEX).unwrap_or_default();
    let mut output = Vec::new();
    let mut saw_header = false;
    for_lines(&existing, |line| {
        if line.starts_with(b"#") {
            output.extend_from_slice(line);
            output.push(b'\n');
            saw_header = true;
            return;
        }
        let fields = split_fields(line);
        if fields.first().copied() == Some(name) {
            return;
        }
        if !line.is_empty() {
            output.extend_from_slice(line);
            output.push(b'\n');
        }
    });
    if !saw_header {
        output.extend_from_slice(b"# name version path checksum\n");
    }

    for path in files {
        let data = read_file(path)?;
        output.extend_from_slice(name);
        output.push(b' ');
        output.extend_from_slice(version);
        output.push(b' ');
        output.extend_from_slice(path);
        output.push(b' ');
        append_hex_u64(&mut output, checksum(&data));
        output.push(b'\n');
    }
    Some(output)
}

fn collect_index_entries(index: &[u8], name: &[u8]) -> Vec<(Vec<u8>, u64)> {
    let mut entries = Vec::new();
    for_lines(index, |line| {
        if line.is_empty() || line.starts_with(b"#") {
            return;
        }
        let fields = split_fields(line);
        if fields.len() < 4 || fields[0] != name {
            return;
        }
        let Some(expected) = parse_hex_u64(fields[3]) else {
            return;
        };
        entries.push((fields[2].to_vec(), expected));
    });
    entries
}

fn package_index_entries(name: &[u8]) -> Vec<(Vec<u8>, u64)> {
    if let Some(index) = read_file(LOCAL_INDEX) {
        let entries = collect_index_entries(&index, name);
        if !entries.is_empty() {
            return entries;
        }
    }
    read_file(BASE_INDEX)
        .map(|index| collect_index_entries(&index, name))
        .unwrap_or_default()
}

fn verify_package(name: &[u8]) -> i32 {
    if !safe_package_name(name) {
        let _ = write_all(2, b"pkg: invalid package name\n");
        return 1;
    }
    let entries = package_index_entries(name);
    let Some(files) = read_db_file(name, b"files") else {
        let _ = write_all(2, b"pkg: unknown package ");
        let _ = write_all(2, name);
        let _ = write_all(2, b"\n");
        return 1;
    };
    if entries.is_empty() {
        let _ = write_all(2, b"pkg: no index entries for ");
        let _ = write_all(2, name);
        let _ = write_all(2, b"\n");
        return 1;
    }

    let mut listed = 0usize;
    let mut ok = true;
    for_lines(&files, |line| {
        let path = trim_ascii(line);
        if path.is_empty() {
            return;
        }
        listed += 1;
        if !entries
            .iter()
            .any(|(entry_path, _)| entry_path.as_slice() == path)
        {
            let _ = write_all(2, b"pkg: missing index entry ");
            let _ = write_all(2, path);
            let _ = write_all(2, b"\n");
            ok = false;
        }
    });
    if listed != entries.len() {
        let _ = write_all(2, b"pkg: file list/index mismatch\n");
        ok = false;
    }
    if !ok {
        return 1;
    }

    for (path, expected) in entries {
        let Some(data) = read_file(&path) else {
            let _ = write_all(2, b"pkg: missing ");
            let _ = write_all(2, &path);
            let _ = write_all(2, b"\n");
            return 1;
        };
        if checksum(&data) != expected {
            let _ = write_all(2, b"pkg: checksum mismatch ");
            let _ = write_all(2, &path);
            let _ = write_all(2, b"\n");
            return 1;
        }
    }

    let _ = write_all(1, b"verified ");
    let _ = write_all(1, name);
    let _ = write_all(1, b"\n");
    0
}

fn register_package(args: &[&[u8]]) -> i32 {
    let name = args[2];
    let version = args[3];
    let list_path = args[4];
    let deps = &args[5..];

    if !safe_package_name(name) || version.is_empty() || version.contains(&b'\n') {
        let _ = write_all(2, b"pkg: invalid package name or version\n");
        return 1;
    }

    let Some(files) = read_file_list(list_path) else {
        let _ = write_all(2, b"pkg: invalid or empty file list\n");
        return 1;
    };
    for dep in deps {
        if !safe_package_name(dep) {
            let _ = write_all(2, b"pkg: invalid dependency ");
            let _ = write_all(2, dep);
            let _ = write_all(2, b"\n");
            return 1;
        }
    }

    let Some(index) = rebuild_package_index(name, version, &files) else {
        let _ = write_all(2, b"pkg: package file missing\n");
        return 1;
    };

    let mut base = Vec::new();
    let _ = mkdir(b"/var", 0o755);
    let _ = mkdir(b"/var/pkg", 0o755);
    let _ = mkdir(b"/var/pkg/db", 0o755);
    base.extend_from_slice(LOCAL_DB);
    base.extend_from_slice(name);
    let _ = mkdir(&base, 0o755);

    let mut version_bytes = version.to_vec();
    version_bytes.push(b'\n');
    let mut files_bytes = Vec::new();
    for path in &files {
        files_bytes.extend_from_slice(path);
        files_bytes.push(b'\n');
    }
    let mut deps_bytes = Vec::new();
    for dep in deps {
        deps_bytes.extend_from_slice(dep);
        deps_bytes.push(b'\n');
    }

    if !write_db_file(name, b"version", &version_bytes)
        || !write_db_file(name, b"files", &files_bytes)
        || !write_db_file(name, b"dependencies", &deps_bytes)
        || !write_db_file(name, b"post-install", b"\n")
        || !write_file(LOCAL_INDEX, &index, 0o644)
    {
        let _ = write_all(2, b"pkg: cannot write package database\n");
        return 1;
    }

    let _ = write_all(1, b"registered ");
    let _ = write_all(1, name);
    let _ = write_all(1, b" ");
    let _ = write_all(1, version);
    let _ = write_all(1, b"\n");
    0
}

fn run_hook(name: &[u8]) -> i32 {
    let Some(bytes) = read_db_file(name, b"post-install") else {
        let _ = write_all(2, b"pkg: unknown package ");
        let _ = write_all(2, name);
        let _ = write_all(2, b"\n");
        return 1;
    };
    let hook = first_line(&bytes);
    if hook.is_empty() {
        return 0;
    }

    let pid = sys::fork();
    if pid < 0 {
        let _ = write_all(2, b"pkg: fork failed\n");
        return 1;
    }
    if pid == 0 {
        let path = cstr(b"/bin/sh");
        let arg0 = cstr(b"sh");
        let flag = cstr(b"-c");
        let command = cstr(hook);
        let argv = [arg0.as_ptr(), flag.as_ptr(), command.as_ptr(), ptr::null()];
        let envp = [ptr::null()];
        let _ = sys::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
        let _ = write_all(2, b"pkg: hook exec failed\n");
        sys::exit(127);
    }

    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) < 0 {
        let _ = write_all(2, b"pkg: wait failed\n");
        return 1;
    }
    wait_exit_status(status)
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() == 2 && args[1] == b"list" {
        return list_packages();
    }
    if args.len() == 3 && args[1] == b"info" {
        return info_package(args[2]);
    }
    if args.len() == 3 && args[1] == b"files" {
        return print_db_file(args[2], b"files");
    }
    if args.len() == 3 && args[1] == b"deps" {
        return print_db_file(args[2], b"dependencies");
    }
    if args.len() == 3 && args[1] == b"hook" {
        return print_db_file(args[2], b"post-install");
    }
    if args.len() == 3 && args[1] == b"run-hook" {
        return run_hook(args[2]);
    }
    if args.len() == 3 && args[1] == b"verify" {
        return verify_package(args[2]);
    }
    if args.len() >= 5 && args[1] == b"register" {
        return register_package(args);
    }
    print_usage();
    2
}

ristux_userland::program_main!(main);
