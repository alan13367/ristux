use alloc::vec::Vec;

use super::{
    IcmpSocketId, Ipv4Addr, LOOPBACK_IP, SocketId, local_ip,
    tcp::{TcpError, TcpStack},
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
    RawIcmp,
}

pub struct SocketTable {
    tcp: TcpStack,
    sockets: Vec<SocketEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketError {
    BadFd,
    Invalid,
    RefcountOverflow,
    WouldBlock,
    AddressInUse,
    AlreadyConnected,
    InProgress,
    ConnectionReset,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SocketReady {
    pub read: bool,
    pub write: bool,
    pub error: bool,
    pub hangup: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SocketBackend {
    Closed,
    Tcp(usize),
    Udp(SocketId),
    Icmp(IcmpSocketId),
}

#[derive(Clone, Copy)]
struct SocketEntry {
    domain: SocketDomain,
    kind: SocketType,
    backend: SocketBackend,
    peer: Option<SocketAddress>,
    options: SocketOptions,
    recv_shutdown: bool,
    send_shutdown: bool,
    fd_flags: u32,
    status_flags: u32,
    ref_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketAddress {
    pub ip: Ipv4Addr,
    pub port: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketRecv {
    pub len: usize,
    pub peer: Option<SocketAddress>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SocketOptions {
    reuse_addr: bool,
    recv_timeout_ms: Option<u64>,
    send_timeout_ms: Option<u64>,
    tcp_nodelay: bool,
    error: i32,
}

impl SocketOptions {
    const fn new() -> Self {
        Self {
            reuse_addr: false,
            recv_timeout_ms: None,
            send_timeout_ms: None,
            tcp_nodelay: false,
            error: 0,
        }
    }
}

impl SocketEntry {
    fn new(domain: SocketDomain, kind: SocketType, backend: SocketBackend) -> Self {
        Self {
            domain,
            kind,
            backend,
            peer: None,
            options: SocketOptions::new(),
            recv_shutdown: false,
            send_shutdown: false,
            fd_flags: 0,
            status_flags: 0,
            ref_count: 1,
        }
    }

    fn closed() -> Self {
        let mut entry = Self::new(
            SocketDomain::Inet,
            SocketType::Stream,
            SocketBackend::Closed,
        );
        entry.ref_count = 0;
        entry
    }
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
                Some(self.insert_entry(SocketEntry::new(domain, kind, SocketBackend::Tcp(tcp))))
            }
            (SocketDomain::Inet, SocketType::Datagram) => {
                let udp = super::udp_socket_open(0)?;
                Some(self.insert_entry(SocketEntry::new(domain, kind, SocketBackend::Udp(udp))))
            }
            (SocketDomain::Inet, SocketType::RawIcmp) => {
                let icmp = super::icmp_socket_open()?;
                Some(self.insert_entry(SocketEntry::new(domain, kind, SocketBackend::Icmp(icmp))))
            }
        }
    }

    fn insert_entry(&mut self, entry: SocketEntry) -> usize {
        if let Some(index) = self
            .sockets
            .iter()
            .position(|socket| socket.backend == SocketBackend::Closed && socket.ref_count == 0)
        {
            self.sockets[index] = entry;
            return index;
        }
        self.sockets.push(entry);
        self.sockets.len() - 1
    }

    pub fn duplicate(&mut self, handle: usize) -> Result<(), SocketError> {
        let entry = self.entry_mut(handle)?;
        entry.ref_count = entry
            .ref_count
            .checked_add(1)
            .ok_or(SocketError::RefcountOverflow)?;
        Ok(())
    }

    pub fn duplicate_descriptor(
        &mut self,
        handle: usize,
        minimum_handle: usize,
        fd_flags: u32,
    ) -> Result<usize, SocketError> {
        let mut duplicate = *self.entry(handle)?;
        duplicate.fd_flags = fd_flags;
        duplicate.ref_count = 1;
        if let Some(index) = self
            .sockets
            .iter()
            .enumerate()
            .skip(minimum_handle)
            .find_map(|(index, socket)| {
                (socket.backend == SocketBackend::Closed && socket.ref_count == 0)
                    .then_some(index)
            })
        {
            self.sockets[index] = duplicate;
            return Ok(index);
        }
        while self.sockets.len() < minimum_handle {
            self.sockets.push(SocketEntry::closed());
        }
        self.sockets.push(duplicate);
        Ok(self.sockets.len() - 1)
    }

    pub fn close(&mut self, handle: usize) -> Result<(), SocketError> {
        {
            let entry = self.entry_mut(handle)?;
            if entry.ref_count > 1 {
                entry.ref_count -= 1;
                return Ok(());
            }
        }
        let backend = self.entry(handle)?.backend;
        self.sockets[handle] = SocketEntry::closed();
        if self
            .sockets
            .iter()
            .any(|entry| entry.backend == backend && entry.ref_count > 0)
        {
            return Ok(());
        }
        match backend {
            SocketBackend::Closed => return Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => {
                self.tcp.close(socket).map_err(map_tcp_error)?;
                super::drive_tcp(&mut self.tcp);
            }
            SocketBackend::Udp(socket) => {
                if !super::udp_socket_close(socket) {
                    return Err(SocketError::BadFd);
                }
            }
            SocketBackend::Icmp(socket) => {
                if !super::icmp_socket_close(socket) {
                    return Err(SocketError::BadFd);
                }
            }
        }
        Ok(())
    }

    pub fn bind(&mut self, handle: usize, local_port: u16) -> Result<(), SocketError> {
        let entry = *self.entry(handle)?;
        if local_port != 0 && self.bound_port_in_use(handle, entry.kind, local_port) {
            self.record_error(handle, SocketError::AddressInUse);
            return Err(SocketError::AddressInUse);
        }
        match entry.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => self
                .tcp
                .bind_existing(socket, local_port)
                .map_err(map_tcp_error),
            SocketBackend::Udp(socket) => {
                if super::udp_socket_bind(socket, local_port) {
                    Ok(())
                } else {
                    Err(SocketError::BadFd)
                }
            }
            SocketBackend::Icmp(_) => Err(SocketError::Invalid),
        }
    }

    pub fn listen(&mut self, handle: usize) -> Result<(), SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => self.tcp.listen(socket).map_err(map_tcp_error),
            SocketBackend::Udp(_) | SocketBackend::Icmp(_) => Err(SocketError::Invalid),
        }
    }

    pub fn connect(
        &mut self,
        handle: usize,
        remote_ip: Ipv4Addr,
        remote_port: u16,
    ) -> Result<(), SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => {
                match self.tcp.connect(socket, remote_ip, remote_port) {
                    Ok(()) | Err(TcpError::InProgress) => {}
                    Err(TcpError::AlreadyConnected) => return Ok(()),
                    Err(error) => {
                        let error = map_tcp_error(error);
                        self.record_error(handle, error);
                        return Err(error);
                    }
                }
                super::drive_tcp(&mut self.tcp);
                if let Some(error) = self.tcp.sockets.get(socket).and_then(|socket| socket.error) {
                    let err = map_tcp_error(error);
                    self.record_error(handle, err);
                    return Err(err);
                }
                if self.tcp.established(socket) {
                    Ok(())
                } else {
                    Err(SocketError::WouldBlock)
                }
            }
            SocketBackend::Udp(_) => {
                self.entry_mut(handle)?.peer = Some(SocketAddress {
                    ip: remote_ip,
                    port: remote_port,
                });
                Ok(())
            }
            SocketBackend::Icmp(_) => {
                self.entry_mut(handle)?.peer = Some(SocketAddress {
                    ip: remote_ip,
                    port: 0,
                });
                Ok(())
            }
        }
    }

    pub fn accept(&mut self, handle: usize) -> Result<usize, SocketError> {
        let entry = *self.entry(handle)?;
        match entry.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => {
                super::drive_tcp(&mut self.tcp);
                let accepted = self.tcp.accept(socket).map_err(map_tcp_error)?;
                let peer = self
                    .tcp
                    .peer_addr(accepted)
                    .map(|(ip, port)| SocketAddress { ip, port });
                let mut socket_entry =
                    SocketEntry::new(entry.domain, entry.kind, SocketBackend::Tcp(accepted));
                socket_entry.peer = peer;
                socket_entry.options = entry.options;
                Ok(self.insert_entry(socket_entry))
            }
            SocketBackend::Udp(_) | SocketBackend::Icmp(_) => Err(SocketError::Invalid),
        }
    }

    pub fn send(&mut self, handle: usize, data: &[u8]) -> Result<usize, SocketError> {
        self.send_to(handle, None, data)
    }

    pub fn send_to(
        &mut self,
        handle: usize,
        target: Option<SocketAddress>,
        data: &[u8],
    ) -> Result<usize, SocketError> {
        let entry = *self.entry(handle)?;
        if entry.send_shutdown {
            return Err(SocketError::Invalid);
        }
        match entry.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => {
                let sent = match self.tcp.send(socket, data).map_err(map_tcp_error) {
                    Ok(sent) => sent,
                    Err(err) => {
                        self.record_error(handle, err);
                        return Err(err);
                    }
                };
                super::drive_tcp(&mut self.tcp);
                Ok(sent)
            }
            SocketBackend::Udp(socket) => {
                let target = target.or(entry.peer).ok_or(SocketError::Invalid)?;
                if super::udp_socket_send(socket, target.ip, target.port, data) {
                    Ok(data.len())
                } else {
                    Err(SocketError::Invalid)
                }
            }
            SocketBackend::Icmp(socket) => {
                let target = target.or(entry.peer).ok_or(SocketError::Invalid)?;
                if super::icmp_socket_send(socket, target.ip, data) {
                    Ok(data.len())
                } else {
                    Err(SocketError::Invalid)
                }
            }
        }
    }

    pub fn recv(&mut self, handle: usize, output: &mut [u8]) -> Result<usize, SocketError> {
        self.recv_from(handle, output).map(|recv| recv.len)
    }

    pub fn recv_from(
        &mut self,
        handle: usize,
        output: &mut [u8],
    ) -> Result<SocketRecv, SocketError> {
        let entry = *self.entry(handle)?;
        if entry.recv_shutdown {
            return Ok(SocketRecv {
                len: 0,
                peer: self.peer_addr(handle).ok().flatten(),
            });
        }
        match entry.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => {
                super::drive_tcp(&mut self.tcp);
                let len = match self.tcp.recv(socket, output).map_err(map_tcp_error) {
                    Ok(len) => len,
                    Err(err) => {
                        self.record_error(handle, err);
                        return Err(err);
                    }
                };
                let peer = self
                    .tcp
                    .peer_addr(socket)
                    .map(|(ip, port)| SocketAddress { ip, port });
                Ok(SocketRecv { len, peer })
            }
            SocketBackend::Udp(socket) => {
                let datagram = super::udp_socket_recv(socket).ok_or(SocketError::WouldBlock)?;
                let len = datagram.payload.len().min(output.len());
                output[..len].copy_from_slice(&datagram.payload[..len]);
                Ok(SocketRecv {
                    len,
                    peer: Some(SocketAddress {
                        ip: datagram.src,
                        port: datagram.src_port,
                    }),
                })
            }
            SocketBackend::Icmp(socket) => {
                let datagram = super::icmp_socket_recv(socket).ok_or(SocketError::WouldBlock)?;
                let len = datagram.payload.len().min(output.len());
                output[..len].copy_from_slice(&datagram.payload[..len]);
                Ok(SocketRecv {
                    len,
                    peer: Some(SocketAddress {
                        ip: datagram.src,
                        port: 0,
                    }),
                })
            }
        }
    }

    pub fn local_port(&self, handle: usize) -> Result<u16, SocketError> {
        match self.entry(handle)?.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => self.tcp.local_port(socket).ok_or(SocketError::BadFd),
            SocketBackend::Udp(socket) => {
                super::udp_socket_local_port(socket).ok_or(SocketError::BadFd)
            }
            SocketBackend::Icmp(_) => Ok(0),
        }
    }

    pub fn local_addr(&self, handle: usize) -> Result<SocketAddress, SocketError> {
        let port = self.local_port(handle)?;
        let ip = match self.peer_addr(handle)? {
            Some(peer) if peer.ip.is_loopback() => LOOPBACK_IP,
            _ => local_ip(),
        };
        Ok(SocketAddress { ip, port })
    }

    pub fn peer_addr(&self, handle: usize) -> Result<Option<SocketAddress>, SocketError> {
        let entry = self.entry(handle)?;
        match entry.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(socket) => Ok(self
                .tcp
                .peer_addr(socket)
                .map(|(ip, port)| SocketAddress { ip, port })),
            SocketBackend::Udp(_) => Ok(entry.peer),
            SocketBackend::Icmp(_) => Ok(entry.peer),
        }
    }

    pub fn poll(&mut self, handle: usize) -> Result<SocketReady, SocketError> {
        let entry = *self.entry(handle)?;
        match entry.backend {
            SocketBackend::Closed => Err(SocketError::BadFd),
            SocketBackend::Tcp(tcp_index) => {
                super::drive_tcp(&mut self.tcp);
                let socket = self.tcp.sockets.get(tcp_index).ok_or(SocketError::BadFd)?;
                let ready = match socket.state {
                    super::tcp::TcpState::Listen => SocketReady {
                        read: self.tcp.has_pending_accept(),
                        ..SocketReady::default()
                    },
                    super::tcp::TcpState::Established => SocketReady {
                        read: entry.recv_shutdown || !socket.rx_buffer.is_empty(),
                        write: !entry.send_shutdown,
                        ..SocketReady::default()
                    },
                    super::tcp::TcpState::TimeWait | super::tcp::TcpState::Closed => SocketReady {
                        read: entry.recv_shutdown || !socket.rx_buffer.is_empty(),
                        error: socket.error.is_some(),
                        hangup: true,
                        ..SocketReady::default()
                    },
                    super::tcp::TcpState::CloseWait => SocketReady {
                        read: true,
                        write: !entry.send_shutdown,
                        hangup: socket.rx_buffer.is_empty(),
                        ..SocketReady::default()
                    },
                    _ => SocketReady::default(),
                };
                Ok(ready)
            }
            SocketBackend::Udp(socket) => Ok(SocketReady {
                read: entry.recv_shutdown || super::udp_socket_readable(socket),
                write: !entry.send_shutdown,
                hangup: entry.recv_shutdown,
                ..SocketReady::default()
            }),
            SocketBackend::Icmp(socket) => Ok(SocketReady {
                read: entry.recv_shutdown || super::icmp_socket_readable(socket),
                write: !entry.send_shutdown,
                hangup: entry.recv_shutdown,
                ..SocketReady::default()
            }),
        }
    }

    pub fn shutdown(&mut self, handle: usize, how: i32) -> Result<(), SocketError> {
        let close_send = match how {
            0 => false,
            1 => true,
            2 => true,
            _ => return Err(SocketError::Invalid),
        };
        let backend = {
            let entry = self.entry_mut(handle)?;
            if how == 0 || how == 2 {
                entry.recv_shutdown = true;
            }
            if close_send {
                entry.send_shutdown = true;
            }
            entry.backend
        };
        if close_send {
            match backend {
                SocketBackend::Closed => return Err(SocketError::BadFd),
                SocketBackend::Tcp(socket) => {
                    self.tcp.close(socket).map_err(map_tcp_error)?;
                    super::drive_tcp(&mut self.tcp);
                }
                SocketBackend::Udp(_) | SocketBackend::Icmp(_) => {}
            }
        }
        Ok(())
    }

    pub fn fd_flags(&self, handle: usize) -> Result<u32, SocketError> {
        Ok(self.entry(handle)?.fd_flags)
    }

    pub fn set_fd_flags(&mut self, handle: usize, flags: u32) -> Result<(), SocketError> {
        self.entry_mut(handle)?.fd_flags = flags;
        Ok(())
    }

    pub fn status_flags(&self, handle: usize) -> Result<u32, SocketError> {
        Ok(self.entry(handle)?.status_flags)
    }

    pub fn set_status_flags(&mut self, handle: usize, flags: u32) -> Result<(), SocketError> {
        let backend = self.entry(handle)?.backend;
        for entry in &mut self.sockets {
            if entry.backend == backend && entry.ref_count > 0 {
                entry.status_flags = flags;
            }
        }
        Ok(())
    }

    pub fn reuse_addr(&self, handle: usize) -> Result<bool, SocketError> {
        Ok(self.entry(handle)?.options.reuse_addr)
    }

    pub fn set_reuse_addr(&mut self, handle: usize, value: bool) -> Result<(), SocketError> {
        self.entry_mut(handle)?.options.reuse_addr = value;
        Ok(())
    }

    pub fn recv_timeout_ms(&self, handle: usize) -> Result<Option<u64>, SocketError> {
        Ok(self.entry(handle)?.options.recv_timeout_ms)
    }

    pub fn set_recv_timeout_ms(
        &mut self,
        handle: usize,
        value: Option<u64>,
    ) -> Result<(), SocketError> {
        self.entry_mut(handle)?.options.recv_timeout_ms = value;
        Ok(())
    }

    pub fn send_timeout_ms(&self, handle: usize) -> Result<Option<u64>, SocketError> {
        Ok(self.entry(handle)?.options.send_timeout_ms)
    }

    pub fn set_send_timeout_ms(
        &mut self,
        handle: usize,
        value: Option<u64>,
    ) -> Result<(), SocketError> {
        self.entry_mut(handle)?.options.send_timeout_ms = value;
        Ok(())
    }

    pub fn tcp_nodelay(&self, handle: usize) -> Result<bool, SocketError> {
        Ok(self.entry(handle)?.options.tcp_nodelay)
    }

    pub fn set_tcp_nodelay(&mut self, handle: usize, value: bool) -> Result<(), SocketError> {
        let entry = self.entry_mut(handle)?;
        if entry.kind != SocketType::Stream {
            return Err(SocketError::Invalid);
        }
        entry.options.tcp_nodelay = value;
        Ok(())
    }

    pub fn take_error(&mut self, handle: usize) -> Result<i32, SocketError> {
        let backend = self.entry(handle)?.backend;
        let tcp_error = match backend {
            SocketBackend::Tcp(socket) => self.tcp.take_error(socket).map(tcp_error_code),
            _ => None,
        };
        let entry = self.entry_mut(handle)?;
        if let Some(error) = tcp_error {
            entry.options.error = error;
        }
        let error = entry.options.error;
        entry.options.error = 0;
        Ok(error)
    }

    fn record_error(&mut self, handle: usize, err: SocketError) {
        let code = socket_error_code(err);
        if code == 0 {
            return;
        }
        if let Some(entry) = self.sockets.get_mut(handle) {
            entry.options.error = code;
        }
    }

    fn bound_port_in_use(&self, handle: usize, kind: SocketType, local_port: u16) -> bool {
        self.sockets.iter().enumerate().any(|(index, entry)| {
            index != handle
                && entry.kind == kind
                && entry.ref_count > 0
                && self.backend_local_port(entry.backend) == Some(local_port)
        })
    }

    fn backend_local_port(&self, backend: SocketBackend) -> Option<u16> {
        match backend {
            SocketBackend::Closed => None,
            SocketBackend::Tcp(socket) => self.tcp.local_port(socket),
            SocketBackend::Udp(socket) => super::udp_socket_local_port(socket),
            SocketBackend::Icmp(_) => None,
        }
    }

    fn entry(&self, handle: usize) -> Result<&SocketEntry, SocketError> {
        let entry = self.sockets.get(handle).ok_or(SocketError::BadFd)?;
        if entry.backend == SocketBackend::Closed {
            return Err(SocketError::BadFd);
        }
        Ok(entry)
    }

    fn entry_mut(&mut self, handle: usize) -> Result<&mut SocketEntry, SocketError> {
        let entry = self.sockets.get_mut(handle).ok_or(SocketError::BadFd)?;
        if entry.backend == SocketBackend::Closed {
            return Err(SocketError::BadFd);
        }
        Ok(entry)
    }
}

fn map_tcp_error(err: TcpError) -> SocketError {
    match err {
        TcpError::WouldBlock => SocketError::WouldBlock,
        TcpError::NotConnected | TcpError::InvalidState => SocketError::Invalid,
        TcpError::AlreadyConnected => SocketError::AlreadyConnected,
        TcpError::InProgress => SocketError::InProgress,
        TcpError::ConnectionReset => SocketError::ConnectionReset,
        TcpError::TimedOut => SocketError::TimedOut,
    }
}

fn socket_error_code(err: SocketError) -> i32 {
    match err {
        SocketError::AddressInUse => 98,
        SocketError::ConnectionReset => 104,
        SocketError::TimedOut => 110,
        _ => 0,
    }
}

fn tcp_error_code(err: TcpError) -> i32 {
    socket_error_code(map_tcp_error(err))
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

pub fn drive_tcp() {
    if let Some(table) = SOCKETS.lock().as_mut() {
        super::drive_tcp(&mut table.tcp);
    }
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

        let listener = table
            .socket(SocketDomain::Inet, SocketType::Stream)
            .expect("loopback listener socket");
        table.bind(listener, 18080).expect("loopback bind");
        table.listen(listener).expect("loopback listen");
        let client = table
            .socket(SocketDomain::Inet, SocketType::Stream)
            .expect("loopback client socket");
        table
            .connect(client, Ipv4Addr([127, 0, 0, 1]), 18080)
            .expect("loopback connect");
        let server = table.accept(listener).expect("loopback accept");
        table.send(client, b"ping").expect("loopback client send");
        let mut request = [0u8; 8];
        let read = table
            .recv(server, &mut request)
            .expect("loopback server recv");
        if &request[..read] != b"ping" {
            panic!("loopback server payload mismatch");
        }
        table.send(server, b"pong").expect("loopback server send");
        let mut response = [0u8; 8];
        let read = table
            .recv(client, &mut response)
            .expect("loopback client recv");
        if &response[..read] != b"pong" {
            panic!("loopback client payload mismatch");
        }
        let client_duplicate = table
            .duplicate_descriptor(client, 0, 1)
            .expect("loopback client descriptor duplicate");
        if table.fd_flags(client_duplicate).expect("duplicate fd flags") != 1 {
            panic!("duplicated socket descriptor flags mismatch");
        }
        table
            .set_status_flags(client, 0x800)
            .expect("shared socket status flags");
        if table
            .status_flags(client_duplicate)
            .expect("duplicate status flags")
            != 0x800
        {
            panic!("duplicated socket status flags were not shared");
        }
        table.close(client).expect("original client close");
        table
            .send(client_duplicate, b"clone")
            .expect("duplicated client send after original close");
        let read = table
            .recv(server, &mut request)
            .expect("duplicated client server recv");
        if &request[..read] != b"clone" {
            panic!("duplicated socket backend lifetime mismatch");
        }

        let udp_server = table
            .socket(SocketDomain::Inet, SocketType::Datagram)
            .expect("udp server socket");
        table.bind(udp_server, 19053).expect("udp bind server");
        table
            .set_reuse_addr(udp_server, true)
            .expect("udp reuseaddr");
        if !table
            .reuse_addr(udp_server)
            .expect("udp reuseaddr readback")
        {
            panic!("udp reuseaddr readback failed");
        }
        table
            .set_recv_timeout_ms(udp_server, Some(25))
            .expect("udp recv timeout");
        if table
            .recv_timeout_ms(udp_server)
            .expect("udp timeout readback")
            != Some(25)
        {
            panic!("udp timeout readback failed");
        }

        let udp_client = table
            .socket(SocketDomain::Inet, SocketType::Datagram)
            .expect("udp client socket");
        table.bind(udp_client, 19054).expect("udp bind client");
        table
            .send_to(
                udp_client,
                Some(SocketAddress {
                    ip: Ipv4Addr([127, 0, 0, 1]),
                    port: 19053,
                }),
                b"dns?",
            )
            .expect("udp client send");
        let mut request = [0u8; 8];
        let recv = table
            .recv_from(udp_server, &mut request)
            .expect("udp server recv");
        if recv.len != 4
            || &request[..recv.len] != b"dns?"
            || recv.peer.map(|peer| peer.port) != Some(19054)
        {
            panic!("udp datagram recv mismatch");
        }
        table
            .send_to(udp_server, recv.peer, b"ok")
            .expect("udp server send");
        let mut response = [0u8; 8];
        let recv = table
            .recv_from(udp_client, &mut response)
            .expect("udp client recv");
        if recv.len != 2 || &response[..recv.len] != b"ok" {
            panic!("udp datagram response mismatch");
        }
        table.close(udp_client).expect("udp client close");
        if table.recv(udp_client, &mut response) != Err(SocketError::BadFd) {
            panic!("closed udp socket stayed readable");
        }
        table.close(udp_server).expect("udp server close");
        table.close(server).expect("loopback server close");
        table
            .close(client_duplicate)
            .expect("duplicated loopback client close");
        table.close(listener).expect("loopback listener close");
        table.close(fd).expect("tcp socket close");
    });
    crate::println!("Socket layer self-test passed.");
}
