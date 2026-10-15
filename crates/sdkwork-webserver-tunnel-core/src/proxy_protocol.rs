//! PROXY protocol (v1/v2) headers (FRP `transport.proxyProtocolVersion`
//! parity, PRD §46).
//!
//! A tunneled service that needs the visitor's real address — an SSH daemon
//! logging the source of a session, an HTTP server behind the agent, a
//! database's connection audit — cannot see it: the agent dials the local
//! target from its own address. The PROXY protocol is the standard way to
//! hand that address over on the same connection, and this module renders the
//! header the agent writes before the first application byte.
//!
//! Rendering lives in the domain crate because it is pure data formatting:
//! the protocol crate carries the wire shape, the agent owns the dial.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

/// PROXY protocol version to emit toward an agent's local target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyProtocolVersion {
    /// Human-readable `PROXY TCP4 …\r\n` line (HAProxy 1.5+, widely parsed).
    V1,
    /// Binary `\r\n\r\n\0\r\nQUIT\n` header (HAProxy 2.0+, carries TLVs).
    V2,
}

impl ProxyProtocolVersion {
    /// Parses the configuration/wire label (`v1`, `v2`).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "v1" | "V1" | "1" => Some(Self::V1),
            "v2" | "V2" | "2" => Some(Self::V2),
            _ => None,
        }
    }

    /// The canonical label used in configuration and on the wire.
    pub const fn label(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
        }
    }

    /// Renders the header for one relayed connection.
    ///
    /// `source` is the visitor (as the gateway observed it) and `destination`
    /// is the local target the agent dialed. Both addresses must be the same
    /// family: the protocol has no mixed-family form, and mixing them would
    /// produce a header every parser rejects. `None` means the pair cannot be
    /// expressed (see [`Self::render`]).
    pub fn header(self, source: SocketAddr, destination: SocketAddr) -> Option<Vec<u8>> {
        match (source, destination) {
            (SocketAddr::V4(source), SocketAddr::V4(destination)) => Some(match self {
                Self::V1 => format!(
                    "PROXY TCP4 {} {} {} {}\r\n",
                    source.ip(),
                    destination.ip(),
                    source.port(),
                    destination.port()
                )
                .into_bytes(),
                Self::V2 => {
                    let mut header = Vec::with_capacity(28);
                    header.extend_from_slice(Self::V2_SIGNATURE);
                    header.push(0x21); // version 2, command PROXY
                    header.push(0x11); // AF_INET, STREAM
                    header.extend_from_slice(&16_u16.to_be_bytes());
                    header.extend_from_slice(&source.ip().octets());
                    header.extend_from_slice(&destination.ip().octets());
                    header.extend_from_slice(&source.port().to_be_bytes());
                    header.extend_from_slice(&destination.port().to_be_bytes());
                    header
                }
            }),
            (SocketAddr::V6(source), SocketAddr::V6(destination)) => Some(match self {
                Self::V1 => format!(
                    "PROXY TCP6 {} {} {} {}\r\n",
                    source.ip(),
                    destination.ip(),
                    source.port(),
                    destination.port()
                )
                .into_bytes(),
                Self::V2 => {
                    let mut header = Vec::with_capacity(52);
                    header.extend_from_slice(Self::V2_SIGNATURE);
                    header.push(0x21); // version 2, command PROXY
                    header.push(0x21); // AF_INET6, STREAM
                    header.extend_from_slice(&36_u16.to_be_bytes());
                    header.extend_from_slice(&source.ip().octets());
                    header.extend_from_slice(&destination.ip().octets());
                    header.extend_from_slice(&source.port().to_be_bytes());
                    header.extend_from_slice(&destination.port().to_be_bytes());
                    header
                }
            }),
            // A Unix socket target has no address pair, and a v4 visitor
            // cannot be described against a v6 destination.
            _ => None,
        }
    }

    /// The 12-byte v2 signature (a deliberately invalid v1 line, so a
    /// v1-only parser fails loudly instead of misreading the binary body).
    const V2_SIGNATURE: &'static [u8] = b"\r\n\r\n\0\r\nQUIT\n";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_parse_and_render() {
        for version in [ProxyProtocolVersion::V1, ProxyProtocolVersion::V2] {
            assert_eq!(ProxyProtocolVersion::parse(version.label()), Some(version));
        }
        assert_eq!(
            ProxyProtocolVersion::parse("v3"),
            None,
            "an unknown version must not silently become v1"
        );
    }

    #[test]
    fn v1_renders_a_readable_line_with_both_ports() {
        let source: SocketAddr = "203.0.113.5:51000".parse().expect("source");
        let destination: SocketAddr = "127.0.0.1:22".parse().expect("destination");
        let header = ProxyProtocolVersion::V1
            .header(source, destination)
            .expect("v4 pair renders");
        assert_eq!(
            String::from_utf8(header).expect("utf8"),
            "PROXY TCP4 203.0.113.5 127.0.0.1 51000 22\r\n"
        );
    }

    #[test]
    fn v1_renders_tcp6_for_ipv6_pairs() {
        let source: SocketAddr = "[2001:db8::1]:51000".parse().expect("source");
        let destination: SocketAddr = "[::1]:22".parse().expect("destination");
        let header = ProxyProtocolVersion::V1
            .header(source, destination)
            .expect("v6 pair renders");
        assert_eq!(
            String::from_utf8(header).expect("utf8"),
            "PROXY TCP6 2001:db8::1 ::1 51000 22\r\n"
        );
    }

    #[test]
    fn v2_renders_the_binary_header_with_exact_lengths() {
        let source: SocketAddr = "203.0.113.5:51000".parse().expect("source");
        let destination: SocketAddr = "127.0.0.1:22".parse().expect("destination");
        let header = ProxyProtocolVersion::V2
            .header(source, destination)
            .expect("v4 pair renders");
        assert_eq!(
            header.len(),
            16 + 12,
            "16-byte addresses + 12-byte fixed part"
        );
        assert_eq!(&header[..12], b"\r\n\r\n\0\r\nQUIT\n");
        assert_eq!(header[12], 0x21, "version 2, command PROXY");
        assert_eq!(header[13], 0x11, "AF_INET + STREAM");
        assert_eq!(&header[14..16], &16_u16.to_be_bytes());
        assert_eq!(&header[16..20], &[203, 0, 113, 5]);
        assert_eq!(&header[20..24], &[127, 0, 0, 1]);
        assert_eq!(&header[24..26], &51000_u16.to_be_bytes());
        assert_eq!(&header[26..28], &22_u16.to_be_bytes());

        let v6_source: SocketAddr = "[2001:db8::1]:51000".parse().expect("source");
        let v6_destination: SocketAddr = "[::1]:22".parse().expect("destination");
        let v6 = ProxyProtocolVersion::V2
            .header(v6_source, v6_destination)
            .expect("v6 pair renders");
        assert_eq!(v6[13], 0x21, "AF_INET6 + STREAM");
        assert_eq!(&v6[14..16], &36_u16.to_be_bytes());
        assert_eq!(v6.len(), 36 + 16);
    }

    #[test]
    fn a_mixed_family_pair_cannot_be_described() {
        // The protocol has no mixed form; emitting one would produce a header
        // every parser rejects, so the agent must skip it instead.
        let source: SocketAddr = "203.0.113.5:51000".parse().expect("source");
        let destination: SocketAddr = "[::1]:22".parse().expect("destination");
        assert!(ProxyProtocolVersion::V1
            .header(source, destination)
            .is_none());
        assert!(ProxyProtocolVersion::V2
            .header(source, destination)
            .is_none());
    }
}
