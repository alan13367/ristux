#![no_std]
#![no_main]

use core::arch::{asm, global_asm};

global_asm!(include_str!("../boot/multiboot2_header.asm"));
global_asm!(include_str!("../boot/boot.asm"));

mod panic;

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(_multiboot_magic: u32, _multiboot_info_addr: u32) -> ! {
    halt_loop()
}

#[inline(always)]
pub fn halt_loop() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
