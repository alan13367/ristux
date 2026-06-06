#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::{vec, vec::Vec};
use core::ptr;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 64;
const O_TRUNC: i32 = 512;
const TARGET: &[u8] = b"x86_64-unknown-ristux";
const SYSROOT: &[u8] = b"/usr";
const PANIC_RUNTIME: &[u8] = b"/usr/lib/rustlib/x86_64-unknown-ristux/lib/libristux_panic.rlib";

struct Package {
    name: Vec<u8>,
    crate_name: Vec<u8>,
    edition: Vec<u8>,
    manifest_dir: Vec<u8>,
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

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn strip_comment(line: &[u8]) -> &[u8] {
    let mut quoted = false;
    for (index, byte) in line.iter().enumerate() {
        if *byte == b'"' {
            quoted = !quoted;
        } else if *byte == b'#' && !quoted {
            return &line[..index];
        }
    }
    line
}

fn parse_string(value: &[u8]) -> Option<&[u8]> {
    let value = trim_ascii(value);
    if value.len() < 2 || value[0] != b'"' || value[value.len() - 1] != b'"' {
        return None;
    }
    let value = &value[1..value.len() - 1];
    if value.contains(&b'"') || value.contains(&b'\\') {
        return None;
    }
    Some(value)
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
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

fn path_exists(path: &[u8]) -> bool {
    let path = cstr(path);
    let mut stat_buf = [0u8; 144];
    sys::stat(path.as_ptr(), stat_buf.as_mut_ptr()) >= 0
}

fn is_dir(path: &[u8]) -> bool {
    let path = cstr(path);
    let mut stat_buf = [0u8; 144];
    if sys::stat(path.as_ptr(), stat_buf.as_mut_ptr()) < 0 {
        return false;
    }
    let mode = u32::from_le_bytes([stat_buf[24], stat_buf[25], stat_buf[26], stat_buf[27]]);
    mode & 0o170000 == 0o040000
}

fn ensure_dir(path: &[u8]) -> bool {
    if path.is_empty() || path == b"." || is_dir(path) {
        return true;
    }
    let mut end = usize::from(path.starts_with(b"/"));
    while end <= path.len() {
        let next = path[end..]
            .iter()
            .position(|byte| *byte == b'/')
            .map(|offset| end + offset)
            .unwrap_or(path.len());
        if next > 0 {
            let component = &path[..next];
            if !component.is_empty() && !is_dir(component) {
                let component_c = cstr(component);
                if sys::mkdir(component_c.as_ptr(), 0o755) < 0 && !is_dir(component) {
                    return false;
                }
            }
        }
        if next == path.len() {
            break;
        }
        end = next + 1;
    }
    true
}

fn join(base: &[u8], child: &[u8]) -> Vec<u8> {
    if base.is_empty() || base == b"." {
        return child.to_vec();
    }
    let mut out = Vec::with_capacity(base.len() + child.len() + 1);
    out.extend_from_slice(base);
    if !base.ends_with(b"/") {
        out.push(b'/');
    }
    out.extend_from_slice(child);
    out
}

fn dirname(path: &[u8]) -> Vec<u8> {
    let path = path.strip_suffix(b"/").unwrap_or(path);
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(0) => b"/".to_vec(),
        Some(index) => path[..index].to_vec(),
        None => b".".to_vec(),
    }
}

fn current_dir() -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 4096];
    let len = sys::getcwd(buf.as_mut_ptr(), buf.len());
    if len <= 0 {
        return None;
    }
    let len = len as usize;
    let end = buf[..len].iter().position(|byte| *byte == 0).unwrap_or(len);
    buf.truncate(end);
    Some(buf)
}

fn valid_package_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-'))
        && name[0].is_ascii_alphanumeric()
}

fn crate_name(name: &[u8]) -> Vec<u8> {
    name.iter()
        .map(|byte| if *byte == b'-' { b'_' } else { *byte })
        .collect()
}

fn parse_manifest(path: &[u8]) -> Result<Package, &'static [u8]> {
    let bytes = read_file(path).ok_or(b"cannot read Cargo.toml".as_slice())?;
    let mut section: &[u8] = b"";
    let mut name = None;
    let mut edition = b"2015".to_vec();
    let mut dependency_entries = false;

    for raw_line in bytes.split(|byte| *byte == b'\n') {
        let line = trim_ascii(strip_comment(raw_line));
        if line.is_empty() {
            continue;
        }
        if line.starts_with(b"[") && line.ends_with(b"]") {
            section = trim_ascii(&line[1..line.len() - 1]);
            continue;
        }
        let Some(equals) = line.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        let key = trim_ascii(&line[..equals]);
        let value = &line[equals + 1..];
        if section == b"package" && key == b"name" {
            name = parse_string(value).map(|value| value.to_vec());
        } else if section == b"package" && key == b"edition" {
            edition = parse_string(value)
                .ok_or(b"package.edition must be a quoted string".as_slice())?
                .to_vec();
        } else if section == b"dependencies" || section.starts_with(b"dependencies.") {
            dependency_entries = true;
        }
    }

    let name = name.ok_or(b"Cargo.toml is missing package.name".as_slice())?;
    if !valid_package_name(&name) {
        return Err(b"package.name contains unsupported characters");
    }
    if !matches!(edition.as_slice(), b"2015" | b"2018" | b"2021" | b"2024") {
        return Err(b"package.edition must be 2015, 2018, 2021, or 2024");
    }
    if dependency_entries {
        return Err(b"dependencies are not supported by the local Cargo bootstrap yet");
    }

    Ok(Package {
        crate_name: crate_name(&name),
        name,
        edition,
        manifest_dir: dirname(path),
    })
}

fn status_code(status: i32) -> i32 {
    if status & 0xff == 0 {
        (status >> 8) & 0xff
    } else {
        1
    }
}

fn spawn(path: &[u8], arg0: &[u8], args: &[&[u8]]) -> i32 {
    let pid = sys::fork();
    if pid < 0 {
        let _ = write_all(2, b"cargo: fork failed\n");
        return 1;
    }
    if pid == 0 {
        let path_c = cstr(path);
        let mut argv_storage = Vec::with_capacity(args.len() + 1);
        argv_storage.push(cstr(arg0));
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
        let _ = write_all(2, b"cargo: exec failed\n");
        sys::exit(127);
    }

    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) < 0 {
        let _ = write_all(2, b"cargo: wait failed\n");
        return 1;
    }
    status_code(status)
}

fn print_error(message: &[u8]) -> i32 {
    let _ = write_all(2, b"error: ");
    let _ = write_all(2, message);
    let _ = write_all(2, b"\n");
    1
}

fn build_package(
    package: &Package,
    release: bool,
    check: bool,
    quiet: bool,
) -> Result<Vec<u8>, i32> {
    let source = join(&package.manifest_dir, b"src/main.rs");
    if !path_exists(&source) {
        return Err(print_error(b"src/main.rs does not exist"));
    }
    let source_bytes = read_file(&source).ok_or_else(|| print_error(b"cannot read src/main.rs"))?;
    let uses_ristux_panic = source_bytes
        .windows(b"extern crate ristux_panic".len())
        .any(|window| window == b"extern crate ristux_panic");
    if uses_ristux_panic && !path_exists(PANIC_RUNTIME) {
        return Err(print_error(b"Ristux panic runtime is not installed"));
    }

    let profile: &[u8] = if release { b"release" } else { b"debug" };
    let target_dir = join(&package.manifest_dir, b"target");
    let profile_dir = join(&target_dir, profile);
    if !ensure_dir(&profile_dir) {
        return Err(print_error(b"cannot create target directory"));
    }

    let output = if check {
        let deps = join(&profile_dir, b"deps");
        if !ensure_dir(&deps) {
            return Err(print_error(b"cannot create target dependency directory"));
        }
        let mut metadata = package.crate_name.clone();
        metadata.extend_from_slice(b".rmeta");
        join(&deps, &metadata)
    } else {
        join(&profile_dir, &package.name)
    };

    if !quiet {
        let _ = write_all(1, b"   Compiling ");
        let _ = write_all(1, &package.name);
        let _ = write_all(1, b" v0.1.0\n");
    }

    let mut args: Vec<&[u8]> = vec![
        b"--crate-name",
        &package.crate_name,
        b"--edition",
        &package.edition,
        b"--target",
        TARGET,
        b"--sysroot",
        SYSROOT,
    ];
    if check {
        args.extend_from_slice(&[b"--emit", b"metadata"]);
    } else if release {
        args.extend_from_slice(&[b"-C", b"opt-level=3"]);
    }
    if uses_ristux_panic {
        args.extend_from_slice(&[
            b"--extern",
            b"ristux_panic=/usr/lib/rustlib/x86_64-unknown-ristux/lib/libristux_panic.rlib",
        ]);
    }
    args.extend_from_slice(&[source.as_slice(), b"-o", output.as_slice()]);

    let status = spawn(b"/bin/rustc", b"rustc", &args);
    if status != 0 {
        return Err(status);
    }
    if !quiet {
        let _ = write_all(1, b"    Finished ");
        let _ = write_all(1, profile);
        let _ = write_all(1, b" profile\n");
    }
    Ok(output)
}

fn create_project(
    path: &[u8],
    requested_name: Option<&[u8]>,
    no_std: bool,
    allow_existing: bool,
) -> i32 {
    let name_path = if path == b"." {
        current_dir().unwrap_or_else(|| b".".to_vec())
    } else {
        path.to_vec()
    };
    let default_name = name_path
        .strip_suffix(b"/")
        .unwrap_or(&name_path)
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or(&name_path);
    let name = requested_name.unwrap_or(default_name);
    if !valid_package_name(name) {
        return print_error(b"invalid package name");
    }
    if path_exists(path) && (!allow_existing || !is_dir(path)) {
        return print_error(b"destination already exists");
    }
    if allow_existing && path_exists(&join(path, b"Cargo.toml")) {
        return print_error(b"Cargo.toml already exists");
    }
    let source_dir = join(path, b"src");
    if !ensure_dir(&source_dir) {
        return print_error(b"cannot create project directory");
    }

    let mut manifest = Vec::new();
    manifest.extend_from_slice(b"[package]\nname = \"");
    manifest.extend_from_slice(name);
    manifest.extend_from_slice(b"\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n");
    if !write_file(&join(path, b"Cargo.toml"), &manifest) {
        return print_error(b"cannot write Cargo.toml");
    }

    let source = if no_std {
        b"#![no_std]\n#![no_main]\n\nextern crate ristux_panic;\n\n#[unsafe(no_mangle)]\npub extern \"C\" fn main() -> i32 { 0 }\n".as_slice()
    } else {
        b"fn main() {\n    println!(\"Hello, world!\");\n}\n".as_slice()
    };
    if !write_file(&join(&source_dir, b"main.rs"), source) {
        return print_error(b"cannot write src/main.rs");
    }
    let _ = write_all(1, b"     Created binary package `");
    let _ = write_all(1, name);
    let _ = write_all(1, b"`\n");
    0
}

fn usage() {
    let _ = write_all(
        1,
        b"usage: cargo [--version] <new|init|build|check|run> [OPTIONS]\n",
    );
    let _ = write_all(
        1,
        b"local packages without dependencies are supported; registry and Git dependencies are pending\n",
    );
}

fn main(args: &[&[u8]]) -> i32 {
    if args.iter().any(|arg| *arg == b"--version" || *arg == b"-V") {
        let _ = write_all(1, b"cargo 1.96.0 (ristux native-local)\n");
        return 0;
    }
    if args.len() < 2 || args.iter().any(|arg| *arg == b"--help" || *arg == b"-h") {
        usage();
        return i32::from(args.len() < 2);
    }

    let command = args[1];
    if command == b"new" || command == b"init" {
        let mut path = if command == b"init" {
            b".".as_slice()
        } else {
            b"".as_slice()
        };
        let mut name = None;
        let mut no_std = true;
        let mut index = 2usize;
        while let Some(arg) = args.get(index) {
            if *arg == b"--name" {
                let Some(value) = args.get(index + 1) else {
                    return print_error(b"--name requires a value");
                };
                name = Some(*value);
                index += 2;
            } else if *arg == b"--no-std" {
                no_std = true;
                index += 1;
            } else if *arg == b"--std" {
                no_std = false;
                index += 1;
            } else if arg.starts_with(b"-") {
                return print_error(b"unsupported project creation option");
            } else if command == b"new" && path.is_empty() {
                path = arg;
                index += 1;
            } else {
                return print_error(b"unexpected project creation argument");
            }
        }
        if path.is_empty() {
            return print_error(b"cargo new requires a path");
        }
        return create_project(path, name, no_std, command == b"init");
    }

    if !matches!(command, b"build" | b"check" | b"run") {
        return print_error(b"unsupported command");
    }

    let mut manifest = b"Cargo.toml".to_vec();
    let mut release = false;
    let mut quiet = false;
    let mut run_args: Vec<&[u8]> = Vec::new();
    let mut index = 2usize;
    while let Some(arg) = args.get(index) {
        if *arg == b"--manifest-path" {
            let Some(value) = args.get(index + 1) else {
                return print_error(b"--manifest-path requires a value");
            };
            manifest = value.to_vec();
            index += 2;
        } else if *arg == b"--release" {
            release = true;
            index += 1;
        } else if *arg == b"--quiet" || *arg == b"-q" {
            quiet = true;
            index += 1;
        } else if *arg == b"--target" {
            let Some(value) = args.get(index + 1) else {
                return print_error(b"--target requires a value");
            };
            if *value != TARGET {
                return print_error(b"only x86_64-unknown-ristux is supported");
            }
            index += 2;
        } else if *arg == b"--" && command == b"run" {
            run_args.extend_from_slice(&args[index + 1..]);
            break;
        } else {
            return print_error(b"unsupported build option");
        }
    }

    let package = match parse_manifest(&manifest) {
        Ok(package) => package,
        Err(message) => return print_error(message),
    };
    let output = match build_package(&package, release, command == b"check", quiet) {
        Ok(output) => output,
        Err(status) => return status,
    };
    if command != b"run" {
        return 0;
    }
    if !quiet {
        let _ = write_all(1, b"     Running `");
        let _ = write_all(1, &output);
        let _ = write_all(1, b"`\n");
    }
    spawn(&output, &package.name, &run_args)
}

ristux_userland::program_main!(main);
