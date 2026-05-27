use core::{arch::asm, ptr};

const CR0_MP: u64 = 1 << 1;
const CR0_EM: u64 = 1 << 2;
const CR0_TS: u64 = 1 << 3;
const CR0_NE: u64 = 1 << 5;
const CR4_OSFXSR: u64 = 1 << 9;
const CR4_OSXMMEXCPT: u64 = 1 << 10;
const MXCSR_DEFAULT: u32 = 0x1f80;
const MXCSR_MASK_DEFAULT: u32 = 0xffff;
const X87_CONTROL_WORD_DEFAULT: u16 = 0x037f;

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct FpuState {
    bytes: [u8; 512],
}

impl FpuState {
    const fn synthetic_initial() -> Self {
        let mut bytes = [0u8; 512];
        bytes[0] = (X87_CONTROL_WORD_DEFAULT & 0xff) as u8;
        bytes[1] = (X87_CONTROL_WORD_DEFAULT >> 8) as u8;
        bytes[24] = (MXCSR_DEFAULT & 0xff) as u8;
        bytes[25] = ((MXCSR_DEFAULT >> 8) & 0xff) as u8;
        bytes[28] = (MXCSR_MASK_DEFAULT & 0xff) as u8;
        bytes[29] = ((MXCSR_MASK_DEFAULT >> 8) & 0xff) as u8;
        Self { bytes }
    }
}

static mut INITIAL_STATE: FpuState = FpuState::synthetic_initial();

pub fn init_cpu() {
    unsafe {
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        cr0 &= !(CR0_EM | CR0_TS);
        cr0 |= CR0_MP | CR0_NE;
        asm!("mov cr0, {}", in(reg) cr0, options(nostack, preserves_flags));

        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        cr4 |= CR4_OSFXSR | CR4_OSXMMEXCPT;
        asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));

        asm!("fninit", options(nostack));
        asm!("ldmxcsr [{}]", in(reg) &MXCSR_DEFAULT, options(nostack, preserves_flags));
        asm!(
            "fxsave64 [{}]",
            in(reg) ptr::addr_of_mut!(INITIAL_STATE) as *mut u8,
            options(nostack, preserves_flags)
        );
    }
}

pub fn initial_state() -> FpuState {
    unsafe { ptr::read_volatile(ptr::addr_of!(INITIAL_STATE)) }
}

#[inline]
pub fn save(state: &mut FpuState) {
    unsafe {
        asm!("fxsave64 [{}]", in(reg) state.bytes.as_mut_ptr(), options(nostack, preserves_flags));
    }
}

#[inline]
pub fn restore(state: &FpuState) {
    unsafe {
        asm!("fxrstor64 [{}]", in(reg) state.bytes.as_ptr(), options(nostack, preserves_flags));
    }
}
