#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use core::ptr;
use ristux_userland::{installer_support as inst, sys};

fn installer_mode() -> bool {
    let Some(cmdline) = inst::read_file(b"/proc/cmdline") else {
        return false;
    };
    cmdline
        .split(|byte| byte.is_ascii_whitespace())
        .any(|part| part == b"ristux.mode=install")
}

fn run_installer() -> bool {
    let _ = sys::write(1, b"init: installer mode; spawning /bin/ristux-install\n");
    let pid = sys::fork();
    if pid < 0 {
        let _ = sys::write(2, b"init: installer fork failed\n");
        return false;
    }
    if pid == 0 {
        let path = b"/bin/ristux-install\0";
        let argv: [*const u8; 2] = [path.as_ptr(), ptr::null()];
        let envp: [*const u8; 1] = [ptr::null()];
        let _ = sys::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
        let _ = sys::write(2, b"init: execve /bin/ristux-install failed\n");
        sys::exit(127);
    }
    let mut status: i32 = 0;
    let _ = sys::wait4(pid, &mut status as *mut i32, 0, 0);
    let exit_code = (status >> 8) & 0xff;
    if exit_code == 0 {
        let _ = sys::write(1, b"init: installer completed; spawning /bin/login\n");
        true
    } else {
        let _ = sys::write(1, b"init: installer exited without installing\n");
        false
    }
}

fn rescue_shell_loop() -> ! {
    let _ = sys::write(1, b"init: spawning installer rescue shell (/bin/sh)\n");
    loop {
        let pid = sys::fork();
        if pid < 0 {
            let _ = sys::write(2, b"init: rescue shell fork failed\n");
            sys::exit(1);
        }
        if pid == 0 {
            let path = b"/bin/sh\0";
            let argv0 = b"sh\0";
            let argv: [*const u8; 2] = [argv0.as_ptr(), ptr::null()];
            let envp: [*const u8; 1] = [ptr::null()];
            let _ = sys::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
            let _ = sys::write(2, b"init: execve /bin/sh failed\n");
            sys::exit(127);
        }

        let mut status: i32 = 0;
        let _ = sys::wait4(pid, &mut status as *mut i32, 0, 0);
        let _ = sys::write(1, b"init: rescue shell exited; respawning\n");
    }
}

fn main(_args: &[&[u8]]) -> i32 {
    if installer_mode() && !run_installer() {
        rescue_shell_loop();
    }

    let _ = sys::write(1, b"init: spawning /bin/login\n");

    loop {
        let pid = sys::fork();
        if pid < 0 {
            let _ = sys::write(2, b"init: fork failed\n");
            sys::exit(1);
        }

        if pid == 0 {
            let path = b"/bin/login\0";
            let argv: [*const u8; 2] = [path.as_ptr(), ptr::null()];
            let envp: [*const u8; 1] = [ptr::null()];
            let _ = sys::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
            let _ = sys::write(2, b"init: execve /bin/login failed\n");
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

        let _ = sys::write(1, b"init: /bin/login exited; respawning\n");
    }
}

ristux_userland::program_main!(main);
