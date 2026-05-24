pub mod serial;
pub mod vga;

pub fn init() {
    serial::init();
    vga::clear_screen();
}

