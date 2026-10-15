//! STP/1 control-plane messages (PRD §19) and protocol version negotiation
//! (PRD §103, §104).
//!
//! Every control message carries its protocol version so an STP/2 gateway
//! can keep answering STP/1 agents. Unknown message types decode into
//! [`ControlMessage::Error`] with [`ErrorCode::UnsupportedMessage`] instead
//! of failing the session, which is the N-1 compatibility seam.

use serde::{Deserialize, Serialize};

use sdkwork_webserver_tunnel_core::{DevicePlatform, RouteId, SessionId, TunnelError};

use crate::frame;

/// Protocol version tuple exchanged in `Hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolVersion {
    /// Protocol family; always `"STP"`.
    pub name: String,
    /// Numeric protocol version; `1` for STP/1.
    pub version: u32,
}

impl ProtocolVersion {
    /// The STP/1 version pair this crate implements.
    pub fn v1() -> Self {
        Self {
            name: sdkwork_webserver_tunnel_core::TUNNEL_PROTOCOL_NAME.to_owned(),
            version: sdkwork_webserver_tunnel_core::TUNNEL_PROTOCOL_VERSION,
        }
    }

    /// True when this version pair is acceptable to an STP/1 peer.
    pub fn is_acceptable(&self) -> bool {
        self.name == sdkwork_webserver_tunnel_core::TUNNEL_PROTOCOL_NAME
            && self.version == sdkwork_webserver_tunnel_core::TUNNEL_PROTOCOL_VERSION
    }
}

/// Machine-readable failure category carried by control errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Credentials missing or invalid.
    AuthenticationFailed,
    /// The identity may not perform the operation.
    AuthorizationDenied,
    /// The route definition was rejected.
    InvalidRoute,
    /// The route is owned by another session.
    RouteConflict,
    /// The referenced route does not exist.
    RouteNotFound,
    /// A server limit rejected the operation.
    ResourceLimit,
    /// The protocol version or message is not supported.
    UnsupportedProtocol,
    /// Anything else; see the human-readable detail.
    Internal,
}

impl ErrorCode {
    /// Maps a domain error onto the wire error category.
    pub fn from_error(error: &TunnelError) -> Self {
        match error {
            TunnelError::AuthenticationFailed => Self::AuthenticationFailed,
            TunnelError::AuthorizationDenied => Self::AuthorizationDenied,
            TunnelError::InvalidRoute(_) => Self::InvalidRoute,
            TunnelError::RouteConflict(_) => Self::RouteConflict,
            TunnelError::RouteNotFound => Self::RouteNotFound,
            TunnelError::ResourceLimit(_) => Self::ResourceLimit,
            TunnelError::Protocol(_) => Self::UnsupportedProtocol,
            TunnelError::Validation { .. }
            | TunnelError::ConnectionFailed(_)
            | TunnelError::ConnectionClosed
            | TunnelError::DeviceNotFound
            | TunnelError::SessionNotFound
            | TunnelError::TargetUnreachable(_)
            // A load-balancer group with no healthy replica is an availability
            // fact, not a protocol or credential problem.
            | TunnelError::NoHealthyReplica
            | TunnelError::Timeout(_) => Self::Internal,
        }
    }
}

/// Agent → gateway handshake opening: identity and protocol negotiation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    /// Versions the agent accepts, best first.
    pub protocol_versions: Vec<ProtocolVersion>,
    /// Device identity (gateway still verifies the token).
    pub device_id: String,
    /// Device display name.
    pub device_name: String,
    /// Reported platform label.
    pub platform: String,
    /// User this machine claims to serve (FRP `user`).
    ///
    /// A request, not a fact: the gateway resolves the effective user from the
    /// token and refuses a contradictory claim, so the field can never be used
    /// to spend another team's quota. Absent means "no user", which is what an
    /// agent predating the field sends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// Agent → gateway credential presentation (PRD §25).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authenticate {
    /// Bearer token; serialized form is transient and must not be logged.
    pub token: String,
}

/// Gateway → agent authentication outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResult {
    /// True when the session is authenticated.
    pub ok: bool,
    /// Session identity assigned by the gateway (present when `ok`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Negotiated protocol version (present when `ok`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ProtocolVersion>,
    /// Failure detail (present when `!ok`; never echoes the token).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Scopes the gateway requires the agent to re-authenticate in (FRP
    /// `auth.additionalScopes`).
    ///
    /// Advertised rather than configured twice: the agent attaches its token
    /// exactly where the gateway checks it, so the two sides cannot drift into
    /// "hardening enabled on one side only", which would lock every agent out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_scopes: Option<Vec<String>>,
}

/// Agent → gateway token re-presentation (FRP `auth.additionalScopes`
/// `HeartBeats`).
///
/// Sent alongside a heartbeat once the gateway advertised
/// [`AuthResult::required_scopes`]. A gateway that did not ask for it treats
/// the message as an ordinary unknown control message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reauthenticate {
    /// Bearer token, re-presented. Transient; never logged or echoed.
    pub token: String,
}

/// Agent → gateway route registration (PRD §55 hot updates).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRoute {
    /// Requested route id (agent-derived from the template name).
    pub route_id: String,
    /// Operator-facing route name.
    pub name: String,
    /// `http`, `tcp`, or `udp`.
    pub protocol: String,
    /// Public domain (HTTP routes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Public gateway port (TCP and UDP routes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Agent-local `ip:port` target.
    pub target: String,
    /// `allow_public` from the route policy.
    pub allow_public: bool,
    /// `allowed_ips` from the route policy as CIDR entries. The raw TCP and
    /// UDP planes cannot carry a bearer token, so this is their only usable
    /// admission channel; an empty or absent list leaves the network
    /// unrestricted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<String>>,
    /// `auth` from the route policy (`none` / `bearer_token`). Absent means
    /// `none`, so an agent that predates visitor-token support keeps
    /// registering exactly what it used to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    /// `visitor_tokens` from the route policy: the shared bearer tokens a
    /// visitor must present on an `auth = bearer_token` route. Without this
    /// field on the wire the gateway could never enforce the policy the
    /// agent declares, so a bearer-protected route registered as
    /// deny-everything instead of "token required".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visitor_tokens: Option<Vec<String>>,
    /// PROXY protocol version (`v1` / `v2`) the agent prepends toward its
    /// local target, so the gateway's route listing shows what the service
    /// behind the tunnel actually receives. Absent means no header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<String>,
    /// Path prefixes this route answers on its host (FRP `locations` parity).
    /// Absent or empty means every path of the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<String>>,
    /// Relay ceiling in bytes per second, each direction (FRP
    /// `transport.bandwidthLimit` parity). Absent means unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit_bytes_per_second: Option<u64>,
    /// Load-balancer group (FRP `loadBalancer.group` parity): registrations
    /// sharing a group and a matcher are replicas of one service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,
    /// Health probe the agent runs against its own target (FRP `healthCheck`
    /// parity). Absent means the gateway treats the route as always healthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check: Option<sdkwork_webserver_tunnel_core::HealthCheckConfig>,
    /// Shared secret for an `stcp`/`sudp` route (FRP `sk` parity). Present only
    /// on secret registrations: it travels inside the encrypted control plane
    /// and is never echoed back in a listing or a result message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
    /// HTTP header rewriting the gateway applies on this route's HTTP plane
    /// (FRP `hostHeaderRewrite` + `requestHeaders.set` / `responseHeaders.set`).
    ///
    /// It travels with the registration because the gateway is the only hop
    /// that parses HTTP: without it a route's rewrite policy would be invisible
    /// to the process that has to apply it. Absent means "rewrite nothing", so
    /// an agent predating this field keeps registering exactly what it used to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<sdkwork_webserver_tunnel_core::RouteHeaderRewrite>,
    /// Free-form operator metadata (FRP `annotations`).
    ///
    /// Absent means "none", and the gateway bounds what it accepts, so an agent
    /// cannot use it as unbounded per-route storage. Carried and listed, never
    /// interpreted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<std::collections::BTreeMap<String, String>>,
}

/// Gateway → agent registration outcome, including the public URL for HTTP
/// routes (PRD §36 response shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRouteResult {
    /// Echo of the requested route id.
    pub route_id: String,
    /// True when the route is active.
    pub ok: bool,
    /// Public URL (HTTP routes, present when `ok`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    /// Failure detail (present when `!ok`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Agent → gateway route removal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnregisterRoute {
    /// Route to remove.
    pub route_id: String,
}

/// Gateway → agent removal outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnregisterRouteResult {
    /// Echo of the removed route id.
    pub route_id: String,
    /// True when the route existed and was removed.
    pub ok: bool,
}

/// Gateway → agent request to activate a route (PRD §35 create-route API).
/// The agent answers by running its normal registration when — and only
/// when — the declaration matches one of its locally configured templates;
/// the target always comes from the agent's own configuration, never from
/// the gateway (PRD §45 SSRF boundary).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeclareRoute {
    /// Correlation id: the declared route's name (agents derive route ids
    /// from template names).
    pub route_id: String,
    /// Template name the agent must match.
    pub name: String,
    /// `http`, `tcp`, or `udp`.
    pub protocol: String,
    /// Public domain (HTTP routes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Public gateway port (TCP and UDP routes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Requested `allow_public` value.
    pub allow_public: bool,
    /// Requested `allowed_ips` as CIDR entries, forwarded to the agent so a
    /// declared route keeps the network restriction the operator asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<String>>,
    /// Requested `auth` (`none` / `bearer_token`). Absent leaves the agent's
    /// local template policy in charge, which is what an operator running an
    /// older control plane expects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    /// Requested `visitor_tokens` for an `auth = bearer_token` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visitor_tokens: Option<Vec<String>>,
    /// Requested PROXY protocol version (`v1` / `v2`) toward the local
    /// target. Absent leaves the agent's local template in charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<String>,
    /// Requested path prefixes (FRP `locations` parity). Absent leaves the
    /// agent's local template in charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<String>>,
    /// Requested relay ceiling in bytes per second, each direction. Absent
    /// leaves the agent's local template in charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit_bytes_per_second: Option<u64>,
    /// Requested load-balancer group. Absent leaves the agent's local
    /// template in charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,
    /// Requested health probe. Absent leaves the agent's local template in
    /// charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check: Option<sdkwork_webserver_tunnel_core::HealthCheckConfig>,
}

/// Gateway → agent control error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMessage {
    /// Failure category.
    pub code: ErrorCode,
    /// Human-readable detail (redacted).
    pub message: String,
}

/// One control-plane message (PRD §19). Tagged JSON keeps the wire shape
/// self-describing and forward-compatible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMessage {
    /// Handshake opening.
    Hello(Hello),
    /// Credential presentation.
    Authenticate(Authenticate),
    /// Authentication outcome.
    AuthResult(AuthResult),
    /// Route registration request.
    RegisterRoute(RegisterRoute),
    /// Route registration outcome.
    RegisterRouteResult(RegisterRouteResult),
    /// Route removal request.
    UnregisterRoute(UnregisterRoute),
    /// Route removal outcome.
    UnregisterRouteResult(UnregisterRouteResult),
    /// Gateway-initiated route declaration (agent validates against its
    /// local templates before registering).
    DeclareRoute(DeclareRoute),
    /// Agent liveness ping.
    Heartbeat,
    /// Gateway liveness ack.
    HeartbeatAck,
    /// Agent → gateway token re-presentation, sent with a heartbeat once the
    /// gateway advertised `heartbeats` in its required scopes (FRP
    /// `auth.additionalScopes`).
    Reauthenticate(Reauthenticate),
    /// Agent → gateway health verdict for one route (FRP `healthCheck`
    /// parity): the agent probes its own target and reports the outcome, which
    /// is what lets the gateway steer a load-balancer group away from a
    /// replica that cannot serve.
    RouteHealth(RouteHealthReport),
    /// Control-plane failure.
    Error(ErrorMessage),
}

/// One route's health as its owning agent observed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteHealthReport {
    /// Route the verdict applies to.
    pub route_id: String,
    /// True while the agent's probe succeeds.
    pub healthy: bool,
    /// Consecutive failed probes; `0` on success.
    pub consecutive_failures: u32,
}

impl ControlMessage {
    /// Serializes to the JSON payload form (without the frame prefix).
    pub fn encode(&self) -> Result<Vec<u8>, TunnelError> {
        serde_json::to_vec(self).map_err(|error| {
            TunnelError::Protocol(format!("control message failed to encode: {error}"))
        })
    }

    /// Parses a JSON payload into a message, rejecting unknown message types
    /// into [`ControlMessage::Error`] with
    /// [`ErrorCode::UnsupportedProtocol`] so future message kinds cannot tear
    /// down an established session.
    pub fn decode(payload: &[u8]) -> Result<Self, TunnelError> {
        match serde_json::from_slice::<ControlMessage>(payload) {
            Ok(message) => Ok(message),
            Err(error) => {
                let detail = error.to_string();
                if detail.contains("unknown variant") {
                    return Ok(Self::Error(ErrorMessage {
                        code: ErrorCode::UnsupportedProtocol,
                        message: "unsupported control message type".to_owned(),
                    }));
                }
                Err(TunnelError::Protocol(format!(
                    "control message failed to decode: {detail}"
                )))
            }
        }
    }

    /// Encodes and frames the message for the wire.
    pub fn to_frame(&self) -> Result<bytes::Bytes, TunnelError> {
        frame::frame_bytes(&self.encode()?)
    }

    /// Parses one frame payload into a message.
    pub fn from_frame(payload: &[u8]) -> Result<Self, TunnelError> {
        Self::decode(payload)
    }

    /// The device id carried by `Hello`, when present.
    pub fn hello_device(&self) -> Option<(&str, DevicePlatform)> {
        match self {
            Self::Hello(hello) => Some((
                hello.device_id.as_str(),
                DevicePlatform::parse(&hello.platform),
            )),
            _ => None,
        }
    }

    /// Convenience constructor for control errors.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error(ErrorMessage {
            code,
            message: message.into(),
        })
    }
}

/// Converts an authenticated session identity into the wire string form.
pub fn session_to_wire(session_id: &SessionId) -> String {
    session_id.to_string()
}

/// Parses a session identity from the wire string form.
pub fn session_from_wire(raw: &str) -> Result<SessionId, TunnelError> {
    SessionId::parse(raw)
}

/// Parses a route identity from the wire string form.
pub fn route_from_wire(raw: &str) -> Result<RouteId, TunnelError> {
    RouteId::parse(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_tunnel_core::TunnelProtocolKind;

    fn round_trip(message: ControlMessage) -> ControlMessage {
        let payload = message.encode().expect("encode");
        ControlMessage::decode(&payload).expect("decode")
    }

    #[test]
    fn hello_round_trips() {
        let message = ControlMessage::Hello(Hello {
            protocol_versions: vec![ProtocolVersion::v1()],
            device_id: "dev_home-pc".to_owned(),
            device_name: "home-pc".to_owned(),
            platform: "windows".to_owned(),
            user: None,
        });
        assert_eq!(round_trip(message.clone()), message);
    }

    #[test]
    fn every_message_variant_round_trips() {
        let messages = vec![
            ControlMessage::Authenticate(Authenticate {
                token: "t0ken".to_owned(),
            }),
            ControlMessage::AuthResult(AuthResult {
                ok: true,
                session_id: Some("session_a".to_owned()),
                protocol: Some(ProtocolVersion::v1()),
                error: None,
                required_scopes: Some(vec!["newWorkConns".to_owned()]),
            }),
            ControlMessage::RegisterRoute(RegisterRoute {
                route_id: "route_web".to_owned(),
                name: "web".to_owned(),
                protocol: "http".to_owned(),
                domain: Some("demo.sdkwork.link".to_owned()),
                port: None,
                target: "127.0.0.1:3000".to_owned(),
                allow_public: false,
                allowed_ips: Some(vec!["203.0.113.0/24".to_owned()]),
                auth: Some("bearer_token".to_owned()),
                visitor_tokens: Some(vec!["visitor-t0ken".to_owned()]),
                proxy_protocol: Some("v1".to_owned()),
                locations: None,
                ..RegisterRoute::default()
            }),
            ControlMessage::RegisterRouteResult(RegisterRouteResult {
                route_id: "route_web".to_owned(),
                ok: true,
                public_url: Some("https://demo.sdkwork.link".to_owned()),
                error: None,
            }),
            ControlMessage::UnregisterRoute(UnregisterRoute {
                route_id: "route_web".to_owned(),
            }),
            ControlMessage::UnregisterRouteResult(UnregisterRouteResult {
                route_id: "route_web".to_owned(),
                ok: true,
            }),
            ControlMessage::DeclareRoute(DeclareRoute {
                route_id: "web".to_owned(),
                name: "web".to_owned(),
                protocol: "http".to_owned(),
                domain: Some("demo.sdkwork.link".to_owned()),
                port: None,
                allow_public: false,
                allowed_ips: Some(vec!["198.51.100.0/24".to_owned()]),
                auth: Some("bearer_token".to_owned()),
                visitor_tokens: Some(vec!["declared-t0ken".to_owned()]),
                proxy_protocol: Some("v2".to_owned()),
                locations: None,
                ..DeclareRoute::default()
            }),
            ControlMessage::Heartbeat,
            ControlMessage::HeartbeatAck,
            ControlMessage::Error(ErrorMessage {
                code: ErrorCode::ResourceLimit,
                message: "too many routes".to_owned(),
            }),
        ];
        for message in messages {
            assert_eq!(round_trip(message.clone()), message, "{message:?}");
        }
    }

    #[test]
    fn wire_shape_is_self_describing() {
        let message = ControlMessage::Heartbeat;
        let payload = message.encode().expect("encode");
        let text = String::from_utf8(payload).expect("utf8");
        assert!(text.contains(r#""type":"heartbeat""#), "{text}");
    }

    #[test]
    fn route_allow_list_round_trips_and_stays_absent_when_unset() {
        let with_list = ControlMessage::RegisterRoute(RegisterRoute {
            route_id: "route_ssh".to_owned(),
            name: "ssh".to_owned(),
            protocol: "tcp".to_owned(),
            domain: None,
            port: Some(7022),
            target: "127.0.0.1:22".to_owned(),
            allow_public: false,
            allowed_ips: Some(vec!["10.0.0.0/8".to_owned(), "203.0.113.7/32".to_owned()]),
            auth: Some("none".to_owned()),
            visitor_tokens: None,
            proxy_protocol: None,
            locations: None,
            ..RegisterRoute::default()
        });
        let text = String::from_utf8(with_list.encode().expect("encode")).expect("utf8");
        assert!(text.contains("10.0.0.0/8"), "{text}");
        assert_eq!(round_trip(with_list.clone()), with_list);

        let without_list = ControlMessage::RegisterRoute(RegisterRoute {
            route_id: "route_ssh".to_owned(),
            name: "ssh".to_owned(),
            protocol: "tcp".to_owned(),
            domain: None,
            port: Some(7022),
            target: "127.0.0.1:22".to_owned(),
            allow_public: false,
            allowed_ips: None,
            auth: None,
            visitor_tokens: None,
            proxy_protocol: None,
            locations: None,
            ..RegisterRoute::default()
        });
        let text = String::from_utf8(without_list.encode().expect("encode")).expect("utf8");
        assert!(!text.contains("allowedIps"), "{text}");
        assert!(!text.contains("visitorTokens"), "{text}");
        assert!(!text.contains("proxyProtocol"), "{text}");
        assert_eq!(round_trip(without_list.clone()), without_list);
    }

    #[test]
    fn header_rewriting_travels_with_the_registration_and_absent_stays_absent() {
        // The gateway is the hop that parses HTTP, so a route's rewriting
        // policy is only actionable if it arrives with the registration.
        let rewritten = ControlMessage::RegisterRoute(RegisterRoute {
            route_id: "route_web".to_owned(),
            name: "web".to_owned(),
            protocol: "http".to_owned(),
            domain: Some("demo.sdkwork.link".to_owned()),
            target: "127.0.0.1:3000".to_owned(),
            allow_public: true,
            headers: Some(sdkwork_webserver_tunnel_core::RouteHeaderRewrite {
                host: Some("backend.internal".to_owned()),
                request_set: vec![sdkwork_webserver_tunnel_core::HeaderPair::parse(
                    "X-From",
                    "edge",
                    sdkwork_webserver_tunnel_core::HeaderDirection::Request,
                )
                .expect("pair")],
                response_set: vec![sdkwork_webserver_tunnel_core::HeaderPair::parse(
                    "X-Served-By",
                    "sdkwork",
                    sdkwork_webserver_tunnel_core::HeaderDirection::Response,
                )
                .expect("pair")],
            }),
            ..RegisterRoute::default()
        });
        let text = String::from_utf8(rewritten.encode().expect("encode")).expect("utf8");
        assert!(text.contains(r#""host":"backend.internal""#), "{text}");
        assert!(text.contains(r#""requestSet""#), "{text}");
        assert_eq!(round_trip(rewritten.clone()), rewritten);

        // A route that rewrites nothing carries no field at all, and an older
        // agent's registration decodes to "rewrite nothing" rather than to a
        // policy the gateway would have to invent.
        let plain = ControlMessage::RegisterRoute(RegisterRoute {
            route_id: "route_plain".to_owned(),
            name: "plain".to_owned(),
            protocol: "http".to_owned(),
            domain: Some("plain.sdkwork.link".to_owned()),
            target: "127.0.0.1:3000".to_owned(),
            allow_public: true,
            ..RegisterRoute::default()
        });
        let text = String::from_utf8(plain.encode().expect("encode")).expect("utf8");
        assert!(!text.contains("headers"), "{text}");
        let legacy: RegisterRoute = serde_json::from_str(
            r#"{"routeId":"route_plain","name":"plain","protocol":"http",
                "domain":"plain.sdkwork.link","target":"127.0.0.1:3000","allowPublic":true}"#,
        )
        .expect("legacy registration decodes");
        assert_eq!(legacy.headers, None);
    }

    /// The scope advertisement and the token re-presentation (FRP
    /// `auth.additionalScopes`): a gateway that requires it must be able to say
    /// so, and an agent must be able to answer, without either side guessing.
    #[test]
    fn auth_scopes_are_advertised_and_re_presented_additively() {
        let advertised = ControlMessage::AuthResult(AuthResult {
            ok: true,
            session_id: Some("session_r".to_owned()),
            protocol: Some(ProtocolVersion::v1()),
            error: None,
            required_scopes: Some(vec!["heartbeats".to_owned()]),
        });
        let text = String::from_utf8(advertised.encode().expect("encode")).expect("utf8");
        assert!(
            text.contains(r#""requiredScopes":["heartbeats"]"#),
            "{text}"
        );
        assert_eq!(round_trip(advertised.clone()), advertised);

        let reauth = ControlMessage::Reauthenticate(Reauthenticate {
            token: "re-presented-token".to_owned(),
        });
        assert_eq!(round_trip(reauth.clone()), reauth);

        // A gateway that requires nothing omits the field, and an older
        // gateway's result (no field at all) decodes to "nothing required"
        // rather than to an invented scope.
        let plain = ControlMessage::AuthResult(AuthResult {
            ok: true,
            session_id: Some("session_p".to_owned()),
            protocol: Some(ProtocolVersion::v1()),
            error: None,
            required_scopes: None,
        });
        let text = String::from_utf8(plain.encode().expect("encode")).expect("utf8");
        assert!(!text.contains("requiredScopes"), "{text}");
        let legacy: AuthResult = serde_json::from_str(r#"{"ok":true,"sessionId":"session_l"}"#)
            .expect("legacy auth result decodes");
        assert_eq!(legacy.required_scopes, None);
    }

    #[test]
    fn visitor_auth_policy_round_trips_and_stays_absent_when_unset() {
        // The gateway rebuilds its route policy from these fields; a policy
        // that fails to travel means a bearer-protected route registers as
        // deny-everything. Absence must stay absence (an N-1 agent sends
        // neither field and keeps the old `auth = none` behaviour).
        let protected = ControlMessage::RegisterRoute(RegisterRoute {
            route_id: "route_web".to_owned(),
            name: "web".to_owned(),
            protocol: "http".to_owned(),
            domain: Some("demo.sdkwork.link".to_owned()),
            port: None,
            target: "127.0.0.1:3000".to_owned(),
            allow_public: false,
            allowed_ips: None,
            auth: Some("bearer_token".to_owned()),
            visitor_tokens: Some(vec!["t1".to_owned(), "t2".to_owned()]),
            proxy_protocol: Some("v1".to_owned()),
            locations: None,
            ..RegisterRoute::default()
        });
        let text = String::from_utf8(protected.encode().expect("encode")).expect("utf8");
        assert!(text.contains(r#""auth":"bearer_token""#), "{text}");
        assert!(text.contains(r#""visitorTokens":["t1","t2"]"#), "{text}");
        assert!(text.contains(r#""proxyProtocol":"v1""#), "{text}");
        assert_eq!(round_trip(protected.clone()), protected);

        // A message from an older agent: unknown-to-it fields absent.
        let legacy: RegisterRoute = serde_json::from_str(
            r#"{"routeId":"route_web","name":"web","protocol":"http","domain":"demo.sdkwork.link",
                "target":"127.0.0.1:3000","allowPublic":true}"#,
        )
        .expect("legacy registration decodes");
        assert_eq!(legacy.auth, None);
        assert_eq!(legacy.visitor_tokens, None);
        assert_eq!(legacy.proxy_protocol, None);
        assert_eq!(legacy.locations, None);

        // Locations travel the same way as the policy: a route that claims
        // only some paths of its host must be dispatched by that claim.
        let located = ControlMessage::RegisterRoute(RegisterRoute {
            route_id: "route_api".to_owned(),
            name: "api".to_owned(),
            protocol: "http".to_owned(),
            domain: Some("demo.sdkwork.link".to_owned()),
            locations: Some(vec!["/api".to_owned(), "/admin/".to_owned()]),
            ..RegisterRoute::default()
        });
        let text = String::from_utf8(located.encode().expect("encode")).expect("utf8");
        assert!(text.contains(r#""locations":["/api","/admin/"]"#), "{text}");
        assert_eq!(round_trip(located.clone()), located);
    }

    #[test]
    fn unknown_message_type_maps_to_unsupported_protocol_error() {
        let payload = br#"{"type":"time_travel","when":"yesterday"}"#;
        let decoded = ControlMessage::decode(payload).expect("graceful decode");
        match decoded {
            ControlMessage::Error(error) => {
                assert_eq!(error.code, ErrorCode::UnsupportedProtocol);
            }
            other => panic!("expected Error message, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_a_protocol_error() {
        assert!(ControlMessage::decode(b"{not json").is_err());
        assert!(ControlMessage::decode(b"{}").is_err());
    }

    #[test]
    fn protocol_version_accepts_only_stp1() {
        assert!(ProtocolVersion::v1().is_acceptable());
        assert!(!ProtocolVersion {
            name: "STP".to_owned(),
            version: 2,
        }
        .is_acceptable());
        assert!(!ProtocolVersion {
            name: "OTHER".to_owned(),
            version: 1,
        }
        .is_acceptable());
    }

    #[test]
    fn error_code_maps_domain_errors() {
        assert_eq!(
            ErrorCode::from_error(&TunnelError::AuthenticationFailed),
            ErrorCode::AuthenticationFailed
        );
        assert_eq!(
            ErrorCode::from_error(&TunnelError::RouteConflict("taken".to_owned())),
            ErrorCode::RouteConflict
        );
        let _ = TunnelProtocolKind::Http; // domain re-export reachable
    }

    #[test]
    fn session_identity_converts_through_wire_form() {
        let session = SessionId::parse("session_x1").expect("valid id");
        let wire = session_to_wire(&session);
        assert_eq!(session_from_wire(&wire).expect("valid"), session);
        assert!(session_from_wire("bogus").is_err());
    }

    #[test]
    fn a_known_type_with_a_missing_field_is_a_hard_protocol_error() {
        // Only *unknown variants* are allowed to degrade into an `Error`
        // message. A truncated message of a type we do understand is a real
        // protocol violation and must fail the read instead of being
        // silently reinterpreted.
        for payload in [
            br#"{"type":"hello"}"#.as_slice(),
            br#"{"type":"register_route","routeId":"route_web"}"#.as_slice(),
        ] {
            match ControlMessage::decode(payload) {
                Err(TunnelError::Protocol(_)) => {}
                other => panic!(
                    "expected a hard protocol error for {}: {other:?}",
                    String::from_utf8_lossy(payload)
                ),
            }
        }
    }

    #[test]
    fn unknown_fields_inside_a_known_message_are_tolerated() {
        // Forward compatibility: a newer agent may add fields to a message an
        // older gateway already understands. Dropping those fields must not
        // tear the session down.
        let payload = br#"{
            "type": "register_route",
            "routeId": "route_web",
            "name": "web",
            "protocol": "http",
            "domain": "demo.sdkwork.link",
            "target": "127.0.0.1:3000",
            "allowPublic": true,
            "futureSelector": {"v": 2}
        }"#;
        let decoded = ControlMessage::decode(payload).expect("forward-compatible decode");
        match decoded {
            ControlMessage::RegisterRoute(route) => {
                assert_eq!(route.route_id, "route_web");
                assert!(route.allow_public);
                assert_eq!(route.allowed_ips, None);
            }
            other => panic!("expected RegisterRoute, got {other:?}"),
        }
    }

    #[test]
    fn every_domain_error_maps_to_a_wire_category() {
        // The mapping is the agent-visible remediation contract: `Internal`
        // is the catch-all, everything else must stay specific.
        let cases: Vec<(TunnelError, ErrorCode)> = vec![
            (
                TunnelError::AuthenticationFailed,
                ErrorCode::AuthenticationFailed,
            ),
            (
                TunnelError::AuthorizationDenied,
                ErrorCode::AuthorizationDenied,
            ),
            (
                TunnelError::InvalidRoute("x".to_owned()),
                ErrorCode::InvalidRoute,
            ),
            (
                TunnelError::RouteConflict("x".to_owned()),
                ErrorCode::RouteConflict,
            ),
            (TunnelError::RouteNotFound, ErrorCode::RouteNotFound),
            (
                TunnelError::ResourceLimit("maxRoutes"),
                ErrorCode::ResourceLimit,
            ),
            (
                TunnelError::Protocol("x".to_owned()),
                ErrorCode::UnsupportedProtocol,
            ),
            (
                TunnelError::Validation {
                    field: sdkwork_webserver_tunnel_core::ValidationField::Config,
                    reason: "x".to_owned(),
                },
                ErrorCode::Internal,
            ),
            (
                TunnelError::ConnectionFailed("x".to_owned()),
                ErrorCode::Internal,
            ),
            (TunnelError::ConnectionClosed, ErrorCode::Internal),
            (TunnelError::DeviceNotFound, ErrorCode::Internal),
            (TunnelError::SessionNotFound, ErrorCode::Internal),
            (
                TunnelError::TargetUnreachable("x".to_owned()),
                ErrorCode::Internal,
            ),
            (TunnelError::Timeout("stream open"), ErrorCode::Internal),
        ];
        for (error, expected) in cases {
            assert_eq!(ErrorCode::from_error(&error), expected, "{error:?}");
        }
    }

    #[test]
    fn authenticate_token_is_present_only_on_the_authenticate_message() {
        // The token must travel exactly once, on `Authenticate`, and must not
        // leak into the outcome message the gateway sends back.
        let auth = ControlMessage::Authenticate(Authenticate {
            token: "super-secret-token".to_owned(),
        });
        let text = String::from_utf8(auth.encode().expect("encode")).expect("utf8");
        assert!(text.contains("super-secret-token"), "the wire carries it");

        let failure = ControlMessage::AuthResult(AuthResult {
            ok: false,
            session_id: None,
            protocol: None,
            error: Some("authentication failed".to_owned()),
            required_scopes: None,
        });
        let text = String::from_utf8(failure.encode().expect("encode")).expect("utf8");
        assert!(
            !text.contains("super-secret-token"),
            "the outcome must not echo the credential: {text}"
        );
    }
}
