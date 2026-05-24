#![no_std]
#![no_main]

use core::arch::global_asm;

global_asm!(include_str!("../boot/multiboot2_header.asm"));
global_asm!(include_str!("../boot/boot.asm"));

mod arch;
mod config;
mod drivers;
mod log;
mod multiboot;
mod panic;
mod sync;

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(multiboot_magic: u32, multiboot_info_addr: u32) -> ! {
    log::init();

    println!("{}", config::KERNEL_HELLO);
    println!("Multiboot2 magic: {:#010x}", multiboot_magic);
    println!("Multiboot2 info:  {:#010x}", multiboot_info_addr);

    if multiboot_magic != config::MULTIBOOT2_BOOTLOADER_MAGIC {
        panic!("invalid Multiboot2 magic: {:#010x}", multiboot_magic);
    }

    println!("Multiboot2 handoff validated.");

    let boot_info = unsafe {
        multiboot::BootInfo::load(multiboot_info_addr as usize)
            .unwrap_or_else(|message| panic!("{}", message))
    };
    boot_info.print_summary();

    halt_loop()
}

#[inline(always)]
pub fn halt_loop() -> ! {
    loop {
        arch::x86_64::instructions::halt();
    }
}
