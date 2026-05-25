#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::format;
use ristux_userland::sys;

fn user_name(uid: isize) -> &'static str {
    match uid {
        0 => "root",
        1000 => "alice",
        _ => "?",
    }
}

fn group_name(gid: isize) -> &'static str {
    match gid {
        0 => "root",
        1000 => "alice",
        _ => "?",
    }
}

fn main(_args: &[&[u8]]) -> i32 {
    let uid = sys::getuid();
    let euid = sys::geteuid();
    let gid = sys::getgid();
    let mut line = format!(
        "uid={}({}) gid={}({})",
        uid,
        user_name(uid),
        gid,
        group_name(gid)
    );
    if euid != uid {
        line.push_str(" euid=");
        line.push_str(if euid == 0 { "0(root)" } else { "?(?)" });
    }
    line.push('\n');
    let _ = sys::write(1, line.as_bytes());
    0
}

ristux_userland::program_main!(main);
