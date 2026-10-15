//! Minimal DNS resolver for the agent's gateway address (FRP `dnsServer`).
//!
//! Why this exists at all: a split-horizon or offline deployment publishes the
//! gateway's name in *its own* resolver, and the system resolver on an agent
//! host may not know it. Without an override the operator's only option is to
//! put IP literals in every agent's configuration, which then cannot follow the
//! gateway across a move.
//!
//! Scope is deliberately narrow — one `A`/`AAAA` question over UDP to one
//! configured server, no caching, no search domains, no DNSSEC:
//!
//! - **one server**, because FRP's `dnsServer` is one server and a failover
//!   list is a different feature with different failure semantics;
//! - **no cache**, because the answer is used once per reconnect and a cache
//!   would need an expiry policy an operator cannot see;
//! - **fail closed**: every failure (timeout, `rcode`, empty answer, malformed
//!   reply) is an error, never a silent fall back to the system resolver. A
//!   fallback would resolve through a resolver the operator explicitly did not
//!   choose, which is the exact confusion the setting exists to remove.
//!
//! The wire format is implemented here rather than pulled in as a dependency
//! because the surface is two record types and the alternative is a resolver
//! library with caching, trust anchors, and its own configuration file.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

/// Longest DNS name accepted in a query (`a.b.c` in wire form).
const MAX_QUERY_NAME_LEN: usize = 255;
/// Response buffer: the classic UDP limit, and enough for any real answer to a
/// single-question lookup.
const RESPONSE_BUFFER_BYTES: usize = 512;
/// Query timeout. Short: the caller is a reconnect loop that already retries,
/// and a resolver that has not answered in a few seconds is down.
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

const TYPE_A: u16 = 1;
const TYPE_AAAA: u16 = 28;
const CLASS_IN: u16 = 1;

/// A configured DNS server (FRP `dnsServer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsServer {
    address: SocketAddr,
}

impl DnsServer {
    /// Parses `ip`, `ip:port` (default port 53).
    ///
    /// A hostname is refused: resolving the resolver is a dependency loop, and
    /// the system resolver would be back in the trust path the operator just
    /// removed.
    pub fn parse(raw: &str) -> Result<Self, String> {
        // One parser for the whole workspace: the config layer validates with
        // it, so a value that passes validation cannot fail here.
        let address = sdkwork_webserver_tunnel_core::parse_dns_server(raw)?;
        Ok(Self { address })
    }

    /// The server's socket address.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Resolves `host`, preferring an IPv4 answer.
    ///
    /// IPv4 first because a dual-stack answer usually means "either works" and
    /// the v4 path is the one that exists on every deployment this ships to.
    pub async fn resolve(&self, host: &str) -> Result<IpAddr, String> {
        let host = host.trim();
        if host.is_empty() {
            return Err("cannot resolve an empty host".to_owned());
        }
        // An IP literal needs no resolver; accepting one here keeps the caller
        // from special-casing it.
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(ip);
        }
        let mut v6_fallback: Option<IpAddr> = None;
        for record in [TYPE_A, TYPE_AAAA] {
            match self.query(host, record).await {
                Ok(addresses) if !addresses.is_empty() => {
                    if let Some(v4) = addresses.iter().find(|ip| ip.is_ipv4()) {
                        return Ok(*v4);
                    }
                    v6_fallback = addresses.first().copied();
                }
                Ok(_) => {}
                Err(error) => {
                    // A hard failure on the first record type is reported; an
                    // absent AAAA is not a failure when A already answered.
                    if record == TYPE_A {
                        return Err(error);
                    }
                }
            }
        }
        v6_fallback.ok_or_else(|| format!("dnsServer returned no address for `{host}`"))
    }

    /// Sends one question and returns the addresses in its answer section.
    async fn query(&self, host: &str, record_type: u16) -> Result<Vec<IpAddr>, String> {
        let id = query_id();
        let query = build_query(id, host, record_type)?;
        let socket = tokio::net::UdpSocket::bind(bind_address_for(self.address))
            .await
            .map_err(|error| format!("dnsServer socket bind failed: {error}"))?;
        socket
            .connect(self.address)
            .await
            .map_err(|error| format!("dnsServer {} unreachable: {error}", self.address))?;
        socket
            .send(&query)
            .await
            .map_err(|error| format!("dnsServer {} write failed: {error}", self.address))?;
        let mut buffer = [0_u8; RESPONSE_BUFFER_BYTES];
        let read = tokio::time::timeout(QUERY_TIMEOUT, socket.recv(&mut buffer))
            .await
            .map_err(|_| format!("dnsServer {} did not answer within 5s", self.address))?
            .map_err(|error| format!("dnsServer {} read failed: {error}", self.address))?;
        parse_response(&buffer[..read], id)
    }
}

/// The local address a query is sent from: the same family as the server.
fn bind_address_for(server: SocketAddr) -> SocketAddr {
    if server.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from(([0u16, 0, 0, 0, 0, 0, 0, 0], 0))
    }
}

/// A per-query transaction id.
///
/// Derived from the clock rather than a counter: two agents (or two retries)
/// must not collide on one resolver, and the id only has to make an unrelated
/// datagram recognisable as unrelated.
fn query_id() -> u16 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    u16::try_from(nanos % u32::from(u16::MAX))
        .unwrap_or(1)
        .max(1)
}

/// Encodes one query: header, one question, no additional records.
fn build_query(id: u16, host: &str, record_type: u16) -> Result<Vec<u8>, String> {
    let mut query = Vec::with_capacity(32 + host.len());
    query.extend_from_slice(&id.to_be_bytes());
    // Recursion desired, everything else clear.
    query.extend_from_slice(&0x0100_u16.to_be_bytes());
    query.extend_from_slice(&1_u16.to_be_bytes()); // QDCOUNT
    query.extend_from_slice(&0_u16.to_be_bytes()); // ANCOUNT
    query.extend_from_slice(&0_u16.to_be_bytes()); // NSCOUNT
    query.extend_from_slice(&0_u16.to_be_bytes()); // ARCOUNT
    let name = host.trim_end_matches('.');
    if name.is_empty() || name.len() > MAX_QUERY_NAME_LEN {
        return Err(format!("`{host}` is not a usable DNS name"));
    }
    for label in name.split('.') {
        let bytes = label.as_bytes();
        if bytes.is_empty() || bytes.len() > 63 {
            return Err(format!("`{host}` has an empty or oversized label"));
        }
        query.push(u8::try_from(bytes.len()).map_err(|_| "label too long".to_owned())?);
        query.extend_from_slice(bytes);
    }
    query.push(0); // root label
    query.extend_from_slice(&record_type.to_be_bytes());
    query.extend_from_slice(&CLASS_IN.to_be_bytes());
    Ok(query)
}

/// Parses a response, returning the address records it carries.
fn parse_response(response: &[u8], expected_id: u16) -> Result<Vec<IpAddr>, String> {
    if response.len() < 12 {
        return Err("dnsServer reply is too short to be a DNS message".to_owned());
    }
    let id = u16::from_be_bytes([response[0], response[1]]);
    if id != expected_id {
        return Err("dnsServer reply does not match the query".to_owned());
    }
    let flags = u16::from_be_bytes([response[2], response[3]]);
    if flags & 0x8000 == 0 {
        return Err("dnsServer reply is not a response".to_owned());
    }
    let rcode = flags & 0x000F;
    if rcode != 0 {
        // 3 is NXDOMAIN; anything else is a resolver-side failure. Both are
        // "this name has no address for us".
        return Err(match rcode {
            3 => "dnsServer answered NXDOMAIN".to_owned(),
            other => format!("dnsServer answered with rcode {other}"),
        });
    }
    let questions = usize::from(u16::from_be_bytes([response[4], response[5]]));
    let answers = usize::from(u16::from_be_bytes([response[6], response[7]]));
    let mut offset = 12;
    for _ in 0..questions {
        offset = skip_name(response, offset)?;
        // QTYPE + QCLASS
        offset = offset
            .checked_add(4)
            .filter(|offset| *offset <= response.len())
            .ok_or_else(|| "dnsServer reply is truncated in its question".to_owned())?;
    }
    let mut addresses = Vec::new();
    for _ in 0..answers {
        offset = skip_name(response, offset)?;
        if offset + 10 > response.len() {
            return Err("dnsServer reply is truncated in an answer".to_owned());
        }
        let record_type = u16::from_be_bytes([response[offset], response[offset + 1]]);
        let class = u16::from_be_bytes([response[offset + 2], response[offset + 3]]);
        let length = usize::from(u16::from_be_bytes([
            response[offset + 8],
            response[offset + 9],
        ]));
        offset += 10;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= response.len())
            .ok_or_else(|| "dnsServer reply is truncated in its record data".to_owned())?;
        if class == CLASS_IN {
            match (record_type, length) {
                (TYPE_A, 4) => addresses.push(IpAddr::V4(Ipv4Addr::new(
                    response[offset],
                    response[offset + 1],
                    response[offset + 2],
                    response[offset + 3],
                ))),
                (TYPE_AAAA, 16) => {
                    let mut octets = [0_u8; 16];
                    octets.copy_from_slice(&response[offset..end]);
                    addresses.push(IpAddr::V6(Ipv6Addr::from(octets)));
                }
                // CNAME and friends: parsed past, not followed. The resolver
                // answers with the records it chose to send; following a chain
                // would mean a second query against a server the operator did
                // not configure.
                _ => {}
            }
        }
        offset = end;
    }
    Ok(addresses)
}

/// Advances past a (possibly compressed) name.
fn skip_name(message: &[u8], mut offset: usize) -> Result<usize, String> {
    loop {
        let length = *message
            .get(offset)
            .ok_or_else(|| "dnsServer reply is truncated in a name".to_owned())?;
        offset += 1;
        if length == 0 {
            return Ok(offset);
        }
        // Compression pointer: two bytes, and the name ends here as far as the
        // walk is concerned.
        if length & 0xC0 == 0xC0 {
            return offset
                .checked_add(1)
                .filter(|offset| *offset <= message.len())
                .ok_or_else(|| "dnsServer reply is truncated in a name pointer".to_owned());
        }
        offset = offset
            .checked_add(usize::from(length))
            .filter(|offset| *offset <= message.len())
            .ok_or_else(|| "dnsServer reply is truncated in a label".to_owned())?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a response for `host` carrying `records`.
    fn response(id: u16, host: &str, records: &[(u16, Vec<u8>)], rcode: u16) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(&id.to_be_bytes());
        message.extend_from_slice(&(0x8180_u16 | rcode).to_be_bytes());
        message.extend_from_slice(&1_u16.to_be_bytes()); // QDCOUNT
        message.extend_from_slice(
            &u16::try_from(records.len())
                .expect("record count")
                .to_be_bytes(),
        );
        message.extend_from_slice(&0_u16.to_be_bytes());
        message.extend_from_slice(&0_u16.to_be_bytes());
        // Question, with a compression pointer in the answer name below.
        let question_start = message.len();
        for label in host.split('.') {
            message.push(u8::try_from(label.len()).expect("label"));
            message.extend_from_slice(label.as_bytes());
        }
        message.push(0);
        message.extend_from_slice(&TYPE_A.to_be_bytes());
        message.extend_from_slice(&CLASS_IN.to_be_bytes());
        for (record_type, data) in records {
            // Pointer to the question name: the parser must cope with the
            // compression every real resolver uses.
            message.push(0xC0);
            message.push(u8::try_from(question_start).expect("offset"));
            message.extend_from_slice(&record_type.to_be_bytes());
            message.extend_from_slice(&CLASS_IN.to_be_bytes());
            message.extend_from_slice(&60_u32.to_be_bytes()); // TTL
            message.extend_from_slice(&u16::try_from(data.len()).expect("rdlength").to_be_bytes());
            message.extend_from_slice(data);
        }
        message
    }

    #[test]
    fn a_query_encodes_the_name_and_type() {
        let query = build_query(0x1234, "gw.sdkwork.link", TYPE_A).expect("encodes");
        assert_eq!(&query[0..2], &[0x12, 0x34]);
        assert_eq!(&query[2..4], &[0x01, 0x00], "recursion desired");
        assert_eq!(&query[4..6], &[0x00, 0x01], "one question");
        assert_eq!(query[12], 2, "first label length");
        assert_eq!(&query[13..15], b"gw");
        assert_eq!(query[15], 7);
        assert_eq!(&query[16..23], b"sdkwork");
        assert_eq!(query[23], 4);
        assert_eq!(&query[24..28], b"link");
        assert_eq!(query[28], 0, "root label");
        assert_eq!(&query[29..31], &[0x00, 0x01], "QTYPE A");
        assert_eq!(&query[31..33], &[0x00, 0x01], "QCLASS IN");
        // A trailing dot is accepted; empty labels are not.
        assert!(build_query(1, "gw.sdkwork.link.", TYPE_A).is_ok());
        assert!(build_query(1, "gw..link", TYPE_A).is_err());
        assert!(build_query(1, "", TYPE_A).is_err());
    }

    #[test]
    fn an_address_response_is_parsed_through_name_compression() {
        let message = response(0x4321, "gw.sdkwork.link", &[(TYPE_A, vec![10, 1, 2, 3])], 0);
        assert_eq!(
            parse_response(&message, 0x4321).expect("parses"),
            vec![IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))]
        );

        let v6 = response(
            7,
            "gw.sdkwork.link",
            &[(TYPE_AAAA, std::net::Ipv6Addr::LOCALHOST.octets().to_vec())],
            0,
        );
        assert_eq!(
            parse_response(&v6, 7).expect("parses"),
            vec![IpAddr::V6(Ipv6Addr::LOCALHOST)]
        );
    }

    #[test]
    fn unusable_responses_are_errors_never_a_silent_empty_answer() {
        // Wrong transaction id: an answer to someone else's question.
        let message = response(1, "gw.test", &[(TYPE_A, vec![10, 0, 0, 1])], 0);
        assert!(parse_response(&message, 2).is_err());

        // NXDOMAIN and a generic failure are both errors.
        let nxdomain = response(3, "gw.test", &[], 3);
        let error = parse_response(&nxdomain, 3).expect_err("nxdomain");
        assert!(error.contains("NXDOMAIN"), "{error}");
        let servfail = response(4, "gw.test", &[], 2);
        assert!(parse_response(&servfail, 4).is_err());

        // Truncated, and a query-shaped message.
        assert!(parse_response(&[0, 1, 2], 1).is_err());
        let mut not_a_response = response(5, "gw.test", &[], 0);
        not_a_response[2] = 0x01; // clear QR
        assert!(parse_response(&not_a_response, 5).is_err());

        // A record whose length runs past the buffer.
        let mut truncated = response(6, "gw.test", &[(TYPE_A, vec![10, 0, 0, 1])], 0);
        let length = truncated.len();
        truncated.truncate(length - 2);
        assert!(parse_response(&truncated, 6).is_err());
    }

    #[test]
    fn a_configured_server_is_an_ip_with_an_optional_port() {
        let server = DnsServer::parse("10.0.0.53").expect("bare ip");
        assert_eq!(server.address().to_string(), "10.0.0.53:53");
        let server = DnsServer::parse("10.0.0.53:5353").expect("ip and port");
        assert_eq!(server.address().to_string(), "10.0.0.53:5353");
        let server = DnsServer::parse("[::1]:5353").expect("v6");
        assert!(server.address().is_ipv6());
        // Resolving the resolver is a dependency loop, and an empty value is a
        // typo, so both are refused.
        assert!(DnsServer::parse("resolver.internal").is_err());
        assert!(DnsServer::parse("").is_err());
    }
}
