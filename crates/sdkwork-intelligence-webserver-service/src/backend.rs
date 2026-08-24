//! Backend-api service surface implementation.

use async_trait::async_trait;
use sdkwork_webserver_contract::{
    CreateApplicationRequest, CreateDeploymentRequest, CreateDomainRequest,
    CreateListenerCertificateBindingRequest, CreateManagedDomainRequest, CreateNginxConfigRequest,
    CreateRootDomainHostnameRequest, CreateRootDomainRequest, CreateServerRequest,
    CreateSourceVersionRequest, ImportGitSourceVersionRequest, IssueCertificateRequest,
    ListApplicationsQuery, ListNginxConfigsQuery, ListRootDomainsQuery, UpdateApplicationRequest,
    UpdateCertificateRequest, UpdateDomainApplicationBindingRequest, UpdateNginxConfigRequest,
    WebAppApi, WebAppRequestContext, WebAppResourceScope, WebBackendApi, WebBackendRequestContext,
    WebServiceError, WebServiceResult,
};

use crate::{AuditLogWrite, WebService};

const MAX_NGINX_CONFIG_BYTES: usize = 1024 * 1024;

impl WebService {
    /// 统一的 fail-closed 租户上下文校验。
    ///
    /// 所有 backend-api 操作（读与写）都必须携带有效 tenant_id（>0），
    /// 防止 `tenant_id=None` 时跨租户读写数据。
    /// 平台级跨租户管理操作应通过独立 platform-admin 鉴权链路实现，不复用此通道。
    fn require_backend_tenant(context: &WebBackendRequestContext) -> WebServiceResult<i64> {
        context
            .tenant_id
            .filter(|tenant_id| *tenant_id > 0)
            .ok_or(WebServiceError::validation(
                "tenant context is required for backend operations",
            ))
    }

    fn backend_app_context(
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<WebAppRequestContext> {
        Ok(WebAppRequestContext {
            tenant_id: Self::require_backend_tenant(context)?,
            actor_id: context.operator_id,
            organization_id: None,
            session_id: None,
            idempotency_key: context.idempotency_key.clone(),
            resource_scope: WebAppResourceScope::Tenant,
        })
    }

    async fn audit_backend_action(
        &self,
        context: &WebBackendRequestContext,
        action: &str,
        target_type: &str,
        target_uuid: &str,
    ) {
        let tenant_id = match Self::require_backend_tenant(context) {
            Ok(tenant_id) => tenant_id,
            Err(error) => {
                tracing::error!(
                    action,
                    target_type,
                    target_uuid,
                    error = ?error,
                    "failed to resolve tenant for backend business audit"
                );
                return;
            }
        };
        let _ = self
            .record_audit_log(AuditLogWrite {
                tenant_id,
                organization_id: 0,
                operator_id: context.operator_id.unwrap_or(0),
                operator_type: "ADMIN",
                action,
                target_type,
                target_id: None,
                target_uuid: Some(target_uuid),
                request_id: None,
                metadata_json: "{}",
            })
            .await;
    }

    fn normalize_root_domain_request(
        request: &CreateRootDomainRequest,
    ) -> WebServiceResult<CreateRootDomainRequest> {
        let hostname = request.hostname.trim().to_ascii_lowercase();
        Self::validate_domain_request(&CreateDomainRequest {
            hostname: hostname.clone(),
            is_primary: false,
            ssl_enabled: false,
            ssl_provider: None,
        })?;
        if hostname.split('.').count() < 2 {
            return Err(WebServiceError::validation(
                "root domain must contain at least two DNS labels",
            ));
        }
        Ok(CreateRootDomainRequest { hostname })
    }

    fn normalize_root_domain_hostname_request(
        request: &CreateRootDomainHostnameRequest,
    ) -> WebServiceResult<CreateRootDomainHostnameRequest> {
        let record_name = request.record_name.trim().to_ascii_lowercase();
        if record_name != "@" {
            Self::validate_domain_request(&CreateDomainRequest {
                hostname: record_name.clone(),
                is_primary: request.is_primary,
                ssl_enabled: request.ssl_enabled,
                ssl_provider: request.ssl_provider.clone(),
            })?;
        } else if request
            .ssl_provider
            .as_deref()
            .is_some_and(|provider| !matches!(provider, "letsencrypt" | "custom" | "none"))
        {
            return Err(WebServiceError::validation(
                "sslProvider must be letsencrypt, custom, or none",
            ));
        }
        if request.application_id.is_none() && request.is_primary {
            return Err(WebServiceError::validation(
                "an unbound hostname cannot be primary",
            ));
        }
        Ok(CreateRootDomainHostnameRequest {
            record_name,
            application_id: request.application_id.clone(),
            is_primary: request.is_primary,
            ssl_enabled: request.ssl_enabled,
            ssl_provider: request.ssl_provider.clone(),
        })
    }
}

#[async_trait]
impl WebBackendApi for WebService {
    async fn list_applications(
        &self,
        context: &WebBackendRequestContext,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationPage> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::list_applications(self, &app_context, query).await
    }

    async fn create_application(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::create_application(self, &app_context, request).await
    }

    async fn retrieve_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::retrieve_application(self, &app_context, application_id).await
    }

    async fn update_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::update_application(self, &app_context, application_id, request).await
    }

    async fn delete_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<()> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::delete_application(self, &app_context, application_id).await
    }

    async fn activate_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::activate_application(self, &app_context, application_id).await
    }

    async fn pause_application(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::pause_application(self, &app_context, application_id).await
    }

    async fn list_application_domains(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainPage> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::list_domains(self, &app_context, application_id, page, page_size).await
    }

    async fn create_application_domain(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::create_domain(self, &app_context, application_id, request).await
    }

    async fn verify_application_domain(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainVerifyResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::verify_domain(self, &app_context, application_id, domain_id).await
    }

    async fn delete_application_domain(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::delete_domain(self, &app_context, application_id, domain_id).await
    }

    async fn list_root_domains(
        &self,
        context: &WebBackendRequestContext,
        query: &ListRootDomainsQuery,
    ) -> WebServiceResult<sdkwork_webserver_contract::RootDomainPage> {
        if query
            .status
            .is_some_and(|status| !(0..=2).contains(&status))
        {
            return Err(WebServiceError::validation(
                "status must be between 0 and 2",
            ));
        }
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository.list_root_domains(tenant_id, query).await
    }

    async fn create_root_domain(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateRootDomainRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::RootDomainResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let request = Self::normalize_root_domain_request(request)?;
        let root_domain = self
            .repository
            .create_root_domain(tenant_id, &request)
            .await?;
        self.audit_backend_action(
            context,
            "root_domains.create",
            "root_domain",
            &root_domain.id,
        )
        .await;
        Ok(root_domain)
    }

    async fn retrieve_root_domain(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::RootDomainResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .retrieve_root_domain(tenant_id, root_domain_id)
            .await
    }

    async fn delete_root_domain(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
    ) -> WebServiceResult<()> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .delete_root_domain(tenant_id, root_domain_id)
            .await?;
        self.audit_backend_action(
            context,
            "root_domains.delete",
            "root_domain",
            root_domain_id,
        )
        .await;
        Ok(())
    }

    async fn list_root_domain_hostnames(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainPage> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .list_root_domain_hostnames(tenant_id, root_domain_id, page, page_size)
            .await
    }

    async fn create_root_domain_hostname(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
        request: &CreateRootDomainHostnameRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let request = Self::normalize_root_domain_hostname_request(request)?;
        let domain = self
            .repository
            .create_root_domain_hostname(tenant_id, root_domain_id, &request)
            .await?;
        self.audit_backend_action(
            context,
            "root_domains.hostnames.create",
            "domain",
            &domain.id,
        )
        .await;
        Ok(domain)
    }

    async fn list_managed_domains(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainPage> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .list_managed_domains(tenant_id, page, page_size)
            .await
    }

    async fn create_managed_domain(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateManagedDomainRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        Self::validate_domain_request(&CreateDomainRequest {
            hostname: request.hostname.clone(),
            is_primary: request.is_primary,
            ssl_enabled: request.ssl_enabled,
            ssl_provider: request.ssl_provider.clone(),
        })?;
        if request.application_id.is_none() && request.is_primary {
            return Err(WebServiceError::validation(
                "an unbound domain cannot be primary",
            ));
        }
        let tenant_id = Self::require_backend_tenant(context)?;
        let domain = self
            .repository
            .create_managed_domain(tenant_id, request)
            .await?;
        self.audit_backend_action(context, "domains.create", "domain", &domain.id)
            .await;
        Ok(domain)
    }

    async fn delete_managed_domain(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
    ) -> WebServiceResult<()> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .delete_managed_domain(tenant_id, domain_id)
            .await?;
        self.audit_backend_action(context, "domains.delete", "domain", domain_id)
            .await;
        Ok(())
    }

    async fn verify_managed_domain(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainVerifyResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let challenge = self
            .repository
            .prepare_managed_domain_verification(tenant_id, domain_id)
            .await?;
        let verification = self
            .execute_domain_verification(tenant_id, challenge)
            .await?;
        self.audit_backend_action(context, "domains.verify", "domain", domain_id)
            .await;
        Ok(verification)
    }

    async fn update_domain_application_binding(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
        request: &UpdateDomainApplicationBindingRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        if request.application_id.trim().is_empty() {
            return Err(WebServiceError::validation(
                "applicationId must not be empty",
            ));
        }
        let tenant_id = Self::require_backend_tenant(context)?;
        let domain = self
            .repository
            .bind_managed_domain(tenant_id, domain_id, request)
            .await?;
        self.audit_backend_action(
            context,
            "domains.application_binding.update",
            "domain",
            domain_id,
        )
        .await;
        Ok(domain)
    }

    async fn delete_domain_application_binding(
        &self,
        context: &WebBackendRequestContext,
        domain_id: &str,
    ) -> WebServiceResult<()> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .unbind_managed_domain(tenant_id, domain_id)
            .await?;
        self.audit_backend_action(
            context,
            "domains.application_binding.delete",
            "domain",
            domain_id,
        )
        .await;
        Ok(())
    }

    async fn list_application_source_versions(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionPage> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::list_source_versions(self, &app_context, application_id, page, page_size, cursor)
            .await
    }

    async fn create_application_source_version(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::create_source_version(self, &app_context, application_id, request).await
    }

    async fn import_application_git_source_version(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &ImportGitSourceVersionRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::import_git_source_version(self, &app_context, application_id, request).await
    }

    async fn retrieve_application_source_version(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        source_version_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::retrieve_source_version(self, &app_context, application_id, source_version_id)
            .await
    }

    async fn list_application_deployments(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentPage> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::list_deployments(
            self,
            &app_context,
            application_id,
            page,
            page_size,
            status,
            cursor,
        )
        .await
    }

    async fn create_application_deployment(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::create_deployment(self, &app_context, application_id, request).await
    }

    async fn rollback_application_deployment(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::rollback_deployment(self, &app_context, application_id, deployment_id).await
    }

    async fn list_managed_certificates(
        &self,
        context: &WebBackendRequestContext,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificatePage> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::list_certificates(self, &app_context, None, domain_id, page, page_size).await
    }

    async fn issue_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        request: &IssueCertificateRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateOperationAcceptedResponse> {
        Self::validate_certificate_issue_request(request)?;
        let tenant_id = Self::require_backend_tenant(context)?;
        let operation = self
            .repository
            .enqueue_certificate_issue(
                tenant_id,
                None,
                context.operator_id,
                request,
                context.idempotency_key.as_deref(),
            )
            .await?;
        self.audit_backend_action(
            context,
            "certificates.issue.requested",
            "certificate_operation",
            &operation.operation_id,
        )
        .await;
        Ok(operation)
    }

    async fn retrieve_managed_certificate_operation(
        &self,
        context: &WebBackendRequestContext,
        operation_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateOperationResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .retrieve_certificate_operation(tenant_id, None, operation_id)
            .await
    }

    async fn update_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
        request: &UpdateCertificateRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let certificate = self
            .repository
            .update_certificate_auto_renew(tenant_id, certificate_id, request.auto_renew)
            .await?;
        self.audit_backend_action(
            context,
            "certificates.auto_renew.update",
            "certificate",
            certificate_id,
        )
        .await;
        Ok(certificate)
    }

    async fn delete_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
    ) -> WebServiceResult<()> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .delete_certificate(tenant_id, certificate_id, context.operator_id)
            .await?;
        self.audit_backend_action(
            context,
            "certificates.delete",
            "certificate",
            certificate_id,
        )
        .await;
        Ok(())
    }

    async fn renew_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateOperationAcceptedResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let operation = self
            .repository
            .enqueue_certificate_renewal(
                tenant_id,
                certificate_id,
                context.operator_id,
                context.idempotency_key.as_deref(),
            )
            .await?;
        self.audit_backend_action(
            context,
            "certificates.renew.requested",
            "certificate_operation",
            &operation.operation_id,
        )
        .await;
        Ok(operation)
    }

    async fn revoke_managed_certificate(
        &self,
        context: &WebBackendRequestContext,
        certificate_id: &str,
        request: &sdkwork_webserver_contract::RevokeCertificateRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let reason = sdkwork_webserver_acme_service::CertificateRevocationReason::parse(
            &request.reason,
        )
        .ok_or_else(|| {
            WebServiceError::validation(
                "revocation reason must be one of keyCompromise, affiliationChanged, superseded, cessationOfOperation, privilegeWithdrawn",
            )
        })?;
        // CA revocation must be acknowledged before the aggregate is marked
        // revoked; a rejected revocation fails the request without touching
        // the certificate state. Self-signed certificates have no CA and are
        // marked revoked locally.
        let material = self
            .repository
            .load_certificate_revocation_material(tenant_id, certificate_id)
            .await?;
        if material.cert_type == 1 {
            self.certificate_issuer
                .revoke_certificate(&material.fullchain_pem, reason)
                .await
                .map_err(|error| {
                    WebServiceError::Internal(format!("certificate revocation failed: {error}"))
                })?;
        }
        let certificate = self
            .repository
            .mark_certificate_revoked(tenant_id, certificate_id, request, context.operator_id)
            .await?;
        self.audit_backend_action(
            context,
            "certificates.revoke",
            "certificate",
            certificate_id,
        )
        .await;
        // Revocation archives the listener bindings; publish immediately so
        // the data plane stops serving the revoked revision.
        self.publish_node_tls_material_best_effort("certificate_revoke")
            .await;
        Ok(certificate)
    }

    async fn list_application_listener_certificate_bindings(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::ListenerCertificateBindingPage> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::list_listener_certificate_bindings(
            self,
            &app_context,
            application_id,
            domain_id,
            page,
            page_size,
        )
        .await
    }

    async fn bind_application_listener_certificate(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ListenerCertificateBindingResponse> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::bind_listener_certificate(self, &app_context, application_id, domain_id, request)
            .await
    }

    async fn unbind_application_listener_certificate(
        &self,
        context: &WebBackendRequestContext,
        application_id: &str,
        domain_id: &str,
        binding_id: &str,
    ) -> WebServiceResult<()> {
        let app_context = Self::backend_app_context(context)?;
        WebAppApi::unbind_listener_certificate(
            self,
            &app_context,
            application_id,
            domain_id,
            binding_id,
        )
        .await
    }

    async fn list_certificate_distribution(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateDistributionPage> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .list_certificate_distribution(tenant_id, page, page_size)
            .await
    }

    async fn list_nginx_configs(
        &self,
        context: &WebBackendRequestContext,
        query: &ListNginxConfigsQuery,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxConfigPage> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .list_nginx_configs(Some(tenant_id), query)
            .await
    }

    async fn create_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateNginxConfigRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxConfigResponse> {
        validate_create_nginx_config_request(request)?;
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .create_nginx_config(tenant_id, request)
            .await
    }

    async fn retrieve_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxConfigResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .retrieve_nginx_config(Some(tenant_id), config_id)
            .await
    }

    async fn update_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxConfigResponse> {
        validate_update_nginx_config_request(request)?;
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .update_nginx_config(Some(tenant_id), config_id, request)
            .await
    }

    async fn validate_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxValidateResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let content = self
            .repository
            .load_nginx_config_content(Some(tenant_id), config_id)
            .await?;
        match self.validate_nginx_content(&content).await {
            Ok(()) => Ok(sdkwork_webserver_contract::NginxValidateResponse {
                valid: true,
                message: None,
            }),
            Err(error) => Ok(sdkwork_webserver_contract::NginxValidateResponse {
                valid: false,
                message: Some(error.to_string()),
            }),
        }
    }

    async fn web_nginx_config(
        &self,
        context: &WebBackendRequestContext,
        config_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxConfigResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let candidate = self
            .repository
            .retrieve_nginx_config(Some(tenant_id), config_id)
            .await?;
        let domain = self
            .repository
            .resolve_site_primary_hostname(tenant_id, &candidate.site_id)
            .await?;
        let content = self
            .repository
            .load_nginx_config_content(Some(tenant_id), config_id)
            .await?;
        self.validate_nginx_content(&content).await?;

        // Activate the edge first (deploy + reload), then record the
        // activation in the control plane. If the database commit fails,
        // roll the edge back to the previously active configuration so the
        // edge never diverges silently from the control-plane state.
        self.deploy_nginx_site(&domain, &content).await?;
        self.reload_nginx_runtime().await?;
        // PRD-FR-020: prove the served revision before reporting success.
        // `nginx -s reload` only signals the master; a config that fails
        // validation keeps the previous revision serving. `nginx -T` dumps
        // the loaded configuration, so the server-name fragment must be
        // present before activation is acknowledged.
        self.verify_nginx_served(&format!("server_name {domain};"))
            .await?;
        let response = match self
            .repository
            .web_nginx_config(Some(tenant_id), config_id)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                self.rollback_nginx_edge(tenant_id, &candidate.site_id, &domain)
                    .await;
                return Err(error);
            }
        };

        Ok(response)
    }

    async fn reload_nginx(
        &self,
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxReloadResponse> {
        if context.operator_id.is_none() {
            // A global Nginx reload affects every tenant's sites on this host;
            // machine principals (Web Node Daemon credentials) must never reach it.
            return Err(sdkwork_webserver_contract::WebServiceError::Forbidden);
        }
        self.reload_nginx_runtime().await?;
        Ok(sdkwork_webserver_contract::NginxReloadResponse { reloaded: true })
    }

    async fn retrieve_nginx_status(
        &self,
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<sdkwork_webserver_contract::NginxStatusResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let mut response = self
            .repository
            .retrieve_nginx_status(Some(tenant_id))
            .await?;
        // Truthfulness (PRD-FR-020): `running` must reflect the actual edge
        // runtime, not a database inference from active config rows. A real
        // `nginx -t` on the served active configuration is the strongest
        // local liveness/validity evidence available to the control plane.
        let runtime = self.edge_runtime.clone();
        let running = tokio::task::spawn_blocking(move || runtime.validate_active_config())
            .await
            .map_err(|error| {
                WebServiceError::Internal(format!("join nginx status probe: {error}"))
            })?
            .is_ok();
        response.running = running;
        Ok(response)
    }

    async fn list_servers(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::ServerPage> {
        let tenant_id = Self::require_backend_tenant(context)?;
        self.repository
            .list_servers(tenant_id, page, page_size, cursor)
            .await
    }

    async fn create_server(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateServerRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::CreateServerResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        validate_create_server_request(request)?;
        self.repository.create_server(tenant_id, request).await
    }

    async fn list_audit_logs(
        &self,
        context: &WebBackendRequestContext,
        query: &sdkwork_webserver_contract::ListAuditLogsQuery,
    ) -> WebServiceResult<sdkwork_webserver_contract::AuditLogPage> {
        let tenant_id = Self::require_backend_tenant(context)?;
        if let Some(operator_id) = query.operator_id {
            if operator_id <= 0 {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "operatorId must be a positive integer",
                ));
            }
        }
        let start_date = match query.start_date.as_deref() {
            Some(value) => Some(
                crate::audit_time::normalize_audit_instant(
                    value,
                    crate::audit_time::AuditInstantBound::StartInclusive,
                )
                .map_err(|detail| {
                    sdkwork_webserver_contract::WebServiceError::validation(format!(
                        "startDate {detail}"
                    ))
                })?,
            ),
            None => None,
        };
        let end_date = match query.end_date.as_deref() {
            Some(value) => Some(
                crate::audit_time::normalize_audit_instant(
                    value,
                    crate::audit_time::AuditInstantBound::EndExclusive,
                )
                .map_err(|detail| {
                    sdkwork_webserver_contract::WebServiceError::validation(format!(
                        "endDate {detail}"
                    ))
                })?,
            ),
            None => None,
        };
        if let (Some(start), Some(end)) = (start_date.as_deref(), end_date.as_deref()) {
            if start >= end {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "startDate must be earlier than endDate",
                ));
            }
        }
        let normalized = sdkwork_webserver_contract::ListAuditLogsQuery {
            page_size: query.page_size,
            cursor: query.cursor.clone(),
            target_type: query.target_type.clone(),
            action: query.action.clone(),
            operator_id: query.operator_id,
            start_date,
            end_date,
        };
        self.repository
            .list_audit_logs(Some(tenant_id), &normalized)
            .await
    }
}

impl WebService {
    /// Rolls the deployed Nginx site back to the previously active
    /// configuration after a control-plane activation failure. Best effort:
    /// the original error is preserved and the rollback failure is logged.
    async fn rollback_nginx_edge(&self, tenant_id: i64, site_id: &str, domain: &str) {
        let previous = match self
            .repository
            .load_active_nginx_config_content(tenant_id, site_id)
            .await
        {
            Ok(Some(content)) => content,
            _ => {
                tracing::error!(
                    tenant_id,
                    site_id,
                    "nginx activation rollback skipped: no previous active configuration"
                );
                return;
            }
        };
        if let Err(error) = self.deploy_nginx_site(domain, &previous).await {
            tracing::error!(
                tenant_id,
                site_id,
                "nginx activation rollback deploy failed: {error}"
            );
            return;
        }
        if let Err(error) = self.reload_nginx_runtime().await {
            tracing::error!(
                tenant_id,
                site_id,
                "nginx activation rollback reload failed: {error}"
            );
        }
    }
}

fn validate_create_nginx_config_request(
    request: &CreateNginxConfigRequest,
) -> WebServiceResult<()> {
    if !matches!(request.config_type, 1..=4) {
        return Err(WebServiceError::validation(
            "configType must be 1 (server), 2 (location), 3 (ssl), or 4 (upstream)",
        ));
    }
    validate_bounded_text("siteId", &request.site_id, 64)?;
    validate_bounded_text("configName", &request.config_name, 200)?;
    validate_nginx_config_content(&request.config_content)
}

fn validate_update_nginx_config_request(
    request: &UpdateNginxConfigRequest,
) -> WebServiceResult<()> {
    if request.config_name.is_none() && request.config_content.is_none() {
        return Err(WebServiceError::validation(
            "at least one Nginx configuration field is required",
        ));
    }
    if let Some(config_name) = request.config_name.as_deref() {
        validate_bounded_text("configName", config_name, 200)?;
    }
    if let Some(config_content) = request.config_content.as_deref() {
        validate_nginx_config_content(config_content)?;
    }
    Ok(())
}

fn validate_nginx_config_content(value: &str) -> WebServiceResult<()> {
    if value.is_empty() || value.len() > MAX_NGINX_CONFIG_BYTES || value.contains('\0') {
        return Err(WebServiceError::validation(
            "configContent must contain 1 byte to 1 MiB and must not contain NUL",
        ));
    }
    Ok(())
}

fn validate_create_server_request(request: &CreateServerRequest) -> WebServiceResult<()> {
    validate_bounded_text("name", &request.name, 100)?;
    validate_bounded_text("host", &request.host, 255)?;
    if request.host.chars().any(char::is_whitespace) {
        return Err(WebServiceError::validation(
            "host must not contain whitespace",
        ));
    }
    if !(1..=65_535).contains(&request.ssh_port) {
        return Err(WebServiceError::validation(
            "sshPort must be between 1 and 65535",
        ));
    }
    validate_tenant_scope_hash(&request.tenant_scope_hash)
}

fn validate_bounded_text(field: &str, value: &str, maximum: usize) -> WebServiceResult<()> {
    if value.is_empty()
        || value != value.trim()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(WebServiceError::validation(format!(
            "{field} must contain 1..{maximum} trimmed non-control characters"
        )));
    }
    Ok(())
}

fn validate_tenant_scope_hash(value: &str) -> WebServiceResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(WebServiceError::validation(
            "tenantScopeHash must be a lowercase SHA-256 digest",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        validate_create_nginx_config_request, validate_create_server_request,
        validate_tenant_scope_hash, validate_update_nginx_config_request, WebService,
        MAX_NGINX_CONFIG_BYTES,
    };
    use sdkwork_webserver_contract::{
        CreateNginxConfigRequest, CreateServerRequest, UpdateNginxConfigRequest,
        WebAppResourceScope, WebBackendRequestContext,
    };

    #[test]
    fn tenant_scope_hash_is_exact_lowercase_sha256_shape() {
        validate_tenant_scope_hash(&"a".repeat(64)).unwrap();
        for invalid in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
            assert!(validate_tenant_scope_hash(&invalid).is_err());
        }
    }

    #[test]
    fn nginx_configuration_requests_are_bounded_and_site_scoped() {
        validate_create_nginx_config_request(&CreateNginxConfigRequest {
            site_id: "site-1".to_owned(),
            config_name: "edge".to_owned(),
            config_type: 1,
            config_content: "server {}".to_owned(),
        })
        .unwrap();
        for request in [
            CreateNginxConfigRequest {
                site_id: String::new(),
                config_name: "edge".to_owned(),
                config_type: 1,
                config_content: "server {}".to_owned(),
            },
            CreateNginxConfigRequest {
                site_id: "site-1".to_owned(),
                config_name: "edge".to_owned(),
                config_type: 0,
                config_content: "server {}".to_owned(),
            },
            CreateNginxConfigRequest {
                site_id: "site-1".to_owned(),
                config_name: "edge".to_owned(),
                config_type: 1,
                config_content: "x".repeat(MAX_NGINX_CONFIG_BYTES + 1),
            },
        ] {
            assert!(validate_create_nginx_config_request(&request).is_err());
        }
        assert!(
            validate_update_nginx_config_request(&UpdateNginxConfigRequest::default()).is_err()
        );
        assert!(
            validate_update_nginx_config_request(&UpdateNginxConfigRequest {
                config_name: None,
                config_content: Some("location / {}".to_owned()),
            })
            .is_ok()
        );
    }

    #[test]
    fn server_registration_rejects_unbounded_hosts_and_invalid_ports() {
        let valid = CreateServerRequest {
            name: "edge-1".to_owned(),
            host: "10.0.0.8".to_owned(),
            tenant_scope_hash: "a".repeat(64),
            ssh_port: 22,
        };
        validate_create_server_request(&valid).unwrap();
        for request in [
            CreateServerRequest {
                ssh_port: 0,
                ..valid.clone()
            },
            CreateServerRequest {
                host: "edge host".to_owned(),
                ..valid.clone()
            },
            CreateServerRequest {
                name: " ".to_owned(),
                ..valid.clone()
            },
        ] {
            assert!(validate_create_server_request(&request).is_err());
        }
    }

    #[test]
    fn backend_application_operations_use_tenant_scope() {
        let context = WebBackendRequestContext {
            tenant_id: Some(42),
            operator_id: Some(7),
            subject_id: Some("7".to_owned()),
            idempotency_key: Some("deployment-create-1".to_owned()),
        };

        let app_context = WebService::backend_app_context(&context).unwrap();

        assert_eq!(app_context.tenant_id, 42);
        assert_eq!(app_context.actor_id, Some(7));
        assert_eq!(
            app_context.idempotency_key.as_deref(),
            Some("deployment-create-1")
        );
        assert_eq!(app_context.resource_scope, WebAppResourceScope::Tenant);
    }
}
