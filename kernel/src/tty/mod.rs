use alloc::vec::Vec;

use crate::signal::Signal;

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

    pub fn read_line(&mut self) -> Option<Vec<u8>> {
        if self.eof {
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
    self_test();
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
    tty.set_mode(TtyMode::Raw);
    tty.input(0x7f);
    tty.input(0x04);
    if tty.read_line() != Some(Vec::new()) {
        panic!("tty ctrl-d self-test failed");
    }
    crate::println!("TTY self-test passed.");
}

