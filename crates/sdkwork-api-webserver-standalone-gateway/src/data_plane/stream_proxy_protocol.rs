//! Inbound PROXY protocol header parsing for stream listeners.
//!
//! nginx enables this per `listen … proxy_protocol` and bounds it with
//! `proxy_protocol_timeout`. The header arrives before the stream payload and
//! carries the real client address, which replaces the peer for the access
//! phase, limit_conn, stream variables and any emitted upstream PROXY header
//! when `set_real_ip_from` trusts the connecting peer.
//!
//! v1 text (reusing the bounded mail-protocol parser) and v2 binary (LOCAL,
//! PROXY over TCP4/TCP6) are supported. Datagram/TLV payloads are parsed and
//! ignored within the bounded header size; unknown versions/families fail the
//! connection closed.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use sdkwork_webserver_mail_protocol::{proxy_protocol::ProxyProtocolV1, Limits};
use tokio::{io::AsyncReadExt, time::timeout};

/// Maximum bytes accepted for the complete PROXY protocol header (nginx's
/// `proxy_protocol` buffer is bounded; the v1 line, v2 fixed header and the
/// largest supported address block fit well below this ceiling).
const MAXIMUM_PROXY_HEADER_BYTES: usize = 512;
const PROXY_V2_SIGNATURE: [u8; 12] = [
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A,
];

/// One parsed inbound PROXY protocol header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InboundProxyHeader {
    /// Addresses from a `PROXY` command; `None` for `LOCAL`/`UNKNOWN`.
    pub(crate) source: Option<SocketAddr>,
    pub(crate) destination: Option<SocketAddr>,
}

/// Read and parse one PROXY protocol header from the accepted stream.
///
/// The header is consumed byte-for-byte so the first payload byte is never
/// swallowed: the stream is left positioned exactly after the header. A
/// missing, malformed, oversized or late header fails the connection closed,
/// matching nginx's `proxy_protocol` contract.
pub(crate) async fn read_inbound_proxy_header<S: tokio::io::AsyncRead + Unpin>(
    stream: &mut S,
    header_timeout: Duration,
) -> Result<InboundProxyHeader, String> {
    let mut header = Vec::with_capacity(32);
    loop {
        if header.len() >= MAXIMUM_PROXY_HEADER_BYTES {
            return Err("inbound PROXY protocol header exceeds the bounded size".to_owned());
        }
        let mut byte = [0_u8; 1];
        let read = timeout(header_timeout, stream.read_exact(&mut byte))
            .await
            .map_err(|_| "inbound PROXY protocol header timed out".to_owned())?
            .map_err(|error| format!("inbound PROXY protocol read failed: {error}"))?;
        debug_assert_eq!(read, 1);
        header.push(byte[0]);
        if header.len() == PROXY_V2_SIGNATURE.len() && header == PROXY_V2_SIGNATURE {
            return read_v2(stream, header_timeout).await;
        }
        // Accept bytes that are still a prefix of either supported signature;
        // the two signatures diverge at byte 1 (0x0D vs 'P' for the text verb),
        // so an unrelated first byte is rejected immediately.
        let v2_prefix =
            header.len() <= PROXY_V2_SIGNATURE.len() && PROXY_V2_SIGNATURE.starts_with(&header);
        let v1_started = header.len() >= 6 && header.starts_with(b"PROXY ");
        let v1_prefix = header.len() < 6 && b"PROXY ".starts_with(&header);
        if !v2_prefix && !v1_prefix && !v1_started {
            return Err("inbound PROXY protocol header is missing".to_owned());
        }
        if v1_started && header.ends_with(b"\r\n") {
            let limits = Limits::default();
            let (parsed, _) = ProxyProtocolV1::parse(&header, &limits)
                .map_err(|error| format!("invalid PROXY protocol v1 header: {error}"))?;
            return Ok(InboundProxyHeader {
                source: parsed.source(),
                destination: parsed.destination(),
            });
        }
    }
}

async fn read_v2<S: tokio::io::AsyncRead + Unpin>(
    stream: &mut S,
    header_timeout: Duration,
) -> Result<InboundProxyHeader, String> {
    // version/command(1) family/protocol(1) length(2)
    let mut fixed = [0_u8; 4];
    timeout(header_timeout, stream.read_exact(&mut fixed))
        .await
        .map_err(|_| "inbound PROXY protocol v2 header timed out".to_owned())?
        .map_err(|error| format!("inbound PROXY protocol v2 read failed: {error}"))?;
    let version = fixed[0] >> 4;
    let command = fixed[0] & 0x0F;
    if version != 2 {
        return Err(format!("unsupported PROXY protocol v2 version {version}"));
    }
    let family_protocol = fixed[1];
    let address_length = u16::from_be_bytes([fixed[2], fixed[3]]) as usize;
    let bounded_length = address_length.min(MAXIMUM_PROXY_HEADER_BYTES - 16);
    let mut payload = vec![0_u8; bounded_length];
    if bounded_length > 0 {
        timeout(header_timeout, stream.read_exact(&mut payload))
            .await
            .map_err(|_| "inbound PROXY protocol v2 payload timed out".to_owned())?
            .map_err(|error| format!("inbound PROXY protocol v2 read failed: {error}"))?;
    }
    match command {
        // LOCAL: health checks and local-originated connections.
        0x0 => Ok(InboundProxyHeader {
            source: None,
            destination: None,
        }),
        0x1 => {
            let family = family_protocol >> 4;
            let protocol = family_protocol & 0x0F;
            if protocol != 0x1 {
                return Err(
                    "PROXY protocol v2 datagram protocol is not supported on a stream listener"
                        .to_owned(),
                );
            }
            match family {
                0x1 if payload.len() >= 12 => {
                    let source_ip = Ipv4Addr::new(payload[0], payload[1], payload[2], payload[3]);
                    let destination_ip =
                        Ipv4Addr::new(payload[4], payload[5], payload[6], payload[7]);
                    let source_port = u16::from_be_bytes([payload[8], payload[9]]);
                    let destination_port = u16::from_be_bytes([payload[10], payload[11]]);
                    Ok(InboundProxyHeader {
                        source: Some(SocketAddr::new(IpAddr::V4(source_ip), source_port)),
                        destination: Some(SocketAddr::new(
                            IpAddr::V4(destination_ip),
                            destination_port,
                        )),
                    })
                }
                0x2 if payload.len() >= 36 => {
                    let mut source = [0_u8; 16];
                    source.copy_from_slice(&payload[0..16]);
                    let mut destination = [0_u8; 16];
                    destination.copy_from_slice(&payload[16..32]);
                    let source_port = u16::from_be_bytes([payload[32], payload[33]]);
                    let destination_port = u16::from_be_bytes([payload[34], payload[35]]);
                    Ok(InboundProxyHeader {
                        source: Some(SocketAddr::new(
                            IpAddr::V6(Ipv6Addr::from(source)),
                            source_port,
                        )),
                        destination: Some(SocketAddr::new(
                            IpAddr::V6(Ipv6Addr::from(destination)),
                            destination_port,
                        )),
                    })
                }
                0x0 => Ok(InboundProxyHeader {
                    source: None,
                    destination: None,
                }),
                other => Err(format!(
                    "PROXY protocol v2 address family {other} is not supported"
                )),
            }
        }
        other => Err(format!(
            "PROXY protocol v2 command {other} is not supported"
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::AsyncReadExt;

    use super::*;

    fn v2_header(
        source: [u8; 4],
        destination: [u8; 4],
        source_port: u16,
        destination_port: u16,
    ) -> Vec<u8> {
        let mut header = Vec::from(PROXY_V2_SIGNATURE);
        header.push(0x21);
        header.push(0x11);
        header.extend_from_slice(&12_u16.to_be_bytes());
        header.extend_from_slice(&source);
        header.extend_from_slice(&destination);
        header.extend_from_slice(&source_port.to_be_bytes());
        header.extend_from_slice(&destination_port.to_be_bytes());
        header
    }

    #[tokio::test]
    async fn parses_v1_and_leaves_the_payload() {
        let mut input =
            std::io::Cursor::new(b"PROXY TCP4 10.0.0.5 127.0.0.1 4242 80\r\npayload".to_vec());
        let parsed = read_inbound_proxy_header(&mut input, Duration::from_secs(1))
            .await
            .expect("v1 header");
        assert_eq!(
            parsed.source,
            Some("10.0.0.5:4242".parse().expect("source"))
        );
        let mut rest = Vec::new();
        input.read_to_end(&mut rest).await.expect("rest");
        assert_eq!(rest, b"payload");
    }

    #[tokio::test]
    async fn parses_v2_and_leaves_the_payload() {
        let mut bytes = v2_header([10, 9, 9, 9], [127, 0, 0, 1], 4242, 80);
        bytes.extend_from_slice(b"payload");
        let mut input = std::io::Cursor::new(bytes);
        let parsed = read_inbound_proxy_header(&mut input, Duration::from_secs(1))
            .await
            .expect("v2 header");
        assert_eq!(
            parsed.source,
            Some("10.9.9.9:4242".parse().expect("source"))
        );
        assert_eq!(
            parsed.destination,
            Some("127.0.0.1:80".parse().expect("destination"))
        );
        let mut rest = Vec::new();
        input.read_to_end(&mut rest).await.expect("rest");
        assert_eq!(rest, b"payload");
    }

    #[tokio::test]
    async fn rejects_a_missing_header() {
        let mut input = std::io::Cursor::new(b"garbage-not-a-proxy".to_vec());
        assert!(
            read_inbound_proxy_header(&mut input, Duration::from_secs(1))
                .await
                .is_err()
        );
    }
}
