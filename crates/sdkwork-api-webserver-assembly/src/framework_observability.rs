use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_intelligence_webserver_service::{AuditLogWrite, WebService};
use sdkwork_web_core::{
    AuditEmitter, AuditFact, SecurityEvent, SecurityEventEmitter, SecurityEventKind,
    WebFrameworkError,
};

const MAX_AUDIT_PATH_BYTES: usize = 2_048;
const MAX_SECURITY_DETAIL_BYTES: usize = 1_024;

pub(crate) struct WebFrameworkAuditEmitter {
    service: Arc<WebService>,
}

impl WebFrameworkAuditEmitter {
    pub(crate) fn new(service: Arc<WebService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl AuditEmitter for WebFrameworkAuditEmitter {
    async fn emit(&self, fact: AuditFact) -> Result<(), WebFrameworkError> {
        // Audit is an observability side-channel and must never fail the
        // business response. A fact without a positive numeric tenant
        // subject (anonymous public endpoints such as login or password
        // reset) or with an unresolvable one (IAM-injection anomaly) is
        // skipped instead of persisted: an audit row must never carry a
        // fabricated tenant id of 0. Persistence failures are logged and
        // downgraded for the same reason.
        let Some(tenant_id) = numeric_subject_id(fact.tenant_id.as_deref()) else {
            tracing::debug!(
                request_id = %fact.request_id,
                operation_id = fact.operation_id.as_deref().unwrap_or("unknown"),
                "skipping audit fact without a positive numeric tenant subject"
            );
            return Ok(());
        };
        let action = fact.operation_id.as_deref().unwrap_or(&fact.method);
        let metadata_json = serde_json::to_string(&serde_json::json!({
            "apiSurface": &fact.api_surface,
            "method": &fact.method,
            "path": bounded_text(&fact.path, MAX_AUDIT_PATH_BYTES),
            "statusCode": fact.status_code,
            "durationMs": fact.duration_ms,
            "subjectId": fact.user_id.as_deref(),
        }))
        .map_err(|_| WebFrameworkError::dependency_unavailable("encode Web audit metadata"))?;
        if let Err(error) = self
            .service
            .record_audit_log(AuditLogWrite {
                tenant_id,
                organization_id: 0,
                operator_id: numeric_subject_id(fact.user_id.as_deref()).unwrap_or(0),
                operator_type: if fact.user_id.is_some() {
                    "USER"
                } else {
                    "SYSTEM"
                },
                action,
                target_type: "http_request",
                target_id: None,
                target_uuid: None,
                request_id: Some(&fact.request_id),
                metadata_json: &metadata_json,
            })
            .await
        {
            tracing::error!(
                request_id = %fact.request_id,
                error = %error,
                "Web audit persistence is unavailable; audit fact skipped"
            );
        }
        Ok(())
    }
}

pub(crate) struct WebFrameworkSecurityEventEmitter {
    service: Arc<WebService>,
}

impl WebFrameworkSecurityEventEmitter {
    pub(crate) fn new(service: Arc<WebService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl SecurityEventEmitter for WebFrameworkSecurityEventEmitter {
    async fn emit(&self, event: SecurityEvent) -> Result<(), WebFrameworkError> {
        // Same downgrade rule as the audit emitter: security events without
        // a positive numeric tenant subject (anonymous endpoints) are
        // skipped, and persistence failures are logged without failing the
        // business response.
        let Some(tenant_id) = numeric_subject_id(event.tenant_id.as_deref()) else {
            tracing::debug!(
                request_id = event.request_id.as_deref().unwrap_or("unknown"),
                kind = ?event.kind,
                "skipping security event without a positive numeric tenant subject"
            );
            return Ok(());
        };
        let metadata_json = serde_json::to_string(&serde_json::json!({
            "apiSurface": &event.api_surface,
            "method": &event.method,
            "path": bounded_text(&event.path, MAX_AUDIT_PATH_BYTES),
            "origin": event.origin.as_deref(),
            "detail": bounded_text(&event.detail, MAX_SECURITY_DETAIL_BYTES),
        }))
        .map_err(|_| WebFrameworkError::dependency_unavailable("encode Web security event"))?;

        if let Err(error) = self
            .service
            .record_audit_log(AuditLogWrite {
                tenant_id,
                organization_id: 0,
                operator_id: 0,
                operator_type: "SYSTEM",
                action: security_event_action(&event.kind),
                target_type: "security_event",
                target_id: None,
                target_uuid: None,
                request_id: event.request_id.as_deref(),
                metadata_json: &metadata_json,
            })
            .await
        {
            tracing::error!(
                request_id = event.request_id.as_deref().unwrap_or("unknown"),
                error = %error,
                "Web security-event persistence is unavailable; security event skipped"
            );
        }
        Ok(())
    }
}

fn numeric_subject_id(value: Option<&str>) -> Option<i64> {
    value
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
}

fn bounded_text(value: &str, maximum_bytes: usize) -> &str {
    if value.len() <= maximum_bytes {
        return value;
    }
    let mut end = maximum_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn security_event_action(kind: &SecurityEventKind) -> &'static str {
    match kind {
        SecurityEventKind::CorsDenied => "security.cors.denied",
        SecurityEventKind::RateLimitExceeded => "security.rate_limit.exceeded",
        SecurityEventKind::AuthenticationFailed => "security.authentication.failed",
        SecurityEventKind::AuthorizationDenied => "security.authorization.denied",
        SecurityEventKind::TenantIsolationDenied => "security.tenant_isolation.denied",
    }
}

#[cfg(test)]
mod tests {
    use super::{bounded_text, numeric_subject_id, security_event_action};
    use sdkwork_web_core::SecurityEventKind;

    #[test]
    fn audit_metadata_bounds_preserve_utf8() {
        let value = "application/部署/".repeat(300);
        let bounded = bounded_text(&value, 257);
        assert!(bounded.len() <= 257);
        assert!(value.starts_with(bounded));
    }

    #[test]
    fn opaque_subjects_do_not_overflow_numeric_audit_columns() {
        assert_eq!(numeric_subject_id(Some("42")), Some(42));
        assert_eq!(numeric_subject_id(Some("user-42")), None);
        assert_eq!(numeric_subject_id(Some("-1")), None);
        assert_eq!(numeric_subject_id(None), None);
    }

    #[test]
    fn security_event_actions_are_stable_and_bounded() {
        let action = security_event_action(&SecurityEventKind::TenantIsolationDenied);
        assert_eq!(action, "security.tenant_isolation.denied");
        assert!(action.len() <= 100);
    }
}
