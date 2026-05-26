#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const WNOHANG: i32 = 1;
const SIGTERM: u8 = 15;
const SIGKILL: u8 = 9;

fn cleanup_child(pid: isize) {
    let _ = sys::kill(pid, SIGTERM);
    let mut status = 0i32;
    for _ in 0..20 {
        let waited = sys::wait4(pid, &mut status as *mut i32, WNOHANG, 0);
        if waited != 0 {
            return;
        }
        let _ = sys::sched_yield();
    }

    let _ = sys::kill(pid, SIGKILL);
    let _ = sys::wait4(pid, &mut status as *mut i32, 0, 0);
}

fn spawn_dropbear() -> isize {
    let pid = sys::fork();
    if pid != 0 {
        return pid;
    }

    let _ = sys::setsid();
    let null_fd = sys::open(b"/dev/null\0".as_ptr(), O_RDONLY, 0);
    if null_fd >= 0 {
        let _ = sys::dup2(null_fd as i32, 0);
        let _ = sys::close(null_fd as i32);
    }

    let argv = [
        b"dropbear\0".as_ptr(),
        b"-F\0".as_ptr(),
        b"-E\0".as_ptr(),
        b"-R\0".as_ptr(),
        b"-p\0".as_ptr(),
        b"127.0.0.1:2222\0".as_ptr(),
        core::ptr::null(),
    ];
    let envp = [core::ptr::null()];
    let _ = sys::execve(b"/bin/dropbear\0".as_ptr(), argv.as_ptr(), envp.as_ptr());
    let _ = sys::write(2, b"ssh_banner: dropbear exec failed\n");
    sys::exit(127);
}

fn read_banner(fd: i32) -> i32 {
    let mut pollfd = sys::PollFd {
        fd,
        events: sys::POLLIN,
        revents: 0,
    };
    let ready = sys::poll(&mut pollfd as *mut sys::PollFd, 1, 10_000);
    if ready <= 0 || pollfd.revents & sys::POLLIN == 0 {
        let _ = sys::write(2, b"ssh_banner: timeout\n");
        return 1;
    }

    let mut buf = [0u8; 128];
    for _ in 0..32 {
        let n = sys::recvfrom(fd, &mut buf, sys::MSG_DONTWAIT);
        if n > 0 {
            let n = n as usize;
            if n >= 4 && &buf[..4] == b"SSH-" {
                let _ = sys::write(1, b"ssh_banner: banner ok\n");
                return 0;
            }
            let _ = sys::write(2, b"ssh_banner: unexpected banner\n");
            return 1;
        }
        let _ = sys::sched_yield();
    }

    let _ = sys::write(2, b"ssh_banner: timeout\n");
    1
}

fn main(_args: &[&[u8]]) -> i32 {
    let addr = sys::SockAddrIn::new([127, 0, 0, 1], 2222);
    let dropbear_pid = spawn_dropbear();
    if dropbear_pid < 0 {
        let _ = sys::write(2, b"ssh_banner: dropbear fork failed\n");
        return 1;
    }

    for _ in 0..400 {
        let fd = sys::socket(sys::AF_INET, sys::SOCK_STREAM, 0);
        if fd < 0 {
            cleanup_child(dropbear_pid);
            let _ = sys::write(2, b"ssh_banner: socket failed\n");
            return 1;
        }
        if sys::connect(fd as i32, &addr) >= 0 {
            let flags = sys::fcntl(fd as i32, sys::F_GETFL, 0);
            if flags < 0
                || sys::fcntl(
                    fd as i32,
                    sys::F_SETFL,
                    flags | sys::O_NONBLOCK as isize,
                ) < 0
            {
                let _ = sys::write(2, b"ssh_banner: fcntl failed\n");
                let _ = sys::close(fd as i32);
                cleanup_child(dropbear_pid);
                return 1;
            }
            let _ = sys::sendto(fd as i32, b"SSH-2.0-ristux-check\r\n", 0);
            let _ = sys::write(1, b"ssh_banner: connected\n");
            let status = read_banner(fd as i32);
            let _ = sys::close(fd as i32);
            cleanup_child(dropbear_pid);
            return status;
        }
        let _ = sys::close(fd as i32);
        let _ = sys::sched_yield();
    }

    cleanup_child(dropbear_pid);
    let _ = sys::write(2, b"ssh_banner: connect failed\n");
    1
}

ristux_userland::program_main!(main);
