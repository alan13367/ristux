use alloc::vec::Vec;

pub mod socket;
pub mod tcp;

use crate::{
    drivers::virtio_net::VirtioNetDriver,
    sync::spinlock::SpinLock,
};

const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_ARP: u16 = 0x0806;
const ARP_REQUEST: u16 = 1;
const ARP_REPLY: u16 = 2;
const IP_PROTO_ICMP: u8 = 1;
const IP_PROTO_UDP: u8 = 17;
const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;

static NET_STATS: SpinLock<NetStats> = SpinLock::new(NetStats::empty());
static NET_STACK: SpinLock<Option<NetworkStack>> = SpinLock::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacAddr(pub [u8; 6]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Addr(pub [u8; 4]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EthernetFrame {
    pub dst: MacAddr,
    pub src: MacAddr,
    pub ethertype: u16,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketId(usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetStats {
    pub rx_frames: usize,
    pub tx_frames: usize,
    pub arp_entries: usize,
    pub udp_sockets: usize,
}

impl NetStats {
    const fn empty() -> Self {
        Self {
            rx_frames: 0,
            tx_frames: 0,
            arp_entries: 0,
            udp_sockets: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct ArpEntry {
    ip: Ipv4Addr,
    mac: MacAddr,
}

struct UdpSocket {
    local_port: u16,
    inbox: Vec<UdpDatagram>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UdpDatagram {
    src: Ipv4Addr,
    src_port: u16,
    payload: Vec<u8>,
}

struct NetworkStack {
    mac: MacAddr,
    ip: Ipv4Addr,
    device: VirtioNetDriver,
    arp_cache: Vec<ArpEntry>,
    udp_sockets: Vec<UdpSocket>,
    rx_frames: usize,
    tx_frames: usize,
}

impl NetworkStack {
    fn new(device: VirtioNetDriver, ip: Ipv4Addr) -> Self {
        let mac = device.mac();
        Self {
            mac,
            ip,
            device,
            arp_cache: Vec::new(),
            udp_sockets: Vec::new(),
            rx_frames: 0,
            tx_frames: 0,
        }
    }

    fn bind_udp(&mut self, local_port: u16) -> SocketId {
        self.udp_sockets.push(UdpSocket {
            local_port,
            inbox: Vec::new(),
        });
        SocketId(self.udp_sockets.len() - 1)
    }

    fn inject_rx(&mut self, frame: EthernetFrame) {
        self.device.inject_rx(frame);
    }

    fn poll(&mut self) {
        loop {
            let Some(frame) = self.device.poll_rx() else {
                break;
            };
            self.rx_frames += 1;
            if frame.dst != self.mac && frame.dst != MacAddr([0xff; 6]) {
                continue;
            }

            match frame.ethertype {
                ETHERTYPE_ARP => self.handle_arp(frame),
                ETHERTYPE_IPV4 => self.handle_ipv4(frame),
                _ => {}
            }
        }
    }

    fn send_udp(
        &mut self,
        socket: SocketId,
        dst_ip: Ipv4Addr,
        dst_port: u16,
        payload: &[u8],
    ) -> bool {
        let Some(local_port) = self
            .udp_sockets
            .get(socket.0)
            .map(|socket| socket.local_port)
        else {
            return false;
        };
        let Some(dst_mac) = self.resolve_mac(dst_ip) else {
            return false;
        };

        let mut body = Vec::new();
        body.extend_from_slice(&local_port.to_be_bytes());
        body.extend_from_slice(&dst_port.to_be_bytes());
        body.extend_from_slice(payload);
        self.transmit_ipv4(dst_mac, dst_ip, IP_PROTO_UDP, &body);
        true
    }

    fn recv_udp(&mut self, socket: SocketId) -> Option<UdpDatagram> {
        let inbox = &mut self.udp_sockets.get_mut(socket.0)?.inbox;
        if inbox.is_empty() {
            None
        } else {
            Some(inbox.remove(0))
        }
    }

    fn pop_tx(&mut self) -> Option<EthernetFrame> {
        self.device.pop_tx()
    }

    fn transmit(&mut self, frame: EthernetFrame) {
        self.tx_frames += 1;
        self.device.transmit(frame);
        self.poll();
    }

    fn handle_arp(&mut self, frame: EthernetFrame) {
        let Some(packet) = parse_arp(&frame.payload) else {
            return;
        };
        self.cache_arp(packet.sender_ip, packet.sender_mac);

        if packet.opcode == ARP_REQUEST && packet.target_ip == self.ip {
            let reply = build_arp(
                ARP_REPLY,
                self.mac,
                self.ip,
                packet.sender_mac,
                packet.sender_ip,
            );
            self.transmit(EthernetFrame {
                dst: packet.sender_mac,
                src: self.mac,
                ethertype: ETHERTYPE_ARP,
                payload: reply,
            });
        }
    }

    fn handle_ipv4(&mut self, frame: EthernetFrame) {
        let Some(packet) = parse_ipv4(&frame.payload) else {
            return;
        };
        self.cache_arp(packet.src, frame.src);
        if packet.dst != self.ip {
            return;
        }

        match packet.protocol {
            IP_PROTO_ICMP => self.handle_icmp(frame.src, packet),
            IP_PROTO_UDP => self.handle_udp(packet),
            _ => {}
        }
    }

    fn handle_icmp(&mut self, dst_mac: MacAddr, packet: Ipv4Packet) {
        if packet.payload.len() < 4 || packet.payload[0] != ICMP_ECHO_REQUEST {
            return;
        }

        let mut reply = Vec::new();
        reply.push(ICMP_ECHO_REPLY);
        reply.push(0);
        reply.extend_from_slice(&packet.payload[2..]);
        self.transmit_ipv4(dst_mac, packet.src, IP_PROTO_ICMP, &reply);
    }

    fn handle_udp(&mut self, packet: Ipv4Packet) {
        if packet.payload.len() < 4 {
            return;
        }
        let src_port = u16::from_be_bytes([packet.payload[0], packet.payload[1]]);
        let dst_port = u16::from_be_bytes([packet.payload[2], packet.payload[3]]);
        let Some(socket) = self
            .udp_sockets
            .iter_mut()
            .find(|socket| socket.local_port == dst_port)
        else {
            return;
        };
        socket.inbox.push(UdpDatagram {
            src: packet.src,
            src_port,
            payload: Vec::from(&packet.payload[4..]),
        });
        crate::process::wake_io_waiters();
    }

    fn transmit_ipv4(&mut self, dst_mac: MacAddr, dst_ip: Ipv4Addr, protocol: u8, body: &[u8]) {
        let payload = build_ipv4(protocol, self.ip, dst_ip, body);
        self.transmit(EthernetFrame {
            dst: dst_mac,
            src: self.mac,
            ethertype: ETHERTYPE_IPV4,
            payload,
        });
    }

    fn cache_arp(&mut self, ip: Ipv4Addr, mac: MacAddr) {
        if let Some(entry) = self.arp_cache.iter_mut().find(|entry| entry.ip == ip) {
            entry.mac = mac;
            return;
        }
        self.arp_cache.push(ArpEntry { ip, mac });
    }

    fn resolve_mac(&self, ip: Ipv4Addr) -> Option<MacAddr> {
        self.arp_cache
            .iter()
            .find(|entry| entry.ip == ip)
            .map(|entry| entry.mac)
    }

    fn stats(&self) -> NetStats {
        NetStats {
            rx_frames: self.rx_frames,
            tx_frames: self.tx_frames,
            arp_entries: self.arp_cache.len(),
            udp_sockets: self.udp_sockets.len(),
        }
    }
}

struct ArpPacket {
    opcode: u16,
    sender_mac: MacAddr,
    sender_ip: Ipv4Addr,
    target_ip: Ipv4Addr,
}

struct Ipv4Packet {
    protocol: u8,
    src: Ipv4Addr,
    dst: Ipv4Addr,
    payload: Vec<u8>,
}

pub fn init() {
    socket::init();
    socket::self_test();
    self_test();
    tcp::self_test();
    *NET_STACK.lock() = Some(runtime_stack());
}

pub fn stats() -> NetStats {
    *NET_STATS.lock()
}

pub fn udp_bind(local_port: u16) -> Option<usize> {
    let mut guard = NET_STACK.lock();
    let stack = guard.as_mut()?;
    let socket = stack.bind_udp(local_port);
    crate::println!(
        "UDP socket {} bound to local port {}.",
        socket.0,
        local_port
    );
    Some(socket.0)
}

pub fn udp_send(socket: usize, dst_ip: [u8; 4], dst_port: u16, payload: &[u8]) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    let socket = SocketId(socket);
    if stack.udp_sockets.get(socket.0).is_none() {
        return false;
    }
    let dst_ip = Ipv4Addr(dst_ip);
    if !stack.send_udp(socket, dst_ip, dst_port, payload) {
        return false;
    }

    crate::println!(
        "UDP socket {} sent {} byte(s) to {}.{}.{}.{}:{}.",
        socket.0,
        payload.len(),
        dst_ip.0[0],
        dst_ip.0[1],
        dst_ip.0[2],
        dst_ip.0[3],
        dst_port
    );
    true
}

pub fn udp_recv(socket: usize, output: &mut [u8]) -> Option<usize> {
    let mut guard = NET_STACK.lock();
    let stack = guard.as_mut()?;
    stack.poll();
    let datagram = stack.recv_udp(SocketId(socket))?;
    let count = datagram.payload.len().min(output.len());
    output[..count].copy_from_slice(&datagram.payload[..count]);
    crate::println!(
        "UDP socket {} received {} byte(s) from {}.{}.{}.{}:{}.",
        socket,
        count,
        datagram.src.0[0],
        datagram.src.0[1],
        datagram.src.0[2],
        datagram.src.0[3],
        datagram.src_port
    );
    Some(count)
}

fn runtime_stack() -> NetworkStack {
    let device =
        VirtioNetDriver::probe().unwrap_or_else(VirtioNetDriver::software_fallback);
    let local_ip = Ipv4Addr([10, 0, 2, 15]);
    let peer_ip = Ipv4Addr([10, 0, 2, 2]);
    let peer_mac = MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let mut stack = NetworkStack::new(device, local_ip);
    stack.cache_arp(peer_ip, peer_mac);
    stack
}

fn build_arp(
    opcode: u16,
    sender_mac: MacAddr,
    sender_ip: Ipv4Addr,
    target_mac: MacAddr,
    target_ip: Ipv4Addr,
) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&opcode.to_be_bytes());
    payload.extend_from_slice(&sender_mac.0);
    payload.extend_from_slice(&sender_ip.0);
    payload.extend_from_slice(&target_mac.0);
    payload.extend_from_slice(&target_ip.0);
    payload
}

fn parse_arp(payload: &[u8]) -> Option<ArpPacket> {
    if payload.len() < 22 {
        return None;
    }
    Some(ArpPacket {
        opcode: u16::from_be_bytes([payload[0], payload[1]]),
        sender_mac: MacAddr(payload[2..8].try_into().ok()?),
        sender_ip: Ipv4Addr(payload[8..12].try_into().ok()?),
        target_ip: Ipv4Addr(payload[18..22].try_into().ok()?),
    })
}

fn build_ipv4(protocol: u8, src: Ipv4Addr, dst: Ipv4Addr, body: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(20 + body.len());
    let total = (20 + body.len()) as u16;
    packet.push(0x45);
    packet.push(0);
    packet.extend_from_slice(&total.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&0x4000u16.to_be_bytes());
    packet.push(64);
    packet.push(protocol);
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&src.0);
    packet.extend_from_slice(&dst.0);
    let checksum = ipv4_checksum(&packet);
    packet[10] = (checksum >> 8) as u8;
    packet[11] = (checksum & 0xff) as u8;
    packet.extend_from_slice(body);
    packet
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

fn parse_ipv4(payload: &[u8]) -> Option<Ipv4Packet> {
    if payload.len() < 20 || payload[0] >> 4 != 4 {
        return None;
    }
    let ihl = (payload[0] & 0x0f) as usize * 4;
    if payload.len() < ihl {
        return None;
    }
    Some(Ipv4Packet {
        protocol: payload[9],
        src: Ipv4Addr(payload[12..16].try_into().ok()?),
        dst: Ipv4Addr(payload[16..20].try_into().ok()?),
        payload: Vec::from(&payload[ihl..]),
    })
}

fn self_test() {
    let peer_mac = MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let local_ip = Ipv4Addr([10, 0, 2, 15]);
    let peer_ip = Ipv4Addr([10, 0, 2, 2]);
    let mut stack = NetworkStack::new(VirtioNetDriver::software_fallback(), local_ip);
    let local_mac = stack.mac;

    stack.inject_rx(EthernetFrame {
        dst: MacAddr([0xff; 6]),
        src: peer_mac,
        ethertype: ETHERTYPE_ARP,
        payload: build_arp(ARP_REQUEST, peer_mac, peer_ip, MacAddr([0; 6]), local_ip),
    });
    stack.poll();
    let arp_reply = stack.pop_tx().expect("ARP did not transmit a reply");
    if arp_reply.ethertype != ETHERTYPE_ARP
        || arp_reply.dst != peer_mac
        || parse_arp(&arp_reply.payload).map(|packet| packet.opcode) != Some(ARP_REPLY)
    {
        panic!("ARP self-test failed");
    }

    let mut echo_body = Vec::new();
    echo_body.push(ICMP_ECHO_REQUEST);
    echo_body.push(0);
    echo_body.extend_from_slice(&7u16.to_be_bytes());
    echo_body.extend_from_slice(b"ping");
    stack.inject_rx(EthernetFrame {
        dst: local_mac,
        src: peer_mac,
        ethertype: ETHERTYPE_IPV4,
        payload: build_ipv4(IP_PROTO_ICMP, peer_ip, local_ip, &echo_body),
    });
    stack.poll();
    let echo_reply = stack.pop_tx().expect("ICMP did not transmit a reply");
    let echo_packet = parse_ipv4(&echo_reply.payload).expect("ICMP reply was not IPv4");
    if echo_reply.ethertype != ETHERTYPE_IPV4
        || echo_reply.dst != peer_mac
        || echo_packet.protocol != IP_PROTO_ICMP
        || echo_packet.payload.first() != Some(&ICMP_ECHO_REPLY)
    {
        panic!("ICMP echo self-test failed");
    }

    let socket = stack.bind_udp(9000);
    if !stack.send_udp(socket, peer_ip, 9001, b"ristux") {
        panic!("UDP send self-test could not resolve peer");
    }
    let udp_tx = stack.pop_tx().expect("UDP send did not transmit a frame");
    let udp_packet = parse_ipv4(&udp_tx.payload).expect("UDP transmit was not IPv4");
    if udp_tx.dst != peer_mac
        || udp_packet.protocol != IP_PROTO_UDP
        || udp_packet.payload[0..4] != [0x23, 0x28, 0x23, 0x29]
        || &udp_packet.payload[4..] != b"ristux"
    {
        panic!("UDP send self-test failed");
    }

    let received = stack
        .recv_udp(socket)
        .expect("UDP socket inbox stayed empty");
    if received.src != peer_ip || received.src_port != 9001 || received.payload != b"udp-reply" {
        panic!("UDP receive self-test failed");
    }

    *NET_STATS.lock() = stack.stats();
    crate::println!("Networking self-test passed: VirtIO net, ARP, IPv4, ICMP, UDP sockets.");
}
