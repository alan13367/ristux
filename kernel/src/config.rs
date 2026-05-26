pub const MULTIBOOT2_BOOTLOADER_MAGIC: u32 = 0x36d7_6289;
pub const KERNEL_HELLO: &str = "Hello from my Rust kernel loaded by GRUB.";
pub const PIT_TARGET_HZ: u32 = 100;
pub const LOG_TIMER_EVERY_TICKS: u64 = 100;
pub const KERNEL_HEAP_SIZE: usize = 16 * 1024 * 1024;
