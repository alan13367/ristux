#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::string::ToString;
use alloc::vec::Vec;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;

const TCGETS: usize = 0x5401;
const TCSETS: usize = 0x5402;
const TIOCGWINSZ: usize = 0x5413;
const TERMIOS_SIZE: usize = 60;
const TERMIOS_IFLAG: usize = 0;
const TERMIOS_OFLAG: usize = 4;
const TERMIOS_LFLAG: usize = 12;
const TERMIOS_CC: usize = 17;
const VTIME: usize = 5;
const VMIN: usize = 6;
const ICRNL: u32 = 0x0100;
const OPOST: u32 = 0x0001;
const ISIG: u32 = 0x0001;
const ICANON: u32 = 0x0002;
const ECHO: u32 = 0x0008;
const IEXTEN: u32 = 0x8000;
const ESCAPE_TIMEOUT_MS: i32 = 50;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Normal,
    Insert,
    Command,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Key {
    Byte(u8),
    Esc,
    Backspace,
    Enter,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

struct Terminal {
    original: [u8; TERMIOS_SIZE],
}

impl Terminal {
    fn enter() -> Option<Self> {
        let mut original = [0u8; TERMIOS_SIZE];
        if sys::ioctl(0, TCGETS, original.as_mut_ptr() as usize) < 0 {
            return None;
        }
        let mut raw = original;
        let iflag = read_u32(&raw, TERMIOS_IFLAG) & !ICRNL;
        let oflag = read_u32(&raw, TERMIOS_OFLAG) & !OPOST;
        let lflag = read_u32(&raw, TERMIOS_LFLAG) & !(ISIG | ICANON | ECHO | IEXTEN);
        set_u32(&mut raw, TERMIOS_IFLAG, iflag);
        set_u32(&mut raw, TERMIOS_OFLAG, oflag);
        set_u32(&mut raw, TERMIOS_LFLAG, lflag);
        raw[TERMIOS_CC + VMIN] = 1;
        raw[TERMIOS_CC + VTIME] = 0;
        if sys::ioctl(0, TCSETS, raw.as_ptr() as usize) < 0 {
            return None;
        }
        let _ = write_all(1, b"\x1b[?1049h\x1b[2J\x1b[H");
        Some(Self { original })
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = sys::ioctl(0, TCSETS, self.original.as_ptr() as usize);
        let _ = write_all(1, b"\x1b[0m\x1b[?1049l\x1b[2J\x1b[H");
    }
}

struct Editor {
    path: Vec<u8>,
    lines: Vec<Vec<u8>>,
    cx: usize,
    cy: usize,
    desired_col: usize,
    row_offset: usize,
    rows: usize,
    cols: usize,
    mode: Mode,
    dirty: bool,
    running: bool,
    command: Vec<u8>,
    status: Vec<u8>,
    pending_delete: bool,
}

impl Editor {
    fn new(path: &[u8], lines: Vec<Vec<u8>>) -> Self {
        let mut editor = Self {
            path: path.to_vec(),
            lines,
            cx: 0,
            cy: 0,
            desired_col: 0,
            row_offset: 0,
            rows: 24,
            cols: 80,
            mode: Mode::Normal,
            dirty: false,
            running: true,
            command: Vec::new(),
            status: Vec::new(),
            pending_delete: false,
        };
        editor.update_window_size();
        if editor.lines.is_empty() {
            editor.status = b"new file".to_vec();
        }
        editor
    }

    fn run(&mut self) -> i32 {
        let mut input = Input::new();
        while self.running {
            self.refresh_screen();
            let key = input.read_key();
            self.handle_key(key);
        }
        0
    }

    fn handle_key(&mut self, key: Key) {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Insert => self.handle_insert_key(key),
            Mode::Command => self.handle_command_key(key),
        }
        self.clamp_cursor();
    }

    fn handle_normal_key(&mut self, key: Key) {
        if self.pending_delete && key != Key::Byte(b'd') {
            self.pending_delete = false;
        }
        match key {
            Key::Byte(b':') => {
                self.command.clear();
                self.mode = Mode::Command;
                self.pending_delete = false;
            }
            Key::Byte(b'i') => {
                self.ensure_current_line();
                self.mode = Mode::Insert;
            }
            Key::Byte(b'a') => {
                self.ensure_current_line();
                if self.cx < self.current_line_len() {
                    self.cx += 1;
                }
                self.desired_col = self.cx;
                self.mode = Mode::Insert;
            }
            Key::Byte(b'A') => {
                self.ensure_current_line();
                self.cx = self.current_line_len();
                self.desired_col = self.cx;
                self.mode = Mode::Insert;
            }
            Key::Byte(b'o') => {
                let index = self.cy.saturating_add(1).min(self.lines.len());
                self.lines.insert(index, Vec::new());
                self.cy = index;
                self.cx = 0;
                self.desired_col = self.cx;
                self.mode = Mode::Insert;
                self.dirty = true;
            }
            Key::Byte(b'O') => {
                let index = self.cy.min(self.lines.len());
                self.lines.insert(index, Vec::new());
                self.cy = index;
                self.cx = 0;
                self.desired_col = self.cx;
                self.mode = Mode::Insert;
                self.dirty = true;
            }
            Key::Byte(b'h') | Key::Left => self.move_left(),
            Key::Byte(b'l') | Key::Right => self.move_right(),
            Key::Byte(b'k') | Key::Up => self.move_up(),
            Key::Byte(b'j') | Key::Down => self.move_down(),
            Key::Byte(b'0') | Key::Home => {
                self.cx = 0;
                self.desired_col = self.cx;
            }
            Key::Byte(b'$') | Key::End => {
                self.cx = self.current_line_len();
                self.desired_col = self.cx;
            }
            Key::Byte(b'G') => {
                if !self.lines.is_empty() {
                    self.cy = self.lines.len() - 1;
                    self.cx = self.cx.min(self.current_line_len());
                    self.desired_col = self.cx;
                }
            }
            Key::Byte(b'x') | Key::Delete => self.delete_char(),
            Key::Byte(b'd') if self.pending_delete => {
                self.delete_line();
                self.pending_delete = false;
            }
            Key::Byte(b'd') => self.pending_delete = true,
            _ => {}
        }
    }

    fn handle_insert_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                if self.cx > 0 {
                    self.cx -= 1;
                }
                self.desired_col = self.cx;
            }
            Key::Backspace => self.backspace(),
            Key::Delete => self.delete_char(),
            Key::Enter => self.insert_newline(),
            Key::Left => self.move_left(),
            Key::Right => self.move_right(),
            Key::Up => self.move_up(),
            Key::Down => self.move_down(),
            Key::Home => {
                self.cx = 0;
                self.desired_col = self.cx;
            }
            Key::End => {
                self.cx = self.current_line_len();
                self.desired_col = self.cx;
            }
            Key::Byte(byte) if byte == b'\t' => {
                for _ in 0..4 {
                    self.insert_byte(b' ');
                }
            }
            Key::Byte(byte) if (0x20..=0x7e).contains(&byte) => self.insert_byte(byte),
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.command.clear();
                self.status.clear();
            }
            Key::Backspace => {
                self.command.pop();
            }
            Key::Enter => self.execute_command(),
            Key::Byte(byte) if (0x20..=0x7e).contains(&byte) => self.command.push(byte),
            _ => {}
        }
    }

    fn execute_command(&mut self) {
        let command = self.command.as_slice();
        match command {
            b"w" | b"write" => {
                let _ = self.write_current_file();
            }
            b"q" | b"quit" => {
                if self.dirty {
                    self.status = b"unsaved changes: use :wq or :q!".to_vec();
                } else {
                    self.running = false;
                }
            }
            b"q!" | b"quit!" => self.running = false,
            b"wq" | b"x" => {
                if self.write_current_file() {
                    self.running = false;
                }
            }
            b"help" | b"h" => {
                self.status = b"normal: i/a/o/O edit, h/j/k/l move, x delete char, dd delete line, :wq save+quit".to_vec();
            }
            _ => self.status = b"unknown command".to_vec(),
        }
        self.command.clear();
        if self.running {
            self.mode = Mode::Normal;
        }
    }

    fn write_current_file(&mut self) -> bool {
        if save_file(&self.path, &self.lines) {
            self.dirty = false;
            let mut status = b"wrote ".to_vec();
            status.extend_from_slice(self.lines.len().to_string().as_bytes());
            status.extend_from_slice(b" line(s)");
            self.status = status;
            true
        } else {
            self.status = b"write failed".to_vec();
            false
        }
    }

    fn ensure_current_line(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
            self.cy = 0;
            self.cx = 0;
        }
    }

    fn insert_byte(&mut self, byte: u8) {
        self.ensure_current_line();
        let line = &mut self.lines[self.cy];
        line.insert(self.cx.min(line.len()), byte);
        self.cx += 1;
        self.desired_col = self.cx;
        self.dirty = true;
    }

    fn insert_newline(&mut self) {
        self.ensure_current_line();
        let rest = {
            let line = &mut self.lines[self.cy];
            line.split_off(self.cx.min(line.len()))
        };
        self.cy += 1;
        self.cx = 0;
        self.desired_col = self.cx;
        self.lines.insert(self.cy, rest);
        self.dirty = true;
    }

    fn backspace(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        if self.cx > 0 {
            let line = &mut self.lines[self.cy];
            if self.cx <= line.len() {
                line.remove(self.cx - 1);
                self.cx -= 1;
                self.desired_col = self.cx;
                self.dirty = true;
            }
        } else if self.cy > 0 {
            let current = self.lines.remove(self.cy);
            self.cy -= 1;
            self.cx = self.lines[self.cy].len();
            self.desired_col = self.cx;
            self.lines[self.cy].extend_from_slice(&current);
            self.dirty = true;
        }
    }

    fn delete_char(&mut self) {
        if self.lines.is_empty() || self.cy >= self.lines.len() {
            return;
        }
        let line = &mut self.lines[self.cy];
        if self.cx < line.len() {
            line.remove(self.cx);
            self.desired_col = self.cx;
            self.dirty = true;
        } else if self.cy + 1 < self.lines.len() {
            let next = self.lines.remove(self.cy + 1);
            self.lines[self.cy].extend_from_slice(&next);
            self.desired_col = self.cx;
            self.dirty = true;
        }
    }

    fn delete_line(&mut self) {
        if self.lines.is_empty() {
            self.status = b"no such line".to_vec();
            return;
        }
        self.lines.remove(self.cy.min(self.lines.len() - 1));
        if self.cy >= self.lines.len() && self.cy > 0 {
            self.cy -= 1;
        }
        self.cx = 0;
        self.desired_col = self.cx;
        self.dirty = true;
        self.status = b"deleted".to_vec();
    }

    fn move_left(&mut self) {
        if self.cx > 0 {
            self.cx -= 1;
        } else if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.current_line_len();
        }
        self.desired_col = self.cx;
    }

    fn move_right(&mut self) {
        if self.cx < self.current_line_len() {
            self.cx += 1;
        } else if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = 0;
        }
        self.desired_col = self.cx;
    }

    fn move_up(&mut self) {
        if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.desired_col.min(self.current_line_len());
        }
    }

    fn move_down(&mut self) {
        if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = self.desired_col.min(self.current_line_len());
        }
    }

    fn current_line_len(&self) -> usize {
        self.lines.get(self.cy).map(|line| line.len()).unwrap_or(0)
    }

    fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.cy = 0;
            self.cx = 0;
            return;
        }
        self.cy = self.cy.min(self.lines.len() - 1);
        self.cx = self.cx.min(self.current_line_len());
    }

    fn update_window_size(&mut self) {
        let mut ws = [0u8; 8];
        if sys::ioctl(1, TIOCGWINSZ, ws.as_mut_ptr() as usize) >= 0 {
            let rows = u16::from_le_bytes([ws[0], ws[1]]) as usize;
            let cols = u16::from_le_bytes([ws[2], ws[3]]) as usize;
            if rows >= 2 {
                self.rows = rows;
            }
            if cols >= 20 {
                self.cols = cols;
            }
        }
    }

    fn refresh_screen(&mut self) {
        self.update_window_size();
        self.scroll();
        let text_rows = self.rows.saturating_sub(1).max(1);
        let _ = write_all(1, b"\x1b[H");
        for screen_row in 0..text_rows {
            move_cursor(screen_row + 1, 1);
            let _ = write_all(1, b"\x1b[2K");
            let file_row = self.row_offset + screen_row;
            if file_row < self.lines.len() {
                write_display_line(&self.lines[file_row], self.cols);
            } else {
                let _ = write_all(1, b"\x1b[34m~\x1b[0m");
            }
        }
        self.draw_bottom_line();
        self.position_cursor();
    }

    fn scroll(&mut self) {
        let text_rows = self.rows.saturating_sub(1).max(1);
        if self.cy < self.row_offset {
            self.row_offset = self.cy;
        } else if self.cy >= self.row_offset + text_rows {
            self.row_offset = self.cy.saturating_sub(text_rows - 1);
        }
    }

    fn draw_bottom_line(&self) {
        move_cursor(self.rows, 1);
        let _ = write_all(1, b"\x1b[2K");
        match self.mode {
            Mode::Command => {
                let _ = write_all(1, b":");
                write_display_line(&self.command, self.cols.saturating_sub(1));
            }
            Mode::Insert => self.draw_status(b"-- INSERT --"),
            Mode::Normal => {
                if self.status.is_empty() {
                    self.draw_file_status();
                } else {
                    self.draw_status(&self.status);
                }
            }
        }
    }

    fn draw_file_status(&self) {
        let _ = write_all(1, b"\x1b[97;44m");
        let _ = write_all(1, b" ");
        let _ = write_all(1, &self.path);
        if self.dirty {
            let _ = write_all(1, b" [+]");
        }
        let _ = write_all(1, b"  ");
        let _ = write_all(1, self.lines.len().to_string().as_bytes());
        let _ = write_all(1, b" lines ");
        let _ = write_all(1, b"\x1b[0m");
    }

    fn draw_status(&self, status: &[u8]) {
        let _ = write_all(1, b"\x1b[97;44m ");
        write_display_line(status, self.cols.saturating_sub(1));
        let _ = write_all(1, b"\x1b[0m");
    }

    fn position_cursor(&self) {
        match self.mode {
            Mode::Command => move_cursor(self.rows, self.command.len().saturating_add(2)),
            _ => {
                let row = self.cy.saturating_sub(self.row_offset).saturating_add(1);
                let col = self.cx.min(self.cols.saturating_sub(1)).saturating_add(1);
                move_cursor(row.min(self.rows.saturating_sub(1).max(1)), col);
            }
        }
    }
}

fn cstr(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 1);
    out.extend_from_slice(s);
    out.push(0);
    out
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return false;
        }
        bytes = &bytes[n as usize..];
    }
    true
}

struct Input {
    pending: Vec<u8>,
}

impl Input {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    fn read_key(&mut self) -> Key {
        let byte = self.read_byte_blocking();
        self.decode_key(byte)
    }

    fn read_byte_blocking(&mut self) -> u8 {
        loop {
            if let Some(byte) = self.pop_pending() {
                return byte;
            }
            self.read_more();
            if let Some(byte) = self.pop_pending() {
                return byte;
            }
            let _ = sys::sched_yield();
        }
    }

    fn read_byte_if_ready(&mut self, timeout_ms: i32) -> Option<u8> {
        if let Some(byte) = self.pop_pending() {
            return Some(byte);
        }
        let mut pollfd = sys::PollFd {
            fd: 0,
            events: sys::POLLIN,
            revents: 0,
        };
        if sys::poll(&mut pollfd as *mut sys::PollFd, 1, timeout_ms) <= 0 {
            return None;
        }
        if pollfd.revents & sys::POLLIN == 0 {
            return None;
        }
        self.read_more();
        self.pop_pending()
    }

    fn read_more(&mut self) {
        let mut buf = [0u8; 8];
        let n = sys::read(0, &mut buf);
        if n > 0 {
            self.pending.extend_from_slice(&buf[..n as usize]);
        }
    }

    fn pop_pending(&mut self) -> Option<u8> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }

    fn decode_key(&mut self, byte: u8) -> Key {
        match byte {
            0x1b => self.decode_escape_key(),
            0x08 | 0x7f => Key::Backspace,
            b'\n' | b'\r' => Key::Enter,
            byte => Key::Byte(byte),
        }
    }

    fn decode_escape_key(&mut self) -> Key {
        match self.read_byte_if_ready(ESCAPE_TIMEOUT_MS) {
            Some(b'[') => self.decode_csi_key(),
            Some(b'O') => self.decode_ss3_key(),
            _ => Key::Esc,
        }
    }

    fn decode_csi_key(&mut self) -> Key {
        let mut seq = [0u8; 8];
        let mut len = 0usize;
        loop {
            let Some(byte) = self.read_byte_if_ready(ESCAPE_TIMEOUT_MS) else {
                return Key::Esc;
            };
            if len < seq.len() {
                seq[len] = byte;
                len += 1;
            }
            if (0x40..=0x7e).contains(&byte) {
                return match byte {
                    b'A' => Key::Up,
                    b'B' => Key::Down,
                    b'C' => Key::Right,
                    b'D' => Key::Left,
                    b'H' => Key::Home,
                    b'F' => Key::End,
                    b'~' => decode_csi_tilde_key(&seq[..len]),
                    _ => Key::Esc,
                };
            }
        }
    }

    fn decode_ss3_key(&mut self) -> Key {
        match self.read_byte_if_ready(ESCAPE_TIMEOUT_MS) {
            Some(b'A') => Key::Up,
            Some(b'B') => Key::Down,
            Some(b'C') => Key::Right,
            Some(b'D') => Key::Left,
            Some(b'H') => Key::Home,
            Some(b'F') => Key::End,
            _ => Key::Esc,
        }
    }
}

fn decode_csi_tilde_key(seq: &[u8]) -> Key {
    match seq.first().copied() {
        Some(b'1') | Some(b'7') => Key::Home,
        Some(b'3') => Key::Delete,
        Some(b'4') | Some(b'8') => Key::End,
        _ => Key::Esc,
    }
}

fn load_file(path: &[u8]) -> Vec<Vec<u8>> {
    let fd = sys::open(cstr(path).as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return Vec::new();
    }

    let mut data = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = sys::read(fd as i32, &mut buf);
        if n <= 0 {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
    }
    let _ = sys::close(fd as i32);

    let mut lines = Vec::new();
    let mut line = Vec::new();
    for byte in data {
        if byte == b'\n' {
            lines.push(core::mem::take(&mut line));
        } else {
            line.push(byte);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn save_file(path: &[u8], lines: &[Vec<u8>]) -> bool {
    let fd = sys::open(cstr(path).as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return false;
    }
    for line in lines {
        if !write_all(fd as i32, line) || !write_all(fd as i32, b"\n") {
            let _ = sys::close(fd as i32);
            return false;
        }
    }
    let _ = sys::close(fd as i32);
    true
}

fn write_display_line(line: &[u8], width: usize) {
    let mut written = 0usize;
    for &byte in line {
        if written >= width {
            break;
        }
        match byte {
            b'\t' => {
                let spaces = 4 - (written % 4);
                for _ in 0..spaces {
                    if written >= width {
                        break;
                    }
                    let _ = write_all(1, b" ");
                    written += 1;
                }
            }
            0x20..=0x7e => {
                let _ = write_all(1, &[byte]);
                written += 1;
            }
            _ => {
                if written + 2 > width {
                    break;
                }
                let _ = write_all(1, b"^");
                let printable = [byte ^ 0x40];
                let _ = write_all(1, &printable);
                written += 2;
            }
        }
    }
}

fn move_cursor(row: usize, col: usize) {
    let _ = write_all(1, b"\x1b[");
    let _ = write_all(1, row.max(1).to_string().as_bytes());
    let _ = write_all(1, b";");
    let _ = write_all(1, col.max(1).to_string().as_bytes());
    let _ = write_all(1, b"H");
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

fn set_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() < 2 {
        let _ = write_all(2, b"usage: vi FILE\n");
        return 1;
    }
    let Some(_terminal) = Terminal::enter() else {
        let _ = write_all(2, b"vi: raw terminal setup failed\n");
        return 1;
    };
    let lines = load_file(args[1]);
    let mut editor = Editor::new(args[1], lines);
    editor.run()
}

ristux_userland::program_main!(main);
