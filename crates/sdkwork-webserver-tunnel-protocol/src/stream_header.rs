//! Data-plane stream header: the first frame on every data stream (PRD §20,
//! §21).
//!
//! After the header the stream carries raw application bytes in both
//! directions — no further framing. The agent uses `route_id` to select the
//! local target; `stream_id` names the stream in logs and metrics.

use serde::{Deserialize, Serialize};

use sdkwork_webserver_tunnel_core::{RouteId, StreamId, TunnelError};

use crate::frame;

/// First frame on a data stream identifying the route it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStreamHeader {
    /// Route the gateway matched for this stream.
    pub route_id: String,
    /// Gateway-assigned per-session stream sequence.
    pub stream_id: u64,
}

impl DataStreamHeader {
    /// Builds a header from domain identities.
    pub fn new(route_id: &RouteId, stream_id: StreamId) -> Self {
        Self {
            route_id: route_id.to_string(),
            stream_id: stream_id.value(),
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
}
