//! SDKWork Web Server tunnel domain model.
//!
//! The pure domain layer of the tunnel capability: strongly typed
//! identifiers, device identity, session lifecycle state, route and matcher
//! semantics, access policy, tunnel configuration, and the crate error type.
//!
//! Cohesion boundary (SDKWORK WebServer Tunnel PRD §10.1, §43): this crate
//! depends on serialization and clock types only. QUIC/TLS transports,
//! HTTP wiring, the webserver runtime, and tokio networking are forbidden
//! here; upper layers ([`sdkwork_webserver_tunnel_protocol`],
//! [`sdkwork_webserver_tunnel_transport`], [`sdkwork_webserver_tunnel`])
//! consume these types instead of redefining them.

pub mod config;
pub mod device;
pub mod error;
pub mod ids;
pub mod policy;
pub mod route;
pub mod session;

pub use config::{
    TunnelAgentConfig, TunnelAgentTlsConfig, TunnelConfig, TunnelGatewayConfig, TunnelLimitsConfig,
    TunnelNetworkConfig, TunnelRouteTemplate, TunnelTimeoutConfig, DEFAULT_HEARTBEAT_INTERVAL_SECS,
    DEFAULT_TUNNEL_PORT,
};
pub use device::{Device, DevicePlatform};
pub use error::{Result, TunnelError, ValidationField};
pub use ids::{DeviceId, RouteId, SessionId, StreamId};
pub use policy::{parse_allowed_ips, AuthPolicy, RoutePolicy};
pub use route::{
    local_target, local_udp_target, public_url, RouteMatcher, TunnelProtocolKind, TunnelRoute,
    TunnelTarget,
};
pub use session::{SessionSnapshot, SessionState, TunnelSession};

/// Protocol name advertised during the STP handshake.
pub const TUNNEL_PROTOCOL_NAME: &str = "STP";

/// The only protocol version this release speaks. Version negotiation in
/// [`sdkwork_webserver_tunnel_protocol`] keeps the wire structure open for
/// STP/2 without changing this constant.
pub const TUNNEL_PROTOCOL_VERSION: u32 = 1;
