use alloc::vec::Vec;

use super::{
    tcp::{TcpError, TcpStack},
    Ipv4Addr,
};
use crate::sync::spinlock::SpinLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketDomain {
    Inet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketType {
    Stream,
    Datagram,
}

pub struct SocketTable {
    tcp: TcpStack,
    sockets: Vec<SocketEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketError {
    BadFd,
    Invalid,
    WouldBlock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SocketBackend {
    Tcp(usize),
}

#[derive(Clone, Copy)]
struct SocketEntry {
    domain: SocketDomain,
    kind: SocketType,
    backend: SocketBackend,
}

impl SocketTable {
    pub fn new() -> Self {
        Self {
            tcp: TcpStack::new(),
            sockets: Vec::new(),
        }
    }

    pub fn socket(&mut self, domain: SocketDomain, kind: SocketType) -> Option<usize> {
        match (domain, kind) {
            (SocketDomain::Inet, SocketType::Stream) => {
                let tcp = self.tcp.open();
                self.sockets.push(SocketEntry {
                    domain,
                    kind,
                    backend: SocketBackend::Tcp(tcp),
                });
                Some(self.sockets.len() - 1)
            }
            (SocketDomain::Inet, SocketType::Datagram) => None,
        }
    }

    pub fn bind(&mut self, handle: usize, local_port: u16) -> Result<(), SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Tcp(socket) => self
                .tcp
                .bind_existing(socket, local_port)
                .map_err(map_tcp_error),
        }
    }

    pub fn listen(&mut self, handle: usize) -> Result<(), SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Tcp(socket) => self.tcp.listen(socket).map_err(map_tcp_error),
        }
    }

    pub fn connect(
        &mut self,
        handle: usize,
        remote_ip: Ipv4Addr,
        remote_port: u16,
    ) -> Result<(), SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Tcp(socket) => self
                .tcp
                .connect(socket, remote_ip, remote_port)
                .map_err(map_tcp_error),
        }
    }

    pub fn accept(&mut self, handle: usize) -> Result<usize, SocketError> {
        let entry = *self.entry(handle)?;
        match entry.backend {
            SocketBackend::Tcp(socket) => {
                let accepted = self.tcp.accept(socket).map_err(map_tcp_error)?;
                self.sockets.push(SocketEntry {
                    domain: entry.domain,
                    kind: entry.kind,
                    backend: SocketBackend::Tcp(accepted),
                });
                Ok(self.sockets.len() - 1)
            }
        }
    }

    pub fn send(&mut self, handle: usize, data: &[u8]) -> Result<usize, SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Tcp(socket) => self.tcp.send(socket, data).map_err(map_tcp_error),
        }
    }

    pub fn recv(&mut self, handle: usize, output: &mut [u8]) -> Result<usize, SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Tcp(socket) => self.tcp.recv(socket, output).map_err(map_tcp_error),
        }
    }

    pub fn local_port(&self, handle: usize) -> Result<u16, SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Tcp(socket) => self.tcp.local_port(socket).ok_or(SocketError::BadFd),
        }
    }

    fn entry(&self, handle: usize) -> Result<&SocketEntry, SocketError> {
        self.sockets.get(handle).ok_or(SocketError::BadFd)
    }
}

fn map_tcp_error(err: TcpError) -> SocketError {
    match err {
        TcpError::WouldBlock => SocketError::WouldBlock,
        TcpError::NotConnected | TcpError::InvalidState => SocketError::Invalid,
    }
}

static SOCKETS: SpinLock<Option<SocketTable>> = SpinLock::new(None);

pub fn init() {
    *SOCKETS.lock() = Some(SocketTable::new());
}

pub fn with_sockets<T>(f: impl FnOnce(&mut SocketTable) -> T) -> T {
    f(SOCKETS
        .lock()
        .as_mut()
        .expect("socket table used before initialization"))
}

pub fn self_test() {
    with_sockets(|table| {
        let fd = table
            .socket(SocketDomain::Inet, SocketType::Stream)
            .expect("tcp socket");
        table
            .connect(fd, Ipv4Addr([10, 0, 2, 2]), 80)
            .expect("tcp connect");
        table.send(fd, b"GET / HTTP/1.0\r\n\r\n").expect("tcp send");
        let mut response = [0u8; 64];
        let read = table.recv(fd, &mut response).expect("tcp recv");
        if !response[..read].starts_with(b"HTTP/1.0 200 OK") {
            panic!("tcp socket response missing");
        }
    });
    crate::println!("Socket layer self-test passed.");
}
