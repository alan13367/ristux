pub mod gdt;
pub mod idt;
pub mod instructions;
pub mod port;

pub fn init() {
    gdt::init();
    idt::init();
}
