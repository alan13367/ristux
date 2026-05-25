use core::{
    arch::{asm, global_asm},
    mem, ptr,
};

use super::gdt;

const IDT_ENTRIES: usize = 256;
const INTERRUPT_GATE: u16 = 0x8e00;
const USER_INTERRUPT_GATE: u16 = 0xee00;
const SYSCALL_VECTOR: usize = 0x80;

global_asm!(
    r#"
.global syscall_interrupt_stub
syscall_interrupt_stub:
    cld
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov rdi, rsp
    call syscall_interrupt_dispatch
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    iretq
"#
);

unsafe extern "C" {
    fn syscall_interrupt_stub();
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    options: u16,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            options: 0,
            offset_middle: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn set_handler_addr(&mut self, addr: u64, ist: u16) {
        self.set_handler_addr_with_options(addr, ist, INTERRUPT_GATE);
    }

    fn set_user_handler_addr(&mut self, addr: u64) {
        self.set_handler_addr_with_options(addr, 0, USER_INTERRUPT_GATE);
    }

    fn set_handler_addr_with_options(&mut self, addr: u64, ist: u16, options: u16) {
        self.offset_low = addr as u16;
        self.selector = gdt::kernel_code_selector();
        self.options = options | (ist & 0x7);
        self.offset_middle = (addr >> 16) as u16;
        self.offset_high = (addr >> 32) as u32;
        self.reserved = 0;
    }
}

#[repr(C, align(16))]
struct InterruptDescriptorTable {
    entries: [IdtEntry; IDT_ENTRIES],
}

impl InterruptDescriptorTable {
    const fn new() -> Self {
        Self {
            entries: [IdtEntry::missing(); IDT_ENTRIES],
        }
    }

    fn set_handler(&mut self, index: usize, addr: u64) {
        self.entries[index].set_handler_addr(addr, 0);
    }

    fn set_handler_with_ist(&mut self, index: usize, addr: u64, ist: u16) {
        self.entries[index].set_handler_addr(addr, ist);
    }

    fn set_user_handler(&mut self, index: usize, addr: u64) {
        self.entries[index].set_user_handler_addr(addr);
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C)]
pub struct InterruptStackFrame {
    instruction_pointer: u64,
    code_segment: u64,
    cpu_flags: u64,
    stack_pointer: u64,
    stack_segment: u64,
}

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

pub fn init() {
    unsafe {
        let idt = ptr::addr_of_mut!(IDT);
        (*idt).set_handler(0, divide_error_handler as *const () as u64);
        (*idt).set_handler(3, breakpoint_handler as *const () as u64);
        (*idt).set_handler(6, invalid_opcode_handler as *const () as u64);
        (*idt).set_handler_with_ist(
            8,
            double_fault_handler as *const () as u64,
            gdt::double_fault_ist(),
        );
        (*idt).set_handler(12, stack_segment_fault_handler as *const () as u64);
        (*idt).set_handler(13, general_protection_fault_handler as *const () as u64);
        (*idt).set_handler(14, page_fault_handler as *const () as u64);
        (*idt).set_handler(
            super::interrupts::TIMER_VECTOR as usize,
            timer_interrupt_handler as *const () as u64,
        );
        (*idt).set_handler(
            super::interrupts::KEYBOARD_VECTOR as usize,
            keyboard_interrupt_handler as *const () as u64,
        );

        let pointer = DescriptorTablePointer {
            limit: (mem::size_of::<InterruptDescriptorTable>() - 1) as u16,
            base: idt as u64,
        };

        asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
    }

    crate::println!("IDT initialized.");
}

pub fn trigger_breakpoint() {
    unsafe {
        asm!("int3", options(nomem, nostack));
    }
}

pub fn install_syscall_gate() {
    unsafe {
        let idt = ptr::addr_of_mut!(IDT);
        (*idt).set_user_handler(SYSCALL_VECTOR, syscall_interrupt_stub as *const () as u64);
    }
    crate::println!("IDT syscall gate installed at vector {:#x}.", SYSCALL_VECTOR);
}

extern "x86-interrupt" fn divide_error_handler(_stack_frame: InterruptStackFrame) {
    panic!("divide error exception");
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    crate::println!("breakpoint exception");
}

extern "x86-interrupt" fn invalid_opcode_handler(_stack_frame: InterruptStackFrame) {
    panic!("invalid opcode exception");
}

extern "x86-interrupt" fn double_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    panic!("double fault exception, error code {:#x}", error_code);
}

extern "x86-interrupt" fn stack_segment_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!("stack segment fault exception, error code {:#x}", error_code);
}

extern "x86-interrupt" fn general_protection_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "general protection fault exception, error code {:#x}",
        error_code
    );
}

extern "x86-interrupt" fn page_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    let fault_addr: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) fault_addr, options(nomem, nostack, preserves_flags));
    }
    panic!(
        "page fault exception at {:#x}, error code {:#x}",
        fault_addr, error_code
    );
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    super::interrupts::timer_tick();
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    super::interrupts::keyboard_interrupt();
}
