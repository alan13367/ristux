use alloc::vec::Vec;
use core::str;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::{process::Pid, signal::Signal, sync::spinlock::SpinLock};

static TTY: SpinLock<Option<Tty>> = SpinLock::new(None);
static LEFT_SHIFT: AtomicBool = AtomicBool::new(false);
static RIGHT_SHIFT: AtomicBool = AtomicBool::new(false);
static CTRL: AtomicBool = AtomicBool::new(false);
static ALT: AtomicBool = AtomicBool::new(false);
static EXTENDED_SCANCODE: AtomicBool = AtomicBool::new(false);
static KEYBOARD_LAYOUT: AtomicU8 = AtomicU8::new(KEYBOARD_LAYOUT_SPANISH_MAC);

const KEYBOARD_LAYOUT_US: u8 = 0;
const KEYBOARD_LAYOUT_SPANISH_MAC: u8 = 1;

pub const TERMIOS_SIZE: usize = 60;
const NCCS: usize = 32;
const VINTR: usize = 0;
const VERASE: usize = 2;
const VEOF: usize = 4;
const VTIME: usize = 5;
const VMIN: usize = 6;
const VSUSP: usize = 10;

const IFLAG_ICRNL: u32 = 0x100;
const OFLAG_OPOST: u32 = 0x1;
const OFLAG_ONLCR: u32 = 0x4;
const CFLAG_CREAD: u32 = 0x80;
const CFLAG_CS8: u32 = 0x30;
const LFLAG_ISIG: u32 = 0x1;
const LFLAG_ICANON: u32 = 0x2;
const LFLAG_ECHO: u32 = 0x8;
const LFLAG_ECHOE: u32 = 0x10;
const LFLAG_ECHOK: u32 = 0x20;
const LFLAG_IEXTEN: u32 = 0x8000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyMode {
    Canonical,
    Raw,
}

pub enum ReadOutcome {
    Ready(Vec<u8>),
    WouldBlock,
    WaitUntil(u64),
}

enum TtyEcho {
    Text(&'static str),
    Byte(u8),
}

#[derive(Clone, Copy)]
struct TtyWaiter {
    pid: Pid,
    deadline_ms: Option<u64>,
}

#[derive(Clone, Copy)]
struct Termios {
    iflag: u32,
    oflag: u32,
    cflag: u32,
    lflag: u32,
    line: u8,
    cc: [u8; NCCS],
    ispeed: u32,
    ospeed: u32,
}

impl Termios {
    const fn default() -> Self {
        let mut cc = [0u8; NCCS];
        cc[VINTR] = 0x03;
        cc[VERASE] = 0x7f;
        cc[VEOF] = 0x04;
        cc[VMIN] = 1;
        cc[VTIME] = 0;
        cc[VSUSP] = 0x1a;
        Self {
            iflag: IFLAG_ICRNL,
            oflag: OFLAG_OPOST | OFLAG_ONLCR,
            cflag: CFLAG_CREAD | CFLAG_CS8,
            lflag: LFLAG_ISIG
                | LFLAG_ICANON
                | LFLAG_ECHO
                | LFLAG_ECHOE
                | LFLAG_ECHOK
                | LFLAG_IEXTEN,
            line: 0,
            cc,
            ispeed: 38_400,
            ospeed: 38_400,
        }
    }

    fn to_bytes(self) -> [u8; TERMIOS_SIZE] {
        let mut out = [0u8; TERMIOS_SIZE];
        put_u32(&mut out, 0, self.iflag);
        put_u32(&mut out, 4, self.oflag);
        put_u32(&mut out, 8, self.cflag);
        put_u32(&mut out, 12, self.lflag);
        out[16] = self.line;
        out[17..17 + NCCS].copy_from_slice(&self.cc);
        put_u32(&mut out, 52, self.ispeed);
        put_u32(&mut out, 56, self.ospeed);
        out
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < TERMIOS_SIZE {
            return None;
        }
        let mut cc = [0u8; NCCS];
        cc.copy_from_slice(&bytes[17..17 + NCCS]);
        Some(Self {
            iflag: read_u32(bytes, 0),
            oflag: read_u32(bytes, 4),
            cflag: read_u32(bytes, 8),
            lflag: read_u32(bytes, 12),
            line: bytes[16],
            cc,
            ispeed: read_u32(bytes, 52),
            ospeed: read_u32(bytes, 56),
        })
    }
}

pub struct Tty {
    termios: Termios,
    line: Vec<u8>,
    /// Lines committed by a newline that are ready to be consumed by readers.
    ready: Vec<Vec<u8>>,
    eof: bool,
    /// Processes parked inside `read()` waiting for input.
    waiters: Vec<TtyWaiter>,
    raw_first_byte_ms: Option<u64>,
    foreground_pgrp: Pid,
}

impl Tty {
    pub fn new() -> Self {
        Self {
            termios: Termios::default(),
            line: Vec::new(),
            ready: Vec::new(),
            eof: false,
            waiters: Vec::new(),
            raw_first_byte_ms: None,
            foreground_pgrp: 1,
        }
    }

    pub fn set_mode(&mut self, mode: TtyMode) {
        match mode {
            TtyMode::Canonical => {
                self.termios.lflag |= LFLAG_ISIG | LFLAG_ICANON | LFLAG_ECHO;
                self.termios.cc[VMIN] = 1;
                self.termios.cc[VTIME] = 0;
            }
            TtyMode::Raw => {
                self.termios.lflag &= !(LFLAG_ISIG | LFLAG_ICANON | LFLAG_ECHO | LFLAG_IEXTEN);
                self.termios.iflag &= !IFLAG_ICRNL;
                self.termios.oflag &= !OFLAG_OPOST;
                self.termios.cc[VMIN] = 1;
                self.termios.cc[VTIME] = 0;
            }
        }
    }

    pub fn input(&mut self, byte: u8) -> Option<Signal> {
        if self.signal_chars_enabled() {
            if self.control_char(VINTR) == Some(byte) {
                return Some(Signal::Int);
            }
            if self.control_char(VSUSP) == Some(byte) {
                return Some(Signal::Tstp);
            }
        }
        if !self.canonical_enabled() {
            if self.line.is_empty() {
                self.raw_first_byte_ms = Some(crate::time::uptime_millis());
            }
            self.line.push(byte);
            return None;
        }
        match byte {
            byte if self.control_char(VEOF) == Some(byte) => {
                self.eof = true;
                None
            }
            byte if self.control_char(VERASE) == Some(byte) || byte == 0x08 => {
                self.line.pop();
                None
            }
            b'\n' => {
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

    pub fn read_available(&mut self) -> Option<Vec<u8>> {
        self.read_available_at(crate::time::uptime_millis(), false)
    }

    fn read_available_at(&mut self, now_ms: u64, timeout_expired: bool) -> Option<Vec<u8>> {
        if !self.canonical_enabled() {
            let min = self.termios.cc[VMIN] as usize;
            let time_ds = self.termios.cc[VTIME] as u64;
            if self.line.is_empty() {
                return if min == 0 && (time_ds == 0 || timeout_expired) {
                    Some(Vec::new())
                } else {
                    None
                };
            }
            let interbyte_expired = time_ds != 0
                && self
                    .raw_first_byte_ms
                    .map(|first| now_ms >= first.saturating_add(time_ds.saturating_mul(100)))
                    .unwrap_or(false);
            if min != 0 && self.line.len() < min && !interbyte_expired {
                return None;
            }
            self.raw_first_byte_ms = None;
            return Some(self.line.drain(..).collect());
        }
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

    fn canonical_enabled(&self) -> bool {
        self.termios.lflag & LFLAG_ICANON != 0
    }

    fn signal_chars_enabled(&self) -> bool {
        self.termios.lflag & LFLAG_ISIG != 0
    }

    fn echo_enabled(&self) -> bool {
        self.termios.lflag & LFLAG_ECHO != 0
    }

    fn erase_echo_enabled(&self) -> bool {
        self.termios.lflag & LFLAG_ECHOE != 0
    }

    fn control_char(&self, index: usize) -> Option<u8> {
        let byte = self.termios.cc.get(index).copied().unwrap_or(0);
        if byte == 0 {
            None
        } else {
            Some(byte)
        }
    }

    fn termios(&self) -> Termios {
        self.termios
    }

    fn set_termios(&mut self, termios: Termios) {
        self.termios = termios;
        if self.canonical_enabled() || self.line.is_empty() {
            self.raw_first_byte_ms = None;
        } else if self.raw_first_byte_ms.is_none() {
            self.raw_first_byte_ms = Some(crate::time::uptime_millis());
        }
    }

    fn park(&mut self, pid: Pid, deadline_ms: Option<u64>) {
        if let Some(waiter) = self.waiters.iter_mut().find(|waiter| waiter.pid == pid) {
            waiter.deadline_ms = deadline_ms;
        } else {
            self.waiters.push(TtyWaiter { pid, deadline_ms });
        }
    }

    fn drain_waiters(&mut self) -> Vec<Pid> {
        core::mem::take(&mut self.waiters)
            .into_iter()
            .map(|waiter| waiter.pid)
            .collect()
    }

    fn expired_waiter(&mut self, pid: Pid, now_ms: u64) -> bool {
        let Some(index) = self.waiters.iter().position(|waiter| {
            waiter.pid == pid
                && waiter
                    .deadline_ms
                    .is_some_and(|deadline| now_ms >= deadline)
        }) else {
            return false;
        };
        self.waiters.swap_remove(index);
        true
    }

    fn clear_waiter(&mut self, pid: Pid) {
        if let Some(index) = self.waiters.iter().position(|waiter| waiter.pid == pid) {
            self.waiters.swap_remove(index);
        }
    }

    fn read_deadline_ms(&self, now_ms: u64) -> Option<u64> {
        if self.canonical_enabled() {
            return None;
        }
        let min = self.termios.cc[VMIN] as usize;
        let time_ds = self.termios.cc[VTIME] as u64;
        if time_ds == 0 {
            return None;
        }
        let timeout_ms = time_ds.saturating_mul(100);
        if self.line.is_empty() {
            return (min == 0).then_some(now_ms.saturating_add(timeout_ms));
        }
        if min != 0 && self.line.len() < min {
            return self
                .raw_first_byte_ms
                .map(|first| first.saturating_add(timeout_ms));
        }
        None
    }

    fn has_data(&self) -> bool {
        if self.canonical_enabled() {
            !self.ready.is_empty() || self.eof
        } else {
            let min = self.termios.cc[VMIN] as usize;
            !self.line.is_empty() && (min == 0 || self.line.len() >= min)
        }
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

    let (waiters, signal_target, echo) = {
        let mut guard = TTY.lock();
        let tty = guard.as_mut().expect("TTY used before initialization");
        let echo = tty_echo_for_input(tty, byte);
        if let Some(signal) = tty.input(byte) {
            (Vec::new(), Some((tty.foreground_pgrp(), signal)), None)
        } else {
            if byte == b'\n' {
                let line = tty.pending_line().unwrap_or(&[]);
                let trimmed = line.strip_suffix(b"\n").unwrap_or(line);
                match str::from_utf8(trimmed) {
                    Ok(text) => crate::serial_println!("TTY canonical line ready: {}", text),
                    Err(_) => crate::serial_println!("TTY canonical line ready: <binary>"),
                }
            }

            let waiters = if tty.has_data() {
                tty.drain_waiters()
            } else {
                Vec::new()
            };
            (waiters, None, echo)
        }
    };

    match echo {
        Some(TtyEcho::Text(text)) => crate::log::write_str(text),
        Some(TtyEcho::Byte(byte)) => {
            let bytes = [byte];
            if let Ok(text) = str::from_utf8(&bytes) {
                crate::log::write_str(text);
            }
        }
        None => {}
    }

    if let Some((pgrp, signal)) = signal_target {
        crate::serial_println!(
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

fn tty_echo_for_input(tty: &Tty, byte: u8) -> Option<TtyEcho> {
    if !tty.echo_enabled() {
        return None;
    }
    if tty.signal_chars_enabled()
        && (tty.control_char(VINTR) == Some(byte) || tty.control_char(VSUSP) == Some(byte))
    {
        return None;
    }
    if tty.canonical_enabled() {
        if tty.control_char(VEOF) == Some(byte) {
            return None;
        }
        if tty.control_char(VERASE) == Some(byte) || byte == 0x08 {
            return (tty.erase_echo_enabled() && !tty.line.is_empty())
                .then_some(TtyEcho::Text("\x08 \x08"));
        }
    }
    match byte {
        b'\n' => Some(TtyEcho::Text("\n")),
        b'\t' => Some(TtyEcho::Text("\t")),
        0x20..=0x7e => Some(TtyEcho::Byte(byte)),
        _ => None,
    }
}

/// Compatibility shim for callers that read bytes synchronously. Returns 0 when
/// no full line is ready (callers should fall back to parking via
/// [`park_current`] + the syscall yield mechanism).
pub fn read(output: &mut [u8]) -> usize {
    let line = {
        let mut guard = TTY.lock();
        let tty = guard.as_mut().expect("TTY used before initialization");
        tty.read_available()
    };
    let Some(line) = line else {
        return 0;
    };
    let count = line.len().min(output.len());
    output[..count].copy_from_slice(&line[..count]);
    count
}

pub fn try_read_for_current() -> ReadOutcome {
    let now_ms = crate::time::uptime_millis();
    let pid = crate::process::current_pid();
    let mut guard = TTY.lock();
    let tty = guard.as_mut().expect("TTY used before initialization");
    let timeout_expired = pid
        .map(|pid| tty.expired_waiter(pid, now_ms))
        .unwrap_or(false);
    if let Some(mut line) = tty.read_available_at(now_ms, timeout_expired) {
        if let Some(pid) = pid {
            tty.clear_waiter(pid);
        }
        if tty.canonical_enabled() && !line.ends_with(b"\n") {
            line.push(b'\n');
        }
        return ReadOutcome::Ready(line);
    }
    match tty.read_deadline_ms(now_ms) {
        Some(deadline_ms) => ReadOutcome::WaitUntil(deadline_ms),
        None => ReadOutcome::WouldBlock,
    }
}

pub fn termios_bytes() -> [u8; TERMIOS_SIZE] {
    let guard = TTY.lock();
    guard
        .as_ref()
        .expect("TTY used before initialization")
        .termios()
        .to_bytes()
}

pub fn default_termios_bytes() -> [u8; TERMIOS_SIZE] {
    Termios::default().to_bytes()
}

pub fn set_termios_bytes(bytes: &[u8]) -> Result<(), ()> {
    let termios = Termios::from_bytes(bytes).ok_or(())?;
    let mut guard = TTY.lock();
    let tty = guard.as_mut().expect("TTY used before initialization");
    tty.set_termios(termios);
    Ok(())
}

pub fn has_data() -> bool {
    let guard = TTY.lock();
    guard.as_ref().map(|tty| tty.has_data()).unwrap_or(false)
}

/// Park the current process on the TTY wait-queue.
pub fn park_current() {
    park_current_until(None);
}

pub fn park_current_until(deadline_ms: Option<u64>) {
    let Some(pid) = crate::process::current_pid() else {
        return;
    };
    {
        let mut guard = TTY.lock();
        let tty = guard.as_mut().expect("TTY used before initialization");
        tty.park(pid, deadline_ms);
    }
    match deadline_ms {
        Some(deadline_ms) => {
            crate::process::block_current(crate::process::BlockReason::WaitIoUntil(deadline_ms))
        }
        None => crate::process::block_current(crate::process::BlockReason::WaitIo),
    }
}

pub fn foreground_pgrp() -> Pid {
    let guard = TTY.lock();
    guard.as_ref().map(|tty| tty.foreground_pgrp()).unwrap_or(1)
}

pub fn set_foreground_pgrp(pgrp: Pid) {
    let mut guard = TTY.lock();
    let tty = guard.as_mut().expect("TTY used before initialization");
    tty.set_foreground_pgrp(pgrp);
}

pub fn configure_keyboard_layout(cmdline: Option<&str>) {
    let layout = match cmdline {
        Some(cmdline) if cmdline.contains("kbd=us") || cmdline.contains("keyboard=us") => {
            KEYBOARD_LAYOUT_US
        }
        Some(cmdline)
            if cmdline.contains("kbd=es")
                || cmdline.contains("kbd=es-mac")
                || cmdline.contains("keyboard=es")
                || cmdline.contains("keyboard=es-mac") =>
        {
            KEYBOARD_LAYOUT_SPANISH_MAC
        }
        _ => KEYBOARD_LAYOUT_SPANISH_MAC,
    };
    KEYBOARD_LAYOUT.store(layout, Ordering::Relaxed);
    crate::println!("Keyboard layout: {}.", keyboard_layout_name(layout));
}

fn keyboard_layout_name(layout: u8) -> &'static str {
    match layout {
        KEYBOARD_LAYOUT_US => "us",
        KEYBOARD_LAYOUT_SPANISH_MAC => "es-mac",
        _ => "unknown",
    }
}

fn translate_set1(scancode: u8) -> Option<u8> {
    if scancode == 0xe0 {
        EXTENDED_SCANCODE.store(true, Ordering::Relaxed);
        return None;
    }
    let _extended = EXTENDED_SCANCODE.swap(false, Ordering::Relaxed);
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
        0x38 => {
            ALT.store(true, Ordering::Relaxed);
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
        0xb8 => {
            ALT.store(false, Ordering::Relaxed);
            return None;
        }
        _ => {}
    }

    if scancode & 0x80 != 0 {
        return None;
    }

    let shifted = LEFT_SHIFT.load(Ordering::Relaxed) || RIGHT_SHIFT.load(Ordering::Relaxed);
    let alt = ALT.load(Ordering::Relaxed);
    let byte = match KEYBOARD_LAYOUT.load(Ordering::Relaxed) {
        KEYBOARD_LAYOUT_SPANISH_MAC => translate_spanish_mac(scancode, shifted, alt),
        _ => translate_us(scancode, shifted),
    }?;
    if CTRL.load(Ordering::Relaxed) && byte.is_ascii_alphabetic() {
        return Some(byte.to_ascii_lowercase() & 0x1f);
    }
    Some(byte)
}

fn translate_us(scancode: u8, shifted: bool) -> Option<u8> {
    match scancode {
        0x01 => Some(0x1b),
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
    }
}

fn translate_spanish_mac(scancode: u8, shifted: bool, alt: bool) -> Option<u8> {
    if alt {
        match (scancode, shifted) {
            (0x02, false) => return Some(b'|'),
            (0x03, false) => return Some(b'@'),
            (0x04, false) => return Some(b'#'),
            (0x08, false) => return Some(b'{'),
            (0x08, true) => return Some(b'\\'),
            (0x09, false) => return Some(b'['),
            (0x0a, false) => return Some(b']'),
            (0x0b, false) => return Some(b'}'),
            (0x0c, false) => return Some(b'\\'),
            (0x1a, false) => return Some(b'['),
            (0x1a, true) => return Some(b'{'),
            (0x1b, false) => return Some(b']'),
            (0x1b, true) => return Some(b'}'),
            (0x28, false) => return Some(b'{'),
            (0x29, false) => return Some(b'\\'),
            (0x2b, false) => return Some(b'}'),
            (0x31, false) => return Some(b'~'),
            (0x56, false) => return Some(b'|'),
            _ => {}
        }
    }

    match scancode {
        0x01 => Some(0x1b),
        0x02 => Some(if shifted { b'!' } else { b'1' }),
        0x03 => Some(if shifted { b'"' } else { b'2' }),
        0x04 => Some(if shifted { b'#' } else { b'3' }),
        0x05 => Some(if shifted { b'$' } else { b'4' }),
        0x06 => Some(if shifted { b'%' } else { b'5' }),
        0x07 => Some(if shifted { b'&' } else { b'6' }),
        0x08 => Some(if shifted { b'/' } else { b'7' }),
        0x09 => Some(if shifted { b'(' } else { b'8' }),
        0x0a => Some(if shifted { b')' } else { b'9' }),
        0x0b => Some(if shifted { b'=' } else { b'0' }),
        0x0c => Some(if shifted { b'?' } else { b'\'' }),
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
        0x1a => Some(if shifted { b'^' } else { b'`' }),
        0x1b => Some(if shifted { b'*' } else { b'+' }),
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
        0x33 => Some(if shifted { b';' } else { b',' }),
        0x34 => Some(if shifted { b':' } else { b'.' }),
        0x35 => Some(if shifted { b'_' } else { b'-' }),
        0x39 => Some(b' '),
        0x56 => Some(if shifted { b'>' } else { b'<' }),
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
    let line = tty.read_available().expect("tty canonical read failed");
    if line != b"ac\n" && line != b"ac" {
        panic!("tty backspace self-test failed");
    }
    if tty.input(0x03) != Some(Signal::Int) {
        panic!("tty ctrl-c self-test failed");
    }
    let layout = KEYBOARD_LAYOUT.load(Ordering::Relaxed);
    if translate_set1(0x01) != Some(0x1b)
        || translate_set1(0x1e) != Some(b'a')
        || translate_set1(0x9e).is_some()
    {
        panic!("tty scancode translation self-test failed");
    }
    translate_set1(0x2a);
    let shifted_punctuation = if layout == KEYBOARD_LAYOUT_SPANISH_MAC {
        translate_set1(0x34) == Some(b':')
    } else {
        translate_set1(0x2b) == Some(b'|')
    };
    if !shifted_punctuation {
        panic!("tty shifted punctuation self-test failed");
    }
    translate_set1(0xaa);
    if layout == KEYBOARD_LAYOUT_SPANISH_MAC {
        if translate_set1(0x56) != Some(b'<') {
            panic!("tty spanish angle bracket self-test failed");
        }
        translate_set1(0x2a);
        if translate_set1(0x56) != Some(b'>') {
            panic!("tty spanish shifted angle bracket self-test failed");
        }
        translate_set1(0xaa);
        translate_set1(0x38);
        let mac_symbols = translate_set1(0x1a) == Some(b'[')
            && translate_set1(0x1b) == Some(b']')
            && translate_set1(0x28) == Some(b'{')
            && translate_set1(0x2b) == Some(b'}');
        let pc_symbols = translate_set1(0x09) == Some(b'[')
            && translate_set1(0x0a) == Some(b']')
            && translate_set1(0x08) == Some(b'{')
            && translate_set1(0x0b) == Some(b'}');
        if !mac_symbols || !pc_symbols {
            panic!("tty spanish bracket self-test failed");
        }
        translate_set1(0xb8);
    }
    tty.input(0x04);
    if tty.read_available() != Some(Vec::new()) {
        panic!("tty ctrl-d self-test failed");
    }
    tty.set_mode(TtyMode::Raw);
    tty.input(0x7f);
    tty.input(0x04);
    if tty.read_available() != Some(alloc::vec![0x7f, 0x04]) {
        panic!("tty raw read self-test failed");
    }
    tty.set_mode(TtyMode::Canonical);
    crate::println!("TTY self-test passed.");
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
