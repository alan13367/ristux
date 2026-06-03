pub mod fpu;
pub mod gdt;
pub mod idt;
pub mod instructions;
pub mod interrupts;
pub mod port;

const IA32_FS_BASE: u32 = 0xc000_0100;

pub fn init() {
    gdt::init();
    idt::init();
}

pub fn set_user_fs_base(base: u64) {
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") IA32_FS_BASE,
            in("eax") base as u32,
            in("edx") (base >> 32) as u32,
            options(nomem, nostack, preserves_flags)
        );
    }
}
