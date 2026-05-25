#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::ptr;
use ristux_userland::sys;

struct Account {
    uid: u32,
    gid: u32,
    home: Vec<u8>,
    shell: Vec<u8>,
}

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn read_line() -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        let n = sys::read(0, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 {
            return Some(line);
        }
        for &byte in &buf[..n as usize] {
            if byte == b'\n' || byte == b'\r' {
                return Some(line);
            }
            line.push(byte);
        }
    }
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), 0, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 256];
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

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    if bytes.is_empty() {
        return None;
    }
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

fn find_account(passwd: &[u8], name: &[u8]) -> Option<Account> {
    for line in passwd.split(|byte| *byte == b'\n') {
        let fields: Vec<&[u8]> = line.split(|byte| *byte == b':').collect();
        if fields.len() < 7 || fields[0] != name {
            continue;
        }
        return Some(Account {
            uid: parse_u32(fields[2])?,
            gid: parse_u32(fields[3])?,
            home: fields[5].to_vec(),
            shell: fields[6].to_vec(),
        });
    }
    None
}

fn exec_shell(account: &Account) -> ! {
    let groups = [account.gid];
    let _ = sys::setgroups(&groups);
    if sys::setgid(account.gid) < 0 || sys::setuid(account.uid) < 0 {
        let _ = sys::write(2, b"login: cannot set credentials\n");
        sys::exit(1);
    }

    let home = cstr(&account.home);
    let _ = sys::chdir(home.as_ptr());

    let shell = cstr(&account.shell);
    let argv: [*const u8; 2] = [shell.as_ptr(), ptr::null()];
    let envp: [*const u8; 1] = [ptr::null()];
    let _ = sys::execve(shell.as_ptr(), argv.as_ptr(), envp.as_ptr());
    let _ = sys::write(2, b"login: exec shell failed\n");
    sys::exit(127);
}

fn main(_args: &[&[u8]]) -> i32 {
    let passwd = match read_file(b"/etc/passwd") {
        Some(data) => data,
        None => {
            let _ = sys::write(2, b"login: cannot read /etc/passwd\n");
            return 1;
        }
    };

    loop {
        let _ = sys::write(1, b"login: ");
        let Some(name) = read_line() else {
            return 1;
        };
        if name.is_empty() {
            continue;
        }
        if let Some(account) = find_account(&passwd, &name) {
            exec_shell(&account);
        }
        let _ = sys::write(1, b"login: unknown user\n");
    }
}

ristux_userland::program_main!(main);
