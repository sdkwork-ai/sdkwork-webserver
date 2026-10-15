//! Data-plane stream header: the first frame on every data stream (PRD §20,
//! §21).
//!
//! After the header the stream carries raw application bytes in both
//! directions — no further framing. The agent uses `route_id` to select the
//! local target; `stream_id` names the stream in logs and metrics.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use sdkwork_webserver_tunnel_core::{RouteId, StreamId, TunnelError};

use crate::frame;

/// The visitor address a data stream is relaying, as the gateway observed it.
///
/// The agent needs it to speak the PROXY protocol toward its local target
/// (FRP `transport.proxyProtocolVersion` parity) and to log which visitor a
/// stream belongs to; the gateway is the only party that can see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisitorAddress {
    /// Visitor IP, after the caller's trusted-proxy resolution.
    pub ip: IpAddr,
    /// Visitor source port, or 0 when the plane that opened the stream does
    /// not expose it (the HTTP relay knows the address, not the socket). The
    /// PROXY protocol defines port 0 as "unknown", so it stays renderable.
    pub port: u16,
}

impl VisitorAddress {
    /// Builds a visitor address from its parts.
    pub const fn new(ip: IpAddr, port: u16) -> Self {
        Self { ip, port }
    }
}

/// First frame on a **visitor** stream: a client asking for a secret route by
/// name (FRP `stcp`/`sudp` visitor parity).
///
/// The gateway accepts these streams on any authenticated session, so the
/// header carries everything needed to authorize the request: the route name,
/// the shared key, and the protocol the visitor expects. After the header the
/// stream is raw bytes (or framed datagrams for a datagram route).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisitorStreamHeader {
    /// Route name the visitor asks for (the provider's template name).
    pub name: String,
    /// Shared secret the provider registered (FRP `sk`).
    pub secret_key: String,
    /// `stcp` or `sudp`: the gateway refuses a request whose protocol does not
    /// match the route's, so a datagram visitor can never be handed a stream.
    pub protocol: String,
    /// Visitor-assigned stream sequence, for logs and metrics.
    pub stream_id: u64,
}

impl VisitorStreamHeader {
    /// Serializes to the JSON payload form.
    pub fn encode(&self) -> Result<Vec<u8>, TunnelError> {
        serde_json::to_vec(self).map_err(|error| {
            TunnelError::Protocol(format!("visitor stream header failed to encode: {error}"))
        })
    }

    /// Parses a payload into a header.
    pub fn decode(payload: &[u8]) -> Result<Self, TunnelError> {
        serde_json::from_slice(payload).map_err(|error| {
            TunnelError::Protocol(format!("visitor stream header failed to decode: {error}"))
        })
    }

    /// Frames the header for the wire.
    pub fn to_frame(&self) -> Result<bytes::Bytes, TunnelError> {
        frame::frame_bytes(&self.encode()?)
    }
}

/// First frame on a data stream identifying the route it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStreamHeader {
    /// Route the gateway matched for this stream.
    pub route_id: String,
    /// Gateway-assigned per-session stream sequence.
    pub stream_id: u64,
    /// Visitor this stream relays, when the gateway knows it. Absent for
    /// streams opened without a visitor identity (and for older gateways).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visitor: Option<VisitorAddress>,
}

impl DataStreamHeader {
    /// Builds a header from domain identities.
    pub fn new(route_id: &RouteId, stream_id: StreamId) -> Self {
        Self {
            route_id: route_id.to_string(),
            stream_id: stream_id.value(),
            visitor: None,
        }
    }

    /// Builds a header that also carries the visitor address.
    pub fn with_visitor(
        route_id: &RouteId,
        stream_id: StreamId,
        visitor: Option<VisitorAddress>,
    ) -> Self {
        Self {
            route_id: route_id.to_string(),
            stream_id: stream_id.value(),
            visitor,
        }
    }

    /// The route identity parsed back from the wire form.
    pub fn route(&self) -> Result<RouteId, TunnelError> {
        RouteId::parse(&self.route_id)
    }

    /// The stream sequence number.
    pub const fn stream(&self) -> StreamId {
        StreamId::new(self.stream_id)
    }

    /// Serializes to the JSON payload form.
    pub fn encode(&self) -> Result<Vec<u8>, TunnelError> {
        serde_json::to_vec(self).map_err(|error| {
            TunnelError::Protocol(format!("stream header failed to encode: {error}"))
        })
    }

    /// Parses a payload into a header.
    pub fn decode(payload: &[u8]) -> Result<Self, TunnelError> {
        serde_json::from_slice(payload).map_err(|error| {
            TunnelError::Protocol(format!("stream header failed to decode: {error}"))
        })
    }

    /// Frames the header for the wire.
    pub fn to_frame(&self) -> Result<bytes::Bytes, TunnelError> {
        frame::frame_bytes(&self.encode()?)
    }

    /// Parses one frame payload into a header.
    pub fn from_frame(payload: &[u8]) -> Result<Self, TunnelError> {
        Self::decode(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let route = RouteId::parse("route_web").expect("valid id");
        let header = DataStreamHeader::new(&route, StreamId::new(7));
        let decoded = DataStreamHeader::decode(&header.encode().expect("encode")).expect("decode");
        assert_eq!(decoded, header);
        assert_eq!(decoded.route().expect("valid route"), route);
        assert_eq!(decoded.stream().value(), 7);
    }

    #[test]
    fn header_frames_round_trip() {
        let route = RouteId::parse("route_ssh").expect("valid id");
        let header = DataStreamHeader::new(&route, StreamId::FIRST);
        let frame = header.to_frame().expect("frame");
        let (payload, consumed) = frame::decode_frame(&frame)
            .expect("parse")
            .expect("complete");
        assert_eq!(consumed, frame.len());
        assert_eq!(
            DataStreamHeader::from_frame(&payload).expect("decode"),
            header
        );
    }

    #[test]
    fn malformed_header_is_rejected() {
        assert!(DataStreamHeader::decode(b"[]").is_err());
    }

    #[test]
    fn a_stream_header_carries_the_visitor_when_known() {
        let route = RouteId::parse("route_ssh").expect("valid id");
        let visitor = VisitorAddress::new("203.0.113.5".parse().expect("ip"), 51000);
        let header = DataStreamHeader::with_visitor(&route, StreamId::new(3), Some(visitor));
        let decoded = DataStreamHeader::decode(&header.encode().expect("encode")).expect("decode");
        assert_eq!(decoded.visitor, Some(visitor));
        assert_eq!(decoded.route().expect("route"), route);

        // A header without a visitor stays omissible on the wire, so a
        // pre-existing gateway's frames keep parsing.
        let bare = DataStreamHeader::new(&route, StreamId::new(4));
        let payload = String::from_utf8(bare.encode().expect("encode")).expect("utf8");
        assert!(!payload.contains("visitor"), "{payload}");
        let legacy: DataStreamHeader =
            serde_json::from_str(r#"{"routeId":"route_ssh","streamId":5}"#)
                .expect("legacy header decodes");
        assert_eq!(legacy.visitor, None);
    }
}
