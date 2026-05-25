#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use core::ptr;
use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    let groups = [0u32];
    let _ = sys::setgroups(&groups);
    if sys::setgid(0) < 0 || sys::setuid(0) < 0 {
        let _ = sys::write(2, b"su: EACCES\n");
        return 1;
    }
    let root = b"/root\0";
    let _ = sys::chdir(root.as_ptr());

    let shell = b"/bin/sh\0";
    let argv: [*const u8; 2] = [shell.as_ptr(), ptr::null()];
    let envp: [*const u8; 1] = [ptr::null()];
    let _ = sys::execve(shell.as_ptr(), argv.as_ptr(), envp.as_ptr());
    let _ = sys::write(2, b"su: exec failed\n");
    127
}

ristux_userland::program_main!(main);
