use alloc::vec::Vec;
use core::str;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::{process::Pid, signal::Signal, sync::spinlock::SpinLock};

static TTY: SpinLock<Option<Tty>> = SpinLock::new(None);
static LEFT_SHIFT: AtomicBool = AtomicBool::new(false);
static RIGHT_SHIFT: AtomicBool = AtomicBool::new(false);
static CTRL: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyMode {
    Canonical,
    Raw,
}

pub struct Tty {
    mode: TtyMode,
    line: Vec<u8>,
    /// Lines committed by a newline that are ready to be consumed by readers.
    ready: Vec<Vec<u8>>,
    eof: bool,
    /// Processes parked inside `read()` waiting for input.
    waiters: Vec<Pid>,
    foreground_pgrp: Pid,
}

impl Tty {
    pub fn new() -> Self {
        Self {
            mode: TtyMode::Canonical,
            line: Vec::new(),
            ready: Vec::new(),
            eof: false,
            waiters: Vec::new(),
            foreground_pgrp: 1,
        }
    }

    pub fn set_mode(&mut self, mode: TtyMode) {
        self.mode = mode;
    }

    pub fn input(&mut self, byte: u8) -> Option<Signal> {
        match byte {
            0x03 => Some(Signal::Int),
            0x1a => Some(Signal::Tstp),
            0x04 => {
                self.eof = true;
                None
            }
            0x08 | 0x7f if self.mode == TtyMode::Canonical => {
                self.line.pop();
                None
            }
            b'\n' if self.mode == TtyMode::Canonical => {
                let mut line = core::mem::take(&mut self.line);
                line.push(b'\n');
                self.ready.push(line);
                None
            }
            byte => {
                self.line.push(byte);
                None
            }
        }
    }

    fn pending_line(&self) -> Option<&[u8]> {
        self.ready.last().map(|v| v.as_slice())
    }

    pub fn read_line(&mut self) -> Option<Vec<u8>> {
        if let Some(line) = self.ready.first().cloned() {
            self.ready.remove(0);
            return Some(line);
        }
        if self.eof {
            self.eof = false;
            return Some(Vec::new());
        }
        // Legacy single-line buffer fallback (canonical text without newline).
        if let Some(newline) = self.line.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.line.drain(..=newline).collect();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            return Some(line);
        }
        None
    }

    fn park(&mut self, pid: Pid) {
        if !self.waiters.contains(&pid) {
            self.waiters.push(pid);
        }
    }

    fn drain_waiters(&mut self) -> Vec<Pid> {
        core::mem::take(&mut self.waiters)
    }

    fn has_data(&self) -> bool {
        !self.ready.is_empty() || self.eof
    }

    fn foreground_pgrp(&self) -> Pid {
        self.foreground_pgrp
    }

    fn set_foreground_pgrp(&mut self, pgrp: Pid) {
        self.foreground_pgrp = pgrp;
    }
}

pub fn init() {
    *TTY.lock() = Some(Tty::new());
    self_test();
}

pub fn input_scancode(scancode: u8) {
    let Some(byte) = translate_set1(scancode) else {
        return;
    };

    let (waiters, signal_target) = {
        let mut guard = TTY.lock();
        let tty = guard.as_mut().expect("TTY used before initialization");
        if let Some(signal) = tty.input(byte) {
            let target = tty.foreground_pgrp();
            (Vec::new(), Some((target, signal)))
        } else {
            if byte == b'\n' {
                let line = tty.pending_line().unwrap_or(&[]);
                let trimmed = line.strip_suffix(b"\n").unwrap_or(line);
                match str::from_utf8(trimmed) {
                    Ok(text) => crate::println!("TTY canonical line ready: {}", text),
                    Err(_) => crate::println!("TTY canonical line ready: <binary>"),
                }
            }

            let waiters = if tty.has_data() {
                tty.drain_waiters()
            } else {
                Vec::new()
            };
            (waiters, None)
        }
    };

    if let Some((pgrp, signal)) = signal_target {
        crate::println!(
            "TTY delivered signal {} to foreground pgrp {}.",
            signal.number(),
            pgrp
        );
        crate::process::signal_pgrp(pgrp, signal.default_status());
        return;
    }

    for pid in waiters {
        crate::process::wake_io_waiters_for(pid);
    }
}

/// Compatibility shim for callers that read bytes synchronously. Returns 0 when
/// no full line is ready (callers should fall back to parking via
/// [`park_current`] + the syscall yield mechanism).
pub fn read(output: &mut [u8]) -> usize {
    let line = {
        let mut guard = TTY.lock();
        let tty = guard.as_mut().expect("TTY used before initialization");
        tty.read_line()
    };
    let Some(line) = line else {
        return 0;
    };
    let count = line.len().min(output.len());
    output[..count].copy_from_slice(&line[..count]);
    count
}

/// Return the next ready canonical line if available (newline already stripped).
pub fn try_read_line() -> Option<Vec<u8>> {
    let mut guard = TTY.lock();
    let tty = guard.as_mut().expect("TTY used before initialization");
    let mut line = tty.read_line()?;
    // Append a trailing newline for legacy callers that supplied canonical
    // text without one; keyboard-committed lines already include it.
    if !line.ends_with(b"\n") {
        line.push(b'\n');
    }
    Some(line)
}

/// Park the current process on the TTY wait-queue.
pub fn park_current() {
    let Some(pid) = crate::process::current_pid() else {
        return;
    };
    {
        let mut guard = TTY.lock();
        let tty = guard.as_mut().expect("TTY used before initialization");
        tty.park(pid);
    }
    crate::process::block_current(crate::process::BlockReason::WaitIo);
}

pub fn foreground_pgrp() -> Pid {
    let guard = TTY.lock();
    guard
        .as_ref()
        .map(|tty| tty.foreground_pgrp())
        .unwrap_or(1)
}

pub fn set_foreground_pgrp(pgrp: Pid) {
    let mut guard = TTY.lock();
    let tty = guard.as_mut().expect("TTY used before initialization");
    tty.set_foreground_pgrp(pgrp);
}

fn translate_set1(scancode: u8) -> Option<u8> {
    match scancode {
        0x2a => {
            LEFT_SHIFT.store(true, Ordering::Relaxed);
            return None;
        }
        0x36 => {
            RIGHT_SHIFT.store(true, Ordering::Relaxed);
            return None;
        }
        0x1d => {
            CTRL.store(true, Ordering::Relaxed);
            return None;
        }
        0xaa => {
            LEFT_SHIFT.store(false, Ordering::Relaxed);
            return None;
        }
        0xb6 => {
            RIGHT_SHIFT.store(false, Ordering::Relaxed);
            return None;
        }
        0x9d => {
            CTRL.store(false, Ordering::Relaxed);
            return None;
        }
        _ => {}
    }

    if scancode & 0x80 != 0 {
        return None;
    }

    let shifted = LEFT_SHIFT.load(Ordering::Relaxed) || RIGHT_SHIFT.load(Ordering::Relaxed);
    let byte = match scancode {
        0x02 => Some(if shifted { b'!' } else { b'1' }),
        0x03 => Some(if shifted { b'@' } else { b'2' }),
        0x04 => Some(if shifted { b'#' } else { b'3' }),
        0x05 => Some(if shifted { b'$' } else { b'4' }),
        0x06 => Some(if shifted { b'%' } else { b'5' }),
        0x07 => Some(if shifted { b'^' } else { b'6' }),
        0x08 => Some(if shifted { b'&' } else { b'7' }),
        0x09 => Some(if shifted { b'*' } else { b'8' }),
        0x0a => Some(if shifted { b'(' } else { b'9' }),
        0x0b => Some(if shifted { b')' } else { b'0' }),
        0x0c => Some(if shifted { b'_' } else { b'-' }),
        0x0d => Some(if shifted { b'+' } else { b'=' }),
        0x0e => Some(0x08),
        0x0f => Some(b'\t'),
        0x10 => Some(if shifted { b'Q' } else { b'q' }),
        0x11 => Some(if shifted { b'W' } else { b'w' }),
        0x12 => Some(if shifted { b'E' } else { b'e' }),
        0x13 => Some(if shifted { b'R' } else { b'r' }),
        0x14 => Some(if shifted { b'T' } else { b't' }),
        0x15 => Some(if shifted { b'Y' } else { b'y' }),
        0x16 => Some(if shifted { b'U' } else { b'u' }),
        0x17 => Some(if shifted { b'I' } else { b'i' }),
        0x18 => Some(if shifted { b'O' } else { b'o' }),
        0x19 => Some(if shifted { b'P' } else { b'p' }),
        0x1a => Some(if shifted { b'{' } else { b'[' }),
        0x1b => Some(if shifted { b'}' } else { b']' }),
        0x1c => Some(b'\n'),
        0x1e => Some(if shifted { b'A' } else { b'a' }),
        0x1f => Some(if shifted { b'S' } else { b's' }),
        0x20 => Some(if shifted { b'D' } else { b'd' }),
        0x21 => Some(if shifted { b'F' } else { b'f' }),
        0x22 => Some(if shifted { b'G' } else { b'g' }),
        0x23 => Some(if shifted { b'H' } else { b'h' }),
        0x24 => Some(if shifted { b'J' } else { b'j' }),
        0x25 => Some(if shifted { b'K' } else { b'k' }),
        0x26 => Some(if shifted { b'L' } else { b'l' }),
        0x27 => Some(if shifted { b':' } else { b';' }),
        0x28 => Some(if shifted { b'"' } else { b'\'' }),
        0x29 => Some(if shifted { b'~' } else { b'`' }),
        0x2b => Some(if shifted { b'|' } else { b'\\' }),
        0x2c => Some(if shifted { b'Z' } else { b'z' }),
        0x2d => Some(if shifted { b'X' } else { b'x' }),
        0x2e => Some(if shifted { b'C' } else { b'c' }),
        0x2f => Some(if shifted { b'V' } else { b'v' }),
        0x30 => Some(if shifted { b'B' } else { b'b' }),
        0x31 => Some(if shifted { b'N' } else { b'n' }),
        0x32 => Some(if shifted { b'M' } else { b'm' }),
        0x33 => Some(if shifted { b'<' } else { b',' }),
        0x34 => Some(if shifted { b'>' } else { b'.' }),
        0x35 => Some(if shifted { b'?' } else { b'/' }),
        0x39 => Some(b' '),
        _ => None,
    }?;
    if CTRL.load(Ordering::Relaxed) && byte.is_ascii_alphabetic() {
        return Some(byte.to_ascii_lowercase() & 0x1f);
    }
    Some(byte)
}

fn self_test() {
    let mut tty = Tty::new();
    tty.input(b'a');
    tty.input(b'b');
    tty.input(0x08);
    tty.input(b'c');
    tty.input(b'\n');
    let line = tty.read_line().expect("tty canonical read failed");
    if line != b"ac\n" && line != b"ac" {
        panic!("tty backspace self-test failed");
    }
    if tty.input(0x03) != Some(Signal::Int) {
        panic!("tty ctrl-c self-test failed");
    }
    if translate_set1(0x1e) != Some(b'a') || translate_set1(0x9e).is_some() {
        panic!("tty scancode translation self-test failed");
    }
    translate_set1(0x2a);
    if translate_set1(0x2b) != Some(b'|') {
        panic!("tty shifted punctuation self-test failed");
    }
    translate_set1(0xaa);
    tty.set_mode(TtyMode::Raw);
    tty.input(0x7f);
    tty.input(0x04);
    if tty.read_line() != Some(Vec::new()) {
        panic!("tty ctrl-d self-test failed");
    }
    crate::println!("TTY self-test passed.");
}
