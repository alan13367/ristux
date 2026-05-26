//! Small packet-driven TCP layer for the in-tree IPv4 stack.
//! Congestion control, options, retransmits, and passive open are deferred.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::net::Ipv4Addr;

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;

const TCP_RETRANSMIT_TICKS: u64 = 25;
const TCP_MAX_RETRANSMITS: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    TimeWait,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpError {
    NotConnected,
    WouldBlock,
    InvalidState,
}

#[derive(Clone, Debug)]
pub struct TcpPacket {
    pub src_ip: Ipv4Addr,
    pub dst_ip: Ipv4Addr,
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct TcpOutbound {
    pub dst_ip: Ipv4Addr,
    pub segment: Vec<u8>,
}

struct TcpRetransmit {
    dst_ip: Ipv4Addr,
    local_port: u16,
    remote_port: u16,
    end_seq: u32,
    segment: Vec<u8>,
    deadline_tick: u64,
    attempts: u8,
}

#[derive(Clone)]
pub struct TcpSocket {
    pub local_port: u16,
    pub remote_ip: Ipv4Addr,
    pub remote_port: u16,
    pub state: TcpState,
    pub seq: u32,
    pub ack: u32,
    pub window: u16,
    pub rx_buffer: VecDeque<u8>,
}

impl TcpSocket {
    pub fn new(local_port: u16) -> Self {
        Self {
            local_port,
            remote_ip: Ipv4Addr([0, 0, 0, 0]),
            remote_port: 0,
            state: TcpState::Closed,
            seq: 1,
            ack: 0,
            window: 4096,
            rx_buffer: VecDeque::new(),
        }
    }

    pub fn listen(&mut self) {
        self.state = TcpState::Listen;
    }

    fn established(&self) -> bool {
        self.state == TcpState::Established
    }

    fn matches_packet(&self, packet: &TcpPacket) -> bool {
        self.local_port == packet.dst_port
            && self.remote_port == packet.src_port
            && self.remote_ip == packet.src_ip
    }
}

pub struct TcpStack {
    pub sockets: Vec<TcpSocket>,
    pub syn_queue: Vec<TcpSocket>,
    pub accept_queue: Vec<TcpSocket>,
    pending_outbound: VecDeque<TcpOutbound>,
    retransmits: Vec<TcpRetransmit>,
}

impl TcpStack {
    pub fn new() -> Self {
        Self {
            sockets: Vec::new(),
            syn_queue: Vec::new(),
            accept_queue: Vec::new(),
            pending_outbound: VecDeque::new(),
            retransmits: Vec::new(),
        }
    }

    pub fn bind(&mut self, local_port: u16) -> usize {
        let mut socket = TcpSocket::new(local_port);
        socket.listen();
        self.sockets.push(socket);
        self.sockets.len() - 1
    }

    pub fn open(&mut self) -> usize {
        let local_port = 49152u16.wrapping_add(self.sockets.len() as u16);
        self.sockets.push(TcpSocket::new(local_port));
        self.sockets.len() - 1
    }

    pub fn bind_existing(&mut self, socket: usize, local_port: u16) -> Result<(), TcpError> {
        let socket = self.sockets.get_mut(socket).ok_or(TcpError::InvalidState)?;
        socket.local_port = local_port;
        Ok(())
    }

    pub fn listen(&mut self, socket: usize) -> Result<(), TcpError> {
        let socket = self.sockets.get_mut(socket).ok_or(TcpError::InvalidState)?;
        socket.listen();
        Ok(())
    }

    pub fn connect(
        &mut self,
        socket: usize,
        remote_ip: Ipv4Addr,
        remote_port: u16,
    ) -> Result<(), TcpError> {
        let (outbound, retransmit) = {
            let socket = self.sockets.get_mut(socket).ok_or(TcpError::InvalidState)?;
            socket.remote_ip = remote_ip;
            socket.remote_port = remote_port;
            socket.state = TcpState::SynSent;
            let seq = socket.seq;
            let outbound = build_outbound(socket, TCP_FLAG_SYN, &[]);
            let span = tcp_sequence_span(TCP_FLAG_SYN, 0);
            let retransmit = build_retransmit(socket, &outbound.segment, seq, span);
            socket.seq = socket.seq.wrapping_add(span);
            (outbound, retransmit)
        };
        self.queue_outbound(outbound, retransmit);
        Ok(())
    }

    pub fn accept(&mut self, socket: usize) -> Result<usize, TcpError> {
        let listener = self.sockets.get_mut(socket).ok_or(TcpError::InvalidState)?;
        if listener.state != TcpState::Listen {
            return Err(TcpError::InvalidState);
        }
        let Some(peer) = self.accept_queue.pop() else {
            return Err(TcpError::WouldBlock);
        };
        self.sockets.push(peer);
        Ok(self.sockets.len() - 1)
    }

    pub fn send(&mut self, socket: usize, data: &[u8]) -> Result<usize, TcpError> {
        let (outbound, retransmit) = {
            let socket = self.sockets.get_mut(socket).ok_or(TcpError::InvalidState)?;
            if !socket.established() {
                return Err(TcpError::NotConnected);
            }
            let seq = socket.seq;
            let outbound = build_outbound(socket, TCP_FLAG_ACK | TCP_FLAG_PSH, data);
            let span = tcp_sequence_span(TCP_FLAG_ACK | TCP_FLAG_PSH, data.len());
            let retransmit = build_retransmit(socket, &outbound.segment, seq, span);
            socket.seq = socket.seq.wrapping_add(span);
            (outbound, retransmit)
        };
        self.queue_outbound(outbound, retransmit);
        Ok(data.len())
    }

    pub fn recv(&mut self, socket: usize, buf: &mut [u8]) -> Result<usize, TcpError> {
        let socket = self.sockets.get_mut(socket).ok_or(TcpError::InvalidState)?;
        if !socket.established() && socket.rx_buffer.is_empty() {
            return Err(TcpError::NotConnected);
        }
        if socket.rx_buffer.is_empty() {
            return Err(TcpError::WouldBlock);
        }
        let mut read = 0;
        for slot in buf.iter_mut() {
            let Some(byte) = socket.rx_buffer.pop_front() else {
                break;
            };
            *slot = byte;
            read += 1;
        }
        Ok(read)
    }

    pub fn close(&mut self, socket: usize) -> Result<(), TcpError> {
        let (outbound, retransmit) = {
            let socket = self.sockets.get_mut(socket).ok_or(TcpError::InvalidState)?;
            if !socket.established() {
                socket.state = TcpState::Closed;
                return Ok(());
            }
            socket.state = TcpState::FinWait1;
            let seq = socket.seq;
            let outbound = build_outbound(socket, TCP_FLAG_ACK | TCP_FLAG_FIN, &[]);
            let span = tcp_sequence_span(TCP_FLAG_ACK | TCP_FLAG_FIN, 0);
            let retransmit = build_retransmit(socket, &outbound.segment, seq, span);
            socket.seq = socket.seq.wrapping_add(span);
            (outbound, retransmit)
        };
        self.queue_outbound(outbound, retransmit);
        Ok(())
    }

    pub fn handle_packet(&mut self, packet: TcpPacket) -> bool {
        if packet.flags & TCP_FLAG_ACK != 0 {
            self.acknowledge(packet.src_ip, packet.src_port, packet.dst_port, packet.ack);
        }

        let Some(index) = self
            .sockets
            .iter()
            .position(|socket| socket.matches_packet(&packet))
            .or_else(|| {
                self.sockets.iter().position(|socket| {
                    socket.state == TcpState::Listen && socket.local_port == packet.dst_port
                })
            })
        else {
            return false;
        };

        let mut outbound = None;
        let socket = &mut self.sockets[index];
        match socket.state {
            TcpState::SynSent => {
                if packet.flags & (TCP_FLAG_SYN | TCP_FLAG_ACK) == (TCP_FLAG_SYN | TCP_FLAG_ACK) {
                    socket.ack = packet.seq.wrapping_add(1);
                    socket.seq = packet.ack;
                    socket.state = TcpState::Established;
                    outbound = Some(build_outbound(socket, TCP_FLAG_ACK, &[]));
                }
            }
            TcpState::Established => {
                if !packet.payload.is_empty() {
                    socket.ack = packet.seq.wrapping_add(packet.payload.len() as u32);
                    socket.rx_buffer.extend(packet.payload.iter().copied());
                    outbound = Some(build_outbound(socket, TCP_FLAG_ACK, &[]));
                } else if packet.flags & TCP_FLAG_FIN != 0 {
                    socket.ack = packet.seq.wrapping_add(1);
                    socket.state = TcpState::TimeWait;
                    outbound = Some(build_outbound(socket, TCP_FLAG_ACK, &[]));
                }
            }
            TcpState::Listen => {
                if packet.flags & TCP_FLAG_SYN != 0 {
                    let mut peer = TcpSocket::new(socket.local_port);
                    peer.remote_ip = packet.src_ip;
                    peer.remote_port = packet.src_port;
                    peer.state = TcpState::Established;
                    peer.seq = 1;
                    peer.ack = packet.seq.wrapping_add(1);
                    outbound = Some(build_outbound(&peer, TCP_FLAG_SYN | TCP_FLAG_ACK, &[]));
                    peer.seq = peer.seq.wrapping_add(1);
                    self.accept_queue.push(peer);
                }
            }
            _ => {}
        }

        if let Some(outbound) = outbound {
            self.pending_outbound.push_back(outbound);
        }
        true
    }

    pub fn poll_retransmit(&mut self, now_tick: u64) -> bool {
        let mut retransmitted = false;
        for entry in self.retransmits.iter_mut() {
            if entry.deadline_tick > now_tick || entry.attempts >= TCP_MAX_RETRANSMITS {
                continue;
            }
            entry.deadline_tick = now_tick.saturating_add(TCP_RETRANSMIT_TICKS);
            entry.attempts = entry.attempts.saturating_add(1);
            self.pending_outbound.push_back(TcpOutbound {
                dst_ip: entry.dst_ip,
                segment: entry.segment.clone(),
            });
            retransmitted = true;
        }
        retransmitted
    }

    pub fn pop_outbound(&mut self) -> Option<TcpOutbound> {
        self.pending_outbound.pop_front()
    }

    pub fn has_pending_accept(&self) -> bool {
        !self.accept_queue.is_empty()
    }

    pub fn established(&self, socket: usize) -> bool {
        self.sockets
            .get(socket)
            .map(|socket| socket.established())
            .unwrap_or(false)
    }

    pub fn local_port(&self, socket: usize) -> Option<u16> {
        self.sockets.get(socket).map(|socket| socket.local_port)
    }

    pub fn peer_addr(&self, socket: usize) -> Option<(Ipv4Addr, u16)> {
        let socket = self.sockets.get(socket)?;
        if socket.remote_port == 0 {
            None
        } else {
            Some((socket.remote_ip, socket.remote_port))
        }
    }

    pub fn stats(&self) -> TcpStats {
        TcpStats {
            sockets: self.sockets.len(),
            established: self.sockets.iter().filter(|s| s.established()).count(),
            listen: self
                .sockets
                .iter()
                .filter(|s| s.state == TcpState::Listen)
                .count(),
        }
    }

    fn queue_outbound(&mut self, outbound: TcpOutbound, retransmit: Option<TcpRetransmit>) {
        if let Some(retransmit) = retransmit {
            self.retransmits.push(retransmit);
        }
        self.pending_outbound.push_back(outbound);
    }

    fn acknowledge(&mut self, remote_ip: Ipv4Addr, remote_port: u16, local_port: u16, ack: u32) {
        let mut pending = Vec::new();
        for entry in self.retransmits.drain(..) {
            let matches = entry.dst_ip == remote_ip
                && entry.remote_port == remote_port
                && entry.local_port == local_port;
            if matches && entry.end_seq <= ack {
                continue;
            }
            pending.push(entry);
        }
        self.retransmits = pending;
    }
}

fn build_outbound(socket: &TcpSocket, flags: u8, payload: &[u8]) -> TcpOutbound {
    TcpOutbound {
        dst_ip: socket.remote_ip,
        segment: build_tcp_segment(socket, flags, payload),
    }
}

fn build_retransmit(
    socket: &TcpSocket,
    segment: &[u8],
    seq: u32,
    span: u32,
) -> Option<TcpRetransmit> {
    if span == 0 {
        return None;
    }
    Some(TcpRetransmit {
        dst_ip: socket.remote_ip,
        local_port: socket.local_port,
        remote_port: socket.remote_port,
        end_seq: seq.wrapping_add(span),
        segment: Vec::from(segment),
        deadline_tick: crate::time::monotonic_ticks().saturating_add(TCP_RETRANSMIT_TICKS),
        attempts: 0,
    })
}

fn tcp_sequence_span(flags: u8, payload_len: usize) -> u32 {
    let control = u32::from(flags & TCP_FLAG_SYN != 0) + u32::from(flags & TCP_FLAG_FIN != 0);
    payload_len as u32 + control
}

#[derive(Clone, Copy, Debug)]
pub struct TcpStats {
    pub sockets: usize,
    pub established: usize,
    pub listen: usize,
}

pub fn build_tcp_segment(socket: &TcpSocket, flags: u8, payload: &[u8]) -> Vec<u8> {
    build_tcp_segment_fields(
        socket.local_port,
        socket.remote_port,
        socket.seq,
        socket.ack,
        socket.window,
        flags,
        payload,
    )
}

pub fn build_tcp_segment_fields(
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    window: u16,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let mut segment = Vec::with_capacity(20 + payload.len());
    segment.extend_from_slice(&src_port.to_be_bytes());
    segment.extend_from_slice(&dst_port.to_be_bytes());
    segment.extend_from_slice(&seq.to_be_bytes());
    segment.extend_from_slice(&ack.to_be_bytes());
    segment.push(0x50);
    segment.push(flags);
    segment.extend_from_slice(&window.to_be_bytes());
    segment.extend_from_slice(&[0, 0]);
    segment.extend_from_slice(&[0, 0]);
    segment.extend_from_slice(payload);
    segment
}

pub fn parse_tcp_packet(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, payload: &[u8]) -> Option<TcpPacket> {
    if payload.len() < 20 {
        return None;
    }
    let data_offset = ((payload[12] >> 4) as usize) * 4;
    if data_offset < 20 || payload.len() < data_offset {
        return None;
    }
    Some(TcpPacket {
        src_ip,
        dst_ip,
        src_port: u16::from_be_bytes([payload[0], payload[1]]),
        dst_port: u16::from_be_bytes([payload[2], payload[3]]),
        seq: u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]),
        ack: u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]),
        flags: payload[13],
        payload: Vec::from(&payload[data_offset..]),
    })
}

pub fn checksum(src: Ipv4Addr, dst: Ipv4Addr, segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    add_words(&mut sum, &src.0);
    add_words(&mut sum, &dst.0);
    sum += 6;
    sum += segment.len() as u32;
    add_words(&mut sum, segment);
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn add_words(sum: &mut u32, bytes: &[u8]) {
    let mut index = 0;
    while index + 1 < bytes.len() {
        *sum += u32::from(u16::from_be_bytes([bytes[index], bytes[index + 1]]));
        index += 2;
    }
    if index < bytes.len() {
        *sum += u32::from(bytes[index]) << 8;
    }
}

pub fn pseudo_header_checksum(src: Ipv4Addr, dst: Ipv4Addr, _tcp_len: u16, segment: &[u8]) -> u16 {
    checksum(src, dst, segment)
}

pub fn self_test() {
    let mut stack = TcpStack::new();
    let socket = stack.open();
    stack
        .connect(socket, Ipv4Addr([10, 0, 2, 2]), 80)
        .expect("tcp connect");
    let syn = stack.pop_outbound().expect("tcp SYN missing");
    let syn_packet = parse_tcp_packet(Ipv4Addr([10, 0, 2, 15]), syn.dst_ip, &syn.segment)
        .expect("tcp SYN parse");
    if syn_packet.flags & TCP_FLAG_SYN == 0 {
        panic!("tcp SYN self-test failed");
    }
    stack.poll_retransmit(crate::time::monotonic_ticks().saturating_add(TCP_RETRANSMIT_TICKS + 1));
    let retry = stack.pop_outbound().expect("tcp SYN retransmit missing");
    let retry_packet = parse_tcp_packet(Ipv4Addr([10, 0, 2, 15]), retry.dst_ip, &retry.segment)
        .expect("tcp SYN retransmit parse");
    if retry_packet.seq != syn_packet.seq || retry_packet.flags & TCP_FLAG_SYN == 0 {
        panic!("tcp SYN retransmit self-test failed");
    }
    stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 2]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 80,
        dst_port: syn_packet.src_port,
        seq: 1000,
        ack: syn_packet.seq + 1,
        flags: TCP_FLAG_SYN | TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if !stack.established(socket) {
        panic!("tcp handshake self-test failed");
    }
    let _ = stack.send(socket, b"GET / HTTP/1.0\r\n\r\n");
    crate::println!(
        "TCP MVP self-test passed: {} socket(s), {} established.",
        stack.stats().sockets,
        stack.stats().established
    );
}
