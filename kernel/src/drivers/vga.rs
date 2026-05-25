use core::{fmt, fmt::Write, ptr};

use crate::sync::spinlock::SpinLock;

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;
const VGA_BUFFER: *mut VgaChar = 0xb8000 as *mut VgaChar;
const COLOR: u8 = 0x0f;

#[repr(C)]
#[derive(Clone, Copy)]
struct VgaChar {
    ascii: u8,
    color: u8,
}

static WRITER: SpinLock<VgaWriter> = SpinLock::new(VgaWriter::new());

pub struct VgaWriter {
    column: usize,
    row: usize,
}

impl VgaWriter {
    pub const fn new() -> Self {
        Self { column: 0, row: 0 }
    }

    fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column >= BUFFER_WIDTH {
                    self.new_line();
                }

                self.write_at(self.row, self.column, byte);
                self.column += 1;
            }
        }
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

    fn write_at(&mut self, row: usize, column: usize, byte: u8) {
        self.write_char_at(
            row,
            column,
            VgaChar {
                ascii: byte,
                color: COLOR,
            },
        );
    }

    fn write_char_at(&mut self, row: usize, column: usize, ch: VgaChar) {
        unsafe {
            ptr::write_volatile(VGA_BUFFER.add(row * BUFFER_WIDTH + column), ch);
        }
    }

    fn read_at(&self, row: usize, column: usize) -> VgaChar {
        unsafe { ptr::read_volatile(VGA_BUFFER.add(row * BUFFER_WIDTH + column)) }
    }
}

impl Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
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
