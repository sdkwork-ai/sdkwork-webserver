//! Gateway: the public tunnel edge (PRD §6.1, §62).
//!
//! Owns the QUIC listener, per-connection control loops, session/route
//! management, security enforcement, TCP route listeners, and the shared
//! relay dispatch used by the webserver's HTTP plane. Composition follows
//! the dependency rule (PRD §43): this module consumes the transport
//! traits and the QUIC implementation, never raw `quinn` handles.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{watch, Mutex as AsyncMutex};

use sdkwork_webserver_tunnel_core::{
    DeviceId, Result, SessionId, TunnelConfig, TunnelError, TunnelLimitsConfig,
    TunnelNetworkConfig, TunnelRouteTemplate, TunnelTimeoutConfig, ValidationField,
};
use sdkwork_webserver_tunnel_transport::{
    QuicServerTransport, TunnelConnection, TunnelServerTransport,
};

use crate::metrics::TunnelMetrics;
use crate::security::{
    resolve_env_tokens as resolve_shared_tokens, AuthRateLimiter, RouteAcl, TokenAuthenticator,
};
use crate::service::TunnelService;

pub mod control;
pub mod devices;
pub mod dispatch;
pub mod health;
pub mod listeners;
pub(crate) mod rate_limit;
pub mod registry;
pub mod sessions;
pub mod tcpmux;
pub(crate) mod udp_listeners;
pub mod visitor;

pub use dispatch::{HttpRelay, RelayVisitor, RelayedStream};
pub use registry::RegisteredRoute;
pub use sessions::{SessionEntry, SessionTable};

/// State shared by every gateway component.
pub struct GatewayShared {
    /// Tunnel metrics (PRD §48).
    pub metrics: Arc<TunnelMetrics>,
    /// Resource ceilings (PRD §106).
    pub limits: TunnelLimitsConfig,
    /// Handshake and I/O timeouts (PRD §51).
    pub timeouts: TunnelTimeoutConfig,
    /// Route registry (PRD §29).
    pub registry: registry::RouteRegistry,
    /// Machine inventory: which devices this gateway manages, their history,
    /// traffic totals and the operator enable switch.
    pub devices: Arc<devices::DeviceRegistry>,
    /// Live session table (PRD §57).
    pub sessions: SessionTable,
    /// Visitor access-control evaluator (PRD §46).
    pub acl: RouteAcl,
    /// Agent bearer-token verifier (PRD §25).
    pub authenticator: TokenAuthenticator,
    /// Authentication failure rate limiter (PRD §107).
    pub rate_limiter: AuthRateLimiter,
    /// Required suffixes for agent-registered domain routes.
    pub domain_suffixes: Vec<String>,
    /// Places an agent must re-present its token, beyond the handshake (FRP
    /// `auth.additionalScopes`); advertised in the authentication result and
    /// enforced on the control plane.
    pub auth_scopes: Vec<sdkwork_webserver_tunnel_core::AuthScope>,
    /// Per-user credentials and quotas (FRP `user`, made enforceable).
    ///
    /// The token table is built from these entries, so a user's quota always
    /// belongs to the credential that authenticates it, and a route's user is
    /// resolved from the token rather than from the registration.
    pub users: Vec<sdkwork_webserver_tunnel_core::TunnelUserConfig>,
    /// Address the per-route 	cp/udp listeners bind (FRP proxyBindAddr).
    pub proxy_bind_addr: std::net::IpAddr,
    /// Inclusive public-port range a `tcp`/`udp` route may register; see
    /// [`TunnelGatewayOptions::route_port_min`].
    pub route_port_min: u16,
    /// Inclusive upper bound; see [`Self::route_port_min`].
    pub route_port_max: u16,
    /// Route declarations queued for devices that are currently offline
    /// (PRD §35 create-route API; memory-only in V1).
    pub pending_declarations: AsyncMutex<HashMap<DeviceId, Vec<TunnelRouteTemplate>>>,
    /// Global visitor/control connection admission. Every accepted QUIC
    /// control connection and every visitor TCP session holds one permit
    /// for its lifetime; UDP visitor sessions hold the per-session
    /// stream-budget permit instead (`maxStreamsPerSession` x sessions), so
    /// the concurrent task count is bounded in both cases rather than by
    /// the arrival rate.
    pub connection_admission: Arc<tokio::sync::Semaphore>,
    /// Public TCP listeners for `tcp` routes (PRD §31).
    pub(crate) tcp: Mutex<listeners::TcpListenerSet>,
    /// Public UDP listeners for `udp` routes (datagram relay).
    pub(crate) udp: Mutex<udp_listeners::UdpListenerSet>,
    /// Largest UDP datagram the relay accepts, framing prefix included
    /// (FRP `transport.udpPacketSize`, resolved from `[tunnel.network]`).
    pub udp_datagram_ceiling: usize,
    /// The **bound** multiplexed ingress port, or `0` when the ingress is
    /// disabled. Written once at startup, after the bind resolves (a requested
    /// port of 0 is replaced by the OS choice, and the status surface must
    /// report what actually listens, not what was asked for).
    multiplexer_port: AtomicU16,
    stream_counter: AtomicU64,
}

impl GatewayShared {
    /// Builds shared state from resolved options; used by composition sites
    /// that must reference the registry before the QUIC listener starts.
    pub fn from_options(options: &TunnelGatewayOptions) -> Arc<Self> {
        Arc::new(Self {
            metrics: options.metrics.clone(),
            limits: options.limits.clone(),
            timeouts: options.timeouts.clone(),
            registry: registry::RouteRegistry::new(),
            devices: Arc::new(devices::DeviceRegistry::new(
                usize::try_from(options.limits.max_devices).unwrap_or(usize::MAX),
            )),
            sessions: SessionTable::new(options.limits.max_streams_per_session),
            acl: RouteAcl,
            authenticator: options.authenticator.clone(),
            rate_limiter: AuthRateLimiter::new(
                options.auth_max_failures,
                Duration::from_secs(options.auth_window_secs.max(1)),
            ),
            connection_admission: Arc::new(tokio::sync::Semaphore::new(
                options.limits.max_connections.max(1) as usize,
            )),
            domain_suffixes: options.domain_suffixes.clone(),
            auth_scopes: options.auth_scopes.clone(),
            users: options.users.clone(),
            proxy_bind_addr: options.proxy_bind_addr,
            route_port_min: options.route_port_min,
            route_port_max: options.route_port_max,
            pending_declarations: AsyncMutex::new(HashMap::new()),
            tcp: Mutex::new(listeners::TcpListenerSet::default()),
            udp: Mutex::new(udp_listeners::UdpListenerSet::default()),
            // The ceiling is the configured payload size plus the frame length
            // prefix, so a datagram at the ceiling is read whole and one above
            // it is over the read buffer (and dropped, counted).
            udp_datagram_ceiling: options.network.udp_packet_size_bytes()
                + sdkwork_webserver_tunnel_protocol::packet_frame::PACKET_LENGTH_BYTES,
            multiplexer_port: AtomicU16::new(0),
            stream_counter: AtomicU64::new(0),
        })
    }

    /// The gateway's declared user, when present.
    ///
    /// Absent means "no per-user quota": a user the operator never declared
    /// keeps the global limits, which is exactly the pre-user behaviour.
    #[must_use]
    pub fn user(&self, name: &str) -> Option<&sdkwork_webserver_tunnel_core::TunnelUserConfig> {
        self.users.iter().find(|user| user.name.trim() == name)
    }

    /// The next per-gateway data-stream sequence number.
    pub fn next_stream_id(&self) -> sdkwork_webserver_tunnel_core::StreamId {
        let value = self.stream_counter.fetch_add(1, Ordering::Relaxed);
        sdkwork_webserver_tunnel_core::StreamId::new(value.saturating_add(1))
    }

    /// Records the multiplexed ingress port that actually bound.
    pub(crate) fn set_multiplexer_port(&self, port: u16) {
        self.multiplexer_port.store(port, Ordering::Relaxed);
    }

    /// The multiplexed ingress port (FRP `tcpmux` parity), when enabled.
    pub fn multiplexer_port(&self) -> Option<u16> {
        match self.multiplexer_port.load(Ordering::Relaxed) {
            0 => None,
            port => Some(port),
        }
    }

    /// The device behind a live session, when that session is still live.
    ///
    /// Session bookkeeping and the machine inventory are keyed differently (by
    /// session and by device); this is the one place that maps between them, so
    /// operator-facing records are never derived from an assumption about
    /// which id means what.
    pub fn session_device(
        &self,
        session_id: &sdkwork_webserver_tunnel_core::SessionId,
    ) -> Option<DeviceId> {
        self.sessions.device_of_session(session_id)
    }

    /// Relays an HTTP visitor stream matched by host and request path
    /// (PRD §30, FRP `locations` parity). The caller speaks whatever protocol
    /// it wants over the returned bytes.
    ///
    /// Returns the admission, not just the stream: the route's HTTP header
    /// rewriting (FRP `hostHeaderRewrite` / `requestHeaders.set` /
    /// `responseHeaders.set`) belongs to the hop that parses HTTP, and only the
    /// selection that happened here knows which route's policy applies.
    pub async fn connect_http_stream(
        self: &Arc<Self>,
        host: &str,
        path: &str,
        visitor: RelayVisitor<'_>,
    ) -> Result<HttpRelay> {
        dispatch::connect_http_stream(self, host, path, visitor).await
    }

    /// Relays a TCP visitor stream matched by gateway port (PRD §31).
    pub async fn connect_tcp_stream(
        self: &Arc<Self>,
        port: u16,
        client: std::net::SocketAddr,
    ) -> Result<RelayedStream> {
        dispatch::connect_tcp_stream(self, port, client).await
    }

    /// Session snapshots enriched with route ids (PRD §82 status surface).
    pub fn session_snapshots(&self) -> Vec<sdkwork_webserver_tunnel_core::SessionSnapshot> {
        let mut route_ids_by_session: HashMap<SessionId, Vec<String>> = HashMap::new();
        for route in self.registry.list() {
            if let Some(owner) = route.session_id.as_ref() {
                route_ids_by_session
                    .entry(owner.clone())
                    .or_default()
                    .push(route.id.to_string());
            }
        }
        self.sessions.snapshots(&route_ids_by_session)
    }
}

/// Gateway construction options, resolved by the composition site (CLI or
/// webserver integration) from [`TunnelConfig`] plus TLS material.
#[derive(Clone)]
pub struct TunnelGatewayOptions {
    /// QUIC listener bind address.
    pub bind: SocketAddr,
    /// Certificate chain (PEM) for the QUIC listener.
    pub cert_pem: Vec<u8>,
    /// Private key (PEM) for the QUIC listener.
    pub key_pem: Vec<u8>,
    /// Agent token verifier.
    pub authenticator: TokenAuthenticator,
    /// Environment variables holding gateway TLS material paths, resolved
    /// by the composition site.
    pub tls_cert_pem_env: Option<String>,
    pub tls_key_pem_env: Option<String>,
    /// Failed authentication attempts allowed per peer per window.
    pub auth_max_failures: u32,
    /// Authentication rate-limit window seconds.
    pub auth_window_secs: u64,
    /// Inclusive public-port range a `tcp`/`udp` route may register
    /// (registration policy, mirroring the gateway `routePortMin`/`Max`).
    pub route_port_min: u16,
    /// Inclusive upper bound; see [`Self::route_port_min`].
    pub route_port_max: u16,
    /// Optional single multiplexed ingress port speaking HTTP `CONNECT`
    /// (FRP `tcpmux` parity, as a public ingress): one port reaches every
    /// registered route, host-routed first and port-routed second.
    pub tcpmux_port: Option<u16>,
    /// Resource ceilings.
    pub limits: TunnelLimitsConfig,
    /// Timeouts.
    pub timeouts: TunnelTimeoutConfig,
    /// Heartbeat and reconnect settings.
    pub network: TunnelNetworkConfig,
    /// Allowed domain suffixes for registered domain routes.
    pub domain_suffixes: Vec<String>,
    /// Places an agent must re-present its token, beyond the handshake (FRP
    /// `auth.additionalScopes`); advertised in the authentication result and
    pub auth_scopes: Vec<sdkwork_webserver_tunnel_core::AuthScope>,
    /// Per-user credentials and quotas (FRP `user`, made enforceable).
    ///
    /// The token table is built from these entries, so a user's quota always
    /// belongs to the credential that authenticates it, and a route's user is
    /// resolved from the token rather than from the registration.
    pub users: Vec<sdkwork_webserver_tunnel_core::TunnelUserConfig>,
    /// Address the per-route 	cp/udp listeners bind (FRP proxyBindAddr).
    pub proxy_bind_addr: std::net::IpAddr,
    /// Shared metrics.
    pub metrics: Arc<TunnelMetrics>,
}

impl TunnelGatewayOptions {
    /// Resolves options from a validated [`TunnelConfig`] and explicit TLS
    /// material.
    pub fn from_config(
        config: &TunnelConfig,
        bind: SocketAddr,
        cert_pem: Vec<u8>,
        key_pem: Vec<u8>,
    ) -> Self {
        let gateway = config.gateway_or_default();
        // Per-user tokens are resolved here, at composition time: the
        // authenticator then knows which user each credential speaks for, and
        // nothing downstream has to trust a claimed name.
        let user_tokens: Vec<(String, String)> = gateway
            .users
            .iter()
            .filter_map(|user| {
                let token = std::env::var(user.token_env.trim()).ok()?;
                let token = token.trim().to_owned();
                (!token.is_empty()).then(|| (user.name.trim().to_owned(), token))
            })
            .collect();
        Self {
            bind,
            cert_pem,
            key_pem,
            authenticator: TokenAuthenticator::with_users(
                resolve_shared_tokens(&gateway.agent_token_env),
                user_tokens,
            ),
            tls_cert_pem_env: gateway.tls_cert_pem_env.clone(),
            tls_key_pem_env: gateway.tls_key_pem_env.clone(),
            auth_max_failures: 10,
            auth_window_secs: 60,
            route_port_min: gateway.route_port_min,
            route_port_max: gateway.route_port_max,
            tcpmux_port: gateway.tcpmux_port,
            limits: config.limits_or_default(),
            timeouts: config.timeout_or_default(),
            network: config.network_or_default(),
            domain_suffixes: gateway.domain_suffixes.clone(),
            auth_scopes: gateway.auth_scopes.clone(),
            proxy_bind_addr: gateway
                .proxy_bind_addr()
                .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
            users: gateway.users.clone(),
            metrics: Arc::new(TunnelMetrics::new()),
        }
    }
}

/// Events emitted while the gateway runs, consumed by CLI and integration
/// logging.
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    /// The QUIC listener is bound and accepting.
    Listening(SocketAddr),
    /// The multiplexed HTTP `CONNECT` ingress is bound and accepting.
    MultiplexerListening(u16),
    /// An agent completed authentication.
    SessionReady {
        /// Session identity.
        session_id: SessionId,
        /// Authenticated device.
        device_id: DeviceId,
    },
    /// A session ended.
    SessionClosed {
        /// Session identity.
        session_id: SessionId,
    },
    /// The gateway stopped.
    Stopped,
}

/// A running tunnel gateway (PRD §6.1). Dropping without `shutdown` is safe
/// but abrupt; prefer [`TunnelGateway::shutdown`].
pub struct TunnelGateway {
    shared: Arc<GatewayShared>,
    transport: Arc<QuicServerTransport>,
    quic_port: u16,
    /// Bound multiplexer port, when the HTTP `CONNECT` ingress is enabled.
    tcpmux_port: Option<u16>,
    stop_tx: watch::Sender<bool>,
    accept_task: tokio::task::JoinHandle<()>,
    sweeper_task: tokio::task::JoinHandle<()>,
    /// Accept loop for the multiplexed ingress, when enabled.
    tcpmux_task: Option<tokio::task::JoinHandle<()>>,
}

impl TunnelGateway {
    /// Binds the QUIC listener and starts the gateway background tasks.
    pub async fn spawn(
        options: TunnelGatewayOptions,
        events: Option<tokio::sync::mpsc::Sender<GatewayEvent>>,
    ) -> Result<Self> {
        let shared = GatewayShared::from_options(&options);
        Self::spawn_with_shared(shared, options, events).await
    }

    /// Binds the QUIC listener over caller-built shared state (the
    /// webserver integration builds it during runtime construction).
    pub async fn spawn_with_shared(
        shared: Arc<GatewayShared>,
        options: TunnelGatewayOptions,
        events: Option<tokio::sync::mpsc::Sender<GatewayEvent>>,
    ) -> Result<Self> {
        if !options.authenticator.is_configured() {
            tracing::warn!(
                "tunnel gateway starts WITHOUT any agent token configured; every agent will be refused"
            );
        }
        let transport = QuicServerTransport::from_pem(
            options.bind,
            &options.cert_pem,
            &options.key_pem,
            transport_options(&options.network, &options.timeouts, &options.limits),
        )?;
        let local_addr = transport.local_addr()?;
        let (stop_tx, stop_rx) = watch::channel(false);
        let transport = Arc::new(transport);
        let accept_task = {
            let shared = shared.clone();
            let transport = transport.clone();
            let mut stop_rx = stop_rx.clone();
            let events = events.clone();
            tokio::spawn(async move {
                accept_loop(shared, transport, &mut stop_rx, events).await;
            })
        };
        let sweeper_task = {
            let shared = shared.clone();
            let mut stop_rx = stop_rx.clone();
            tokio::spawn(async move {
                heartbeat_sweeper(shared, &options.network, &options.timeouts, &mut stop_rx).await;
            })
        };
        // The multiplexed ingress is optional: a gateway that has not been
        // asked for it binds nothing, so an operator cannot open a port by
        // upgrading.
        let (tcpmux_task, tcpmux_port) = match options.tcpmux_port {
            Some(requested) => {
                match tokio::net::TcpListener::bind(("0.0.0.0", requested)).await {
                    Ok(listener) => {
                        let bound = listener
                            .local_addr()
                            .map(|addr| addr.port())
                            .unwrap_or(requested);
                        tracing::info!(
                            port = bound,
                            "tunnel multiplexer listening (HTTP CONNECT reaches every route)"
                        );
                        if let Some(events) = events.as_ref() {
                            let _ = events.try_send(GatewayEvent::MultiplexerListening(bound));
                        }
                        shared.set_multiplexer_port(bound);
                        let shared = shared.clone();
                        let stop_rx = stop_rx.clone();
                        let task = tokio::spawn(async move {
                            tcpmux::accept_loop(shared, listener, stop_rx.clone()).await;
                        });
                        (Some(task), Some(bound))
                    }
                    Err(error) => {
                        // A port the operator explicitly asked for must not be
                        // silently skipped: the gateway refuses to start with
                        // half of its configured ingress.
                        return Err(TunnelError::Validation {
                            field: ValidationField::Config,
                            reason: format!("tcpmuxPort {requested} could not be bound: {error}"),
                        });
                    }
                }
            }
            None => (None, None),
        };
        tracing::info!(%local_addr, "tunnel gateway listening");
        if let Some(events) = events.as_ref() {
            let _ = events.try_send(GatewayEvent::Listening(local_addr));
        }
        Ok(Self {
            shared,
            transport,
            quic_port: local_addr.port(),
            tcpmux_port,
            stop_tx,
            accept_task,
            sweeper_task,
            tcpmux_task,
        })
    }

    /// The bound QUIC listener port (resolved from an ephemeral bind).
    pub fn quic_port(&self) -> u16 {
        self.quic_port
    }

    /// The bound multiplexer port, when the multiplexed ingress is enabled.
    pub fn tcpmux_port(&self) -> Option<u16> {
        self.tcpmux_port
    }

    /// Shared state for dispatch, service, and status surfaces.
    pub fn shared(&self) -> Arc<GatewayShared> {
        self.shared.clone()
    }

    /// The [`TunnelService`] facade over this gateway (PRD §41).
    pub fn service(&self) -> Arc<dyn TunnelService> {
        Arc::new(crate::service::GatewayTunnelService {
            shared: self.shared.clone(),
        })
    }

    /// Stops the gateway: closes the QUIC endpoint and every TCP listener,
    /// then joins the background tasks (PRD §105).
    pub async fn shutdown(self) {
        let _ = self.stop_tx.send(true);
        // Closing the endpoint refuses new connections and drops existing
        // ones, which unwinds every control loop through its teardown path
        // (PRD §105: stop accept → notify sessions → close streams).
        self.transport.close();
        let _ = self.accept_task.await;
        let _ = self.sweeper_task.await;
        if let Some(task) = self.tcpmux_task {
            let _ = task.await;
        }
        self.shared
            .tcp
            .lock()
            .expect("tcp listener set lock is never held across awaits")
            .shutdown();
        self.shared
            .udp
            .lock()
            .expect("udp listener set lock is never held across awaits")
            .shutdown();
        tracing::info!("tunnel gateway stopped");
    }
}

async fn accept_loop(
    shared: Arc<GatewayShared>,
    transport: Arc<QuicServerTransport>,
    stop: &mut watch::Receiver<bool>,
    events: Option<tokio::sync::mpsc::Sender<GatewayEvent>>,
) {
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => break,
            accepted = transport.accept() => {
                match accepted {
                    Ok(connection) => {
                        // Bounded admission: a saturated gateway closes the
                        // new control connection instead of queueing tasks.
                        let Ok(permit) = shared.connection_admission.clone().try_acquire_owned() else {
                            shared.metrics.record_error();
                            tracing::warn!("tunnel gateway at connection capacity; control connection closed");
                            drop(connection);
                            continue;
                        };
                        let shared = shared.clone();
                        let events = events.clone();
                        let stop = stop.clone();
                        let peer = connection
                            .remote_addr()
                            .unwrap_or_else(|_| unspecified_peer());
                        let connection: Arc<dyn TunnelConnection> = Arc::from(connection);
                        tokio::spawn(async move {
                            let _permit = permit;
                            control::run_connection(shared, connection, peer, stop).await;
                            let _ = events;
                        });
                    }
                    Err(error) => {
                        shared.metrics.record_error();
                        tracing::warn!(error = %error, "tunnel gateway accept failed");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        }
    }
}

fn unspecified_peer() -> SocketAddr {
    "0.0.0.0:0".parse().expect("valid socket address")
}

async fn heartbeat_sweeper(
    shared: Arc<GatewayShared>,
    network: &TunnelNetworkConfig,
    timeouts: &TunnelTimeoutConfig,
    stop: &mut watch::Receiver<bool>,
) {
    // A session is stale after three missed heartbeats, bounded by the
    // configured idle timeout — or after the explicitly configured
    // `heartbeatTimeoutSecs` (PRD §28, §51; FRP `transport.heartbeatTimeout`).
    let heartbeat = Duration::from_secs(network.heartbeat_interval_secs.max(1));
    let stale_after = network.stale_after(timeouts.idle_duration());
    let mut ticker = tokio::time::interval(heartbeat.max(Duration::from_secs(1)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => break,
            _ = ticker.tick() => {
                for session_id in shared.sessions.expire_idle(stale_after, &shared.metrics) {
                    control::teardown_session(&shared, &session_id);
                }
                shared.rate_limiter.sweep();
            }
        }
    }
}

fn transport_options(
    network: &TunnelNetworkConfig,
    timeouts: &TunnelTimeoutConfig,
    limits: &TunnelLimitsConfig,
) -> sdkwork_webserver_tunnel_transport::TransportOptions {
    sdkwork_webserver_tunnel_transport::TransportOptions {
        max_idle_timeout_ms: u32::try_from(timeouts.idle_duration().as_millis())
            .unwrap_or(u32::MAX),
        // Keep-alive cadence is configurable (FRP `quic.keepalivePeriod`); the
        // default is half the heartbeat interval, so a missed heartbeat is
        // noticed before the session expires.
        keep_alive_interval_ms: network
            .keep_alive_interval()
            .map(|interval| u32::try_from(interval.as_millis()).unwrap_or(u32::MAX)),
        max_concurrent_bidi_streams: network.max_concurrent_streams(limits.max_streams_per_session),
    }
}
