use core::fmt::{self, Write};

use crate::{arch::x86_64::port, sync::spinlock::SpinLock};

const COM1: u16 = 0x3f8;

static SERIAL1: SpinLock<SerialPort> = SpinLock::new(SerialPort::new(COM1));

pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    pub const fn new(base: u16) -> Self {
        Self { base }
    }

    pub fn init(&mut self) {
        unsafe {
            port::outb(self.base + 1, 0x00);
            port::outb(self.base + 3, 0x80);
            port::outb(self.base, 0x03);
            port::outb(self.base + 1, 0x00);
            port::outb(self.base + 3, 0x03);
            port::outb(self.base + 2, 0xc7);
            port::outb(self.base + 4, 0x0b);
        }
    }

    fn transmit_empty(&self) -> bool {
        unsafe { port::inb(self.base + 5) & 0x20 != 0 }
    }

    fn write_byte(&mut self, byte: u8) {
        while !self.transmit_empty() {}
        unsafe {
            port::outb(self.base, byte);
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

pub fn init() {
    SERIAL1.lock().init();
}

pub fn write_fmt(args: fmt::Arguments<'_>) -> fmt::Result {
    SERIAL1.lock().write_fmt(args)
}

pub fn write_str(s: &str) -> fmt::Result {
    use core::fmt::Write;
    SERIAL1.lock().write_str(s)
}
