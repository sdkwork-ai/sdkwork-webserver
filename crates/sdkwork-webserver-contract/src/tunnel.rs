//! Tunnel machine (FRP-style connector node) wire contracts.
//!
//! A *machine* is one connector node the tunnel gateway manages: a device
//! running the agent, its liveness, the routes it serves, the traffic it moved
//! and the operator switch that takes it out of service. The management plane
//! does not own that state — the gateway process does — so these contracts
//! describe what an admin console may read and change, not where it is stored.
//!
//! Route declarations created from here are **declarations only**: the agent
//! activates a declared route only when it matches one of its own local
//! templates and always dials its own configured target (PRD §45 SSRF
//! boundary). The request therefore carries no target at all.

use serde::{Deserialize, Serialize};

/// Wire enum: relay kinds a machine route may use (mirrors the tunnel's own
/// protocol labels).
pub const TUNNEL_MACHINE_PROTOCOLS: [&str; 5] = ["http", "tcp", "udp", "stcp", "sudp"];

/// Wire enum: visitor admission policy labels (`none`, `allowed_ips`,
/// `bearer_token`).
pub const TUNNEL_MACHINE_AUTH_POLICIES: [&str; 3] = ["none", "allowed_ips", "bearer_token"];

/// Wire enum: PROXY protocol versions a route may prepend (`v1`, `v2`).
pub const TUNNEL_MACHINE_PROXY_PROTOCOLS: [&str; 2] = ["v1", "v2"];

/// One route a machine currently serves.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelMachineRouteResponse {
    /// Gateway route id (`route_<name>`).
    pub route_id: String,
    /// Operator-facing route name.
    pub name: String,
    /// Relay kind (`http` | `tcp` | `udp` | `stcp` | `sudp`).
    pub protocol: String,
    /// Host-routed domain, for `http` routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Gateway port, for `tcp`/`udp` routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Secret-route name, for `stcp`/`sudp` routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_name: Option<String>,
    /// Path prefixes this route answers (empty = every path of the host).
    #[serde(default)]
    pub locations: Vec<String>,
    /// Load-balancer group, when this route is a replica of one service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,
    /// Per-direction relay ceiling in bytes per second.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "sdkwork_utils_rust::serde_int64::option"
    )]
    pub bandwidth_limit_bytes_per_second: Option<i64>,
    /// PROXY protocol version prepended toward the machine's local target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<String>,
    /// Whether any visitor the network policy admits may use the route.
    pub allow_public: bool,
    /// Visitor admission policy (`none` | `allowed_ips` | `bearer_token`).
    pub auth: String,
    /// Visitor network allow-list, when the policy uses one.
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    /// Whether the machine's own probe currently reports the route healthy.
    pub healthy: bool,
}

/// One managed machine.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelMachineResponse {
    /// Device identity reported by the agent.
    pub device_id: String,
    /// Display name reported by the agent.
    pub device_name: String,
    /// Platform label (`linux`, `windows`, `macos`, `other`).
    pub platform: String,
    /// Whether a session is live right now.
    pub online: bool,
    /// Operator switch: a disabled machine is refused at authentication.
    pub enabled: bool,
    /// Live session id, when online.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// First time this gateway saw the machine (RFC 3339).
    pub first_seen_at: String,
    /// Last heartbeat or registration (RFC 3339).
    pub last_seen_at: String,
    /// When the current (or last) session started (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<String>,
    /// When the last session ended (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disconnected_at: Option<String>,
    /// Peer address of the last session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
    /// Bytes relayed from visitors toward the machine.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub bytes_in: i64,
    /// Bytes relayed from the machine toward visitors.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub bytes_out: i64,
    /// Sessions this machine opened since the gateway started.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub sessions_total: i64,
    /// Routes the machine serves right now.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub route_count: i64,
    /// Routes whose last probe reported them healthy.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub healthy_route_count: i64,
    /// Routes currently reported unhealthy.
    #[serde(default)]
    pub unhealthy_route_ids: Vec<String>,
    /// The routes themselves.
    #[serde(default)]
    pub routes: Vec<TunnelMachineRouteResponse>,
}

/// Machine list payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelMachineListResponse {
    /// Managed machines, most recently seen first.
    #[serde(default)]
    pub machines: Vec<TunnelMachineResponse>,
}

/// Operator switch request: `{"enabled": false}` takes the machine out of
/// service immediately (its live session is closed and further sessions are
/// refused).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTunnelMachineEnabledRequest {
    /// Desired switch state.
    pub enabled: bool,
}

/// Route declaration for one machine.
///
/// There is deliberately **no target field**: the agent dials its own
/// configured target for the template it matches, so a console can publish a
/// route but never redirect a machine's traffic somewhere else.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTunnelMachineRouteRequest {
    /// Route name; must match a template configured on the machine.
    pub name: String,
    /// Relay kind (`http` | `tcp` | `udp` | `stcp` | `sudp`).
    pub protocol: String,
    /// Host-routed domain, for `http`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Gateway port, for `tcp`/`udp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Whether any visitor the network policy admits may use the route.
    #[serde(default)]
    pub allow_public: bool,
    /// Visitor network allow-list.
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    /// Visitor admission policy (`none` | `allowed_ips` | `bearer_token`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    /// Visitor bearer tokens, when the policy is `bearer_token`.
    #[serde(default)]
    pub visitor_tokens: Vec<String>,
    /// Path prefixes the route answers.
    #[serde(default)]
    pub locations: Vec<String>,
    /// Per-direction relay ceiling in bytes per second.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "sdkwork_utils_rust::serde_int64::option"
    )]
    pub bandwidth_limit_bytes_per_second: Option<i64>,
    /// PROXY protocol version to prepend toward the machine's target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<String>,
    /// Load-balancer group, when this route is a replica of one service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,
}

/// Outcome of a route declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelMachineRouteDeclarationResponse {
    /// Derived route id.
    pub route_id: String,
    /// Whether the machine is serving the route right now. A queued
    /// declaration (`false`) becomes active when the machine connects **and**
    /// the route matches one of its local templates.
    pub active: bool,
    /// Public URL for `http` routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    /// Detail when the declaration could not be activated now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
