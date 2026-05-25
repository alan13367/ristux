use core::ptr;

pub const MMIO_MAGIC: u32 = 0x7472_6976;
pub const MMIO_VERSION: u32 = 0x01;

pub const MMIO_MAGIC_VALUE: usize = 0x000;
pub const MMIO_VERSION_REG: usize = 0x004;
pub const MMIO_DEVICE_ID: usize = 0x008;
pub const MMIO_QUEUE_SEL: usize = 0x030;
pub const MMIO_QUEUE_NUM: usize = 0x034;
pub const MMIO_QUEUE_READY: usize = 0x044;
pub const MMIO_QUEUE_NOTIFY: usize = 0x050;
pub const MMIO_INTERRUPT_STATUS: usize = 0x060;
pub const MMIO_INTERRUPT_ACK: usize = 0x064;
pub const MMIO_STATUS: usize = 0x070;
pub const MMIO_QUEUE_DESC_LOW: usize = 0x080;
pub const MMIO_QUEUE_DESC_HIGH: usize = 0x084;
pub const MMIO_QUEUE_DRIVER_LOW: usize = 0x090;
pub const MMIO_QUEUE_DRIVER_HIGH: usize = 0x094;
pub const MMIO_QUEUE_DEVICE_LOW: usize = 0x0a0;
pub const MMIO_QUEUE_DEVICE_HIGH: usize = 0x0a4;

pub const STATUS_ACK: u32 = 1;
pub const STATUS_DRIVER: u32 = 2;
pub const STATUS_DRIVER_OK: u32 = 4;
pub const STATUS_FEATURES_OK: u32 = 8;

pub fn map_bar0(bar0: u32) -> Option<*mut u8> {
    let address = bar0 & 0xffff_fff0;
    if address == 0 {
        return None;
    }
    Some(address as usize as *mut u8)
}

pub unsafe fn read8(base: *mut u8, offset: usize) -> u8 {
    unsafe { ptr::read_volatile(base.add(offset)) }
}

pub unsafe fn read32(base: *mut u8, offset: usize) -> u32 {
    unsafe { ptr::read_volatile(base.add(offset) as *const u32) }
}

pub unsafe fn write32(base: *mut u8, offset: usize, value: u32) {
    unsafe {
        ptr::write_volatile(base.add(offset) as *mut u32, value);
    }
}

pub fn init_device(mmio: *mut u8, queue_count: u32) {
    unsafe {
        write32(mmio, MMIO_STATUS, 0);
        write32(mmio, MMIO_STATUS, STATUS_ACK);
        write32(mmio, MMIO_STATUS, STATUS_ACK | STATUS_DRIVER);
        for queue in 0..queue_count {
            write32(mmio, MMIO_QUEUE_SEL, queue);
            write32(mmio, MMIO_QUEUE_NUM, 256);
            write32(mmio, MMIO_QUEUE_READY, 1);
        }
        write32(
            mmio,
            MMIO_STATUS,
            STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );
    }
}

pub fn setup_queue(
    mmio: *mut u8,
    queue_index: u32,
    desc: u64,
    driver: u64,
    device: u64,
    size: u32,
) {
    unsafe {
        write32(mmio, MMIO_QUEUE_SEL, queue_index);
        write32(mmio, MMIO_QUEUE_NUM, size);
        write32(mmio, MMIO_QUEUE_DESC_LOW, desc as u32);
        write32(mmio, MMIO_QUEUE_DESC_HIGH, (desc >> 32) as u32);
        write32(mmio, MMIO_QUEUE_DRIVER_LOW, driver as u32);
        write32(mmio, MMIO_QUEUE_DRIVER_HIGH, (driver >> 32) as u32);
        write32(mmio, MMIO_QUEUE_DEVICE_LOW, device as u32);
        write32(mmio, MMIO_QUEUE_DEVICE_HIGH, (device >> 32) as u32);
        write32(mmio, MMIO_QUEUE_READY, 1);
    }
}

pub fn interrupt_status(mmio: *mut u8) -> u32 {
    unsafe { read32(mmio, MMIO_INTERRUPT_STATUS) }
}

pub fn ack_interrupt(mmio: *mut u8, value: u32) {
    unsafe {
        write32(mmio, MMIO_INTERRUPT_ACK, value);
    }
}

pub fn kick_queue(mmio: *mut u8, queue_index: u32) {
    unsafe {
        write32(mmio, MMIO_QUEUE_SEL, queue_index);
        write32(mmio, MMIO_QUEUE_NOTIFY, queue_index);
    }
}

pub fn device_id(mmio: *mut u8) -> u32 {
    unsafe { read32(mmio, MMIO_DEVICE_ID) }
}

pub fn validate(mmio: *mut u8, expected_device: u32) -> bool {
    unsafe {
        read32(mmio, MMIO_MAGIC_VALUE) == MMIO_MAGIC
            && read32(mmio, MMIO_VERSION_REG) == MMIO_VERSION
            && read32(mmio, MMIO_DEVICE_ID) == expected_device
    }
}
