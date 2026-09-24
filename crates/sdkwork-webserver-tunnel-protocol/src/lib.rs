//! SDKWORK Tunnel Protocol (STP/1) wire layer (PRD §18–§21).
//!
//! The protocol is independent from the domain model: every wire type lives
//! here, converts explicitly to and from [`sdkwork_webserver_tunnel_core`]
//! domain types, and travels over the transport crate's streams.
//!
//! Layout on the wire:
//!
//! - **Control plane** (one long-lived bidirectional stream per session):
//!   length-prefixed JSON control messages (4-byte big-endian length).
//! - **Data plane** (one bidirectional stream per visitor connection): one
//!   length-prefixed [`DataStreamHeader`] frame, then raw application bytes.
//!   Business data is never base64- or JSON-encoded (PRD §20).

pub mod frame;
pub mod message;
pub mod packet_frame;
pub mod stream_header;

pub use frame::{
    decode_frame, encode_frame, read_exact_frame, read_frame, write_frame, FRAME_LENGTH_BYTES,
    MAX_FRAME_LENGTH,
};
pub use message::{
    route_from_wire, session_from_wire, session_to_wire, AuthResult, Authenticate, ControlMessage,
    DeclareRoute, ErrorCode, ErrorMessage, Hello, ProtocolVersion, RegisterRoute,
    RegisterRouteResult, UnregisterRoute, UnregisterRouteResult,
};
pub use packet_frame::{
    decode_packet, encode_packet, read_packet, MAX_PACKET_PAYLOAD, PACKET_LENGTH_BYTES,
};
pub use stream_header::DataStreamHeader;
