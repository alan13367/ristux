#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::ptr;
use ristux_userland::sys;

const TCGETS: usize = 0x5401;
const TCSETS: usize = 0x5402;
const TERMIOS_SIZE: usize = 60;
const TERMIOS_LFLAG: usize = 12;
const ECHO: u32 = 0x0008;

struct Account {
    name: Vec<u8>,
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

fn env_cstr(name: &[u8], value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len() + value.len() + 2);
    out.extend_from_slice(name);
    out.push(b'=');
    out.extend_from_slice(value);
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

struct EchoGuard {
    original: [u8; TERMIOS_SIZE],
    active: bool,
}

impl EchoGuard {
    fn disable() -> Self {
        let mut original = [0u8; TERMIOS_SIZE];
        if sys::ioctl(0, TCGETS, original.as_mut_ptr() as usize) < 0 {
            return Self {
                original,
                active: false,
            };
        }
        let mut raw = original;
        let lflag = read_u32(&raw, TERMIOS_LFLAG) & !ECHO;
        raw[TERMIOS_LFLAG..TERMIOS_LFLAG + 4].copy_from_slice(&lflag.to_le_bytes());
        if sys::ioctl(0, TCSETS, raw.as_ptr() as usize) < 0 {
            return Self {
                original,
                active: false,
            };
        }
        Self {
            original,
            active: true,
        }
    }
}

impl Drop for EchoGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = sys::ioctl(0, TCSETS, self.original.as_ptr() as usize);
        }
    }
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
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
            name: fields[0].to_vec(),
            uid: parse_u32(fields[2])?,
            gid: parse_u32(fields[3])?,
            home: fields[5].to_vec(),
            shell: fields[6].to_vec(),
        });
    }
    None
}

fn find_shadow_hash(shadow: &[u8], name: &[u8]) -> Option<Vec<u8>> {
    for line in shadow.split(|byte| *byte == b'\n') {
        let fields: Vec<&[u8]> = line.split(|byte| *byte == b':').collect();
        if fields.len() >= 2 && fields[0] == name {
            return Some(fields[1].to_vec());
        }
    }
    None
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_hex(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0usize;
    while index < bytes.len() {
        let high = hex_nibble(bytes[index])?;
        let low = hex_nibble(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Some(out)
}

fn parse_hex_u64(bytes: &[u8]) -> Option<u64> {
    let mut value = 0u64;
    if bytes.is_empty() {
        return None;
    }
    for &byte in bytes {
        value = value.checked_mul(16)?;
        value = value.checked_add(hex_nibble(byte)? as u64)?;
    }
    Some(value)
}

fn password_hash(salt: &[u8], password: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in salt {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash ^= b':' as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    for &byte in password {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn verify_password(encoded: &[u8], password: &[u8]) -> bool {
    if encoded.is_empty() {
        return true;
    }
    let fields: Vec<&[u8]> = encoded.split(|byte| *byte == b'$').collect();
    if fields.len() != 3 || fields[0] != b"ristux1" {
        return false;
    }
    let Some(salt) = decode_hex(fields[1]) else {
        return false;
    };
    let Some(expected) = parse_hex_u64(fields[2]) else {
        return false;
    };
    password_hash(&salt, password) == expected
}

fn authenticate(account: &Account) -> bool {
    let shadow = read_file(b"/etc/shadow").unwrap_or_default();
    let hash = find_shadow_hash(&shadow, &account.name).unwrap_or_default();
    if hash.is_empty() {
        return true;
    }
    let _ = sys::write(1, b"Password: ");
    let password = {
        let _guard = EchoGuard::disable();
        read_line().unwrap_or_default()
    };
    let _ = sys::write(1, b"\n");
    verify_password(&hash, &password)
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
    let argv0 = cstr(b"-sh");
    let argv: [*const u8; 2] = [argv0.as_ptr(), ptr::null()];
    let env = [
        env_cstr(b"USER", &account.name),
        env_cstr(b"LOGNAME", &account.name),
        env_cstr(b"HOME", &account.home),
        env_cstr(b"SHELL", &account.shell),
        env_cstr(b"PATH", b"/bin"),
    ];
    let mut envp: Vec<*const u8> = env.iter().map(|entry| entry.as_ptr()).collect();
    envp.push(ptr::null());
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
            if !authenticate(&account) {
                let _ = sys::write(1, b"login: authentication failed\n");
                continue;
            }
            exec_shell(&account);
        }
        let _ = sys::write(1, b"login: unknown user\n");
    }
}

ristux_userland::program_main!(main);
