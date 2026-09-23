//! Per-connection gateway control loop (PRD §19, §23, §25, §27, §28).
//!
//! Handshake sequencing: `Hello` → version negotiation → `Authenticate` →
//! session creation → `RegisterRoute`/`UnregisterRoute`/`Heartbeat` steady
//! state. Every step is timeout-bounded and every teardown path
//! unregisters the session's routes and TCP listener claims.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::BytesMut;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use sdkwork_webserver_tunnel_core::{
    parse_allowed_ips, Device, DeviceId, DevicePlatform, Result, RouteId, RouteMatcher,
    RoutePolicy, SessionId, TunnelError, TunnelProtocolKind, TunnelRoute, TunnelTarget,
};
use sdkwork_webserver_tunnel_protocol::{
    frame, AuthResult, Authenticate, ControlMessage, ErrorCode, Hello, ProtocolVersion,
    RegisterRoute, RegisterRouteResult, UnregisterRouteResult,
};
use sdkwork_webserver_tunnel_transport::{TunnelConnection, TunnelStream};

use super::GatewayShared;

/// Serves one agent connection until the control stream closes, the
/// connection drops, the gateway stops, or the session is expired by the
/// sweeper.
pub(crate) async fn run_connection(
    shared: Arc<GatewayShared>,
    connection: Arc<dyn TunnelConnection>,
    peer: SocketAddr,
    mut stop: watch::Receiver<bool>,
) {
    shared.metrics.record_connection_open();
    let mut registered_session: Option<SessionId> = None;
    let outcome = handshake_and_serve(
        &shared,
        connection.clone(),
        peer,
        &mut stop,
        &mut registered_session,
    )
    .await;
    if let Err(error) = &outcome {
        tracing::debug!(%peer, error = %error, "tunnel session ended");
        shared.metrics.record_error();
    }
    if let Some(session_id) = registered_session.take() {
        teardown_session(&shared, &session_id);
    }
    connection.close("gateway session ended");
    shared.metrics.record_connection_close();
}

async fn handshake_and_serve(
    shared: &Arc<GatewayShared>,
    connection: Arc<dyn TunnelConnection>,
    peer: SocketAddr,
    stop: &mut watch::Receiver<bool>,
    registered_session: &mut Option<SessionId>,
) -> Result<()> {
    let handshake_budget = shared.timeouts.handshake_duration();
    // First accepted stream is the long-lived control stream (PRD §21);
    // agents open exactly this one stream inbound.
    let mut control = match tokio::time::timeout(handshake_budget, connection.accept_stream()).await
    {
        Ok(Ok(control)) => control,
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err(TunnelError::Timeout("control stream open")),
    };

    // One scratch buffer per session: read_frame may slurp several frames
    // in one chunk, and leftovers must survive across read_message calls.
    let mut scratch = BytesMut::with_capacity(4096);
    let (device, negotiated) = tokio::time::timeout(
        handshake_budget,
        handshake(shared, &mut control, &mut scratch, peer),
    )
    .await
    .map_err(|_| TunnelError::Timeout("handshake"))??;

    let session_id = register_session(shared, connection.clone(), device.clone())?;
    *registered_session = Some(session_id.clone());

    write_message(
        &mut control,
        &ControlMessage::AuthResult(AuthResult {
            ok: true,
            session_id: Some(session_id.to_string()),
            protocol: Some(negotiated),
            error: None,
        }),
    )
    .await?;
    tracing::info!(
        device = %device.id,
        session = %session_id,
        %peer,
        "tunnel session connected"
    );

    // Any route declarations queued for this device while it was offline
    // are flushed now (PRD §35 create-route API).
    flush_pending_declarations(shared, &device.id, &mut control).await;

    let outcome = steady_state(shared, &session_id, &mut control, &mut scratch, stop).await;
    if outcome.is_ok() {
        tracing::info!(device = %device.id, session = %session_id, "tunnel session closed");
    }
    outcome
}

async fn handshake(
    shared: &Arc<GatewayShared>,
    control: &mut Box<dyn TunnelStream>,
    scratch: &mut BytesMut,
    peer: SocketAddr,
) -> Result<(Device, ProtocolVersion)> {
    let hello = read_message(shared, control, scratch).await?;
    let ControlMessage::Hello(Hello {
        protocol_versions,
        device_id,
        device_name,
        platform,
    }) = hello
    else {
        return Err(TunnelError::Protocol(
            "handshake must start with Hello".to_owned(),
        ));
    };
    let Some(negotiated) = protocol_versions
        .iter()
        .find(|version| version.is_acceptable())
        .cloned()
    else {
        write_message(
            control,
            &ControlMessage::error(
                ErrorCode::UnsupportedProtocol,
                "no common STP version; this gateway speaks STP/1",
            ),
        )
        .await?;
        return Err(TunnelError::Protocol(
            "protocol version negotiation failed".to_owned(),
        ));
    };

    let authenticate = read_message(shared, control, scratch).await?;
    let ControlMessage::Authenticate(Authenticate { token }) = authenticate else {
        return Err(TunnelError::Protocol(
            "expected Authenticate after Hello".to_owned(),
        ));
    };
    let device = authenticate_peer(
        shared,
        control,
        peer,
        &device_id,
        &device_name,
        &platform,
        &token,
    )
    .await?;
    Ok((device, negotiated))
}

async fn authenticate_peer(
    shared: &Arc<GatewayShared>,
    control: &mut Box<dyn TunnelStream>,
    peer: SocketAddr,
    device_id: &str,
    device_name: &str,
    platform: &str,
    token: &str,
) -> Result<Device> {
    async fn reject(
        shared: &GatewayShared,
        peer: SocketAddr,
        control: &mut Box<dyn TunnelStream>,
        detail: &'static str,
    ) -> TunnelError {
        shared.metrics.record_auth_failure();
        let _ = write_message(
            control,
            &ControlMessage::AuthResult(AuthResult {
                ok: false,
                session_id: None,
                protocol: None,
                error: Some(detail.to_owned()),
            }),
        )
        .await;
        // Bounded grace so the driver transmits the rejection before the
        // teardown closes the connection (which would discard the buffered
        // reply and leave the agent guessing).
        tokio::time::sleep(Duration::from_millis(50)).await;
        tracing::info!(%peer, detail, "tunnel authentication rejected");
        TunnelError::AuthenticationFailed
    }
    if !shared.authenticator.is_configured() {
        tracing::warn!(%peer, "tunnel gateway has no agent tokens configured; refusing all agents");
        return Err(reject(
            shared,
            peer,
            control,
            "gateway has no agent tokens configured",
        )
        .await);
    }
    if !shared.rate_limiter.record_failure(peer.ip()) {
        tracing::warn!(%peer, "tunnel authentication rate limit reached");
        return Err(reject(shared, peer, control, "authentication rate limit reached").await);
    }
    let Ok(id) = DeviceId::parse(device_id) else {
        return Err(reject(shared, peer, control, "invalid device id").await);
    };
    if shared.authenticator.authenticate(&id, token).is_err() {
        tracing::warn!(device = %id, %peer, "tunnel authentication failed");
        return Err(reject(shared, peer, control, "invalid credentials").await);
    }
    let device = Device::new(id, device_name.to_owned(), DevicePlatform::parse(platform))?;
    if shared.sessions.session_of_device(&device.id).is_none()
        && shared
            .sessions
            .device_budget_exhausted(shared.limits.max_devices)
    {
        tracing::warn!(device = %device.id, %peer, "tunnel device limit reached");
        let _ = write_message(
            control,
            &ControlMessage::Error(sdkwork_webserver_tunnel_protocol::ErrorMessage {
                code: ErrorCode::ResourceLimit,
                message: "device limit reached".to_owned(),
            }),
        )
        .await;
        return Err(TunnelError::ResourceLimit("maxDevices"));
    }
    Ok(device)
}

fn register_session(
    shared: &Arc<GatewayShared>,
    connection: Arc<dyn TunnelConnection>,
    device: Device,
) -> Result<SessionId> {
    if shared.sessions.session_of_device(&device.id).is_none()
        && shared.sessions.session_count()
            >= usize::try_from(shared.limits.max_sessions).unwrap_or(usize::MAX)
    {
        return Err(TunnelError::ResourceLimit("maxSessions"));
    }
    let session_id = generate_session_id();
    shared
        .sessions
        .insert(session_id.clone(), device, connection, &shared.metrics);
    Ok(session_id)
}

async fn steady_state(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    control: &mut Box<dyn TunnelStream>,
    scratch: &mut BytesMut,
    stop: &mut watch::Receiver<bool>,
) -> Result<()> {
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => {
                return Err(TunnelError::ConnectionClosed);
            }
            // A dead transport surfaces as a control-stream read failure
            // (QUIC idle timeout closes the stream); the heartbeat sweeper
            // covers live-but-silent peers.
            message = read_message(shared, control, scratch) => {
                let message = message?;
                match message {
                    ControlMessage::RegisterRoute(request) => {
                        let reply = handle_register(shared, session_id, &request).await;
                        write_message(control, &reply).await?;
                    }
                    ControlMessage::UnregisterRoute(request) => {
                        let reply = handle_unregister(shared, session_id, &request);
                        write_message(control, &reply).await?;
                    }
                    ControlMessage::Heartbeat => {
                        shared.sessions.touch(session_id)?;
                        write_message(control, &ControlMessage::HeartbeatAck).await?;
                    }
                    ControlMessage::Error(error) => {
                        tracing::warn!(
                            session = %session_id,
                            code = ?error.code,
                            "agent reported a control error: {}", error.message
                        );
                    }
                    ControlMessage::Hello(_)
                    | ControlMessage::Authenticate(_)
                    | ControlMessage::AuthResult(_)
                    | ControlMessage::RegisterRouteResult(_)
                    | ControlMessage::UnregisterRouteResult(_)
                    | ControlMessage::HeartbeatAck => {
                        return Err(TunnelError::Protocol(
                            "unexpected control message in steady state".to_owned(),
                        ));
                    }
                    ControlMessage::DeclareRoute(_) => {
                        return Err(TunnelError::Protocol(
                            "agents may not send DeclareRoute".to_owned(),
                        ));
                    }
                }
            }
        }
    }
}

async fn handle_register(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    request: &RegisterRoute,
) -> ControlMessage {
    let route = match validate_registration(shared, session_id, request) {
        Ok(route) => route,
        Err(error) => {
            return ControlMessage::RegisterRouteResult(RegisterRouteResult {
                route_id: request.route_id.clone(),
                ok: false,
                public_url: None,
                error: Some(error.to_string()),
            });
        }
    };
    let route_id = route.id.clone();
    let route_protocol = route.protocol;
    let port = route.matcher.as_port();
    let registered = match shared.registry.register(route) {
        Ok(registered) => registered,
        Err(error) => {
            return ControlMessage::RegisterRouteResult(RegisterRouteResult {
                route_id: route_id.to_string(),
                ok: false,
                public_url: None,
                error: Some(error.to_string()),
            });
        }
    };
    shared.metrics.record_route_change(1);
    // Listener lifecycle per protocol: TCP routes bind a TcpListener, UDP
    // routes bind a UdpSocket — both inline so a port conflict is reported
    // back to the agent synchronously.
    match (route_protocol, port) {
        (TunnelProtocolKind::Tcp, Some(port)) => {
            match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
                Ok(listener) => {
                    shared
                        .tcp
                        .lock()
                        .expect("tcp listener set lock is never held across awaits")
                        .serve(shared, port, route_id.clone(), listener);
                }
                Err(error) => {
                    shared.registry.unregister(&route_id);
                    shared.metrics.record_route_change(-1);
                    return ControlMessage::RegisterRouteResult(RegisterRouteResult {
                        route_id: route_id.to_string(),
                        ok: false,
                        public_url: None,
                        error: Some(format!("gateway port {port} bind failed: {error}")),
                    });
                }
            }
        }
        (TunnelProtocolKind::Udp, Some(port)) => {
            match tokio::net::UdpSocket::bind(("0.0.0.0", port)).await {
                Ok(listener) => {
                    shared
                        .udp
                        .lock()
                        .expect("udp listener set lock is never held across awaits")
                        .serve(shared, port, route_id.clone(), listener);
                }
                Err(error) => {
                    shared.registry.unregister(&route_id);
                    shared.metrics.record_route_change(-1);
                    return ControlMessage::RegisterRouteResult(RegisterRouteResult {
                        route_id: route_id.to_string(),
                        ok: false,
                        public_url: None,
                        error: Some(format!("gateway udp port {port} bind failed: {error}")),
                    });
                }
            }
        }
        _ => {}
    }
    tracing::info!(
        session = %session_id,
        route = %registered.route.id,
        matcher = %registered.route.matcher.as_domain().unwrap_or(""),
        "tunnel route registered"
    );
    ControlMessage::RegisterRouteResult(RegisterRouteResult {
        route_id: route_id.to_string(),
        ok: true,
        public_url: sdkwork_webserver_tunnel_core::public_url(&registered.route.matcher),
        error: None,
    })
}

fn handle_unregister(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    request: &sdkwork_webserver_tunnel_protocol::UnregisterRoute,
) -> ControlMessage {
    let reply = |ok: bool| {
        ControlMessage::UnregisterRouteResult(UnregisterRouteResult {
            route_id: request.route_id.clone(),
            ok,
        })
    };
    let Ok(route_id) = sdkwork_webserver_tunnel_protocol::route_from_wire(&request.route_id) else {
        return reply(false);
    };
    let Some(registered) = shared.registry.get(&route_id) else {
        return reply(false);
    };
    if crate::security::require_route_owner(registered.session(), session_id, &route_id).is_err() {
        return reply(false);
    }
    let removed = shared.registry.unregister(&route_id);
    let Some(removed) = removed else {
        return reply(false);
    };
    if let Some(port) = removed.route.matcher.as_port() {
        let datagram =
            removed.route.protocol == sdkwork_webserver_tunnel_core::TunnelProtocolKind::Udp;
        if datagram {
            shared
                .udp
                .lock()
                .expect("udp listener set lock is never held across awaits")
                .release(port, &route_id);
        } else {
            shared
                .tcp
                .lock()
                .expect("tcp listener set lock is never held across awaits")
                .release(port, &route_id);
        }
    }
    shared.metrics.record_route_change(-1);
    tracing::info!(route = %removed.route.id, "tunnel route unregistered");
    reply(true)
}

/// Sends queued gateway-side route declarations for a freshly connected
/// device. Declarations the agent does not recognize are rejected by the
/// agent and simply expire.
async fn flush_pending_declarations(
    shared: &Arc<GatewayShared>,
    device_id: &DeviceId,
    control: &mut Box<dyn TunnelStream>,
) {
    let pending = {
        let mut declarations = shared.pending_declarations.lock().await;
        declarations.remove(device_id).unwrap_or_default()
    };
    for declaration in pending {
        let message =
            ControlMessage::DeclareRoute(sdkwork_webserver_tunnel_protocol::DeclareRoute {
                route_id: declaration.name.clone(),
                name: declaration.name.clone(),
                protocol: match declaration.protocol {
                    TunnelProtocolKind::Http => "http".to_owned(),
                    TunnelProtocolKind::Tcp => "tcp".to_owned(),
                    TunnelProtocolKind::Udp => "udp".to_owned(),
                },
                domain: declaration.domain.clone(),
                port: declaration.port,
                allow_public: declaration.policy_or_default().allow_public,
                allowed_ips: {
                    let policy = declaration.policy_or_default();
                    (!policy.allowed_ips.is_empty())
                        .then(|| policy.allowed_ips.iter().map(ToString::to_string).collect())
                },
            });
        if write_message(control, &message).await.is_err() {
            return;
        }
    }
}

async fn read_message(
    shared: &Arc<GatewayShared>,
    stream: &mut Box<dyn TunnelStream>,
    scratch: &mut BytesMut,
) -> Result<ControlMessage> {
    let payload = frame::read_frame(stream, scratch).await?;
    if payload.len() as u32 > shared.limits.max_control_message_bytes {
        return Err(TunnelError::Protocol(format!(
            "control message of {} bytes exceeds maxControlMessageBytes {}",
            payload.len(),
            shared.limits.max_control_message_bytes
        )));
    }
    ControlMessage::from_frame(&payload)
}

async fn write_message(stream: &mut Box<dyn TunnelStream>, message: &ControlMessage) -> Result<()> {
    let frame = message.to_frame()?;
    stream
        .write_all(&frame)
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    stream
        .flush()
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    Ok(())
}

fn validate_registration(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    request: &RegisterRoute,
) -> Result<TunnelRoute> {
    let route_id = RouteId::parse(&request.route_id)?;
    let protocol = match request.protocol.as_str() {
        "http" => TunnelProtocolKind::Http,
        "tcp" => TunnelProtocolKind::Tcp,
        "udp" => TunnelProtocolKind::Udp,
        other => {
            return Err(TunnelError::InvalidRoute(format!(
                "unsupported route protocol `{other}`"
            )))
        }
    };
    let matcher = match (protocol, request.domain.as_deref(), request.port) {
        (TunnelProtocolKind::Http, Some(domain), _) => RouteMatcher::domain(domain)?,
        (TunnelProtocolKind::Tcp, _, Some(port)) | (TunnelProtocolKind::Udp, _, Some(port)) => {
            RouteMatcher::Port(port)
        }
        _ => {
            return Err(TunnelError::InvalidRoute(
                "route matcher does not match its protocol".to_owned(),
            ))
        }
    };
    if let Some(domain) = matcher.as_domain() {
        validate_domain_suffixes(shared, domain)?;
    }
    let target = TunnelTarget::parse_for_protocol(protocol, &request.target)?;
    let allowed_ips = request
        .allowed_ips
        .as_deref()
        .map(parse_allowed_ips)
        .transpose()?
        .unwrap_or_default();
    let policy = RoutePolicy {
        allow_public: request.allow_public,
        allowed_ips,
        ..RoutePolicy::private()
    };
    let route = TunnelRoute::new(
        route_id,
        request.name.clone(),
        protocol,
        matcher,
        target,
        policy,
    )?;
    if shared.registry.count() >= usize::try_from(shared.limits.max_routes).unwrap_or(usize::MAX) {
        return Err(TunnelError::ResourceLimit("maxRoutes"));
    }
    Ok(route.with_session(session_id.clone(), current_time()))
}

fn validate_domain_suffixes(shared: &Arc<GatewayShared>, domain: &str) -> Result<()> {
    if shared.domain_suffixes.is_empty() {
        return Ok(());
    }
    if shared.domain_suffixes.iter().any(|suffix| {
        let suffix = suffix.trim_start_matches('.');
        domain == suffix || domain.ends_with(&format!(".{suffix}"))
    }) {
        return Ok(());
    }
    Err(TunnelError::InvalidRoute(format!(
        "domain {domain} is outside the gateway's allowed domain suffixes"
    )))
}

fn current_time() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn generate_session_id() -> SessionId {
    loop {
        let bytes: [u8; 8] = rand::random();
        let candidate = format!(
            "session_{}",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if let Ok(id) = SessionId::parse(candidate) {
            return id;
        }
    }
}

/// Tears down a session: removes its entry, unregisters its routes, and
/// releases its TCP listener claims. Idempotent: the heartbeat sweeper may
/// have expired the session already.
pub(crate) fn teardown_session(shared: &Arc<GatewayShared>, session_id: &SessionId) {
    if shared
        .sessions
        .remove(session_id, &shared.metrics)
        .is_none()
    {
        return;
    }
    let removed_routes = shared.registry.remove_session(session_id);
    {
        let mut listeners = shared
            .tcp
            .lock()
            .expect("tcp listener set lock is never held across awaits");
        for route_id in &removed_routes {
            listeners.release_route(route_id);
        }
    }
    {
        let mut udp = shared
            .udp
            .lock()
            .expect("udp listener set lock is never held across awaits");
        for route_id in &removed_routes {
            udp.release_route(route_id);
        }
    }
    shared
        .metrics
        .record_route_change(-(removed_routes.len() as i64));
    tracing::info!(
        session = %session_id,
        routes = removed_routes.len(),
        "tunnel session torn down"
    );
}
