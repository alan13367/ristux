use crate::arch::x86_64::{instructions, port};

const REBOOT_MAGIC1: u32 = 0xfee1_dead;
const REBOOT_MAGIC2: u32 = 672_274_793;
const REBOOT_MAGIC2A: u32 = 850_722_78;
const REBOOT_MAGIC2B: u32 = 369_367_448;
const REBOOT_MAGIC2C: u32 = 537_993_216;

pub const LINUX_REBOOT_CMD_RESTART: u32 = 0x0123_4567;
pub const LINUX_REBOOT_CMD_HALT: u32 = 0xcdef_0123;
pub const LINUX_REBOOT_CMD_POWER_OFF: u32 = 0x4321_fedc;

pub fn reboot_syscall(magic1: u32, magic2: u32, cmd: u32) -> Result<(), ()> {
    if magic1 != REBOOT_MAGIC1
        || !matches!(
            magic2,
            REBOOT_MAGIC2 | REBOOT_MAGIC2A | REBOOT_MAGIC2B | REBOOT_MAGIC2C
        )
    {
        return Err(());
    }

    match cmd {
        LINUX_REBOOT_CMD_POWER_OFF | LINUX_REBOOT_CMD_HALT => poweroff(),
        LINUX_REBOOT_CMD_RESTART => restart(),
        _ => Err(()),
    }
}

pub fn poweroff() -> Result<(), ()> {
    crate::println!("Powering off.");
    unsafe {
        port::outw(0x604, 0x2000);
        port::outw(0xb004, 0x2000);
        port::outw(0x4004, 0x3400);
    }
    halt_forever()
}

pub fn restart() -> Result<(), ()> {
    crate::println!("Restarting.");
    unsafe {
        port::outb(0xcf9, 0x02);
        port::outb(0xcf9, 0x06);
    }
    halt_forever()
}

fn halt_forever() -> ! {
    instructions::disable_interrupts();
    loop {
        instructions::halt();
    }
}
