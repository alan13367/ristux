//! Higher-level I/O helpers built on top of `sys::*`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::sys;

/// Write a byte slice in full, retrying on short writes.
pub fn write_all(fd: i32, mut buf: &[u8]) -> isize {
    while !buf.is_empty() {
        let n = sys::write(fd, buf);
        if n <= 0 {
            return n;
        }
        buf = &buf[n as usize..];
    }
    0
}

/// Write a string slice with a trailing newline.
pub fn writeln(fd: i32, msg: &str) {
    let _ = write_all(fd, msg.as_bytes());
    let _ = write_all(fd, b"\n");
}

/// Write a string slice.
pub fn write_str(fd: i32, msg: &str) {
    let _ = write_all(fd, msg.as_bytes());
}

/// Read one line (terminated by `\n`) from `fd` into a `String`.
/// Returns Ok(None) on EOF, Ok(Some(line)) otherwise (newline stripped).
pub fn read_line(fd: i32) -> Result<Option<String>, isize> {
    let mut buf = [0u8; 256];
    let n = sys::read(fd, &mut buf);
    if n < 0 {
        return Err(n);
    }
    if n == 0 {
        return Ok(None);
    }
    let mut bytes: Vec<u8> = Vec::with_capacity(n as usize);
    bytes.extend_from_slice(&buf[..n as usize]);
    while bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    match String::from_utf8(bytes) {
        Ok(s) => Ok(Some(s)),
        Err(e) => Ok(Some(String::from_utf8_lossy(e.as_bytes()).into_owned())),
    }
}

/// Convert a `&str` into a NUL-terminated `Vec<u8>` for syscall use.
pub fn cstring(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s.as_bytes());
    v.push(0);
    v
}
