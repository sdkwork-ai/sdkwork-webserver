//! PROXY protocol v1 header builder/parser (nginx `proxy_protocol` on the
//! mail backend leg).
//!
//! Only the v1 text header is modelled (the bounded mail configuration only
//! needs the single-header form): `PROXY TCP4|TCP6 <src> <dst> <sport> <dport>`
//! or `PROXY UNKNOWN`. v2 binary framing is deliberately out of scope.

use core::net::SocketAddr;

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Address family of one PROXY v1 header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyProtocolFamily {
    /// IPv4 TCP (`PROXY TCP4`).
    Tcp4,
    /// IPv6 TCP (`PROXY TCP6`).
    Tcp6,
    /// `PROXY UNKNOWN`.
    Unknown,
}

/// One bounded PROXY protocol v1 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyProtocolV1 {
    family: ProxyProtocolFamily,
    source: Option<SocketAddr>,
    destination: Option<SocketAddr>,
}

impl ProxyProtocolV1 {
    /// Build the header for a TCP connection.
    ///
    /// # Errors
    ///
    /// Fails closed when the source and destination address families differ or
    /// a port is zero.
    pub fn tcp(source: SocketAddr, destination: SocketAddr) -> Result<Self> {
        if source.is_ipv4() != destination.is_ipv4() {
            return Err(Error::ProxyProtocol {
                reason: "address families differ",
            });
        }
        if source.port() == 0 || destination.port() == 0 {
            return Err(Error::ProxyProtocol {
                reason: "port must not be zero",
            });
        }
        let family = if source.is_ipv4() {
            ProxyProtocolFamily::Tcp4
        } else {
            ProxyProtocolFamily::Tcp6
        };
        Ok(Self {
            family,
            source: Some(source),
            destination: Some(destination),
        })
    }

    /// Build `PROXY UNKNOWN` (no address information is forwarded).
    pub fn unknown() -> Self {
        Self {
            family: ProxyProtocolFamily::Unknown,
            source: None,
            destination: None,
        }
    }

    /// Address family.
    pub fn family(&self) -> ProxyProtocolFamily {
        self.family
    }

    /// Source address when the header carries one.
    pub fn source(&self) -> Option<SocketAddr> {
        self.source
    }

    /// Destination address when the header carries one.
    pub fn destination(&self) -> Option<SocketAddr> {
        self.destination
    }

    /// Encode the header including its trailing CRLF.
    ///
    /// # Errors
    ///
    /// Fails closed when the encoded header exceeds
    /// `limits.max_relay_header_bytes`.
    pub fn encode(&self, limits: &Limits) -> Result<Vec<u8>> {
        let limits = limits.sanitized();
        let line = match (self.source, self.destination) {
            (Some(source), Some(destination)) => {
                let family = match self.family {
                    ProxyProtocolFamily::Tcp4 => "TCP4",
                    ProxyProtocolFamily::Tcp6 => "TCP6",
                    ProxyProtocolFamily::Unknown => {
                        return Err(Error::ProxyProtocol {
                            reason: "UNKNOWN must not carry addresses",
                        })
                    }
                };
                format!(
                    "PROXY {family} {} {} {} {}",
                    source.ip(),
                    destination.ip(),
                    source.port(),
                    destination.port()
                )
            }
            _ => "PROXY UNKNOWN".to_owned(),
        };
        if line.len() + 2 > limits.max_relay_header_bytes {
            return Err(Error::limit(
                "proxy-protocol",
                "header bytes",
                limits.max_relay_header_bytes,
            ));
        }
        let mut encoded = line.into_bytes();
        encoded.extend_from_slice(b"\r\n");
        Ok(encoded)
    }

    /// Parse one header from the start of `input`.
    ///
    /// Returns the header and the number of consumed bytes.
    ///
    /// # Errors
    ///
    /// Fails closed on a malformed header, a missing CRLF, unknown families, or
    /// an oversized header.
    pub fn parse(input: &[u8], limits: &Limits) -> Result<(Self, usize)> {
        let limits = limits.sanitized();
        let terminator = input
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or(Error::ProxyProtocol {
                reason: "missing CRLF",
            })?;
        let consumed = terminator + 2;
        if consumed > limits.max_relay_header_bytes {
            return Err(Error::limit(
                "proxy-protocol",
                "header bytes",
                limits.max_relay_header_bytes,
            ));
        }
        let text =
            core::str::from_utf8(&input[..terminator]).map_err(|_| Error::ProxyProtocol {
                reason: "header is not UTF-8",
            })?;
        let mut parts = text.split(' ');
        if parts.next() != Some("PROXY") {
            return Err(Error::ProxyProtocol {
                reason: "missing PROXY verb",
            });
        }
        let family = match parts.next() {
            Some("TCP4") => ProxyProtocolFamily::Tcp4,
            Some("TCP6") => ProxyProtocolFamily::Tcp6,
            Some("UNKNOWN") => {
                if parts.next().is_some() {
                    return Err(Error::ProxyProtocol {
                        reason: "UNKNOWN takes no addresses",
                    });
                }
                return Ok((Self::unknown(), consumed));
            }
            _ => {
                return Err(Error::ProxyProtocol {
                    reason: "unsupported family",
                })
            }
        };
        let source_ip = parts.next().ok_or(Error::ProxyProtocol {
            reason: "missing source",
        })?;
        let destination_ip = parts.next().ok_or(Error::ProxyProtocol {
            reason: "missing destination",
        })?;
        let source_port = parts.next().ok_or(Error::ProxyProtocol {
            reason: "missing source port",
        })?;
        let destination_port = parts.next().ok_or(Error::ProxyProtocol {
            reason: "missing destination port",
        })?;
        if parts.next().is_some() {
            return Err(Error::ProxyProtocol {
                reason: "trailing fields",
            });
        }
        let source: SocketAddr =
            format!("{source_ip}:{source_port}")
                .parse()
                .map_err(|_| Error::ProxyProtocol {
                    reason: "invalid source address",
                })?;
        let destination: SocketAddr = format!("{destination_ip}:{destination_port}")
            .parse()
            .map_err(|_| Error::ProxyProtocol {
                reason: "invalid destination address",
            })?;
        match family {
            ProxyProtocolFamily::Tcp4 if !source.is_ipv4() => {
                return Err(Error::ProxyProtocol {
                    reason: "TCP4 with a non-IPv4 address",
                })
            }
            ProxyProtocolFamily::Tcp6 if source.is_ipv6() && destination.is_ipv4() => {
                return Err(Error::ProxyProtocol {
                    reason: "mixed address families",
                })
            }
            _ => {}
        }
        let header = Self::tcp(source, destination)?;
        Ok((header, consumed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_header_round_trips() {
        let limits = Limits::default();
        let header = ProxyProtocolV1::tcp(
            "192.0.2.1:34567".parse().expect("source"),
            "198.51.100.2:25".parse().expect("destination"),
        )
        .expect("header");
        let encoded = header.encode(&limits).expect("encode");
        assert_eq!(encoded, b"PROXY TCP4 192.0.2.1 198.51.100.2 34567 25\r\n");
        let (parsed, consumed) = ProxyProtocolV1::parse(&encoded, &limits).expect("parse");
        assert_eq!(parsed, header);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn unknown_and_invalid_headers_are_handled() {
        let limits = Limits::default();
        let unknown = ProxyProtocolV1::unknown();
        assert_eq!(
            unknown.encode(&limits).expect("encode"),
            b"PROXY UNKNOWN\r\n"
        );
        for input in [
            b"PROXY TCP4 192.0.2.1\r\n".as_slice(),
            b"PROXY TCP4 192.0.2.1 198.51.100.2 1\r\n",
            b"PROXY TCP5 192.0.2.1 198.51.100.2 1 2\r\n",
            b"PROXY UNKNOWN extra\r\n",
            b"PROXY TCP4 192.0.2.1 198.51.100.2 1 2",
            b"garbage\r\n",
        ] {
            assert!(ProxyProtocolV1::parse(input, &limits).is_err(), "{input:?}");
        }
        assert!(ProxyProtocolV1::parse(b"PROXY UNKNOWN\r\nextra", &limits).is_ok());
    }
}
