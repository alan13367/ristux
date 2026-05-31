//! Small packet-driven TCP layer for the in-tree IPv4 stack.
//! Congestion control, options, retransmits, and passive open are deferred.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::net::Ipv4Addr;

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;

const TCP_RETRANSMIT_TICKS: u64 = 25;
const TCP_MAX_RETRANSMITS: u8 = 3;
const TCP_RECV_WINDOW: u16 = 4096;
const TCP_TIME_WAIT_TICKS: u64 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    CloseWait,
    FinWait1,
    FinWait2,
    Closing,
    LastAck,
    TimeWait,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpError {
    NotConnected,
    WouldBlock,
    InvalidState,
    AlreadyConnected,
    InProgress,
    ConnectionReset,
    TimedOut,
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
    pub error: Option<TcpError>,
    time_wait_deadline_tick: Option<u64>,
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
            window: TCP_RECV_WINDOW,
            rx_buffer: VecDeque::new(),
            error: None,
            time_wait_deadline_tick: None,
        }
    }

    pub fn listen(&mut self) {
        self.state = TcpState::Listen;
        self.time_wait_deadline_tick = None;
    }

    fn established(&self) -> bool {
        self.state == TcpState::Established
    }

    fn recv_window_available(&self) -> usize {
        usize::from(self.window).saturating_sub(self.rx_buffer.len())
    }

    fn advertised_window(&self) -> u16 {
        self.recv_window_available().min(u16::MAX as usize) as u16
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
            match socket.state {
                TcpState::Closed => {}
                TcpState::SynSent => return Err(TcpError::InProgress),
                TcpState::Established => return Err(TcpError::AlreadyConnected),
                _ => return Err(TcpError::InvalidState),
            }
            socket.remote_ip = remote_ip;
            socket.remote_port = remote_port;
            socket.state = TcpState::SynSent;
            socket.error = None;
            socket.time_wait_deadline_tick = None;
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
            if let Some(error) = socket.error {
                return Err(error);
            }
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
        if let Some(error) = socket.error {
            return Err(error);
        }
        if socket.rx_buffer.is_empty() {
            return match socket.state {
                TcpState::Established => Err(TcpError::WouldBlock),
                TcpState::CloseWait | TcpState::TimeWait | TcpState::Closed => Ok(0),
                _ => Err(TcpError::NotConnected),
            };
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
            let next_state = match socket.state {
                TcpState::Established => TcpState::FinWait1,
                TcpState::CloseWait => TcpState::LastAck,
                TcpState::FinWait1
                | TcpState::FinWait2
                | TcpState::LastAck
                | TcpState::TimeWait => {
                    return Ok(());
                }
                _ => {
                    socket.state = TcpState::Closed;
                    return Ok(());
                }
            };
            socket.state = next_state;
            socket.time_wait_deadline_tick = None;
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
        if let Some(index) = self
            .sockets
            .iter()
            .position(|socket| socket.matches_packet(&packet))
        {
            return self.handle_socket_packet(index, packet);
        }

        if let Some(index) = self
            .accept_queue
            .iter()
            .position(|socket| socket.matches_packet(&packet))
        {
            return self.handle_queued_accept_packet(index, packet);
        }

        let Some(index) = self.sockets.iter().position(|socket| {
            socket.state == TcpState::Listen && socket.local_port == packet.dst_port
        }) else {
            if packet.flags & TCP_FLAG_RST == 0 {
                self.pending_outbound
                    .push_back(build_reset_for_unmatched(&packet));
                return true;
            }
            return false;
        };

        self.handle_socket_packet(index, packet)
    }

    fn handle_socket_packet(&mut self, index: usize, packet: TcpPacket) -> bool {
        if packet.flags & TCP_FLAG_RST != 0 {
            let socket = &mut self.sockets[index];
            if socket.state != TcpState::Listen {
                socket.state = TcpState::Closed;
                socket.error = Some(TcpError::ConnectionReset);
                socket.time_wait_deadline_tick = None;
                self.remove_retransmits(packet.src_ip, packet.src_port, packet.dst_port);
                return true;
            }
        }

        if packet.flags & TCP_FLAG_ACK != 0 {
            self.acknowledge(packet.src_ip, packet.src_port, packet.dst_port, packet.ack);
        }

        let mut outbound = None;
        let socket = &mut self.sockets[index];
        match socket.state {
            TcpState::SynSent => {
                if packet.flags & (TCP_FLAG_SYN | TCP_FLAG_ACK) == (TCP_FLAG_SYN | TCP_FLAG_ACK) {
                    socket.ack = packet.seq.wrapping_add(1);
                    socket.seq = packet.ack;
                    socket.state = TcpState::Established;
                    socket.error = None;
                    socket.time_wait_deadline_tick = None;
                    outbound = Some(build_outbound(socket, TCP_FLAG_ACK, &[]));
                }
            }
            TcpState::Established => {
                outbound = handle_established_like(socket, &packet, TcpState::CloseWait);
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
            TcpState::CloseWait => {
                outbound = handle_established_like(socket, &packet, TcpState::CloseWait);
            }
            TcpState::FinWait1 => {
                outbound = handle_fin_wait1(socket, &packet);
            }
            TcpState::FinWait2 => {
                outbound = handle_fin_wait2(socket, &packet);
            }
            TcpState::Closing => {
                outbound = handle_closing(socket, &packet);
            }
            TcpState::LastAck => {
                if packet.flags & TCP_FLAG_FIN != 0 {
                    outbound = Some(build_outbound(socket, TCP_FLAG_ACK, &[]));
                }
                if fin_acknowledged(socket, &packet) {
                    socket.state = TcpState::Closed;
                    socket.time_wait_deadline_tick = None;
                }
            }
            TcpState::TimeWait => {
                if packet.flags & TCP_FLAG_FIN != 0 {
                    enter_time_wait(socket);
                    outbound = Some(build_outbound(socket, TCP_FLAG_ACK, &[]));
                }
            }
            _ => {}
        }

        if let Some(outbound) = outbound {
            self.pending_outbound.push_back(outbound);
        }
        true
    }

    fn handle_queued_accept_packet(&mut self, index: usize, packet: TcpPacket) -> bool {
        if packet.flags & TCP_FLAG_RST != 0 {
            self.accept_queue.remove(index);
            self.remove_retransmits(packet.src_ip, packet.src_port, packet.dst_port);
            return true;
        }

        if packet.flags & TCP_FLAG_ACK != 0 {
            self.acknowledge(packet.src_ip, packet.src_port, packet.dst_port, packet.ack);
        }

        let socket = &mut self.accept_queue[index];
        let outbound = match socket.state {
            TcpState::Established => handle_established_like(socket, &packet, TcpState::CloseWait),
            TcpState::CloseWait => handle_established_like(socket, &packet, TcpState::CloseWait),
            TcpState::FinWait1 => {
                handle_fin_wait1(socket, &packet)
            }
            TcpState::FinWait2 => {
                handle_fin_wait2(socket, &packet)
            }
            TcpState::Closing => {
                handle_closing(socket, &packet)
            }
            TcpState::TimeWait => {
                if packet.flags & TCP_FLAG_FIN != 0 {
                    enter_time_wait(socket);
                    Some(build_outbound(socket, TCP_FLAG_ACK, &[]))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(outbound) = outbound {
            self.pending_outbound.push_back(outbound);
        }
        true
    }

    pub fn poll_retransmit(&mut self, now_tick: u64) -> bool {
        self.expire_time_wait(now_tick);
        let mut retransmitted = false;
        let mut failed = Vec::new();
        for entry in self.retransmits.iter_mut() {
            if entry.deadline_tick > now_tick {
                continue;
            }
            if entry.attempts >= TCP_MAX_RETRANSMITS {
                failed.push((entry.dst_ip, entry.remote_port, entry.local_port));
                continue;
            }
            entry.attempts = entry.attempts.saturating_add(1);
            let backoff = TCP_RETRANSMIT_TICKS << entry.attempts.min(5);
            entry.deadline_tick = now_tick.saturating_add(backoff);
            self.pending_outbound.push_back(TcpOutbound {
                dst_ip: entry.dst_ip,
                segment: entry.segment.clone(),
            });
            retransmitted = true;
        }
        if !failed.is_empty() {
            self.retransmits.retain(|entry| {
                !failed.iter().any(|(ip, remote_port, local_port)| {
                    entry.dst_ip == *ip
                        && entry.remote_port == *remote_port
                        && entry.local_port == *local_port
                })
            });
            for (ip, remote_port, local_port) in failed {
                self.fail_socket(ip, remote_port, local_port, TcpError::TimedOut);
            }
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

    pub fn take_error(&mut self, socket: usize) -> Option<TcpError> {
        self.sockets.get_mut(socket)?.error.take()
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

    fn remove_retransmits(&mut self, remote_ip: Ipv4Addr, remote_port: u16, local_port: u16) {
        self.retransmits.retain(|entry| {
            !(entry.dst_ip == remote_ip
                && entry.remote_port == remote_port
                && entry.local_port == local_port)
        });
    }

    fn fail_socket(
        &mut self,
        remote_ip: Ipv4Addr,
        remote_port: u16,
        local_port: u16,
        error: TcpError,
    ) {
        if let Some(socket) = self.sockets.iter_mut().find(|socket| {
            socket.remote_ip == remote_ip
                && socket.remote_port == remote_port
                && socket.local_port == local_port
        }) {
            socket.state = TcpState::Closed;
            socket.error = Some(error);
            socket.time_wait_deadline_tick = None;
        }
    }

    fn expire_time_wait(&mut self, now_tick: u64) {
        for socket in self.sockets.iter_mut() {
            if socket.state == TcpState::TimeWait
                && socket
                    .time_wait_deadline_tick
                    .map(|deadline| deadline <= now_tick)
                    .unwrap_or(false)
            {
                socket.state = TcpState::Closed;
                socket.time_wait_deadline_tick = None;
            }
        }
    }
}

fn handle_established_like(
    socket: &mut TcpSocket,
    packet: &TcpPacket,
    fin_state: TcpState,
) -> Option<TcpOutbound> {
    let mut ack_needed = false;
    if !packet.payload.is_empty() {
        if packet.seq == socket.ack {
            let accepted = packet.payload.len().min(socket.recv_window_available());
            if accepted > 0 {
                socket.ack = packet.seq.wrapping_add(accepted as u32);
                socket.rx_buffer.extend(packet.payload[..accepted].iter().copied());
            }
        }
        ack_needed = true;
    }
    if packet.flags & TCP_FLAG_FIN != 0 {
        let fin_seq = packet.seq.wrapping_add(packet.payload.len() as u32);
        if fin_seq == socket.ack {
            socket.ack = fin_seq.wrapping_add(1);
            socket.state = fin_state;
        }
        ack_needed = true;
    }
    if ack_needed {
        Some(build_outbound(socket, TCP_FLAG_ACK, &[]))
    } else {
        None
    }
}

fn handle_fin_wait1(socket: &mut TcpSocket, packet: &TcpPacket) -> Option<TcpOutbound> {
    let fin_acked = fin_acknowledged(socket, packet);
    let mut ack_needed = accept_in_order_payload(socket, packet);
    if consume_remote_fin(socket, packet) {
        if fin_acked {
            enter_time_wait(socket);
        } else {
            socket.state = TcpState::Closing;
        }
        ack_needed = true;
    } else if fin_acked {
        socket.state = TcpState::FinWait2;
    }
    if ack_needed {
        Some(build_outbound(socket, TCP_FLAG_ACK, &[]))
    } else {
        None
    }
}

fn handle_fin_wait2(socket: &mut TcpSocket, packet: &TcpPacket) -> Option<TcpOutbound> {
    let mut ack_needed = accept_in_order_payload(socket, packet);
    if consume_remote_fin(socket, packet) {
        enter_time_wait(socket);
        ack_needed = true;
    }
    if ack_needed {
        Some(build_outbound(socket, TCP_FLAG_ACK, &[]))
    } else {
        None
    }
}

fn handle_closing(socket: &mut TcpSocket, packet: &TcpPacket) -> Option<TcpOutbound> {
    let mut outbound = None;
    if packet.flags & TCP_FLAG_FIN != 0 {
        outbound = Some(build_outbound(socket, TCP_FLAG_ACK, &[]));
    }
    if fin_acknowledged(socket, packet) {
        enter_time_wait(socket);
    }
    outbound
}

fn accept_in_order_payload(socket: &mut TcpSocket, packet: &TcpPacket) -> bool {
    if packet.payload.is_empty() {
        return false;
    }
    if packet.seq == socket.ack {
        let accepted = packet.payload.len().min(socket.recv_window_available());
        if accepted > 0 {
            socket.ack = packet.seq.wrapping_add(accepted as u32);
            socket.rx_buffer.extend(packet.payload[..accepted].iter().copied());
        }
    }
    true
}

fn consume_remote_fin(socket: &mut TcpSocket, packet: &TcpPacket) -> bool {
    if packet.flags & TCP_FLAG_FIN == 0 {
        return false;
    }
    let fin_seq = packet.seq.wrapping_add(packet.payload.len() as u32);
    if fin_seq != socket.ack {
        return false;
    }
    socket.ack = fin_seq.wrapping_add(1);
    true
}

fn fin_acknowledged(socket: &TcpSocket, packet: &TcpPacket) -> bool {
    packet.flags & TCP_FLAG_ACK != 0 && seq_at_or_after(packet.ack, socket.seq)
}

fn seq_at_or_after(seq: u32, checkpoint: u32) -> bool {
    (seq.wrapping_sub(checkpoint) as i32) >= 0
}

fn enter_time_wait(socket: &mut TcpSocket) {
    socket.state = TcpState::TimeWait;
    socket.time_wait_deadline_tick =
        Some(crate::time::monotonic_ticks().saturating_add(TCP_TIME_WAIT_TICKS));
}

fn build_reset_for_unmatched(packet: &TcpPacket) -> TcpOutbound {
    let (seq, ack, flags) = if packet.flags & TCP_FLAG_ACK != 0 {
        (packet.ack, 0, TCP_FLAG_RST)
    } else {
        (
            0,
            packet
                .seq
                .wrapping_add(tcp_sequence_span(packet.flags, packet.payload.len())),
            TCP_FLAG_RST | TCP_FLAG_ACK,
        )
    };
    TcpOutbound {
        dst_ip: packet.src_ip,
        segment: build_tcp_segment_fields(
            packet.dst_port,
            packet.src_port,
            seq,
            ack,
            0,
            flags,
            &[],
        ),
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
        socket.advertised_window(),
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
    let _ = stack.pop_outbound();

    let mut oversized = Vec::new();
    oversized.resize(TCP_RECV_WINDOW as usize + 16, b'x');
    stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 2]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 80,
        dst_port: syn_packet.src_port,
        seq: 1001,
        ack: syn_packet.seq + 1,
        flags: TCP_FLAG_ACK | TCP_FLAG_PSH,
        payload: oversized,
    });
    let window_ack = stack
        .pop_outbound()
        .and_then(|outbound| {
            parse_tcp_packet(Ipv4Addr([10, 0, 2, 15]), outbound.dst_ip, &outbound.segment)
        })
        .expect("tcp window ACK missing");
    if window_ack.ack != 1001 + TCP_RECV_WINDOW as u32 || !window_ack.payload.is_empty() {
        panic!("tcp receive window self-test failed");
    }
    if stack.sockets[socket].advertised_window() != 0 {
        panic!("tcp zero-window self-test failed");
    }
    let mut drained = 0usize;
    let mut drain = [0u8; 128];
    while drained < TCP_RECV_WINDOW as usize {
        let read = stack.recv(socket, &mut drain).expect("tcp window drain failed");
        if read == 0 {
            break;
        }
        drained += read;
    }
    if drained != TCP_RECV_WINDOW as usize
        || stack.sockets[socket].advertised_window() != TCP_RECV_WINDOW
    {
        panic!("tcp receive window reopen self-test failed");
    }
    let _ = stack.send(socket, b"GET / HTTP/1.0\r\n\r\n");

    let mut close_stack = TcpStack::new();
    let active = close_stack.open();
    {
        let socket = &mut close_stack.sockets[active];
        socket.remote_ip = Ipv4Addr([10, 0, 2, 2]);
        socket.remote_port = 443;
        socket.state = TcpState::Established;
        socket.seq = 5000;
        socket.ack = 9000;
    }
    close_stack.close(active).expect("tcp active close");
    let _ = close_stack.pop_outbound().expect("tcp active FIN missing");
    close_stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 2]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 443,
        dst_port: close_stack.sockets[active].local_port,
        seq: 9000,
        ack: 5000,
        flags: TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if close_stack.sockets[active].state != TcpState::FinWait1 {
        panic!("tcp close accepted partial FIN ACK");
    }
    close_stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 2]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 443,
        dst_port: close_stack.sockets[active].local_port,
        seq: 9000,
        ack: 5001,
        flags: TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if close_stack.sockets[active].state != TcpState::FinWait2 {
        panic!("tcp close did not enter FIN_WAIT_2");
    }
    close_stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 2]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 443,
        dst_port: close_stack.sockets[active].local_port,
        seq: 9000,
        ack: 5001,
        flags: TCP_FLAG_FIN | TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if close_stack.sockets[active].state != TcpState::TimeWait {
        panic!("tcp close did not enter TIME_WAIT");
    }
    let _ = close_stack.pop_outbound().expect("tcp FIN ACK missing");
    close_stack.poll_retransmit(
        crate::time::monotonic_ticks().saturating_add(TCP_TIME_WAIT_TICKS + 1),
    );
    if close_stack.sockets[active].state != TcpState::Closed {
        panic!("tcp TIME_WAIT did not expire");
    }

    let simultaneous = close_stack.open();
    {
        let socket = &mut close_stack.sockets[simultaneous];
        socket.remote_ip = Ipv4Addr([10, 0, 2, 3]);
        socket.remote_port = 444;
        socket.state = TcpState::Established;
        socket.seq = 7000;
        socket.ack = 3000;
    }
    close_stack
        .close(simultaneous)
        .expect("tcp simultaneous close");
    let _ = close_stack
        .pop_outbound()
        .expect("tcp simultaneous FIN missing");
    close_stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 3]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 444,
        dst_port: close_stack.sockets[simultaneous].local_port,
        seq: 3000,
        ack: 7000,
        flags: TCP_FLAG_FIN | TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if close_stack.sockets[simultaneous].state != TcpState::Closing {
        panic!("tcp simultaneous close did not enter CLOSING");
    }
    let _ = close_stack
        .pop_outbound()
        .expect("tcp simultaneous FIN ACK missing");
    close_stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 3]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 444,
        dst_port: close_stack.sockets[simultaneous].local_port,
        seq: 3001,
        ack: 7001,
        flags: TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if close_stack.sockets[simultaneous].state != TcpState::TimeWait {
        panic!("tcp simultaneous close did not enter TIME_WAIT");
    }

    let passive = close_stack.open();
    {
        let socket = &mut close_stack.sockets[passive];
        socket.remote_ip = Ipv4Addr([10, 0, 2, 4]);
        socket.remote_port = 445;
        socket.state = TcpState::CloseWait;
        socket.seq = 8000;
        socket.ack = 4001;
    }
    close_stack.close(passive).expect("tcp passive close");
    let _ = close_stack
        .pop_outbound()
        .expect("tcp passive FIN missing");
    close_stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 4]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 445,
        dst_port: close_stack.sockets[passive].local_port,
        seq: 4001,
        ack: 8000,
        flags: TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if close_stack.sockets[passive].state != TcpState::LastAck {
        panic!("tcp passive close accepted partial FIN ACK");
    }
    close_stack.handle_packet(TcpPacket {
        src_ip: Ipv4Addr([10, 0, 2, 4]),
        dst_ip: Ipv4Addr([10, 0, 2, 15]),
        src_port: 445,
        dst_port: close_stack.sockets[passive].local_port,
        seq: 4001,
        ack: 8001,
        flags: TCP_FLAG_ACK,
        payload: Vec::new(),
    });
    if close_stack.sockets[passive].state != TcpState::Closed {
        panic!("tcp passive close did not close after FIN ACK");
    }
    crate::println!(
        "TCP MVP self-test passed: {} socket(s), {} established.",
        stack.stats().sockets,
        stack.stats().established
    );
}
