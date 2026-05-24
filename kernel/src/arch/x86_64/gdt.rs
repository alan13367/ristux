use core::{arch::asm, mem, ptr};

const KERNEL_CODE_SELECTOR: u16 = 0x08;
const KERNEL_DATA_SELECTOR: u16 = 0x10;
const TSS_SELECTOR: u16 = 0x18;
const DOUBLE_FAULT_IST_INDEX: usize = 0;
const DOUBLE_FAULT_STACK_SIZE: usize = 4096 * 5;

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct TaskStateSegment {
    reserved_1: u32,
    privilege_stack_table: [u64; 3],
    reserved_2: u64,
    interrupt_stack_table: [u64; 7],
    reserved_3: u64,
    reserved_4: u16,
    io_map_base: u16,
}

impl TaskStateSegment {
    const fn new() -> Self {
        Self {
            reserved_1: 0,
            privilege_stack_table: [0; 3],
            reserved_2: 0,
            interrupt_stack_table: [0; 7],
            reserved_3: 0,
            reserved_4: 0,
            io_map_base: mem::size_of::<TaskStateSegment>() as u16,
        }
    }
}

#[repr(C, align(16))]
struct Stack([u8; DOUBLE_FAULT_STACK_SIZE]);

static DOUBLE_FAULT_STACK: Stack = Stack([0; DOUBLE_FAULT_STACK_SIZE]);
static mut TSS: TaskStateSegment = TaskStateSegment::new();
static mut GDT: [u64; 5] = [0; 5];

pub fn init() {
    unsafe {
        let tss = ptr::addr_of_mut!(TSS);
        (*tss).interrupt_stack_table[DOUBLE_FAULT_IST_INDEX] =
            ptr::addr_of!(DOUBLE_FAULT_STACK.0) as u64 + DOUBLE_FAULT_STACK_SIZE as u64;

        let gdt = ptr::addr_of_mut!(GDT);
        (*gdt)[0] = 0;
        (*gdt)[1] = 0x00af_9a00_0000_ffff;
        (*gdt)[2] = 0x00af_9200_0000_ffff;

        let (tss_low, tss_high) = tss_descriptor(tss as u64);
        (*gdt)[3] = tss_low;
        (*gdt)[4] = tss_high;

        let pointer = DescriptorTablePointer {
            limit: (mem::size_of::<[u64; 5]>() - 1) as u16,
            base: gdt as u64,
        };

        asm!("lgdt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
        asm!(
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            in("ax") KERNEL_DATA_SELECTOR,
            options(nostack, preserves_flags)
        );
        asm!("ltr ax", in("ax") TSS_SELECTOR, options(nostack, preserves_flags));
    }

    crate::println!("GDT and TSS initialized.");
}

pub const fn double_fault_ist() -> u16 {
    (DOUBLE_FAULT_IST_INDEX + 1) as u16
}

pub const fn kernel_code_selector() -> u16 {
    KERNEL_CODE_SELECTOR
}

fn tss_descriptor(base: u64) -> (u64, u64) {
    let limit = (mem::size_of::<TaskStateSegment>() - 1) as u64;
    let low = (limit & 0xffff)
        | ((base & 0x00ff_ffff) << 16)
        | (0x89 << 40)
        | (((limit >> 16) & 0x0f) << 48)
        | (((base >> 24) & 0xff) << 56);
    let high = base >> 32;

    (low, high)
}

