//! Gateway TCP listener set for `tcp` routes (PRD §31).
//!
//! The control loop binds a listener inline (so port conflicts are
//! reported to the agent synchronously) and registers it here; this set
//! owns the per-port accept tasks and releases listeners when their last
//! route leaves. The set lives behind a `std` sync Mutex in
//! [`GatewayShared`]: every critical section is map manipulation plus
//! `tokio::spawn`, never an await.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::io::copy_bidirectional;
use tokio::net::TcpListener;

use sdkwork_webserver_tunnel_core::RouteId;

use super::{dispatch, GatewayShared};

#[derive(Default)]
pub(crate) struct TcpListenerSet {
    ports: HashMap<u16, PortState>,
    route_ports: HashMap<RouteId, u16>,
}

struct PortState {
    routes: HashSet<RouteId>,
    accept_task: tokio::task::JoinHandle<()>,
    /// Kept alive for as long as the port is served; dropping it stops
    /// accepting.
    _listener: Arc<TcpListener>,
}

impl TcpListenerSet {
    /// Registers a bound listener for `route_id` on `port`, spawning the
    /// accept loop on first use.
    pub(crate) fn serve(
        &mut self,
        shared: &Arc<GatewayShared>,
        port: u16,
        route_id: RouteId,
        listener: TcpListener,
    ) {
        self.route_ports.insert(route_id.clone(), port);
        match self.ports.get_mut(&port) {
            Some(state) => {
                state.routes.insert(route_id);
                // Should not happen (the registry admits one route per
                // port); drop the spare listener.
                drop(listener);
            }
            None => {
                let listener = Arc::new(listener);
                let accept_task = tokio::spawn(accept_loop(shared.clone(), port, listener.clone()));
                self.ports.insert(
                    port,
                    PortState {
                        routes: HashSet::from([route_id]),
                        accept_task,
                        _listener: listener,
                    },
                );
                tracing::info!(port, "tunnel tcp listener bound");
            }
        }
    }

    /// Releases one route's port claim, closing the listener when it was
    /// the last route on the port.
    pub(crate) fn release(&mut self, port: u16, route_id: &RouteId) {
        self.route_ports.remove(route_id);
        let Some(state) = self.ports.get_mut(&port) else {
            return;
        };
        state.routes.remove(route_id);
        if state.routes.is_empty() {
            if let Some(state) = self.ports.remove(&port) {
                state.accept_task.abort();
                drop(state._listener);
                tracing::info!(port, "tunnel tcp listener released");
            }
        }
    }

    /// Releases a route's claim when its port is unknown to the caller
    /// (session teardown).
    pub(crate) fn release_route(&mut self, route_id: &RouteId) {
        let Some(port) = self.route_ports.get(route_id).copied() else {
            return;
        };
        self.release(port, route_id);
    }

    /// Ports currently served (sorted, for diagnostics).
    pub(crate) fn served_ports(&self) -> Vec<u16> {
        let mut ports: Vec<u16> = self.ports.keys().copied().collect();
        ports.sort_unstable();
        ports
    }

    /// Shuts every listener down; called from gateway shutdown.
    pub(crate) fn shutdown(&mut self) {
        for (port, state) in self.ports.drain() {
            state.accept_task.abort();
            drop(state._listener);
            tracing::debug!(port, "tunnel tcp listener closed at shutdown");
        }
        self.route_ports.clear();
    }
}

async fn accept_loop(shared: Arc<GatewayShared>, port: u16, listener: Arc<TcpListener>) {
    loop {
        match listener.accept().await {
            Ok((downstream, peer)) => {
                let shared = shared.clone();
                tokio::spawn(async move {
                    relay(shared, port, downstream, peer).await;
                });
            }
            Err(error) => {
                tracing::warn!(port, error = %error, "tunnel tcp listener accept failed");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Relays one visitor TCP connection into the tunnel and back
/// (PRD §31: 公网 TCP → Gateway → tunnel stream → Agent → local target).
async fn relay(
    shared: Arc<GatewayShared>,
    port: u16,
    mut downstream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
) {
    match dispatch::connect_tcp_stream(&shared, port, peer.ip()).await {
        Ok(mut relayed) => {
            let outcome = copy_bidirectional(&mut downstream, &mut relayed).await;
            match outcome {
                Ok((visitor_bytes, agent_bytes)) => {
                    tracing::debug!(
                        %peer,
                        port,
                        visitor_bytes,
                        agent_bytes,
                        "tunnel tcp relay finished"
                    );
                }
                Err(error) => {
                    shared.metrics.record_error();
                    tracing::debug!(%peer, port, error = %error, "tunnel tcp relay failed");
                }
            }
        }
        Err(error) => {
            tracing::debug!(%peer, port, error = %error, "tunnel tcp relay refused");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_tracks_ports_and_routes() {
        let mut set = TcpListenerSet::default();
        let route = RouteId::parse("route_a").expect("valid id");
        // No listener required for bookkeeping-only assertions.
        set.route_ports.insert(route.clone(), 7000);
        assert!(set.served_ports().is_empty());
        set.release_route(&route);
        assert!(set.route_ports.is_empty());
    }
}
