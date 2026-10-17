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
    AuthPolicy, Device, ProxyProtocolVersion, Result, RouteId, TunnelConfig, TunnelError,
    TunnelRouteTemplate, TunnelTarget,
};
use sdkwork_webserver_tunnel_protocol::{
    frame, AuthResult, Authenticate, ControlMessage, DataStreamHeader, DeclareRoute, ErrorCode,
    Hello, ProtocolVersion, Reauthenticate, RegisterRoute, RegisterRouteResult, RouteHealthReport,
    VisitorAddress,
};
use sdkwork_webserver_tunnel_transport::{
    tls, QuicClientTransport, RemoteEndpoint, TransportOptions, TunnelClientTransport,
    TunnelConnection, TunnelStream,
};

use crate::metrics::TunnelMetrics;

/// Deadline for writing one heartbeat control frame. Must stay far below the
/// transport idle timeout (300s): a heartbeat write that stalls this long
/// means the control stream is wedged, and the session loop must end and
/// reconnect rather than park every other select arm behind it.
const HEARTBEAT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// How many health verdicts may queue while the control stream is busy. Probe
/// results are cumulative, so a deeper queue buys nothing: the newest verdict
/// supersedes the older ones.
const HEALTH_REPORT_QUEUE: usize = 32;

/// How long AgentRuntime::reload waits for the session to apply a new route
/// set. A reload touches the control plane once per changed route, so the
/// budget is generous; the operator is waiting synchronously either way.
const RELOAD_TIMEOUT: Duration = Duration::from_secs(30);

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Gateway endpoint (the primary).
    pub endpoint: RemoteEndpoint,
    /// Further gateway addresses tried in order when the current one cannot be
    /// reached (FRP's ordered `serverAddr` list).
    ///
    /// Failover only advances on an attempt that never became established: a
    /// session that served traffic and then dropped is not evidence that the
    /// address is bad, and moving away from a healthy-but-restarted gateway
    /// would turn one rollout into a permanent migration.
    pub alternates: Vec<RemoteEndpoint>,
    /// Stop instead of retrying when the gateway rejects our credentials
    /// (FRP `loginFailExit`).
    ///
    /// A rejected credential is an operator error, not an outage: retrying it
    /// forever hides the mistake behind a reconnect log. Only a *credential
    /// rejection* ends the process — a timeout or an unreachable gateway keeps
    /// retrying.
    pub login_fail_exit: bool,
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
        let alternates = config
            .agent
            .as_ref()
            .map(sdkwork_webserver_tunnel_core::TunnelAgentConfig::gateway_endpoints)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|address| parse_endpoint_authority(&address))
            // The primary is passed in explicitly and stays first; a list that
            // repeats it must not produce a second attempt against the same
            // address.
            .filter(|candidate| candidate.authority() != endpoint.authority())
            .collect();
        // The machine's user claim travels on the device, which is what the
        // gateway compares against the token before it labels anything. An
        // invalid name is refused at startup (the config layer validates it
        // too), so the fallback below only keeps the value as given.
        let device = match config
            .agent
            .as_ref()
            .map(|agent| agent.user.trim())
            .filter(|user| !user.is_empty())
        {
            Some(user) => match device.clone().with_user(Some(user.to_owned())) {
                Ok(device) => device,
                Err(error) => {
                    tracing::warn!(%error, user, "agent user claim is not usable; continuing unlabelled");
                    device
                }
            },
            None => device,
        };
        Self {
            endpoint,
            alternates,
            login_fail_exit: config
                .agent
                .as_ref()
                .is_some_and(|agent| agent.login_fail_exit),
            tls,
            device,
            token,
            routes: config.routes_or_empty().to_vec(),
            network: config.network_or_default(),
            timeouts: config.timeout_or_default(),
            metrics,
        }
    }

    /// Every gateway address to try, the primary first.
    #[must_use]
    pub fn gateway_endpoints(&self) -> Vec<RemoteEndpoint> {
        std::iter::once(self.endpoint.clone())
            .chain(self.alternates.iter().cloned())
            .collect()
    }
}

/// Which gateway address the next attempt should use.
///
/// The policy is deliberately two-sided:
///
/// - an attempt that **never connected** advances to the next address, because
///   the current one is the thing that just failed;
/// - an attempt that **was established** returns to the primary (index 0), so
///   a gateway that merely restarted is picked up again instead of being
///   abandoned for a fallback that may be a worse network path.
///
/// A single-address configuration always answers 0, which is why this is a
/// function rather than inline arithmetic: the two rules above are what the
/// tests pin.
fn next_endpoint_index(current: usize, total: usize, established: bool) -> usize {
    if total <= 1 || established {
        return 0;
    }
    (current + 1) % total
}

/// Parses a validated `host:port` authority into a [`RemoteEndpoint`].
///
/// The configuration layer already refuses a malformed address, so a `None`
/// here means a caller built options by hand; such an address is skipped
/// rather than turned into a dial to a nonsense host.
fn parse_endpoint_authority(address: &str) -> Option<RemoteEndpoint> {
    let (host, port) = address.trim().rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    if host.is_empty() || port == 0 {
        return None;
    }
    Some(RemoteEndpoint::new(host, port))
}

/// The agent's live route configuration, swapped atomically on reload
/// (FRP `frpc reload` parity: `frpc reload -c frpc.toml`).
///
/// Two views of one configuration: the templates a session registers at the
/// gateway, and the table the data plane resolves incoming streams against.
/// They travel together because a route that is registered but not resolvable
/// black-holes visitor traffic, and a route that is resolvable but not
/// registered is dead weight.
struct RouteState {
    templates: Vec<TunnelRouteTemplate>,
    table: Arc<RouteTable>,
}

impl RouteState {
    fn build(
        templates: Vec<TunnelRouteTemplate>,
        user: Option<&str>,
    ) -> std::result::Result<Self, String> {
        // Every template is validated before anything is published: a reload
        // that half-applies would leave the agent serving a configuration the
        // operator never asked for.
        for template in &templates {
            template.validate().map_err(|error| error.to_string())?;
        }
        let table = Arc::new(build_route_table(&templates, user));
        Ok(Self { templates, table })
    }
}

/// A reload request from [`AgentRuntime::reload`].
struct ReloadRequest {
    templates: Vec<TunnelRouteTemplate>,
    reply: tokio::sync::oneshot::Sender<std::result::Result<RouteReloadReport, String>>,
}

/// What a reload changed, as the operator sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteReloadReport {
    /// Routes that are now registered and were not before.
    pub added: Vec<String>,
    /// Routes that were unregistered.
    pub removed: Vec<String>,
    /// Routes whose definition did not change.
    pub unchanged: Vec<String>,
    /// Routes the gateway refused, with its reason.
    pub rejected: Vec<RouteRejection>,
    /// True when the new configuration was applied without a live session and
    /// will be registered at the next connect.
    pub applied_offline: bool,
}

/// A running agent loop handle (PRD §61: Identity + Transport + Session +
/// Registration + Heartbeat + Reconnect + LocalProxy).
pub struct AgentRuntime {
    stop_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
    events_rx: mpsc::Receiver<AgentEvent>,
    /// Reload channel (FRP `frpc reload` parity).
    reload_tx: mpsc::Sender<ReloadRequest>,
}

impl AgentRuntime {
    /// Starts the agent connect/reconnect loop.
    pub fn spawn(options: AgentRuntimeOptions) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        let (events_tx, events_rx) = mpsc::channel(64);
        let (reload_tx, reload_rx) = mpsc::channel(4);
        let task = tokio::spawn(run_loop(options, stop_rx, events_tx, reload_rx));
        Self {
            stop_tx,
            task,
            events_rx,
            reload_tx,
        }
    }

    /// Receives the next lifecycle event.
    pub async fn next_event(&mut self) -> Option<AgentEvent> {
        self.events_rx.recv().await
    }

    /// Replaces the route set at runtime, the way `frpc reload` does.
    ///
    /// Removed routes are unregistered at the gateway, added or changed ones
    /// are registered, and the data plane switches to the new table. Streams
    /// already in flight keep running: a reload must not cut live traffic, or
    /// every configuration change becomes an outage.
    ///
    /// With no live session the new configuration is published and applies at
    /// the next connect; the report says so (`appliedOffline`).
    pub async fn reload(
        &self,
        routes: Vec<TunnelRouteTemplate>,
    ) -> std::result::Result<RouteReloadReport, String> {
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        self.reload_tx
            .send(ReloadRequest {
                templates: routes,
                reply,
            })
            .await
            .map_err(|_| "the agent runtime has stopped; reload is not possible".to_owned())?;
        match tokio::time::timeout(RELOAD_TIMEOUT, reply_rx).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Err("the agent dropped the reload request".to_owned()),
            Err(_) => Err("the reload timed out (the agent did not answer)".to_owned()),
        }
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
    mut reload: mpsc::Receiver<ReloadRequest>,
) {
    // One user for the whole process: it decides the namespace every route id
    // is derived in, at startup and across reloads alike, so a reload can never
    // move a route into a namespace the gateway would then refuse.
    let user: Option<String> = options.device.user.clone();
    let user = user.as_deref();
    let initial_state = match RouteState::build(options.routes.clone(), user) {
        Ok(state) => Arc::new(state),
        Err(error) => {
            // A configuration that cannot be built is a startup failure, not a
            // silent no-op: the operator must see why nothing is registered.
            tracing::error!(%error, "tunnel route configuration is unusable");
            let _ = events.try_send(AgentEvent::Stopped);
            return;
        }
    };
    let (state_tx, state_rx) = watch::channel(initial_state);
    let mut backoff = Backoff::new(
        Duration::from_secs(options.network.initial_backoff_secs.max(1)),
        Duration::from_secs(options.network.max_backoff_secs.max(1)),
    );
    // FRP's ordered `serverAddr` list: the primary is index 0, and the cursor
    // advances only when an attempt never became established. A single-address
    // configuration has a one-entry list, so the loop below is the old
    // behaviour with no extra state to reason about.
    let endpoints = options.gateway_endpoints();
    let mut cursor = 0_usize;
    loop {
        if *stop.borrow() {
            let _ = events.try_send(AgentEvent::Stopped);
            return;
        }
        let endpoint = endpoints
            .get(cursor % endpoints.len().max(1))
            .cloned()
            .unwrap_or_else(|| options.endpoint.clone());
        let _ = events.try_send(AgentEvent::Connecting {
            endpoint: endpoint.authority(),
        });
        match connect_session(
            &options,
            &endpoint,
            &state_tx,
            &state_rx,
            &mut reload,
            &stop,
            &events,
        )
        .await
        {
            SessionAttempt {
                outcome: SessionOutcome::GracefulStop,
                ..
            } => {
                let _ = events.try_send(AgentEvent::Stopped);
                return;
            }
            SessionAttempt {
                outcome: SessionOutcome::Ended(reason),
                established,
                uptime,
                address_related,
                credentials_rejected,
            } => {
                // Only a session that actually served resets the curve
                // (PRD §27). A dial or handshake failure returns `Ended`
                // within milliseconds; resetting there would pin the retry
                // rate at the initial delay and turn a gateway outage into
                // a fleet-wide ~1 Hz reconnect storm.
                if should_reset_backoff(
                    established,
                    uptime,
                    stable_uptime_threshold(&options.network),
                ) {
                    backoff.reset_on_success();
                }
                // Failover, and the return path: an address-related attempt
                // that never connected moves to the next address, while a
                // session that was established (and has now ended for any
                // reason) sends the agent back to the primary, so a restarted
                // gateway is picked up again instead of being abandoned for
                // the fallback. A local misconfiguration never advances the
                // cursor: every address would fail the same way.
                cursor = if address_related {
                    next_endpoint_index(cursor, endpoints.len(), established)
                } else {
                    cursor
                };
                tracing::info!(endpoint = %endpoint, reason, established, "tunnel session ended");
                // FRP `loginFailExit`: a rejected credential is an operator
                // error, not an outage. Retrying it forever would bury the one
                // line that explains it under a reconnect log, so with the
                // switch on the process stops here — after reporting why.
                let stop_after_rejection = options.login_fail_exit && credentials_rejected;
                let _ = events.try_send(AgentEvent::Disconnected { reason });
                if stop_after_rejection {
                    tracing::error!(
                        endpoint = %endpoint,
                        "tunnel credentials were rejected and loginFailExit is set; stopping"
                    );
                    let _ = events.try_send(AgentEvent::Stopped);
                    return;
                }
                if !options.network.reconnect {
                    let _ = events.try_send(AgentEvent::Stopped);
                    return;
                }
                options.metrics.record_reconnect();
                let delay = backoff.next();
                let _ = events.try_send(AgentEvent::ReconnectingIn {
                    after_ms: u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                });
                // A reload arriving while disconnected still has to be
                // answered (the operator is waiting on it) and must not cut
                // the backoff short, or a reload loop would become a connect
                // loop.
                let deadline = tokio::time::Instant::now() + delay;
                loop {
                    tokio::select! {
                        biased;
                        _ = stop.changed() => {
                            let _ = events.try_send(AgentEvent::Stopped);
                            return;
                        }
                        request = reload.recv() => {
                            if let Some(request) = request {
                                let outcome = apply_offline_reload(&state_tx, request.templates, user);
                                let _ = request.reply.send(outcome);
                            }
                        }
                        _ = tokio::time::sleep_until(deadline) => break,
                    }
                }
            }
        }
    }
}

enum SessionOutcome {
    GracefulStop,
    Ended(String),
}

/// One connect/serve attempt plus what the reconnect backoff needs to know
/// about it: whether the session ever reached the ready state and how long
/// it stayed up. A session that never got there must not reset the
/// exponential curve.
struct SessionAttempt {
    outcome: SessionOutcome,
    established: bool,
    uptime: Duration,
    /// True when the failure could plausibly be the *address's* fault.
    ///
    /// Failover keys on this: a local configuration defect (TLS material
    /// missing, unusable transport options) fails identically against every
    /// gateway, so walking the list would only add log noise and delay the one
    /// error the operator has to fix.
    address_related: bool,
    /// True when the gateway explicitly refused our credentials.
    ///
    /// Distinct from `address_related`: a rejection is not the address's fault
    /// (failover cannot help), and it is not an outage either — it is the one
    /// failure an operator must fix, so `loginFailExit` may stop on it.
    credentials_rejected: bool,
}

impl SessionAttempt {
    /// An attempt that never reached the gateway: the address may be the
    /// problem, so failover is allowed to advance.
    fn failed(reason: String) -> Self {
        Self {
            outcome: SessionOutcome::Ended(reason),
            established: false,
            uptime: Duration::ZERO,
            address_related: true,
            credentials_rejected: false,
        }
    }

    /// An attempt whose credentials the gateway refused (FRP `loginFailExit`).
    fn rejected(reason: String) -> Self {
        Self {
            // A rejection is not worth walking the address list for: the same
            // token is presented to every gateway.
            address_related: false,
            credentials_rejected: true,
            ..Self::failed(reason)
        }
    }

    /// An attempt that could not even be started, for a reason that has
    /// nothing to do with which gateway was chosen.
    fn misconfigured(reason: String) -> Self {
        Self {
            address_related: false,
            ..Self::failed(reason)
        }
    }
}

/// Route table the agent enforces locally: gateway-registered ids → local
/// targets plus the name index for gateway declarations (PRD §45: the
/// target never comes from the gateway).
struct RouteTable {
    by_id: HashMap<RouteId, Arc<TunnelRouteTemplate>>,
    by_name: HashMap<String, (RouteId, Arc<TunnelRouteTemplate>)>,
    /// Resolved shared secrets for secret routes (FRP `sk`).
    ///
    /// Read from the environment once, at startup, so a missing or too-short
    /// key fails immediately instead of at the first registration — and never
    /// touches a configuration file or a log line.
    secret_keys: HashMap<RouteId, String>,
}

fn build_route_table(templates: &[TunnelRouteTemplate], user: Option<&str>) -> RouteTable {
    let mut by_id = HashMap::new();
    let mut by_name = HashMap::new();
    let mut secret_keys = HashMap::new();
    for template in templates {
        // The same user-namespaced id the gateway will derive and check: two
        // teams may each serve a template called `web`, and the gateway→agent
        // data streams name the route by this id.
        let id = template.route_id_for_user(user);
        let target = template.target();
        let matcher = template.matcher();
        match (id, target, matcher) {
            (Ok(id), Ok(_target), Ok(_matcher)) => {
                match template.secret_key_from_env() {
                    Ok(Some(key)) => {
                        secret_keys.insert(id.clone(), key);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(
                            route = %template.name,
                            error = %error,
                            "secret route key is unavailable; route skipped"
                        );
                        continue;
                    }
                }
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
    RouteTable {
        by_id,
        by_name,
        secret_keys,
    }
}

async fn connect_session(
    options: &AgentRuntimeOptions,
    endpoint: &RemoteEndpoint,
    state_tx: &watch::Sender<Arc<RouteState>>,
    state_rx: &watch::Receiver<Arc<RouteState>>,
    reload: &mut mpsc::Receiver<ReloadRequest>,
    stop: &watch::Receiver<bool>,
    events: &mpsc::Sender<AgentEvent>,
) -> SessionAttempt {
    let transport = match QuicClientTransport::new(&options.tls, transport_options(options)) {
        Ok(transport) => transport,
        // Building the transport decides whether we *can* speak to a gateway at
        // all (TLS material, options); no other address would change that.
        Err(error) => return SessionAttempt::misconfigured(error.to_string()),
    };
    let server_name = tls::server_name_for(endpoint);
    // FRP `dnsServer`: when configured, the gateway's name is resolved through
    // that server and nowhere else. The TLS server name stays the operator's
    // name — only the address is substituted — so certificate verification and
    // pinning keep working exactly as before.
    let connected_endpoint = match resolve_endpoint(options, endpoint).await {
        Ok(resolved) => resolved,
        Err(error) => return SessionAttempt::failed(error),
    };
    let connection = match transport.connect(&connected_endpoint, &server_name).await {
        Ok(connection) => connection,
        Err(error) => return SessionAttempt::failed(error.to_string()),
    };
    options.metrics.record_connection_open();

    // Control stream: the agent's one inbound stream (PRD §21).
    let mut control = match connection.open_stream().await {
        Ok(control) => control,
        Err(error) => {
            options.metrics.record_error();
            options.metrics.record_connection_close();
            return SessionAttempt::failed(format!("control stream open failed: {error}"));
        }
    };
    // One scratch per session: a single read may slurp several control
    // frames, and leftovers must survive across message reads.
    let mut scratch = BytesMut::with_capacity(4096);

    let authenticated = match authenticate(options, &mut *control, &mut scratch).await {
        Ok(authenticated) => authenticated,
        Err(failure) => {
            options.metrics.record_connection_close();
            return if failure.credentials_rejected {
                SessionAttempt::rejected(failure.reason)
            } else {
                SessionAttempt::failed(failure.reason)
            };
        }
    };
    let session_id = authenticated.session_id.clone();
    let _ = events.try_send(AgentEvent::Connected {
        session_id: session_id.clone(),
    });

    // Registration works from the configuration as it stands at connect time;
    // the session then keeps it in sync through reloads.
    let state = state_rx.borrow().clone();
    let (ready, rejected, registered) =
        match register_routes(options, &state, &mut *control, &mut scratch, events).await {
            Ok(outcome) => outcome,
            Err(error) => {
                options.metrics.record_connection_close();
                return SessionAttempt::failed(error);
            }
        };
    let _ = events.try_send(AgentEvent::Ready {
        routes: ready,
        rejected,
    });

    let serve_started = std::time::Instant::now();
    let outcome = serve(
        options,
        state_tx,
        state_rx,
        registered,
        connection,
        control,
        scratch,
        reload,
        stop,
        events,
        authenticated.heartbeat_reauth,
    )
    .await;
    options.metrics.record_connection_close();
    SessionAttempt {
        uptime: serve_started.elapsed(),
        established: true,
        outcome,
        // Irrelevant for an established session (the cursor returns to the
        // primary regardless), and `true` is the honest default: this attempt
        // did reach an address.
        address_related: true,
        // It authenticated; whatever ended it came afterwards.
        credentials_rejected: false,
    }
}

/// What the gateway answered to our credential presentation (PRD §25).
struct AuthenticatedSession {
    /// Session identity the gateway assigned.
    session_id: String,
    /// True when the gateway requires the token on every heartbeat
    /// (`auth.additionalScopes = ["heartbeats"]`).
    heartbeat_reauth: bool,
}

/// The endpoint to actually connect to, with the host resolved through the
/// configured DNS server when there is one (FRP `dnsServer`).
///
/// Without a configured server the endpoint is returned untouched and the
/// transport resolves it as it always has. With one, an IP literal passes
/// through and a name is resolved against that server only: a failure is
/// reported as an address-related failure, so failover can move to the next
/// gateway address instead of retrying a name that will not resolve.
async fn resolve_endpoint(
    options: &AgentRuntimeOptions,
    endpoint: &RemoteEndpoint,
) -> std::result::Result<RemoteEndpoint, String> {
    let Some(configured) = options
        .network
        .dns_server
        .as_deref()
        .map(str::trim)
        .filter(|server| !server.is_empty())
    else {
        return Ok(endpoint.clone());
    };
    let server = crate::dns::DnsServer::parse(configured)?;
    let address = server
        .resolve(&endpoint.host)
        .await
        .map_err(|error| format!("dnsServer {}: {error}", server.address()))?;
    tracing::debug!(
        host = %endpoint.host,
        resolved = %address,
        server = %server.address(),
        "gateway address resolved through the configured DNS server"
    );
    Ok(RemoteEndpoint::new(address.to_string(), endpoint.port))
}

/// Why a handshake failed, and whether our credential was the cause.
///
/// The distinction is what `loginFailExit` acts on: a rejected credential is
/// an operator error that no amount of retrying fixes, while a timeout or a
/// dead gateway is an outage the agent is expected to ride out.
struct HandshakeFailure {
    reason: String,
    credentials_rejected: bool,
}

impl HandshakeFailure {
    fn other(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            credentials_rejected: false,
        }
    }

    fn rejected(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            credentials_rejected: true,
        }
    }
}

async fn authenticate(
    options: &AgentRuntimeOptions,
    control: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
) -> std::result::Result<AuthenticatedSession, HandshakeFailure> {
    let hello = ControlMessage::Hello(Hello {
        protocol_versions: vec![ProtocolVersion::v1()],
        device_id: options.device.id.to_string(),
        device_name: options.device.name.clone(),
        platform: options.device.platform.as_str().to_owned(),
        // The machine's claim about which user it serves; the gateway checks
        // the claim against the token rather than trusting it.
        user: options.device.user.clone(),
    });
    write_msg(control, &hello)
        .await
        .map_err(HandshakeFailure::other)?;
    write_msg(
        control,
        &ControlMessage::Authenticate(Authenticate {
            token: options.token.clone(),
        }),
    )
    .await
    .map_err(HandshakeFailure::other)?;
    let response = tokio::time::timeout(
        options.timeouts.handshake_duration(),
        read_msg(control, scratch),
    )
    .await
    .map_err(|_| HandshakeFailure::other("handshake timeout"))?;
    match response {
        Ok(ControlMessage::AuthResult(AuthResult {
            ok: true,
            session_id,
            required_scopes,
            ..
        })) => Ok(AuthenticatedSession {
            session_id: session_id
                .ok_or_else(|| HandshakeFailure::other("gateway omitted the session id"))?,
            // FRP `auth.additionalScopes`: the gateway advertises where it
            // wants the token re-presented, so one security decision is never
            // configured on two sides.
            heartbeat_reauth: required_scopes.is_some_and(|scopes| {
                scopes.iter().any(|scope| {
                    sdkwork_webserver_tunnel_core::AuthScope::parse(scope).is_ok_and(|scope| {
                        scope == sdkwork_webserver_tunnel_core::AuthScope::Heartbeats
                    })
                })
            }),
        }),
        Ok(ControlMessage::AuthResult(AuthResult {
            ok: false,
            error: Some(detail),
            ..
        })) => Err(HandshakeFailure::rejected(format!(
            "authentication rejected: {detail}"
        ))),
        // A rejection without a detail is still a rejection: the gateway
        // decided against this credential, and no retry changes that.
        Ok(ControlMessage::AuthResult(AuthResult { ok: false, .. })) => {
            Err(HandshakeFailure::rejected("authentication rejected"))
        }
        Ok(ControlMessage::Error(error))
            if error.code == sdkwork_webserver_tunnel_protocol::ErrorCode::AuthenticationFailed =>
        {
            Err(HandshakeFailure::rejected(format!(
                "gateway rejected the session: {}",
                error.message
            )))
        }
        Ok(ControlMessage::Error(error)) => Err(HandshakeFailure::other(format!(
            "gateway error during handshake: {:?} {}",
            error.code, error.message
        ))),
        Ok(other) => Err(HandshakeFailure::other(format!(
            "unexpected handshake reply: {other:?}"
        ))),
        Err(message) => Err(HandshakeFailure::other(message)),
    }
}

/// Outcome of one registration pass: what the operator sees, plus the wire
/// requests kept for the next reload's diff.
type RegistrationOutcome = (
    Vec<RouteReadiness>,
    Vec<RouteRejection>,
    HashMap<RouteId, RegisterRoute>,
);

async fn register_routes(
    options: &AgentRuntimeOptions,
    state: &RouteState,
    control: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
    events: &mpsc::Sender<AgentEvent>,
) -> std::result::Result<RegistrationOutcome, String> {
    let _ = events;
    let mut registered: HashMap<RouteId, RegisterRoute> = HashMap::new();
    let mut requests = 0_usize;
    for template in &state.templates {
        let Some((route_id, _config)) = state.table.by_name.get(&template.name) else {
            continue;
        };
        let request = registration_request(
            template,
            route_id,
            state.table.secret_keys.get(route_id).map(String::as_str),
        );
        write_msg(control, &ControlMessage::RegisterRoute(request.clone())).await?;
        registered.insert(route_id.clone(), request);
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
    Ok((ready, rejected, registered))
}

/// Steady state: heartbeat writes, control-message reads (acks, gateway
/// declarations), reload application, and the data-stream accept loop
/// (PRD §27, §28, §55).
#[allow(clippy::too_many_arguments)]
async fn serve(
    options: &AgentRuntimeOptions,
    state_tx: &watch::Sender<Arc<RouteState>>,
    state_rx: &watch::Receiver<Arc<RouteState>>,
    registered: HashMap<RouteId, RegisterRoute>,
    connection: Box<dyn TunnelConnection>,
    mut control: Box<dyn TunnelStream>,
    mut scratch: BytesMut,
    reload: &mut mpsc::Receiver<ReloadRequest>,
    stop: &watch::Receiver<bool>,
    events: &mpsc::Sender<AgentEvent>,
    heartbeat_reauth: bool,
) -> SessionOutcome {
    let mut stop = stop.clone();
    let connection: Arc<dyn TunnelConnection> = Arc::from(connection);
    let (session_stop_tx, mut session_stop_rx) = watch::channel(false);
    let data_task = {
        let state_rx = state_rx.clone();
        let metrics = options.metrics.clone();
        let timeouts = options.timeouts.clone();
        let connection = connection.clone();
        let mut session_stop = session_stop_rx.clone();
        // FRP `transport.dialServerLocalIP`, read once per session from the
        // configuration this session was started with.
        let dial_source = options.network.dial_local_addr;
        tokio::spawn(async move {
            data_accept_loop(
                &connection,
                &state_rx,
                &metrics,
                &timeouts,
                &mut session_stop,
                dial_source,
            )
            .await;
        })
    };
    let mut registered = registered;

    let heartbeat = Duration::from_secs(options.network.heartbeat_interval_secs.max(1));
    let mut ticker = tokio::time::interval(heartbeat);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick fires immediately; skip it so the first heartbeat goes
    // out one full interval after connect.
    ticker.tick().await;

    // Health probes run beside the session and report through this channel:
    // the control stream is owned by the loop below, and a probe task must
    // never write to it directly (two writers would interleave frames).
    let (health_tx, mut health_rx) = mpsc::channel::<RouteHealthReport>(HEALTH_REPORT_QUEUE);
    let mut probe_tasks = spawn_health_probes(
        state_rx,
        options,
        health_tx.clone(),
        session_stop_rx.clone(),
    );

    let outcome = loop {
        tokio::select! {
            biased;
            _ = stop.changed() => {
                break SessionOutcome::GracefulStop;
            }
            _ = session_stop_rx.changed() => {
                break SessionOutcome::Ended("local stream accept loop ended".to_owned());
            }
            request = reload.recv() => {
                let Some(request) = request else {
                    continue;
                };
                let outcome = apply_reload(
                    options,
                    state_tx,
                    &mut registered,
                    request.templates,
                    &mut *control,
                    &mut scratch,
                )
                .await;
                if outcome.is_ok() {
                    // Health probes follow the routes: a probe for a removed
                    // route would keep reporting a verdict nobody reads, and a
                    // probe for an added one would never start.
                    for task in probe_tasks.drain(..) {
                        task.abort();
                    }
                    probe_tasks = spawn_health_probes(
                        state_rx,
                        options,
                        health_tx.clone(),
                        session_stop_rx.clone(),
                    );
                }
                let _ = request.reply.send(outcome);
            }
            report = health_rx.recv() => {
                let Some(report) = report else {
                    continue;
                };
                // Best effort by design: a control stream that cannot take a
                // health verdict is already being torn down by the read arm,
                // and dropping one probe result is recoverable (the next probe
                // carries the current verdict).
                let write = async {
                    write_message(&mut *control, &ControlMessage::RouteHealth(report)).await
                };
                if let Err(error) = tokio::time::timeout(HEARTBEAT_WRITE_TIMEOUT, write).await {
                    tracing::debug!(%error, "route health report was not delivered");
                }
            }
            _ = ticker.tick() => {
                // A heartbeat frame is ~20 bytes; a write that outlives this
                // deadline means the control stream is flow-control stalled.
                // The session must end (and reconnect) instead of parking the
                // whole select loop — no reads, no route refresh, no local
                // proxy teardown — until the QUIC idle timeout notices.
                let heartbeat_write = async {
                    // FRP `auth.additionalScopes = ["heartBeats"]`: when the
                    // gateway asked for it, every heartbeat is preceded by a
                    // fresh proof that we still hold the token. The token is
                    // written to the control stream and nowhere else — no log
                    // line, no event, no error text.
                    if heartbeat_reauth {
                        write_message(
                            &mut *control,
                            &ControlMessage::Reauthenticate(Reauthenticate {
                                token: options.token.clone(),
                            }),
                        )
                        .await?;
                    }
                    write_message(&mut *control, &ControlMessage::Heartbeat).await
                };
                match tokio::time::timeout(HEARTBEAT_WRITE_TIMEOUT, heartbeat_write).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        break SessionOutcome::Ended(format!("heartbeat write failed: {error}"));
                    }
                    Err(_) => {
                        break SessionOutcome::Ended(
                            "heartbeat write timed out (control stream stalled)".to_owned(),
                        );
                    }
                }
            }
            message = read_message(&mut *control, &mut scratch) => {
                match message {
                    Ok(ControlMessage::HeartbeatAck) => {}
                    Ok(ControlMessage::DeclareRoute(declaration)) => {
                        if let Err(error) = handle_declaration(
                            options,
                            state_rx,
                            &mut registered,
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
    for task in probe_tasks {
        task.abort();
    }
    if matches!(outcome, SessionOutcome::GracefulStop) {
        let _ = events.try_send(AgentEvent::Disconnected {
            reason: "agent stopped".to_owned(),
        });
    }
    outcome
}

/// Applies a reload while disconnected: the configuration is published and
/// takes effect at the next connect.
fn apply_offline_reload(
    state_tx: &watch::Sender<Arc<RouteState>>,
    templates: Vec<TunnelRouteTemplate>,
    user: Option<&str>,
) -> std::result::Result<RouteReloadReport, String> {
    let state = RouteState::build(templates, user)?;
    let mut added: Vec<String> = state
        .templates
        .iter()
        .map(|template| template.name.clone())
        .collect();
    added.sort();
    let _ = state_tx.send(Arc::new(state));
    tracing::info!(
        routes = added.len(),
        "tunnel configuration reloaded while disconnected; it applies at the next connect"
    );
    Ok(RouteReloadReport {
        added,
        removed: Vec::new(),
        unchanged: Vec::new(),
        rejected: Vec::new(),
        applied_offline: true,
    })
}

/// Applies a reload on a live session: unregister what left, register what
/// arrived or changed, then switch the data plane (FRP `frpc reload` parity).
///
/// Ordering matters. Registrations are added before the table switches, so a
/// visitor that arrives immediately after a reload finds the route already
/// live; removals are unregistered before the table switches, so the gateway
/// stops selecting a route the agent is about to stop serving. Streams already
/// in flight are untouched.
async fn apply_reload(
    options: &AgentRuntimeOptions,
    state_tx: &watch::Sender<Arc<RouteState>>,
    registered: &mut HashMap<RouteId, RegisterRoute>,
    templates: Vec<TunnelRouteTemplate>,
    control: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
) -> std::result::Result<RouteReloadReport, String> {
    let user = options.device.user.clone();
    let state = RouteState::build(templates, user.as_deref())?;
    let timeouts = options.timeouts.clone();

    // What the new configuration wants on the wire.
    let mut desired: HashMap<RouteId, RegisterRoute> = HashMap::new();
    for template in &state.templates {
        let Some((route_id, _)) = state.table.by_name.get(&template.name) else {
            continue;
        };
        desired.insert(
            route_id.clone(),
            registration_request(
                template,
                route_id,
                state.table.secret_keys.get(route_id).map(String::as_str),
            ),
        );
    }

    // Removals first: a route the operator deleted must stop receiving traffic
    // before the agent stops serving it.
    let mut removed = Vec::new();
    for (route_id, request) in registered.iter() {
        if desired.contains_key(route_id) {
            continue;
        }
        write_msg(
            control,
            &ControlMessage::UnregisterRoute(sdkwork_webserver_tunnel_protocol::UnregisterRoute {
                route_id: route_id.to_string(),
            }),
        )
        .await?;
        // Report the operator-facing name, not the derived id: a reload report
        // is compared against the configuration file it came from.
        removed.push(request.name.clone());
    }
    for _ in 0..removed.len() {
        let reply = tokio::time::timeout(timeouts.handshake_duration(), read_msg(control, scratch))
            .await
            .map_err(|_| "route removal timed out during reload".to_owned())?;
        match reply {
            Ok(ControlMessage::UnregisterRouteResult(result)) => {
                if !result.ok {
                    // Not fatal: the gateway may have dropped the route with a
                    // previous session, and the local configuration is still
                    // the operator's intent.
                    tracing::debug!(
                        route = %result.route_id,
                        "reload: gateway reported no such route"
                    );
                }
            }
            Ok(_) => return Err("unexpected reply during route removal".to_owned()),
            Err(message) => return Err(message),
        }
    }

    // Then additions and changes.
    let mut added = Vec::new();
    let mut unchanged = Vec::new();
    let mut pending: Vec<(RouteId, RegisterRoute)> = Vec::new();
    for (route_id, request) in desired.iter() {
        match registered.get(route_id) {
            Some(existing) if existing == request => unchanged.push(request.name.clone()),
            _ => {
                write_msg(control, &ControlMessage::RegisterRoute(request.clone())).await?;
                pending.push((route_id.clone(), request.clone()));
                added.push(request.name.clone());
            }
        }
    }

    let mut accepted: HashMap<RouteId, RegisterRoute> = HashMap::new();
    let mut rejected = Vec::new();
    for (route_id, request) in pending {
        let reply = tokio::time::timeout(timeouts.handshake_duration(), read_msg(control, scratch))
            .await
            .map_err(|_| "route registration timed out during reload".to_owned())?;
        match reply {
            Ok(ControlMessage::RegisterRouteResult(result)) => {
                if result.ok {
                    accepted.insert(route_id, request);
                } else {
                    let error = result.error.unwrap_or_default();
                    tracing::warn!(route = %route_id, %error, "reload: route rejected");
                    rejected.push(RouteRejection {
                        route_id: route_id.to_string(),
                        error,
                    });
                    added.retain(|name| name != &request.name);
                }
            }
            Ok(_) => return Err("unexpected reply during route registration".to_owned()),
            Err(message) => return Err(message),
        }
    }

    // The session's bookkeeping becomes the new truth: what is live at the
    // gateway, not what the file asked for.
    registered.retain(|route_id, _| desired.contains_key(route_id));
    for (route_id, request) in accepted {
        registered.insert(route_id, request);
    }
    // A route the gateway refused is not resolvable either: keeping it in the
    // table would accept visitor streams for a route nobody can reach.
    let served: Vec<TunnelRouteTemplate> = state
        .templates
        .iter()
        .filter(|template| {
            state
                .table
                .by_name
                .get(&template.name)
                .is_some_and(|(route_id, _)| registered.contains_key(route_id))
        })
        .cloned()
        .collect();
    let served_state = RouteState::build(served, options.device.user.as_deref())?;

    removed.sort();
    added.sort();
    unchanged.sort();
    tracing::info!(
        added = added.len(),
        removed = removed.len(),
        unchanged = unchanged.len(),
        rejected = rejected.len(),
        "tunnel configuration reloaded"
    );
    let _ = state_tx.send(Arc::new(served_state));
    Ok(RouteReloadReport {
        added,
        removed,
        unchanged,
        rejected,
        applied_offline: false,
    })
}

/// Spawns one probe task per route that declares a health check (FRP
/// `healthCheck` parity).
///
/// A route without a probe is reported healthy exactly once, so a replica that
/// replaced an unhealthy one starts serving without waiting for a first
/// interval — and a route nobody probes never looks "down" by accident.
fn spawn_health_probes(
    state_rx: &watch::Receiver<Arc<RouteState>>,
    _options: &AgentRuntimeOptions,
    reports: mpsc::Sender<RouteHealthReport>,
    stop: watch::Receiver<bool>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut tasks = Vec::new();
    let state = state_rx.borrow().clone();
    for (route_id, template) in state.table.by_name.values() {
        let Some(config) = template.health_check.clone() else {
            continue;
        };
        let route_id = route_id.clone();
        let target = match template.target() {
            Ok(target) => target,
            Err(error) => {
                tracing::warn!(route = %route_id, %error, "route target is invalid; no probe");
                continue;
            }
        };
        let reports = reports.clone();
        let mut stop = stop.clone();
        let interval = Duration::from_secs(config.interval_seconds.max(1));
        tasks.push(tokio::spawn(async move {
            let mut consecutive_failures = 0_u32;
            let mut healthy = true;
            loop {
                tokio::select! {
                    biased;
                    _ = stop.changed() => return,
                    _ = tokio::time::sleep(interval) => {}
                }
                let probe_healthy = probe_route_target(&config, &target).await;
                if probe_healthy {
                    consecutive_failures = 0;
                } else {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                }
                // FRP semantics: `maxFailed` consecutive failures mark the
                // route down; a single success brings it straight back.
                let verdict = probe_healthy || consecutive_failures < config.max_failed.max(1);
                let changed = verdict != healthy;
                healthy = verdict;
                if changed {
                    tracing::info!(
                        route = %route_id,
                        healthy = verdict,
                        consecutive_failures,
                        "route health changed"
                    );
                }
                let report = RouteHealthReport {
                    route_id: route_id.to_string(),
                    healthy: verdict,
                    consecutive_failures,
                };
                // A full queue means the control stream is not draining; the
                // next report carries the current verdict anyway.
                if reports.try_send(report).is_err() {
                    tracing::debug!(route = %route_id, "health report dropped (queue full)");
                }
            }
        }));
    }
    tasks
}

/// One probe against the route's local target.
async fn probe_route_target(
    config: &sdkwork_webserver_tunnel_core::HealthCheckConfig,
    target: &TunnelTarget,
) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let timeout = Duration::from_secs(config.timeout_seconds.max(1));
    let TunnelTarget::Tcp(addr) = target else {
        // A datagram route cannot carry a probe (refused at validation) and a
        // unix socket target has no probe implementation in V1: report healthy
        // rather than black-holing a route nobody can check.
        return true;
    };
    let connect = tokio::net::TcpStream::connect(addr);
    let Ok(Ok(mut stream)) = tokio::time::timeout(timeout, connect).await else {
        return false;
    };
    if config.kind == sdkwork_webserver_tunnel_core::HealthCheckKind::Tcp {
        return true;
    }
    let request = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        config.request_path(),
        addr
    );
    let exchange = async {
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut head = Vec::with_capacity(256);
        let mut buffer = [0_u8; 256];
        while head.len() < 1024 {
            let read = stream.read(&mut buffer).await.ok()?;
            if read == 0 {
                break;
            }
            head.extend_from_slice(&buffer[..read]);
            if head.windows(2).any(|window| window == b"\r\n") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&head).to_string();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())?;
        Some((200..400).contains(&status))
    };
    match tokio::time::timeout(timeout, exchange).await {
        Ok(Some(healthy)) => healthy,
        _ => false,
    }
}

/// Gateway-initiated route declaration (PRD §35): activated only when it
/// matches a locally configured template, name and matcher both; the local
/// target always wins (PRD §45).
async fn handle_declaration(
    options: &AgentRuntimeOptions,
    state_rx: &watch::Receiver<Arc<RouteState>>,
    registered: &mut HashMap<RouteId, RegisterRoute>,
    control: &mut dyn TunnelStream,
    scratch: &mut BytesMut,
    declaration: &DeclareRoute,
) -> std::result::Result<(), String> {
    let state = state_rx.borrow().clone();
    let routes = state.table.clone();
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
        // A secret route is matched by its name, which the declaration
        // already carries (`by_name` lookup above): there is no second key.
        sdkwork_webserver_tunnel_core::TunnelProtocolKind::Stcp
        | sdkwork_webserver_tunnel_core::TunnelProtocolKind::Sudp => true,
    };
    if !matcher_matches {
        tracing::info!(
            declaration = %declaration.name,
            "gateway declaration does not match the local template matcher; ignored"
        );
        return Ok(());
    }
    // The declaration carries the operator's admission policy; the target
    // still comes from the local template (PRD §45 SSRF boundary). Fields the
    // gateway did not send leave the local template's policy in charge, so an
    // older control plane behaves exactly as before.
    let declared_auth = match declaration.auth.as_deref() {
        Some(label) => match AuthPolicy::from_wire_label(label) {
            Ok(auth) => Some(auth),
            Err(error) => {
                tracing::warn!(
                    declaration = %declaration.name,
                    %error,
                    "gateway declaration carries an unknown auth policy; ignored"
                );
                return Ok(());
            }
        },
        None => None,
    };
    // The declared PROXY-protocol version follows the same override rule as
    // the admission policy: sent means "use this", absent leaves the local
    // template in charge.
    let declared_proxy = match declaration.proxy_protocol.as_deref() {
        Some(label) => match ProxyProtocolVersion::parse(label) {
            Some(version) => Some(version),
            None => {
                tracing::warn!(
                    declaration = %declaration.name,
                    label,
                    "gateway declaration carries an unknown PROXY protocol version; ignored"
                );
                return Ok(());
            }
        },
        None => template.proxy_protocol,
    };
    let mut effective = TunnelRouteTemplate::clone(template);
    effective.proxy_protocol = declared_proxy;
    // The declared path claim follows the same rule: a declaration that names
    // locations replaces the template's, one that omits them keeps the local
    // claim (an older control plane sends nothing).
    if let Some(locations) = declaration.locations.clone() {
        effective.locations = locations;
    }
    // Same rule for the bandwidth ceiling: a declaration that names one wins,
    // an older control plane that sends none leaves the template's in charge.
    if let Some(limit) = declaration.bandwidth_limit_bytes_per_second {
        effective.bandwidth_limit_bytes_per_second = Some(limit);
    }
    if let Some(group) = declaration.load_balancer_group.clone() {
        effective.load_balancer_group = Some(group);
    }
    if let Some(health_check) = declaration.health_check.clone() {
        effective.health_check = Some(health_check);
    }
    effective.policy = Some(
        template
            .policy_or_default()
            .with_declared_auth(declared_auth, declaration.visitor_tokens.clone()),
    );
    if let Err(error) = effective.validate() {
        // A declaration that asks for an unusable policy (bearer on a raw
        // TCP/UDP route, or bearer with no tokens) is refused here instead of
        // being registered into permanent unreachability. It must not end the
        // session: the agent keeps serving every other route.
        tracing::warn!(
            declaration = %declaration.name,
            %error,
            "gateway declaration asks for an unusable route policy; ignored"
        );
        return Ok(());
    }
    let request = registration_request(
        &effective,
        route_id,
        routes.secret_keys.get(route_id).map(String::as_str),
    );
    write_msg(control, &ControlMessage::RegisterRoute(request.clone())).await?;
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
                // The declaration changed the route's live definition, so the
                // session's bookkeeping must follow: a later reload compares
                // against what is actually registered.
                if let Ok(id) = RouteId::parse(&route_id) {
                    registered.insert(id, request);
                }
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
    state_rx: &watch::Receiver<Arc<RouteState>>,
    metrics: &Arc<TunnelMetrics>,
    timeouts: &sdkwork_webserver_tunnel_core::TunnelTimeoutConfig,
    stop: &mut watch::Receiver<bool>,
    dial_source: Option<std::net::IpAddr>,
) {
    loop {
        tokio::select! {
            biased;
            _ = stop.wait_for(|stopped| *stopped) => break,
            accepted = connection.accept_stream() => {
                            match accepted {
                    Ok(stream) => {
                        // The route table is read per stream, not captured:
                        // that is what makes a reload visible to traffic
                        // without restarting the session.
                        let state = state_rx.borrow().clone();
                        let metrics = metrics.clone();
                        let timeouts = timeouts.clone();
                        tokio::spawn(async move {
                            handle_data_stream(
                                stream,
                                &state.table,
                                &metrics,
                                &timeouts,
                                dial_source,
                            )
                            .await;
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
    // Source address for this stream's target dial (FRP
    // `transport.dialServerLocalIP`); `None` lets the kernel choose.
    dial_source: Option<std::net::IpAddr>,
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
    // Datagram targets ride a framed packet pump instead of copy_bidirectional
    // — both `udp` (public port) and `sudp` (secret name) carry datagrams.
    if template.protocol.is_datagram() {
        let TunnelTarget::Udp(addr) = template.target().unwrap_or_else(|_| {
            TunnelTarget::parse_udp("127.0.0.1:1").expect("fallback target parses")
        }) else {
            tracing::debug!(route = %route_id, "datagram route does not carry a udp target");
            metrics.record_stream_close();
            return;
        };
        udp_pump(stream.as_mut(), addr, &route_id, metrics, timeouts).await;
        metrics.record_stream_close();
        return;
    }
    let target = template.target().unwrap_or_else(|_| {
        TunnelTarget::parse_tcp("127.0.0.1:1").expect("fallback target parses")
    });
    // A route that declares PROXY protocol prefixes the visitor's address to
    // the local connection (FRP `transport.proxyProtocolVersion` parity). The
    // address arrives in the stream header: the agent dials the target itself,
    // so the gateway is the only party that ever saw the visitor.
    let proxy = template
        .proxy_protocol
        .map(|version| (version, header.visitor));
    let mut local =
        match dial_local_target(&target, &route_id, metrics, timeouts, proxy, dial_source).await {
            Some(local) => local,
            None => {
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

/// One dialed local target: a TCP socket, or — on POSIX agents — a Unix
/// domain socket named by a `unix:/absolute/path` template target.
///
/// The two transports differ only in how they are dialed; both are byte
/// streams, so the relay itself stays transport-agnostic.
enum LocalTarget {
    Tcp(tokio::net::TcpStream),
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
}

impl tokio::io::AsyncRead for LocalTarget {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for LocalTarget {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
        }
    }
}

/// Dials a local target, optionally from a specific source address.
///
/// FRP `transport.dialServerLocalIP`: a multi-homed agent reaches the same
/// service on several networks, and the service or the firewall in front of it
/// decides by source address. Binding first is the only way to choose — letting
/// the kernel pick the route's egress address is usually right and sometimes
/// exactly wrong.
///
/// The address family decides the socket type: an IPv4 source cannot carry an
/// IPv6 dial, and the mismatch is reported as a dial error like any other.
async fn connect_from(
    source: Option<std::net::IpAddr>,
    target: std::net::SocketAddr,
) -> std::io::Result<tokio::net::TcpStream> {
    let Some(source) = source else {
        return tokio::net::TcpStream::connect(target).await;
    };
    if source.is_ipv4() != target.is_ipv4() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("source address {source} cannot reach {target}"),
        ));
    }
    let socket = match source {
        std::net::IpAddr::V4(_) => tokio::net::TcpSocket::new_v4()?,
        std::net::IpAddr::V6(_) => tokio::net::TcpSocket::new_v6()?,
    };
    // Port 0: the kernel picks the ephemeral port, we choose only the address.
    socket.bind(std::net::SocketAddr::new(source, 0))?;
    socket.connect(target).await
}

/// Dials the local target a route names, applying the platform conventions
/// each transport needs. `None` means the target is unreachable or
/// unsupported on this platform — already logged and counted.
///
/// `proxy` carries the route's PROXY-protocol request plus the visitor address
/// the gateway reported; when both are present the header is written to the
/// local connection before any application byte, which is what makes a
/// tunneled service see the real visitor.
async fn dial_local_target(
    target: &TunnelTarget,
    route_id: &sdkwork_webserver_tunnel_core::RouteId,
    metrics: &Arc<TunnelMetrics>,
    timeouts: &sdkwork_webserver_tunnel_core::TunnelTimeoutConfig,
    proxy: Option<(ProxyProtocolVersion, Option<VisitorAddress>)>,
    dial_source: Option<std::net::IpAddr>,
) -> Option<LocalTarget> {
    match target {
        TunnelTarget::Tcp(addr) => {
            let dial = tokio::time::timeout(
                timeouts.connect_duration(),
                connect_from(dial_source, *addr),
            )
            .await;
            let local = match dial {
                Ok(Ok(local)) => local,
                Ok(Err(error)) => {
                    metrics.record_error();
                    tracing::info!(route = %route_id, %addr, error = %error, "local target unreachable");
                    return None;
                }
                Err(_) => {
                    metrics.record_error();
                    tracing::info!(route = %route_id, %addr, "local target dial timed out");
                    return None;
                }
            };
            // Interactive protocols that ride a TCP route — SSH (the server
            // speaks first with its banner), telnet, RDP — send many small
            // writes and are latency-bound: with Nagle enabled a keystroke's
            // packet waits for the previous one to be ACKed, adding a round
            // trip per keystroke. The webserver's own upstream client disables
            // Nagle for the same reason (`upstream_client.rs`), so the tunnel
            // path follows the platform convention. Failing to set it costs
            // latency, not correctness.
            if let Err(error) = local.set_nodelay(true) {
                tracing::debug!(route = %route_id, error = %error, "could not disable Nagle on the local target");
            }
            let mut local = LocalTarget::Tcp(local);
            if let Some((version, visitor)) = proxy {
                write_proxy_header(&mut local, version, visitor, *addr, route_id).await;
            }
            Some(local)
        }
        #[cfg(unix)]
        TunnelTarget::UnixSocket(path) => {
            let dial = tokio::time::timeout(
                timeouts.connect_duration(),
                tokio::net::UnixStream::connect(path),
            )
            .await;
            match dial {
                Ok(Ok(local)) => Some(LocalTarget::Unix(local)),
                Ok(Err(error)) => {
                    metrics.record_error();
                    tracing::info!(
                        route = %route_id,
                        socket = %path.display(),
                        error = %error,
                        "local unix socket unreachable"
                    );
                    None
                }
                Err(_) => {
                    metrics.record_error();
                    tracing::info!(
                        route = %route_id,
                        socket = %path.display(),
                        "local unix socket dial timed out"
                    );
                    None
                }
            }
        }
        #[cfg(not(unix))]
        TunnelTarget::UnixSocket(path) => {
            metrics.record_error();
            tracing::warn!(
                route = %route_id,
                socket = %path.display(),
                "unix socket targets require a POSIX agent"
            );
            None
        }
        TunnelTarget::Udp(_) => {
            // Datagram targets ride `udp_pump`; reaching here means the route
            // and the stream disagreed, which the registration rejects.
            metrics.record_error();
            tracing::debug!(route = %route_id, "datagram target on a stream relay");
            None
        }
    }
}

/// String-error variant used by the session functions, whose outcomes feed
/// [`SessionOutcome::Ended`].

/// Writes the PROXY-protocol header toward the local target, or explains why
/// it cannot be written.
///
/// A missing visitor address is *not* an error: a gateway that predates the
/// stream-header field sends none. Inventing an address would be worse than
/// omitting the header — the local service would log and authorize a visitor
/// that does not exist — so the connection proceeds un-prefixed and the
/// operator sees a warning naming the route.
async fn write_proxy_header(
    local: &mut LocalTarget,
    version: ProxyProtocolVersion,
    visitor: Option<VisitorAddress>,
    destination: std::net::SocketAddr,
    route_id: &sdkwork_webserver_tunnel_core::RouteId,
) {
    let Some(visitor) = visitor else {
        tracing::warn!(
            route = %route_id,
            version = version.label(),
            "route declares PROXY protocol but the gateway sent no visitor address; relaying without the header"
        );
        return;
    };
    let source = std::net::SocketAddr::new(visitor.ip, visitor.port);
    let Some(header) = version.header(source, destination) else {
        tracing::warn!(
            route = %route_id,
            version = version.label(),
            %source,
            %destination,
            "PROXY protocol cannot describe this address pair; relaying without the header"
        );
        return;
    };
    if let Err(error) = local.write_all(&header).await {
        tracing::debug!(
            route = %route_id,
            error = %error,
            "could not write the PROXY protocol header to the local target"
        );
        return;
    }
    tracing::debug!(
        route = %route_id,
        version = version.label(),
        %source,
        %destination,
        "PROXY protocol header written to the local target"
    );
}

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
        let tunnel_read = sdkwork_webserver_tunnel_protocol::packet_frame::read_packet(
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

/// The wire protocol label for one route template.
fn protocol_label(protocol: sdkwork_webserver_tunnel_core::TunnelProtocolKind) -> &'static str {
    protocol.wire_label()
}

/// Builds the route-registration request for one agent route template.
///
/// Single builder so initial registration, hot re-registration and
/// gateway-declared routes always transmit an identical policy — including
/// the network allow-list, which is the only admission channel the raw TCP
/// and UDP planes can use, and the visitor-token policy, which the gateway
/// cannot enforce unless it travels with the registration (FRP
/// `httpUser`/`httpPassword` parity).
fn registration_request(
    template: &TunnelRouteTemplate,
    route_id: &RouteId,
    secret_key: Option<&str>,
) -> RegisterRoute {
    let policy = template.policy_or_default();
    RegisterRoute {
        route_id: route_id.to_string(),
        name: template.name.clone(),
        protocol: protocol_label(template.protocol).to_owned(),
        domain: template.domain.clone(),
        port: template.port,
        target: template.target.clone(),
        allow_public: policy.allow_public,
        allowed_ips: (!policy.allowed_ips.is_empty())
            .then(|| policy.allowed_ips.iter().map(ToString::to_string).collect()),
        auth: Some(policy.auth.wire_label().to_owned()),
        visitor_tokens: (!policy.visitor_tokens.is_empty()).then(|| policy.visitor_tokens.clone()),
        proxy_protocol: template
            .proxy_protocol
            .map(|version| version.label().to_owned()),
        locations: (!template.locations.is_empty()).then(|| template.locations.clone()),
        bandwidth_limit_bytes_per_second: template.bandwidth_limit_bytes_per_second,
        load_balancer_group: template.load_balancer_group.clone(),
        health_check: template.health_check.clone(),
        // The key rides the encrypted control plane only: the gateway needs it
        // to authenticate visitors, and it never appears in a listing or log.
        secret_key: secret_key.map(str::to_owned),
        // The gateway applies the rewrite (it is the HTTP-parsing hop), so the
        // policy has to travel with the registration. An empty policy is
        // omitted on the wire, keeping the message identical for routes that
        // rewrite nothing.
        //
        // The `Err` arm is unreachable rather than swallowed: `RouteState::build`
        // validates every template before a session can register anything, so a
        // template whose headers do not compile never reaches this function.
        headers: match template.header_rewrite() {
            Ok(rewrite) if !rewrite.is_empty() => Some(rewrite),
            Ok(_) | Err(_) => None,
        },
        // Operator metadata travels with the registration so the gateway can
        // list it and a console can group by it; the template validation that
        // already ran bounds it.
        annotations: (!template.annotations.is_empty()).then(|| template.annotations.clone()),
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
        // Same two knobs as the gateway side (FRP `quic.keepalivePeriod` and
        // the connection's stream ceiling): the agent used to hard-code 5s and
        // 512, which meant a deployment could not tune the connector's
        // connection behaviour at all.
        keep_alive_interval_ms: options
            .network
            .keep_alive_interval()
            .map(|interval| u32::try_from(interval.as_millis()).unwrap_or(u32::MAX)),
        max_concurrent_bidi_streams: options.network.max_concurrent_streams(512),
    }
}

/// A session that never reached the ready state (dial, handshake, or route
/// registration failure) never resets the backoff; a session that did must
/// stay up at least one initial-delay interval before its end counts as a
/// success, so a flapping ready-then-die session cannot pin the retry rate
/// at the floor either.
fn should_reset_backoff(established: bool, uptime: Duration, stable_after: Duration) -> bool {
    established && uptime >= stable_after
}

fn stable_uptime_threshold(
    network: &sdkwork_webserver_tunnel_core::TunnelNetworkConfig,
) -> Duration {
    Duration::from_secs(network.initial_backoff_secs.max(1))
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

    /// FRP `serverAddr` list behaviour: advance after a failed attempt, return
    /// to the primary after an established one, stay put with one address.
    #[test]
    fn the_failover_cursor_advances_only_when_an_attempt_never_connected() {
        // Three addresses: a dial/handshake failure walks the list in order
        // and wraps around, so a total outage keeps trying every gateway.
        assert_eq!(next_endpoint_index(0, 3, false), 1);
        assert_eq!(next_endpoint_index(1, 3, false), 2);
        assert_eq!(next_endpoint_index(2, 3, false), 0);
        // An established session that ended (restart, network blip, gateway
        // drain) sends the agent back to the primary.
        assert_eq!(next_endpoint_index(2, 3, true), 0);
        assert_eq!(next_endpoint_index(1, 3, true), 0);
        // One address: nothing to fail over to, whatever happened.
        assert_eq!(next_endpoint_index(0, 1, false), 0);
        assert_eq!(next_endpoint_index(0, 0, false), 0);
    }

    #[test]
    fn the_gateway_endpoints_helper_keeps_the_primary_first() {
        let options = AgentRuntimeOptions {
            endpoint: RemoteEndpoint::new("gw1.sdkwork.link", 8443),
            alternates: vec![
                RemoteEndpoint::new("gw2.sdkwork.link", 8443),
                RemoteEndpoint::new("gw3.sdkwork.link", 8443),
            ],
            login_fail_exit: false,
            tls: tls::ClientTlsOptions::default(),
            device: Device::new(
                sdkwork_webserver_tunnel_core::DeviceId::parse("dev_order").expect("id"),
                "order-test",
                sdkwork_webserver_tunnel_core::DevicePlatform::Linux,
            )
            .expect("device"),
            token: "t".to_owned(),
            routes: Vec::new(),
            network: TunnelConfig::disabled().network_or_default(),
            timeouts: TunnelConfig::disabled().timeout_or_default(),
            metrics: Arc::new(TunnelMetrics::new()),
        };
        let authorities: Vec<String> = options
            .gateway_endpoints()
            .iter()
            .map(RemoteEndpoint::authority)
            .collect();
        assert_eq!(
            authorities,
            vec![
                "gw1.sdkwork.link:8443".to_owned(),
                "gw2.sdkwork.link:8443".to_owned(),
                "gw3.sdkwork.link:8443".to_owned(),
            ]
        );
    }

    /// The connector's connection behaviour is configurable, not hard-coded:
    /// keep-alive and the stream ceiling come from `[tunnel.network]`, with
    /// the same derivation the gateway uses when they are unset.
    #[test]
    fn transport_options_follow_the_configured_network_tunables() {
        let base =
            |network: sdkwork_webserver_tunnel_core::TunnelNetworkConfig| AgentRuntimeOptions {
                endpoint: RemoteEndpoint::new("gw.sdkwork.link", 8443),
                alternates: Vec::new(),
                login_fail_exit: false,
                tls: tls::ClientTlsOptions::default(),
                device: Device::new(
                    sdkwork_webserver_tunnel_core::DeviceId::parse("dev_transport").expect("id"),
                    "transport-test",
                    sdkwork_webserver_tunnel_core::DevicePlatform::Linux,
                )
                .expect("device"),
                token: "t".to_owned(),
                routes: Vec::new(),
                network,
                timeouts: TunnelConfig::disabled().timeout_or_default(),
                metrics: Arc::new(TunnelMetrics::new()),
            };

        // Derived: probe at half the heartbeat cadence, protocol default
        // stream ceiling.
        let derived = transport_options(&base(
            sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default(),
        ));
        assert_eq!(derived.keep_alive_interval_ms, Some(5_000));
        assert_eq!(derived.max_concurrent_bidi_streams, 512);

        // Configured: both knobs follow the file, and `0` disables probing.
        let configured =
            transport_options(&base(sdkwork_webserver_tunnel_core::TunnelNetworkConfig {
                keep_alive_interval_secs: Some(2),
                max_concurrent_streams: Some(64),
                ..sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default()
            }));
        assert_eq!(configured.keep_alive_interval_ms, Some(2_000));
        assert_eq!(configured.max_concurrent_bidi_streams, 64);

        let no_probe =
            transport_options(&base(sdkwork_webserver_tunnel_core::TunnelNetworkConfig {
                keep_alive_interval_secs: Some(0),
                ..sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default()
            }));
        assert_eq!(no_probe.keep_alive_interval_ms, None);
    }

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
                proxy_protocol: None,
                locations: Vec::new(),
                ..TunnelRouteTemplate::default()
            },
            TunnelRouteTemplate {
                name: "broken".to_owned(),
                protocol: sdkwork_webserver_tunnel_core::TunnelProtocolKind::Http,
                domain: None,
                port: None,
                target: "127.0.0.1:3001".to_owned(),
                policy: None,
                proxy_protocol: None,
                locations: Vec::new(),
                ..TunnelRouteTemplate::default()
            },
        ];
        let table = build_route_table(&templates, None);
        assert_eq!(table.by_id.len(), 1);
        assert!(table.by_name.contains_key("web"));
        assert!(!table.by_name.contains_key("broken"));
    }

    #[test]
    fn failed_dial_never_resets_backoff() {
        // A gateway outage ends every attempt within milliseconds and
        // `established` stays false; the curve must keep climbing instead
        // of being pinned at the initial delay (PRD §27).
        let network = sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default();
        let stable_after = stable_uptime_threshold(&network);
        let dial_failure = SessionAttempt::failed("connect failed".to_owned());
        assert!(matches!(dial_failure.outcome, SessionOutcome::Ended(_)));
        // A dial failure is the address's problem; a local misconfiguration is
        // not, and must not walk the failover list.
        assert!(dial_failure.address_related);
        assert!(!SessionAttempt::misconfigured("no tls material".to_owned()).address_related);
        assert!(!should_reset_backoff(
            dial_failure.established,
            dial_failure.uptime,
            stable_after
        ));
        // Even a long-lived attempt that never reached ready must not
        // reset: only `established` sessions count as success.
        assert!(!should_reset_backoff(
            false,
            Duration::from_secs(3600),
            stable_after
        ));
    }

    #[test]
    fn established_session_resets_backoff_only_after_stable_uptime() {
        let network = sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default();
        let stable_after = stable_uptime_threshold(&network);
        // Ready-then-instantly-die sessions flap below the threshold and
        // must not reset the curve either.
        assert!(!should_reset_backoff(
            true,
            Duration::from_millis(50),
            stable_after
        ));
        // A session that served at least one initial-delay interval is a
        // real success and may reset.
        assert!(should_reset_backoff(true, stable_after, stable_after));
    }
}
