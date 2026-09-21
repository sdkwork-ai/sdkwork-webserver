//! Agent: the inside-network connector (PRD §6.2, §27, §61).
//!
//! Dials the gateway, authenticates, registers its configured routes,
//! heartbeats, reconnects with jittered backoff, and relays data streams to
//! local targets. The agent never trusts gateway-supplied targets: every
//! data stream resolves through this agent's own route table (PRD §45).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio::io::{copy_bidirectional, AsyncWriteExt};
use tokio::sync::{mpsc, watch};

use sdkwork_webserver_tunnel_core::{
    Device, Result, RouteId, TunnelConfig, TunnelError, TunnelRouteTemplate, TunnelTarget,
};
use sdkwork_webserver_tunnel_protocol::{
    frame, AuthResult, Authenticate, ControlMessage, DataStreamHeader, DeclareRoute, ErrorCode,
    Hello, ProtocolVersion, RegisterRoute, RegisterRouteResult,
};
use sdkwork_webserver_tunnel_transport::{
    tls, QuicClientTransport, RemoteEndpoint, TransportOptions, TunnelClientTransport,
    TunnelConnection, TunnelStream,
};

use crate::metrics::TunnelMetrics;

/// Lifecycle events an agent reports to its supervisor (CLI or webserver
/// integration).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum AgentEvent {
    /// Dialing the gateway.
    Connecting {
        /// Gateway endpoint label.
        endpoint: String,
    },
    /// Authenticated with a session.
    Connected {
        /// Session identity issued by the gateway.
        session_id: String,
    },
    /// Registration pass finished; the tunnel is usable.
    Ready {
        /// Live routes with their public URLs.
        routes: Vec<RouteReadiness>,
        /// Routes the gateway rejected.
        rejected: Vec<RouteRejection>,
    },
    /// The transport dropped.
    Disconnected {
        /// Human-readable cause.
        reason: String,
    },
    /// Waiting before the next reconnect attempt (PRD §27 backoff).
    ReconnectingIn {
        /// Delay before the next attempt, milliseconds.
        after_ms: u64,
    },
    /// The agent stopped permanently (shutdown or reconnect disabled).
    Stopped,
}

/// One route's readiness as reported in [`AgentEvent::Ready`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteReadiness {
    /// Route identity.
    pub route_id: String,
    /// Operator-facing name.
    pub name: String,
    /// Public URL (HTTP routes).
    pub public_url: Option<String>,
}

/// One route's rejection detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRejection {
    /// Route identity.
    pub route_id: String,
    /// Gateway-provided detail.
    pub error: String,
}

/// Agent construction options.
#[derive(Clone)]
pub struct AgentRuntimeOptions {
    /// Gateway endpoint.
    pub endpoint: RemoteEndpoint,
    /// TLS verification policy for the gateway certificate.
    pub tls: tls::ClientTlsOptions,
    /// Device identity reported to the gateway.
    pub device: Device,
    /// Bearer token.
    pub token: String,
    /// Routes to expose from this machine.
    pub routes: Vec<TunnelRouteTemplate>,
    /// Heartbeat and reconnect settings.
    pub network: sdkwork_webserver_tunnel_core::TunnelNetworkConfig,
    /// Timeouts.
    pub timeouts: sdkwork_webserver_tunnel_core::TunnelTimeoutConfig,
    /// Shared metrics.
    pub metrics: Arc<TunnelMetrics>,
}

impl AgentRuntimeOptions {
    /// Resolves agent options from a validated [`TunnelConfig`] plus the
    /// runtime-resolved device and token (PRD §40 env resolution).
    pub fn from_config(
        config: &TunnelConfig,
        endpoint: RemoteEndpoint,
        tls: tls::ClientTlsOptions,
        device: Device,
        token: String,
        metrics: Arc<TunnelMetrics>,
    ) -> Self {
        Self {
            endpoint,
            tls,
            device,
            token,
            routes: config.routes_or_empty().to_vec(),
            network: config.network_or_default(),
            timeouts: config.timeout_or_default(),
            metrics,
        }
    }
}

/// A running agent loop handle (PRD §61: Identity + Transport + Session +
/// Registration + Heartbeat + Reconnect + LocalProxy).
pub struct AgentRuntime {
    stop_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
    events_rx: mpsc::Receiver<AgentEvent>,
}

impl AgentRuntime {
    /// Starts the agent connect/reconnect loop.
    pub fn spawn(options: AgentRuntimeOptions) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        let (events_tx, events_rx) = mpsc::channel(64);
        let task = tokio::spawn(run_loop(options, stop_rx, events_tx));
        Self {
            stop_tx,
            task,
            events_rx,
        }
    }

    /// Receives the next lifecycle event.
    pub async fn next_event(&mut self) -> Option<AgentEvent> {
        self.events_rx.recv().await
    }

    /// Signals the agent to stop and waits for the loop to exit
    /// (PRD §105: unregister → close streams → close session).
    pub async fn shutdown(self) {
        let _ = self.stop_tx.send(true);
        let _ = self.task.await;
    }
}

async fn run_loop(
    options: AgentRuntimeOptions,
    mut stop: watch::Receiver<bool>,
    events: mpsc::Sender<AgentEvent>,
) {
    let routes = Arc::new(build_route_table(&options.routes));
    let mut backoff = Backoff::new(
        Duration::from_secs(options.network.initial_backoff_secs.max(1)),
        Duration::from_secs(options.network.max_backoff_secs.max(1)),
    );
    loop {
        if *stop.borrow() {
            let _ = events.try_send(AgentEvent::Stopped);
            return;
        }
        let _ = events.try_send(AgentEvent::Connecting {
            endpoint: options.endpoint.authority(),
        });
        match connect_session(&options, &routes, &stop, &events).await {
            SessionOutcome::GracefulStop => {
                let _ = events.try_send(AgentEvent::Stopped);
                return;
            }
            SessionOutcome::Ended(reason) => {
                backoff.reset_on_success();
                tracing::info!(endpoint = %options.endpoint, reason, "tunnel session ended");
                let _ = events.try_send(AgentEvent::Disconnected { reason });
                if !options.network.reconnect {
                    let _ = events.try_send(AgentEvent::Stopped);
                    return;
                }
                options.metrics.record_reconnect();
                let delay = backoff.next();
                let _ = events.try_send(AgentEvent::ReconnectingIn {
                    after_ms: u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                });
                tokio::select! {
                    biased;
                    _ = stop.changed() => {
                        let _ = events.try_send(AgentEvent::Stopped);
                        return;
                    }
                    _ = tokio::time::sleep(delay) => {}
                }
            }
        }
    }
}

enum SessionOutcome {
    GracefulStop,
    Ended(String),
}

/// Route table the agent enforces locally: gateway-registered ids → local
/// targets plus the name index for gateway declarations (PRD §45: the
/// target never comes from the gateway).
struct RouteTable {
    by_id: HashMap<RouteId, Arc<TunnelRouteTemplate>>,
    by_name: HashMap<String, (RouteId, Arc<TunnelRouteTemplate>)>,
}

fn build_route_table(templates: &[TunnelRouteTemplate]) -> RouteTable {
    let mut by_id = HashMap::new();
    let mut by_name = HashMap::new();
    for template in templates {
        let id = RouteId::parse(format!("route_{}", template.name));
        let target = template.target();
        let matcher = template.matcher();
        match (id, target, matcher) {
            (Ok(id), Ok(_target), Ok(_matcher)) => {
                let shared_template = Arc::new(template.clone());
                by_id.insert(id.clone(), shared_template.clone());
                by_name.insert(template.name.clone(), (id, shared_template));
            }
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                tracing::warn!(
                    route = %template.name,
                    error = %error,
                    "tunnel route template is invalid; skipped"
                );
            }
        }
    }
    RouteTable { by_id, by_name }
}

async fn connect_session(
    options: &AgentRuntimeOptions,
    routes: &Arc<RouteTable>,
    stop: &watch::Receiver<bool>,
    events: &mpsc::Sender<AgentEvent>,
) -> SessionOutcome {
    let transport = match QuicClientTransport::new(&options.tls, transport_options(options)) {
        Ok(transport) => transport,
        Err(error) => return SessionOutcome::Ended(error.to_string()),
    };
    let server_name = tls::server_name_for(&options.endpoint);
    let connection = match transport.connect(&options.endpoint, &server_name).await {
        Ok(connection) => connection,
        Err(error) => return SessionOutcome::Ended(error.to_string()),
    };
    options.metrics.record_connection_open();

    // Control stream: the agent's one inbound stream (PRD §21).
    let mut control = match connection.open_stream().await {
        Ok(control) => control,
        Err(error) => {
            options.metrics.record_error();
            options.metrics.record_connection_close();
            return SessionOutcome::Ended(format!("control stream open failed: {error}"));
        }
    };
    // One scratch per session: a single read may slurp several control
    // frames, and leftovers must survive across message reads.
    let mut scratch = BytesMut::with_capacity(4096);

    let session_id = match authenticate(options, &mut *control, &mut scratch).await {
        Ok(session_id) => session_id,
        Err(error) => {
            options.metrics.record_connection_close();
            return SessionOutcome::Ended(error);
        }
    };
    let _ = events.try_send(AgentEvent::Connected {
        session_id: session_id.clone(),
    });

    let (ready, rejected) =
        match register_routes(options, routes, &mut *control, &mut scratch, events).await {
            Ok(outcome) => outcome,
            Err(error) => {
                options.metrics.record_connection_close();
                return SessionOutcome::Ended(error);
            }
        };
    let _ = events.try_send(AgentEvent::Ready {
        routes: ready,
        rejected,
    });

    let outcome = serve(options, routes, connection, control, scratch, stop, events).await;
    options.metrics.record_connection_close();
    outcome
}

async fn authenticate(
    options: &AgentRuntimeOptions,
    control: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
) -> std::result::Result<String, String> {
    let hello = ControlMessage::Hello(Hello {
        protocol_versions: vec![ProtocolVersion::v1()],
        device_id: options.device.id.to_string(),
        device_name: options.device.name.clone(),
        platform: options.device.platform.as_str().to_owned(),
    });
    write_msg(control, &hello).await?;
    write_msg(
        control,
        &ControlMessage::Authenticate(Authenticate {
            token: options.token.clone(),
        }),
    )
    .await?;
    let response = tokio::time::timeout(
        options.timeouts.handshake_duration(),
        read_msg(control, scratch),
    )
    .await
    .map_err(|_| "handshake timeout".to_owned())?;
    match response {
        Ok(ControlMessage::AuthResult(AuthResult {
            ok: true,
            session_id,
            ..
        })) => session_id.ok_or_else(|| "gateway omitted the session id".to_owned()),
        Ok(ControlMessage::AuthResult(AuthResult {
            ok: false,
            error: Some(detail),
            ..
        })) => Err(format!("authentication rejected: {detail}")),
        Ok(ControlMessage::Error(error)) => Err(format!(
            "gateway error during handshake: {:?} {}",
            error.code, error.message
        )),
        Ok(other) => Err(format!("unexpected handshake reply: {other:?}")),
        Err(message) => Err(message),
    }
}

type RegistrationOutcome = (Vec<RouteReadiness>, Vec<RouteRejection>);

async fn register_routes(
    options: &AgentRuntimeOptions,
    routes: &Arc<RouteTable>,
    control: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
    events: &mpsc::Sender<AgentEvent>,
) -> std::result::Result<RegistrationOutcome, String> {
    let _ = events;
    let mut requests = 0_usize;
    for template in &options.routes {
        let Some((route_id, _config)) = routes.by_name.get(&template.name) else {
            continue;
        };
        let request = ControlMessage::RegisterRoute(RegisterRoute {
            route_id: route_id.to_string(),
            name: template.name.clone(),
            protocol: protocol_label(template.protocol).to_owned(),
            domain: template.domain.clone(),
            port: template.port,
            target: template.target.clone(),
            allow_public: template.policy_or_default().allow_public,
        });
        write_msg(control, &request).await?;
        requests += 1;
    }
    let mut ready = Vec::new();
    let mut rejected = Vec::new();
    for _ in 0..requests {
        let reply = tokio::time::timeout(
            options.timeouts.handshake_duration(),
            read_msg(control, scratch),
        )
        .await
        .map_err(|_| "route registration timeout".to_owned())?;
        match reply {
            Ok(ControlMessage::RegisterRouteResult(RegisterRouteResult {
                route_id,
                ok,
                public_url,
                error,
            })) => {
                if ok {
                    ready.push(RouteReadiness {
                        name: route_id
                            .strip_prefix("route_")
                            .unwrap_or(&route_id)
                            .to_owned(),
                        route_id,
                        public_url,
                    });
                } else {
                    tracing::warn!(%route_id, rejected = ?error, "tunnel route rejected");
                    rejected.push(RouteRejection {
                        route_id,
                        error: error.unwrap_or_default(),
                    });
                }
            }
            Ok(ControlMessage::Error(gateway_error)) => {
                return Err(format!(
                    "gateway rejected registration: {:?} {}",
                    gateway_error.code, gateway_error.message
                ));
            }
            Ok(_) => return Err("unexpected reply during registration".to_owned()),
            Err(message) => return Err(message),
        }
    }
    Ok((ready, rejected))
}

/// Steady state: heartbeat writes, control-message reads (acks, gateway
/// declarations), and the data-stream accept loop (PRD §27, §28, §55).
async fn serve(
    options: &AgentRuntimeOptions,
    routes: &Arc<RouteTable>,
    connection: Box<dyn TunnelConnection>,
    mut control: Box<dyn TunnelStream>,
    mut scratch: BytesMut,
    stop: &watch::Receiver<bool>,
    events: &mpsc::Sender<AgentEvent>,
) -> SessionOutcome {
    let mut stop = stop.clone();
    let connection: Arc<dyn TunnelConnection> = Arc::from(connection);
    let (session_stop_tx, mut session_stop_rx) = watch::channel(false);
    let data_task = {
        let routes = routes.clone();
        let metrics = options.metrics.clone();
        let timeouts = options.timeouts.clone();
        let connection = connection.clone();
        let mut session_stop = session_stop_rx.clone();
        tokio::spawn(async move {
            data_accept_loop(&connection, &routes, &metrics, &timeouts, &mut session_stop).await;
        })
    };

    let heartbeat = Duration::from_secs(options.network.heartbeat_interval_secs.max(1));
    let mut ticker = tokio::time::interval(heartbeat);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick fires immediately; skip it so the first heartbeat goes
    // out one full interval after connect.
    ticker.tick().await;

    let outcome = loop {
        tokio::select! {
            biased;
            _ = stop.changed() => {
                break SessionOutcome::GracefulStop;
            }
            _ = session_stop_rx.changed() => {
                break SessionOutcome::Ended("local stream accept loop ended".to_owned());
            }
            _ = ticker.tick() => {
                if let Err(error) = write_message(&mut *control, &ControlMessage::Heartbeat).await {
                    break SessionOutcome::Ended(format!("heartbeat write failed: {error}"));
                }
            }
            message = read_message(&mut *control, &mut scratch) => {
                match message {
                    Ok(ControlMessage::HeartbeatAck) => {}
                    Ok(ControlMessage::DeclareRoute(declaration)) => {
                        if let Err(error) = handle_declaration(
                            options,
                            routes,
                            &mut *control,
                            &mut scratch,
                            &declaration,
                        )
                        .await
                        {
                            break SessionOutcome::Ended(error);
                        }
                    }
                    Ok(ControlMessage::Error(error)) => {
                        tracing::warn!(
                            code = ?error.code,
                            "gateway reported a control error: {}",
                            error.message
                        );
                    }
                    Ok(other) => {
                        break SessionOutcome::Ended(format!(
                            "unexpected control message from gateway: {other:?}"
                        ));
                    }
                    Err(error) => {
                        break SessionOutcome::Ended(format!("control stream failed: {error}"));
                    }
                }
            }
        }
    };

    let _ = session_stop_tx.send(true);
    data_task.abort();
    if matches!(outcome, SessionOutcome::GracefulStop) {
        let _ = events.try_send(AgentEvent::Disconnected {
            reason: "agent stopped".to_owned(),
        });
    }
    outcome
}

/// Gateway-initiated route declaration (PRD §35): activated only when it
/// matches a locally configured template, name and matcher both; the local
/// target always wins (PRD §45).
async fn handle_declaration(
    options: &AgentRuntimeOptions,
    routes: &Arc<RouteTable>,
    control: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
    declaration: &DeclareRoute,
) -> std::result::Result<(), String> {
    let Some((route_id, template)) = routes.by_name.get(&declaration.name) else {
        tracing::info!(
            declaration = %declaration.name,
            "gateway declaration does not match any local template; ignored"
        );
        return Ok(());
    };
    let matcher_matches = match template.protocol {
        sdkwork_webserver_tunnel_core::TunnelProtocolKind::Http => template
            .domain
            .as_deref()
            .zip(declaration.domain.as_deref())
            .map(|(local, declared)| local.eq_ignore_ascii_case(declared))
            .unwrap_or(false),
        sdkwork_webserver_tunnel_core::TunnelProtocolKind::Tcp
        | sdkwork_webserver_tunnel_core::TunnelProtocolKind::Udp => template
            .port
            .zip(declaration.port)
            .map(|(local, declared)| local == declared)
            .unwrap_or(false),
    };
    if !matcher_matches {
        tracing::info!(
            declaration = %declaration.name,
            "gateway declaration does not match the local template matcher; ignored"
        );
        return Ok(());
    }
    let request = ControlMessage::RegisterRoute(RegisterRoute {
        route_id: route_id.to_string(),
        name: template.name.clone(),
        protocol: protocol_label(template.protocol).to_owned(),
        domain: template.domain.clone(),
        port: template.port,
        target: template.target.clone(),
        allow_public: template.policy_or_default().allow_public,
    });
    write_msg(control, &request).await?;
    // Await the registration outcome, ignoring interleaved acks.
    loop {
        let reply = tokio::time::timeout(
            options.timeouts.handshake_duration(),
            read_msg(control, scratch),
        )
        .await
        .map_err(|_| "declaration registration timeout".to_owned())?;
        match reply {
            Ok(ControlMessage::RegisterRouteResult(RegisterRouteResult {
                route_id,
                ok: _,
                public_url,
                error,
            })) => {
                tracing::info!(
                    %route_id,
                    ?public_url,
                    ?error,
                    "gateway declaration resolved"
                );
                return Ok(());
            }
            Ok(ControlMessage::HeartbeatAck) => continue,
            Ok(ControlMessage::Error(gateway_error)) => {
                if gateway_error.code == ErrorCode::InvalidRoute {
                    return Err(format!(
                        "declaration rejected by gateway: {}",
                        gateway_error.message
                    ));
                }
                tracing::warn!(
                    code = ?gateway_error.code,
                    "gateway error during declaration: {}",
                    gateway_error.message
                );
                continue;
            }
            Ok(_) => return Err("unexpected reply during declaration".to_owned()),
            Err(message) => return Err(message),
        }
    }
}

/// Accepts gateway-opened data streams and relays each to the local target
/// (PRD §20, §61 LocalProxy).
async fn data_accept_loop(
    connection: &Arc<dyn TunnelConnection>,
    routes: &Arc<RouteTable>,
    metrics: &Arc<TunnelMetrics>,
    timeouts: &sdkwork_webserver_tunnel_core::TunnelTimeoutConfig,
    stop: &mut watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            biased;
            _ = stop.wait_for(|stopped| *stopped) => break,
            accepted = connection.accept_stream() => {
                            match accepted {
                    Ok(stream) => {
                        let routes = routes.clone();
                        let metrics = metrics.clone();
                        let timeouts = timeouts.clone();
                        tokio::spawn(async move {
                            handle_data_stream(stream, &routes, &metrics, &timeouts).await;
                        });
                    }
                    Err(error) => {
                        tracing::debug!(error = %error, "tunnel data stream accept ended");
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_data_stream(
    mut stream: Box<dyn TunnelStream>,
    routes: &Arc<RouteTable>,
    metrics: &Arc<TunnelMetrics>,
    timeouts: &sdkwork_webserver_tunnel_core::TunnelTimeoutConfig,
) {
    metrics.record_stream_open();
    let header_read = tokio::time::timeout(timeouts.stream_open_duration(), async {
        // Exact read: application bytes follow the header on the same
        // stream and must never be swallowed into a scratch buffer.
        let payload = frame::read_exact_frame(&mut stream).await?;
        DataStreamHeader::from_frame(&payload)
    })
    .await;
    let header = match header_read {
        Ok(Ok(header)) => header,
        Ok(Err(error)) => {
            metrics.record_error();
            tracing::debug!(error = %error, "tunnel data stream header failed");
            metrics.record_stream_close();
            return;
        }
        Err(_) => {
            metrics.record_error();
            tracing::debug!("tunnel data stream header timed out");
            metrics.record_stream_close();
            return;
        }
    };
    let route_id = match header.route() {
        Ok(route_id) => route_id,
        Err(error) => {
            metrics.record_error();
            tracing::debug!(error = %error, "tunnel data stream has an invalid route id");
            metrics.record_stream_close();
            return;
        }
    };
    let Some(template) = routes.by_id.get(&route_id) else {
        tracing::debug!(route = %route_id, "tunnel data stream references an unknown route");
        metrics.record_stream_close();
        return;
    };
    // Datagram targets ride a framed packet pump instead of copy_bidirectional.
    if matches!(
        template.protocol,
        sdkwork_webserver_tunnel_core::TunnelProtocolKind::Udp
    ) {
        let TunnelTarget::Udp(addr) = template.target().unwrap_or_else(|_| {
            TunnelTarget::parse_udp("127.0.0.1:1").expect("fallback target parses")
        }) else {
            tracing::debug!(route = %route_id, "udp route does not carry a udp target");
            metrics.record_stream_close();
            return;
        };
        udp_pump(stream.as_mut(), addr, &route_id, metrics, timeouts).await;
        metrics.record_stream_close();
        return;
    }
    let TunnelTarget::Tcp(addr) = template.target().unwrap_or_else(|_| {
        TunnelTarget::parse_tcp("127.0.0.1:1").expect("fallback target parses")
    }) else {
        tracing::debug!(route = %route_id, "unix targets are not supported on this platform");
        metrics.record_stream_close();
        return;
    };
    let dial = tokio::time::timeout(
        timeouts.connect_duration(),
        tokio::net::TcpStream::connect(addr),
    )
    .await;
    let mut local = match dial {
        Ok(Ok(local)) => local,
        Ok(Err(error)) => {
            metrics.record_error();
            tracing::info!(route = %route_id, %addr, error = %error, "local target unreachable");
            metrics.record_stream_close();
            return;
        }
        Err(_) => {
            metrics.record_error();
            tracing::info!(route = %route_id, %addr, "local target dial timed out");
            metrics.record_stream_close();
            return;
        }
    };
    match copy_bidirectional(&mut local, &mut stream).await {
        Ok((inbound, outbound)) => {
            tracing::trace!(route = %route_id, inbound, outbound, "tunnel data relay finished");
            metrics.add_bytes_in(inbound);
            metrics.add_bytes_out(outbound);
        }
        Err(error) => {
            tracing::trace!(route = %route_id, error = %error, "tunnel data relay ended");
        }
    }
    metrics.record_stream_close();
}

/// String-error variant used by the session functions, whose outcomes feed
/// [`SessionOutcome::Ended`].

/// UDP datagram pump (PRD: UDP 内网穿透): framed packets from the tunnel →
/// the local UDP target, replies from the target → framed back to the
/// tunnel. A datagram relay has no EOF, so both directions end on the
/// configured idle timeout instead.
async fn udp_pump(
    stream: &mut dyn TunnelStream,
    target: std::net::SocketAddr,
    route_id: &sdkwork_webserver_tunnel_core::RouteId,
    metrics: &Arc<TunnelMetrics>,
    timeouts: &sdkwork_webserver_tunnel_core::TunnelTimeoutConfig,
) {
    use tokio::io::AsyncWriteExt as _;

    let local = match tokio::time::timeout(
        timeouts.connect_duration(),
        tokio::net::UdpSocket::bind("0.0.0.0:0"),
    )
    .await
    {
        Ok(Ok(local)) => local,
        Ok(Err(error)) => {
            metrics.record_error();
            tracing::info!(route = %route_id, error = %error, "udp local bind failed");
            return;
        }
        Err(_) => {
            metrics.record_error();
            tracing::info!(route = %route_id, "udp local bind timed out");
            return;
        }
    };
    let local = std::sync::Arc::new(local);
    if let Err(error) = local.connect(target).await {
        tracing::info!(route = %route_id, %target, error = %error, "udp connect failed");
    }

    let idle = timeouts.idle_duration().min(Duration::from_secs(120));
    let mut tunnel_scratch = BytesMut::with_capacity(2048);
    let mut local_buffer = [0_u8; 2048];
    let mut last_activity = tokio::time::Instant::now();
    loop {
        let tunnel_read =
            sdkwork_webserver_tunnel_protocol::packet_frame::read_packet(
                stream,
                &mut tunnel_scratch,
            );
        let local_read = local.recv(&mut local_buffer);
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(last_activity + idle) => {
                tracing::debug!(route = %route_id, "udp pump idle timeout");
                return;
            }
            read = tunnel_read => {
                match read {
                    Ok(datagram) => {
                        if let Err(error) = local.send(&datagram).await {
                            tracing::debug!(route = %route_id, error = %error, "udp local send failed");
                            return;
                        }
                        metrics.add_bytes_in(u64::try_from(datagram.len()).unwrap_or(u64::MAX));
                        last_activity = tokio::time::Instant::now();
                    }
                    Err(error) => {
                        metrics.record_error();
                        tracing::debug!(route = %route_id, error = %error, "udp tunnel read failed");
                        return;
                    }
                }
            }
            read = local_read => {
                match read {
                    Ok(size) => {
                        let datagram = &local_buffer[..size];
                        let mut framed = bytes::BytesMut::with_capacity(
                            sdkwork_webserver_tunnel_protocol::packet_frame::PACKET_LENGTH_BYTES
                                + datagram.len(),
                        );
                        if let Err(error) = sdkwork_webserver_tunnel_protocol::packet_frame::encode_packet(datagram, &mut framed) {
                            tracing::debug!(route = %route_id, error = %error, "udp reply frame failed");
                            return;
                        }
                        if let Err(error) = stream.write_all(&framed).await {
                            tracing::debug!(route = %route_id, error = %error, "udp tunnel write failed");
                            return;
                        }
                        let _ = stream.flush().await;
                        metrics.add_bytes_out(u64::try_from(size).unwrap_or(u64::MAX));
                        last_activity = tokio::time::Instant::now();
                    }
                    Err(error) => {
                        metrics.record_error();
                        tracing::debug!(route = %route_id, error = %error, "udp local read failed");
                        return;
                    }
                }
            }
        }
    }
}

/// The wire protocol label for one route template (`http` / `tcp` / `udp`).
fn protocol_label(protocol: sdkwork_webserver_tunnel_core::TunnelProtocolKind) -> &'static str {
    match protocol {
        sdkwork_webserver_tunnel_core::TunnelProtocolKind::Http => "http",
        sdkwork_webserver_tunnel_core::TunnelProtocolKind::Tcp => "tcp",
        sdkwork_webserver_tunnel_core::TunnelProtocolKind::Udp => "udp",
    }
}

async fn write_msg(
    stream: &mut dyn TunnelStream,
    message: &ControlMessage,
) -> std::result::Result<(), String> {
    write_message(stream, message)
        .await
        .map_err(|error| error.to_string())
}

/// String-error read variant; see [`write_msg`].
async fn read_msg(
    stream: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
) -> std::result::Result<ControlMessage, String> {
    read_message(stream, scratch)
        .await
        .map_err(|error| error.to_string())
}

async fn write_message(stream: &mut dyn TunnelStream, message: &ControlMessage) -> Result<()> {
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

/// Reads one control message; transport failures map to `ConnectionClosed`
/// so callers treat them uniformly. `scratch` persists across calls so a
/// multi-frame chunk is not lost between reads.
async fn read_message(
    stream: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
) -> Result<ControlMessage> {
    let payload = frame::read_frame(stream, scratch).await?;
    ControlMessage::from_frame(&payload)
}

fn transport_options(options: &AgentRuntimeOptions) -> TransportOptions {
    TransportOptions {
        max_idle_timeout_ms: u32::try_from(options.timeouts.idle_duration().as_millis())
            .unwrap_or(u32::MAX),
        keep_alive_interval_ms: Some(5_000),
        max_concurrent_bidi_streams: 512,
    }
}

/// Jittered exponential backoff: 1s → 2s → 4s … capped, ±20% jitter
/// (PRD §27).
struct Backoff {
    current: Duration,
    initial: Duration,
    max: Duration,
}

impl Backoff {
    fn new(initial: Duration, max: Duration) -> Self {
        Self {
            current: initial,
            initial,
            max,
        }
    }

    fn next(&mut self) -> Duration {
        let jitter_scale = 0.8 + rand::random::<f64>() * 0.4;
        let delay = self.current.mul_f64(jitter_scale);
        self.current = (self.current * 2).min(self.max);
        delay
    }

    fn reset_on_success(&mut self) {
        self.current = self.initial;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps_with_jitter() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(8));
        for expected_secs in [1_u64, 2, 4, 8, 8] {
            let delay = backoff.next();
            let lower = Duration::from_millis(expected_secs * 800);
            let upper = Duration::from_millis(expected_secs * 1200);
            assert!(delay >= lower && delay <= upper, "{delay:?}");
        }
        backoff.reset_on_success();
        assert_eq!(backoff.current, Duration::from_secs(1));
    }

    #[test]
    fn route_table_skips_invalid_templates() {
        let templates = vec![
            TunnelRouteTemplate {
                name: "web".to_owned(),
                protocol: sdkwork_webserver_tunnel_core::TunnelProtocolKind::Http,
                domain: Some("demo.sdkwork.link".to_owned()),
                port: None,
                target: "127.0.0.1:3000".to_owned(),
                policy: None,
            },
            TunnelRouteTemplate {
                name: "broken".to_owned(),
                protocol: sdkwork_webserver_tunnel_core::TunnelProtocolKind::Http,
                domain: None,
                port: None,
                target: "127.0.0.1:3001".to_owned(),
                policy: None,
            },
        ];
        let table = build_route_table(&templates);
        assert_eq!(table.by_id.len(), 1);
        assert!(table.by_name.contains_key("web"));
        assert!(!table.by_name.contains_key("broken"));
    }
}
