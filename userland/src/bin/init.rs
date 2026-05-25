#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec;
use core::ptr;
use ristux_userland::sys;

fn main(_args: &[&[u8]]) -> i32 {
    let _ = sys::write(1, b"init: spawning /bin/sh\n");

    loop {
        let pid = sys::fork();
        if pid < 0 {
            let _ = sys::write(2, b"init: fork failed\n");
            sys::exit(1);
        }

        if pid == 0 {
            let path = b"/bin/sh\0";
            let argv: [*const u8; 2] = [path.as_ptr(), ptr::null()];
            let envp: [*const u8; 1] = [ptr::null()];
            let _ = sys::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
            let _ = sys::write(2, b"init: execve /bin/sh failed\n");
            sys::exit(127);
        }

        let mut status: i32 = 0;
        loop {
            let waited = sys::wait4(-1, &mut status as *mut i32, 0, 0);
            if waited < 0 {
                break;
            }
            if waited as isize == pid as isize {
                break;
            }
        }

        let _ = sys::write(1, b"init: /bin/sh exited; respawning\n");
        let _ = sys::write(1, b"");
        let _ = vec![0u8; 0];
    }
}

ristux_userland::program_main!(main);
