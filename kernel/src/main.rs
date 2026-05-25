#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![no_std]
#![no_main]

extern crate alloc;

use core::arch::global_asm;

global_asm!(include_str!("../boot/multiboot2_header.asm"));
global_asm!(include_str!("../boot/boot.asm"));

mod arch;
mod config;
mod drivers;
mod dynamic_linker;
mod error;
mod fs;
mod initrd;
mod ipc;
mod log;
mod memory;
mod multiboot;
mod net;
mod panic;
mod process;
mod sched;
mod security;
mod shell;
mod signal;
mod smp;
mod storage;
mod sync;
mod syscall;
mod task;
mod testing;
mod time;
mod tty;
mod userspace;

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
    arch::x86_64::init();
    arch::x86_64::idt::trigger_breakpoint();

    let boot_info = unsafe {
        multiboot::BootInfo::load(multiboot_info_addr as usize)
            .unwrap_or_else(|message| panic!("{}", message))
    };
    boot_info.print_summary();
    memory::init(&boot_info);
    time::init();
    drivers::framebuffer::init(boot_info.framebuffer());
    let initrd =
        initrd::Initrd::from_boot_info(&boot_info).unwrap_or_else(|message| panic!("{}", message));
    initrd.print_summary();
    fs::init(&initrd);
    dynamic_linker::init();
    userspace::init();
    process::init();
    ipc::init();
    security::init();
    signal::init();
    tty::init();
    net::init();
    storage::init();
    shell::init();
    task::init();

    arch::x86_64::interrupts::init();
    println!(
        "Initial timer tick count: {}",
        arch::x86_64::interrupts::timer_ticks()
    );

    smp::init(boot_info.acpi_rsdp());
    testing::run_kernel_self_tests();

    // Phase A: hand control to ring 3. /bin/init is responsible for spawning
    // /bin/sh and reaping zombies forever; once we enter user mode the kernel
    // only runs again via syscalls and interrupts.
    println!("init: spawning /bin/init");
    let _ = userspace::run_user_program("/bin/init", 1);

    // /bin/init should never exit. If it does, fall back to halt to surface
    // the failure on serial.
    println!("init exited unexpectedly; halting.");
    crate::halt_loop();
}

#[inline(always)]
pub fn halt_loop() -> ! {
    loop {
        arch::x86_64::instructions::halt();
    }
}
