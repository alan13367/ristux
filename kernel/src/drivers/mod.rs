pub mod keyboard;
pub mod serial;
pub mod vga;

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
];

pub fn init() {
    serial::init();
    vga::clear_screen();
}

pub fn registered_drivers() -> &'static [DriverInfo] {
    REGISTERED_DRIVERS
}
