//! systemd socket-activation adoption (`LISTEN_PID` / `LISTEN_FDS`), the
//! controlled descriptor-inheritance baseline PRD-production-operations §8
//! names for zero-downtime executable upgrades: the orchestrator pre-binds the
//! declared listening sockets and hands them to the new process, so a binary
//! upgrade never drops a connection or leaves an accept gap.
//!
//! Contract (fail closed):
//! - `LISTEN_PID` must equal this process id, else no inheritance happened
//!   (systemd's spoofing guard) and the caller binds normally.
//! - When inheritance is active, every declared data-plane socket must find an
//!   inherited descriptor with the exact bound local address, and every
//!   inherited descriptor must be claimed. A leftover descriptor means the
//!   orchestrator passed sockets the configuration does not declare — split
//!   ownership, exactly the failure mode §8 forbids.

use std::{
    io,
    net::SocketAddr,
    os::unix::io::{FromRawFd, RawFd},
};

/// systemd passes adopted descriptors starting at file descriptor 3.
const SD_LISTEN_FDS_START: RawFd = 3;

/// The inherited listening sockets, adopted and keyed by bound address.
pub(crate) struct InheritedSockets {
    /// Sticky: socket activation was requested for this process, so every
    /// declared data-plane socket must claim an inherited descriptor.
    active: bool,
    tcp: Vec<(SocketAddr, std::net::TcpListener)>,
    udp: Vec<(SocketAddr, std::net::UdpSocket)>,
}

impl InheritedSockets {
    /// Adopts `LISTEN_FDS` descriptors when `LISTEN_PID` names this process;
    /// otherwise returns an empty set (the caller binds normally).
    ///
    /// # Errors
    /// Fails closed when an inherited descriptor is not a usable socket: the
    /// orchestrator's descriptor contract is broken and starting with a
    /// partial set would open an accept gap.
    #[cfg(unix)]
    pub(crate) fn adopt() -> Result<Self, io::Error> {
        let Ok(listen_pid) = std::env::var("LISTEN_PID") else {
            return Ok(Self::empty());
        };
        if listen_pid.trim().parse::<u32>().ok() != Some(std::process::id()) {
            // `LISTEN_FDS` targets another process (systemd's spoofing guard).
            return Ok(Self::empty());
        }
        let count: usize = std::env::var("LISTEN_FDS")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0);
        let mut tcp = Vec::new();
        let mut udp = Vec::new();
        for index in 0..count {
            let raw_fd = SD_LISTEN_FDS_START + index as RawFd;
            // The raw-descriptor ownership contract (sd_listen_fds(3): the
            // orchestrator hands out descriptors 3..3+LISTEN_FDS, bound and
            // listening, owned exclusively by this process after adoption)
            // is discharged inside `socket_from_raw_fd`'s own SAFETY notes.
            let std_socket = socket_from_raw_fd(raw_fd);
            match std_socket {
                AdoptedSocket::Tcp(listener) => {
                    listener.set_nonblocking(true)?;
                    let address = listener.local_addr()?;
                    tcp.push((address, listener));
                }
                AdoptedSocket::Udp(socket) => {
                    socket.set_nonblocking(true)?;
                    let address = socket.local_addr()?;
                    udp.push((address, socket));
                }
            }
        }
        Ok(Self {
            active: true,
            tcp,
            udp,
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn adopt() -> Result<Self, io::Error> {
        Ok(Self::empty())
    }

    fn empty() -> Self {
        Self {
            active: false,
            tcp: Vec::new(),
            udp: Vec::new(),
        }
    }

    /// Whether descriptor inheritance is active for this process (sticky for
    /// the whole bootstrap: every declared socket must claim a descriptor).
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    /// Takes the inherited TCP listener bound to `address`, if any.
    pub(crate) fn take_tcp(&mut self, address: SocketAddr) -> Option<std::net::TcpListener> {
        let index = self.tcp.iter().position(|(bound, _)| *bound == address)?;
        Some(self.tcp.remove(index).1)
    }

    /// Takes the inherited UDP socket bound to `address`, if any.
    pub(crate) fn take_udp(&mut self, address: SocketAddr) -> Option<std::net::UdpSocket> {
        let index = self.udp.iter().position(|(bound, _)| *bound == address)?;
        Some(self.udp.remove(index).1)
    }

    /// Verifies every inherited descriptor was claimed by a declared socket.
    /// A leftover means the orchestrator passed sockets the configuration does
    /// not declare: split ownership, refused instead of silently ignored.
    pub(crate) fn verify_all_claimed(&self) -> Result<(), io::Error> {
        for (bound, _) in &self.tcp {
            return Err(io::Error::other(format!(
                "inherited TCP socket {bound} is not declared by the active configuration"
            )));
        }
        for (bound, _) in &self.udp {
            return Err(io::Error::other(format!(
                "inherited UDP socket {bound} is not declared by the active configuration"
            )));
        }
        Ok(())
    }
}

enum AdoptedSocket {
    Tcp(std::net::TcpListener),
    Udp(std::net::UdpSocket),
}

/// Adopts one raw descriptor and determines whether it is a TCP listener or a
/// UDP socket. The caller's SAFETY contract guarantees the descriptor is a
/// socket this process owns.
#[cfg(unix)]
#[allow(unsafe_code)]
fn socket_from_raw_fd(raw_fd: RawFd) -> AdoptedSocket {
    // A TCP listener answers SO_ACCEPTCONN; a UDP socket does not. Peek the
    // socket type through getsockopt without taking ownership yet.
    let is_tcp = socket_type_is_tcp(raw_fd);
    if is_tcp {
        // SAFETY: the orchestrator handed out descriptor `raw_fd` per the
        // sd_listen_fds(3) contract; this process owns it exclusively and
        // claims it exactly once.
        unsafe { AdoptedSocket::Tcp(std::net::TcpListener::from_raw_fd(raw_fd)) }
    } else {
        // SAFETY: same ownership contract as the TCP branch.
        unsafe { AdoptedSocket::Udp(std::net::UdpSocket::from_raw_fd(raw_fd)) }
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn socket_type_is_tcp(raw_fd: RawFd) -> bool {
    // SAFETY: `raw_fd` is, per the caller's SAFETY contract, a socket this
    // process owns; `getsockopt` only reads its type and never transfers or
    // closes it.
    unsafe {
        let mut socket_type: libc::c_int = 0;
        let mut length = libc::socklen_t::try_from(std::mem::size_of::<libc::c_int>())
            .expect("socklen_t fits the socket type size");
        let result = libc::getsockopt(
            raw_fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            std::ptr::addr_of_mut!(socket_type).cast(),
            std::ptr::addr_of_mut!(length),
        );
        if result != 0 {
            // Not a readable socket: let `from_raw_fd`/`local_addr` surface
            // the failure through the normal error path.
            return true;
        }
        socket_type == libc::SOCK_STREAM
    }
}
