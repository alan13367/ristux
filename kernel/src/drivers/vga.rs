use core::{fmt, fmt::Write, ptr};

use crate::sync::spinlock::SpinLock;

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;
const BUFFER_CELLS: usize = BUFFER_HEIGHT * BUFFER_WIDTH;
const VGA_BUFFER: *mut VgaChar = 0xb8000 as *mut VgaChar;
const DEFAULT_COLOR: u8 = 0x0f;
const CSI_MAX_PARAMS: usize = 8;

const BLANK: VgaChar = VgaChar {
    ascii: b' ',
    color: DEFAULT_COLOR,
};

const ANSI_TO_VGA: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

#[repr(C)]
#[derive(Clone, Copy)]
struct VgaChar {
    ascii: u8,
    color: u8,
}

#[derive(Clone, Copy)]
enum EscapeState {
    Ground,
    Escape,
    Csi,
}

static WRITER: SpinLock<VgaWriter> = SpinLock::new(VgaWriter::new());

pub struct VgaWriter {
    column: usize,
    row: usize,
    color: u8,
    saved_column: usize,
    saved_row: usize,
    state: EscapeState,
    csi_private: bool,
    csi_params: [usize; CSI_MAX_PARAMS],
    csi_param_count: usize,
    csi_current: usize,
    csi_has_current: bool,
    alternate_active: bool,
    alternate_saved_column: usize,
    alternate_saved_row: usize,
    screen: [VgaChar; BUFFER_CELLS],
    alternate_screen: [VgaChar; BUFFER_CELLS],
}

impl VgaWriter {
    pub const fn new() -> Self {
        Self {
            column: 0,
            row: 0,
            color: DEFAULT_COLOR,
            saved_column: 0,
            saved_row: 0,
            state: EscapeState::Ground,
            csi_private: false,
            csi_params: [0; CSI_MAX_PARAMS],
            csi_param_count: 0,
            csi_current: 0,
            csi_has_current: false,
            alternate_active: false,
            alternate_saved_column: 0,
            alternate_saved_row: 0,
            screen: [BLANK; BUFFER_CELLS],
            alternate_screen: [BLANK; BUFFER_CELLS],
        }
    }

    fn write_byte(&mut self, byte: u8) {
        match self.state {
            EscapeState::Ground => self.write_ground(byte),
            EscapeState::Escape => self.write_escape(byte),
            EscapeState::Csi => self.write_csi(byte),
        }
    }

    fn write_ground(&mut self, byte: u8) {
        match byte {
            0x1b => self.state = EscapeState::Escape,
            b'\n' => self.new_line(),
            b'\r' => self.column = 0,
            b'\x08' | 0x7f => self.backspace(),
            b'\t' => {
                let spaces = 8 - (self.column % 8);
                for _ in 0..spaces {
                    self.write_printable(b' ');
                }
            }
            0x20..=0x7e => self.write_printable(byte),
            _ => self.write_printable(0xfe),
        }
    }

    fn write_escape(&mut self, byte: u8) {
        match byte {
            b'[' => self.begin_csi(),
            b'7' => {
                self.save_cursor();
                self.state = EscapeState::Ground;
            }
            b'8' => {
                self.restore_cursor();
                self.state = EscapeState::Ground;
            }
            b'c' => {
                self.reset_terminal();
                self.state = EscapeState::Ground;
            }
            byte => {
                self.state = EscapeState::Ground;
                if byte.is_ascii_graphic() || byte == b' ' {
                    self.write_ground(byte);
                }
            }
        }
    }

    fn write_csi(&mut self, byte: u8) {
        match byte {
            b'?' if self.csi_param_count == 0 && !self.csi_has_current => {
                self.csi_private = true;
            }
            b'0'..=b'9' => {
                self.csi_has_current = true;
                self.csi_current = self
                    .csi_current
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as usize)
                    .min(9999);
            }
            b';' => self.push_csi_param(),
            0x40..=0x7e => {
                self.finish_csi(byte);
                self.state = EscapeState::Ground;
            }
            _ => self.state = EscapeState::Ground,
        }
    }

    fn write_printable(&mut self, byte: u8) {
        if self.column >= BUFFER_WIDTH {
            self.new_line();
        }

        self.write_at(self.row, self.column, byte);
        self.column += 1;
    }

    fn new_line(&mut self) {
        self.column = 0;
        if self.row + 1 >= BUFFER_HEIGHT {
            self.scroll();
        } else {
            self.row += 1;
        }
    }

    fn scroll(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for column in 0..BUFFER_WIDTH {
                let ch = self.read_at(row, column);
                self.write_char_at(row - 1, column, ch);
            }
        }

        self.clear_row(BUFFER_HEIGHT - 1);
    }

    fn backspace(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.column = BUFFER_WIDTH - 1;
        }
    }

    fn clear_row(&mut self, row: usize) {
        for column in 0..BUFFER_WIDTH {
            self.write_at(row, column, b' ');
        }
    }

    fn clear(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column = 0;
        self.row = 0;
    }

    fn reset_terminal(&mut self) {
        self.color = DEFAULT_COLOR;
        self.saved_column = 0;
        self.saved_row = 0;
        self.state = EscapeState::Ground;
        self.alternate_active = false;
        self.clear();
    }

    fn write_at(&mut self, row: usize, column: usize, byte: u8) {
        self.write_char_at(
            row,
            column,
            VgaChar {
                ascii: byte,
                color: self.color,
            },
        );
    }

    fn write_char_at(&mut self, row: usize, column: usize, ch: VgaChar) {
        let index = row * BUFFER_WIDTH + column;
        self.screen[index] = ch;
        unsafe {
            ptr::write_volatile(VGA_BUFFER.add(index), ch);
        }
    }

    fn read_at(&self, row: usize, column: usize) -> VgaChar {
        self.read_index(row * BUFFER_WIDTH + column)
    }

    fn read_index(&self, index: usize) -> VgaChar {
        self.screen[index]
    }

    fn write_index(&mut self, index: usize, ch: VgaChar) {
        self.screen[index] = ch;
        unsafe {
            ptr::write_volatile(VGA_BUFFER.add(index), ch);
        }
    }

    fn begin_csi(&mut self) {
        self.state = EscapeState::Csi;
        self.csi_private = false;
        self.csi_params = [0; CSI_MAX_PARAMS];
        self.csi_param_count = 0;
        self.csi_current = 0;
        self.csi_has_current = false;
    }

    fn push_csi_param(&mut self) {
        if self.csi_param_count < CSI_MAX_PARAMS {
            self.csi_params[self.csi_param_count] = if self.csi_has_current {
                self.csi_current
            } else {
                0
            };
            self.csi_param_count += 1;
        }
        self.csi_current = 0;
        self.csi_has_current = false;
    }

    fn finish_csi(&mut self, final_byte: u8) {
        if self.csi_has_current || self.csi_param_count > 0 {
            self.push_csi_param();
        }

        match final_byte {
            b'A' => self.cursor_up(self.param_or(0, 1)),
            b'B' => self.cursor_down(self.param_or(0, 1)),
            b'C' => self.cursor_forward(self.param_or(0, 1)),
            b'D' => self.cursor_back(self.param_or(0, 1)),
            b'G' => self.set_column(self.param_or(0, 1)),
            b'H' | b'f' => self.set_cursor(self.param_or(0, 1), self.param_or(1, 1)),
            b'J' => self.erase_display(self.param_or_zero(0)),
            b'K' => self.erase_line(self.param_or_zero(0)),
            b'm' => self.set_graphics(),
            b's' => self.save_cursor(),
            b'u' => self.restore_cursor(),
            b'h' if self.csi_private => self.set_private_mode(true),
            b'l' if self.csi_private => self.set_private_mode(false),
            _ => {}
        }
    }

    fn param_or(&self, index: usize, default: usize) -> usize {
        let value = self.param_or_zero(index);
        if value == 0 { default } else { value }
    }

    fn param_or_zero(&self, index: usize) -> usize {
        if index < self.csi_param_count {
            self.csi_params[index]
        } else {
            0
        }
    }

    fn set_cursor(&mut self, row: usize, column: usize) {
        self.row = row.saturating_sub(1).min(BUFFER_HEIGHT - 1);
        self.column = column.saturating_sub(1).min(BUFFER_WIDTH - 1);
    }

    fn set_column(&mut self, column: usize) {
        self.column = column.saturating_sub(1).min(BUFFER_WIDTH - 1);
    }

    fn cursor_up(&mut self, count: usize) {
        self.row = self.row.saturating_sub(count);
    }

    fn cursor_down(&mut self, count: usize) {
        self.row = (self.row + count).min(BUFFER_HEIGHT - 1);
    }

    fn cursor_forward(&mut self, count: usize) {
        self.column = (self.column + count).min(BUFFER_WIDTH - 1);
    }

    fn cursor_back(&mut self, count: usize) {
        self.column = self.column.saturating_sub(count);
    }

    fn erase_display(&mut self, mode: usize) {
        match mode {
            0 => {
                for column in self.column..BUFFER_WIDTH {
                    self.write_at(self.row, column, b' ');
                }
                for row in self.row + 1..BUFFER_HEIGHT {
                    self.clear_row(row);
                }
            }
            1 => {
                for row in 0..self.row {
                    self.clear_row(row);
                }
                for column in 0..=self.column {
                    self.write_at(self.row, column, b' ');
                }
            }
            2 | 3 => self.clear(),
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: usize) {
        match mode {
            0 => {
                for column in self.column..BUFFER_WIDTH {
                    self.write_at(self.row, column, b' ');
                }
            }
            1 => {
                for column in 0..=self.column {
                    self.write_at(self.row, column, b' ');
                }
            }
            2 => self.clear_row(self.row),
            _ => {}
        }
    }

    fn set_graphics(&mut self) {
        if self.csi_param_count == 0 {
            self.color = DEFAULT_COLOR;
            return;
        }

        for index in 0..self.csi_param_count {
            match self.csi_params[index] {
                0 => self.color = DEFAULT_COLOR,
                1 => self.color |= 0x08,
                22 => self.color &= !0x08,
                30..=37 => {
                    let ansi = self.csi_params[index] - 30;
                    self.color = (self.color & 0xf0) | ANSI_TO_VGA[ansi];
                }
                39 => self.color = (self.color & 0xf0) | (DEFAULT_COLOR & 0x0f),
                40..=47 => {
                    let ansi = self.csi_params[index] - 40;
                    self.color = (self.color & 0x0f) | (ANSI_TO_VGA[ansi] << 4);
                }
                49 => self.color = (self.color & 0x0f) | (DEFAULT_COLOR & 0xf0),
                90..=97 => {
                    let ansi = self.csi_params[index] - 90;
                    self.color = (self.color & 0xf0) | (ANSI_TO_VGA[ansi] | 0x08);
                }
                100..=107 => {
                    let ansi = self.csi_params[index] - 100;
                    self.color = (self.color & 0x0f) | ((ANSI_TO_VGA[ansi] | 0x08) << 4);
                }
                _ => {}
            }
        }
    }

    fn save_cursor(&mut self) {
        self.saved_row = self.row;
        self.saved_column = self.column;
    }

    fn restore_cursor(&mut self) {
        self.row = self.saved_row.min(BUFFER_HEIGHT - 1);
        self.column = self.saved_column.min(BUFFER_WIDTH - 1);
    }

    fn set_private_mode(&mut self, enabled: bool) {
        for index in 0..self.csi_param_count {
            match self.csi_params[index] {
                47 | 1047 | 1049 => {
                    if enabled {
                        self.enter_alternate_screen();
                    } else {
                        self.leave_alternate_screen();
                    }
                }
                1048 if enabled => self.save_cursor(),
                1048 => self.restore_cursor(),
                _ => {}
            }
        }
    }

    fn enter_alternate_screen(&mut self) {
        if self.alternate_active {
            self.clear();
            return;
        }
        for index in 0..BUFFER_CELLS {
            self.alternate_screen[index] = self.read_index(index);
        }
        self.alternate_saved_row = self.row;
        self.alternate_saved_column = self.column;
        self.alternate_active = true;
        self.clear();
    }

    fn leave_alternate_screen(&mut self) {
        if !self.alternate_active {
            return;
        }
        for index in 0..BUFFER_CELLS {
            self.write_index(index, self.alternate_screen[index]);
        }
        self.row = self.alternate_saved_row.min(BUFFER_HEIGHT - 1);
        self.column = self.alternate_saved_column.min(BUFFER_WIDTH - 1);
        self.alternate_active = false;
        self.state = EscapeState::Ground;
    }
}

impl Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

pub fn clear_screen() {
    WRITER.lock().clear();
}

pub fn write_fmt(args: fmt::Arguments<'_>) -> fmt::Result {
    WRITER.lock().write_fmt(args)
}

pub fn write_str(s: &str) -> fmt::Result {
    WRITER.lock().write_str(s)
}

pub fn self_test() -> bool {
    let mut writer = WRITER.lock();
    let saved_row = writer.row;
    let saved_column = writer.column;
    let saved_color = writer.color;
    let saved_state = writer.state;
    let saved_cursor_row = writer.saved_row;
    let saved_cursor_column = writer.saved_column;
    let saved_alternate_active = writer.alternate_active;
    let saved_alternate_row = writer.alternate_saved_row;
    let saved_alternate_column = writer.alternate_saved_column;
    let saved_shadow_screen = writer.screen;
    let saved_alternate_screen = writer.alternate_screen;

    writer.reset_terminal();
    let _ = writer.write_str("ab\x1b[2;4Hc");
    let cursor_move_ok = writer.read_at(1, 3).ascii == b'c';
    let _ = writer.write_str("\x1b[31mR");
    let color_ok = writer.read_at(1, 4).color & 0x0f == 4;
    let _ = writer.write_str("\x1b[2J");
    let clear_ok = writer.row == 0 && writer.column == 0 && writer.read_at(1, 3).ascii == b' ';
    let _ = writer.write_str("p\x1b[?1049hALT\x1b[?1049l");
    let alternate_ok = !writer.alternate_active && writer.read_at(0, 0).ascii == b'p';

    for index in 0..BUFFER_CELLS {
        writer.write_index(index, saved_shadow_screen[index]);
    }
    writer.row = saved_row;
    writer.column = saved_column;
    writer.color = saved_color;
    writer.state = saved_state;
    writer.saved_row = saved_cursor_row;
    writer.saved_column = saved_cursor_column;
    writer.alternate_active = saved_alternate_active;
    writer.alternate_saved_row = saved_alternate_row;
    writer.alternate_saved_column = saved_alternate_column;
    writer.screen = saved_shadow_screen;
    writer.alternate_screen = saved_alternate_screen;

    let ok = cursor_move_ok && color_ok && clear_ok && alternate_ok;
    if !ok {
        let _ = crate::drivers::serial::write_fmt(format_args!(
            "VGA ANSI self-test detail: cursor={} color={} clear={} alternate={}\n",
            cursor_move_ok, color_ok, clear_ok, alternate_ok
        ));
    }
    ok
}
