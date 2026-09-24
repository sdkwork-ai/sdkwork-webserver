//! Backend-api service surface implementation.

use async_trait::async_trait;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use sdkwork_webserver_contract::{
    web_is_platform_operator_tenant, ClusterEventPage, ClusterHeartbeatSamplePage, ClusterHostPage,
    ClusterHostResponse, ClusterInstancePage, ClusterInstanceResponse, ClusterOverviewResponse,
    ClusterPage, ClusterResponse, ClusterSyncManifest, CreateApplicationRequest,
    CreateClusterRequest, CreateDomainRequest, CreateListenerCertificateBindingRequest,
    CreateManagedDomainRequest, CreateNginxConfigRequest, CreateRootDomainHostnameRequest,
    CreateRootDomainRequest, CreateServerRequest, CreateSourceVersionRequest,
    EnqueueClusterPeerMessagesRequest, EnqueueClusterPeerMessagesResponse,
    ImportGitSourceVersionRequest, IssueCertificateRequest, ListApplicationsQuery,
    ListNginxConfigsQuery, ListRootDomainsQuery, MetricsSeriesWindow, MetricsSummaryQuery,
    MetricsSummaryResponse, MetricsWindowBounds, MetricsWindowRequest, TrafficUsageStatisticsQuery,
    TrafficUsageStatisticsResponse, TrafficUsageWindow, UpdateApplicationRequest,
    UpdateCertificateRequest, UpdateClusterHostRequest, UpdateClusterInstanceRequest,
    UpdateClusterRequest, UpdateDomainApplicationBindingRequest, UpdateNginxConfigRequest,
    UpdateRootDomainRequest, WebAppApi, WebAppRequestContext, WebAppResourceScope, WebBackendApi,
    WebBackendRequestContext, WebServiceError, WebServiceResult, DEFAULT_TRAFFIC_USAGE_TOP_APPS,
    DEFAULT_TRAFFIC_USAGE_WINDOW_DAYS, MAX_TRAFFIC_USAGE_TOP_APPS, MAX_TRAFFIC_USAGE_WINDOW_DAYS,
    METRICS_ENTITY_TENANTS, METRICS_LAST_SEVEN_DAYS_SPAN, METRICS_WINDOWS,
    METRICS_WINDOW_CURRENT_MONTH, METRICS_WINDOW_LAST_7_DAYS, METRICS_WINDOW_LIFETIME,
    METRICS_WINDOW_TODAY,
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

    /// Validate and canonicalise a partial root-domain edit.
    ///
    /// The descriptive fields mirror the tenant console's zone form exactly: a
    /// field left blank is *omitted*, and an omitted field leaves the stored
    /// value alone. That is why a blank string folds to `None` here rather than
    /// being sent through as "clear it" — the two planes have to read the same
    /// form the same way, and this is the reading the console already ships.
    ///
    /// Lengths are the column widths (`display_name` 200, `dns_provider` 64,
    /// `provider_zone_ref` 512). Rejecting over-length here keeps a too-long
    /// value a validation problem at the edge instead of a store error after
    /// the fact.
    fn normalize_root_domain_update_request(
        request: &UpdateRootDomainRequest,
    ) -> WebServiceResult<UpdateRootDomainRequest> {
        fn optional_text(
            value: Option<&String>,
            max_len: usize,
            field: &str,
        ) -> WebServiceResult<Option<String>> {
            let Some(value) = value else {
                return Ok(None);
            };
            let value = value.trim();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > max_len {
                return Err(WebServiceError::validation(format!(
                    "{field} must be at most {max_len} characters"
                )));
            }
            Ok(Some(value.to_string()))
        }

        if let Some(status) = request.status {
            if !(0..=2).contains(&status) {
                return Err(WebServiceError::validation(
                    "status must be between 0 and 2",
                ));
            }
        }

        let display_name = optional_text(request.display_name.as_ref(), 200, "displayName")?;
        let dns_provider = optional_text(request.dns_provider.as_ref(), 64, "dnsProvider")?
            .map(|value| value.to_ascii_lowercase());
        let provider_zone_ref =
            optional_text(request.provider_zone_ref.as_ref(), 512, "providerZoneRef")?;

        if display_name.is_none()
            && dns_provider.is_none()
            && provider_zone_ref.is_none()
            && request.status.is_none()
        {
            return Err(WebServiceError::validation(
                "at least one of displayName, dnsProvider, providerZoneRef or status is required",
            ));
        }

        Ok(UpdateRootDomainRequest {
            display_name,
            dns_provider,
            provider_zone_ref,
            status: request.status,
        })
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

    async fn update_root_domain(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
        request: &UpdateRootDomainRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::RootDomainResponse> {
        let tenant_id = Self::require_backend_tenant(context)?;
        let request = Self::normalize_root_domain_update_request(request)?;
        let root_domain = self
            .repository
            .update_root_domain(tenant_id, root_domain_id, &request)
            .await?;
        // One audit action for both edits: the actor, the zone and the instant
        // are what an audit trail is read for, and the field-level diff lives in
        // the row's own `updated_at`/`version` rather than in a second action
        // name per field.
        self.audit_backend_action(
            context,
            "root_domains.update",
            "root_domain",
            &root_domain.id,
        )
        .await;
        Ok(root_domain)
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

    async fn webserver_nginx_config(
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
            .webserver_nginx_config(Some(tenant_id), config_id)
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
    async fn list_clusters(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ClusterPage> {
        self.cluster_list(context, page, page_size).await
    }

    async fn create_cluster(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateClusterRequest,
    ) -> WebServiceResult<ClusterResponse> {
        self.cluster_create(context, request).await
    }

    async fn retrieve_cluster(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
    ) -> WebServiceResult<ClusterResponse> {
        self.cluster_retrieve(context, cluster_id).await
    }

    async fn update_cluster(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
        request: &UpdateClusterRequest,
    ) -> WebServiceResult<ClusterResponse> {
        self.cluster_update(context, cluster_id, request).await
    }

    async fn delete_cluster(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
    ) -> WebServiceResult<()> {
        self.cluster_delete(context, cluster_id).await
    }

    async fn list_cluster_hosts(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        status: Option<i32>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHostPage> {
        self.cluster_host_list(context, cluster_id, status, page_size, cursor)
            .await
    }

    async fn retrieve_cluster_host(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
    ) -> WebServiceResult<ClusterHostResponse> {
        self.cluster_host_retrieve(context, host_id).await
    }

    async fn update_cluster_host(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
        request: &UpdateClusterHostRequest,
    ) -> WebServiceResult<ClusterHostResponse> {
        self.cluster_host_update(context, host_id, request).await
    }

    async fn delete_cluster_host(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
    ) -> WebServiceResult<()> {
        self.cluster_host_delete(context, host_id).await
    }

    async fn list_cluster_instances(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        host_id: Option<&str>,
        status: Option<i32>,
        health_state: Option<&str>,
        join_mode: Option<i32>,
        sync_status: Option<i32>,
        labels: Option<&str>,
        search: Option<&str>,
        build_version: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterInstancePage> {
        self.cluster_instance_list(
            context,
            cluster_id,
            host_id,
            status,
            health_state,
            join_mode,
            sync_status,
            labels,
            search,
            build_version,
            page_size,
            cursor,
        )
        .await
    }

    async fn drain_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        self.cluster_instance_drain(context, instance_id).await
    }

    async fn undrain_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        self.cluster_instance_undrain(context, instance_id).await
    }

    async fn cordon_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        self.cluster_instance_cordon(context, instance_id).await
    }

    async fn uncordon_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        self.cluster_instance_uncordon(context, instance_id).await
    }

    async fn cluster_instance_metrics_history(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage> {
        self.cluster_heartbeat_list(context, instance_id, page_size, cursor)
            .await
    }

    async fn publish_cluster_sync_revision(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
        kind: &str,
        payload: serde_json::Value,
    ) -> WebServiceResult<ClusterSyncManifest> {
        self.cluster_sync_publish(context, cluster_id, kind, payload)
            .await
    }

    async fn retrieve_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        self.cluster_instance_retrieve(context, instance_id).await
    }

    async fn update_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        request: &UpdateClusterInstanceRequest,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        self.cluster_instance_update(context, instance_id, request)
            .await
    }

    async fn delete_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<()> {
        self.cluster_instance_delete(context, instance_id).await
    }

    async fn list_cluster_events(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        severity: Option<&str>,
        instance_id: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterEventPage> {
        self.cluster_events_list(
            context,
            cluster_id,
            severity,
            instance_id,
            page_size,
            cursor,
        )
        .await
    }

    async fn retrieve_cluster_overview(
        &self,
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<ClusterOverviewResponse> {
        self.cluster_overview(context).await
    }

    async fn list_cluster_heartbeats(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage> {
        self.cluster_heartbeat_list(context, instance_id, page_size, cursor)
            .await
    }

    async fn enqueue_cluster_messages(
        &self,
        context: &WebBackendRequestContext,
        request: &EnqueueClusterPeerMessagesRequest,
    ) -> WebServiceResult<EnqueueClusterPeerMessagesResponse> {
        self.cluster_message_enqueue(context, request).await
    }

    async fn retrieve_traffic_usage_statistics(
        &self,
        context: &WebBackendRequestContext,
        query: &TrafficUsageStatisticsQuery,
    ) -> WebServiceResult<TrafficUsageStatisticsResponse> {
        let tenant_id = Some(Self::require_backend_tenant(context)?);
        let window = resolve_traffic_usage_window(query)?;
        self.read_traffic_usage_statistics(tenant_id, &window).await
    }

    async fn retrieve_platform_traffic_usage_statistics(
        &self,
        context: &WebBackendRequestContext,
        query: &TrafficUsageStatisticsQuery,
    ) -> WebServiceResult<TrafficUsageStatisticsResponse> {
        require_traffic_usage_platform_operator(context)?;
        let window = resolve_traffic_usage_window(query)?;
        self.read_traffic_usage_statistics(None, &window).await
    }

    async fn retrieve_metrics_summary(
        &self,
        context: &WebBackendRequestContext,
        query: &MetricsSummaryQuery,
    ) -> WebServiceResult<MetricsSummaryResponse> {
        let tenant_id = Some(Self::require_backend_tenant(context)?);
        let windows = resolve_metrics_windows();
        let series = resolve_metrics_series_window(query)?;
        self.read_metrics_summary(tenant_id, &windows, &series)
            .await
    }

    async fn retrieve_platform_metrics_summary(
        &self,
        context: &WebBackendRequestContext,
        query: &MetricsSummaryQuery,
    ) -> WebServiceResult<MetricsSummaryResponse> {
        require_platform_operator_tenant(context)?;
        let windows = resolve_metrics_windows();
        let series = resolve_metrics_series_window(query)?;
        self.read_metrics_summary(None, &windows, &series).await
    }
}

impl WebService {
    /// Shared body of both metric-summary readings; `tenant_id: None` means
    /// "every tenant" and is reachable only through the platform operation.
    ///
    /// The scope-shaping lives here rather than in the read model on purpose.
    /// Which metrics exist is a consequence of the *authorization* this layer
    /// resolved, so this layer is where "a tenant-scoped reading has no tenant
    /// count" is enforced; a read model that forgot would otherwise ship a
    /// structurally-constant `1` as a platform metric, and nothing downstream
    /// could tell that it had.
    ///
    /// A missing reader is reported as `503 unavailable` for the same reason the
    /// traffic readings do it: an empty metric row and an unwired one look
    /// identical once drawn.
    async fn read_metrics_summary(
        &self,
        tenant_id: Option<i64>,
        windows: &MetricsWindowRequest,
        series_window: &MetricsSeriesWindow,
    ) -> WebServiceResult<MetricsSummaryResponse> {
        let port = self.metrics_summary.as_ref().ok_or_else(|| {
            WebServiceError::unavailable(
                "the dashboard metrics summary read model is not assembled in this deployment",
            )
        })?;
        let readings = port
            .retrieve_metrics_summary(tenant_id, windows, series_window)
            .await?;
        let platform_scope = tenant_id.is_none();
        Ok(MetricsSummaryResponse {
            as_of: windows.as_of.clone(),
            platform_scope,
            traffic_since: readings.traffic_since,
            windows: metrics_window_bounds(windows),
            entities: if platform_scope {
                readings.entities
            } else {
                readings
                    .entities
                    .into_iter()
                    .filter(|metric| metric.metric != METRICS_ENTITY_TENANTS)
                    .collect()
            },
            traffic: readings.traffic,
            // Passed through unshaped by reach: how much space a tenant holds
            // is answerable for the caller's own tenant, so unlike the tenant
            // count there is nothing here a scope has to withhold.
            storage: readings.storage,
            // The series and its window travel together and are passed through
            // unshaped by reach, on the same terms as the entities: an agent or
            // a user that exists in the caller's tenant is a real figure for
            // that tenant, and the tenant series this reading does not request
            // is withheld by the read model rather than filtered here — there is
            // no platform-only member to strip.
            series: readings.series,
            series_window: series_window.clone(),
            // Not filtered by reach: a deployment that cannot count agents
            // cannot count them for anybody, so the same names apply to both
            // readings and a console that hid them would be claiming a
            // capability the edge does not have.
            unassembled_metrics: readings.unassembled_metrics,
        })
    }

    /// Shared body of both traffic-usage readings; `tenant_id: None` means
    /// "every tenant" and is reachable only through the platform operation.
    ///
    /// A missing reader is reported as `503 unavailable` rather than as an
    /// empty result: the two are indistinguishable in a chart, and "this
    /// deployment assembles no usage read model" must not be presented to an
    /// operator as "this edge served no traffic".
    async fn read_traffic_usage_statistics(
        &self,
        tenant_id: Option<i64>,
        window: &TrafficUsageWindow,
    ) -> WebServiceResult<TrafficUsageStatisticsResponse> {
        let port = self.traffic_usage.as_ref().ok_or_else(|| {
            WebServiceError::unavailable(
                "the aggregated traffic usage read model is not assembled in this deployment",
            )
        })?;
        port.retrieve_traffic_usage_statistics(tenant_id, window)
            .await
    }

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

/// The one predicate every platform-wide reading rests on: the caller must
/// belong to the operator tenant.
///
/// Resolved through [`web_is_platform_operator_tenant`] rather than a literal
/// so this guard, the route guard, and the IAM bootstrap cannot disagree about
/// which tenant the platform operator is — the same rule the cluster plane
/// already follows. A context bound to any other tenant is rejected rather
/// than silently narrowed to that tenant's slice: a narrowed answer looks like
/// a working platform total and would be believed.
///
/// Kept to exactly one body rather than copied per surface. Two identical
/// copies are how one reachable path ends up missing a change applied to the
/// other, and this particular failure is invisible in the response — a
/// tenant-scoped reading served as the platform total carries no field that
/// reveals it.
fn require_platform_operator_tenant(context: &WebBackendRequestContext) -> WebServiceResult<()> {
    let tenant_id = context.tenant_id.map(|id| id.to_string());
    if web_is_platform_operator_tenant(tenant_id.as_deref()) {
        Ok(())
    } else {
        Err(WebServiceError::Forbidden)
    }
}

/// Gate for the cross-tenant traffic reading — the shared operator predicate
/// under the name of the operation it guards.
fn require_traffic_usage_platform_operator(
    context: &WebBackendRequestContext,
) -> WebServiceResult<()> {
    require_platform_operator_tenant(context)
}

/// Turns the wire query into a concrete window.
///
/// Split out of both operation bodies so "what does an omitted bound mean" has
/// exactly one answer: the trailing
/// [`DEFAULT_TRAFFIC_USAGE_WINDOW_DAYS`] days, ending at tomorrow (UTC) — the
/// end bound is **exclusive**, so the default always covers today's partial
/// day instead of dropping it.
fn resolve_traffic_usage_window(
    query: &TrafficUsageStatisticsQuery,
) -> WebServiceResult<TrafficUsageWindow> {
    let (date_from, date_to) =
        resolve_date_bounds(query.date_from.as_deref(), query.date_to.as_deref())?;
    let dimension = match query.dimension.as_deref() {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed.len() > 64 {
                return Err(WebServiceError::validation(
                    "dimension must contain 1 to 64 characters",
                ));
            }
            Some(trimmed.to_owned())
        }
        None => None,
    };
    let top_apps = query.top_apps.unwrap_or(DEFAULT_TRAFFIC_USAGE_TOP_APPS);
    if !(1..=MAX_TRAFFIC_USAGE_TOP_APPS).contains(&top_apps) {
        return Err(WebServiceError::validation(format!(
            "topApps must be between 1 and {MAX_TRAFFIC_USAGE_TOP_APPS}"
        )));
    }
    Ok(TrafficUsageWindow {
        date_from: date_from.format("%Y-%m-%d").to_string(),
        date_to: date_to.format("%Y-%m-%d").to_string(),
        dimension,
        top_apps,
    })
}

/// Turns a caller-supplied pair of optional bounds into concrete UTC days.
///
/// Split out of the two readings that accept a window — the traffic usage
/// reading and the metric summary's series — because "what does an omitted
/// bound mean" must have exactly **one** answer. Two implementations would
/// default differently the first time one of them was edited, and the failure
/// is invisible: both would still return a plausible window, and the two lines
/// of one chart would simply be drawn over different periods.
///
/// The defaults end **exclusively** at tomorrow (UTC), so a read always covers
/// today's partial day instead of dropping it. The span bound is a bound on the
/// *work*: both readings return one row per day per series, so an unchecked span
/// lets one request ask for the whole retention period and hold a shared-pool
/// connection while it does.
fn resolve_date_bounds(
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> WebServiceResult<(NaiveDate, NaiveDate)> {
    let date_to = match date_to {
        Some(value) => parse_traffic_usage_date("dateTo", value)?,
        None => Utc::now().date_naive() + Duration::days(1),
    };
    let date_from = match date_from {
        Some(value) => parse_traffic_usage_date("dateFrom", value)?,
        None => date_to - Duration::days(DEFAULT_TRAFFIC_USAGE_WINDOW_DAYS),
    };
    if date_from >= date_to {
        return Err(WebServiceError::validation(
            "dateFrom must be earlier than dateTo (dateTo is exclusive)",
        ));
    }
    let span_days = (date_to - date_from).num_days();
    if span_days > MAX_TRAFFIC_USAGE_WINDOW_DAYS {
        return Err(WebServiceError::validation(format!(
            "the window must not exceed {MAX_TRAFFIC_USAGE_WINDOW_DAYS} days; read a longer range as consecutive windows"
        )));
    }
    Ok((date_from, date_to))
}

/// Resolves the metric summary's **series** window.
///
/// Deliberately the same shape — same parameter names, same default, same bound
/// — as the traffic reading's window, because one chart draws both. The four
/// card windows are **not** resolved here and are unaffected by the query: they
/// are properties of the product rather than of the caller's view, and
/// `resolve_metrics_windows` reads the clock for them alone.
fn resolve_metrics_series_window(
    query: &MetricsSummaryQuery,
) -> WebServiceResult<MetricsSeriesWindow> {
    let (date_from, date_to) =
        resolve_date_bounds(query.date_from.as_deref(), query.date_to.as_deref())?;
    Ok(MetricsSeriesWindow {
        date_from: date_from.format("%Y-%m-%d").to_string(),
        date_to: date_to.format("%Y-%m-%d").to_string(),
    })
}

/// Resolves the four reporting windows against the current UTC clock.
///
/// The three bounded windows share one **exclusive** upper bound — tomorrow
/// (UTC) — so `today` covers the whole of today including the hours already
/// elapsed, for the same reason the traffic readings' default end bound does.
/// Their lower bounds are:
///
/// - `today` — today itself;
/// - `last_7_days` — six days before today, so a "7 day" window holds seven
///   calendar days rather than eight. An off-by-one here is invisible in the
///   figure and only ever wrong in the label, which is the kind of error that
///   survives review;
/// - `current_month` — the first of the current month, which on the first of
///   the month is today rather than a day of the previous month.
///
/// `lifetime` gets no bound at all: what it covers is a property of the figures
/// rather than of the calendar, and the read model reports it back through
/// `trafficSince` instead of having a lower bound invented for it here.
///
/// The clock is read exactly once. Two reads of `Utc::now()` can straddle
/// midnight and label one response's figures with two different days.
fn resolve_metrics_windows() -> MetricsWindowRequest {
    let today = Utc::now().date_naive();
    let day = |offset_days: i64| {
        (today + Duration::days(offset_days))
            .format("%Y-%m-%d")
            .to_string()
    };
    MetricsWindowRequest {
        as_of: day(0),
        last_seven_days_from: day(-(METRICS_LAST_SEVEN_DAYS_SPAN - 1)),
        // `day0()` is the zero-based day of the month, so subtracting it lands
        // on the first without a date constructor that can fail.
        current_month_from: (today - Duration::days(i64::from(today.day0())))
            .format("%Y-%m-%d")
            .to_string(),
        ends_before: day(1),
    }
}

/// The concrete bounds behind the window ids, for the response to state.
///
/// Ordered by the contract's own vocabulary rather than by this function's
/// literal order, so a window added to [`METRICS_WINDOWS`] cannot end up
/// described in one place and drawn in another.
fn metrics_window_bounds(windows: &MetricsWindowRequest) -> Vec<MetricsWindowBounds> {
    let starts: [(&str, Option<&str>); 4] = [
        (METRICS_WINDOW_TODAY, Some(windows.as_of.as_str())),
        (
            METRICS_WINDOW_LAST_7_DAYS,
            Some(windows.last_seven_days_from.as_str()),
        ),
        (
            METRICS_WINDOW_CURRENT_MONTH,
            Some(windows.current_month_from.as_str()),
        ),
        (METRICS_WINDOW_LIFETIME, None),
    ];
    METRICS_WINDOWS
        .iter()
        .filter_map(|window| {
            starts
                .iter()
                .find(|(id, _)| id == window)
                .map(|(_, date_from)| MetricsWindowBounds {
                    window: (*window).to_owned(),
                    date_from: date_from.map(str::to_owned),
                    date_to: windows.ends_before.clone(),
                })
        })
        .collect()
}

/// Strict `YYYY-MM-DD`. The shape is checked before the calendar: `chrono`
/// accepts single-digit month/day, and a value that only reaches the read
/// model after being silently reshaped is a value the caller cannot reproduce
/// from what they sent.
fn parse_traffic_usage_date(field: &str, value: &str) -> WebServiceResult<NaiveDate> {
    let invalid = || {
        WebServiceError::validation(format!(
            "{field} must be a UTC calendar day formatted YYYY-MM-DD"
        ))
    };
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(invalid());
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return Err(invalid());
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::{
        require_traffic_usage_platform_operator, resolve_traffic_usage_window,
        validate_create_nginx_config_request, validate_create_server_request,
        validate_tenant_scope_hash, validate_update_nginx_config_request, WebService,
        MAX_NGINX_CONFIG_BYTES,
    };
    use chrono::{Duration, Utc};
    use sdkwork_webserver_contract::{
        web_platform_operator_tenant_id, CreateNginxConfigRequest, CreateServerRequest,
        TrafficUsageStatisticsQuery, UpdateNginxConfigRequest, WebAppResourceScope,
        WebBackendRequestContext, DEFAULT_TRAFFIC_USAGE_TOP_APPS,
        DEFAULT_TRAFFIC_USAGE_WINDOW_DAYS, MAX_TRAFFIC_USAGE_TOP_APPS,
        MAX_TRAFFIC_USAGE_WINDOW_DAYS,
    };

    fn traffic_usage_query(
        date_from: Option<&str>,
        date_to: Option<&str>,
        dimension: Option<&str>,
        top_apps: Option<i32>,
    ) -> TrafficUsageStatisticsQuery {
        TrafficUsageStatisticsQuery {
            date_from: date_from.map(str::to_owned),
            date_to: date_to.map(str::to_owned),
            dimension: dimension.map(str::to_owned),
            top_apps,
        }
    }

    fn backend_context(tenant_id: Option<i64>) -> WebBackendRequestContext {
        WebBackendRequestContext {
            tenant_id,
            operator_id: Some(7),
            subject_id: Some("7".to_owned()),
            idempotency_key: None,
            permission_scope: Vec::new(),
        }
    }

    #[test]
    fn traffic_usage_window_defaults_to_trailing_thirty_days_ending_tomorrow() {
        let window =
            resolve_traffic_usage_window(&traffic_usage_query(None, None, None, None)).unwrap();

        // Exclusive end, so the default window always contains today.
        let expected_end = Utc::now().date_naive() + Duration::days(1);
        assert_eq!(window.date_to, expected_end.format("%Y-%m-%d").to_string());
        assert_eq!(
            window.date_from,
            (expected_end - Duration::days(DEFAULT_TRAFFIC_USAGE_WINDOW_DAYS))
                .format("%Y-%m-%d")
                .to_string()
        );
        assert_eq!(window.top_apps, DEFAULT_TRAFFIC_USAGE_TOP_APPS);
        assert_eq!(window.dimension, None);
    }

    #[test]
    fn traffic_usage_window_rejects_unshaped_or_inverted_bounds() {
        for query in [
            // Single-digit month: `chrono` would accept it and silently reshape
            // the value the caller sent.
            traffic_usage_query(Some("2026-9-01"), Some("2026-10-01"), None, None),
            traffic_usage_query(Some("2026-09-01T00:00:00Z"), Some("2026-10-01"), None, None),
            traffic_usage_query(Some("20260901"), Some("2026-10-01"), None, None),
            traffic_usage_query(Some("2026-13-01"), Some("2026-14-01"), None, None),
            // Reversed and empty windows are both rejected: an inverted range
            // returns no rows, which a surface draws as "no traffic".
            traffic_usage_query(Some("2026-10-01"), Some("2026-09-01"), None, None),
            traffic_usage_query(Some("2026-10-01"), Some("2026-10-01"), None, None),
            // One day past the span bound: the daily series and the aggregate
            // scan both grow with the window, so the bound is on the work.
            traffic_usage_query(Some("2020-01-01"), Some("2021-06-01"), None, None),
        ] {
            assert!(resolve_traffic_usage_window(&query).is_err());
        }
    }

    #[test]
    fn traffic_usage_window_span_bound_admits_exactly_the_configured_maximum() {
        // The bound is inclusive of the maximum span and exclusive of anything
        // wider, so the widest accepted window is exactly
        // `MAX_TRAFFIC_USAGE_WINDOW_DAYS`.
        let widest = resolve_traffic_usage_window(&traffic_usage_query(
            Some("2026-01-01"),
            Some("2027-01-02"),
            None,
            None,
        ))
        .unwrap();
        assert_eq!(
            (chrono::NaiveDate::parse_from_str(&widest.date_to, "%Y-%m-%d").unwrap()
                - chrono::NaiveDate::parse_from_str(&widest.date_from, "%Y-%m-%d").unwrap())
            .num_days(),
            MAX_TRAFFIC_USAGE_WINDOW_DAYS
        );

        assert!(resolve_traffic_usage_window(&traffic_usage_query(
            Some("2026-01-01"),
            Some("2027-01-03"),
            None,
            None,
        ))
        .is_err());
    }

    #[test]
    fn traffic_usage_window_trims_dimension_and_bounds_top_apps() {
        let window = resolve_traffic_usage_window(&traffic_usage_query(
            Some("2026-09-01"),
            Some("2026-10-01"),
            Some(" traffic.requests "),
            Some(MAX_TRAFFIC_USAGE_TOP_APPS),
        ))
        .unwrap();
        assert_eq!(window.dimension.as_deref(), Some("traffic.requests"));
        assert_eq!(window.top_apps, MAX_TRAFFIC_USAGE_TOP_APPS);

        for query in [
            traffic_usage_query(Some("2026-09-01"), Some("2026-10-01"), Some("   "), None),
            traffic_usage_query(
                Some("2026-09-01"),
                Some("2026-10-01"),
                Some(&"d".repeat(65)),
                None,
            ),
            traffic_usage_query(Some("2026-09-01"), Some("2026-10-01"), None, Some(0)),
            traffic_usage_query(Some("2026-09-01"), Some("2026-10-01"), None, Some(-1)),
            traffic_usage_query(
                Some("2026-09-01"),
                Some("2026-10-01"),
                None,
                Some(MAX_TRAFFIC_USAGE_TOP_APPS + 1),
            ),
        ] {
            assert!(resolve_traffic_usage_window(&query).is_err());
        }
    }

    #[test]
    fn platform_traffic_usage_is_restricted_to_the_operator_tenant() {
        let operator_tenant_id = web_platform_operator_tenant_id();
        let operator_id: i64 = operator_tenant_id.parse().expect(
            "the configured platform operator tenant id must be numeric for the context to carry it",
        );

        // The operator tenant reads every tenant ...
        require_traffic_usage_platform_operator(&backend_context(Some(operator_id))).unwrap();
        // ... and nothing else does: a tenant-bound context is rejected rather
        // than narrowed to its own slice, and an absent tenant is rejected
        // rather than treated as "all".
        for context in [
            backend_context(Some(operator_id + 1)),
            backend_context(Some(0)),
            backend_context(None),
        ] {
            assert!(require_traffic_usage_platform_operator(&context).is_err());
        }
    }

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
            permission_scope: Vec::new(),
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
