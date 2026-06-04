#![no_std]
#![no_main]

extern crate ristux_userland;
extern crate alloc;

use ristux_userland::sys;

const VERSION: &[u8] = b"rustc 1.96.0 (ristux official-bootstrap stage0)\n";
const TARGET: &[u8] = b"x86_64-unknown-ristux\n";
const SYSROOT: &[u8] = b"/usr\n";
const TARGET_LIBDIR: &[u8] = b"/usr/lib/rustlib/x86_64-unknown-ristux/lib\n";
const CFG: &[u8] = b"debug_assertions\npanic=\"abort\"\ntarget_arch=\"x86_64\"\ntarget_endian=\"little\"\ntarget_env=\"\"\ntarget_family=\"unix\"\ntarget_has_atomic=\"64\"\ntarget_os=\"ristux\"\ntarget_pointer_width=\"64\"\ntarget_vendor=\"unknown\"\nunix\n";
const TARGET_SPEC_JSON: &[u8] = include_bytes!("../../../targets/x86_64-unknown-ristux.json");

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
}

fn has_arg(args: &[&[u8]], needle: &[u8]) -> bool {
    args.iter().any(|arg| *arg == needle)
}

fn print_value_for(args: &[&[u8]], key: &[u8], value: &[u8]) -> bool {
    if args.windows(2).any(|pair| pair[0] == b"--print" && pair[1] == key) {
        write_all(1, value);
        return true;
    }
    false
}

fn main(args: &[&[u8]]) -> i32 {
    if has_arg(args, b"--version") || has_arg(args, b"-V") {
        write_all(1, VERSION);
        return 0;
    }
    if print_value_for(args, b"target-list", TARGET)
        || print_value_for(args, b"sysroot", SYSROOT)
        || print_value_for(args, b"target-libdir", TARGET_LIBDIR)
        || print_value_for(args, b"cfg", CFG)
    {
        return 0;
    }
    if args
        .windows(2)
        .any(|pair| pair[0] == b"--print" && pair[1] == b"target-spec-json")
    {
        write_all(1, TARGET_SPEC_JSON);
        write_all(1, b"\n");
        return 0;
    }
    if has_arg(args, b"--help") || has_arg(args, b"-h") {
        write_all(1, b"usage: rustc [--version] [--print KIND] INPUT\n");
        write_all(1, b"supported --print KIND values: target-list, sysroot, target-libdir, cfg, target-spec-json\n");
        write_all(1, b"Ristux packages the Rust 1.96.0 official-toolchain contract; native code generation is pending the upstream rustc_driver and Ristux std port.\n");
        return 0;
    }
    write_all(2, b"rustc 1.96.0: native compilation is not available yet on this Ristux image\n");
    write_all(2, b"rustc 1.96.0: pending upstream rustc_driver host port and Ristux std support\n");
    1
}

ristux_userland::program_main!(main);
