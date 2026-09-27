//! Web Node heartbeat and sync orchestration for the v3 Agent wire contract.

use chrono::{Duration, Utc};
use sdkwork_webserver_contract::{
    AgentCertificateObservation, AgentHeartbeatRequest, AgentHeartbeatResponse, AgentSyncResponse,
    WebServiceError, WebServiceResult,
};
use std::collections::HashSet;

use crate::WebService;

const MAX_NODE_SYNC_RESPONSE_BYTES: usize = 15 * 1024 * 1024;
const MAX_CERTIFICATE_OBSERVATIONS: usize = 2_048;
const MAX_OBSERVATION_CLOCK_SKEW_MINUTES: i64 = 5;

impl WebService {
    /// Authenticates an agent bootstrap token and returns `(server_uuid, tenant_id)`.
    ///
    /// Called by `MachineCredentialResolverDecorator` during framework authentication
    /// (C8-C9) to resolve `X-SDKWork-Agent-Token` into a `WebRequestPrincipal`.
    pub async fn try_authenticate_agent_token(
        &self,
        token: &str,
    ) -> WebServiceResult<(String, i64)> {
        self.repository.authenticate_agent_token(token).await
    }

    /// Records an edge-agent heartbeat after the framework has already authenticated the token
    /// and resolved `server_id` + `tenant_id` via `MachineCredentialResolverDecorator` (C8-C9).
    pub async fn agent_heartbeat(
        &self,
        server_id: &str,
        tenant_id: i64,
        request: &AgentHeartbeatRequest,
    ) -> WebServiceResult<AgentHeartbeatResponse> {
        validate_certificate_observations(&request.certificate_observations)?;
        self.repository
            .record_agent_heartbeat(server_id, tenant_id, request)
            .await
    }

    /// Builds the agent sync manifest after the framework has already authenticated the token
    /// and resolved `server_id` + `tenant_id` via `MachineCredentialResolverDecorator` (C8-C9).
    pub async fn agent_sync(
        &self,
        server_id: &str,
        tenant_id: i64,
        if_sync_version: Option<&str>,
    ) -> WebServiceResult<AgentSyncResponse> {
        let manifest = self
            .repository
            .build_agent_sync_manifest(server_id, tenant_id, if_sync_version)
            .await?;

        validate_node_sync_response_size(&manifest, MAX_NODE_SYNC_RESPONSE_BYTES)?;

        Ok(manifest)
    }
}

fn validate_certificate_observations(
    observations: &[AgentCertificateObservation],
) -> WebServiceResult<()> {
    if observations.len() > MAX_CERTIFICATE_OBSERVATIONS {
        return Err(WebServiceError::validation(format!(
            "certificateObservations must contain at most {MAX_CERTIFICATE_OBSERVATIONS} items"
        )));
    }
    let now = Utc::now();
    let latest = now + Duration::minutes(MAX_OBSERVATION_CLOCK_SKEW_MINUTES);
    let mut certificate_ids = HashSet::with_capacity(observations.len());
    for observation in observations {
        if !(1..=64).contains(&observation.certificate_id.len())
            || observation
                .certificate_id
                .bytes()
                .any(|byte| byte.is_ascii_control())
            || !certificate_ids.insert(observation.certificate_id.as_str())
        {
            return Err(WebServiceError::validation(
                "certificateObservations must contain unique bounded certificateId values",
            ));
        }
        validate_sha256(
            &observation.fingerprint,
            "certificate observation fingerprint",
        )?;
        validate_sync_version(&observation.sync_version)?;
        if !matches!(
            observation.state.as_str(),
            "STAGED" | "ACTIVE" | "SERVED" | "FAILED"
        ) {
            return Err(WebServiceError::validation(
                "certificate observation state is unsupported",
            ));
        }
        match (
            observation.state.as_str(),
            observation.failure_code.as_deref(),
        ) {
            ("FAILED", Some(code)) if valid_failure_code(code) => {}
            ("FAILED", _) => {
                return Err(WebServiceError::validation(
                    "FAILED certificate observations require a valid failureCode",
                ));
            }
            (_, None) => {}
            _ => {
                return Err(WebServiceError::validation(
                    "failureCode is only valid for FAILED certificate observations",
                ));
            }
        }
        let observed_at = chrono::DateTime::parse_from_rfc3339(&observation.observed_at)
            .map_err(|_| {
                WebServiceError::validation("certificate observation observedAt must be RFC 3339")
            })?
            .with_timezone(&Utc);
        if observed_at > latest {
            return Err(WebServiceError::validation(
                "certificate observation observedAt exceeds the accepted clock skew",
            ));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> WebServiceResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(WebServiceError::validation(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_sync_version(value: &str) -> WebServiceResult<()> {
    let digest = value.strip_prefix("sv1:").ok_or_else(|| {
        WebServiceError::validation("certificate observation syncVersion must use sv1")
    })?;
    validate_sha256(digest, "certificate observation syncVersion digest")
}

fn valid_failure_code(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_uppercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'_' | b'.' | b'-'))
        })
}

/// Measures the encoded manifest without materializing it. The framework
/// serializes the response exactly once on the way out; this check only needs
/// the byte count, so a full-size buffer (up to the cap itself) is never
/// allocated just to be thrown away.
fn validate_node_sync_response_size(
    manifest: &AgentSyncResponse,
    maximum_bytes: usize,
) -> WebServiceResult<()> {
    struct CountingWriter {
        count: usize,
    }
    impl std::io::Write for CountingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.count = self.count.saturating_add(buf.len());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = CountingWriter { count: 0 };
    serde_json::to_writer(&mut writer, manifest).map_err(|error| {
        WebServiceError::Internal(format!("encode node sync response: {error}"))
    })?;
    if writer.count > maximum_bytes {
        return Err(WebServiceError::Internal(format!(
            "node sync response exceeds {maximum_bytes} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use sdkwork_webserver_contract::{AgentCertificateObservation, AgentSyncResponse};

    use super::{validate_certificate_observations, validate_node_sync_response_size};

    #[test]
    fn node_sync_response_size_is_bounded_after_materialization() {
        let manifest = AgentSyncResponse {
            server_id: "node-1".to_string(),
            sync_version: "sv1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            unchanged: false,
            nginx_configs: Vec::new(),
            certificates: Vec::new(),
        };
        let encoded = serde_json::to_vec(&manifest).unwrap();

        validate_node_sync_response_size(&manifest, encoded.len()).unwrap();
        assert!(validate_node_sync_response_size(&manifest, encoded.len() - 1).is_err());
    }

    #[test]
    fn certificate_observations_are_current_bounded_and_state_specific() {
        let valid = AgentCertificateObservation {
            certificate_id: "certificate-1".to_string(),
            fingerprint: "a".repeat(64),
            sync_version: format!("sv1:{}", "b".repeat(64)),
            state: "SERVED".to_string(),
            observed_at: (Utc::now() - Duration::days(30)).to_rfc3339(),
            failure_code: None,
        };
        validate_certificate_observations(std::slice::from_ref(&valid)).unwrap();

        let mut duplicate = vec![valid.clone(), valid.clone()];
        assert!(validate_certificate_observations(&duplicate).is_err());

        duplicate.truncate(1);
        duplicate[0].state = "FAILED".to_string();
        assert!(validate_certificate_observations(&duplicate).is_err());
        duplicate[0].failure_code = Some("TLS_SNI_PROBE_FAILED".to_string());
        validate_certificate_observations(&duplicate).unwrap();

        duplicate[0].observed_at = (Utc::now() + Duration::minutes(10)).to_rfc3339();
        assert!(validate_certificate_observations(&duplicate).is_err());
    }
}
