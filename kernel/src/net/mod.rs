use alloc::vec::Vec;

pub mod socket;
pub mod tcp;

use crate::{drivers::virtio_net::VirtioNetDriver, sync::spinlock::SpinLock};

const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_ARP: u16 = 0x0806;
const ARP_REQUEST: u16 = 1;
const ARP_REPLY: u16 = 2;
const IP_PROTO_ICMP: u8 = 1;
const IP_PROTO_TCP: u8 = 6;
const IP_PROTO_UDP: u8 = 17;
const UDP_HEADER_LEN: usize = 8;
const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;
pub(crate) const LOCAL_IP: Ipv4Addr = Ipv4Addr([10, 0, 2, 15]);
pub(crate) const LOOPBACK_IP: Ipv4Addr = Ipv4Addr([127, 0, 0, 1]);

static NET_STATS: SpinLock<NetStats> = SpinLock::new(NetStats::empty());
static NET_STACK: SpinLock<Option<NetworkStack>> = SpinLock::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacAddr(pub [u8; 6]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    pub const fn is_loopback(self) -> bool {
        self.0[0] == 127
    }

    pub const fn is_unspecified(self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    pub const fn is_broadcast(self) -> bool {
        self.0[0] == 255 && self.0[1] == 255 && self.0[2] == 255 && self.0[3] == 255
    }
}

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
pub(crate) struct IcmpSocketId(usize);

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

struct IcmpSocket {
    inbox: Vec<IcmpDatagram>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UdpDatagram {
    pub src: Ipv4Addr,
    pub src_port: u16,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IcmpDatagram {
    pub src: Ipv4Addr,
    pub payload: Vec<u8>,
}

struct NetworkStack {
    mac: MacAddr,
    ip: Ipv4Addr,
    subnet_mask: Option<Ipv4Addr>,
    gateway: Option<Ipv4Addr>,
    dns_server: Option<Ipv4Addr>,
    device: VirtioNetDriver,
    arp_cache: Vec<ArpEntry>,
    udp_sockets: Vec<UdpSocket>,
    icmp_sockets: Vec<IcmpSocket>,
    tcp_inbox: Vec<tcp::TcpPacket>,
    rx_frames: usize,
    tx_frames: usize,
    dhcp_status: &'static str,
}

impl NetworkStack {
    fn new(device: VirtioNetDriver, ip: Ipv4Addr) -> Self {
        let mac = device.mac();
        Self {
            mac,
            ip,
            subnet_mask: None,
            gateway: None,
            dns_server: None,
            device,
            arp_cache: Vec::new(),
            udp_sockets: Vec::new(),
            icmp_sockets: Vec::new(),
            tcp_inbox: Vec::new(),
            rx_frames: 0,
            tx_frames: 0,
            dhcp_status: "not_attempted",
        }
    }

    fn bind_udp(&mut self, local_port: u16) -> SocketId {
        let local_port = self.udp_port_or_ephemeral(local_port);
        self.udp_sockets.push(UdpSocket {
            local_port,
            inbox: Vec::new(),
        });
        SocketId(self.udp_sockets.len() - 1)
    }

    fn rebind_udp(&mut self, socket: SocketId, local_port: u16) -> bool {
        let local_port = self.udp_port_or_ephemeral(local_port);
        let Some(socket) = self.udp_sockets.get_mut(socket.0) else {
            return false;
        };
        socket.local_port = local_port;
        true
    }

    fn close_udp(&mut self, socket: SocketId) -> bool {
        let Some(socket) = self.udp_sockets.get_mut(socket.0) else {
            return false;
        };
        socket.local_port = 0;
        socket.inbox.clear();
        true
    }

    fn open_icmp(&mut self) -> IcmpSocketId {
        self.icmp_sockets.push(IcmpSocket { inbox: Vec::new() });
        IcmpSocketId(self.icmp_sockets.len() - 1)
    }

    fn close_icmp(&mut self, socket: IcmpSocketId) -> bool {
        let Some(socket) = self.icmp_sockets.get_mut(socket.0) else {
            return false;
        };
        socket.inbox.clear();
        true
    }

    fn udp_port_or_ephemeral(&self, local_port: u16) -> u16 {
        if local_port != 0 {
            return local_port;
        }
        49152u16.wrapping_add(self.udp_sockets.len() as u16)
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
        let src_ip = if dst_ip.is_loopback() {
            LOOPBACK_IP
        } else {
            self.ip
        };
        let Some(body) = build_udp(src_ip, dst_ip, local_port, dst_port, payload) else {
            return false;
        };
        if dst_ip.is_loopback() {
            self.transmit_loopback_ipv4(dst_ip, IP_PROTO_UDP, &body);
            return true;
        }

        let Some(dst_mac) = self.resolve_route_mac(dst_ip) else {
            return false;
        };
        self.transmit_ipv4(dst_mac, dst_ip, IP_PROTO_UDP, &body);
        true
    }

    fn send_icmp(&mut self, socket: IcmpSocketId, dst_ip: Ipv4Addr, payload: &[u8]) -> bool {
        if self.icmp_sockets.get(socket.0).is_none() || payload.len() < 4 {
            return false;
        }
        if dst_ip.is_loopback() {
            self.transmit_loopback_ipv4(dst_ip, IP_PROTO_ICMP, payload);
            return true;
        }

        let Some(dst_mac) = self.resolve_route_mac(dst_ip) else {
            return false;
        };
        self.transmit_ipv4(dst_mac, dst_ip, IP_PROTO_ICMP, payload);
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

    fn recv_icmp(&mut self, socket: IcmpSocketId) -> Option<IcmpDatagram> {
        let inbox = &mut self.icmp_sockets.get_mut(socket.0)?.inbox;
        if inbox.is_empty() {
            None
        } else {
            Some(inbox.remove(0))
        }
    }

    fn send_tcp(&mut self, mut outbound: tcp::TcpOutbound) -> bool {
        if outbound.segment.len() < 20 {
            return false;
        }
        let src_ip = if outbound.dst_ip.is_loopback() {
            LOOPBACK_IP
        } else {
            self.ip
        };
        outbound.segment[16] = 0;
        outbound.segment[17] = 0;
        let checksum = tcp::checksum(src_ip, outbound.dst_ip, &outbound.segment);
        outbound.segment[16] = (checksum >> 8) as u8;
        outbound.segment[17] = (checksum & 0xff) as u8;
        if outbound.dst_ip.is_loopback() {
            self.transmit_loopback_ipv4(outbound.dst_ip, IP_PROTO_TCP, &outbound.segment);
            return true;
        }

        let Some(dst_mac) = self.resolve_route_mac(outbound.dst_ip) else {
            return false;
        };
        self.transmit_ipv4(dst_mac, outbound.dst_ip, IP_PROTO_TCP, &outbound.segment);
        true
    }

    fn pop_tcp_inbound(&mut self) -> Option<tcp::TcpPacket> {
        if self.tcp_inbox.is_empty() {
            None
        } else {
            Some(self.tcp_inbox.remove(0))
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
        let accepts_dst = packet.dst == self.ip
            || packet.dst.is_loopback()
            || packet.dst.is_broadcast()
            || (self.ip.is_unspecified() && packet.protocol == IP_PROTO_UDP);
        if !accepts_dst {
            return;
        }

        match packet.protocol {
            IP_PROTO_ICMP => self.handle_icmp(frame.src, packet),
            IP_PROTO_TCP => self.handle_tcp(packet),
            IP_PROTO_UDP => self.handle_udp(packet),
            _ => {}
        }
    }

    fn handle_icmp(&mut self, dst_mac: MacAddr, packet: Ipv4Packet) {
        if packet.payload.len() < 4 {
            return;
        }

        match packet.payload[0] {
            ICMP_ECHO_REQUEST => {
                let mut reply = packet.payload;
                reply[0] = ICMP_ECHO_REPLY;
                reply[1] = 0;
                reply[2] = 0;
                reply[3] = 0;
                let checksum = internet_checksum(&reply);
                reply[2] = (checksum >> 8) as u8;
                reply[3] = (checksum & 0xff) as u8;
                self.transmit_ipv4(dst_mac, packet.src, IP_PROTO_ICMP, &reply);
            }
            ICMP_ECHO_REPLY => {
                for socket in &mut self.icmp_sockets {
                    socket.inbox.push(IcmpDatagram {
                        src: packet.src,
                        payload: packet.payload.clone(),
                    });
                }
                crate::process::wake_io_waiters();
            }
            _ => {}
        }
    }

    fn handle_udp(&mut self, packet: Ipv4Packet) {
        let Some(datagram) = parse_udp(packet.src, packet.dst, &packet.payload) else {
            return;
        };
        let Some(socket) = self
            .udp_sockets
            .iter_mut()
            .find(|socket| socket.local_port == datagram.dst_port)
        else {
            return;
        };
        socket.inbox.push(UdpDatagram {
            src: packet.src,
            src_port: datagram.src_port,
            payload: datagram.payload,
        });
        crate::process::wake_io_waiters();
    }

    fn handle_tcp(&mut self, packet: Ipv4Packet) {
        let Some(tcp_packet) = tcp::parse_tcp_packet(packet.src, packet.dst, &packet.payload)
        else {
            return;
        };
        self.tcp_inbox.push(tcp_packet);
        crate::process::wake_io_waiters();
    }

    fn transmit_ipv4(&mut self, dst_mac: MacAddr, dst_ip: Ipv4Addr, protocol: u8, body: &[u8]) {
        if dst_ip.is_loopback() {
            self.transmit_loopback_ipv4(dst_ip, protocol, body);
            return;
        }
        let payload = build_ipv4(protocol, self.ip, dst_ip, body);
        self.transmit(EthernetFrame {
            dst: dst_mac,
            src: self.mac,
            ethertype: ETHERTYPE_IPV4,
            payload,
        });
    }

    fn transmit_loopback_ipv4(&mut self, dst_ip: Ipv4Addr, protocol: u8, body: &[u8]) {
        self.tx_frames += 1;
        self.rx_frames += 1;
        let packet = Ipv4Packet {
            protocol,
            src: LOOPBACK_IP,
            dst: dst_ip,
            payload: Vec::from(body),
        };
        match protocol {
            IP_PROTO_ICMP => self.handle_icmp(self.mac, packet),
            IP_PROTO_TCP => self.handle_tcp(packet),
            IP_PROTO_UDP => self.handle_udp(packet),
            _ => {}
        }
    }

    fn cache_arp(&mut self, ip: Ipv4Addr, mac: MacAddr) {
        if let Some(entry) = self.arp_cache.iter_mut().find(|entry| entry.ip == ip) {
            entry.mac = mac;
            return;
        }
        self.arp_cache.push(ArpEntry { ip, mac });
    }

    fn resolve_mac(&self, ip: Ipv4Addr) -> Option<MacAddr> {
        if ip.is_broadcast() {
            return Some(MacAddr([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]));
        }
        self.arp_cache
            .iter()
            .find(|entry| entry.ip == ip)
            .map(|entry| entry.mac)
    }

    fn resolve_route_mac(&mut self, dst_ip: Ipv4Addr) -> Option<MacAddr> {
        let next_hop = self.next_hop_ip(dst_ip);
        if let Some(mac) = self.resolve_mac(next_hop) {
            return Some(mac);
        }

        self.send_arp_request(next_hop);
        self.resolve_mac(next_hop)
    }

    fn next_hop_ip(&self, dst_ip: Ipv4Addr) -> Ipv4Addr {
        if dst_ip.is_broadcast() || self.is_same_subnet(dst_ip) {
            return dst_ip;
        }
        self.gateway.unwrap_or(dst_ip)
    }

    fn is_same_subnet(&self, ip: Ipv4Addr) -> bool {
        let Some(mask) = self.subnet_mask else {
            return true;
        };
        if self.ip.is_unspecified() {
            return false;
        }
        (u32::from_be_bytes(self.ip.0) & u32::from_be_bytes(mask.0))
            == (u32::from_be_bytes(ip.0) & u32::from_be_bytes(mask.0))
    }

    fn send_arp_request(&mut self, target_ip: Ipv4Addr) {
        let payload = build_arp(ARP_REQUEST, self.mac, self.ip, MacAddr([0; 6]), target_ip);
        self.transmit(EthernetFrame {
            dst: MacAddr([0xff; 6]),
            src: self.mac,
            ethertype: ETHERTYPE_ARP,
            payload,
        });
    }

    fn begin_ipv4_conflict_probe(&mut self, ip: Ipv4Addr) {
        self.arp_cache.retain(|entry| entry.ip != ip);
        self.send_arp_request(ip);
    }

    fn ipv4_conflict_probe_result(&mut self, ip: Ipv4Addr) -> Option<MacAddr> {
        self.poll();
        self.resolve_mac(ip).filter(|mac| *mac != self.mac)
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

struct UdpPacket {
    src_port: u16,
    dst_port: u16,
    payload: Vec<u8>,
}

pub fn init() {
    socket::init();
    *NET_STACK.lock() = Some(runtime_stack());

    let use_dhcp = {
        let has_hw = NET_STACK
            .lock()
            .as_ref()
            .map(|stack| stack.device.is_hardware())
            .unwrap_or(false);
        if has_hw {
            !crate::boot_config::contains("ip=static")
                && crate::boot_config::value("ip")
                    .map(|val| val != "static")
                    .unwrap_or(true)
        } else {
            crate::boot_config::value("ip")
                .map(|val| val == "dhcp")
                .unwrap_or(false)
                || crate::boot_config::contains("ip=dhcp")
        }
    };

    if use_dhcp {
        crate::println!("DHCP: Initializing dynamic IP configuration...");
        if let Some(stack) = NET_STACK.lock().as_mut() {
            stack.dhcp_status = "in_progress";
        }
        if let Some(reply) = run_dhcp_client() {
            set_local_ip(reply.yiaddr);
            if let Some(stack) = NET_STACK.lock().as_mut() {
                stack.subnet_mask = reply.subnet_mask;
                stack.gateway = reply.router;
                stack.dns_server = reply.dns_server;
                stack.dhcp_status = "success";
            }
            if let Some(mask) = reply.subnet_mask {
                crate::println!(
                    "DHCP: Subnet mask {}.{}.{}.{}",
                    mask.0[0],
                    mask.0[1],
                    mask.0[2],
                    mask.0[3]
                );
            }
            if let Some(router) = reply.router {
                if let Some(gateway_mac) = resolve_gateway_mac(router) {
                    crate::println!(
                        "DHCP: Configured default gateway {}.{}.{}.{} at {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                        router.0[0],
                        router.0[1],
                        router.0[2],
                        router.0[3],
                        gateway_mac.0[0],
                        gateway_mac.0[1],
                        gateway_mac.0[2],
                        gateway_mac.0[3],
                        gateway_mac.0[4],
                        gateway_mac.0[5]
                    );
                } else {
                    crate::println!(
                        "DHCP: Configured default gateway {}.{}.{}.{} (MAC unresolved)",
                        router.0[0],
                        router.0[1],
                        router.0[2],
                        router.0[3]
                    );
                }
            }
            if let Some(dns) = reply.dns_server {
                crate::println!(
                    "DHCP: Configured DNS server {}.{}.{}.{}",
                    dns.0[0],
                    dns.0[1],
                    dns.0[2],
                    dns.0[3]
                );
            }
        } else {
            crate::println!(
                "DHCP: Dynamic configuration failed. Falling back to static IP {}.{}.{}.{}",
                LOCAL_IP.0[0],
                LOCAL_IP.0[1],
                LOCAL_IP.0[2],
                LOCAL_IP.0[3]
            );
            if let Some(stack) = NET_STACK.lock().as_mut() {
                stack.dhcp_status = "failed";
            }
            set_local_ip(LOCAL_IP);
        }
    } else {
        if let Some(stack) = NET_STACK.lock().as_mut() {
            stack.dhcp_status = "static";
        }
        crate::println!(
            "Network: Using static configuration IP {}.{}.{}.{}",
            LOCAL_IP.0[0],
            LOCAL_IP.0[1],
            LOCAL_IP.0[2],
            LOCAL_IP.0[3]
        );
    }

    socket::self_test();
    self_test();
    tcp::self_test();
}

pub fn local_ip() -> Ipv4Addr {
    NET_STACK
        .lock()
        .as_ref()
        .map(|stack| stack.ip)
        .unwrap_or(LOCAL_IP)
}

pub fn set_local_ip(ip: Ipv4Addr) {
    if let Some(stack) = NET_STACK.lock().as_mut() {
        stack.ip = ip;
        crate::println!(
            "Network IP set to {}.{}.{}.{}",
            ip.0[0],
            ip.0[1],
            ip.0[2],
            ip.0[3]
        );
    }
}

pub fn local_mac() -> MacAddr {
    NET_STACK
        .lock()
        .as_ref()
        .map(|stack| stack.mac)
        .unwrap_or(MacAddr([0, 0, 0, 0, 0, 0]))
}

pub fn subnet_mask() -> Option<Ipv4Addr> {
    NET_STACK
        .lock()
        .as_ref()
        .and_then(|stack| stack.subnet_mask)
}

pub fn gateway() -> Option<Ipv4Addr> {
    NET_STACK.lock().as_ref().and_then(|stack| stack.gateway)
}

pub fn dns_server() -> Option<Ipv4Addr> {
    NET_STACK.lock().as_ref().and_then(|stack| stack.dns_server)
}

pub fn dhcp_status() -> &'static str {
    NET_STACK
        .lock()
        .as_ref()
        .map(|stack| stack.dhcp_status)
        .unwrap_or("unknown")
}

pub fn stats() -> NetStats {
    *NET_STATS.lock()
}

pub fn poll_devices() {
    let received = {
        let mut guard = NET_STACK.lock();
        let Some(stack) = guard.as_mut() else {
            return;
        };
        let before = stack.rx_frames;
        stack.poll();
        let received = stack.rx_frames != before;
        if received {
            *NET_STATS.lock() = stack.stats();
        }
        received
    };
    if received {
        socket::drive_tcp();
        crate::process::wake_io_waiters();
    }
}

pub fn udp_bind(local_port: u16) -> Option<usize> {
    let socket = udp_socket_open(local_port)?;
    crate::println!(
        "UDP socket {} bound to local port {}.",
        socket.0,
        udp_socket_local_port(socket).unwrap_or(local_port)
    );
    Some(socket.0)
}

pub(crate) fn udp_socket_open(local_port: u16) -> Option<SocketId> {
    let mut guard = NET_STACK.lock();
    let stack = guard.as_mut()?;
    Some(stack.bind_udp(local_port))
}

pub(crate) fn udp_socket_bind(socket: SocketId, local_port: u16) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    stack.rebind_udp(socket, local_port)
}

pub(crate) fn udp_socket_close(socket: SocketId) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    stack.close_udp(socket)
}

pub(crate) fn udp_socket_local_port(socket: SocketId) -> Option<u16> {
    let guard = NET_STACK.lock();
    guard
        .as_ref()?
        .udp_sockets
        .get(socket.0)
        .map(|socket| socket.local_port)
}

pub(crate) fn udp_socket_send(
    socket: SocketId,
    dst_ip: Ipv4Addr,
    dst_port: u16,
    payload: &[u8],
) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    if stack.udp_sockets.get(socket.0).is_none() {
        return false;
    }
    stack.send_udp(socket, dst_ip, dst_port, payload)
}

pub(crate) fn udp_socket_recv(socket: SocketId) -> Option<UdpDatagram> {
    let mut guard = NET_STACK.lock();
    let stack = guard.as_mut()?;
    stack.poll();
    stack.recv_udp(socket)
}

pub(crate) fn udp_socket_readable(socket: SocketId) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    stack.poll();
    stack
        .udp_sockets
        .get(socket.0)
        .map(|socket| !socket.inbox.is_empty())
        .unwrap_or(false)
}

pub(crate) fn icmp_socket_open() -> Option<IcmpSocketId> {
    let mut guard = NET_STACK.lock();
    let stack = guard.as_mut()?;
    Some(stack.open_icmp())
}

pub(crate) fn icmp_socket_close(socket: IcmpSocketId) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    stack.close_icmp(socket)
}

pub(crate) fn icmp_socket_send(socket: IcmpSocketId, dst_ip: Ipv4Addr, payload: &[u8]) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    stack.send_icmp(socket, dst_ip, payload)
}

pub(crate) fn icmp_socket_recv(socket: IcmpSocketId) -> Option<IcmpDatagram> {
    let mut guard = NET_STACK.lock();
    let stack = guard.as_mut()?;
    stack.poll();
    stack.recv_icmp(socket)
}

pub(crate) fn icmp_socket_readable(socket: IcmpSocketId) -> bool {
    let mut guard = NET_STACK.lock();
    let Some(stack) = guard.as_mut() else {
        return false;
    };
    stack.poll();
    stack
        .icmp_sockets
        .get(socket.0)
        .map(|socket| !socket.inbox.is_empty())
        .unwrap_or(false)
}

pub fn udp_send(socket: usize, dst_ip: [u8; 4], dst_port: u16, payload: &[u8]) -> bool {
    let socket = SocketId(socket);
    let dst_ip = Ipv4Addr(dst_ip);
    if !udp_socket_send(socket, dst_ip, dst_port, payload) {
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
    let datagram = udp_socket_recv(SocketId(socket))?;
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

pub fn drive_tcp(tcp_stack: &mut tcp::TcpStack) -> bool {
    let mut made_progress = false;
    let mut rounds = 0usize;
    loop {
        rounds += 1;
        if tcp_stack.poll_retransmit(crate::time::monotonic_ticks()) {
            made_progress = true;
        }
        let mut sent_outbound = false;
        {
            let mut guard = NET_STACK.lock();
            let Some(stack) = guard.as_mut() else {
                return made_progress;
            };
            stack.poll();
            while let Some(packet) = stack.pop_tcp_inbound() {
                if tcp_stack.handle_packet(packet) {
                    made_progress = true;
                }
            }
            while let Some(outbound) = tcp_stack.pop_outbound() {
                if stack.send_tcp(outbound) {
                    made_progress = true;
                    sent_outbound = true;
                }
            }
            if sent_outbound {
                stack.poll();
                while let Some(packet) = stack.pop_tcp_inbound() {
                    if tcp_stack.handle_packet(packet) {
                        made_progress = true;
                    }
                }
            }
        }
        if !sent_outbound || rounds >= 4 {
            break;
        }
    }
    made_progress
}

fn runtime_stack() -> NetworkStack {
    let device = VirtioNetDriver::probe().unwrap_or_else(VirtioNetDriver::software_fallback);
    let peer_ip = Ipv4Addr([10, 0, 2, 2]);
    let peer_mac = MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let mut stack = NetworkStack::new(device, LOCAL_IP);
    stack.subnet_mask = Some(Ipv4Addr([255, 255, 255, 0]));
    stack.gateway = Some(peer_ip);
    stack.dns_server = Some(peer_ip);
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
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    payload.push(6);
    payload.push(4);
    payload.extend_from_slice(&opcode.to_be_bytes());
    payload.extend_from_slice(&sender_mac.0);
    payload.extend_from_slice(&sender_ip.0);
    payload.extend_from_slice(&target_mac.0);
    payload.extend_from_slice(&target_ip.0);
    payload
}

fn parse_arp(payload: &[u8]) -> Option<ArpPacket> {
    if payload.len() < 28 {
        return None;
    }
    let htype = u16::from_be_bytes([payload[0], payload[1]]);
    let ptype = u16::from_be_bytes([payload[2], payload[3]]);
    if htype != 1 || ptype != ETHERTYPE_IPV4 || payload[4] != 6 || payload[5] != 4 {
        return None;
    }
    Some(ArpPacket {
        opcode: u16::from_be_bytes([payload[6], payload[7]]),
        sender_mac: MacAddr(payload[8..14].try_into().ok()?),
        sender_ip: Ipv4Addr(payload[14..18].try_into().ok()?),
        target_ip: Ipv4Addr(payload[24..28].try_into().ok()?),
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

fn parse_udp(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, payload: &[u8]) -> Option<UdpPacket> {
    if payload.len() < UDP_HEADER_LEN {
        return None;
    }
    let udp_len = u16::from_be_bytes([payload[4], payload[5]]) as usize;
    if udp_len < UDP_HEADER_LEN || udp_len > payload.len() {
        return None;
    }
    let datagram = &payload[..udp_len];
    let checksum = u16::from_be_bytes([datagram[6], datagram[7]]);
    if checksum != 0 && udp_checksum(src_ip, dst_ip, datagram) != 0 {
        return None;
    }

    Some(UdpPacket {
        src_port: u16::from_be_bytes([datagram[0], datagram[1]]),
        dst_port: u16::from_be_bytes([datagram[2], datagram[3]]),
        payload: Vec::from(&datagram[UDP_HEADER_LEN..]),
    })
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

fn parse_ipv4(payload: &[u8]) -> Option<Ipv4Packet> {
    if payload.len() < 20 || payload[0] >> 4 != 4 {
        return None;
    }
    let ihl = (payload[0] & 0x0f) as usize * 4;
    let total_len = u16::from_be_bytes([payload[2], payload[3]]) as usize;
    if payload.len() < ihl || total_len < ihl || payload.len() < total_len {
        return None;
    }
    Some(Ipv4Packet {
        protocol: payload[9],
        src: Ipv4Addr(payload[12..16].try_into().ok()?),
        dst: Ipv4Addr(payload[16..20].try_into().ok()?),
        payload: Vec::from(&payload[ihl..total_len]),
    })
}

pub(crate) struct DhcpReply {
    message_type: u8,
    yiaddr: Ipv4Addr,
    subnet_mask: Option<Ipv4Addr>,
    router: Option<Ipv4Addr>,
    dns_server: Option<Ipv4Addr>,
    server_id: Option<Ipv4Addr>,
}

fn parse_dhcp_reply(payload: &[u8], expected_xid: u32) -> Option<DhcpReply> {
    if payload.len() < 240 {
        return None;
    }
    if payload[0] != 2 {
        // Boot Reply
        return None;
    }
    let xid = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
    if xid != expected_xid {
        return None;
    }
    if payload[236..240] != [99, 130, 83, 99] {
        return None;
    }

    let yiaddr = Ipv4Addr([payload[16], payload[17], payload[18], payload[19]]);
    let mut message_type = 0;
    let mut subnet_mask = None;
    let mut router = None;
    let mut dns_server = None;
    let mut server_id = None;

    let mut offset = 240;
    while offset < payload.len() {
        let opt_type = payload[offset];
        if opt_type == 255 {
            break;
        }
        if opt_type == 0 {
            // Pad
            offset += 1;
            continue;
        }
        if offset + 1 >= payload.len() {
            break;
        }
        let opt_len = payload[offset + 1] as usize;
        if offset + 2 + opt_len > payload.len() {
            break;
        }
        let opt_val = &payload[offset + 2..offset + 2 + opt_len];
        match opt_type {
            53 => {
                if opt_len == 1 {
                    message_type = opt_val[0];
                }
            }
            1 => {
                if opt_len == 4 {
                    subnet_mask = Some(Ipv4Addr([opt_val[0], opt_val[1], opt_val[2], opt_val[3]]));
                }
            }
            3 => {
                if opt_len >= 4 {
                    router = Some(Ipv4Addr([opt_val[0], opt_val[1], opt_val[2], opt_val[3]]));
                }
            }
            6 => {
                if opt_len >= 4 {
                    dns_server = Some(Ipv4Addr([opt_val[0], opt_val[1], opt_val[2], opt_val[3]]));
                }
            }
            54 => {
                if opt_len == 4 {
                    server_id = Some(Ipv4Addr([opt_val[0], opt_val[1], opt_val[2], opt_val[3]]));
                }
            }
            _ => {}
        }
        offset += 2 + opt_len;
    }

    Some(DhcpReply {
        message_type,
        yiaddr,
        subnet_mask,
        router,
        dns_server,
        server_id,
    })
}

fn build_dhcp_discover(xid: u32, mac: MacAddr) -> Vec<u8> {
    let mut packet = Vec::new();
    packet.resize(240, 0);
    packet[0] = 1; // Boot Request
    packet[1] = 1; // Ethernet
    packet[2] = 6; // Hardware address length
    packet[4..8].copy_from_slice(&xid.to_be_bytes());
    packet[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // Broadcast flag
    packet[28..34].copy_from_slice(&mac.0); // Client MAC

    // Magic cookie
    packet[236..240].copy_from_slice(&[99, 130, 83, 99]);

    // Option 53: DHCP Message Type (DHCPDISCOVER)
    packet.extend_from_slice(&[53, 1, 1]);

    append_dhcp_client_identity(&mut packet, mac);

    // Option 55: Parameter Request List (Subnet Mask, Router, DNS)
    packet.extend_from_slice(&[55, 3, 1, 3, 6]);

    // Option 255: End
    packet.push(255);

    packet
}

fn build_dhcp_request(
    xid: u32,
    mac: MacAddr,
    requested_ip: Ipv4Addr,
    server_ip: Ipv4Addr,
) -> Vec<u8> {
    let mut packet = Vec::new();
    packet.resize(240, 0);
    packet[0] = 1; // Boot Request
    packet[1] = 1; // Ethernet
    packet[2] = 6; // Hardware address length
    packet[4..8].copy_from_slice(&xid.to_be_bytes());
    packet[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // Broadcast flag
    packet[28..34].copy_from_slice(&mac.0); // Client MAC

    // Magic cookie
    packet[236..240].copy_from_slice(&[99, 130, 83, 99]);

    // Option 53: DHCP Message Type (DHCPREQUEST)
    packet.extend_from_slice(&[53, 1, 3]);

    append_dhcp_client_identity(&mut packet, mac);

    // Option 50: Requested IP
    packet.extend_from_slice(&[50, 4]);
    packet.extend_from_slice(&requested_ip.0);

    // Option 54: Server Identifier
    packet.extend_from_slice(&[54, 4]);
    packet.extend_from_slice(&server_ip.0);

    // Option 255: End
    packet.push(255);

    packet
}

fn build_dhcp_decline(
    xid: u32,
    mac: MacAddr,
    declined_ip: Ipv4Addr,
    server_ip: Ipv4Addr,
) -> Vec<u8> {
    let mut packet = Vec::new();
    packet.resize(240, 0);
    packet[0] = 1; // Boot Request
    packet[1] = 1; // Ethernet
    packet[2] = 6; // Hardware address length
    packet[4..8].copy_from_slice(&xid.to_be_bytes());
    packet[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // Broadcast flag
    packet[28..34].copy_from_slice(&mac.0); // Client MAC

    packet[236..240].copy_from_slice(&[99, 130, 83, 99]);
    packet.extend_from_slice(&[53, 1, 4]); // DHCPDECLINE
    append_dhcp_client_identity(&mut packet, mac);
    packet.extend_from_slice(&[50, 4]);
    packet.extend_from_slice(&declined_ip.0);
    packet.extend_from_slice(&[54, 4]);
    packet.extend_from_slice(&server_ip.0);
    packet.push(255);
    packet
}

fn append_dhcp_client_identity(packet: &mut Vec<u8>, mac: MacAddr) {
    // Option 61: client identifier. Use Ethernet hardware type plus the VM MAC,
    // so DHCP servers do not accidentally bucket us with the host client.
    packet.extend_from_slice(&[61, 7, 1]);
    packet.extend_from_slice(&mac.0);

    // Option 12: host name.
    packet.extend_from_slice(&[12, 6]);
    packet.extend_from_slice(b"ristux");
}

fn dhcp_xid(mac: MacAddr) -> u32 {
    let tsc = rdtsc() as u32;
    let mac_mix = ((mac.0[2] as u32) << 24)
        | ((mac.0[3] as u32) << 16)
        | ((mac.0[4] as u32) << 8)
        | mac.0[5] as u32;
    0x3903_f39f ^ tsc ^ mac_mix
}

fn rdtsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}

pub(crate) fn run_dhcp_client() -> Option<DhcpReply> {
    let mac = {
        let guard = NET_STACK.lock();
        let Some(stack) = guard.as_ref() else {
            return None;
        };
        stack.mac
    };

    let socket = udp_socket_open(68)?;
    let xid = dhcp_xid(mac);
    let discover = build_dhcp_discover(xid, mac);

    crate::println!("DHCP: Sending DHCPDISCOVER...");
    set_local_ip(Ipv4Addr([0, 0, 0, 0]));

    if !udp_socket_send(socket, Ipv4Addr([255, 255, 255, 255]), 67, &discover) {
        crate::println!("DHCP: Failed to send DHCPDISCOVER");
        udp_socket_close(socket);
        return None;
    }

    let start_tick = crate::time::monotonic_ticks();
    let timeout_ticks = 150; // 1.5 seconds at 100Hz
    let start_tsc = rdtsc();
    let timeout_tsc = 6_000_000_000; // ~2.0 seconds at 3GHz
    let mut offer = None;

    loop {
        let ticks_elapsed = crate::time::monotonic_ticks() - start_tick;
        if ticks_elapsed > timeout_ticks
            || (ticks_elapsed == 0 && rdtsc() - start_tsc > timeout_tsc)
        {
            break;
        }
        if let Some(datagram) = udp_socket_recv(socket) {
            if let Some(mut reply) = parse_dhcp_reply(&datagram.payload, xid) {
                if reply.message_type == 2 {
                    // DHCPOFFER
                    if reply.server_id.is_none() {
                        reply.server_id = Some(datagram.src);
                    }
                    crate::println!(
                        "DHCP: Received DHCPOFFER for IP {}.{}.{}.{}",
                        reply.yiaddr.0[0],
                        reply.yiaddr.0[1],
                        reply.yiaddr.0[2],
                        reply.yiaddr.0[3]
                    );
                    offer = Some(reply);
                    break;
                }
            }
        }
        core::hint::spin_loop();
    }

    let Some(offer) = offer else {
        crate::println!("DHCP: Timeout waiting for DHCPOFFER");
        udp_socket_close(socket);
        return None;
    };

    let Some(server_id) = offer.server_id else {
        crate::println!("DHCP: DHCPOFFER did not identify a server");
        udp_socket_close(socket);
        return None;
    };
    let request = build_dhcp_request(xid, mac, offer.yiaddr, server_id);

    crate::println!("DHCP: Sending DHCPREQUEST...");
    if !udp_socket_send(socket, Ipv4Addr([255, 255, 255, 255]), 67, &request) {
        crate::println!("DHCP: Failed to send DHCPREQUEST");
        udp_socket_close(socket);
        return None;
    }

    let start_tick = crate::time::monotonic_ticks();
    let start_tsc = rdtsc();
    let mut ack = None;

    loop {
        let ticks_elapsed = crate::time::monotonic_ticks() - start_tick;
        if ticks_elapsed > timeout_ticks
            || (ticks_elapsed == 0 && rdtsc() - start_tsc > timeout_tsc)
        {
            break;
        }
        if let Some(datagram) = udp_socket_recv(socket) {
            if let Some(reply) = parse_dhcp_reply(&datagram.payload, xid) {
                if reply.message_type == 5 {
                    // DHCPACK
                    crate::println!("DHCP: Received DHCPACK");
                    ack = Some(reply);
                    break;
                }
            }
        }
        core::hint::spin_loop();
    }

    if let Some(reply) = ack.as_ref() {
        let conflict = probe_ipv4_conflict(reply.yiaddr);
        if let Some(conflict_mac) = conflict {
            crate::println!(
                "DHCP: Offered IP {}.{}.{}.{} is already in use by {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}; declining lease",
                reply.yiaddr.0[0],
                reply.yiaddr.0[1],
                reply.yiaddr.0[2],
                reply.yiaddr.0[3],
                conflict_mac.0[0],
                conflict_mac.0[1],
                conflict_mac.0[2],
                conflict_mac.0[3],
                conflict_mac.0[4],
                conflict_mac.0[5]
            );
            let decline = build_dhcp_decline(xid, mac, reply.yiaddr, server_id);
            let _ = udp_socket_send(socket, Ipv4Addr([255, 255, 255, 255]), 67, &decline);
            udp_socket_close(socket);
            return None;
        }
    }

    udp_socket_close(socket);
    ack
}

pub fn resolve_gateway_mac(gateway_ip: Ipv4Addr) -> Option<MacAddr> {
    let start_tick = crate::time::monotonic_ticks();
    let start_tsc = rdtsc();
    let mut requested = false;
    loop {
        {
            let mut guard = NET_STACK.lock();
            let stack = guard.as_mut()?;
            let next_hop = stack.next_hop_ip(gateway_ip);
            if let Some(mac) = stack.resolve_mac(next_hop) {
                return Some(mac);
            }
            if !requested {
                stack.send_arp_request(next_hop);
                requested = true;
            }
            stack.poll();
            if let Some(mac) = stack.resolve_mac(next_hop) {
                return Some(mac);
            }
        }

        let ticks_elapsed = crate::time::monotonic_ticks() - start_tick;
        if ticks_elapsed > 20 || (ticks_elapsed == 0 && rdtsc() - start_tsc > 600_000_000) {
            return None;
        }
        core::hint::spin_loop();
    }
}

fn probe_ipv4_conflict(ip: Ipv4Addr) -> Option<MacAddr> {
    {
        let mut guard = NET_STACK.lock();
        guard.as_mut()?.begin_ipv4_conflict_probe(ip);
    }

    let start_tick = crate::time::monotonic_ticks();
    let start_tsc = rdtsc();
    loop {
        {
            let mut guard = NET_STACK.lock();
            if let Some(conflict) = guard.as_mut()?.ipv4_conflict_probe_result(ip) {
                return Some(conflict);
            }
        }

        let ticks_elapsed = crate::time::monotonic_ticks() - start_tick;
        if ticks_elapsed > 50 || (ticks_elapsed == 0 && rdtsc() - start_tsc > 1_500_000_000) {
            return None;
        }
        core::hint::spin_loop();
    }
}

fn self_test() {
    let peer_mac = MacAddr([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let peer_ip = Ipv4Addr([10, 0, 2, 2]);
    let mut stack = NetworkStack::new(VirtioNetDriver::software_fallback(), LOCAL_IP);
    let local_mac = stack.mac;

    stack.inject_rx(EthernetFrame {
        dst: MacAddr([0xff; 6]),
        src: peer_mac,
        ethertype: ETHERTYPE_ARP,
        payload: build_arp(ARP_REQUEST, peer_mac, peer_ip, MacAddr([0; 6]), LOCAL_IP),
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
        payload: build_ipv4(IP_PROTO_ICMP, peer_ip, LOCAL_IP, &echo_body),
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
    let udp_segment =
        parse_udp(LOCAL_IP, peer_ip, &udp_packet.payload).expect("UDP transmit was malformed");
    if udp_tx.dst != peer_mac
        || udp_packet.protocol != IP_PROTO_UDP
        || udp_segment.src_port != 9000
        || udp_segment.dst_port != 9001
        || udp_segment.payload != b"ristux"
    {
        panic!("UDP send self-test failed");
    }

    let received = stack
        .recv_udp(socket)
        .expect("UDP socket inbox stayed empty");
    if received.src != peer_ip || received.src_port != 9001 || received.payload != b"udp-reply" {
        panic!("UDP receive self-test failed");
    }

    let loop_socket = stack.bind_udp(9100);
    if !stack.send_udp(loop_socket, LOOPBACK_IP, 9100, b"loopback") {
        panic!("UDP loopback send failed");
    }
    let loop_datagram = stack
        .recv_udp(loop_socket)
        .expect("UDP loopback inbox stayed empty");
    if loop_datagram.src != LOOPBACK_IP
        || loop_datagram.src_port != 9100
        || loop_datagram.payload != b"loopback"
    {
        panic!("UDP loopback self-test failed");
    }

    *NET_STATS.lock() = stack.stats();
    crate::println!(
        "Networking self-test passed: VirtIO net, loopback, ARP, IPv4, ICMP, UDP sockets."
    );
}
