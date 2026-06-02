#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec;
use core::ptr;
use ristux_userland::{installer_support as inst, sys};

fn spawn_dropbear() -> isize {
    let pid = sys::fork();
    if pid < 0 {
        let _ = sys::write(2, b"init: dropbear fork failed\n");
        return -1;
    }

    if pid == 0 {
        let path = b"/bin/dropbear\0";
        let argv0 = b"dropbear\0";
        let arg_f = b"-F\0";
        let arg_e = b"-E\0";
        let arg_r = b"-R\0";
        let arg_b = b"-B\0";
        let arg_p = b"-p\0";
        let bind = b"0.0.0.0:2222\0";
        let argv: [*const u8; 8] = [
            argv0.as_ptr(),
            arg_f.as_ptr(),
            arg_e.as_ptr(),
            arg_r.as_ptr(),
            arg_b.as_ptr(),
            arg_p.as_ptr(),
            bind.as_ptr(),
            ptr::null(),
        ];
        let envp: [*const u8; 1] = [ptr::null()];
        let _ = sys::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
        let _ = sys::write(2, b"init: execve /bin/dropbear failed\n");
        sys::exit(127);
    }

    let _ = sys::write(1, b"init: started dropbear on 0.0.0.0:2222\n");
    pid
}

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

    let mut dropbear_pid = spawn_dropbear();
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
            if waited == dropbear_pid {
                let _ = sys::write(1, b"init: dropbear exited; restarting\n");
                dropbear_pid = spawn_dropbear();
                continue;
            }
            if waited as isize == pid as isize {
                break;
            }
        }

        let _ = sys::write(1, b"init: /bin/login exited; respawning\n");
        let _ = sys::write(1, b"");
        let _ = vec![0u8; 0];
    }
}

ristux_userland::program_main!(main);
