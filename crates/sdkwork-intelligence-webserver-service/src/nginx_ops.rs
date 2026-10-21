//! Nginx deploy, validate, and reload orchestration through the edge runtime.

use sdkwork_webserver_contract::{WebServiceError, WebServiceResult};
use sdkwork_webserver_core::scan_nginx_directive_risks;

use crate::WebService;

impl WebService {
    /// Whether this node was configured with the nginx edge capability.
    ///
    /// The edge runtime treats a disabled capability as a successful no-op for
    /// the node-sync internal path (a Rust-data-plane node must finish a sync).
    /// The operator HTTP surface must not reuse that behaviour: reporting
    /// `valid: true` / `reloaded: true` / `running: true` without touching an
    /// edge is a false success (PRD §3, PRD-FR-020/021).
    pub(crate) fn nginx_edge_configured(&self) -> bool {
        self.edge_runtime.config().nginx_enabled
    }

    pub(crate) fn require_nginx_edge(&self) -> WebServiceResult<()> {
        if self.nginx_edge_configured() {
            return Ok(());
        }
        Err(WebServiceError::unavailable(
            "the nginx edge capability is disabled on this node \
             (SDKWORK_WEBSERVER_NGINX_ENABLED=false); validate, deploy, reload and status \
             cannot be reported as successful without an edge",
        ))
    }

    pub async fn validate_nginx_content(&self, content: &str) -> WebServiceResult<()> {
        self.require_nginx_edge()?;
        let risks = scan_nginx_directive_risks(content);
        if let Some(risk) = risks.first() {
            return Err(WebServiceError::validation(format!(
                "nginx configuration is not approved for activation: {risk}"
            )));
        }
        let runtime = self.edge_runtime.clone();
        let content = content.to_owned();
        tokio::task::spawn_blocking(move || runtime.validate_config_content(&content))
            .await
            .map_err(|error| WebServiceError::Internal(format!("join nginx validation: {error}")))?
            .map_err(|error| WebServiceError::validation(error.to_string()))
    }

    pub async fn deploy_nginx_site(&self, domain: &str, content: &str) -> WebServiceResult<()> {
        self.require_nginx_edge()?;
        let runtime = self.edge_runtime.clone();
        let domain = domain.to_owned();
        let content = content.to_owned();
        tokio::task::spawn_blocking(move || runtime.deploy_app_config(&domain, &content))
            .await
            .map_err(|error| WebServiceError::Internal(format!("join nginx deployment: {error}")))?
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }

    pub async fn reload_nginx_runtime(&self) -> WebServiceResult<()> {
        self.require_nginx_edge()?;
        let runtime = self.edge_runtime.clone();
        tokio::task::spawn_blocking(move || runtime.reload())
            .await
            .map_err(|error| WebServiceError::Internal(format!("join nginx reload: {error}")))?
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }

    /// Proves the running Nginx master actually serves the deployed revision
    /// (PRD-FR-020): `nginx -T` must contain the site's server-name fragment.
    /// A reload that failed validation keeps the previous revision serving,
    /// so this check fails instead of reporting a false success.
    pub async fn verify_nginx_served(&self, expected_fragment: &str) -> WebServiceResult<()> {
        self.require_nginx_edge()?;
        let runtime = self.edge_runtime.clone();
        let fragment = expected_fragment.to_owned();
        tokio::task::spawn_blocking(move || runtime.verify_served_config(&fragment))
            .await
            .map_err(|error| {
                WebServiceError::Internal(format!("join nginx served verification: {error}"))
            })?
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }
}
