//! Single-port multiplexed ingress (FRP `tcpmux` parity, expressed as a public
//! ingress).
//!
//! FRP's `tcpmux` exists to save *ports*: instead of one public listener per
//! proxy, clients reach many services through one socket. This gateway already
//! multiplexes every route of an agent over a single QUIC session, so the
//! useful half of the feature is the **ingress**: one public port that speaks
//! HTTP `CONNECT` and reaches every registered route.
//!
//! ```text
//! CONNECT ssh.example.com:22 HTTP/1.1
//! Proxy-Authorization: Bearer <visitor token>   (only for token-protected routes)
//! ```
//!
//! The target is resolved against the domain registry first (so an HTTP route
//! is reachable through the multiplexer) and the port registry second (so a
//! `tcp` route is reachable without its own public port). Everything else —
//! admission policy, load balancing, health steering, bandwidth ceilings — is
//! the same code the dedicated listeners use: a second ingress must not be a
//! second implementation of the policy.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use sdkwork_webserver_tunnel_core::{AuthPolicy, TunnelError};

use super::registry::RouteSelection;
use super::{dispatch, GatewayShared};

/// Longest request head this ingress will read before giving up.
///
/// A CONNECT request head is a few hundred bytes; anything larger is either a
/// mistake or an attempt to make the gateway allocate.
const MAX_HEAD_BYTES: usize = 8 * 1024;

/// How long a client has to send its request head.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);

/// One parsed `CONNECT` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRequest {
    /// Authority from the request line: `host` or `host:port`.
    pub authority: String,
    /// Optional bearer token from `Proxy-Authorization`.
    pub bearer: Option<String>,
}

impl ConnectRequest {
    /// The host part of the authority (no port, lowercased).
    pub fn host(&self) -> String {
        let authority = self.authority.trim();
        match authority.rsplit_once(':') {
            // A trailing `:port` is the port; an IPv6 literal in brackets keeps
            // its own colons, so only a suffix that parses as a port counts.
            Some((host, port)) if port.parse::<u16>().is_ok() => host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_ascii_lowercase(),
            _ => authority
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_ascii_lowercase(),
        }
    }

    /// The port part of the authority, when it names one.
    pub fn port(&self) -> Option<u16> {
        let authority = self.authority.trim();
        let (_, port) = authority.rsplit_once(':')?;
        port.parse::<u16>().ok()
    }
}

/// Parses an HTTP `CONNECT` request head.
///
/// Only `CONNECT` is accepted: this port exists to multiplex tunneled routes,
/// and answering an ordinary `GET` here would make it look like a web server
/// with a strange routing table.
pub fn parse_connect_request(head: &str) -> Result<ConnectRequest, ConnectError> {
    let mut lines = head.split("\r\n").flat_map(|line| line.split('\n'));
    let request_line = lines.next().unwrap_or_default().trim();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    if method.is_empty() {
        return Err(ConnectError::BadRequest("empty request line"));
    }
    if !method.eq_ignore_ascii_case("CONNECT") {
        return Err(ConnectError::MethodNotAllowed);
    }
    // A request line that stops after the method is the common operator
    // mistake (`CONNECT host:port` with no version), and it reads as a missing
    // target rather than as a bad version.
    let (Some(authority), Some(version)) = (parts.next(), parts.next()) else {
        return Err(ConnectError::BadRequest("CONNECT needs a target authority"));
    };
    if authority.is_empty() {
        return Err(ConnectError::BadRequest("CONNECT needs a target authority"));
    }
    if !version.to_ascii_uppercase().starts_with("HTTP/1.") {
        return Err(ConnectError::BadRequest("only HTTP/1.x is supported"));
    }
    let mut bearer = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("proxy-authorization") {
            let value = value.trim();
            if let Some(token) = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
            {
                bearer = Some(token.trim().to_owned());
            } else {
                return Err(ConnectError::BadRequest(
                    "Proxy-Authorization must use the Bearer scheme",
                ));
            }
        }
    }
    Ok(ConnectRequest {
        authority: authority.to_owned(),
        bearer,
    })
}

/// Why a `CONNECT` request could not be served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// Malformed request line or headers.
    BadRequest(&'static str),
    /// A method other than `CONNECT`.
    MethodNotAllowed,
    /// No route answers the requested authority.
    NotFound,
    /// The route's policy turned the visitor away.
    Denied {
        /// True when the route wants a bearer token: the client gets a 407.
        bearer_required: bool,
    },
    /// Every replica of the group is unhealthy.
    Unavailable,
    /// The provider could not accept the stream.
    RelayFailed(String),
}

impl ConnectError {
    /// One-line reason, for logs (never sent verbatim to a client).
    pub fn reason(&self) -> String {
        match self {
            Self::BadRequest(detail) => (*detail).to_owned(),
            Self::MethodNotAllowed => "method is not CONNECT".to_owned(),
            Self::NotFound => "no route answers the authority".to_owned(),
            Self::Denied { bearer_required } => {
                format!("policy denied the visitor (token required: {bearer_required})")
            }
            Self::Unavailable => "no healthy replica".to_owned(),
            Self::RelayFailed(error) => error.clone(),
        }
    }

    /// The status line this refusal answers with.
    pub fn status_line(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "HTTP/1.1 400 Bad Request",
            Self::MethodNotAllowed => "HTTP/1.1 405 Method Not Allowed",
            Self::NotFound => "HTTP/1.1 404 Not Found",
            Self::Denied {
                bearer_required: false,
            } => "HTTP/1.1 403 Forbidden",
            Self::Denied {
                bearer_required: true,
            } => "HTTP/1.1 407 Proxy Authentication Required",
            Self::Unavailable => "HTTP/1.1 503 Service Unavailable",
            Self::RelayFailed(_) => "HTTP/1.1 502 Bad Gateway",
        }
    }

    /// The human-readable detail sent in the body.
    pub fn detail(&self) -> String {
        match self {
            Self::BadRequest(detail) => (*detail).to_owned(),
            Self::MethodNotAllowed => {
                "this port multiplexes tunneled routes and only accepts CONNECT".to_owned()
            }
            Self::NotFound => "no tunneled route answers that authority".to_owned(),
            Self::Denied { .. } => "the route's admission policy refused this visitor".to_owned(),
            Self::Unavailable => "no healthy replica is available".to_owned(),
            Self::RelayFailed(error) => error.clone(),
        }
    }
}

/// Accepts multiplexed visitor connections until the gateway stops.
pub(crate) async fn accept_loop(
    shared: Arc<GatewayShared>,
    listener: tokio::net::TcpListener,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        let accepted = tokio::select! {
            biased;
            _ = stop.changed() => return,
            accepted = listener.accept() => accepted,
        };
        let (visitor, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::debug!(%error, "tcpmux accept failed");
                continue;
            }
        };
        // Admission mirrors the other visitor planes: one permit per live
        // visitor connection, so the concurrent task count is bounded by
        // configuration rather than by the arrival rate.
        let permit = match shared.connection_admission.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                tracing::warn!(%peer, "tcpmux connection refused: admission limit reached");
                drop(visitor);
                continue;
            }
        };
        let shared = shared.clone();
        tokio::spawn(async move {
            let _permit = permit;
            serve_visitor(shared, visitor, peer).await;
        });
    }
}

/// Reads one request head, authorizes it, and bridges the connection.
async fn serve_visitor(
    shared: Arc<GatewayShared>,
    mut visitor: tokio::net::TcpStream,
    peer: SocketAddr,
) {
    let _ = visitor.set_nodelay(true);
    let request = match read_connect_request(&mut visitor).await {
        Ok(request) => request,
        Err(error) => {
            shared.metrics.record_relay_denied();
            tracing::info!(%peer, error = %error.reason(), "tcpmux request refused");
            // A malformed request may not even be HTTP; answering is best
            // effort and the connection closes either way.
            let _ = write_refusal(&mut visitor, &error).await;
            return;
        }
    };

    match dispatch_connect(&shared, &request, peer).await {
        Ok(mut provider) => {
            if visitor
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .is_err()
            {
                return;
            }
            let _ = visitor.flush().await;
            match tokio::io::copy_bidirectional(&mut visitor, &mut provider).await {
                Ok((inbound, outbound)) => {
                    shared.metrics.add_bytes_in(inbound);
                    shared.metrics.add_bytes_out(outbound);
                }
                Err(error) => {
                    tracing::debug!(%peer, %error, "tcpmux relay ended");
                }
            }
        }
        Err(error) => {
            shared.metrics.record_relay_denied();
            tracing::info!(
                %peer,
                authority = %request.authority,
                error = %error.reason(),
                "tcpmux request refused"
            );
            let _ = write_refusal(&mut visitor, &error).await;
        }
    }
}

/// Resolves the authority and opens the provider stream.
async fn dispatch_connect(
    shared: &Arc<GatewayShared>,
    request: &ConnectRequest,
    peer: SocketAddr,
) -> Result<dispatch::RelayedStream, ConnectError> {
    let host = request.host();
    if host.is_empty() {
        return Err(ConnectError::BadRequest("CONNECT authority has no host"));
    }

    // The domain registry first: an HTTP route is reachable through the
    // multiplexer exactly as it is through the TLS listener.
    let by_domain = shared.registry.select_domain_path(&host, "/");
    let registered = match by_domain {
        RouteSelection::Selected(route) => route,
        RouteSelection::NoHealthyReplica => {
            // A host that is claimed but unhealthy must not fall through to
            // the port registry: the operator needs the real reason.
            return Err(ConnectError::Unavailable);
        }
        RouteSelection::NoRoute => match request.port() {
            // Then the port registry: `CONNECT anything:7022` reaches the tcp
            // route that owns 7022, which is what makes one port enough.
            Some(port) => match shared.registry.select_port(port) {
                RouteSelection::Selected(route) => route,
                RouteSelection::NoHealthyReplica => return Err(ConnectError::Unavailable),
                RouteSelection::NoRoute => return Err(ConnectError::NotFound),
            },
            None => return Err(ConnectError::NotFound),
        },
    };

    // The route's own policy decides, through the same evaluator the other
    // visitor planes use, with the CONNECT equivalent of an `Authorization`
    // header: `Proxy-Authorization`.
    let bearer = request.bearer.as_deref();
    if let Err(error) = dispatch::admit(shared, &registered.route.policy, peer.ip(), bearer) {
        return Err(match error {
            TunnelError::AuthorizationDenied => ConnectError::Denied {
                bearer_required: registered.route.policy.auth == AuthPolicy::BearerToken,
            },
            other => ConnectError::RelayFailed(other.to_string()),
        });
    }

    let owner = registered
        .session()
        .cloned()
        .ok_or(ConnectError::NotFound)?;
    dispatch::open_data_stream(
        shared,
        &owner,
        &registered.route.id,
        Some(sdkwork_webserver_tunnel_protocol::VisitorAddress::new(
            peer.ip(),
            peer.port(),
        )),
        registered.limiter.clone(),
    )
    .await
    .map_err(|error| match error {
        TunnelError::NoHealthyReplica => ConnectError::Unavailable,
        TunnelError::RouteNotFound => ConnectError::NotFound,
        other => ConnectError::RelayFailed(other.to_string()),
    })
}

/// Reads the request head, bounded in both size and time.
async fn read_connect_request(
    visitor: &mut tokio::net::TcpStream,
) -> Result<ConnectRequest, ConnectError> {
    let mut head = Vec::with_capacity(256);
    let mut buffer = [0_u8; 512];
    let read = async {
        loop {
            let read = visitor.read(&mut buffer).await.map_err(|error| {
                ConnectError::BadRequest(if error.kind() == std::io::ErrorKind::InvalidData {
                    "request head is not valid UTF-8"
                } else {
                    "request head could not be read"
                })
            })?;
            if read == 0 {
                break;
            }
            head.extend_from_slice(&buffer[..read]);
            if head.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            if head.len() >= MAX_HEAD_BYTES {
                return Err(ConnectError::BadRequest("request head is too large"));
            }
        }
        Ok(())
    };
    match tokio::time::timeout(HEAD_TIMEOUT, read).await {
        Ok(result) => result?,
        Err(_) => return Err(ConnectError::BadRequest("request head timed out")),
    }
    if head.is_empty() {
        return Err(ConnectError::BadRequest(
            "connection closed before a request",
        ));
    }
    let text = String::from_utf8_lossy(&head).to_string();
    parse_connect_request(&text)
}

/// Answers a refused request with a status line and a one-line body.
async fn write_refusal(
    visitor: &mut tokio::net::TcpStream,
    error: &ConnectError,
) -> std::io::Result<()> {
    let detail = error.detail();
    let body = format!("{detail}\n");
    let challenge = match error {
        ConnectError::Denied {
            bearer_required: true,
        } => "Proxy-Authenticate: Bearer realm=\"sdkwork-tunnel\"\r\n",
        _ => "",
    };
    let response = format!(
        "{}\r\n{challenge}Content-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        error.status_line(),
        body.len()
    );
    visitor.write_all(response.as_bytes()).await?;
    visitor.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connect_request_is_parsed_with_its_authority_and_token() {
        let request = parse_connect_request(
            "CONNECT ssh.example.com:22 HTTP/1.1\r\nHost: ssh.example.com:22\r\n\
             Proxy-Authorization: Bearer visitor-token\r\n\r\n",
        )
        .expect("parses");
        assert_eq!(request.authority, "ssh.example.com:22");
        assert_eq!(request.host(), "ssh.example.com");
        assert_eq!(request.port(), Some(22));
        assert_eq!(request.bearer.as_deref(), Some("visitor-token"));

        // A bare host with no port is as valid as an authority with one, and
        // the host is lowercased for lookup.
        let bare =
            parse_connect_request("CONNECT Web.Example.COM HTTP/1.1\r\n\r\n").expect("parses");
        assert_eq!(bare.host(), "web.example.com");
        assert_eq!(bare.port(), None);
        assert!(bare.bearer.is_none());

        // An IPv6 literal keeps its own colons; only a numeric suffix is a
        // port, so the host is not truncated at the first colon.
        let v6 =
            parse_connect_request("CONNECT [2001:db8::1]:7022 HTTP/1.1\r\n\r\n").expect("parses");
        assert_eq!(v6.host(), "2001:db8::1");
        assert_eq!(v6.port(), Some(7022));
    }

    #[test]
    fn only_connect_is_accepted_on_the_multiplexer_port() {
        assert_eq!(
            parse_connect_request("GET / HTTP/1.1\r\nHost: a.test\r\n\r\n"),
            Err(ConnectError::MethodNotAllowed)
        );
        assert_eq!(
            parse_connect_request("CONNECT HTTP/1.1\r\n\r\n"),
            Err(ConnectError::BadRequest("CONNECT needs a target authority"))
        );
        assert_eq!(
            parse_connect_request("CONNECT a.test:22 HTTP/2.0\r\n\r\n"),
            Err(ConnectError::BadRequest("only HTTP/1.x is supported"))
        );
        assert_eq!(
            parse_connect_request(
                "CONNECT a.test:22 HTTP/1.1\r\nProxy-Authorization: Basic x\r\n\r\n"
            ),
            Err(ConnectError::BadRequest(
                "Proxy-Authorization must use the Bearer scheme"
            ))
        );
        // An empty head is a client that connected and said nothing.
        assert!(matches!(
            parse_connect_request("\r\n\r\n"),
            Err(ConnectError::BadRequest(_))
        ));
    }

    #[test]
    fn refusals_carry_the_right_status_and_challenge() {
        assert_eq!(
            ConnectError::NotFound.status_line(),
            "HTTP/1.1 404 Not Found"
        );
        assert_eq!(
            ConnectError::Denied {
                bearer_required: false
            }
            .status_line(),
            "HTTP/1.1 403 Forbidden"
        );
        // 407 is the only status that can ask a CONNECT client for a token.
        assert_eq!(
            ConnectError::Denied {
                bearer_required: true
            }
            .status_line(),
            "HTTP/1.1 407 Proxy Authentication Required"
        );
        assert_eq!(
            ConnectError::Unavailable.status_line(),
            "HTTP/1.1 503 Service Unavailable"
        );
        assert!(ConnectError::MethodNotAllowed
            .detail()
            .contains("only accepts CONNECT"));
    }
}
