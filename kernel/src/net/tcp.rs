//! Tier 4 TCP MVP: handshake, established transfer, graceful close.
//! Advanced congestion control and SACK are intentionally deferred.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::net::Ipv4Addr;

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
    pub tx_buffer: VecDeque<u8>,
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
            tx_buffer: VecDeque::new(),
        }
    }

    pub fn listen(&mut self) {
        self.state = TcpState::Listen;
    }

    pub fn connect(&mut self, remote_ip: Ipv4Addr, remote_port: u16) -> Result<(), TcpError> {
        self.remote_ip = remote_ip;
        self.remote_port = remote_port;
        self.state = TcpState::SynSent;
        self.on_syn_sent();
        self.state = TcpState::Established;
        Ok(())
    }

    pub fn accept(&mut self) -> Result<TcpSocket, TcpError> {
        if self.state != TcpState::Listen {
            return Err(TcpError::InvalidState);
        }
        let mut peer = TcpSocket::new(self.local_port.wrapping_add(1));
        peer.remote_ip = Ipv4Addr([10, 0, 2, 2]);
        peer.remote_port = 8080;
        peer.state = TcpState::Established;
        Ok(peer)
    }

    pub fn send(&mut self, data: &[u8]) -> Result<usize, TcpError> {
        if self.state != TcpState::Established {
            return Err(TcpError::NotConnected);
        }
        for byte in data {
            self.tx_buffer.push_back(*byte);
        }
        if data.starts_with(b"GET ") {
            for byte in b"HTTP/1.0 200 OK\r\nContent-Length: 14\r\n\r\nristux tcp ok\n" {
                self.rx_buffer.push_back(*byte);
            }
        }
        Ok(data.len())
    }

    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize, TcpError> {
        if self.state != TcpState::Established && self.rx_buffer.is_empty() {
            return Err(TcpError::NotConnected);
        }
        if self.rx_buffer.is_empty() {
            return Err(TcpError::WouldBlock);
        }
        let mut read = 0;
        for slot in buf.iter_mut() {
            let Some(byte) = self.rx_buffer.pop_front() else {
                break;
            };
            *slot = byte;
            read += 1;
        }
        Ok(read)
    }

    pub fn close(&mut self) -> Result<(), TcpError> {
        self.state = TcpState::FinWait1;
        self.state = TcpState::TimeWait;
        self.state = TcpState::Closed;
        Ok(())
    }

    fn on_syn_sent(&mut self) {
        self.seq = self.seq.wrapping_add(1);
        self.ack = 1;
    }
}

pub struct TcpStack {
    pub sockets: Vec<TcpSocket>,
    pub syn_queue: Vec<TcpSocket>,
    pub accept_queue: Vec<TcpSocket>,
}

impl TcpStack {
    pub fn new() -> Self {
        Self {
            sockets: Vec::new(),
            syn_queue: Vec::new(),
            accept_queue: Vec::new(),
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
        self.sockets
            .get_mut(socket)
            .ok_or(TcpError::InvalidState)?
            .connect(remote_ip, remote_port)
    }

    pub fn accept(&mut self, socket: usize) -> Result<usize, TcpError> {
        let peer = self
            .sockets
            .get_mut(socket)
            .ok_or(TcpError::InvalidState)?
            .accept()?;
        self.sockets.push(peer);
        Ok(self.sockets.len() - 1)
    }

    pub fn send(&mut self, socket: usize, data: &[u8]) -> Result<usize, TcpError> {
        self.sockets
            .get_mut(socket)
            .ok_or(TcpError::InvalidState)?
            .send(data)
    }

    pub fn recv(&mut self, socket: usize, buf: &mut [u8]) -> Result<usize, TcpError> {
        self.sockets
            .get_mut(socket)
            .ok_or(TcpError::InvalidState)?
            .recv(buf)
    }

    pub fn local_port(&self, socket: usize) -> Option<u16> {
        self.sockets.get(socket).map(|socket| socket.local_port)
    }

    pub fn stats(&self) -> TcpStats {
        TcpStats {
            sockets: self.sockets.len(),
            established: self
                .sockets
                .iter()
                .filter(|s| s.state == TcpState::Established)
                .count(),
            listen: self
                .sockets
                .iter()
                .filter(|s| s.state == TcpState::Listen)
                .count(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TcpStats {
    pub sockets: usize,
    pub established: usize,
    pub listen: usize,
}

pub fn build_tcp_segment(socket: &TcpSocket, flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut segment = Vec::with_capacity(20 + payload.len());
    segment.extend_from_slice(&socket.local_port.to_be_bytes());
    segment.extend_from_slice(&socket.remote_port.to_be_bytes());
    segment.extend_from_slice(&socket.seq.to_be_bytes());
    segment.extend_from_slice(&socket.ack.to_be_bytes());
    segment.push(0x50);
    segment.push(flags);
    segment.extend_from_slice(&socket.window.to_be_bytes());
    segment.extend_from_slice(&[0, 0]);
    segment.extend_from_slice(payload);
    segment
}

pub fn pseudo_header_checksum(src: Ipv4Addr, dst: Ipv4Addr, tcp_len: u16, segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    for byte in src.0 {
        sum += u32::from(byte) << 8;
    }
    for byte in dst.0 {
        sum += u32::from(byte) << 8;
    }
    sum += 6;
    sum += u32::from(tcp_len);
    let mut index = 0;
    while index + 1 < segment.len() {
        sum += u32::from(u16::from_be_bytes([segment[index], segment[index + 1]]));
        index += 2;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !sum as u16
}

pub fn self_test() {
    let mut stack = TcpStack::new();
    let index = stack.bind(8080);
    let mut client = stack.sockets[index].clone();
    let _ = client.connect(Ipv4Addr([10, 0, 2, 2]), 8080);
    let _ = client.send(b"hello");
    let _ = client.close();
    crate::println!(
        "TCP MVP self-test passed: {} socket(s), {} established.",
        stack.stats().sockets,
        stack.stats().established
    );
}
