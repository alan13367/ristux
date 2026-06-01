use core::fmt;

use crate::{arch::x86_64::instructions, drivers};

pub fn init() {
    drivers::init();
}

pub fn _print(args: fmt::Arguments<'_>) {
    instructions::without_interrupts(|| {
        let _ = drivers::serial::write_fmt(args);
        let _ = drivers::vga::write_fmt(args);
    });
}

pub fn serial_print(args: fmt::Arguments<'_>) {
    instructions::without_interrupts(|| {
        let _ = drivers::serial::write_fmt(args);
    });
}

pub fn write_str(text: &str) {
    instructions::without_interrupts(|| {
        let _ = drivers::serial::write_str(text);
        let _ = drivers::vga::write_str(text);
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::log::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($fmt:expr) => {
        $crate::print!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::print!(concat!($fmt, "\n"), $($arg)*)
    };
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::log::serial_print(format_args!("\n"))
    };
    ($fmt:expr) => {
        $crate::log::serial_print(format_args!(concat!($fmt, "\n")))
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::log::serial_print(format_args!(concat!($fmt, "\n"), $($arg)*))
    };
}
