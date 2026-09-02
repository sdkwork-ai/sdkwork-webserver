//! SDKWork Web Node Daemon control-plane synchronization runtime.

#[path = "state.rs"]
mod state;

use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_backend_sdk::{
    AgentCertificateObservation as SdkAgentCertificateObservation,
    AgentHeartbeatRequest as SdkAgentHeartbeatRequest,
    AgentHeartbeatResponse as SdkAgentHeartbeatResponse, AgentSyncResponse as SdkAgentSyncResponse,
    SdkworkBackendClient, SdkworkConfig,
};
use sdkwork_webserver_contract::{
    AgentCertificateBundle, AgentCertificateObservation, AgentHeartbeatResponse,
    AgentNginxConfigBundle, AgentSyncResponse,
};
use sdkwork_webserver_edge_runtime::{
    CertificateBundleMaterial, EdgeRuntime, NginxSiteConfigMaterial, PendingEdgeDeployment,
};
use state::{resolve_state_path, NodeDaemonLock, NodeDaemonState};
use tracing::{info, warn};

const NODE_DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 30;
const MIN_SYNC_INTERVAL_SECS: u64 = 1;
const MAX_SYNC_INTERVAL_SECS: u64 = 3_600;
const HTTP_TIMEOUT_SECS: u64 = 60;
const MAX_HEARTBEAT_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_SYNC_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_NGINX_CONFIGS_PER_SYNC: usize = 2_048;
const MAX_CERTIFICATES_PER_SYNC: usize = 2_048;

struct NodeDaemonRuntimeConfig {
    control_plane: String,
    node_token: String,
    interval_secs: u64,
}

struct NodeDaemonSdkClients {
    heartbeat: SdkworkBackendClient,
    sync: SdkworkBackendClient,
}

impl NodeDaemonSdkClients {
    fn new(runtime: &NodeDaemonRuntimeConfig) -> anyhow::Result<Self> {
        Ok(Self {
            heartbeat: build_backend_sdk_client(runtime, MAX_HEARTBEAT_RESPONSE_BYTES)?,
            sync: build_backend_sdk_client(runtime, MAX_SYNC_RESPONSE_BYTES)?,
        })
    }
}

fn build_backend_sdk_client(
    runtime: &NodeDaemonRuntimeConfig,
    maximum_response_bytes: usize,
) -> anyhow::Result<SdkworkBackendClient> {
    let mut config = SdkworkConfig::new(runtime.control_plane.clone());
    config.timeout_ms = HTTP_TIMEOUT_SECS * 1_000;
    config.max_response_body_bytes = maximum_response_bytes;
    let client = SdkworkBackendClient::new(config)?;
    client.set_agent_token(runtime.node_token.clone());
    Ok(client)
}

impl NodeDaemonRuntimeConfig {
    fn from_env() -> anyhow::Result<Self> {
        let control_plane = parse_control_plane_url(
            &std::env::var("SDKWORK_WEBSERVER_CONTROL_PLANE_URL")
                .map_err(|_| anyhow::anyhow!("SDKWORK_WEBSERVER_CONTROL_PLANE_URL is required"))?,
        )?;
        let node_token = required_env_alias(
            "SDKWORK_WEBSERVER_NODE_TOKEN",
            "SDKWORK_WEBSERVER_AGENT_TOKEN",
        )?;
        validate_node_token(&node_token)?;
        let interval_secs = read_env_alias(
            "SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS",
            "SDKWORK_WEBSERVER_AGENT_SYNC_INTERVAL_SECS",
        )?
        .map(|value| parse_sync_interval(&value))
        .transpose()?
        .unwrap_or(DEFAULT_SYNC_INTERVAL_SECS);
        Ok(Self {
            control_plane,
            node_token,
            interval_secs,
        })
    }
}

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let edge = EdgeRuntime::from_env()?;
    let runtime = NodeDaemonRuntimeConfig::from_env()?;
    let state_path = resolve_state_path()?;
    let _node_daemon_lock = NodeDaemonLock::acquire(&state_path)?;
    let mut local_state = NodeDaemonState::load(&state_path)?;
    let clients = NodeDaemonSdkClients::new(&runtime)?;

    info!(
        interval_secs = runtime.interval_secs,
        nginx_enabled = edge.config().nginx_enabled,
        state_revision = local_state.revision(),
        state_pending = local_state.is_pending(),
        desired_sync_version = local_state.desired_sync_version(),
        observed_sync_version = local_state.observed_sync_version(),
        "sdkwork web node daemon started"
    );

    // Randomize the startup phase so a fleet of nodes does not synchronize in
    // lockstep against the control plane.
    tokio::time::sleep(Duration::from_millis(jitter_millis(
        runtime.interval_secs * 1_000,
    )))
    .await;

    let mut consecutive_failures: u32 = 0;
    loop {
        if let Err(error) = sync_once(&edge, &clients, &state_path, &mut local_state).await {
            warn!(error = %error, "node sync cycle failed");
            consecutive_failures = consecutive_failures.saturating_add(1);
        } else {
            consecutive_failures = 0;
        }
        // Exponential backoff on failure (capped at the configured interval) so
        // the control plane is not hammered while it is degraded; jitter spreads
        // retries across the fleet on recovery.
        let base_secs = if consecutive_failures > 0 {
            (1_u64 << consecutive_failures.min(6)).min(runtime.interval_secs.max(1))
        } else {
            runtime.interval_secs
        };
        let delay =
            Duration::from_secs(base_secs) + Duration::from_millis(jitter_millis(base_secs * 500));
        tokio::time::sleep(delay).await;
    }
}

/// Non-cryptographic millisecond jitter in `[0, max_ms)` derived from the clock
/// and process identity; used only to de-synchronize polling fleets.
fn jitter_millis(max_ms: u64) -> u64 {
    if max_ms == 0 {
        return 0;
    }
    let mut state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id()) << 32;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state % max_ms
}

async fn sync_once(
    edge: &EdgeRuntime,
    clients: &NodeDaemonSdkClients,
    state_path: &std::path::Path,
    local_state: &mut NodeDaemonState,
) -> anyhow::Result<()> {
    let heartbeat_ack = report_heartbeat(edge, clients, local_state).await?;

    let manifest = map_sync_response(
        clients
            .sync
            .agent()
            .retrieve(local_state.observed_sync_version())
            .await?,
    )?;
    validate_manifest_bounds(&manifest)?;
    if manifest.server_id != heartbeat_ack.server_id {
        anyhow::bail!("node identity mismatch between heartbeat acknowledgement and sync manifest");
    }

    if manifest.unchanged {
        if local_state.observed_sync_version() != Some(manifest.sync_version.as_str()) {
            anyhow::bail!(
                "control plane reported unchanged for a version the Web Node Daemon has not observed"
            );
        }
        info!(
            server_id = %manifest.server_id,
            sync_version = %manifest.sync_version,
            "node sync manifest unchanged"
        );
        return Ok(());
    }

    let desired_state = local_state.with_desired(&manifest.sync_version)?;
    desired_state.save(state_path)?;
    *local_state = desired_state;

    info!(
        server_id = %manifest.server_id,
        sync_version = %manifest.sync_version,
        nginx_configs = manifest.nginx_configs.len(),
        certificates = manifest.certificates.len(),
        "node sync manifest received"
    );

    let (nginx_configs, certificates) = deployment_materials(&manifest);
    let deployment = match edge
        .activate_deployment_async(&nginx_configs, &certificates)
        .await
    {
        Ok(deployment) => deployment,
        Err(error) => {
            let failure = record_deployment_failure(
                state_path,
                local_state,
                &manifest,
                "EDGE_DEPLOYMENT_STAGE_FAILED",
                edge,
                clients,
            )
            .await;
            return Err(append_error(
                anyhow::anyhow!("stage edge deployment: {error}"),
                "persist deployment failure",
                failure,
            ));
        }
    };

    if !manifest.certificates.is_empty() {
        if let Err(error) = persist_certificate_observations(
            state_path,
            local_state,
            certificate_observations(&manifest, "STAGED", None),
        ) {
            return Err(rollback_deployment(edge, deployment, false, error).await);
        }
        if let Err(error) = report_heartbeat(edge, clients, local_state).await {
            return Err(rollback_deployment(edge, deployment, false, error).await);
        }
    }

    if let Err(error) = edge.validate_active_config() {
        let failure = record_deployment_failure(
            state_path,
            local_state,
            &manifest,
            "NGINX_CONFIG_VALIDATION_FAILED",
            edge,
            clients,
        )
        .await;
        let error = append_error(
            anyhow::anyhow!("validate active Nginx configuration: {error}"),
            "persist deployment failure",
            failure,
        );
        return Err(rollback_deployment(edge, deployment, false, error).await);
    }
    if let Err(error) = edge.reload() {
        let failure = record_deployment_failure(
            state_path,
            local_state,
            &manifest,
            "NGINX_RELOAD_FAILED",
            edge,
            clients,
        )
        .await;
        let error = append_error(
            anyhow::anyhow!("reload Nginx after node sync: {error}"),
            "persist deployment failure",
            failure,
        );
        return Err(rollback_deployment(edge, deployment, true, error).await);
    }

    if !manifest.certificates.is_empty() {
        if let Err(error) = persist_certificate_observations(
            state_path,
            local_state,
            certificate_observations(&manifest, "ACTIVE", None),
        ) {
            return Err(rollback_deployment(edge, deployment, true, error).await);
        }
        if let Err(error) = report_heartbeat(edge, clients, local_state).await {
            return Err(rollback_deployment(edge, deployment, true, error).await);
        }

        for certificate in &manifest.certificates {
            for hostname in &certificate.hostnames {
                if let Err(error) = edge
                    .verify_served_certificate_async(hostname, &certificate.fingerprint)
                    .await
                {
                    let failure = record_deployment_failure(
                        state_path,
                        local_state,
                        &manifest,
                        "TLS_SNI_PROBE_FAILED",
                        edge,
                        clients,
                    )
                    .await;
                    let error = append_error(
                        anyhow::anyhow!(
                            "verify served certificate {} for {hostname}: {error}",
                            certificate.certificate_id
                        ),
                        "persist deployment failure",
                        failure,
                    );
                    return Err(rollback_deployment(edge, deployment, true, error).await);
                }
            }
        }

        if let Err(error) = persist_certificate_observations(
            state_path,
            local_state,
            certificate_observations(&manifest, "SERVED", None),
        ) {
            return Err(rollback_deployment(edge, deployment, true, error).await);
        }
    }

    deployment
        .commit()
        .await
        .map_err(|error| anyhow::anyhow!("commit edge deployment: {error}"))?;

    let observed_state = local_state.with_observed(&manifest.sync_version)?;
    observed_state.save(state_path)?;
    *local_state = observed_state;
    report_heartbeat(edge, clients, local_state).await?;

    Ok(())
}

async fn report_heartbeat(
    edge: &EdgeRuntime,
    clients: &NodeDaemonSdkClients,
    local_state: &NodeDaemonState,
) -> anyhow::Result<AgentHeartbeatResponse> {
    let heartbeat = SdkAgentHeartbeatRequest {
        agent_version: Some(NODE_DAEMON_VERSION.to_string()),
        nginx_enabled: Some(edge.config().nginx_enabled),
        active_configs: None,
        last_sync_version: local_state.observed_sync_version().map(str::to_string),
        certificate_observations: Some(
            local_state
                .certificate_observations()
                .iter()
                .map(|observation| SdkAgentCertificateObservation {
                    certificate_id: observation.certificate_id.clone(),
                    fingerprint: observation.fingerprint.clone(),
                    sync_version: observation.sync_version.clone(),
                    state: observation.state.clone(),
                    observed_at: observation.observed_at.clone(),
                    failure_code: observation.failure_code.clone(),
                })
                .collect(),
        ),
    };
    let acknowledgement =
        map_heartbeat_response(clients.heartbeat.agent().heartbeat(&heartbeat).await?)?;
    if acknowledgement.server_id.trim().is_empty() {
        anyhow::bail!("control-plane heartbeat acknowledgement has an empty serverId");
    }
    if acknowledgement.status != 1 {
        anyhow::bail!("control-plane heartbeat acknowledgement did not mark the node active");
    }
    Ok(acknowledgement)
}

async fn report_observation_failure(
    edge: &EdgeRuntime,
    clients: &NodeDaemonSdkClients,
    local_state: &NodeDaemonState,
) {
    if let Err(error) = report_heartbeat(edge, clients, local_state).await {
        warn!(error = %error, "report failed certificate observation");
    }
}

fn persist_certificate_observations(
    state_path: &std::path::Path,
    local_state: &mut NodeDaemonState,
    observations: Vec<AgentCertificateObservation>,
) -> anyhow::Result<()> {
    let next = local_state.with_certificate_observations(observations)?;
    next.save(state_path)?;
    *local_state = next;
    Ok(())
}

fn certificate_observations(
    manifest: &AgentSyncResponse,
    state: &str,
    failure_code: Option<&str>,
) -> Vec<AgentCertificateObservation> {
    let observed_at = Utc::now().to_rfc3339();
    manifest
        .certificates
        .iter()
        .map(|certificate| AgentCertificateObservation {
            certificate_id: certificate.certificate_id.clone(),
            fingerprint: certificate.fingerprint.clone(),
            sync_version: manifest.sync_version.clone(),
            state: state.to_string(),
            observed_at: observed_at.clone(),
            failure_code: failure_code.map(str::to_string),
        })
        .collect()
}

fn deployment_materials(
    manifest: &AgentSyncResponse,
) -> (Vec<NginxSiteConfigMaterial>, Vec<CertificateBundleMaterial>) {
    let nginx_configs = manifest
        .nginx_configs
        .iter()
        .map(|config| NginxSiteConfigMaterial {
            domain: config.domain.clone(),
            config_content: config.config_content.clone(),
        })
        .collect();
    let certificates = manifest
        .certificates
        .iter()
        .map(|certificate| CertificateBundleMaterial {
            bundle_name: certificate.cert_name.clone(),
            fullchain_pem: certificate.fullchain_pem.clone(),
            private_key_pem: certificate.privkey_pem.clone(),
        })
        .collect();
    (nginx_configs, certificates)
}

async fn record_deployment_failure(
    state_path: &std::path::Path,
    local_state: &mut NodeDaemonState,
    manifest: &AgentSyncResponse,
    failure_code: &str,
    edge: &EdgeRuntime,
    clients: &NodeDaemonSdkClients,
) -> Option<anyhow::Error> {
    match persist_certificate_observations(
        state_path,
        local_state,
        certificate_observations(manifest, "FAILED", Some(failure_code)),
    ) {
        Ok(()) => {
            report_observation_failure(edge, clients, local_state).await;
            None
        }
        Err(error) => Some(error),
    }
}

async fn rollback_deployment(
    edge: &EdgeRuntime,
    deployment: PendingEdgeDeployment,
    reload_previous: bool,
    primary: anyhow::Error,
) -> anyhow::Error {
    let rollback_error = deployment.rollback().await.err().map(anyhow::Error::from);
    let mut error = append_error(primary, "roll back edge deployment", rollback_error);
    if reload_previous {
        error = append_error(
            error,
            "reload restored Nginx configuration",
            edge.reload().err().map(anyhow::Error::from),
        );
    }
    error
}

fn append_error(
    primary: anyhow::Error,
    operation: &str,
    secondary: Option<anyhow::Error>,
) -> anyhow::Error {
    match secondary {
        Some(secondary) => anyhow::anyhow!("{primary}; {operation}: {secondary}"),
        None => primary,
    }
}

fn map_heartbeat_response(
    response: SdkAgentHeartbeatResponse,
) -> anyhow::Result<AgentHeartbeatResponse> {
    Ok(AgentHeartbeatResponse {
        server_id: response.server_id,
        status: i32::try_from(response.status)
            .map_err(|_| anyhow::anyhow!("heartbeat status is outside the i32 range"))?,
        acknowledged_at: response.acknowledged_at,
    })
}

fn map_sync_response(response: SdkAgentSyncResponse) -> anyhow::Result<AgentSyncResponse> {
    let nginx_configs = response
        .nginx_configs
        .into_iter()
        .map(|config| {
            Ok(AgentNginxConfigBundle {
                config_id: config.config_id,
                domain: config.domain,
                config_content: config.config_content,
                fingerprint: config.fingerprint,
                version: config
                    .version
                    .parse::<i64>()
                    .map_err(|error| anyhow::anyhow!("invalid node sync Nginx version: {error}"))?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let certificates = response
        .certificates
        .into_iter()
        .map(|certificate| AgentCertificateBundle {
            certificate_id: certificate.certificate_id,
            cert_name: certificate.cert_name,
            fingerprint: certificate.fingerprint,
            hostnames: certificate.hostnames,
            fullchain_pem: certificate.fullchain_pem,
            privkey_pem: certificate.privkey_pem,
        })
        .collect();
    Ok(AgentSyncResponse {
        server_id: response.server_id,
        sync_version: response.sync_version,
        unchanged: response.unchanged,
        nginx_configs,
        certificates,
    })
}

fn validate_manifest_bounds(manifest: &AgentSyncResponse) -> anyhow::Result<()> {
    if manifest.nginx_configs.len() > MAX_NGINX_CONFIGS_PER_SYNC {
        anyhow::bail!(
            "node sync contains more than {MAX_NGINX_CONFIGS_PER_SYNC} Nginx configurations"
        );
    }
    if manifest.certificates.len() > MAX_CERTIFICATES_PER_SYNC {
        anyhow::bail!("node sync contains more than {MAX_CERTIFICATES_PER_SYNC} certificates");
    }
    if manifest.unchanged
        && (!manifest.nginx_configs.is_empty() || !manifest.certificates.is_empty())
    {
        anyhow::bail!("unchanged node sync response must not contain deployment bundles");
    }
    let mut config_ids = HashSet::with_capacity(manifest.nginx_configs.len());
    let mut config_domains = HashSet::with_capacity(manifest.nginx_configs.len());
    for config in &manifest.nginx_configs {
        if config.version < 0 {
            anyhow::bail!("node sync contains a negative Nginx configuration version");
        }
        if !config_ids.insert(config.config_id.as_str()) {
            anyhow::bail!("node sync contains a duplicate Nginx configuration ID");
        }
        if !config_domains.insert(config.domain.to_ascii_lowercase()) {
            anyhow::bail!("node sync contains a duplicate Nginx activation domain");
        }
        let fingerprint = sha256_hash(config.config_content.as_bytes());
        if config.fingerprint != fingerprint {
            anyhow::bail!("node sync Nginx configuration fingerprint mismatch");
        }
    }
    let mut certificate_ids = HashSet::with_capacity(manifest.certificates.len());
    let mut certificate_names = HashSet::with_capacity(manifest.certificates.len());
    for certificate in &manifest.certificates {
        if !certificate_ids.insert(certificate.certificate_id.as_str()) {
            anyhow::bail!("node sync contains a duplicate certificate ID");
        }
        if !certificate_names.insert(certificate.cert_name.to_ascii_lowercase()) {
            anyhow::bail!("node sync contains a duplicate certificate activation name");
        }
        if certificate.fingerprint.len() != 64
            || !certificate
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            anyhow::bail!("node sync contains an invalid certificate fingerprint");
        }
        if certificate.hostnames.is_empty() || certificate.hostnames.len() > 128 {
            anyhow::bail!("node sync certificate verification hostname count is invalid");
        }
        let mut hostnames = HashSet::with_capacity(certificate.hostnames.len());
        for hostname in &certificate.hostnames {
            if hostname.is_empty()
                || hostname.len() > 253
                || !hostnames.insert(hostname.to_ascii_lowercase())
            {
                anyhow::bail!("node sync contains an invalid certificate verification hostname");
            }
        }
    }
    Ok(())
}

fn parse_control_plane_url(value: &str) -> anyhow::Result<String> {
    let url = url::Url::parse(value.trim())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        anyhow::bail!(
            "SDKWORK_WEBSERVER_CONTROL_PLANE_URL must be an HTTP(S) origin without credentials, path, query, or fragment"
        );
    }
    Ok(url.to_string())
}

fn validate_node_token(value: &str) -> anyhow::Result<()> {
    if !(16..=4_096).contains(&value.len()) || value.bytes().any(|byte| byte.is_ascii_control()) {
        anyhow::bail!("SDKWORK_WEBSERVER_NODE_TOKEN must contain 16..=4096 non-control bytes");
    }
    Ok(())
}

fn required_env_alias(preferred: &str, legacy: &str) -> anyhow::Result<String> {
    read_env_alias(preferred, legacy)?
        .ok_or_else(|| anyhow::anyhow!("{preferred} is required ({legacy} is a legacy alias)"))
}

fn read_env_alias(preferred: &str, legacy: &str) -> anyhow::Result<Option<String>> {
    let preferred_value = read_unicode_env(preferred)?;
    let legacy_value = read_unicode_env(legacy)?;
    resolve_alias_values(preferred, preferred_value, legacy, legacy_value)
}

fn read_unicode_env(name: &str) -> anyhow::Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("{name} must contain valid Unicode")
        }
    }
}

fn resolve_alias_values(
    preferred_name: &str,
    preferred_value: Option<String>,
    legacy_name: &str,
    legacy_value: Option<String>,
) -> anyhow::Result<Option<String>> {
    match (preferred_value, legacy_value) {
        (Some(preferred), Some(legacy)) if preferred != legacy => {
            anyhow::bail!("{preferred_name} conflicts with legacy alias {legacy_name}")
        }
        (Some(preferred), _) => Ok(Some(preferred)),
        (None, legacy) => Ok(legacy),
    }
}

fn parse_sync_interval(value: &str) -> anyhow::Result<u64> {
    let interval = value.parse::<u64>().map_err(|error| {
        anyhow::anyhow!("invalid SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS: {error}")
    })?;
    if !(MIN_SYNC_INTERVAL_SECS..=MAX_SYNC_INTERVAL_SECS).contains(&interval) {
        anyhow::bail!(
            "SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS must be between {MIN_SYNC_INTERVAL_SECS} and {MAX_SYNC_INTERVAL_SECS}"
        );
    }
    Ok(interval)
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use super::*;

    async fn serve_once(
        status: &str,
        content_length: usize,
        body: &'static [u8],
    ) -> (String, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind SDK mock server");
        let address = listener.local_addr().expect("SDK mock address");
        let (request_sender, request_receiver) = oneshot::channel();
        let status = status.to_string();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept SDK request");
            let mut request = vec![0_u8; 16 * 1024];
            let bytes_read = stream.read(&mut request).await.expect("read SDK request");
            request.truncate(bytes_read);
            let _ = request_sender.send(String::from_utf8_lossy(&request).to_string());
            let headers = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(headers.as_bytes())
                .await
                .expect("write SDK response headers");
            stream
                .write_all(body)
                .await
                .expect("write SDK response body");
            stream.shutdown().await.expect("close SDK response");
        });
        (format!("http://{address}/"), request_receiver)
    }

    fn heartbeat_request() -> SdkAgentHeartbeatRequest {
        SdkAgentHeartbeatRequest {
            agent_version: Some("0.1.0".to_string()),
            nginx_enabled: Some(true),
            active_configs: None,
            last_sync_version: None,
            certificate_observations: None,
        }
    }

    #[tokio::test]
    async fn generated_sdk_applies_agent_token_unwraps_envelope_and_enforces_body_limit() {
        let body = br#"{"code":0,"data":{"item":{"serverId":"server-1","status":1,"acknowledgedAt":"2026-07-20T00:00:00Z"}},"traceId":"trace-1"}"#;
        let (control_plane, request_receiver) = serve_once("200 OK", body.len(), body).await;
        let runtime = NodeDaemonRuntimeConfig {
            control_plane,
            node_token: "0123456789abcdef".to_string(),
            interval_secs: 30,
        };
        let client = build_backend_sdk_client(&runtime, MAX_HEARTBEAT_RESPONSE_BYTES)
            .expect("build generated backend SDK client");
        let response = client
            .agent()
            .heartbeat(&heartbeat_request())
            .await
            .expect("decode bounded SDKWork resource envelope");
        assert_eq!(response.server_id, "server-1");
        let request = request_receiver.await.expect("captured SDK request");
        assert!(request.starts_with("POST /backend/v3/api/agent/heartbeat HTTP/1.1"));
        assert!(request
            .to_ascii_lowercase()
            .contains("x-sdkwork-agent-token: 0123456789abcdef"));

        let (control_plane, _) = serve_once("200 OK", MAX_HEARTBEAT_RESPONSE_BYTES + 1, b"").await;
        let runtime = NodeDaemonRuntimeConfig {
            control_plane,
            node_token: "0123456789abcdef".to_string(),
            interval_secs: 30,
        };
        let client = build_backend_sdk_client(&runtime, MAX_HEARTBEAT_RESPONSE_BYTES)
            .expect("build bounded generated backend SDK client");
        let error = client
            .agent()
            .heartbeat(&heartbeat_request())
            .await
            .expect_err("oversized SDK response must fail closed");
        assert!(error
            .to_string()
            .contains("response body exceeds 65536 bytes"));
    }

    #[test]
    fn generated_sdk_models_map_to_the_domain_contract() {
        let decoded = map_sync_response(SdkAgentSyncResponse {
            server_id: "server-1".to_string(),
            sync_version: "sv1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            unchanged: true,
            nginx_configs: Vec::new(),
            certificates: Vec::new(),
        })
        .expect("generated SDK response mapping");
        assert_eq!(decoded.server_id, "server-1");
        assert!(decoded.unchanged);
    }

    #[test]
    fn runtime_inputs_are_strict_and_bounded() {
        assert!(parse_control_plane_url("https://control.sdkwork.com").is_ok());
        for invalid in [
            "file:///tmp/control",
            "https://user:secret@control.sdkwork.com",
            "https://control.sdkwork.com/backend",
            "https://control.sdkwork.com?tenant=1",
        ] {
            assert!(parse_control_plane_url(invalid).is_err(), "{invalid}");
        }
        assert!(validate_node_token("0123456789abcdef").is_ok());
        assert!(validate_node_token("short").is_err());
        assert!(parse_sync_interval("1").is_ok());
        assert!(parse_sync_interval("3600").is_ok());
        assert!(parse_sync_interval("0").is_err());
        assert!(parse_sync_interval("3601").is_err());
    }

    #[test]
    fn node_configuration_aliases_are_additive_and_fail_on_conflict() {
        assert_eq!(
            resolve_alias_values(
                "SDKWORK_WEBSERVER_NODE_TOKEN",
                Some("preferred".to_string()),
                "SDKWORK_WEBSERVER_AGENT_TOKEN",
                None,
            )
            .unwrap(),
            Some("preferred".to_string())
        );
        assert_eq!(
            resolve_alias_values(
                "SDKWORK_WEBSERVER_NODE_TOKEN",
                None,
                "SDKWORK_WEBSERVER_AGENT_TOKEN",
                Some("legacy".to_string()),
            )
            .unwrap(),
            Some("legacy".to_string())
        );
        assert!(resolve_alias_values(
            "SDKWORK_WEBSERVER_NODE_TOKEN",
            Some("left".to_string()),
            "SDKWORK_WEBSERVER_AGENT_TOKEN",
            Some("right".to_string()),
        )
        .is_err());
    }

    #[test]
    fn node_sync_manifest_rejects_duplicate_targets_and_bad_fingerprints() {
        let mut manifest = AgentSyncResponse {
            server_id: "node-1".to_string(),
            sync_version: "sv1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            unchanged: false,
            nginx_configs: vec![sdkwork_webserver_contract::AgentNginxConfigBundle {
                config_id: "config-1".to_string(),
                domain: "Example.com".to_string(),
                config_content: "server {}".to_string(),
                fingerprint: sha256_hash(b"server {}"),
                version: 1,
            }],
            certificates: Vec::new(),
        };
        validate_manifest_bounds(&manifest).unwrap();

        manifest.nginx_configs[0].fingerprint = "bad".to_string();
        assert!(validate_manifest_bounds(&manifest).is_err());
        manifest.nginx_configs[0].fingerprint = sha256_hash(b"server {}");
        let mut duplicate = manifest.nginx_configs[0].clone();
        duplicate.config_id = "config-2".to_string();
        duplicate.domain = "example.COM".to_string();
        manifest.nginx_configs.push(duplicate);
        assert!(validate_manifest_bounds(&manifest).is_err());
    }
}
