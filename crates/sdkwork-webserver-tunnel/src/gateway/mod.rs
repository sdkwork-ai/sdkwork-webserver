//! Gateway: the public tunnel edge (PRD §6.1, §62).
//!
//! Owns the QUIC listener, per-connection control loops, session/route
//! management, security enforcement, TCP route listeners, and the shared
//! relay dispatch used by the webserver's HTTP plane. Composition follows
//! the dependency rule (PRD §43): this module consumes the transport
//! traits and the QUIC implementation, never raw `quinn` handles.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{watch, Mutex as AsyncMutex};

use sdkwork_webserver_tunnel_core::{
    DeviceId, Result, SessionId, TunnelConfig, TunnelLimitsConfig, TunnelNetworkConfig,
    TunnelRouteTemplate, TunnelTimeoutConfig,
};
use sdkwork_webserver_tunnel_transport::{
    QuicServerTransport, TunnelConnection, TunnelServerTransport,
};

use crate::metrics::TunnelMetrics;
use crate::security::{AuthRateLimiter, RouteAcl, TokenAuthenticator};
use crate::service::TunnelService;

pub mod control;
pub mod dispatch;
pub mod listeners;
pub(crate) mod udp_listeners;
pub mod registry;
pub mod sessions;

pub use dispatch::{RelayVisitor, RelayedStream};
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
    /// Route declarations queued for devices that are currently offline
    /// (PRD §35 create-route API; memory-only in V1).
    pub pending_declarations: AsyncMutex<HashMap<DeviceId, Vec<TunnelRouteTemplate>>>,
    /// Public TCP listeners for `tcp` routes (PRD §31).
    pub(crate) tcp: Mutex<listeners::TcpListenerSet>,
    /// Public UDP listeners for `udp` routes (datagram relay).
    pub(crate) udp: Mutex<udp_listeners::UdpListenerSet>,
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
            sessions: SessionTable::new(options.limits.max_streams_per_session),
            acl: RouteAcl,
            authenticator: options.authenticator.clone(),
            rate_limiter: AuthRateLimiter::new(
                options.auth_max_failures,
                Duration::from_secs(options.auth_window_secs.max(1)),
            ),
            domain_suffixes: options.domain_suffixes.clone(),
            pending_declarations: AsyncMutex::new(HashMap::new()),
            tcp: Mutex::new(listeners::TcpListenerSet::default()),
            udp: Mutex::new(udp_listeners::UdpListenerSet::default()),
            stream_counter: AtomicU64::new(0),
        })
    }

    /// The next per-gateway data-stream sequence number.
    pub fn next_stream_id(&self) -> sdkwork_webserver_tunnel_core::StreamId {
        let value = self.stream_counter.fetch_add(1, Ordering::Relaxed);
        sdkwork_webserver_tunnel_core::StreamId::new(value.saturating_add(1))
    }

    /// Relays an HTTP visitor stream matched by host (PRD §30). The caller
    /// speaks whatever protocol it wants over the returned bytes.
    pub async fn connect_http_stream(
        self: &Arc<Self>,
        host: &str,
        visitor: RelayVisitor<'_>,
    ) -> Result<RelayedStream> {
        dispatch::connect_http_stream(self, host, visitor).await
    }

    /// Relays a TCP visitor stream matched by gateway port (PRD §31).
    pub async fn connect_tcp_stream(
        self: &Arc<Self>,
        port: u16,
        client_ip: std::net::IpAddr,
    ) -> Result<RelayedStream> {
        dispatch::connect_tcp_stream(self, port, client_ip).await
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
    /// Resource ceilings.
    pub limits: TunnelLimitsConfig,
    /// Timeouts.
    pub timeouts: TunnelTimeoutConfig,
    /// Heartbeat and reconnect settings.
    pub network: TunnelNetworkConfig,
    /// Allowed domain suffixes for registered domain routes.
    pub domain_suffixes: Vec<String>,
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
        Self {
            bind,
            cert_pem,
            key_pem,
            authenticator: TokenAuthenticator::from_env(&gateway.agent_token_env),
            tls_cert_pem_env: gateway.tls_cert_pem_env.clone(),
            tls_key_pem_env: gateway.tls_key_pem_env.clone(),
            auth_max_failures: 10,
            auth_window_secs: 60,
            limits: config.limits_or_default(),
            timeouts: config.timeout_or_default(),
            network: config.network_or_default(),
            domain_suffixes: gateway.domain_suffixes.clone(),
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
    stop_tx: watch::Sender<bool>,
    accept_task: tokio::task::JoinHandle<()>,
    sweeper_task: tokio::task::JoinHandle<()>,
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
        tracing::info!(%local_addr, "tunnel gateway listening");
        if let Some(events) = events.as_ref() {
            let _ = events.try_send(GatewayEvent::Listening(local_addr));
        }
        Ok(Self {
            shared,
            transport,
            quic_port: local_addr.port(),
            stop_tx,
            accept_task,
            sweeper_task,
        })
    }

    /// The bound QUIC listener port (resolved from an ephemeral bind).
    pub fn quic_port(&self) -> u16 {
        self.quic_port
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
                        let shared = shared.clone();
                        let events = events.clone();
                        let stop = stop.clone();
                        let peer = connection
                            .remote_addr()
                            .unwrap_or_else(|_| unspecified_peer());
                        let connection: Arc<dyn TunnelConnection> = Arc::from(connection);
                        tokio::spawn(async move {
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
    // A session is stale after roughly three missed heartbeats, bounded by
    // the configured idle timeout (PRD §28, §51).
    let heartbeat = Duration::from_secs(network.heartbeat_interval_secs.max(1));
    let stale_after = timeouts.idle_duration().min(heartbeat * 3);
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
        keep_alive_interval_ms: Some(
            u32::try_from((network.heartbeat_interval_secs.max(1) * 1000) / 2).unwrap_or(u32::MAX),
        ),
        max_concurrent_bidi_streams: limits.max_streams_per_session.max(16),
    }
}
