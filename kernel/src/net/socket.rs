use alloc::vec::Vec;

use super::tcp::TcpStack;
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
    udp_indices: Vec<usize>,
}

impl SocketTable {
    pub fn new() -> Self {
        Self {
            tcp: TcpStack::new(),
            udp_indices: Vec::new(),
        }
    }

    pub fn socket(&mut self, domain: SocketDomain, kind: SocketType) -> Option<usize> {
        match (domain, kind) {
            (SocketDomain::Inet, SocketType::Stream) => Some(self.tcp.bind(0)),
            (SocketDomain::Inet, SocketType::Datagram) => {
                let index = self.udp_indices.len();
                self.udp_indices.push(index);
                Some(index)
            }
        }
    }

    pub fn tcp_mut(&mut self) -> &mut TcpStack {
        &mut self.tcp
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
        let _ = fd;
    });
    crate::println!("Socket layer self-test passed.");
}
