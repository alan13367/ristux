use alloc::vec::Vec;
use core::str;

use crate::{signal::Signal, sync::spinlock::SpinLock};

static TTY: SpinLock<Option<Tty>> = SpinLock::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyMode {
    Canonical,
    Raw,
}

pub struct Tty {
    mode: TtyMode,
    line: Vec<u8>,
    eof: bool,
}

impl Tty {
    pub fn new() -> Self {
        Self {
            mode: TtyMode::Canonical,
            line: Vec::new(),
            eof: false,
        }
    }

    pub fn set_mode(&mut self, mode: TtyMode) {
        self.mode = mode;
    }

    pub fn input(&mut self, byte: u8) -> Option<Signal> {
        match byte {
            0x03 => Some(Signal::Int),
            0x04 => {
                self.eof = true;
                None
            }
            0x08 | 0x7f if self.mode == TtyMode::Canonical => {
                self.line.pop();
                None
            }
            byte => {
                self.line.push(byte);
                None
            }
        }
    }

    fn pending_line(&self) -> Option<&[u8]> {
        let newline = self.line.iter().position(|byte| *byte == b'\n')?;
        Some(&self.line[..newline])
    }

    pub fn read_line(&mut self) -> Option<Vec<u8>> {
        if self.eof {
            self.eof = false;
            return Some(Vec::new());
        }

        let newline = self.line.iter().position(|byte| *byte == b'\n')?;
        let mut line = self.line.drain(..=newline).collect::<Vec<_>>();
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        Some(line)
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

    let mut guard = TTY.lock();
    let tty = guard.as_mut().expect("TTY used before initialization");
    if let Some(signal) = tty.input(byte) {
        crate::println!(
            "TTY line discipline generated signal {} from keyboard input.",
            signal.number()
        );
        return;
    }

    if byte == b'\n' {
        let line = tty.pending_line().unwrap_or(&[]);
        match str::from_utf8(line) {
            Ok(text) => crate::println!("TTY canonical line ready: {}", text),
            Err(_) => crate::println!("TTY canonical line ready: <binary>"),
        }
    }
}

pub fn read(output: &mut [u8]) -> usize {
    let mut guard = TTY.lock();
    let tty = guard.as_mut().expect("TTY used before initialization");
    let Some(line) = tty.read_line() else {
        return 0;
    };
    let count = line.len().min(output.len());
    output[..count].copy_from_slice(&line[..count]);
    count
}

fn translate_set1(scancode: u8) -> Option<u8> {
    if scancode & 0x80 != 0 {
        return None;
    }

    match scancode {
        0x02 => Some(b'1'),
        0x03 => Some(b'2'),
        0x04 => Some(b'3'),
        0x05 => Some(b'4'),
        0x06 => Some(b'5'),
        0x07 => Some(b'6'),
        0x08 => Some(b'7'),
        0x09 => Some(b'8'),
        0x0a => Some(b'9'),
        0x0b => Some(b'0'),
        0x0e => Some(0x08),
        0x10 => Some(b'q'),
        0x11 => Some(b'w'),
        0x12 => Some(b'e'),
        0x13 => Some(b'r'),
        0x14 => Some(b't'),
        0x15 => Some(b'y'),
        0x16 => Some(b'u'),
        0x17 => Some(b'i'),
        0x18 => Some(b'o'),
        0x19 => Some(b'p'),
        0x1c => Some(b'\n'),
        0x1e => Some(b'a'),
        0x1f => Some(b's'),
        0x20 => Some(b'd'),
        0x21 => Some(b'f'),
        0x22 => Some(b'g'),
        0x23 => Some(b'h'),
        0x24 => Some(b'j'),
        0x25 => Some(b'k'),
        0x26 => Some(b'l'),
        0x2c => Some(b'z'),
        0x2d => Some(b'x'),
        0x2e => Some(b'c'),
        0x2f => Some(b'v'),
        0x30 => Some(b'b'),
        0x31 => Some(b'n'),
        0x32 => Some(b'm'),
        0x39 => Some(b' '),
        _ => None,
    }
}

fn self_test() {
    let mut tty = Tty::new();
    tty.input(b'a');
    tty.input(b'b');
    tty.input(0x08);
    tty.input(b'c');
    tty.input(b'\n');
    let line = tty.read_line().expect("tty canonical read failed");
    if line != b"ac" {
        panic!("tty backspace self-test failed");
    }
    if tty.input(0x03) != Some(Signal::Int) {
        panic!("tty ctrl-c self-test failed");
    }
    if translate_set1(0x1e) != Some(b'a') || translate_set1(0x9e).is_some() {
        panic!("tty scancode translation self-test failed");
    }
    tty.set_mode(TtyMode::Raw);
    tty.input(0x7f);
    tty.input(0x04);
    if tty.read_line() != Some(Vec::new()) {
        panic!("tty ctrl-d self-test failed");
    }
    crate::println!("TTY self-test passed.");
}
