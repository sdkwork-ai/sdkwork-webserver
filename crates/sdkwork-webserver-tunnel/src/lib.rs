//! SDKWork Web Server tunnel composition layer.
//!
//! Assembles the tunnel building blocks into the two runtime roles (PRD §61,
//! §62):
//!
//! - [`gateway`] — the public edge: QUIC listener, authentication, session
//!   and route management, HTTP/TCP relay dispatch, and the REST-facing
//!   service facade.
//! - [`agent`] — the inside connector: dials the gateway, authenticates,
//!   registers routes, heartbeats, reconnects, and relays to local targets.
//!
//! Both roles expose a [`service::TunnelService`] facade so the webserver,
//! REST handlers, and the CLI share one model (PRD §80, §81).
//!
//! Dependency direction (PRD §43): `tunnel-core` ← `tunnel-protocol` ←
//! `tunnel-transport` ← this crate ← the webserver integration. QUIC and
//! TLS types stay inside the transport crate; HTTP semantics stay inside
//! the webserver.

pub mod agent;
pub mod cli;
pub mod gateway;
pub mod metrics;
pub mod security;
pub mod service;

pub use agent::{AgentEvent, AgentRuntime, AgentRuntimeOptions, RouteReadiness, RouteRejection};
pub use gateway::{
    GatewayEvent, GatewayShared, RelayVisitor, RelayedStream, TunnelGateway, TunnelGatewayOptions,
};
pub use metrics::{TunnelMetrics, TunnelMetricsSnapshot};
pub use security::{AuthRateLimiter, RouteAcl, TokenAuthenticator};
pub use service::{
    GatewayTunnelService, RouteRegistration, TunnelRole, TunnelService, TunnelStatus,
};

/// Test-only TLS material helpers shared by composition tests.
#[cfg(test)]
pub(crate) mod transport_test_utils {
    use sdkwork_webserver_tunnel_transport::tls;
    use std::sync::OnceLock;

    struct Cached {
        cert: Vec<u8>,
        key: Vec<u8>,
    }

    fn cached() -> &'static Cached {
        static CACHE: OnceLock<Cached> = OnceLock::new();
        CACHE.get_or_init(|| {
            let material =
                tls::generate_self_signed(&["localhost".to_owned(), "127.0.0.1".to_owned()])
                    .expect("self-signed material");
            Cached {
                cert: material.cert_pem.into_bytes(),
                key: material.key_pem.into_bytes(),
            }
        })
    }

    /// A throwaway self-signed certificate PEM.
    pub(crate) fn test_cert_pem() -> Vec<u8> {
        cached().cert.clone()
    }

    /// The matching private key PEM.
    pub(crate) fn test_key_pem() -> Vec<u8> {
        cached().key.clone()
    }
}
