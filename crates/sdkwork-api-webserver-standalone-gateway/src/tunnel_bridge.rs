//! Bridge between the webserver data plane and the tunnel capability.
//!
//! Owns the HTTP relay (hyper over the relayed tunnel stream, PRD §30),
//! the WebSocket upgrade pump (PRD §124), and the resolution of
//! [`TunnelConfig`] into concrete gateway options. HTTP semantics live
//! here, on the webserver side; the tunnel crate stays protocol-agnostic.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use hyper_util::rt::TokioIo;
use sdkwork_webserver_tunnel::gateway::{GatewayShared, RelayVisitor};
use sdkwork_webserver_tunnel::TunnelGatewayOptions;
use sdkwork_webserver_tunnel_core::TunnelConfig;

/// Hostname endpoints resolve on the async resolver with a deadline: a
/// blocking `getaddrinfo` inside the relay would stall one Tokio worker per
/// relayed request, and a saturated resolver would stall all of them.
const CLUSTER_ENDPOINT_RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);
/// The relayed connect to a cluster instance is bounded independently of the
/// outer request deadline so an unreachable instance fails fast.
const CLUSTER_ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Relay WebSocket pumps are raw byte bridges between two upgraded sockets;
/// neither socket is owned by hyper after the 101, so the listener's
/// connection-age limit cannot reclaim them. The pump therefore carries its
/// own finite lifetime (aligned with the default `maxConnectionAgeMs`) and
/// closes the relay when it expires instead of pinning two sockets forever.
const RELAY_PUMP_MAXIMUM_LIFETIME: Duration = Duration::from_secs(3_600);

fn cluster_failure(message: String) -> TunnelRelayError {
    TunnelRelayError::Failure(sdkwork_webserver_tunnel_core::TunnelError::ConnectionFailed(message))
}

/// Bridges one upgraded visitor socket to one upgraded upstream socket until
/// either side closes or the pump lifetime expires.
async fn pump_websocket(
    downstream: hyper::upgrade::OnUpgrade,
    upstream: hyper::upgrade::OnUpgrade,
) -> std::io::Result<()> {
    tokio::time::timeout(RELAY_PUMP_MAXIMUM_LIFETIME, async {
        let downstream = downstream
            .await
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let upstream = upstream
            .await
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let mut downstream = TokioIo::new(downstream);
        let mut upstream = TokioIo::new(upstream);
        tokio::io::copy_bidirectional(&mut downstream, &mut upstream)
            .await
            .map(|_| ())
    })
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "relay websocket pump lifetime exceeded",
        )
    })?
}

/// Builds the gateway options from the app config's `[tunnel]` section.
/// Returns `None` when the section is absent or disabled (PRD §77: a
/// disabled tunnel leaves the runtime unchanged).
pub(crate) fn build_gateway_options(
    config: &TunnelConfig,
    metrics: Arc<sdkwork_webserver_tunnel::TunnelMetrics>,
) -> Option<TunnelGatewayOptions> {
    if !config.enabled {
        return None;
    }
    let bind: std::net::SocketAddr = config.gateway_or_default().listen.parse().ok()?;
    let mut options = TunnelGatewayOptions::from_config(config, bind, Vec::new(), Vec::new());
    let (cert_pem, key_pem) =
        load_tls_material(&options.tls_cert_pem_env, &options.tls_key_pem_env)?;
    options.cert_pem = cert_pem;
    options.key_pem = key_pem;
    options.metrics = metrics;
    Some(options)
}

fn load_tls_material(
    cert_env: &Option<String>,
    key_env: &Option<String>,
) -> Option<(Vec<u8>, Vec<u8>)> {
    match (cert_env, key_env) {
        (Some(cert_name), Some(key_name)) => {
            let cert_path = std::env::var(cert_name).ok()?;
            let key_path = std::env::var(key_name).ok()?;
            let cert = std::fs::read(cert_path).ok()?;
            let key = std::fs::read(key_path).ok()?;
            Some((cert, key))
        }
        _ => {
            let sans = vec!["localhost".to_owned(), "127.0.0.1".to_owned()];
            let material =
                sdkwork_webserver_tunnel_transport::tls::generate_self_signed(&sans).ok()?;
            tracing::warn!(
                pin = %material.sha256,
                "tunnel gateway uses a SELF-SIGNED development certificate; agents should pin this fingerprint or supply TLS material via tlsCertPemEnv/tlsKeyPemEnv"
            );
            Some((
                material.cert_pem.into_bytes(),
                material.key_pem.into_bytes(),
            ))
        }
    }
}

/// Tunnel relay failure surfaced to the request handler.
#[derive(Debug)]
pub(crate) enum TunnelRelayError {
    /// No tunnel route matched; the caller falls through to the normal
    /// virtual-host/404 handling.
    NoRoute,
    /// The relay was denied (ACL/auth) or failed mid-relay; surface as an
    /// HTTP status instead of falling through.
    Failure(sdkwork_webserver_tunnel_core::TunnelError),
}

/// Relays one visitor HTTP request through the tunnel toward the agent's
/// local target (PRD §30 flow). WebSocket upgrades pump raw bytes in both
/// directions after the 101 (PRD §124).
#[allow(clippy::too_many_lines)]
pub(crate) async fn relay_tunnel_http(
    shared: &Arc<GatewayShared>,
    metrics: &Arc<sdkwork_webserver_tunnel::TunnelMetrics>,
    host: &str,
    visitor_ip: IpAddr,
    mut request: axum::http::Request<Body>,
) -> Result<axum::response::Response<Body>, TunnelRelayError> {
    let is_websocket = is_upgrade_request(&request);
    let bearer = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_owned);

    let stream = shared
        .connect_http_stream(
            host,
            RelayVisitor {
                ip: visitor_ip,
                bearer: bearer.as_deref(),
            },
        )
        .await
        .map_err(|error| match error {
            sdkwork_webserver_tunnel_core::TunnelError::RouteNotFound => TunnelRelayError::NoRoute,
            other => TunnelRelayError::Failure(other),
        })?;

    // The downstream upgrade must be captured before the request is split.
    let downstream_upgrade = if is_websocket {
        Some(hyper::upgrade::on(&mut request))
    } else {
        None
    };

    let (mut parts, body) = request.into_parts();
    strip_hop_by_hop_headers(&mut parts.headers, is_websocket);
    inject_forwarded_headers(&mut parts.headers, visitor_ip);
    let upstream_request = axum::http::Request::from_parts(parts, body);

    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|error| {
            TunnelRelayError::Failure(
                sdkwork_webserver_tunnel_core::TunnelError::ConnectionFailed(error.to_string()),
            )
        })?;
    if is_websocket {
        tokio::spawn(connection.with_upgrades());
    } else {
        tokio::spawn(connection);
    }

    let mut response = sender
        .send_request(upstream_request)
        .await
        .map_err(|error| {
            TunnelRelayError::Failure(
                sdkwork_webserver_tunnel_core::TunnelError::ConnectionFailed(error.to_string()),
            )
        })?;

    if is_websocket && response.status() == axum::http::StatusCode::SWITCHING_PROTOCOLS {
        let upstream_upgrade = hyper::upgrade::on(&mut response);
        if let Some(downstream) = downstream_upgrade {
            let metrics = metrics.clone();
            metrics.record_stream_open();
            tokio::spawn(async move {
                match pump_websocket(downstream, upstream_upgrade).await {
                    Ok(()) => {}
                    Err(error) => {
                        tracing::debug!(error = %error, "tunnel websocket pump ended");
                    }
                }
                metrics.record_stream_close();
            });
        }
        let (mut parts, empty_body) = response.into_parts();
        parts.headers.insert(
            axum::http::header::CONNECTION,
            axum::http::HeaderValue::from_static("upgrade"),
        );
        return Ok(axum::response::Response::from_parts(
            parts,
            Body::new(empty_body),
        ));
    }

    let (parts, response_body) = response.into_parts();
    Ok(axum::response::Response::from_parts(
        parts,
        Body::new(response_body),
    ))
}

fn is_upgrade_request(request: &axum::http::Request<Body>) -> bool {
    let connection_upgrades = request
        .headers()
        .get(axum::http::header::CONNECTION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    connection_upgrades && request.headers().get(axum::http::header::UPGRADE).is_some()
}

/// FRP-parity visitor identity propagation: appends the visitor address to
/// `X-Forwarded-For` and fills `X-Real-IP` when absent, so services behind
/// the agent observe the real client instead of the gateway hop.
fn inject_forwarded_headers(headers: &mut axum::http::HeaderMap, visitor_ip: IpAddr) {
    let visitor = visitor_ip.to_string();
    match headers.get_mut("x-forwarded-for") {
        Some(existing) => {
            if let Ok(value) = existing.to_str() {
                let appended = format!("{value}, {visitor}");
                if let Ok(value) = axum::http::HeaderValue::from_str(&appended) {
                    *existing = value;
                }
            }
        }
        None => {
            if let Ok(value) = axum::http::HeaderValue::from_str(&visitor) {
                headers.insert("x-forwarded-for", value);
            }
        }
    }
    if !headers.contains_key("x-real-ip") {
        if let Ok(value) = axum::http::HeaderValue::from_str(&visitor) {
            headers.insert("x-real-ip", value);
        }
    }
}

fn strip_hop_by_hop_headers(headers: &mut axum::http::HeaderMap, keep_upgrade: bool) {
    const KEEP_ALIVE: &str = "keep-alive";
    const PROXY_AUTHENTICATE: &str = "proxy-authenticate";
    const PROXY_AUTHORIZATION: &str = "proxy-authorization";
    // RFC 9110 §7.6.1: the Connection header's token list names additional
    // hop-by-hop members that must not be relayed. The main proxy path honors
    // it (`hop_by_hop_headers` in data_plane/proxy.rs); the relay used to
    // strip only the fixed set, so `Connection: X-Custom` let X-Custom cross
    // to the agent/upstream.
    let mut declared: Vec<axum::http::HeaderName> = Vec::new();
    for value in headers.get_all(axum::http::header::CONNECTION) {
        if let Ok(value) = value.to_str() {
            for token in value.split(',').map(str::trim) {
                if let Ok(name) = axum::http::HeaderName::from_bytes(token.as_bytes()) {
                    declared.push(name);
                }
            }
        }
    }
    for name in [
        axum::http::header::CONNECTION,
        axum::http::header::PROXY_AUTHORIZATION,
        axum::http::header::TE,
        axum::http::header::TRAILER,
        axum::http::header::TRANSFER_ENCODING,
        axum::http::header::UPGRADE,
    ] {
        if keep_upgrade
            && (name == axum::http::header::CONNECTION || name == axum::http::header::UPGRADE)
        {
            continue;
        }
        headers.remove(name);
    }
    for name in [KEEP_ALIVE, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION] {
        if let Ok(parsed) = axum::http::HeaderName::from_lowercase(name.as_bytes()) {
            headers.remove(parsed);
        }
    }
    for name in declared {
        if keep_upgrade && name == axum::http::header::UPGRADE {
            continue;
        }
        headers.remove(name);
    }
}

/// Marks a request this gateway already relayed to a sibling instance, so the
/// receiving instance knows it is the last hop: relay once, never again. Two
/// instances whose round-robin cursors point at each other — or an instance
/// whose overlay makes it pick itself — would otherwise bounce the same
/// request forever.
///
/// Trust model: one listener accepts both visitor and sibling traffic, so a
/// visitor *can* present this header. It carries the same trust as
/// `x-cluster-lb-strategy` (a documented, client-settable routing hint) and the
/// effect is confined to the request that presents it: that request is served
/// by the instance it reached instead of being balanced onward. It never grants
/// access to content the receiving instance would not already serve, and it
/// never escalates privilege. Hardening this into a non-forgeable signal needs
/// a dedicated east-west listener with its own trust declaration, which the
/// listener configuration model does not express today.
pub(crate) const CLUSTER_HOP_HEADER: &str = "x-served-by-cluster-hop";

/// Relays one request to a cluster instance over the internal network
/// (auto-routing east-west hop): opens a TCP stream to the picked
/// instance's bind endpoint and speaks HTTP/1.1 through it. WebSocket
/// upgrades pump raw bytes after the 101, exactly like the tunnel relay.
///
/// `visitor_ip` is the resolved client address of the original request; it
/// travels to the sibling in `X-Forwarded-For`/`X-Real-IP` so services behind
/// the hop observe the real client instead of this gateway.
#[allow(clippy::too_many_lines)]
pub(crate) async fn relay_cluster_http(
    endpoint: &str,
    visitor_ip: IpAddr,
    mut request: axum::http::Request<Body>,
) -> Result<axum::response::Response<Body>, TunnelRelayError> {
    let is_websocket = is_upgrade_request(&request);
    let downstream_upgrade = if is_websocket {
        Some(hyper::upgrade::on(&mut request))
    } else {
        None
    };

    // Cluster east-west hops are plain HTTP to the instance bind; strip
    // hop-by-hop headers, keep everything else verbatim.
    let (mut parts, body) = request.into_parts();
    strip_hop_by_hop_headers(&mut parts.headers, is_websocket);
    inject_forwarded_headers(&mut parts.headers, visitor_ip);
    parts
        .headers
        .insert(CLUSTER_HOP_HEADER, "1".parse().expect("valid header"));
    let upstream_request = axum::http::Request::from_parts(parts, body);

    // Endpoint form: `host:port` (bind address reported by the instance).
    let authority = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_owned();
    let socket_addr: std::net::SocketAddr = match authority.parse::<std::net::SocketAddr>() {
        Ok(addr) => addr,
        Err(_) => {
            let mut resolved = tokio::time::timeout(
                CLUSTER_ENDPOINT_RESOLVE_TIMEOUT,
                tokio::net::lookup_host(authority.as_str()),
            )
            .await
            .map_err(|_| {
                cluster_failure(format!("resolving cluster endpoint {authority} timed out"))
            })?
            .map_err(|error| {
                cluster_failure(format!("resolving cluster endpoint {authority}: {error}"))
            })?;
            resolved.next().ok_or_else(|| {
                cluster_failure(format!("unresolvable cluster endpoint {authority}"))
            })?
        }
    };
    let tcp = tokio::time::timeout(
        CLUSTER_ENDPOINT_CONNECT_TIMEOUT,
        tokio::net::TcpStream::connect(socket_addr),
    )
    .await
    .map_err(|_| {
        cluster_failure(format!(
            "connecting to cluster instance {socket_addr} timed out"
        ))
    })?
    .map_err(|error| cluster_failure(format!("cluster instance {socket_addr}: {error}")))?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(tcp))
        .await
        .map_err(|error| {
            TunnelRelayError::Failure(
                sdkwork_webserver_tunnel_core::TunnelError::ConnectionFailed(error.to_string()),
            )
        })?;
    if is_websocket {
        tokio::spawn(connection.with_upgrades());
    } else {
        tokio::spawn(connection);
    }
    let mut response = sender
        .send_request(upstream_request)
        .await
        .map_err(|error| {
            TunnelRelayError::Failure(
                sdkwork_webserver_tunnel_core::TunnelError::ConnectionFailed(error.to_string()),
            )
        })?;

    if is_websocket && response.status() == axum::http::StatusCode::SWITCHING_PROTOCOLS {
        let upstream_upgrade = hyper::upgrade::on(&mut response);
        if let Some(downstream) = downstream_upgrade {
            tokio::spawn(async move {
                if let Err(error) = pump_websocket(downstream, upstream_upgrade).await {
                    tracing::debug!(error = %error, "cluster websocket pump ended");
                }
            });
        }
        let (mut parts, empty_body) = response.into_parts();
        parts.headers.insert(
            axum::http::header::CONNECTION,
            axum::http::HeaderValue::from_static("upgrade"),
        );
        return Ok(axum::response::Response::from_parts(
            parts,
            Body::new(empty_body),
        ));
    }

    let (parts, response_body) = response.into_parts();
    Ok(axum::response::Response::from_parts(
        parts,
        Body::new(response_body),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn forwarded_headers_are_appended_and_filled() {
        let mut headers = axum::http::HeaderMap::new();
        let ip: IpAddr = "203.0.113.7".parse().expect("ip");
        inject_forwarded_headers(&mut headers, ip);
        assert_eq!(
            headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok()),
            Some("203.0.113.7")
        );
        assert_eq!(
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok()),
            Some("203.0.113.7")
        );

        // A second hop appends instead of overwriting; X-Real-IP stays at
        // the first observed client.
        let upstream: IpAddr = "198.51.100.4".parse().expect("ip");
        inject_forwarded_headers(&mut headers, upstream);
        assert_eq!(
            headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok()),
            Some("203.0.113.7, 198.51.100.4")
        );
        assert_eq!(
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok()),
            Some("203.0.113.7")
        );
    }
}
