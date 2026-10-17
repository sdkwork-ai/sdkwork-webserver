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
pub mod headers;
pub mod health;
pub mod ids;
pub mod policy;
pub mod proxy_protocol;
pub mod route;
pub mod session;
pub mod verify;

pub use config::{
    parse_dns_server, validate_endpoint_authority, TunnelAgentConfig, TunnelAgentTlsConfig,
    TunnelConfig, TunnelGatewayConfig, TunnelLimitsConfig, TunnelNetworkConfig,
    TunnelRouteTemplate, TunnelTimeoutConfig, TunnelUserConfig, DEFAULT_HEARTBEAT_INTERVAL_SECS,
    DEFAULT_PROXY_BIND_ADDR, DEFAULT_TUNNEL_PORT,
};
pub use device::{validate_user_name, Device, DevicePlatform, MAX_USER_NAME_LEN};
pub use error::{Result, TunnelError, ValidationField};
pub use headers::{
    HeaderDirection, HeaderPair, HeaderSetConfig, RouteHeaderRewrite, MAX_HEADER_PAIRS,
};
pub use health::{
    validate_load_balancer_group, HealthCheckConfig, HealthCheckKind, DEFAULT_HEALTH_INTERVAL_SECS,
    DEFAULT_HEALTH_MAX_FAILED, DEFAULT_HEALTH_TIMEOUT_SECS,
};
pub use ids::{DeviceId, RouteId, SessionId, StreamId};
pub use policy::{parse_allowed_ips, AuthPolicy, AuthScope, RoutePolicy};
pub use proxy_protocol::ProxyProtocolVersion;
pub use route::{
    derived_route_id, local_target, local_udp_target, location_matches, namespaced_name,
    parse_bandwidth_limit, public_url, validate_location, RouteMatcher, TunnelProtocolKind,
    TunnelRoute, TunnelTarget,
};
pub use session::{SessionSnapshot, SessionState, TunnelSession};
pub use verify::{
    registered_matcher, report_to_result, verify_config, verify_template_standalone, Finding,
    Severity, VerificationReport,
};

/// Protocol name advertised during the STP handshake.
pub const TUNNEL_PROTOCOL_NAME: &str = "STP";

/// The only protocol version this release speaks. Version negotiation in
/// [`sdkwork_webserver_tunnel_protocol`] keeps the wire structure open for
/// STP/2 without changing this constant.
pub const TUNNEL_PROTOCOL_VERSION: u32 = 1;
