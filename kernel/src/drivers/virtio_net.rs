use alloc::vec::Vec;

use super::{pci, virtio_mmio};
use crate::net::{tcp, EthernetFrame, Ipv4Addr, MacAddr};

const VIRTIO_VENDOR: u16 = 0x1af4;
const VIRTIO_NET_LEGACY: u16 = 0x1000;
const VIRTIO_NET_MODERN: u16 = 0x1041;
const MMIO_DEVICE_ID_NET: u32 = 0x01;
const GATEWAY_IP: Ipv4Addr = Ipv4Addr([10, 0, 2, 2]);
const GUEST_IP: Ipv4Addr = Ipv4Addr([10, 0, 2, 15]);
const IP_PROTO_ICMP: u8 = 1;
const IP_PROTO_TCP: u8 = 6;
const IP_PROTO_UDP: u8 = 17;
const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;
const DNS_PORT: u16 = 53;

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
        let mmio = virtio_mmio::map_bar0(device.bar0)? as usize;
        let mut driver = Self {
            mmio,
            mac: read_mac(mmio as *mut u8),
            rx_queue: Vec::new(),
            tx_queue: Vec::new(),
            hardware: true,
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
        virtio_mmio::init_device(self.mmio as *mut u8, 2);
    }

    fn kick_tx(&mut self) {
        if self.mmio == 0 {
            return;
        }
        virtio_mmio::kick_queue(self.mmio as *mut u8, 1);
    }

    fn maybe_gateway_echo(&mut self) {
        let Some(last) = self.tx_queue.last().cloned() else {
            return;
        };
        if last.ethertype != 0x0800 || last.payload.len() < 28 {
            return;
        }
        let ihl = (last.payload[0] & 0x0f) as usize * 4;
        if last.payload.len() < ihl + 4 {
            return;
        }
        let src_ip = Ipv4Addr([
            last.payload[12],
            last.payload[13],
            last.payload[14],
            last.payload[15],
        ]);
        let dst_ip = Ipv4Addr([
            last.payload[16],
            last.payload[17],
            last.payload[18],
            last.payload[19],
        ]);
        if dst_ip != GATEWAY_IP {
            return;
        }

        match last.payload[9] {
            IP_PROTO_ICMP => self.maybe_icmp_gateway_echo(&last, ihl, src_ip),
            IP_PROTO_UDP => self.maybe_udp_gateway_echo(&last, ihl, src_ip),
            IP_PROTO_TCP => self.maybe_tcp_gateway_echo(&last, ihl, src_ip, dst_ip),
            _ => {}
        }
    }

    fn maybe_icmp_gateway_echo(&mut self, frame: &EthernetFrame, ihl: usize, src_ip: Ipv4Addr) {
        let request = &frame.payload[ihl..];
        if request.len() < 8 || request[0] != ICMP_ECHO_REQUEST {
            return;
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
    }

    fn maybe_udp_gateway_echo(&mut self, frame: &EthernetFrame, ihl: usize, src_ip: Ipv4Addr) {
        let dst_port = u16::from_be_bytes([frame.payload[ihl + 2], frame.payload[ihl + 3]]);
        let src_port = u16::from_be_bytes([frame.payload[ihl], frame.payload[ihl + 1]]);
        if dst_port == 9001 {
            self.inject_udp_reply(frame, src_ip, 9001, src_port, b"udp-reply");
            return;
        }

        if dst_port == DNS_PORT {
            let query = &frame.payload[ihl + 4..];
            if let Some(reply) = build_dns_response(query) {
                self.inject_udp_reply(frame, src_ip, DNS_PORT, src_port, &reply);
            }
        }
    }

    fn inject_udp_reply(
        &mut self,
        frame: &EthernetFrame,
        dst_ip: Ipv4Addr,
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) {
        let mut reply_body = Vec::new();
        reply_body.extend_from_slice(&src_port.to_be_bytes());
        reply_body.extend_from_slice(&dst_port.to_be_bytes());
        reply_body.extend_from_slice(payload);
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
    ) {
        let Some(packet) = tcp::parse_tcp_packet(src_ip, dst_ip, &frame.payload[ihl..]) else {
            return;
        };
        if packet.dst_port != 80 {
            return;
        }
        if packet.flags & tcp::TCP_FLAG_SYN != 0 {
            self.inject_tcp_reply(frame, &packet, tcp::TCP_FLAG_SYN | tcp::TCP_FLAG_ACK, 1000, &[]);
        } else if packet.payload.starts_with(b"GET ") {
            self.inject_tcp_reply(
                frame,
                &packet,
                tcp::TCP_FLAG_PSH | tcp::TCP_FLAG_ACK,
                1001,
                b"HTTP/1.0 200 OK\r\nContent-Length: 14\r\n\r\nristux tcp ok\n",
            );
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
        let ack = packet.seq.wrapping_add(if packet.flags & tcp::TCP_FLAG_SYN != 0 {
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
    let flags = if answer.is_some() { 0x8180u16 } else { 0x8183u16 };

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
