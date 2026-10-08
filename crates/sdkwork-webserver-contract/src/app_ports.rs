use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::cluster::*;
use crate::dto::*;
use crate::metrics::*;
use crate::problem::WebServiceResult;
use crate::usage::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebAppResourceScope {
    #[default]
    Owner,
    Tenant,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebAppRequestContext {
    // Transport-internal today (axum Extension, framework-resolved), but the
    // serde derives mean any future wire exposure would emit raw JSON numbers
    // for snowflake-scale ids — pinned to the shared int64-string helpers so
    // that can never happen silently (API_SPEC §13.6).
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub tenant_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    pub actor_id: Option<i64>,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    pub organization_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub resource_scope: WebAppResourceScope,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebBackendRequestContext {
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    pub operator_id: Option<i64>,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    pub tenant_id: Option<i64>,
    /// Raw principal subject identifier (server UUID for agent-token routes, user_id string for dual-token).
    /// Present when the framework resolves a principal; absent for anonymous/public contexts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// IAM permission codes granted to the principal (empty for machine-only
    /// contexts). Consumed by per-operation authorization checks such as the
    /// deploy-class Server Files operations (PRD-FR-029).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_scope: Vec<String>,
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
    /// Generic free-text search on the wire is `q` (API_SPEC §16.4).
    #[serde(rename = "q")]
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
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string"
    )]
    pub cursor: Option<String>,
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string"
    )]
    pub target_type: Option<String>,
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string"
    )]
    pub action: Option<String>,
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::serde_int64::option_query::deserialize"
    )]
    pub operator_id: Option<i64>,
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string"
    )]
    pub start_date: Option<String>,
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string"
    )]
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
    /// Generic free-text search on the wire is `q` (API_SPEC §16.4).
    #[serde(rename = "q")]
    pub keyword: Option<String>,
    /// Restrict the inventory to the root domains bound to one cloud account.
    ///
    /// Three states, which is why this is a single optional string rather than an
    /// id plus a flag: an absent member filters nothing, a named account id
    /// returns the Zones bound to that account, and the literal
    /// [`ROOT_DOMAIN_CLOUD_ACCOUNT_UNASSIGNED`] returns the Zones bound to no
    /// account at all — the ones whose account resolves per operation. The
    /// literal cannot collide with a real account id, because
    /// [`crate::dto::cloud_account_id_shape_error`] refuses whitespace and admits
    /// only ASCII alphanumerics and `_.:-`, so the word is unreachable as an id.
    ///
    /// A parameter object rather than two query members keeps the one question
    /// ("which binding?") in one value, so a caller cannot send an account id and
    /// "unassigned" at the same time and get an empty list with no explanation.
    ///
    /// The wire name is snake_case like every other query member on this surface
    /// (`page_size`), while the member the SDK exposes is camelCase: the
    /// generated client maps one to the other, so the rename here is what keeps
    /// the contract's own naming rule rather than a second spelling of it.
    #[serde(
        rename = "cloud_account_id",
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string"
    )]
    pub cloud_account_id: Option<String>,
}

/// The wire value that asks for root domains bound to no cloud account.
///
/// See [`RootDomainCloudAccountFilter::from_query`] for why this is a reserved
/// literal rather than a second boolean parameter.
pub const ROOT_DOMAIN_CLOUD_ACCOUNT_UNASSIGNED: &str = "UNASSIGNED";

/// The cloud-account binding a root-domain list is restricted to.
///
/// The three states are mutually exclusive by construction rather than by
/// convention, which is what keeps "no filter", "bound to no account" and "bound
/// to this account" from being expressible as contradictory pairs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootDomainCloudAccountFilter {
    /// Return every root domain in the tenant, whatever it is bound to.
    Any,
    /// Return only the root domains bound to no account.
    Unassigned,
    /// Return only the root domains bound to this account.
    Assigned(String),
}

impl RootDomainCloudAccountFilter {
    /// Reads the filter off the raw query value.
    ///
    /// A blank value is an omission, matching every other optional query string
    /// on this surface: `?cloudAccountId=` is what a cleared form field submits,
    /// and reading it as "the account whose id is the empty string" would turn a
    /// cleared filter into an empty list.
    ///
    /// The reserved literal is matched case-sensitively on purpose. Account ids
    /// are case-sensitive (the shape rule admits both cases), so folding case here
    /// would make `unassigned` mean something the caller's own account id
    /// `unassigned` does not.
    pub fn from_query(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("") => Self::Any,
            Some(ROOT_DOMAIN_CLOUD_ACCOUNT_UNASSIGNED) => Self::Unassigned,
            Some(account_id) => Self::Assigned(account_id.to_owned()),
        }
    }

    /// The account id to match, or `None` when this filter is not an assignment.
    pub fn assigned_account_id(&self) -> Option<&str> {
        match self {
            Self::Assigned(account_id) => Some(account_id),
            Self::Any | Self::Unassigned => None,
        }
    }
}

impl ListRootDomainsQuery {
    /// Resolves the query's cloud-account member into the filter the store reads.
    ///
    /// A method rather than a field on the query so the wire shape stays a plain
    /// optional string — the three states exist for the store's benefit, and
    /// putting them on the deserialized struct would mean the query could hold a
    /// state the wire cannot express.
    pub fn cloud_account_filter(&self) -> RootDomainCloudAccountFilter {
        RootDomainCloudAccountFilter::from_query(self.cloud_account_id.as_deref())
    }
}

/// Query of a root-domain Zone's synced resolution records.
///
/// `domain_id` restricts the page to one registered subdomain — the rows the
/// sync matched to that hostname, wildcard semantics already applied at sync
/// time (a wildcard declaration's rows are its base owner and every owner
/// beneath it). An absent member pages the whole Zone snapshot, which is what
/// the Zone-level read asks for.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListDomainDnsRecordsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(
        rename = "domain_id",
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_string"
    )]
    pub domain_id: Option<String>,
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
        cursor: Option<&str>,
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

    /// Edit a tenant root-domain Zone: its descriptive fields, its lifecycle
    /// status, or both. The operations surface reaches the same three
    /// descriptive fields the tenant console's zone form edits.
    async fn update_root_domain(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
        request: &UpdateRootDomainRequest,
    ) -> WebServiceResult<RootDomainResponse>;

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

    /// Pages the Zone's synced DNS resolution records, optionally restricted
    /// to one subdomain. The read is over the last cloud-account sync's
    /// snapshot: it never contacts the provider, so a page render costs a
    /// store read and never a vendor round trip.
    async fn list_root_domain_dns_records(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
        query: &ListDomainDnsRecordsQuery,
    ) -> WebServiceResult<DomainDnsRecordPage>;

    /// Re-reads the Zone's resolution-record inventory from its cloud account
    /// and replaces the stored snapshot. The Zone's account resolves the way
    /// every DNS operation for it does: the bound account when one is pinned,
    /// otherwise the registered account whose zone covers the apex.
    async fn sync_root_domain_dns_records(
        &self,
        context: &WebBackendRequestContext,
        root_domain_id: &str,
    ) -> WebServiceResult<DomainDnsSyncResponse>;

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

    async fn list_managed_certificates(
        &self,
        context: &WebBackendRequestContext,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
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

    /// The cloud DNS accounts this edge can publish challenge records with.
    ///
    /// Served so an operator can name an account on an issue request instead of
    /// guessing; the answer is the runtime registry the ACME engine itself
    /// presents with, not a second copy that could disagree with it.
    async fn list_dns_accounts(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DnsAccountPage>;

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

    async fn webserver_nginx_config(
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

    async fn list_clusters(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ClusterPage>;

    async fn create_cluster(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateClusterRequest,
    ) -> WebServiceResult<ClusterResponse>;

    async fn retrieve_cluster(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
    ) -> WebServiceResult<ClusterResponse>;

    async fn update_cluster(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
        request: &UpdateClusterRequest,
    ) -> WebServiceResult<ClusterResponse>;

    async fn delete_cluster(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_cluster_hosts(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        status: Option<i32>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHostPage>;

    async fn retrieve_cluster_host(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
    ) -> WebServiceResult<ClusterHostResponse>;

    async fn update_cluster_host(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
        request: &UpdateClusterHostRequest,
    ) -> WebServiceResult<ClusterHostResponse>;

    async fn delete_cluster_host(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
    ) -> WebServiceResult<()>;

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
    ) -> WebServiceResult<ClusterInstancePage>;

    /// Per-instance heartbeat metric history: cursor/keyset pagination
    /// (PAGINATION_SPEC), newest first, same contract as the heartbeat list.
    async fn cluster_instance_metrics_history(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage>;

    /// Publishes one desired-state revision (config or applications track)
    /// to every instance of the cluster.
    async fn publish_cluster_sync_revision(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
        kind: &str,
        payload: serde_json::Value,
    ) -> WebServiceResult<ClusterSyncManifest>;

    async fn retrieve_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    /// Graceful drain: exclude the instance from routing; in-flight work
    /// finishes before the node stops.
    async fn drain_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    /// Clears drain and restores routing participation.
    async fn undrain_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    /// Cordon: remove from routing without draining.
    async fn cordon_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    /// Uncordon: restore routing participation.
    async fn uncordon_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    async fn update_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        request: &UpdateClusterInstanceRequest,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    async fn delete_cluster_instance(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_cluster_events(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        severity: Option<&str>,
        instance_id: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterEventPage>;

    async fn retrieve_cluster_overview(
        &self,
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<ClusterOverviewResponse>;

    async fn list_cluster_heartbeats(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage>;

    async fn enqueue_cluster_messages(
        &self,
        context: &WebBackendRequestContext,
        request: &EnqueueClusterPeerMessagesRequest,
    ) -> WebServiceResult<EnqueueClusterPeerMessagesResponse>;

    /// Aggregated traffic usage of **the caller's own tenant** — the console
    /// reading.
    ///
    /// The reach is a property of this operation, never of who calls it. The
    /// same principal that may read its own traffic here also holds
    /// [`Self::retrieve_platform_traffic_usage_statistics`] when it belongs to
    /// the operator tenant, and the two must not be distinguishable by a
    /// parameter: with one identity-derived scope a user of the operator tenant
    /// browsing the console would silently read every tenant's traffic, and the
    /// response would look exactly like their own.
    async fn retrieve_traffic_usage_statistics(
        &self,
        context: &WebBackendRequestContext,
        query: &TrafficUsageStatisticsQuery,
    ) -> WebServiceResult<TrafficUsageStatisticsResponse>;

    /// Aggregated traffic usage of **every tenant this edge serves** — the
    /// operations reading.
    ///
    /// Restricted to the platform operator tenant; a tenant-bound context is
    /// rejected rather than narrowed to its own slice, so a misrouted admin
    /// page fails loudly instead of quietly rendering a single tenant's traffic
    /// as the platform total.
    async fn retrieve_platform_traffic_usage_statistics(
        &self,
        context: &WebBackendRequestContext,
        query: &TrafficUsageStatisticsQuery,
    ) -> WebServiceResult<TrafficUsageStatisticsResponse>;

    /// The dashboard metric summary of **the caller's own tenant** — the
    /// console reading.
    ///
    /// Reports the caller's own users, applications, and agents across every
    /// window, and deliberately no tenant count: a tenant counting itself is
    /// always one. Same reach rule as
    /// [`Self::retrieve_traffic_usage_statistics`] — the operation decides the
    /// scope, no parameter can widen it.
    ///
    /// `query` bounds the **series** only. The four card windows are resolved
    /// server-side from the clock and ignore it; see [`MetricsSummaryQuery`].
    async fn retrieve_metrics_summary(
        &self,
        context: &WebBackendRequestContext,
        query: &MetricsSummaryQuery,
    ) -> WebServiceResult<MetricsSummaryResponse>;

    /// The dashboard metric summary of **every tenant this edge serves** — the
    /// operations reading, and the only one that reports a tenant count.
    ///
    /// Restricted to the platform operator tenant; a tenant-bound context is
    /// rejected rather than narrowed, so a misrouted admin page fails loudly
    /// instead of presenting one tenant's estate as the platform's.
    async fn retrieve_platform_metrics_summary(
        &self,
        context: &WebBackendRequestContext,
        query: &MetricsSummaryQuery,
    ) -> WebServiceResult<MetricsSummaryResponse>;
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
