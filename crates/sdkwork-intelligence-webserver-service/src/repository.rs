//! Repository port consumed by the Web service layer.

use async_trait::async_trait;
use sdkwork_webserver_contract::WebServiceResult;
use sdkwork_webserver_contract::{
    AgentHeartbeatRequest, AgentHeartbeatResponse, AgentSyncResponse, ApplicationPage,
    ApplicationResponse, AuditLogPage, CertificateDistributionPage, CertificateIssueUpdate,
    CertificateOperationAcceptedResponse, CertificateOperationLease, CertificateOperationResponse,
    CertificatePage, CertificateResponse, CreateApplicationRequest, CreateDeploymentRequest,
    CreateDomainRequest, CreateEnvVariableRequest, CreateHealthCheckRequest,
    CreateListenerCertificateBindingRequest, CreateManagedDomainRequest, CreateNginxConfigRequest,
    CreateRootDomainHostnameRequest, CreateRootDomainRequest, CreateServerRequest,
    CreateServerResponse, CreateSourceVersionRequest, DeploymentPage, DeploymentResponse,
    DomainPage, DomainResponse, EnvVariablePage, EnvVariableResponse, HealthCheckPage,
    HealthCheckResponse, IssueCertificateRequest, ListApplicationsQuery, ListAuditLogsQuery,
    ListNginxConfigsQuery, ListRootDomainsQuery, ListenerCertificateBindingPage,
    ListenerCertificateBindingResponse, NginxConfigPage, NginxConfigResponse, NginxStatusResponse,
    RevokeCertificateRequest, RootDomainPage, RootDomainResponse, RuntimeAssignment,
    RuntimeAssignmentDelivery, RuntimeObservation, RuntimeObservationState, ServerPage,
    SourceVersionPage, SourceVersionResponse, TlsCertificateAssignmentMaterial,
    UpdateApplicationRequest, UpdateDomainApplicationBindingRequest, UpdateEnvVariableRequest,
    UpdateNginxConfigRequest,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAssignmentTarget {
    pub server_id: i64,
    pub node_uuid: String,
    pub tenant_id: i64,
    pub tenant_scope_hash: String,
}

#[derive(Clone, Debug)]
pub struct CertificateRevocationMaterial {
    pub cert_type: i32,
    pub fullchain_pem: String,
}

#[derive(Clone, Debug)]
pub struct RuntimeAssignmentWrite {
    pub tenant_id: i64,
    pub server_id: i64,
    pub node_uuid: String,
    pub environment: String,
    pub generation: u64,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub runtime_set_json: String,
    pub runtime_set_bytes: usize,
    pub assigned_by_subject: String,
}

#[derive(Clone, Debug)]
pub struct RuntimeObservationWrite {
    pub tenant_id: i64,
    pub node_uuid: String,
    pub snapshot_uuid: String,
    pub generation: u64,
    pub snapshot_sha256: String,
    pub state: RuntimeObservationState,
    pub node_version: Option<String>,
    pub reason_code: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainVerificationChallenge {
    pub challenge_id: String,
    pub hostname: String,
    pub method: String,
    pub record_name: String,
    pub proof_sha256: String,
    pub status: String,
    pub attempt_count: i32,
    pub expires_at: String,
    pub next_attempt_at: Option<String>,
    pub checked_at: Option<String>,
    pub failure_code: Option<String>,
    pub ready_for_check: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainVerificationObservation {
    pub observed_sha256: Option<String>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct AuditLogWrite<'a> {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub operator_id: i64,
    pub operator_type: &'a str,
    pub action: &'a str,
    pub target_type: &'a str,
    pub target_id: Option<i64>,
    pub target_uuid: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub metadata_json: &'a str,
}

#[async_trait]
pub trait WebRepositoryPort: Send + Sync {
    async fn ready_check(&self) -> WebServiceResult<()>;

    async fn list_applications(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<ApplicationPage>;

    async fn create_application(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        owner_id: Option<i64>,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn retrieve_application(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn update_application(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn delete_application(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
    ) -> WebServiceResult<()>;

    async fn set_application_status(
        &self,
        tenant_id: i64,
        application_id: &str,
        status: i32,
    ) -> WebServiceResult<ApplicationResponse>;

    /// Resolves the backing site id for an application resource. The site is
    /// the internal carrier row; all child resources (domains, deployments,
    /// source versions, env variables, health checks) are site-scoped.
    async fn resolve_site_id(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<String>;

    async fn list_domains(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    /// Tenant-scoped domain inventory with optional owner filter, used by the
    /// application API to offer certificate issuance choices independently of
    /// application routing.
    async fn list_certificate_domains(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_domain(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn retrieve_domain(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse>;

    async fn delete_domain(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn prepare_domain_verification(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge>;

    async fn record_domain_verification_observation(
        &self,
        tenant_id: i64,
        challenge_id: &str,
        observation: &DomainVerificationObservation,
    ) -> WebServiceResult<DomainVerificationChallenge>;

    async fn list_root_domains(
        &self,
        tenant_id: i64,
        query: &ListRootDomainsQuery,
    ) -> WebServiceResult<RootDomainPage>;

    async fn create_root_domain(
        &self,
        tenant_id: i64,
        request: &CreateRootDomainRequest,
    ) -> WebServiceResult<RootDomainResponse>;

    async fn retrieve_root_domain(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<RootDomainResponse>;

    async fn delete_root_domain(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_root_domain_hostnames(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_root_domain_hostname(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        request: &CreateRootDomainHostnameRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn list_managed_domains(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_managed_domain(
        &self,
        tenant_id: i64,
        request: &CreateManagedDomainRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn delete_managed_domain(&self, tenant_id: i64, domain_id: &str) -> WebServiceResult<()>;

    async fn bind_managed_domain(
        &self,
        tenant_id: i64,
        domain_id: &str,
        request: &UpdateDomainApplicationBindingRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn unbind_managed_domain(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse>;

    async fn prepare_managed_domain_verification(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge>;

    async fn list_source_versions(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<SourceVersionPage>;

    async fn create_source_version(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
        retention_limit: i32,
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse>;

    async fn retrieve_source_version(
        &self,
        tenant_id: i64,
        application_id: &str,
        source_version_id: &str,
    ) -> WebServiceResult<SourceVersionResponse>;

    async fn list_deployments(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<DeploymentPage>;

    async fn create_deployment(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn retrieve_deployment(
        &self,
        tenant_id: i64,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn rollback_deployment(
        &self,
        tenant_id: i64,
        application_id: &str,
        deployment_id: &str,
        actor_id: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn list_env_variables(
        &self,
        tenant_id: i64,
        application_id: &str,
        environment: Option<&str>,
    ) -> WebServiceResult<EnvVariablePage>;

    async fn create_env_variable(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse>;

    async fn update_env_variable(
        &self,
        tenant_id: i64,
        application_id: &str,
        variable_id: &str,
        request: &UpdateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse>;

    async fn delete_env_variable(
        &self,
        tenant_id: i64,
        application_id: &str,
        variable_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_certificates(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        site_id: Option<&str>,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificatePage>;

    async fn enqueue_certificate_issue(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        requested_by: Option<i64>,
        request: &IssueCertificateRequest,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse>;

    async fn enqueue_certificate_renewal(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        requested_by: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse>;

    async fn retrieve_certificate_operation(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        operation_id: &str,
    ) -> WebServiceResult<CertificateOperationResponse>;

    async fn schedule_due_certificate_renewals(
        &self,
        renew_before_days: u32,
        limit: i32,
    ) -> WebServiceResult<usize>;

    async fn claim_certificate_operations(
        &self,
        lease_owner: &str,
        lease_seconds: i64,
        limit: i32,
    ) -> WebServiceResult<Vec<CertificateOperationLease>>;

    async fn renew_certificate_operation_lease(
        &self,
        lease: &CertificateOperationLease,
        lease_seconds: i64,
    ) -> WebServiceResult<()>;

    async fn delete_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        deleted_by: Option<i64>,
    ) -> WebServiceResult<()>;

    async fn list_listener_certificate_bindings(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ListenerCertificateBindingPage>;

    async fn bind_listener_certificate(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<ListenerCertificateBindingResponse>;

    async fn unbind_listener_certificate(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
        binding_id: &str,
    ) -> WebServiceResult<()>;

    async fn finalize_certificate_operation(
        &self,
        lease: &CertificateOperationLease,
        update: &CertificateIssueUpdate,
    ) -> WebServiceResult<CertificateResponse>;

    async fn fail_certificate_operation(
        &self,
        lease: &CertificateOperationLease,
        failure_code: &str,
        retry_at: &str,
        terminal_retry_at: &str,
    ) -> WebServiceResult<CertificateOperationResponse>;

    async fn update_certificate_auto_renew(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        auto_renew: bool,
    ) -> WebServiceResult<CertificateResponse>;

    /// Projects the decrypted TLS certificate material the node must serve
    /// into its self-hosted TLS runtime snapshot: every listener certificate
    /// binding on the node with a desired version, across all tenants the
    /// node serves.
    async fn load_node_tls_certificate_assignments(
        &self,
        node_uuid: &str,
    ) -> WebServiceResult<Vec<TlsCertificateAssignmentMaterial>>;

    /// Loads the revocation material (cert type and leaf chain) for a
    /// certificate, locking the aggregate so revocation is exclusive.
    async fn load_certificate_revocation_material(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> WebServiceResult<CertificateRevocationMaterial>;

    /// Marks a certificate revoked after the CA acknowledged the revocation:
    /// stops auto-renewal, archives listener bindings, and records the
    /// revocation metadata.
    async fn mark_certificate_revoked(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        request: &RevokeCertificateRequest,
        revoked_by: Option<i64>,
    ) -> WebServiceResult<CertificateResponse>;

    /// Records the CA-suggested ARI renewal window on the certificate
    /// aggregate so the due-renewal scheduler prefers it over the fixed
    /// `renew_before_days` fallback.
    async fn record_certificate_renewal_info(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        window_start: &str,
        window_end: &str,
    ) -> WebServiceResult<()>;

    async fn list_certificate_distribution(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificateDistributionPage>;

    async fn list_health_checks(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<HealthCheckPage>;

    async fn create_health_check(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> WebServiceResult<HealthCheckResponse>;

    async fn list_nginx_configs(
        &self,
        tenant_id: Option<i64>,
        query: &ListNginxConfigsQuery,
    ) -> WebServiceResult<NginxConfigPage>;

    async fn create_nginx_config(
        &self,
        tenant_id: i64,
        request: &CreateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn retrieve_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn update_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn load_nginx_config_content(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<String>;

    /// Loads the content of the currently active Nginx configuration for a
    /// site, if one exists. Used to roll the edge back when a new activation
    /// succeeds at the edge but fails in the control plane.
    async fn load_active_nginx_config_content(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<Option<String>>;

    async fn resolve_site_primary_hostname(
        &self,
        tenant_id: i64,
        site_uuid: &str,
    ) -> WebServiceResult<String>;

    async fn web_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn retrieve_nginx_status(
        &self,
        tenant_id: Option<i64>,
    ) -> WebServiceResult<NginxStatusResponse>;

    async fn list_servers(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ServerPage>;

    async fn create_server(
        &self,
        tenant_id: i64,
        request: &CreateServerRequest,
    ) -> WebServiceResult<CreateServerResponse>;

    async fn authenticate_agent_token(&self, token: &str) -> WebServiceResult<(String, i64)>;

    async fn resolve_runtime_assignment_target(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        node_uuid: &str,
    ) -> WebServiceResult<RuntimeAssignmentTarget>;

    async fn publish_runtime_assignment(
        &self,
        write: RuntimeAssignmentWrite,
    ) -> WebServiceResult<RuntimeAssignment>;

    async fn retrieve_current_runtime_assignment(
        &self,
        tenant_id: i64,
        node_uuid: &str,
        environment: &str,
        if_generation: Option<&str>,
        if_snapshot_sha256: Option<&str>,
    ) -> WebServiceResult<RuntimeAssignmentDelivery>;

    async fn create_runtime_observation(
        &self,
        write: RuntimeObservationWrite,
    ) -> WebServiceResult<RuntimeObservation>;

    async fn retrieve_latest_runtime_observation(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        snapshot_uuid: &str,
    ) -> WebServiceResult<RuntimeObservation>;

    async fn record_agent_heartbeat(
        &self,
        server_id: &str,
        tenant_id: i64,
        request: &AgentHeartbeatRequest,
    ) -> WebServiceResult<AgentHeartbeatResponse>;

    async fn build_agent_sync_manifest(
        &self,
        server_id: &str,
        tenant_id: i64,
        if_sync_version: Option<&str>,
    ) -> WebServiceResult<AgentSyncResponse>;

    async fn list_audit_logs(
        &self,
        tenant_id: Option<i64>,
        query: &ListAuditLogsQuery,
    ) -> WebServiceResult<AuditLogPage>;

    async fn insert_audit_log(&self, entry: AuditLogWrite<'_>) -> WebServiceResult<()>;
}
