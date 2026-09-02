use async_trait::async_trait;
use sdkwork_webserver_contract::{
    AuthenticatedMachineCredential, CreateRuntimeObservationRequest,
    MachineCredentialAuthenticator, PublishRuntimeAssignmentRequest, RuntimeAssignment,
    RuntimeAssignmentDelivery, RuntimeObservation, RuntimeObservationState, WebInternalApi,
    WebInternalRequestContext, WebServiceError, WebServiceResult,
};
use sdkwork_webserver_core::website_runtime::{
    compile_website_runtime_set_snapshot, WebsiteRuntimeEnvironment,
};

use crate::{AuditLogWrite, RuntimeAssignmentWrite, RuntimeObservationWrite, WebService};

const AGENT_TOKEN_PREFIX: &str = "wagent_";
const MAX_NODE_VERSION_BYTES: usize = 64;
const MAX_REASON_CODE_BYTES: usize = 64;
const MAX_OBSERVATION_DETAIL_BYTES: usize = 512;

#[async_trait]
impl MachineCredentialAuthenticator for WebService {
    async fn authenticate_machine_credential(
        &self,
        credential: &str,
    ) -> WebServiceResult<Option<AuthenticatedMachineCredential>> {
        if !credential.starts_with(AGENT_TOKEN_PREFIX) {
            return Ok(None);
        }
        let (subject_id, tenant_id) = self.try_authenticate_agent_token(credential).await?;
        Ok(Some(AuthenticatedMachineCredential {
            tenant_id,
            subject_id,
            app_id: "sdkwork-webserver-agent".to_owned(),
            permission_scope: vec!["web.agent.*".to_owned()],
        }))
    }
}

#[async_trait]
impl WebInternalApi for WebService {
    async fn publish_runtime_assignment(
        &self,
        context: &WebInternalRequestContext,
        node_uuid: &str,
        environment: &str,
        request: &PublishRuntimeAssignmentRequest,
    ) -> WebServiceResult<RuntimeAssignment> {
        validate_opaque_id(node_uuid, 64, "nodeUuid")?;
        let expected_environment = parse_environment(environment)?;
        let runtime_set_json = serde_json::to_string(&request.runtime_set).map_err(|error| {
            WebServiceError::Internal(format!("encode website runtime-set: {error}"))
        })?;
        let compiled = compile_website_runtime_set_snapshot(runtime_set_json.as_bytes())
            .map_err(|error| WebServiceError::validation(error.to_string()))?;
        if compiled.node_uuid() != node_uuid || compiled.environment() != expected_environment {
            return Err(WebServiceError::validation(
                "runtime-set nodeUuid and environment must match the assignment target",
            ));
        }

        let target = self
            .repository
            .resolve_runtime_assignment_target(
                context.tenant_id,
                context.can_publish_cross_tenant,
                node_uuid,
            )
            .await?;
        if !compiled.is_empty_or_single_tenant_scope(&target.tenant_scope_hash) {
            return Err(WebServiceError::Forbidden);
        }

        let assignment = self
            .repository
            .publish_runtime_assignment(RuntimeAssignmentWrite {
                tenant_id: target.tenant_id,
                server_id: target.server_id,
                node_uuid: target.node_uuid,
                environment: environment.to_owned(),
                generation: compiled.generation(),
                snapshot_uuid: compiled.snapshot_uuid().to_owned(),
                snapshot_sha256: compiled.snapshot_sha256().to_owned(),
                runtime_set_bytes: runtime_set_json.len(),
                runtime_set_json,
                assigned_by_subject: context.subject_id.clone(),
            })
            .await?;
        // Cross-tenant publication is an explicit exception on the internal
        // surface (permission + tenant-scope-hash gated): record a durable
        // audit marker so the event is attributable and observable.
        if target.tenant_id != context.tenant_id {
            let metadata = format!(
                "{{\"targetTenantId\":{},\"sourceTenantId\":{},\"subjectId\":{}}}",
                target.tenant_id,
                context.tenant_id,
                serde_json::to_string(&context.subject_id)
                    .unwrap_or_else(|_| "\"unknown\"".to_string())
            );
            let _ = self
                .record_audit_log(AuditLogWrite {
                    tenant_id: context.tenant_id,
                    organization_id: 0,
                    operator_id: 0,
                    operator_type: "MACHINE",
                    action: "web.runtime_assignment.publish_cross_tenant",
                    target_type: "node",
                    target_id: Some(target.server_id),
                    target_uuid: Some(node_uuid),
                    request_id: None,
                    metadata_json: &metadata,
                })
                .await;
        }
        Ok(assignment)
    }

    async fn retrieve_current_runtime_assignment(
        &self,
        context: &WebInternalRequestContext,
        environment: &str,
        if_generation: Option<&str>,
        if_snapshot_sha256: Option<&str>,
    ) -> WebServiceResult<RuntimeAssignmentDelivery> {
        parse_environment(environment)?;
        if let Some(value) = if_generation {
            parse_generation(value)?;
        }
        if let Some(value) = if_snapshot_sha256 {
            validate_sha256(value, "ifSnapshotSha256")?;
        }
        let node_uuid = context
            .agent_node_uuid
            .as_deref()
            .ok_or(WebServiceError::Forbidden)?;
        self.repository
            .retrieve_current_runtime_assignment(
                context.tenant_id,
                node_uuid,
                environment,
                if_generation,
                if_snapshot_sha256,
            )
            .await
    }

    async fn create_runtime_observation(
        &self,
        context: &WebInternalRequestContext,
        snapshot_uuid: &str,
        request: &CreateRuntimeObservationRequest,
    ) -> WebServiceResult<RuntimeObservation> {
        validate_opaque_id(snapshot_uuid, 128, "snapshotUuid")?;
        let node_uuid = context
            .agent_node_uuid
            .as_deref()
            .ok_or(WebServiceError::Forbidden)?;
        let generation = parse_generation(&request.generation)?;
        validate_sha256(&request.snapshot_sha256, "snapshotSha256")?;
        validate_optional_text(
            request.node_version.as_deref(),
            MAX_NODE_VERSION_BYTES,
            "nodeVersion",
        )?;
        validate_observation_reason(request)?;

        self.repository
            .create_runtime_observation(RuntimeObservationWrite {
                tenant_id: context.tenant_id,
                node_uuid: node_uuid.to_owned(),
                snapshot_uuid: snapshot_uuid.to_owned(),
                generation,
                snapshot_sha256: request.snapshot_sha256.clone(),
                state: request.state,
                node_version: request.node_version.clone(),
                reason_code: request.reason_code.clone(),
                detail: request.detail.clone(),
            })
            .await
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        context: &WebInternalRequestContext,
        snapshot_uuid: &str,
    ) -> WebServiceResult<RuntimeObservation> {
        validate_opaque_id(snapshot_uuid, 128, "snapshotUuid")?;
        self.repository
            .retrieve_latest_runtime_observation(
                context.tenant_id,
                context.can_publish_cross_tenant,
                snapshot_uuid,
            )
            .await
    }
}

fn parse_environment(value: &str) -> WebServiceResult<WebsiteRuntimeEnvironment> {
    match value {
        "development" => Ok(WebsiteRuntimeEnvironment::Development),
        "test" => Ok(WebsiteRuntimeEnvironment::Test),
        "staging" => Ok(WebsiteRuntimeEnvironment::Staging),
        "production" => Ok(WebsiteRuntimeEnvironment::Production),
        _ => Err(WebServiceError::validation(
            "unsupported runtime environment",
        )),
    }
}

fn parse_generation(value: &str) -> WebServiceResult<u64> {
    if value.is_empty()
        || value.len() > 16
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(WebServiceError::validation(
            "generation must be a positive JSON-safe decimal string",
        ));
    }
    let generation = value
        .parse::<u64>()
        .map_err(|_| WebServiceError::validation("generation is outside the supported range"))?;
    if generation == 0 || generation > 9_007_199_254_740_991 {
        return Err(WebServiceError::validation(
            "generation is outside the supported range",
        ));
    }
    Ok(generation)
}

fn validate_sha256(value: &str, field: &str) -> WebServiceResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(WebServiceError::validation(format!(
            "{field} must be a lowercase SHA-256 digest"
        )));
    }
    Ok(())
}

fn validate_opaque_id(value: &str, maximum: usize, field: &str) -> WebServiceResult<()> {
    if value.is_empty()
        || value.len() > maximum
        || !value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(WebServiceError::validation(format!(
            "{field} is not a bounded opaque identifier"
        )));
    }
    Ok(())
}

fn validate_optional_text(
    value: Option<&str>,
    maximum: usize,
    field: &str,
) -> WebServiceResult<()> {
    if value.is_some_and(|value| {
        value.is_empty()
            || value.len() > maximum
            || value.bytes().any(|byte| byte.is_ascii_control())
    }) {
        return Err(WebServiceError::validation(format!(
            "{field} must be non-empty, bounded, and control-free when supplied"
        )));
    }
    Ok(())
}

fn validate_observation_reason(request: &CreateRuntimeObservationRequest) -> WebServiceResult<()> {
    validate_optional_text(
        request.reason_code.as_deref(),
        MAX_REASON_CODE_BYTES,
        "reasonCode",
    )?;
    validate_optional_text(
        request.detail.as_deref(),
        MAX_OBSERVATION_DETAIL_BYTES,
        "detail",
    )?;
    if let Some(reason_code) = request.reason_code.as_deref() {
        if !reason_code.bytes().enumerate().all(|(index, byte)| {
            (index == 0 && byte.is_ascii_uppercase())
                || (index > 0
                    && (byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
        }) {
            return Err(WebServiceError::validation(
                "reasonCode must use upper snake case",
            ));
        }
    }
    match request.state {
        RuntimeObservationState::Rejected if request.reason_code.is_none() => Err(
            WebServiceError::validation("REJECTED observations require reasonCode"),
        ),
        RuntimeObservationState::Rejected => Ok(()),
        _ if request.reason_code.is_some() || request.detail.is_some() => {
            Err(WebServiceError::validation(
                "reasonCode and detail are allowed only for REJECTED observations",
            ))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_generation, validate_sha256};

    #[test]
    fn generation_parser_enforces_positive_json_safe_decimal() {
        assert_eq!(parse_generation("1").unwrap(), 1);
        assert_eq!(
            parse_generation("9007199254740991").unwrap(),
            9_007_199_254_740_991
        );
        for invalid in ["", "0", "01", "-1", "9007199254740992"] {
            assert!(parse_generation(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn sha256_parser_rejects_uppercase_and_non_hex() {
        validate_sha256(&"a".repeat(64), "hash").unwrap();
        assert!(validate_sha256(&"A".repeat(64), "hash").is_err());
        assert!(validate_sha256(&"z".repeat(64), "hash").is_err());
    }
}
