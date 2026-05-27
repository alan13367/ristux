use core::{arch::asm, mem, ptr};

const KERNEL_CODE_SELECTOR: u16 = 0x08;
const KERNEL_DATA_SELECTOR: u16 = 0x10;
const TSS_SELECTOR: u16 = 0x18;
const USER_DATA_SELECTOR: u16 = 0x28 | 3;
const USER_CODE_SELECTOR: u16 = 0x30 | 3;
const DOUBLE_FAULT_IST_INDEX: usize = 0;
const DOUBLE_FAULT_STACK_SIZE: usize = 4096 * 5;
const RING0_STACK_SIZE: usize = 4096 * 5;
const MAX_CPUS: usize = 8;

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

#[repr(C, align(16))]
struct Ring0Stack([u8; RING0_STACK_SIZE]);

static DOUBLE_FAULT_STACK: Stack = Stack([0; DOUBLE_FAULT_STACK_SIZE]);
static mut CPU_RING0_STACKS: [Ring0Stack; MAX_CPUS] =
    [const { Ring0Stack([0; RING0_STACK_SIZE]) }; MAX_CPUS];
static mut CPU_TSS: [TaskStateSegment; MAX_CPUS] = [const { TaskStateSegment::new() }; MAX_CPUS];
static mut CPU_GDT: [[u64; 7]; MAX_CPUS] = [[0; 7]; MAX_CPUS];

pub fn init() {
    init_cpu(0);
    crate::println!("GDT and TSS initialized.");
}

pub fn init_ap(cpu_id: usize) {
    if cpu_id >= MAX_CPUS {
        return;
    }
    init_cpu(cpu_id);
}

fn init_cpu(cpu_id: usize) {
    unsafe {
        let tss = ptr::addr_of_mut!(CPU_TSS[cpu_id]);
        (*tss).interrupt_stack_table[DOUBLE_FAULT_IST_INDEX] =
            ptr::addr_of!(DOUBLE_FAULT_STACK.0) as u64 + DOUBLE_FAULT_STACK_SIZE as u64;
        (*tss).privilege_stack_table[0] =
            ptr::addr_of!(CPU_RING0_STACKS[cpu_id].0) as u64 + RING0_STACK_SIZE as u64;

        let gdt = ptr::addr_of_mut!(CPU_GDT[cpu_id]);
        (*gdt)[0] = 0;
        (*gdt)[1] = 0x00af_9a00_0000_ffff;
        (*gdt)[2] = 0x00af_9200_0000_ffff;

        let (tss_low, tss_high) = tss_descriptor(tss as u64);
        (*gdt)[3] = tss_low;
        (*gdt)[4] = tss_high;
        (*gdt)[5] = 0x00af_f200_0000_ffff;
        (*gdt)[6] = 0x00af_fa00_0000_ffff;

        let pointer = DescriptorTablePointer {
            limit: (mem::size_of::<[u64; 7]>() - 1) as u16,
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
    super::fpu::init_cpu();
}

pub const fn double_fault_ist() -> u16 {
    (DOUBLE_FAULT_IST_INDEX + 1) as u16
}

pub const fn kernel_code_selector() -> u16 {
    KERNEL_CODE_SELECTOR
}

pub const fn user_data_selector() -> u16 {
    USER_DATA_SELECTOR
}

pub const fn user_code_selector() -> u16 {
    USER_CODE_SELECTOR
}

pub fn init_user_segments() {
    crate::println!(
        "User mode segments configured: code {:#x}, data {:#x}, rsp0 {:#x}",
        user_code_selector(),
        user_data_selector(),
        kernel_stack_top(0)
    );
}

pub fn kernel_stack_top(cpu_id: usize) -> u64 {
    let id = cpu_id.min(MAX_CPUS - 1);
    unsafe { ptr::addr_of!(CPU_RING0_STACKS[id].0) as u64 + RING0_STACK_SIZE as u64 }
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
