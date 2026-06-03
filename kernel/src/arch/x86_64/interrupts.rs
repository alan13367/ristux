use core::sync::atomic::{AtomicU64, Ordering};

use crate::config;

use super::{instructions, port};

pub const PIC_1_OFFSET: u8 = 32;
pub const TIMER_VECTOR: u8 = PIC_1_OFFSET;
pub const KEYBOARD_VECTOR: u8 = PIC_1_OFFSET + 1;

const PIC_1_COMMAND: u16 = 0x20;
const PIC_1_DATA: u16 = 0x21;
const PIC_2_COMMAND: u16 = 0xa0;
const PIC_2_DATA: u16 = 0xa1;
const PIC_EOI: u8 = 0x20;

const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL_0: u16 = 0x40;
const PIT_BASE_FREQUENCY: u32 = 1_193_182;

static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    instructions::disable_interrupts();
    unsafe {
        remap_pic();
        init_pit(config::PIT_TARGET_HZ);
        unmask_irq(0);
        unmask_irq(1);
    }

    instructions::enable_interrupts();
    crate::println!("PIC remapped; PIT and keyboard interrupts enabled.");
}

pub fn timer_tick() {
    let tick = TIMER_TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    crate::task::on_timer_tick(tick);
    crate::process::wake_expired_io_waiters(crate::time::uptime_millis());
    crate::tty::poll_key_repeats();
    crate::net::poll_devices();
    if tick == 1
        || (config::LOG_TIMER_EVERY_TICKS != 0 && tick % config::LOG_TIMER_EVERY_TICKS == 0)
    {
        crate::serial_println!("timer tick {}", tick);
    }
    unsafe {
        end_of_interrupt(0);
    }
}

pub fn keyboard_interrupt() {
    let scancode = unsafe { port::inb(0x60) };
    crate::entropy::mix_interrupt_sample(scancode as u64);
    crate::drivers::keyboard::push_scancode(scancode);
    crate::tty::input_scancode(scancode);
    crate::process::wake_io_waiters();
    crate::serial_println!("keyboard scancode {:#04x}", scancode);
    unsafe {
        end_of_interrupt(1);
    }
}

pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

unsafe fn remap_pic() {
    let pic1_mask = unsafe { port::inb(PIC_1_DATA) };
    let pic2_mask = unsafe { port::inb(PIC_2_DATA) };

    unsafe {
        port::outb(PIC_1_COMMAND, 0x11);
        io_wait();
        port::outb(PIC_2_COMMAND, 0x11);
        io_wait();

        port::outb(PIC_1_DATA, PIC_1_OFFSET);
        io_wait();
        port::outb(PIC_2_DATA, PIC_1_OFFSET + 8);
        io_wait();

        port::outb(PIC_1_DATA, 4);
        io_wait();
        port::outb(PIC_2_DATA, 2);
        io_wait();

        port::outb(PIC_1_DATA, 0x01);
        io_wait();
        port::outb(PIC_2_DATA, 0x01);
        io_wait();

        port::outb(PIC_1_DATA, pic1_mask);
        port::outb(PIC_2_DATA, pic2_mask);
    }
}

unsafe fn init_pit(hz: u32) {
    let divisor = (PIT_BASE_FREQUENCY / hz) as u16;
    unsafe {
        port::outb(PIT_COMMAND, 0x36);
        port::outb(PIT_CHANNEL_0, divisor as u8);
        port::outb(PIT_CHANNEL_0, (divisor >> 8) as u8);
    }
}

unsafe fn unmask_irq(irq: u8) {
    let port_addr = if irq < 8 { PIC_1_DATA } else { PIC_2_DATA };
    let irq_line = if irq < 8 { irq } else { irq - 8 };
    let mask = unsafe { port::inb(port_addr) } & !(1 << irq_line);
    unsafe {
        port::outb(port_addr, mask);
    }
}

unsafe fn end_of_interrupt(irq: u8) {
    unsafe {
        if irq >= 8 {
            port::outb(PIC_2_COMMAND, PIC_EOI);
        }
        port::outb(PIC_1_COMMAND, PIC_EOI);
    }
}

unsafe fn io_wait() {
    unsafe {
        port::outb(0x80, 0);
    }
}
