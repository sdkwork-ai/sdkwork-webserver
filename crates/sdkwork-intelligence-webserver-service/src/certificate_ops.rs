//! Certificate command submission. Certificate issuance is executed by the worker.

use sdkwork_webserver_contract::{
    CertificateOperationAcceptedResponse, IssueCertificateRequest, WebAppRequestContext,
    WebServiceError, WebServiceResult,
};

use crate::{AuditLogWrite, WebService};

impl WebService {
    /// Two facts only the running engine can answer, so they are checked here
    /// rather than restated as shape rules the contract crate could only
    /// answer weakly: which cloud accounts this edge holds credentials for,
    /// and which CA directory it is configured to order from.
    ///
    /// Both refuse rather than fall back. Accepting an unknown account would
    /// mean the operator's choice was silently replaced by whichever account
    /// the registry prefers, and accepting the other CA profile would mean
    /// the certificate came from somewhere other than where the request said.
    /// The app port and the backend surface share this rule so a request
    /// cannot get past it by choosing the other door.
    pub(crate) fn validate_certificate_issue_edge_truth(
        &self,
        request: &IssueCertificateRequest,
    ) -> WebServiceResult<()> {
        if let Some(account_id) = request.provider_account_id.as_deref() {
            let known = self
                .certificate_issuer
                .dns_accounts()
                .iter()
                .any(|account| account.account_id == account_id);
            if !known {
                return Err(WebServiceError::validation(format!(
                    "providerAccountId `{account_id}` is not one of the cloud DNS accounts configured on this edge"
                )));
            }
        }
        if let Some(profile) = request.ca_profile.as_deref() {
            let configured = self.certificate_issuer.acme_profile();
            if profile != configured {
                return Err(WebServiceError::validation(format!(
                    "caProfile `{profile}` does not match this edge's ACME directory, which is configured for `{configured}`"
                )));
            }
        }
        Ok(())
    }

    pub async fn enqueue_certificate_issue(
        &self,
        context: &WebAppRequestContext,
        request: &IssueCertificateRequest,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse> {
        Self::validate_certificate_issue_request(request)?;
        self.validate_certificate_issue_edge_truth(request)?;
        if context.tenant_id <= 0 {
            return Err(WebServiceError::Forbidden);
        }
        let owner_id = Self::owner_filter(context)?;
        let operation = self
            .repository
            .enqueue_certificate_issue(
                context.tenant_id,
                owner_id,
                context.actor_id,
                request,
                context.idempotency_key.as_deref(),
            )
            .await?;
        self.audit_certificate_command(
            context.tenant_id,
            context.organization_id.unwrap_or(0),
            context.actor_id.unwrap_or(0),
            "USER",
            "certificates.issue.requested",
            &operation.operation_id,
        )
        .await;
        Ok(operation)
    }

    pub(crate) async fn audit_certificate_command(
        &self,
        tenant_id: i64,
        organization_id: i64,
        operator_id: i64,
        operator_type: &'static str,
        action: &'static str,
        operation_id: &str,
    ) {
        let _ = self
            .record_audit_log(AuditLogWrite {
                tenant_id,
                organization_id,
                operator_id,
                operator_type,
                action,
                target_type: "certificate_operation",
                target_id: None,
                target_uuid: Some(operation_id),
                request_id: None,
                metadata_json: "{}",
            })
            .await;
    }
}
