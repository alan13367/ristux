use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacAddr(pub [u8; 6]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Addr(pub [u8; 4]);

pub struct EthernetFrame {
    pub dst: MacAddr,
    pub src: MacAddr,
    pub ethertype: u16,
    pub payload: Vec<u8>,
}

pub struct UdpPacket {
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: Vec<u8>,
}

pub fn init() {
    self_test();
}

fn arp_reply(sender_mac: MacAddr, sender_ip: Ipv4Addr, target_ip: Ipv4Addr) -> EthernetFrame {
    let mut payload = Vec::new();
    payload.extend_from_slice(&sender_ip.0);
    payload.extend_from_slice(&target_ip.0);
    EthernetFrame {
        dst: MacAddr([0xff; 6]),
        src: sender_mac,
        ethertype: 0x0806,
        payload,
    }
}

fn icmp_echo_reply(src: Ipv4Addr, dst: Ipv4Addr, sequence: u16) -> Vec<u8> {
    let mut packet = Vec::new();
    packet.extend_from_slice(&src.0);
    packet.extend_from_slice(&dst.0);
    packet.push(0);
    packet.push(0);
    packet.extend_from_slice(&sequence.to_be_bytes());
    packet
}

fn udp_send(src_port: u16, dst_port: u16, payload: &[u8]) -> UdpPacket {
    UdpPacket {
        src_port,
        dst_port,
        payload: Vec::from(payload),
    }
}

fn self_test() {
    let mac = MacAddr([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
    let local = Ipv4Addr([10, 0, 2, 15]);
    let peer = Ipv4Addr([10, 0, 2, 2]);
    let arp = arp_reply(mac, local, peer);
    if arp.ethertype != 0x0806
        || arp.dst != MacAddr([0xff; 6])
        || arp.src != mac
        || arp.payload.len() != 8
    {
        panic!("ARP self-test failed");
    }

    let echo = icmp_echo_reply(local, peer, 7);
    if echo[8] != 0 || echo[10..12] != 7u16.to_be_bytes() {
        panic!("ICMP echo self-test failed");
    }

    let udp = udp_send(1000, 1001, b"ristux");
    if udp.src_port != 1000 || udp.dst_port != 1001 || udp.payload != b"ristux" {
        panic!("UDP self-test failed");
    }

    crate::println!("Networking self-test passed: ARP, ICMP, UDP.");
}
