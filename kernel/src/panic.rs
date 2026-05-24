use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    crate::println!();
    crate::println!("kernel panic: {}", info);
    crate::halt_loop()
}
