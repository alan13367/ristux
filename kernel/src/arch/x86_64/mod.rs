pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod instructions;
pub mod port;

pub fn init() {
    gdt::init();
    idt::init();
}
