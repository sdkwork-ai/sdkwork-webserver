// `WebRepositoryPort` implementation delegated to the engine-specific repository modules.

use async_trait::async_trait;
use sdkwork_intelligence_webserver_service::{
    AuditLogWrite, CertificateRevocationMaterial, DomainVerificationChallenge,
    DomainVerificationObservation, RuntimeAssignmentTarget, RuntimeAssignmentWrite,
    RuntimeObservationWrite, WebRepositoryPort,
};
use sdkwork_webserver_contract::{
    AgentHeartbeatRequest, AgentHeartbeatResponse, AgentSyncResponse, AuditLogPage,
    CertificateDistributionPage, CertificateIssueUpdate, CertificateOperationAcceptedResponse,
    CertificateOperationLease, CertificateOperationResponse, CertificatePage,
    ListAuditLogsQuery,
    CertificateResponse, IssueCertificateRequest,
    CreateDeploymentRequest, CreateDomainRequest, CreateEnvVariableRequest, CreatePlatformTargetRequest,
    CreateHealthCheckRequest, CreateManagedDomainRequest, CreateNginxConfigRequest,
    CreateListenerCertificateBindingRequest, ListenerCertificateBindingPage,
    ListenerCertificateBindingResponse,
    CreateRootDomainHostnameRequest, CreateRootDomainRequest, CreateServerRequest,
    CreateServerResponse, CreateApplicationRequest, CreateSourceVersionRequest, DeploymentPage,
    DeploymentResponse, DomainPage, DomainResponse, EnvVariablePage, EnvVariableResponse,
    UpdateEnvVariableRequest,
    HealthCheckPage, HealthCheckResponse, ListNginxConfigsQuery,
    ListRootDomainsQuery, ListApplicationsQuery, NginxConfigPage, NginxConfigResponse,
    NginxStatusResponse, RootDomainPage,
    RootDomainResponse, RuntimeAssignment, RuntimeAssignmentDelivery, RuntimeObservation,
    ServerPage, ApplicationPage, ApplicationResponse, PlatformTargetPage, PlatformTargetResponse,
    SourceVersionPage, SourceVersionResponse,
    TlsCertificateAssignmentMaterial, UpdateDomainApplicationBindingRequest,
    UpdateNginxConfigRequest, UpdateApplicationRequest, RevokeCertificateRequest,
};
use sdkwork_webserver_contract::{WebServiceError, WebServiceResult};

use super::agents::AuthenticatedAgent;
use super::WebRepository;

#[async_trait]
impl WebRepositoryPort for WebRepository {
    async fn ready_check(&self) -> WebServiceResult<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|_| WebServiceError::DatabaseUnavailable)?;
        Ok(())
    }

    async fn list_applications(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<ApplicationPage> {
        self.list_applications_repo(tenant_id, owner_id, query).await
    }

    async fn create_application(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        owner_id: Option<i64>,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse> {
        self.create_application_repo(tenant_id, organization_id, owner_id, request)
            .await
    }

    async fn retrieve_application(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse> {
        self.retrieve_application_repo(tenant_id, owner_id, application_id).await
    }

    async fn update_application(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse> {
        self.update_application_repo(tenant_id, application_id, request).await
    }

    async fn delete_application(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
    ) -> WebServiceResult<()> {
        self.delete_application_repo(tenant_id, application_id, actor_id).await
    }

    async fn set_application_status(
        &self,
        tenant_id: i64,
        application_id: &str,
        status: i32,
    ) -> WebServiceResult<ApplicationResponse> {
        self.set_application_status_repo(tenant_id, application_id, status).await
    }

    async fn resolve_site_id(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<String> {
        self.resolve_site_id_repo(tenant_id, application_id).await
    }

    async fn create_platform_target(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreatePlatformTargetRequest,
    ) -> WebServiceResult<PlatformTargetResponse> {
        self.create_platform_target_repo(tenant_id, application_id, request)
            .await
    }

    async fn list_platform_targets(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<PlatformTargetPage> {
        self.list_platform_targets_repo(tenant_id, application_id, page, page_size)
            .await
    }

    async fn retrieve_platform_target(
        &self,
        tenant_id: i64,
        application_id: &str,
        platform_target_id: &str,
    ) -> WebServiceResult<PlatformTargetResponse> {
        self.retrieve_platform_target_repo(tenant_id, application_id, platform_target_id)
            .await
    }

    async fn list_domains(
        &self,
        tenant_id: i64,
        site_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        self.list_domains_repo(tenant_id, site_id, page, page_size)
            .await
    }

    async fn create_domain(
        &self,
        tenant_id: i64,
        site_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<DomainResponse> {
        self.create_domain_repo(tenant_id, site_id, request).await
    }

    async fn retrieve_domain(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse> {
        self.retrieve_domain_repo(tenant_id, site_id, domain_id)
            .await
    }

    async fn delete_domain(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()> {
        self.delete_domain_repo(tenant_id, site_id, domain_id).await
    }

    async fn prepare_domain_verification(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge> {
        self.prepare_domain_verification_repo(tenant_id, site_id, domain_id)
            .await
    }

    async fn record_domain_verification_observation(
        &self,
        tenant_id: i64,
        challenge_id: &str,
        observation: &DomainVerificationObservation,
    ) -> WebServiceResult<DomainVerificationChallenge> {
        self.record_domain_verification_observation_repo(tenant_id, challenge_id, observation)
            .await
    }

    async fn list_root_domains(
        &self,
        tenant_id: i64,
        query: &ListRootDomainsQuery,
    ) -> WebServiceResult<RootDomainPage> {
        self.list_root_domains_repo(tenant_id, query).await
    }

    async fn create_root_domain(
        &self,
        tenant_id: i64,
        request: &CreateRootDomainRequest,
    ) -> WebServiceResult<RootDomainResponse> {
        self.create_root_domain_repo(tenant_id, request).await
    }

    async fn retrieve_root_domain(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<RootDomainResponse> {
        self.retrieve_root_domain_repo(tenant_id, root_domain_id)
            .await
    }

    async fn delete_root_domain(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<()> {
        self.delete_root_domain_repo(tenant_id, root_domain_id)
            .await
    }

    async fn list_root_domain_hostnames(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        self.list_root_domain_hostnames_repo(tenant_id, root_domain_id, page, page_size)
            .await
    }

    async fn create_root_domain_hostname(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        request: &CreateRootDomainHostnameRequest,
    ) -> WebServiceResult<DomainResponse> {
        self.create_root_domain_hostname_repo(tenant_id, root_domain_id, request)
            .await
    }

    async fn list_managed_domains(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        self.list_managed_domains_repo(tenant_id, page, page_size)
            .await
    }

    async fn list_certificate_domains(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        self.list_certificate_domains_repo(tenant_id, owner_id, page, page_size)
            .await
    }

    async fn create_managed_domain(
        &self,
        tenant_id: i64,
        request: &CreateManagedDomainRequest,
    ) -> WebServiceResult<DomainResponse> {
        self.create_managed_domain_repo(tenant_id, request).await
    }

    async fn delete_managed_domain(&self, tenant_id: i64, domain_id: &str) -> WebServiceResult<()> {
        self.delete_managed_domain_repo(tenant_id, domain_id).await
    }

    async fn bind_managed_domain(
        &self,
        tenant_id: i64,
        domain_id: &str,
        request: &UpdateDomainApplicationBindingRequest,
    ) -> WebServiceResult<DomainResponse> {
        self.bind_managed_domain_repo(tenant_id, domain_id, request)
            .await
    }

    async fn unbind_managed_domain(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse> {
        self.unbind_managed_domain_repo(tenant_id, domain_id, None)
            .await
    }

    async fn prepare_managed_domain_verification(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge> {
        self.prepare_managed_domain_verification_repo(tenant_id, domain_id)
            .await
    }

    async fn list_source_versions(
        &self,
        tenant_id: i64,
        site_id: &str,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<SourceVersionPage> {
        self.list_source_versions_repo(tenant_id, site_id, page, page_size, cursor)
            .await
    }

    async fn create_source_version(
        &self,
        tenant_id: i64,
        site_id: &str,
        actor_id: Option<i64>,
        retention_limit: i32,
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse> {
        self.create_source_version_repo(tenant_id, site_id, actor_id, retention_limit, request)
            .await
    }

    async fn retrieve_source_version(
        &self,
        tenant_id: i64,
        site_id: &str,
        source_version_id: &str,
    ) -> WebServiceResult<SourceVersionResponse> {
        self.retrieve_source_version_repo(tenant_id, site_id, source_version_id)
            .await
    }

    async fn list_deployments(
        &self,
        tenant_id: i64,
        site_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<DeploymentPage> {
        self.list_deployments_repo(tenant_id, site_id, page, page_size, status, cursor)
            .await
    }

    async fn create_deployment(
        &self,
        tenant_id: i64,
        site_id: &str,
        actor_id: Option<i64>,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<DeploymentResponse> {
        self.create_deployment_repo(tenant_id, site_id, actor_id, request)
            .await
    }

    async fn retrieve_deployment(
        &self,
        tenant_id: i64,
        site_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<DeploymentResponse> {
        self.retrieve_deployment_repo(tenant_id, site_id, deployment_id)
            .await
    }

    async fn rollback_deployment(
        &self,
        tenant_id: i64,
        site_id: &str,
        deployment_id: &str,
        actor_id: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<DeploymentResponse> {
        self.rollback_deployment_repo(tenant_id, site_id, deployment_id, actor_id, idempotency_key)
            .await
    }

    async fn list_env_variables(
        &self,
        tenant_id: i64,
        site_id: &str,
        environment: Option<&str>,
    ) -> WebServiceResult<EnvVariablePage> {
        self.list_env_variables_repo(tenant_id, site_id, environment)
            .await
    }

    async fn create_env_variable(
        &self,
        tenant_id: i64,
        site_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse> {
        self.create_env_variable_repo(tenant_id, site_id, request)
            .await
    }

    async fn update_env_variable(
        &self,
        tenant_id: i64,
        site_id: &str,
        variable_id: &str,
        request: &UpdateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse> {
        self.update_env_variable_repo(tenant_id, site_id, variable_id, request)
            .await
    }

    async fn delete_env_variable(
        &self,
        tenant_id: i64,
        site_id: &str,
        variable_id: &str,
    ) -> WebServiceResult<()> {
        self.delete_env_variable_repo(tenant_id, site_id, variable_id)
            .await
    }

    async fn list_certificates(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        site_id: Option<&str>,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificatePage> {
        self.list_certificates_repo(tenant_id, owner_id, site_id, domain_id, page, page_size)
            .await
    }

    async fn enqueue_certificate_issue(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        requested_by: Option<i64>,
        request: &IssueCertificateRequest,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse> {
        self.enqueue_certificate_issue_repo(
            tenant_id,
            owner_id,
            requested_by,
            request,
            idempotency_key,
        )
        .await
    }

    async fn enqueue_certificate_renewal(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        requested_by: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse> {
        self.enqueue_certificate_renewal_repo(
            tenant_id,
            certificate_id,
            requested_by,
            idempotency_key,
        )
        .await
    }

    async fn retrieve_certificate_operation(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        operation_id: &str,
    ) -> WebServiceResult<CertificateOperationResponse> {
        self.retrieve_certificate_operation_repo(tenant_id, owner_id, operation_id)
            .await
    }

    async fn schedule_due_certificate_renewals(
        &self,
        renew_before_days: u32,
        limit: i32,
    ) -> WebServiceResult<usize> {
        self.schedule_due_certificate_renewals_repo(renew_before_days, limit)
            .await
    }

    async fn claim_certificate_operations(
        &self,
        lease_owner: &str,
        lease_seconds: i64,
        limit: i32,
    ) -> WebServiceResult<Vec<CertificateOperationLease>> {
        self.claim_certificate_operations_repo(lease_owner, lease_seconds, limit)
            .await
    }

    async fn renew_certificate_operation_lease(
        &self,
        lease: &CertificateOperationLease,
        lease_seconds: i64,
    ) -> WebServiceResult<()> {
        self.renew_certificate_operation_lease_repo(lease, lease_seconds)
            .await
    }

    async fn delete_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        deleted_by: Option<i64>,
    ) -> WebServiceResult<()> {
        self.delete_certificate_repo(tenant_id, certificate_id, deleted_by)
            .await
    }

    async fn list_listener_certificate_bindings(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ListenerCertificateBindingPage> {
        self.list_listener_certificate_bindings_repo(
            tenant_id,
            site_id,
            domain_id,
            page,
            page_size,
        )
        .await
    }

    async fn bind_listener_certificate(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<ListenerCertificateBindingResponse> {
        self.bind_listener_certificate_repo(tenant_id, site_id, domain_id, request)
            .await
    }

    async fn unbind_listener_certificate(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
        binding_id: &str,
    ) -> WebServiceResult<()> {
        self.unbind_listener_certificate_repo(tenant_id, site_id, domain_id, binding_id)
            .await
    }

    async fn finalize_certificate_operation(
        &self,
        lease: &CertificateOperationLease,
        update: &CertificateIssueUpdate,
    ) -> WebServiceResult<CertificateResponse> {
        self.finalize_certificate_operation_repo(lease, update)
            .await
    }

    async fn fail_certificate_operation(
        &self,
        lease: &CertificateOperationLease,
        failure_code: &str,
        retry_at: &str,
        terminal_retry_at: &str,
    ) -> WebServiceResult<CertificateOperationResponse> {
        self.fail_certificate_operation_repo(
            lease,
            failure_code,
            retry_at,
            terminal_retry_at,
        )
            .await
    }

    async fn update_certificate_auto_renew(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        auto_renew: bool,
    ) -> WebServiceResult<CertificateResponse> {
        self.update_certificate_auto_renew_repo(tenant_id, certificate_id, auto_renew)
            .await
    }

    async fn load_node_tls_certificate_assignments(
        &self,
        node_uuid: &str,
    ) -> WebServiceResult<Vec<TlsCertificateAssignmentMaterial>> {
        self.load_node_tls_certificate_assignments_repo(node_uuid)
            .await
    }

    async fn load_certificate_revocation_material(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> WebServiceResult<CertificateRevocationMaterial> {
        self.load_certificate_revocation_material_repo(tenant_id, certificate_id)
            .await
    }

    async fn mark_certificate_revoked(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        request: &RevokeCertificateRequest,
        revoked_by: Option<i64>,
    ) -> WebServiceResult<CertificateResponse> {
        self.mark_certificate_revoked_repo(tenant_id, certificate_id, request, revoked_by)
            .await
    }

    async fn record_certificate_renewal_info(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        window_start: &str,
        window_end: &str,
    ) -> WebServiceResult<()> {
        self.record_certificate_renewal_info_repo(
            tenant_id,
            certificate_id,
            window_start,
            window_end,
        )
        .await
    }

    async fn list_certificate_distribution(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificateDistributionPage> {
        self.list_certificate_distribution_repo(tenant_id, page, page_size)
            .await
    }

    async fn list_health_checks(
        &self,
        tenant_id: i64,
        site_id: &str,
    ) -> WebServiceResult<HealthCheckPage> {
        self.list_health_checks_repo(tenant_id, site_id).await
    }

    async fn create_health_check(
        &self,
        tenant_id: i64,
        site_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> WebServiceResult<HealthCheckResponse> {
        self.create_health_check_repo(tenant_id, site_id, request)
            .await
    }

    async fn list_nginx_configs(
        &self,
        tenant_id: Option<i64>,
        query: &ListNginxConfigsQuery,
    ) -> WebServiceResult<NginxConfigPage> {
        self.list_nginx_configs_repo(tenant_id, query).await
    }

    async fn create_nginx_config(
        &self,
        tenant_id: i64,
        request: &CreateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse> {
        self.create_nginx_config_repo(tenant_id, request).await
    }

    async fn retrieve_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse> {
        self.retrieve_nginx_config_repo(tenant_id, config_id).await
    }

    async fn update_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse> {
        self.update_nginx_config_repo(tenant_id, config_id, request)
            .await
    }

    async fn load_nginx_config_content(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<String> {
        self.load_nginx_config_content_repo(tenant_id, config_id)
            .await
    }

    async fn resolve_site_primary_hostname(
        &self,
        tenant_id: i64,
        site_uuid: &str,
    ) -> WebServiceResult<String> {
        self.resolve_site_primary_hostname_repo(tenant_id, site_uuid)
            .await
    }

    async fn load_active_nginx_config_content(
        &self,
        tenant_id: i64,
        site_id: &str,
    ) -> WebServiceResult<Option<String>> {
        self.load_active_nginx_config_content_repo(tenant_id, site_id)
            .await
    }

    async fn web_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse> {
        self.web_nginx_config_repo(tenant_id, config_id).await
    }

    async fn retrieve_nginx_status(
        &self,
        tenant_id: Option<i64>,
    ) -> WebServiceResult<NginxStatusResponse> {
        self.retrieve_nginx_status_repo(tenant_id).await
    }

    async fn list_servers(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ServerPage> {
        self.list_servers_repo(tenant_id, page, page_size, cursor)
            .await
    }

    async fn create_server(
        &self,
        tenant_id: i64,
        request: &CreateServerRequest,
    ) -> WebServiceResult<CreateServerResponse> {
        self.create_server_repo(tenant_id, request).await
    }

    async fn authenticate_agent_token(&self, token: &str) -> WebServiceResult<(String, i64)> {
        let agent = self.authenticate_agent_token_repo(token).await?;
        Ok((agent.server_uuid, agent.tenant_id))
    }

    async fn resolve_runtime_assignment_target(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        node_uuid: &str,
    ) -> WebServiceResult<RuntimeAssignmentTarget> {
        self.resolve_runtime_assignment_target_repo(
            requester_tenant_id,
            can_cross_tenant,
            node_uuid,
        )
        .await
    }

    async fn publish_runtime_assignment(
        &self,
        write: RuntimeAssignmentWrite,
    ) -> WebServiceResult<RuntimeAssignment> {
        self.publish_runtime_assignment_repo(write).await
    }

    async fn retrieve_current_runtime_assignment(
        &self,
        tenant_id: i64,
        node_uuid: &str,
        environment: &str,
        if_generation: Option<&str>,
        if_snapshot_sha256: Option<&str>,
    ) -> WebServiceResult<RuntimeAssignmentDelivery> {
        self.retrieve_current_runtime_assignment_repo(
            tenant_id,
            node_uuid,
            environment,
            if_generation,
            if_snapshot_sha256,
        )
        .await
    }

    async fn create_runtime_observation(
        &self,
        write: RuntimeObservationWrite,
    ) -> WebServiceResult<RuntimeObservation> {
        self.create_runtime_observation_repo(write).await
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        snapshot_uuid: &str,
    ) -> WebServiceResult<RuntimeObservation> {
        self.retrieve_latest_runtime_observation_repo(
            requester_tenant_id,
            can_cross_tenant,
            snapshot_uuid,
        )
        .await
    }

    async fn record_agent_heartbeat(
        &self,
        server_id: &str,
        tenant_id: i64,
        request: &AgentHeartbeatRequest,
    ) -> WebServiceResult<AgentHeartbeatResponse> {
        let agent = AuthenticatedAgent {
            server_uuid: server_id.to_string(),
            tenant_id,
        };
        self.record_agent_heartbeat_repo(&agent, request).await
    }

    async fn build_agent_sync_manifest(
        &self,
        server_id: &str,
        tenant_id: i64,
        if_sync_version: Option<&str>,
    ) -> WebServiceResult<AgentSyncResponse> {
        let agent = AuthenticatedAgent {
            server_uuid: server_id.to_string(),
            tenant_id,
        };
        self.build_agent_sync_manifest_repo(&agent, if_sync_version)
            .await
    }

    async fn list_audit_logs(
        &self,
        tenant_id: Option<i64>,
        query: &ListAuditLogsQuery,
    ) -> WebServiceResult<AuditLogPage> {
        self.list_audit_logs_repo(tenant_id, query).await
    }

    async fn insert_audit_log(&self, entry: AuditLogWrite<'_>) -> WebServiceResult<()> {
        self.insert_audit_log_repo(entry).await
    }
}
