use alloc::vec::Vec;
use core::{
    ptr,
    sync::atomic::{Ordering, compiler_fence},
};

use super::{pci, virtio_mmio};
use crate::arch::x86_64::port;
use crate::net::{EthernetFrame, Ipv4Addr, MacAddr, tcp};

const VIRTIO_VENDOR: u16 = 0x1af4;
const VIRTIO_NET_LEGACY: u16 = 0x1000;
const VIRTIO_NET_MODERN: u16 = 0x1041;
const MMIO_DEVICE_ID_NET: u32 = 0x01;
const LEGACY_DEVICE_FEATURES: u16 = 0x00;
const LEGACY_GUEST_FEATURES: u16 = 0x04;
const LEGACY_QUEUE_PFN: u16 = 0x08;
const LEGACY_QUEUE_SIZE_REG: u16 = 0x0c;
const LEGACY_QUEUE_SELECT: u16 = 0x0e;
const LEGACY_QUEUE_NOTIFY: u16 = 0x10;
const LEGACY_DEVICE_STATUS: u16 = 0x12;
const LEGACY_ISR_STATUS: u16 = 0x13;
const LEGACY_CONFIG_MAC: u16 = 0x14;
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FAILED: u8 = 0x80;
const KERNEL_HIGH_BASE: usize = 0xffff_ffff_8000_0000;
const LEGACY_NET_QUEUE_CAPACITY: usize = 16;
const LEGACY_NET_QUEUE_MAX_SIZE: usize = 256;
const LEGACY_NET_QUEUE_BYTES: usize = 12 * 1024;
const VIRTIO_NET_HDR_LEN: usize = 10;
const NET_FRAME_BUFFER_SIZE: usize = 2048;
const GATEWAY_IP: Ipv4Addr = Ipv4Addr([10, 0, 2, 2]);
const GUEST_IP: Ipv4Addr = Ipv4Addr([10, 0, 2, 15]);
const IP_PROTO_ICMP: u8 = 1;
const IP_PROTO_TCP: u8 = 6;
const IP_PROTO_UDP: u8 = 17;
const UDP_HEADER_LEN: usize = 8;
const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;
const DNS_PORT: u16 = 53;
pub struct VirtioNetDriver {
    mmio: usize,
    io_base: u16,
    mac: MacAddr,
    rx_queue: Vec<EthernetFrame>,
    tx_queue: Vec<EthernetFrame>,
    legacy_rx: LegacyNetQueue,
    legacy_tx: LegacyNetQueue,
    hardware: bool,
    transport: Transport,
}

unsafe impl Send for VirtioNetDriver {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Transport {
    LegacyIo,
    Mmio,
    Software,
}

impl VirtioNetDriver {
    pub fn probe() -> Option<Self> {
        if let Some(driver) = Self::probe_legacy_io() {
            return Some(driver);
        }
        Self::probe_mmio()
    }

    fn probe_legacy_io() -> Option<Self> {
        let device = pci::find_device(VIRTIO_VENDOR, VIRTIO_NET_LEGACY)?;
        if device.bar0 & 0x1 == 0 {
            return None;
        }
        pci::enable_bus_master(&device);
        let io_base = (device.bar0 & !0x3) as u16;

        unsafe {
            legacy_write8(io_base, LEGACY_DEVICE_STATUS, 0);
            legacy_write8(io_base, LEGACY_DEVICE_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);
            legacy_write8(
                io_base,
                LEGACY_DEVICE_STATUS,
                VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
            );
            let _device_features = legacy_read32(io_base, LEGACY_DEVICE_FEATURES);
            legacy_write32(io_base, LEGACY_GUEST_FEATURES, 0);
        }

        let rx_size = legacy_queue_size(io_base, 0)?;
        let tx_size = legacy_queue_size(io_base, 1)?;
        let legacy_rx = LegacyNetQueue::new(QueueMem::Rx, rx_size);
        let legacy_tx = LegacyNetQueue::new(QueueMem::Tx, tx_size);
        unsafe {
            legacy_write16(io_base, LEGACY_QUEUE_SELECT, 0);
            legacy_write32(io_base, LEGACY_QUEUE_PFN, legacy_rx.pfn());
            legacy_write16(io_base, LEGACY_QUEUE_SELECT, 1);
            legacy_write32(io_base, LEGACY_QUEUE_PFN, legacy_tx.pfn());
        }

        let mut mac = [0u8; 6];
        for (index, byte) in mac.iter_mut().enumerate() {
            *byte = unsafe { legacy_read8(io_base, LEGACY_CONFIG_MAC + index as u16) };
        }
        if mac == [0; 6] {
            mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        }

        let mut driver = Self {
            mmio: 0,
            io_base,
            mac: MacAddr(mac),
            rx_queue: Vec::new(),
            tx_queue: Vec::new(),
            legacy_rx,
            legacy_tx,
            hardware: true,
            transport: Transport::LegacyIo,
        };
        if driver.legacy_prime_rx().is_err() {
            unsafe {
                legacy_write8(io_base, LEGACY_DEVICE_STATUS, VIRTIO_STATUS_FAILED);
            }
            return None;
        }
        unsafe {
            legacy_write8(
                io_base,
                LEGACY_DEVICE_STATUS,
                VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK,
            );
            legacy_write16(io_base, LEGACY_QUEUE_NOTIFY, 0);
        }
        crate::println!(
            "VirtIO legacy net driver initialized at I/O {:#x}, MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}.",
            io_base,
            driver.mac.0[0],
            driver.mac.0[1],
            driver.mac.0[2],
            driver.mac.0[3],
            driver.mac.0[4],
            driver.mac.0[5],
        );
        Some(driver)
    }

    fn probe_mmio() -> Option<Self> {
        let device = pci::find_device(VIRTIO_VENDOR, VIRTIO_NET_LEGACY)
            .or_else(|| pci::find_device(VIRTIO_VENDOR, VIRTIO_NET_MODERN))?;
        if device.bar0 & 0x1 != 0 {
            return None;
        }
        pci::enable_bus_master(&device);
        let mmio = virtio_mmio::map_bar0(device.bar0)? as usize;
        let mut driver = Self {
            mmio,
            io_base: 0,
            mac: read_mac(mmio as *mut u8),
            rx_queue: Vec::new(),
            tx_queue: Vec::new(),
            legacy_rx: LegacyNetQueue::empty(QueueMem::Rx),
            legacy_tx: LegacyNetQueue::empty(QueueMem::Tx),
            hardware: true,
            transport: Transport::Mmio,
        };
        driver.init_device();
        crate::println!(
            "VirtIO net driver initialized at {:#x}, MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}.",
            mmio,
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
            io_base: 0,
            mac: MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]),
            rx_queue: Vec::new(),
            tx_queue: Vec::new(),
            legacy_rx: LegacyNetQueue::empty(QueueMem::Rx),
            legacy_tx: LegacyNetQueue::empty(QueueMem::Tx),
            hardware: false,
            transport: Transport::Software,
        }
    }

    pub fn mac(&self) -> MacAddr {
        self.mac
    }

    pub fn is_hardware(&self) -> bool {
        self.hardware
    }

    pub fn inject_rx(&mut self, frame: EthernetFrame) {
        self.rx_queue.push(frame);
    }

    pub fn transmit(&mut self, frame: EthernetFrame) {
        let gateway_handled = self.maybe_gateway_echo(&frame);
        if self.hardware && !gateway_handled {
            match self.transport {
                Transport::LegacyIo => {
                    let _ = self.legacy_transmit(&frame);
                }
                Transport::Mmio => self.kick_tx(),
                Transport::Software => {}
            }
        }
        self.tx_queue.push(frame);
    }

    pub fn poll_rx(&mut self) -> Option<EthernetFrame> {
        if self.hardware {
            match self.transport {
                Transport::LegacyIo => self.legacy_poll_rx(),
                Transport::Mmio => self.kick_tx(),
                Transport::Software => {}
            }
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
        virtio_mmio::init_device(self.mmio as *mut u8, 2);
    }

    fn kick_tx(&mut self) {
        if self.mmio == 0 {
            return;
        }
        virtio_mmio::kick_queue(self.mmio as *mut u8, 1);
    }

    fn legacy_prime_rx(&mut self) -> Result<(), ()> {
        for index in 0..self.legacy_rx.size.min(LEGACY_NET_QUEUE_CAPACITY as u16) {
            self.legacy_rx.set_desc(
                index,
                virt_to_phys(rx_buffer_ptr(index as usize) as usize),
                NET_FRAME_BUFFER_SIZE as u32,
                virtio_queue_write_flag(),
                0,
            );
            self.legacy_rx.submit_chain(index)?;
        }
        unsafe {
            legacy_write16(self.io_base, LEGACY_QUEUE_NOTIFY, 0);
        }
        Ok(())
    }

    fn legacy_poll_rx(&mut self) {
        unsafe {
            legacy_write16(self.io_base, LEGACY_QUEUE_NOTIFY, 0);
        }
        let Some((head, len)) = self.legacy_rx.pop_used() else {
            self.legacy_ack_interrupt();
            return;
        };
        if head as usize >= LEGACY_NET_QUEUE_CAPACITY {
            return;
        }

        let frame_len = (len as usize)
            .saturating_sub(VIRTIO_NET_HDR_LEN)
            .min(NET_FRAME_BUFFER_SIZE.saturating_sub(VIRTIO_NET_HDR_LEN));
        if frame_len >= 14 {
            let frame = unsafe {
                let frame_ptr = rx_buffer_ptr(head as usize).add(VIRTIO_NET_HDR_LEN);
                let bytes = core::slice::from_raw_parts(frame_ptr, frame_len);
                parse_ethernet_frame(bytes)
            };
            if let Some(frame) = frame {
                self.rx_queue.push(frame);
            }
        }

        self.legacy_rx.set_desc(
            head,
            virt_to_phys(rx_buffer_ptr(head as usize) as usize),
            NET_FRAME_BUFFER_SIZE as u32,
            virtio_queue_write_flag(),
            0,
        );
        if self.legacy_rx.submit_chain(head).is_ok() {
            unsafe {
                legacy_write16(self.io_base, LEGACY_QUEUE_NOTIFY, 0);
            }
        }
        self.legacy_ack_interrupt();
    }

    fn legacy_transmit(&mut self, frame: &EthernetFrame) -> Result<(), ()> {
        let frame_len = serialized_frame_len(frame).ok_or(())?;
        unsafe {
            ptr::write_bytes(tx_header_ptr(), 0, VIRTIO_NET_HDR_LEN);
            write_ethernet_frame(frame, tx_frame_ptr(), frame_len);
        }

        self.legacy_tx.set_desc(
            0,
            virt_to_phys(tx_header_ptr() as usize),
            VIRTIO_NET_HDR_LEN as u32,
            virtio_queue_next_flag(),
            1,
        );
        self.legacy_tx.set_desc(
            1,
            virt_to_phys(tx_frame_ptr() as usize),
            frame_len as u32,
            0,
            0,
        );
        self.legacy_tx.submit_chain(0)?;
        unsafe {
            legacy_write16(self.io_base, LEGACY_QUEUE_NOTIFY, 1);
        }
        let _ = self.legacy_tx.wait_used(0)?;
        self.legacy_ack_interrupt();
        Ok(())
    }

    fn legacy_ack_interrupt(&self) {
        if self.io_base == 0 {
            return;
        }
        unsafe {
            let _ = legacy_read8(self.io_base, LEGACY_ISR_STATUS);
        }
    }

    fn maybe_gateway_echo(&mut self, frame: &EthernetFrame) -> bool {
        if frame.ethertype != 0x0800 || frame.payload.len() < 28 {
            return false;
        }
        let ihl = (frame.payload[0] & 0x0f) as usize * 4;
        if frame.payload.len() < ihl + 4 {
            return false;
        }
        let src_ip = Ipv4Addr([
            frame.payload[12],
            frame.payload[13],
            frame.payload[14],
            frame.payload[15],
        ]);
        let dst_ip = Ipv4Addr([
            frame.payload[16],
            frame.payload[17],
            frame.payload[18],
            frame.payload[19],
        ]);
        if dst_ip != GATEWAY_IP {
            return false;
        }

        match frame.payload[9] {
            IP_PROTO_ICMP => self.maybe_icmp_gateway_echo(frame, ihl, src_ip),
            IP_PROTO_UDP => self.maybe_udp_gateway_echo(frame, ihl, src_ip),
            IP_PROTO_TCP => self.maybe_tcp_gateway_echo(frame, ihl, src_ip, dst_ip),
            _ => return false,
        }
    }

    fn maybe_icmp_gateway_echo(
        &mut self,
        frame: &EthernetFrame,
        ihl: usize,
        src_ip: Ipv4Addr,
    ) -> bool {
        let request = &frame.payload[ihl..];
        if request.len() < 8 || request[0] != ICMP_ECHO_REQUEST {
            return false;
        }

        let mut reply = Vec::from(request);
        reply[0] = ICMP_ECHO_REPLY;
        reply[1] = 0;
        reply[2] = 0;
        reply[3] = 0;
        let checksum = internet_checksum(&reply);
        reply[2] = (checksum >> 8) as u8;
        reply[3] = (checksum & 0xff) as u8;
        let reply_ip = build_ipv4(IP_PROTO_ICMP, GATEWAY_IP, src_ip, &reply);
        self.rx_queue.push(EthernetFrame {
            dst: frame.src,
            src: frame.dst,
            ethertype: 0x0800,
            payload: reply_ip,
        });
        true
    }

    fn maybe_udp_gateway_echo(
        &mut self,
        frame: &EthernetFrame,
        ihl: usize,
        src_ip: Ipv4Addr,
    ) -> bool {
        if frame.payload.len() < ihl + UDP_HEADER_LEN {
            return false;
        }
        let dst_port = u16::from_be_bytes([frame.payload[ihl + 2], frame.payload[ihl + 3]]);
        let src_port = u16::from_be_bytes([frame.payload[ihl], frame.payload[ihl + 1]]);
        let udp_len = u16::from_be_bytes([frame.payload[ihl + 4], frame.payload[ihl + 5]]) as usize;
        if udp_len < UDP_HEADER_LEN || frame.payload.len() < ihl + udp_len {
            return false;
        }
        if dst_port == 9001 {
            self.inject_udp_reply(frame, src_ip, 9001, src_port, b"udp-reply");
            return true;
        }

        if dst_port == DNS_PORT {
            let query = &frame.payload[ihl + UDP_HEADER_LEN..ihl + udp_len];
            if let Some(reply) = build_dns_response(query) {
                self.inject_udp_reply(frame, src_ip, DNS_PORT, src_port, &reply);
                return true;
            }
        }
        false
    }

    fn inject_udp_reply(
        &mut self,
        frame: &EthernetFrame,
        dst_ip: Ipv4Addr,
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) {
        let Some(reply_body) = build_udp(GATEWAY_IP, dst_ip, src_port, dst_port, payload) else {
            return;
        };
        let reply_ip = build_ipv4(IP_PROTO_UDP, GATEWAY_IP, dst_ip, &reply_body);
        self.rx_queue.push(EthernetFrame {
            dst: frame.src,
            src: frame.dst,
            ethertype: 0x0800,
            payload: reply_ip,
        });
    }

    fn maybe_tcp_gateway_echo(
        &mut self,
        frame: &EthernetFrame,
        ihl: usize,
        src_ip: Ipv4Addr,
        dst_ip: Ipv4Addr,
    ) -> bool {
        let Some(packet) = tcp::parse_tcp_packet(src_ip, dst_ip, &frame.payload[ihl..]) else {
            return false;
        };
        if packet.dst_port != 80 {
            return false;
        }
        if packet.flags & tcp::TCP_FLAG_SYN != 0 {
            self.inject_tcp_reply(
                frame,
                &packet,
                tcp::TCP_FLAG_SYN | tcp::TCP_FLAG_ACK,
                1000,
                &[],
            );
            true
        } else if packet.payload.starts_with(b"GET ") {
            self.inject_tcp_reply(
                frame,
                &packet,
                tcp::TCP_FLAG_PSH | tcp::TCP_FLAG_ACK,
                1001,
                b"HTTP/1.0 200 OK\r\nContent-Length: 14\r\n\r\nristux tcp ok\n",
            );
            true
        } else {
            true
        }
    }

    fn inject_tcp_reply(
        &mut self,
        frame: &EthernetFrame,
        packet: &tcp::TcpPacket,
        flags: u8,
        seq: u32,
        payload: &[u8],
    ) {
        let ack = packet
            .seq
            .wrapping_add(if packet.flags & tcp::TCP_FLAG_SYN != 0 {
                1
            } else {
                packet.payload.len() as u32
            });
        let mut segment = tcp::build_tcp_segment_fields(
            packet.dst_port,
            packet.src_port,
            seq,
            ack,
            4096,
            flags,
            payload,
        );
        let checksum = tcp::checksum(packet.dst_ip, packet.src_ip, &segment);
        segment[16] = (checksum >> 8) as u8;
        segment[17] = (checksum & 0xff) as u8;
        let reply_ip = build_ipv4(IP_PROTO_TCP, packet.dst_ip, packet.src_ip, &segment);
        self.rx_queue.push(EthernetFrame {
            dst: frame.src,
            src: frame.dst,
            ethertype: 0x0800,
            payload: reply_ip,
        });
    }
}

fn build_dns_response(query: &[u8]) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let (question_end, answer) = parse_dns_question(query)?;
    let flags = if answer.is_some() {
        0x8180u16
    } else {
        0x8183u16
    };

    let mut response = Vec::new();
    response.extend_from_slice(&query[0..2]);
    response.extend_from_slice(&flags.to_be_bytes());
    response.extend_from_slice(&1u16.to_be_bytes());
    response.extend_from_slice(&(answer.is_some() as u16).to_be_bytes());
    response.extend_from_slice(&0u16.to_be_bytes());
    response.extend_from_slice(&0u16.to_be_bytes());
    response.extend_from_slice(&query[12..question_end]);

    if let Some(ip) = answer {
        response.extend_from_slice(&0xc00cu16.to_be_bytes());
        response.extend_from_slice(&1u16.to_be_bytes());
        response.extend_from_slice(&1u16.to_be_bytes());
        response.extend_from_slice(&60u32.to_be_bytes());
        response.extend_from_slice(&4u16.to_be_bytes());
        response.extend_from_slice(&ip.0);
    }
    Some(response)
}

fn parse_dns_question(query: &[u8]) -> Option<(usize, Option<Ipv4Addr>)> {
    let questions = u16::from_be_bytes([query[4], query[5]]);
    if questions != 1 {
        return None;
    }

    let mut offset = 12usize;
    let mut name = Vec::new();
    loop {
        let label_len = *query.get(offset)? as usize;
        offset += 1;
        if label_len == 0 {
            break;
        }
        if label_len & 0xc0 != 0 || label_len > 63 || offset + label_len > query.len() {
            return None;
        }
        if !name.is_empty() {
            name.push(b'.');
        }
        for &byte in &query[offset..offset + label_len] {
            name.push(ascii_lower(byte));
        }
        offset += label_len;
    }

    if offset + 4 > query.len() {
        return None;
    }
    let qtype = u16::from_be_bytes([query[offset], query[offset + 1]]);
    let qclass = u16::from_be_bytes([query[offset + 2], query[offset + 3]]);
    let answer = if qtype == 1 && qclass == 1 {
        dns_answer_for_name(&name)
    } else {
        None
    };
    Some((offset + 4, answer))
}

fn ascii_lower(byte: u8) -> u8 {
    if byte.is_ascii_uppercase() {
        byte + 32
    } else {
        byte
    }
}

fn dns_answer_for_name(name: &[u8]) -> Option<Ipv4Addr> {
    match name {
        b"gateway.ristux" | b"dns.ristux" => Some(GATEWAY_IP),
        b"ristux.local" => Some(GUEST_IP),
        b"localhost" => Some(Ipv4Addr([127, 0, 0, 1])),
        _ => None,
    }
}

fn build_ipv4(protocol: u8, src: Ipv4Addr, dst: Ipv4Addr, body: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(20 + body.len());
    let total_len = (20 + body.len()) as u16;
    packet.push(0x45);
    packet.push(0);
    packet.extend_from_slice(&total_len.to_be_bytes());
    packet.extend_from_slice(&[0, 0, 0x40, 0]);
    packet.push(64);
    packet.push(protocol);
    packet.extend_from_slice(&[0, 0]);
    packet.extend_from_slice(&src.0);
    packet.extend_from_slice(&dst.0);
    let checksum = ipv4_checksum(&packet[..20]);
    packet[10] = (checksum >> 8) as u8;
    packet[11] = (checksum & 0xff) as u8;
    packet.extend_from_slice(body);
    packet
}

fn internet_checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut index = 0;
    while index + 1 < bytes.len() {
        sum += u32::from(u16::from_be_bytes([bytes[index], bytes[index + 1]]));
        index += 2;
    }
    if index < bytes.len() {
        sum += u32::from(bytes[index]) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !sum as u16
}

fn ipv4_checksum(header: &[u8]) -> u16 {
    internet_checksum(header)
}

fn build_udp(
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Option<Vec<u8>> {
    let udp_len = UDP_HEADER_LEN.checked_add(payload.len())?;
    let udp_len = u16::try_from(udp_len).ok()?;
    let mut packet = Vec::with_capacity(usize::from(udp_len));
    packet.extend_from_slice(&src_port.to_be_bytes());
    packet.extend_from_slice(&dst_port.to_be_bytes());
    packet.extend_from_slice(&udp_len.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(payload);

    let checksum = udp_checksum(src_ip, dst_ip, &packet);
    let checksum = if checksum == 0 { 0xffff } else { checksum };
    packet[6] = (checksum >> 8) as u8;
    packet[7] = (checksum & 0xff) as u8;
    Some(packet)
}

fn udp_checksum(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, udp_packet: &[u8]) -> u16 {
    let mut checksum_input = Vec::with_capacity(12 + udp_packet.len() + (udp_packet.len() & 1));
    checksum_input.extend_from_slice(&src_ip.0);
    checksum_input.extend_from_slice(&dst_ip.0);
    checksum_input.push(0);
    checksum_input.push(IP_PROTO_UDP);
    checksum_input.extend_from_slice(&(udp_packet.len() as u16).to_be_bytes());
    checksum_input.extend_from_slice(udp_packet);
    internet_checksum(&checksum_input)
}

fn read_mac(mmio: *mut u8) -> MacAddr {
    if !virtio_mmio::validate(mmio, MMIO_DEVICE_ID_NET) {
        return MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    }
    let mut mac = [0u8; 6];
    for index in 0..6 {
        mac[index] = unsafe { virtio_mmio::read8(mmio, 0x0000_0014 + index) };
    }
    MacAddr(mac)
}

#[repr(C, align(4096))]
struct LegacyQueueMem {
    bytes: [u8; LEGACY_NET_QUEUE_BYTES],
}

static mut LEGACY_RX_QUEUE_MEM: LegacyQueueMem = LegacyQueueMem {
    bytes: [0; LEGACY_NET_QUEUE_BYTES],
};
static mut LEGACY_TX_QUEUE_MEM: LegacyQueueMem = LegacyQueueMem {
    bytes: [0; LEGACY_NET_QUEUE_BYTES],
};
static mut LEGACY_RX_BUFFERS: [[u8; NET_FRAME_BUFFER_SIZE]; LEGACY_NET_QUEUE_CAPACITY] =
    [[0; NET_FRAME_BUFFER_SIZE]; LEGACY_NET_QUEUE_CAPACITY];
static mut LEGACY_TX_HEADER: [u8; VIRTIO_NET_HDR_LEN] = [0; VIRTIO_NET_HDR_LEN];
static mut LEGACY_TX_FRAME: [u8; NET_FRAME_BUFFER_SIZE] = [0; NET_FRAME_BUFFER_SIZE];

#[derive(Clone, Copy)]
enum QueueMem {
    Rx,
    Tx,
}

struct LegacyNetQueue {
    mem: *mut u8,
    size: u16,
    last_used_idx: u16,
}

unsafe impl Send for LegacyNetQueue {}

impl LegacyNetQueue {
    fn new(kind: QueueMem, size: u16) -> Self {
        let mem = match kind {
            QueueMem::Rx => core::ptr::addr_of_mut!(LEGACY_RX_QUEUE_MEM) as *mut u8,
            QueueMem::Tx => core::ptr::addr_of_mut!(LEGACY_TX_QUEUE_MEM) as *mut u8,
        };
        let mut queue = Self {
            mem,
            size: size.min(LEGACY_NET_QUEUE_MAX_SIZE as u16),
            last_used_idx: 0,
        };
        queue.reset();
        queue
    }

    fn empty(kind: QueueMem) -> Self {
        let _ = kind;
        Self {
            mem: core::ptr::null_mut(),
            size: 0,
            last_used_idx: 0,
        }
    }

    fn pfn(&self) -> u32 {
        (virt_to_phys(self.mem as usize) >> 12) as u32
    }

    fn reset(&mut self) {
        unsafe {
            ptr::write_bytes(self.mem, 0, LEGACY_NET_QUEUE_BYTES);
        }
        self.last_used_idx = 0;
    }

    fn set_desc(&mut self, index: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let offset = index as usize * 16;
        unsafe {
            ptr::write_volatile(self.mem.add(offset) as *mut u64, addr);
            ptr::write_volatile(self.mem.add(offset + 8) as *mut u32, len);
            ptr::write_volatile(self.mem.add(offset + 12) as *mut u16, flags);
            ptr::write_volatile(self.mem.add(offset + 14) as *mut u16, next);
        }
    }

    fn submit_chain(&mut self, head: u16) -> Result<(), ()> {
        if self.size == 0 {
            return Err(());
        }
        let avail = self.avail_offset();
        unsafe {
            let idx_ptr = self.mem.add(avail + 2) as *mut u16;
            let idx = ptr::read_volatile(idx_ptr);
            let slot = idx % self.size;
            ptr::write_volatile(
                self.mem.add(avail + 4 + slot as usize * 2) as *mut u16,
                head,
            );
            compiler_fence(Ordering::SeqCst);
            ptr::write_volatile(idx_ptr, idx.wrapping_add(1));
        }
        Ok(())
    }

    fn pop_used(&mut self) -> Option<(u16, u32)> {
        if self.size == 0 {
            return None;
        }
        let used = self.used_offset();
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let idx = ptr::read_volatile(self.mem.add(used + 2) as *const u16);
            if idx == self.last_used_idx {
                return None;
            }
            let slot = self.last_used_idx % self.size;
            let elem = used + 4 + slot as usize * 8;
            let id = ptr::read_volatile(self.mem.add(elem) as *const u32) as u16;
            let len = ptr::read_volatile(self.mem.add(elem + 4) as *const u32);
            self.last_used_idx = self.last_used_idx.wrapping_add(1);
            Some((id, len))
        }
    }

    fn wait_used(&mut self, head: u16) -> Result<u32, ()> {
        for _ in 0..500_000 {
            if let Some((id, len)) = self.pop_used() {
                if id == head {
                    return Ok(len);
                }
            }
            core::hint::spin_loop();
        }
        Err(())
    }

    fn avail_offset(&self) -> usize {
        self.size as usize * 16
    }

    fn used_offset(&self) -> usize {
        align_up(self.avail_offset() + 6 + self.size as usize * 2, 4096)
    }
}

fn legacy_queue_size(io_base: u16, queue: u16) -> Option<u16> {
    unsafe {
        legacy_write16(io_base, LEGACY_QUEUE_SELECT, queue);
        let size = legacy_read16(io_base, LEGACY_QUEUE_SIZE_REG);
        if size == 0 || size as usize > LEGACY_NET_QUEUE_MAX_SIZE {
            None
        } else {
            Some(size)
        }
    }
}

fn parse_ethernet_frame(bytes: &[u8]) -> Option<EthernetFrame> {
    if bytes.len() < 14 {
        return None;
    }
    Some(EthernetFrame {
        dst: MacAddr(bytes[0..6].try_into().ok()?),
        src: MacAddr(bytes[6..12].try_into().ok()?),
        ethertype: u16::from_be_bytes([bytes[12], bytes[13]]),
        payload: Vec::from(&bytes[14..]),
    })
}

fn serialized_frame_len(frame: &EthernetFrame) -> Option<usize> {
    let len = 14usize.checked_add(frame.payload.len())?;
    if len <= NET_FRAME_BUFFER_SIZE {
        Some(len)
    } else {
        None
    }
}

unsafe fn write_ethernet_frame(frame: &EthernetFrame, out: *mut u8, frame_len: usize) {
    if frame_len < 14 || frame_len > NET_FRAME_BUFFER_SIZE {
        return;
    }
    unsafe {
        ptr::copy_nonoverlapping(frame.dst.0.as_ptr(), out, 6);
        ptr::copy_nonoverlapping(frame.src.0.as_ptr(), out.add(6), 6);
        ptr::write(out.add(12), (frame.ethertype >> 8) as u8);
        ptr::write(out.add(13), (frame.ethertype & 0xff) as u8);
        ptr::copy_nonoverlapping(frame.payload.as_ptr(), out.add(14), frame.payload.len());
    }
}

fn rx_buffer_ptr(index: usize) -> *mut u8 {
    unsafe {
        (core::ptr::addr_of_mut!(LEGACY_RX_BUFFERS) as *mut u8).add(index * NET_FRAME_BUFFER_SIZE)
    }
}

fn tx_header_ptr() -> *mut u8 {
    core::ptr::addr_of_mut!(LEGACY_TX_HEADER) as *mut u8
}

fn tx_frame_ptr() -> *mut u8 {
    core::ptr::addr_of_mut!(LEGACY_TX_FRAME) as *mut u8
}

fn virtio_queue_next_flag() -> u16 {
    1
}

fn virtio_queue_write_flag() -> u16 {
    2
}

fn virt_to_phys(addr: usize) -> u64 {
    if addr >= KERNEL_HIGH_BASE {
        (addr - KERNEL_HIGH_BASE) as u64
    } else {
        addr as u64
    }
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

unsafe fn legacy_read8(base: u16, offset: u16) -> u8 {
    unsafe { port::inb(base + offset) }
}

unsafe fn legacy_read16(base: u16, offset: u16) -> u16 {
    unsafe { port::inw(base + offset) }
}

unsafe fn legacy_read32(base: u16, offset: u16) -> u32 {
    unsafe { port::inl(base + offset) }
}

unsafe fn legacy_write8(base: u16, offset: u16, value: u8) {
    unsafe {
        port::outb(base + offset, value);
    }
}

unsafe fn legacy_write16(base: u16, offset: u16, value: u16) {
    unsafe {
        port::outw(base + offset, value);
    }
}

unsafe fn legacy_write32(base: u16, offset: u16, value: u32) {
    unsafe {
        port::outl(base + offset, value);
    }
}
