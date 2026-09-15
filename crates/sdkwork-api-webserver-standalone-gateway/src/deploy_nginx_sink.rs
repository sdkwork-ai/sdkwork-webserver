//! Edge sink for `deploy_app.nginx_conf`.
//!
//! The Deploy control plane owns the app's nginx-compatible configuration
//! (`deploy_app.nginx_conf`, with an environment- or hostname-scoped
//! `deploy_nginx_config` override taking precedence) but never writes the
//! edge's site files. The data plane resolves the app, so it is the data
//! plane that hands the configuration to the edge through
//! [`NginxSiteSink`](crate::deploy_fallback::NginxSiteSink).
//!
//! This adapter materializes the configuration with
//! `sdkwork-webserver-edge-runtime`, the same plane the node agent uses for
//! its own deployment materials: the candidate is validated with `nginx -t`
//! and activated by an atomic rename under the edge deployment lock, so a
//! broken configuration can never replace a working one.
//!
//! The edge is configured from the environment exactly like the agent is
//! (`SDKWORK_WEBSERVER_NGINX_ENABLED`, `SDKWORK_WEBSERVER_NGINX_SITES_ROOT`,
//! …). When nginx materialization is disabled on this node the sink reports
//! itself unavailable and the resolver keeps the resolved configuration
//! cached and observable without touching the filesystem.

use std::sync::Arc;

use sdkwork_webserver_edge_runtime::deploy_nginx_config;

use crate::deploy_fallback::{NginxConfMaterial, NginxSiteSink};

/// [`NginxSiteSink`] backed by the edge node runtime.
pub struct EdgeNginxSiteSink {
    config: sdkwork_webserver_edge_runtime::EdgeRuntimeConfig,
}

impl EdgeNginxSiteSink {
    /// Build the sink from the node environment. Returns `None` when the node
    /// does not materialize nginx site files or its edge configuration is
    /// invalid; the app-domain fallback then serves normally and simply does
    /// not propagate `deploy_app.nginx_conf` to the edge.
    pub fn from_env() -> Option<Arc<dyn NginxSiteSink>> {
        let config = match sdkwork_webserver_edge_runtime::EdgeRuntimeConfig::from_env() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "app-domain fallback nginx site sink disabled: invalid edge runtime configuration"
                );
                return None;
            }
        };
        if !config.nginx_enabled {
            tracing::info!(
                "app-domain fallback nginx site sink disabled: edge nginx materialization is off"
            );
            return None;
        }
        tracing::info!(
            nginx_sites_root = %config.nginx_sites_root.display(),
            "app-domain fallback nginx site sink enabled"
        );
        Some(Arc::new(Self { config }))
    }
}

impl NginxSiteSink for EdgeNginxSiteSink {
    fn apply_site(&self, hostname: &str, material: &NginxConfMaterial) -> Result<(), String> {
        deploy_nginx_config(&self.config, hostname, &material.config).map_err(|error| {
            format!(
                "materialize nginx site configuration for {hostname} at {}: {error}",
                self.config.nginx_sites_root.display()
            )
        })
    }
}
