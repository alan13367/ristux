const CONFIG_ADDRESS: u16 = 0xcf8;
const CONFIG_DATA: u16 = 0xcfc;

#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub bar0: u32,
}

pub fn scan_all() -> alloc::vec::Vec<PciDevice> {
    let mut devices = alloc::vec::Vec::new();
    for slot in 0..32 {
        let vendor = read_config(0, slot, 0, 0);
        if vendor == 0xffff {
            continue;
        }
        let device = read_config(0, slot, 0, 2);
        let bar0_low = read_config(0, slot, 0, 0x10) as u32;
        let bar0_high = read_config(0, slot, 0, 0x14) as u32;
        let bar0 = bar0_low | (bar0_high << 16);
        devices.push(PciDevice {
            bus: 0,
            slot: slot as u8,
            func: 0,
            vendor_id: vendor,
            device_id: device,
            bar0,
        });
    }
    devices
}

pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    scan_all()
        .into_iter()
        .find(|dev| dev.vendor_id == vendor_id && dev.device_id == device_id)
}

pub fn enable_bus_master(device: &PciDevice) {
    let command = read_config(device.bus, device.slot, device.func, 0x04);
    write_config(
        device.bus,
        device.slot,
        device.func,
        0x04,
        command | 0x0004 | 0x0002 | 0x0001,
    );
}

fn read_config(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let address = 0x8000_0000u32
        | u32::from(bus) << 16
        | u32::from(slot) << 11
        | u32::from(func) << 8
        | u32::from(offset & 0xfc);
    unsafe {
        crate::arch::x86_64::port::outl(CONFIG_ADDRESS, address);
        (crate::arch::x86_64::port::inl(CONFIG_DATA) >> ((offset & 2) * 8)) as u16
    }
}

fn write_config(bus: u8, slot: u8, func: u8, offset: u8, value: u16) {
    let shift = (offset & 2) * 8;
    let address = 0x8000_0000u32
        | u32::from(bus) << 16
        | u32::from(slot) << 11
        | u32::from(func) << 8
        | u32::from(offset & 0xfc);
    unsafe {
        crate::arch::x86_64::port::outl(CONFIG_ADDRESS, address);
        let current = crate::arch::x86_64::port::inl(CONFIG_DATA);
        let mask = !(0xffffu32 << shift);
        let merged = (current & mask) | (u32::from(value) << shift);
        crate::arch::x86_64::port::outl(CONFIG_ADDRESS, address);
        crate::arch::x86_64::port::outl(CONFIG_DATA, merged);
    }
}
