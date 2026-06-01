pub mod framebuffer;
pub mod keyboard;
pub mod pci;
pub mod serial;
pub mod vga;
pub mod virtio_blk;
pub mod virtio_mmio;
pub mod virtio_net;
pub mod virtio_queue;

pub struct DriverInfo {
    pub name: &'static str,
    pub kind: &'static str,
}

const REGISTERED_DRIVERS: &[DriverInfo] = &[
    DriverInfo {
        name: "serial-com1",
        kind: "text-output",
    },
    DriverInfo {
        name: "vga-text",
        kind: "text-output",
    },
    DriverInfo {
        name: "ps2-keyboard",
        kind: "input",
    },
    DriverInfo {
        name: "framebuffer",
        kind: "graphics-output",
    },
    DriverInfo {
        name: "virtio-net",
        kind: "network",
    },
    DriverInfo {
        name: "virtio-blk",
        kind: "block",
    },
];

pub fn init() {
    serial::init();
    vga::clear_screen();
    vga::init_cursor();
}

pub fn registered_drivers() -> &'static [DriverInfo] {
    REGISTERED_DRIVERS
}
