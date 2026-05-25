#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

extern "C" fn handle_sigint(signum: usize, frame: usize) -> ! {
    if signum == 2 {
        let _ = sys::write(1, b"sig_demo: handler ran\n");
    }
    let _ = sys::rt_sigreturn(frame);
    loop {
        let _ = sys::sched_yield();
    }
}

fn main(_args: &[&[u8]]) -> i32 {
    if sys::rt_sigaction(2, handle_sigint as usize) < 0 {
        let _ = sys::write(2, b"sig_demo: sigaction failed\n");
        return 1;
    }
    let pid = sys::getpid();
    if pid < 0 || sys::kill(pid, 2) < 0 {
        let _ = sys::write(2, b"sig_demo: kill failed\n");
        return 1;
    }
    let _ = sys::write(1, b"sig_demo: after sigreturn\n");
    0
}

ristux_userland::program_main!(main);
