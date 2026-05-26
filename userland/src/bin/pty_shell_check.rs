#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDWR: i32 = 2;
const WNOHANG: i32 = 1;
const SIGTERM: u8 = 15;
const SIGKILL: u8 = 9;
const TIOCGPTN: usize = 0x8004_5430;
const TIOCSPTLCK: usize = 0x4004_5431;

fn pts_path(number: u32) -> Vec<u8> {
    let mut path = Vec::new();
    path.extend_from_slice(b"/dev/pts/");
    let mut digits = [0u8; 10];
    let mut count = 0usize;
    let mut n = number;
    loop {
        digits[count] = b'0' + (n % 10) as u8;
        count += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    while count > 0 {
        count -= 1;
        path.push(digits[count]);
    }
    path.push(0);
    path
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn cleanup_child(pid: isize) {
    let _ = sys::kill(pid, SIGTERM);
    let mut status = 0i32;
    for _ in 0..20 {
        if sys::wait4(pid, &mut status as *mut i32, WNOHANG, 0) != 0 {
            return;
        }
        let _ = sys::sched_yield();
    }
    let _ = sys::kill(pid, SIGKILL);
    let _ = sys::wait4(pid, &mut status as *mut i32, 0, 0);
}

fn drive_shell(master: i32, child: isize) -> i32 {
    let command = b"echo pty_shell_check: child shell ok\n";
    if sys::write(master, command) != command.len() as isize {
        let _ = sys::write(2, b"pty_shell_check: write failed\n");
        cleanup_child(child);
        return 1;
    }

    let mut output = Vec::new();
    let mut buf = [0u8; 128];
    for _ in 0..200 {
        let mut pollfd = sys::PollFd {
            fd: master,
            events: sys::POLLIN,
            revents: 0,
        };
        let ready = sys::poll(&mut pollfd as *mut sys::PollFd, 1, 50);
        if ready > 0 && pollfd.revents & sys::POLLIN != 0 {
            let n = sys::read(master, &mut buf);
            if n > 0 {
                output.extend_from_slice(&buf[..n as usize]);
                if contains(&output, b"pty_shell_check: child shell ok") {
                    let _ = sys::write(master, b"exit\n");
                    let _ = sys::write(1, b"pty_shell_check: shell output ok\n");
                    let mut status = 0i32;
                    for _ in 0..20 {
                        if sys::wait4(child, &mut status as *mut i32, WNOHANG, 0) == child {
                            return 0;
                        }
                        let _ = sys::sched_yield();
                    }
                    cleanup_child(child);
                    return 0;
                }
            }
        }
        let mut status = 0i32;
        if sys::wait4(child, &mut status as *mut i32, WNOHANG, 0) == child {
            break;
        }
    }

    let _ = sys::write(2, b"pty_shell_check: shell output missing\n");
    cleanup_child(child);
    1
}

fn main(_args: &[&[u8]]) -> i32 {
    let master = sys::open(b"/dev/ptmx\0".as_ptr(), O_RDWR, 0);
    if master < 0 {
        let _ = sys::write(2, b"pty_shell_check: ptmx open failed\n");
        return 1;
    }

    let mut number = 0u32;
    if sys::ioctl(master as i32, TIOCGPTN, &mut number as *mut u32 as usize) < 0 {
        let _ = sys::write(2, b"pty_shell_check: pty number failed\n");
        let _ = sys::close(master as i32);
        return 1;
    }
    let mut unlock = 0u32;
    if sys::ioctl(master as i32, TIOCSPTLCK, &mut unlock as *mut u32 as usize) < 0 {
        let _ = sys::write(2, b"pty_shell_check: unlock failed\n");
        let _ = sys::close(master as i32);
        return 1;
    }

    let slave_path = pts_path(number);
    let slave = sys::open(slave_path.as_ptr(), O_RDWR, 0);
    if slave < 0 {
        let _ = sys::write(2, b"pty_shell_check: slave open failed\n");
        let _ = sys::close(master as i32);
        return 1;
    }

    let child = sys::fork();
    if child < 0 {
        let _ = sys::write(2, b"pty_shell_check: fork failed\n");
        let _ = sys::close(slave as i32);
        let _ = sys::close(master as i32);
        return 1;
    }

    if child == 0 {
        let _ = sys::setsid();
        let _ = sys::dup2(slave as i32, 0);
        let _ = sys::dup2(slave as i32, 1);
        let _ = sys::dup2(slave as i32, 2);
        let _ = sys::close(master as i32);
        let _ = sys::close(slave as i32);
        let argv = [b"sh\0".as_ptr(), core::ptr::null()];
        let envp = [core::ptr::null()];
        let _ = sys::execve(b"/bin/sh\0".as_ptr(), argv.as_ptr(), envp.as_ptr());
        let _ = sys::write(2, b"pty_shell_check: exec failed\n");
        sys::exit(127);
    }

    let _ = sys::close(slave as i32);
    let status = drive_shell(master as i32, child);
    let _ = sys::close(master as i32);
    if status == 0 {
        let _ = sys::write(1, b"pty_shell_check: done\n");
    }
    status
}

ristux_userland::program_main!(main);
