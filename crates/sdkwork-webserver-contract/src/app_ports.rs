use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::dto::*;
use crate::problem::WebServiceResult;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebAppResourceScope {
    #[default]
    Owner,
    Tenant,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebAppRequestContext {
    pub tenant_id: i64,
    pub actor_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub resource_scope: WebAppResourceScope,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebBackendRequestContext {
    pub operator_id: Option<i64>,
    pub tenant_id: Option<i64>,
    /// Raw principal subject identifier (server UUID for agent-token routes, user_id string for dual-token).
    /// Present when the framework resolves a principal; absent for anonymous/public contexts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListApplicationsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    pub status: Option<i32>,
    #[serde(rename = "application_type")]
    pub application_type: Option<String>,
    #[serde(rename = "site_type")]
    pub site_type: Option<i32>,
    pub keyword: Option<String>,
}

/// Backend audit log list filters. `start_date`/`end_date` accept RFC 3339
/// instants or date-only `YYYY-MM-DD` (normalized at the service boundary);
/// `operator_id` is an int64 serialized as a string on the wire.
/// Cursor mode (`cursor` + `page_size`, keyset on `(created_at, id)`) is the
/// contract for this growing log table.
///
/// Fields are declared flat (no `#[serde(flatten)]`) because Axum's
/// `serde_urlencoded` Query extractor cannot reliably deserialize flattened
/// structs; flatten here surfaces as HTTP 40002 Malformed request.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListAuditLogsQuery {
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_i32"
    )]
    pub page_size: Option<i32>,
    #[serde(default, deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string")]
    pub cursor: Option<String>,
    #[serde(default, deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string")]
    pub target_type: Option<String>,
    #[serde(default, deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string")]
    pub action: Option<String>,
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::serde_int64::option_query::deserialize"
    )]
    pub operator_id: Option<i64>,
    #[serde(default, deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string")]
    pub start_date: Option<String>,
    #[serde(default, deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string")]
    pub end_date: Option<String>,
}

impl ListAuditLogsQuery {
    pub fn resolved_page_size(&self) -> i32 {
        self.page_size.unwrap_or(crate::dto::default_page_size())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListRootDomainsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    pub status: Option<i32>,
    pub keyword: Option<String>,
}

#[async_trait]
pub trait WebAppApi: Send + Sync {
    async fn list_applications(
        &self,
        context: &WebAppRequestContext,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<ApplicationPage>;

    async fn create_application(
        &self,
        context: &WebAppRequestContext,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn retrieve_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn update_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn delete_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<()>;

    async fn activate_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn pause_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn list_domains(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    /// Tenant-scoped verified hostname inventory for certificate issuance. Certificate
    /// issuance is independent of application routing, so the issuer selects from every
    /// owned hostname instead of one application's route.
    async fn list_certificate_domains(
        &self,
        context: &WebAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn retrieve_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse>;

    async fn delete_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn verify_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerifyResponse>;

    async fn list_source_versions(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _page: i32,
        _page_size: i32,
        _cursor: Option<&str>,
    ) -> WebServiceResult<SourceVersionPage> {
        Err(crate::WebServiceError::Internal(
            "source versions are unavailable".to_string(),
        ))
    }

    async fn create_source_version(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse> {
        Err(crate::WebServiceError::Internal(
            "source versions are unavailable".to_string(),
        ))
    }

    async fn import_git_source_version(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &ImportGitSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse> {
        Err(crate::WebServiceError::Internal(
            "Git source import is unavailable".to_string(),
        ))
    }

    async fn retrieve_source_version(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _source_version_id: &str,
    ) -> WebServiceResult<SourceVersionResponse> {
        Err(crate::WebServiceError::Internal(
            "source versions are unavailable".to_string(),
        ))
    }

    async fn list_deployments(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<DeploymentPage>;

    async fn create_deployment(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn retrieve_deployment(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn rollback_deployment(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn list_env_variables(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        environment: Option<&str>,
    ) -> WebServiceResult<EnvVariablePage>;

    async fn create_env_variable(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse>;

    async fn update_env_variable(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        variable_id: &str,
        request: &UpdateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse>;

    async fn delete_env_variable(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        variable_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_certificates(
        &self,
        context: &WebAppRequestContext,
        application_id: Option<&str>,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificatePage>;

    async fn issue_certificate(
        &self,
        context: &WebAppRequestContext,
        request: &IssueCertificateRequest,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse>;

    async fn retrieve_certificate_operation(
        &self,
        context: &WebAppRequestContext,
        operation_id: &str,
    ) -> WebServiceResult<CertificateOperationResponse>;

    async fn list_listener_certificate_bindings(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ListenerCertificateBindingPage>;

    async fn bind_listener_certificate(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<ListenerCertificateBindingResponse>;

    async fn unbind_listener_certificate(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
        binding_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_health_checks(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<HealthCheckPage>;

    async fn create_health_check(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> WebServiceResult<HealthCheckResponse>;

    async fn create_platform_target(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreatePlatformTargetRequest,
    ) -> WebServiceResult<PlatformTargetResponse>;

    async fn list_platform_targets(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<PlatformTargetPage>;

    async fn retrieve_platform_target(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        platform_target_id: &str,
    ) -> WebServiceResult<PlatformTargetResponse>;
}

#[async_trait]
pub trait WebBackendApi: Send + Sync {
    async fn list_applications(
        &self,
        context: &WebBackendRequestContext,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<ApplicationPage>;

    async fn create_application(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn retrieve_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn update_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn delete_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<()>;

    async fn activate_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn pause_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn list_application_domains(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_application_domain(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn verify_application_domain(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerifyResponse>;

    async fn delete_application_domain(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_root_domains(
        &self,
        context: &WebBackendRequestContext,
        query: &ListRootDomainsQuery,
    ) -> WebServiceResult<RootDomainPage>;

    async fn create_root_domain(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateRootDomainRequest,
    ) -> WebServiceResult<RootDomainResponse>;

    async fn retrieve_root_domain(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
    ) -> WebServiceResult<RootDomainResponse>;

    async fn delete_root_domain(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_root_domain_hostnames(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_root_domain_hostname(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
        request: &CreateRootDomainHostnameRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn list_managed_domains(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_managed_domain(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateManagedDomainRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn delete_managed_domain(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn verify_managed_domain(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerifyResponse>;

    async fn update_domain_application_binding(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
        request: &UpdateDomainApplicationBindingRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn delete_domain_application_binding(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_application_source_versions(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<SourceVersionPage>;

    async fn create_application_source_version(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse>;

    async fn import_application_git_source_version(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &ImportGitSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse>;

    async fn retrieve_application_source_version(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        source_version_id: &str,
    ) -> WebServiceResult<SourceVersionResponse>;

    async fn list_application_deployments(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<DeploymentPage>;

    async fn create_application_deployment(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn rollback_application_deployment(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn list_managed_certificates(
        &self,
        context: &WebBackendRequestContext,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificatePage>;

    async fn issue_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        request: &IssueCertificateRequest,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse>;

    async fn retrieve_managed_certificate_operation(
        &self,
        context: &WebBackendRequestContext,
        operation_id: &str,
    ) -> WebServiceResult<CertificateOperationResponse>;

    async fn update_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
        request: &UpdateCertificateRequest,
    ) -> WebServiceResult<CertificateResponse>;

    async fn delete_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
    ) -> WebServiceResult<()>;

    async fn renew_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse>;

    async fn revoke_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
        request: &RevokeCertificateRequest,
    ) -> WebServiceResult<CertificateResponse>;

    async fn list_application_listener_certificate_bindings(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ListenerCertificateBindingPage>;

    async fn bind_application_listener_certificate(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<ListenerCertificateBindingResponse>;

    async fn unbind_application_listener_certificate(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
        binding_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_certificate_distribution(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificateDistributionPage>;

    async fn list_nginx_configs(
        &self,
        context: &WebBackendRequestContext,
        query: &ListNginxConfigsQuery,
    ) -> WebServiceResult<NginxConfigPage>;

    async fn create_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn retrieve_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn update_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn validate_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
    ) -> WebServiceResult<NginxValidateResponse>;

    async fn web_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn reload_nginx(
        &self,
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<NginxReloadResponse>;

    async fn retrieve_nginx_status(
        &self,
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<NginxStatusResponse>;

    async fn list_servers(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ServerPage>;

    async fn create_server(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateServerRequest,
    ) -> WebServiceResult<CreateServerResponse>;

    async fn list_audit_logs(
        &self,
        context: &WebBackendRequestContext,
        query: &ListAuditLogsQuery,
    ) -> WebServiceResult<AuditLogPage>;
}

#[cfg(test)]
mod list_audit_logs_query_tests {
    use super::ListAuditLogsQuery;

    #[test]
    fn deserializes_empty_query_for_first_page() {
        let query: ListAuditLogsQuery = serde_urlencoded::from_str("").expect("empty query");
        assert_eq!(query.resolved_page_size(), 20);
        assert!(query.cursor.is_none());
    }

    #[test]
    fn deserializes_canonical_cursor_pagination_and_filters() {
        let query: ListAuditLogsQuery = serde_urlencoded::from_str(
            "page_size=20&cursor=opaque-token&target_type=site&action=create&operator_id=42&start_date=2024-01-01&end_date=2024-12-31",
        )
        .expect("canonical query");
        assert_eq!(query.resolved_page_size(), 20);
        assert_eq!(query.cursor.as_deref(), Some("opaque-token"));
        assert_eq!(query.target_type.as_deref(), Some("site"));
        assert_eq!(query.action.as_deref(), Some("create"));
        assert_eq!(query.operator_id, Some(42));
        assert_eq!(query.start_date.as_deref(), Some("2024-01-01"));
        assert_eq!(query.end_date.as_deref(), Some("2024-12-31"));
    }

    #[test]
    fn treats_blank_operator_id_as_absent() {
        let query: ListAuditLogsQuery =
            serde_urlencoded::from_str("operator_id=").expect("blank operator_id");
        assert!(query.operator_id.is_none());
    }

    #[test]
    fn ignores_unsupported_page_parameter() {
        let query: ListAuditLogsQuery =
            serde_urlencoded::from_str("page=1&page_size=20").expect("page is ignored at extract");
        assert_eq!(query.resolved_page_size(), 20);
    }
}
