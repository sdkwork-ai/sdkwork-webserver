//! The [`TunnelService`] facade (PRD §41, §80, §81).
//!
//! One model for every control surface: the Rust API used by the webserver
//! runtime, the operations REST endpoints, and the CLI all call this trait
//! instead of reaching into gateway or agent internals.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sdkwork_webserver_tunnel_core::{
    DeviceId, Result, RouteId, SessionSnapshot, TunnelError, TunnelRoute, TunnelRouteTemplate,
};

use crate::gateway::GatewayShared;

/// 离线设备的待声明队列上限:每台设备最多排队多少条声明。
const MAXIMUM_PENDING_DECLARATIONS_PER_DEVICE: usize = 64;
/// 同时排队的离线设备台数上限。
///
/// 队列按设备键入且只在设备连接时排空,从未连接的设备会永久占位——没有
/// 上限就是认证后可触达的增长向量。已连接设备的声明直接下发,不经过这里。
const MAXIMUM_PENDING_DECLARATION_DEVICES: usize = 1024;

/// 把一条声明排入离线设备的待声明队列,返回是否被接受。
///
/// 双重上限:每设备队列封顶(防单设备刷爆)、设备数封顶(防大量离线设备
/// 各占一席)。拒绝即返回 `false`,调用方以资源上限错误回给操作者——
/// 与 PRD §106 的资源限额语义一致。
async fn queue_pending_declaration(
    pending: &tokio::sync::Mutex<HashMap<DeviceId, Vec<TunnelRouteTemplate>>>,
    device_id: &DeviceId,
    template: &TunnelRouteTemplate,
) -> bool {
    let mut pending = pending.lock().await;
    if !pending.contains_key(device_id) && pending.len() >= MAXIMUM_PENDING_DECLARATION_DEVICES {
        return false;
    }
    let queue = pending.entry(device_id.clone()).or_default();
    if queue.len() >= MAXIMUM_PENDING_DECLARATIONS_PER_DEVICE {
        return false;
    }
    queue.push(template.clone());
    true
}
use crate::metrics::TunnelMetricsSnapshot;

/// Role a service instance governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelRole {
    /// Public edge gateway.
    Gateway,
    /// Inside-network agent.
    Agent,
}

/// Aggregate tunnel status (PRD §82).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    /// Whether the tunnel capability is enabled.
    pub enabled: bool,
    /// Which role this service instance governs.
    pub role: TunnelRole,
    /// Live session count (gateway) or `0`/`1` semantics on the agent side.
    pub sessions: u32,
    /// Registered route count.
    pub routes: u32,
    /// Currently served TCP listener ports (gateway).
    pub ports: Vec<u16>,
    /// Metric snapshot.
    #[serde(flatten)]
    pub metrics: TunnelMetricsSnapshot,
}

/// Outcome of a route registration request through the service.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRegistration {
    /// Route identity (agent-side derived id).
    pub route_id: String,
    /// Whether the route is live right now.
    pub active: bool,
    /// Public URL for HTTP routes.
    pub public_url: Option<String>,
    /// Detail when the route could not be activated.
    pub error: Option<String>,
}

/// The unified tunnel control surface (PRD §41).
#[async_trait]
pub trait TunnelService: Send + Sync {
    /// Declares a route for a device. The target always comes from the
    /// device's own configuration: the gateway forwards the declaration and
    /// the agent activates it only when it matches a locally configured
    /// template (PRD §45 SSRF boundary).
    async fn create_route(
        &self,
        device_id: &DeviceId,
        template: &TunnelRouteTemplate,
    ) -> Result<RouteRegistration>;

    /// Removes a route (gateway: unregisters and releases its listener;
    /// agent: unregisters its own route).
    async fn remove_route(&self, id: &RouteId) -> Result<()>;

    /// Lists every route visible to this role.
    async fn list_routes(&self) -> Result<Vec<TunnelRoute>>;

    /// Lists live sessions (gateway) or the single local session (agent).
    async fn list_sessions(&self) -> Result<Vec<SessionSnapshot>>;

    /// Aggregate status.
    async fn status(&self) -> Result<TunnelStatus>;
}

/// [`TunnelService`] implementation over a running gateway.
pub struct GatewayTunnelService {
    /// Shared gateway state.
    pub shared: Arc<GatewayShared>,
}

#[async_trait]
impl TunnelService for GatewayTunnelService {
    async fn create_route(
        &self,
        device_id: &DeviceId,
        template: &TunnelRouteTemplate,
    ) -> Result<RouteRegistration> {
        template.validate()?;
        if !self.shared.sessions.is_connected(device_id) {
            let queued =
                queue_pending_declaration(&self.shared.pending_declarations, device_id, template)
                    .await;
            if !queued {
                return Err(TunnelError::ResourceLimit(
                    "pending declaration queue is full for this device",
                ));
            }
            return Ok(RouteRegistration {
                route_id: format!("route_{}", template.name),
                active: false,
                public_url: template
                    .domain
                    .as_ref()
                    .map(|domain| format!("https://{domain}")),
                error: Some("device is offline; declaration queued until it connects".to_owned()),
            });
        }
        // Device online: queue the declaration; the gateway control loop
        // flushes queued declarations on connect, and connected devices
        // receive them on their next control-plane interaction. The agent
        // activates only templates it recognizes, so a missed match simply
        // expires.
        let queued =
            queue_pending_declaration(&self.shared.pending_declarations, device_id, template).await;
        if !queued {
            return Err(TunnelError::ResourceLimit(
                "pending declaration queue is full for this device",
            ));
        }
        Ok(RouteRegistration {
            route_id: format!("route_{}", template.name),
            active: false,
            public_url: template
                .domain
                .as_ref()
                .map(|domain| format!("https://{domain}")),
            error: Some(
                "declaration queued; the agent activates it when it matches a local template"
                    .to_owned(),
            ),
        })
    }

    async fn remove_route(&self, id: &RouteId) -> Result<()> {
        let removed = self
            .shared
            .registry
            .unregister(id)
            .ok_or(TunnelError::RouteNotFound)?;
        if let Some(port) = removed.route.matcher.as_port() {
            self.shared
                .tcp
                .lock()
                .expect("tcp listener set lock is never held across awaits")
                .release(port, id);
        }
        self.shared.metrics.record_route_change(-1);
        Ok(())
    }

    async fn list_routes(&self) -> Result<Vec<TunnelRoute>> {
        Ok(self.shared.registry.list())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionSnapshot>> {
        Ok(self.shared.session_snapshots())
    }

    async fn status(&self) -> Result<TunnelStatus> {
        let metrics = self.shared.metrics.snapshot();
        Ok(TunnelStatus {
            enabled: true,
            role: TunnelRole::Gateway,
            sessions: u32::try_from(self.shared.sessions.session_count()).unwrap_or(u32::MAX),
            routes: u32::try_from(self.shared.registry.count()).unwrap_or(u32::MAX),
            ports: {
                let mut listeners = self
                    .shared
                    .tcp
                    .lock()
                    .expect("tcp listener set lock is never held across awaits")
                    .served_ports();
                let udp = self
                    .shared
                    .udp
                    .lock()
                    .expect("udp listener set lock is never held across awaits")
                    .served_ports();
                listeners.extend(udp);
                listeners.sort_unstable();
                listeners.dedup();
                listeners
            },
            metrics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn gateway_service_reports_empty_status() {
        use crate::gateway::{TunnelGateway, TunnelGatewayOptions};
        use crate::security::TokenAuthenticator;
        use crate::transport_test_utils::*;
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let options = TunnelGatewayOptions {
            bind: "127.0.0.1:0".parse().expect("bind"),
            cert_pem: test_cert_pem(),
            key_pem: test_key_pem(),
            authenticator: TokenAuthenticator::new(vec!["token".to_owned()]),
            tls_cert_pem_env: None,
            tls_key_pem_env: None,
            auth_max_failures: 10,
            auth_window_secs: 60,
            limits: Default::default(),
            timeouts: Default::default(),
            network: Default::default(),
            domain_suffixes: vec!["sdkwork.link".to_owned()],
            metrics: std::sync::Arc::new(crate::TunnelMetrics::new()),
        };
        let gateway = TunnelGateway::spawn(options, None)
            .await
            .expect("gateway spawns");
        let service = gateway.service();
        let status = service.status().await.expect("status");
        assert_eq!(status.role, TunnelRole::Gateway);
        assert_eq!(status.routes, 0);
        assert_eq!(status.sessions, 0);
        let routes = service.list_routes().await.expect("routes");
        assert!(routes.is_empty());
        gateway.shutdown().await;
    }
}
