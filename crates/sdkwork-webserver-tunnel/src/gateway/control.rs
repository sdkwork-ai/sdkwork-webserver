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
    parse_allowed_ips, AuthPolicy, Device, DeviceId, DevicePlatform, ProxyProtocolVersion, Result,
    RouteId, RouteMatcher, RoutePolicy, SessionId, TunnelError, TunnelProtocolKind, TunnelRoute,
    TunnelTarget,
};
use sdkwork_webserver_tunnel_protocol::{
    frame, AuthResult, Authenticate, ControlMessage, ErrorCode, ErrorMessage, Hello,
    ProtocolVersion, RegisterRoute, RegisterRouteResult, UnregisterRouteResult,
};
use sdkwork_webserver_tunnel_transport::{TunnelConnection, TunnelStream};

use super::GatewayShared;

/// Write deadline for control-plane replies. A flow-control-stalled agent
/// (not reading) would otherwise park the whole `steady_state` loop on one
/// `write_message` — no reads, no route updates, no heartbeats — until the
/// QUIC idle timeout (minutes) tore the session down. The agent bounds the
/// mirrored operation at 10s; the gateway uses the same budget and ends the
/// session on expiry, which the agent treats as any other transport loss.
const CONTROL_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Grace period after a terminal control-plane refusal.
///
/// Returning from the control loop tears the connection down, which discards a
/// reply the driver has not transmitted yet and leaves the agent guessing why.
/// The handshake rejection path has always waited this long; the
/// re-authentication refusals need the same, for the same reason.
const REFUSAL_GRACE: Duration = Duration::from_millis(50);

/// True when this gateway requires a token re-presentation on every heartbeat
/// (FRP `auth.additionalScopes = ["HeartBeats"]`).
fn requires_heartbeat_reauth(shared: &Arc<GatewayShared>) -> bool {
    sdkwork_webserver_tunnel_core::AuthScope::Heartbeats.is_required_in(&shared.auth_scopes)
}

/// The user a session belongs to, resolved through its device.
///
/// The device is the authoritative record because the gateway wrote the
/// token-resolved user into it at handshake time; nothing on the registration
/// path can change it afterwards.
fn session_user(shared: &Arc<GatewayShared>, session_id: &SessionId) -> Option<String> {
    let device_id = shared.sessions.device_of_session(session_id)?;
    shared
        .devices
        .get(&device_id)
        .and_then(|record| record.device.user.clone())
}

/// Enforces one user's concurrent-machine ceiling at session admission.
///
/// The machine being admitted is excluded from the count: a reconnect would
/// otherwise be refused by its own previous session while that session is still
/// winding down, which is exactly when a rolling restart happens.
fn check_user_device_quota(
    shared: &Arc<GatewayShared>,
    user: Option<&str>,
    device_id: &DeviceId,
) -> Result<()> {
    let Some(user) = user else {
        return Ok(());
    };
    let Some(config) = shared.user(user) else {
        return Ok(());
    };
    let Some(max_devices) = config.max_devices else {
        return Ok(());
    };
    // Liveness comes from the session table, which is the same source the
    // machine listings and the heartbeat sweeper use: a machine whose session
    // has ended must not keep occupying its user's slots.
    let online = shared
        .devices
        .list()
        .iter()
        .filter(|record| record.device.user.as_deref() == Some(user))
        .filter(|record| &record.device.id != device_id)
        .filter(|record| {
            shared
                .sessions
                .session_of_device(&record.device.id)
                .is_some()
        })
        .count();
    if online >= max_devices as usize {
        tracing::warn!(user, max_devices, "tunnel user device ceiling reached");
        return Err(TunnelError::ResourceLimit("maxDevices for this user"));
    }
    Ok(())
}

/// Enforces one user's ceilings before a route is registered.
///
/// A user with no entry in the gateway's user table has no per-user quota and
/// falls back to the global limits, which is what an unconfigured deployment
/// has always had.
fn check_user_quota(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    request: &RegisterRoute,
) -> Result<()> {
    let Some(user) = session_user(shared, session_id) else {
        return Ok(());
    };
    let Some(config) = shared.user(&user) else {
        return Ok(());
    };
    let routes = shared.registry.list();
    let owned: Vec<_> = routes
        .iter()
        .filter(|route| route.user.as_deref() == Some(user.as_str()))
        .collect();
    // A re-registration of the same route is a replacement, not an addition.
    let replacing = owned
        .iter()
        .any(|route| route.id.to_string() == request.route_id);
    if let Some(max_routes) = config.max_routes {
        let live = owned.len() - usize::from(replacing);
        if live >= max_routes as usize {
            return Err(TunnelError::ResourceLimit("maxRoutes for this user"));
        }
    }
    if let Some(max_ports) = config.max_ports {
        if request.port.is_some() {
            let live_ports = owned
                .iter()
                .filter(|route| route.matcher.as_port().is_some())
                .filter(|route| route.id.to_string() != request.route_id)
                .count();
            if live_ports >= max_ports as usize {
                return Err(TunnelError::ResourceLimit(
                    "maxPorts for this user: every public port is taken",
                ));
            }
        }
    }
    Ok(())
}

/// One bounded control-plane write; see [`CONTROL_WRITE_TIMEOUT`].
async fn write_message_bounded(
    control: &mut Box<dyn TunnelStream>,
    message: &sdkwork_webserver_tunnel_protocol::ControlMessage,
) -> Result<()> {
    let write = write_message(control, message);
    tokio::time::timeout(CONTROL_WRITE_TIMEOUT, write)
        .await
        .map_err(|_| {
            TunnelError::ConnectionFailed(
                "control stream write timed out (flow-control stalled)".to_owned(),
            )
        })?
}

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
        // The inventory remembers the machine after its session is gone.
        if let Some(device) = shared.session_device(&session_id) {
            shared.devices.observe_disconnect(&device, current_time());
        }
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
    shared
        .devices
        .observe_connect(&device, peer, current_time());
    *registered_session = Some(session_id.clone());

    write_message_bounded(
        &mut control,
        &ControlMessage::AuthResult(AuthResult {
            ok: true,
            session_id: Some(session_id.to_string()),
            protocol: Some(negotiated),
            error: None,
            // Advertised so the agent knows exactly where to re-present its
            // token: a scope the operator enabled is enforced here, and the
            // two sides cannot drift into "hardening enabled on one side only".
            required_scopes: (!shared.auth_scopes.is_empty()).then(|| {
                shared
                    .auth_scopes
                    .iter()
                    .map(|scope| scope.wire_label().to_owned())
                    .collect()
            }),
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

    // The same session may also carry visitor streams (FRP `stcp`/`sudp`):
    // agents never open streams toward the gateway, so every accepted stream
    // here is a visitor request the header identifies.
    let visitor_task = {
        let shared = shared.clone();
        let connection = connection.clone();
        let visitor_stop = stop.clone();
        tokio::spawn(async move {
            super::visitor::visitor_accept_loop(shared, connection, peer, visitor_stop).await
        })
    };

    let outcome = steady_state(shared, &session_id, &mut control, &mut scratch, stop).await;
    visitor_task.abort();
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
        user: claimed_user,
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
        write_message_bounded(
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
        // The user the machine claims; `authenticate_peer` compares it with the
        // user the presented token actually speaks for.
        claimed_user.as_deref(),
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
    claimed_user: Option<&str>,
) -> Result<Device> {
    async fn reject(
        shared: &GatewayShared,
        peer: SocketAddr,
        control: &mut Box<dyn TunnelStream>,
        detail: &'static str,
    ) -> TunnelError {
        shared.metrics.record_auth_failure();
        // Every rejection is a failed authentication attempt: charge the
        // failure budget here so successful handshakes (a fleet reconnecting
        // behind one NAT gateway) never consume it, while repeated bad
        // credentials still lock the peer out.
        shared.rate_limiter.record_failure(peer.ip());
        let _ = write_message_bounded(
            control,
            &ControlMessage::AuthResult(AuthResult {
                ok: false,
                session_id: None,
                protocol: None,
                error: Some(detail.to_owned()),
                // A rejected handshake advertises no scopes: there is no session
                // to re-authenticate.
                required_scopes: None,
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
    if !shared.rate_limiter.check(peer.ip()) {
        tracing::warn!(%peer, "tunnel authentication rate limit reached");
        return Err(reject(shared, peer, control, "authentication rate limit reached").await);
    }
    let Ok(id) = DeviceId::parse(device_id) else {
        return Err(reject(shared, peer, control, "invalid device id").await);
    };
    if shared
        .authenticator
        .authenticate(&id, token, claimed_user.as_deref())
        .is_err()
    {
        tracing::warn!(device = %id, %peer, "tunnel authentication failed");
        return Err(reject(shared, peer, control, "invalid credentials").await);
    }
    // The user is decided here, by the credential, not by the claim: the
    // claimed name was already checked against the token's own user, so the
    // resolved value is the token's and the claim only had to agree with it.
    let resolved_user = shared.authenticator.user_of(token);
    check_user_device_quota(shared, resolved_user.as_deref(), &id)?;
    let device = Device::new(id, device_name.to_owned(), DevicePlatform::parse(platform))?
        .with_user(resolved_user)?;
    // The operator switch and the inventory ceiling are decided together,
    // before anything is recorded: a refused machine must not appear in the
    // inventory, or "disable" would look like it worked while the machine
    // keeps showing up as managed.
    match shared.devices.admit(&device.id) {
        super::devices::DeviceAdmission::Admitted => {}
        super::devices::DeviceAdmission::Disabled => {
            tracing::warn!(device = %device.id, %peer, "tunnel machine is switched off");
            return Err(reject(shared, peer, control, "this machine is disabled").await);
        }
        super::devices::DeviceAdmission::InventoryFull => {
            tracing::warn!(device = %device.id, %peer, "tunnel device inventory is full");
            let _ = write_message_bounded(
                control,
                &ControlMessage::Error(sdkwork_webserver_tunnel_protocol::ErrorMessage {
                    code: ErrorCode::ResourceLimit,
                    message: "device inventory is full".to_owned(),
                }),
            )
            .await;
            return Err(TunnelError::ResourceLimit("maxDevices"));
        }
    }
    if shared.sessions.session_of_device(&device.id).is_none()
        && shared
            .sessions
            .device_budget_exhausted(shared.limits.max_devices)
    {
        tracing::warn!(device = %device.id, %peer, "tunnel device limit reached");
        let _ = write_message_bounded(
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
    // Set by a successful `Reauthenticate` and cleared by the heartbeat it
    // authorizes (FRP `auth.additionalScopes = ["HeartBeats"]`): one
    // re-presentation per heartbeat, so a session cannot be kept alive
    // indefinitely on a credential that was presented once at handshake time.
    let mut reauth_seen = false;
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
                        write_message_bounded(control, &reply).await?;
                    }
                    ControlMessage::UnregisterRoute(request) => {
                        let reply = handle_unregister(shared, session_id, &request);
                        write_message_bounded(control, &reply).await?;
                    }
                    ControlMessage::Heartbeat => {
                        // FRP `auth.additionalScopes = ["HeartBeats"]`: when the
                        // gateway requires it, a heartbeat is only accepted
                        // after a token re-presentation. Ordering is what makes
                        // this enforceable on a message that carries no token:
                        // the agent re-authenticates, then pings, so a session
                        // whose credential is gone (rotated, revoked, hijacked)
                        // stops being renewed instead of living until the
                        // transport drops.
                        if requires_heartbeat_reauth(shared) && !reauth_seen {
                            shared.metrics.record_auth_failure();
                            tracing::warn!(
                                session = %session_id,
                                "heartbeat without the required token re-presentation; closing session"
                            );
                            write_message_bounded(
                                control,
                                &ControlMessage::Error(ErrorMessage {
                                    code: ErrorCode::AuthenticationFailed,
                                    message: "this gateway requires the agent token on every \
                                              heartbeat (authScopes = [\"heartbeats\"])"
                                        .to_owned(),
                                }),
                            )
                            .await
                            .ok();
                            tokio::time::sleep(REFUSAL_GRACE).await;
                            return Err(TunnelError::AuthenticationFailed);
                        }
                        reauth_seen = false;
                        shared.sessions.touch(session_id)?;
                        // The machine inventory follows liveness: an operator
                        // reading the console sees the same "last seen" the
                        // heartbeat sweeper will act on.
                        if let Some(device) = shared.session_device(session_id) {
                            shared.devices.observe_activity(&device, current_time());
                        }
                        write_message_bounded(control, &ControlMessage::HeartbeatAck).await?;
                    }
                    ControlMessage::RouteHealth(report) => {
                        handle_route_health(shared, session_id, &report);
                    }
                    ControlMessage::Reauthenticate(reauth) => {
                        // The token is compared in constant time and never
                        // logged or echoed; a mismatch is indistinguishable
                        // from an absent one on the wire.
                        if shared.authenticator.verify(&reauth.token) {
                            reauth_seen = true;
                        } else {
                            shared.metrics.record_auth_failure();
                            tracing::warn!(
                                session = %session_id,
                                "token re-presentation rejected; closing session"
                            );
                            write_message_bounded(
                                control,
                                &ControlMessage::Error(ErrorMessage {
                                    code: ErrorCode::AuthenticationFailed,
                                    message: "token re-presentation rejected".to_owned(),
                                }),
                            )
                            .await
                            .ok();
                            tokio::time::sleep(REFUSAL_GRACE).await;
                            return Err(TunnelError::AuthenticationFailed);
                        }
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

/// Applies one agent health verdict to the route it names.
///
/// Ownership is checked like every other control-plane operation: an agent may
/// only report on its own routes, so one device cannot mark another device's
/// replica down.
fn handle_route_health(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    report: &sdkwork_webserver_tunnel_protocol::RouteHealthReport,
) {
    let Ok(route_id) = sdkwork_webserver_tunnel_protocol::route_from_wire(&report.route_id) else {
        tracing::debug!(route = %report.route_id, "health report carries an invalid route id");
        return;
    };
    let Some(registered) = shared.registry.get(&route_id) else {
        tracing::debug!(route = %route_id, "health report for an unknown route");
        return;
    };
    if crate::security::require_route_owner(registered.session(), session_id, &route_id).is_err() {
        tracing::warn!(
            route = %route_id,
            session = %session_id,
            "health report from a session that does not own the route; ignored"
        );
        return;
    }
    let changed = registered
        .health
        .apply_probe(report.healthy, report.consecutive_failures);
    if !report.healthy {
        shared.metrics.record_health_failure();
    }
    if changed {
        tracing::info!(
            route = %route_id,
            healthy = report.healthy,
            consecutive_failures = report.consecutive_failures,
            "route health changed"
        );
    }
}

async fn handle_register(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    request: &RegisterRoute,
) -> ControlMessage {
    // Per-user quotas are checked before the registry sees anything: a refusal
    // must not leave a route registered, and the counts are read from the
    // registry itself so there is no second index to keep in step.
    if let Err(error) = check_user_quota(shared, session_id, request) {
        return ControlMessage::RegisterRouteResult(RegisterRouteResult {
            route_id: request.route_id.clone(),
            ok: false,
            public_url: None,
            error: Some(error.to_string()),
        });
    }
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
            // A group replica must not rebind the port the first member owns:
            // the listener is per port, and each accepted connection is
            // dispatched to a group member (see `TcpListenerSet::serve`).
            if shared
                .tcp
                .lock()
                .expect("tcp listener set lock is never held across awaits")
                .is_served(port)
            {
                return ControlMessage::RegisterRouteResult(RegisterRouteResult {
                    route_id: route_id.to_string(),
                    ok: true,
                    public_url: None,
                    error: None,
                });
            }
            match tokio::net::TcpListener::bind((shared.proxy_bind_addr, port)).await {
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
            match tokio::net::UdpSocket::bind((shared.proxy_bind_addr, port)).await {
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
                protocol: declaration.protocol.wire_label().to_owned(),
                domain: declaration.domain.clone(),
                port: declaration.port,
                allow_public: declaration.policy_or_default().allow_public,
                allowed_ips: {
                    let policy = declaration.policy_or_default();
                    (!policy.allowed_ips.is_empty())
                        .then(|| policy.allowed_ips.iter().map(ToString::to_string).collect())
                },
                auth: Some(declaration.policy_or_default().auth.wire_label().to_owned()),
                visitor_tokens: {
                    let policy = declaration.policy_or_default();
                    (!policy.visitor_tokens.is_empty()).then(|| policy.visitor_tokens.clone())
                },
                proxy_protocol: declaration
                    .proxy_protocol
                    .map(|version| version.label().to_owned()),
                locations: (!declaration.locations.is_empty())
                    .then(|| declaration.locations.clone()),
                bandwidth_limit_bytes_per_second: declaration.bandwidth_limit_bytes_per_second,
                load_balancer_group: declaration.load_balancer_group.clone(),
                health_check: declaration.health_check.clone(),
            });
        if write_message_bounded(control, &message).await.is_err() {
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
    // The route id must sit in the namespace the *token* put this session in.
    // Without this check a machine could register under another team's id —
    // taking over its slot and, with `maxRoutes`, spending its budget.
    if let Some(user) = session_user(shared, session_id) {
        let expected = sdkwork_webserver_tunnel_core::derived_route_id(Some(&user), &request.name)?;
        if expected != route_id {
            return Err(TunnelError::InvalidRoute(format!(
                "route id `{route_id}` is outside user `{user}`'s namespace (expected `{expected}`)"
            )));
        }
    }
    let protocol = TunnelProtocolKind::parse(&request.protocol).ok_or_else(|| {
        TunnelError::InvalidRoute(format!("unsupported route protocol `{}`", request.protocol))
    })?;
    let matcher = match (protocol, request.domain.as_deref(), request.port) {
        (TunnelProtocolKind::Http, Some(domain), _) => RouteMatcher::domain(domain)?,
        (TunnelProtocolKind::Tcp, _, Some(port)) | (TunnelProtocolKind::Udp, _, Some(port)) => {
            RouteMatcher::Port(port)
        }
        // A secret route has no public surface: its name is what a visitor
        // asks for, and the gateway must not bind a host or port it never
        // promised (FRP `stcp`/`sudp` parity).
        (TunnelProtocolKind::Stcp, _, _) | (TunnelProtocolKind::Sudp, _, _) => {
            RouteMatcher::secret(&request.name)?
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
    if let Some(port) = matcher.as_port() {
        // Ports are the gateway-side registration policy, mirroring the
        // domain-suffix policy: an authenticated agent must not pre-empt the
        // edge's own listeners or well-known services.
        let (min, max) = (shared.route_port_min, shared.route_port_max);
        if port < min || port > max {
            return Err(TunnelError::InvalidRoute(format!(
                "route port {port} is outside the allowed {min}-{max} range"
            )));
        }
    }
    let target = TunnelTarget::parse_for_protocol(protocol, &request.target)?;
    let allowed_ips = request
        .allowed_ips
        .as_deref()
        .map(parse_allowed_ips)
        .transpose()?
        .unwrap_or_default();
    // Visitor authentication travels with the registration: the gateway has
    // no other source for the agent's template policy, and dropping it made a
    // bearer-protected HTTP route register as deny-everything. An absent
    // label means `none`, so a pre-existing agent keeps working unchanged.
    let auth = match request.auth.as_deref() {
        Some(label) => AuthPolicy::from_wire_label(label)?,
        None => AuthPolicy::None,
    };
    let policy = RoutePolicy {
        allow_public: request.allow_public,
        allowed_ips,
        auth,
        visitor_tokens: request.visitor_tokens.clone().unwrap_or_default(),
    };
    let route = TunnelRoute::new(
        route_id,
        request.name.clone(),
        protocol,
        matcher,
        target,
        policy,
    )?
    .with_proxy_protocol(match request.proxy_protocol.as_deref() {
        Some(label) => Some(ProxyProtocolVersion::parse(label).ok_or_else(|| {
            TunnelError::InvalidRoute(format!(
                "unknown PROXY protocol version `{label}` (expected v1|v2)"
            ))
        })?),
        None => None,
    })?
    .with_locations(request.locations.clone().unwrap_or_default())?
    .with_bandwidth_limit(request.bandwidth_limit_bytes_per_second)?
    .with_load_balancing(
        request.load_balancer_group.clone(),
        request.health_check.clone(),
    )?
    // The shared secret is what authorizes a visitor on a secret route: a
    // registration without one would create a route nobody can reach, or
    // worse, one reachable by a visitor that guesses the name.
    .with_secret_key(request.secret_key.clone())?
    // Header rewriting is validated here rather than trusted: the registration
    // arrives from a peer, and a policy that only fails at relay time would
    // turn one bad registration into failing live traffic. `with_headers`
    // rejects framing/hop-by-hop names, control characters, and any rewrite on
    // a plane that carries no headers.
    .with_headers(request.headers.clone().unwrap_or_default())?
    // Annotations are bounded here rather than trusted: the map arrives from a
    // peer, and an unbounded one would be free per-route storage for any
    // authenticated agent.
    .with_annotations(request.annotations.clone().unwrap_or_default())?;
    if shared.registry.count() >= usize::try_from(shared.limits.max_routes).unwrap_or(usize::MAX) {
        return Err(TunnelError::ResourceLimit("maxRoutes"));
    }
    // The route inherits the session's user, resolved from the token at
    // handshake time. Attaching it here (rather than trusting the
    // registration) is what makes per-user listings and quotas unspoofable,
    // and it is the only place a route's user is ever written.
    Ok(route
        .with_user(session_user(shared, session_id))
        .with_session(session_id.clone(), current_time()))
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
