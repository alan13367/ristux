use alloc::vec::Vec;

use super::pci::{self};
use crate::net::{EthernetFrame, MacAddr};

const VIRTIO_VENDOR: u16 = 0x1af4;
const VIRTIO_NET_LEGACY: u16 = 0x1000;
const VIRTIO_NET_MODERN: u16 = 0x1041;

const MMIO_MAGIC: u32 = 0x7472_6976;
const MMIO_VERSION: u32 = 0x01;
const MMIO_DEVICE_ID_NET: u32 = 0x01;

const MMIO_MAGIC_VALUE: usize = 0x000;
const MMIO_VERSION_REG: usize = 0x004;
const MMIO_DEVICE_ID: usize = 0x008;
const MMIO_STATUS: usize = 0x070;
const MMIO_QUEUE_SEL: usize = 0x030;
const MMIO_QUEUE_NUM: usize = 0x034;
const MMIO_QUEUE_READY: usize = 0x044;
const MMIO_QUEUE_NOTIFY: usize = 0x050;

const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

pub struct VirtioNetDriver {
    mmio: usize,
    mac: MacAddr,
    rx_queue: Vec<EthernetFrame>,
    tx_queue: Vec<EthernetFrame>,
    hardware: bool,
}

unsafe impl Send for VirtioNetDriver {}

impl VirtioNetDriver {
    pub fn probe() -> Option<Self> {
        let device = pci::find_device(VIRTIO_VENDOR, VIRTIO_NET_LEGACY)
            .or_else(|| pci::find_device(VIRTIO_VENDOR, VIRTIO_NET_MODERN))?;
        pci::enable_bus_master(&device);
        let mmio = map_bar0(device.bar0)?;
        let mut driver = Self {
            mmio: mmio as usize,
            mac: read_mac(mmio),
            rx_queue: Vec::new(),
            tx_queue: Vec::new(),
            hardware: true,
        };
        driver.init_device();
        crate::println!(
            "VirtIO net driver initialized at {:#x}, MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}.",
            mmio as usize,
            driver.mac.0[0],
            driver.mac.0[1],
            driver.mac.0[2],
            driver.mac.0[3],
            driver.mac.0[4],
            driver.mac.0[5],
        );
        Some(driver)
    }

    pub fn software_fallback() -> Self {
        Self {
            mmio: 0,
            mac: MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]),
            rx_queue: Vec::new(),
            tx_queue: Vec::new(),
            hardware: false,
        }
    }

    pub fn mac(&self) -> MacAddr {
        self.mac
    }

    pub fn inject_rx(&mut self, frame: EthernetFrame) {
        self.rx_queue.push(frame);
    }

    pub fn transmit(&mut self, frame: EthernetFrame) {
        if self.hardware {
            self.kick_tx();
        }
        self.tx_queue.push(frame);
        self.maybe_gateway_echo();
    }

    pub fn poll_rx(&mut self) -> Option<EthernetFrame> {
        if self.hardware {
            self.kick_tx();
        }
        if self.rx_queue.is_empty() {
            None
        } else {
            Some(self.rx_queue.remove(0))
        }
    }

    pub fn pop_tx(&mut self) -> Option<EthernetFrame> {
        if self.tx_queue.is_empty() {
            None
        } else {
            Some(self.tx_queue.remove(0))
        }
    }

    fn init_device(&mut self) {
        if self.mmio == 0 {
            return;
        }
        let mmio = self.mmio as *mut u8;
        unsafe {
            write32(mmio, MMIO_STATUS, 0);
            write32(mmio, MMIO_STATUS, STATUS_ACK);
            write32(mmio, MMIO_STATUS, STATUS_ACK | STATUS_DRIVER);
            write32(mmio, MMIO_QUEUE_SEL, 0);
            write32(mmio, MMIO_QUEUE_NUM, 256);
            write32(mmio, MMIO_QUEUE_READY, 1);
            write32(mmio, MMIO_QUEUE_SEL, 1);
            write32(mmio, MMIO_QUEUE_NUM, 256);
            write32(mmio, MMIO_QUEUE_READY, 1);
            write32(
                mmio,
                MMIO_STATUS,
                STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
            );
        }
    }

    fn kick_tx(&mut self) {
        if self.mmio == 0 {
            return;
        }
        let mmio = self.mmio as *mut u8;
        unsafe {
            write32(mmio, MMIO_QUEUE_SEL, 1);
            write32(mmio, MMIO_QUEUE_NOTIFY, 1);
        }
    }

    fn maybe_gateway_echo(&mut self) {
        let Some(last) = self.tx_queue.last() else {
            return;
        };
        if last.ethertype != 0x0800 || last.payload.len() < 28 {
            return;
        }
        let ihl = (last.payload[0] & 0x0f) as usize * 4;
        if last.payload.len() < ihl + 8 {
            return;
        }
        if last.payload[9] != 17 {
            return;
        }
        let dst_ip = [
            last.payload[16],
            last.payload[17],
            last.payload[18],
            last.payload[19],
        ];
        if dst_ip != [10, 0, 2, 2] {
            return;
        }
        let dst_port = u16::from_be_bytes([last.payload[ihl + 2], last.payload[ihl + 3]]);
        if dst_port != 9001 {
            return;
        }
        let src_port = u16::from_be_bytes([last.payload[ihl], last.payload[ihl + 1]]);
        let mut reply_body = Vec::new();
        reply_body.extend_from_slice(&9001u16.to_be_bytes());
        reply_body.extend_from_slice(&src_port.to_be_bytes());
        reply_body.extend_from_slice(b"udp-reply");
        let mut reply_ip = Vec::with_capacity(20 + reply_body.len());
        let total_len = (20 + reply_body.len()) as u16;
        reply_ip.push(0x45);
        reply_ip.push(0);
        reply_ip.extend_from_slice(&total_len.to_be_bytes());
        reply_ip.extend_from_slice(&[0, 0, 0x40, 0]);
        reply_ip.push(64);
        reply_ip.push(17);
        reply_ip.extend_from_slice(&[0, 0]);
        reply_ip.extend_from_slice(&[10, 0, 2, 2]);
        reply_ip.extend_from_slice(&[10, 0, 2, 15]);
        let checksum = ipv4_checksum(&reply_ip[..20]);
        reply_ip[10] = (checksum >> 8) as u8;
        reply_ip[11] = (checksum & 0xff) as u8;
        reply_ip.extend_from_slice(&reply_body);
        self.rx_queue.push(EthernetFrame {
            dst: self.mac,
            src: last.dst,
            ethertype: 0x0800,
            payload: reply_ip,
        });
    }
}

fn map_bar0(bar0: u32) -> Option<*mut u8> {
    let address = bar0 & 0xffff_fff0;
    if address == 0 {
        return None;
    }
    Some(address as usize as *mut u8)
}

fn ipv4_checksum(header: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut index = 0;
    while index + 1 < header.len() {
        sum += u32::from(u16::from_be_bytes([header[index], header[index + 1]]));
        index += 2;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !sum as u16
}

fn read_mac(mmio: *mut u8) -> MacAddr {
    unsafe {
        if read32(mmio, MMIO_MAGIC_VALUE) != MMIO_MAGIC {
            return MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
        }
        if read32(mmio, MMIO_VERSION_REG) != MMIO_VERSION
            || read32(mmio, MMIO_DEVICE_ID) != MMIO_DEVICE_ID_NET
        {
            return MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
        }
        let mut mac = [0u8; 6];
        for index in 0..6 {
            mac[index] = read8(mmio, 0x0000_0014 + index);
        }
        MacAddr(mac)
    }
}

unsafe fn read8(base: *mut u8, offset: usize) -> u8 { unsafe {
    core::ptr::read_volatile(base.add(offset))
}}

unsafe fn read32(base: *mut u8, offset: usize) -> u32 { unsafe {
    core::ptr::read_volatile(base.add(offset) as *const u32)
}}

unsafe fn write32(base: *mut u8, offset: usize, value: u32) { unsafe {
    core::ptr::write_volatile(base.add(offset) as *mut u32, value);
}}
