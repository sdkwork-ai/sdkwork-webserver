use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaChecksum {
    pub algorithm: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaResource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub kind: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "publicUrl", default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(
        rename = "objectBlobId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub object_blob_id: Option<String>,
    #[serde(rename = "fileName", default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(rename = "sizeBytes", default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<MediaChecksum>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(
        rename = "durationSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub duration_seconds: Option<f64>,
    #[serde(rename = "altText", default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationStoreListing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<MediaResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaResource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previews: Vec<MediaResource>,
    #[serde(
        rename = "shortDescription",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub short_description: Option<String>,
    #[serde(
        rename = "fullDescription",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub full_description: Option<String>,
    #[serde(
        rename = "releaseNotes",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub release_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(
        rename = "supportUrl",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub support_url: Option<String>,
    #[serde(
        rename = "privacyPolicyUrl",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub privacy_policy_url: Option<String>,
    #[serde(
        rename = "officialWebsiteUrl",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub official_website_url: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "applicationType")]
    pub application_type: String,
    #[serde(rename = "siteType")]
    pub site_type: i32,
    pub status: i32,
    #[serde(rename = "runtimeConfig", skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<Value>,
    #[serde(rename = "storeListing", skip_serializing_if = "Option::is_none")]
    pub store_listing: Option<ApplicationStoreListing>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationPage {
    pub items: Vec<ApplicationResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateApplicationRequest {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "applicationType", default = "default_application_type")]
    pub application_type: String,
    #[serde(rename = "siteType")]
    pub site_type: i32,
    #[serde(rename = "runtimeConfig", default)]
    pub runtime_config: Option<Value>,
    #[serde(rename = "storeListing", default)]
    pub store_listing: Option<ApplicationStoreListing>,
}

fn default_application_type() -> String {
    "WEB".to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateApplicationRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "runtimeConfig", default)]
    pub runtime_config: Option<Value>,
    #[serde(rename = "storeListing", default)]
    pub store_listing: Option<ApplicationStoreListing>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainDeploymentResponse {
    pub id: String,
    pub status: i32,
    pub environment: String,
    #[serde(rename = "versionTag", skip_serializing_if = "Option::is_none")]
    pub version_tag: Option<String>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainResponse {
    pub id: String,
    pub hostname: String,
    #[serde(rename = "rootDomainId", skip_serializing_if = "Option::is_none")]
    pub root_domain_id: Option<String>,
    #[serde(rename = "recordName", skip_serializing_if = "Option::is_none")]
    pub record_name: Option<String>,
    #[serde(rename = "applicationId", skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(rename = "applicationName", skip_serializing_if = "Option::is_none")]
    pub application_name: Option<String>,
    #[serde(rename = "certificateCount", with = "sdkwork_utils_rust::serde_int64")]
    pub certificate_count: i64,
    #[serde(rename = "isPrimary")]
    pub is_primary: bool,
    #[serde(rename = "isVerified")]
    pub is_verified: bool,
    #[serde(rename = "sslEnabled")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", skip_serializing_if = "Option::is_none")]
    pub ssl_provider: Option<String>,
    pub status: i32,
    #[serde(rename = "latestDeployment", skip_serializing_if = "Option::is_none")]
    pub latest_deployment: Option<DomainDeploymentResponse>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainPage {
    pub items: Vec<DomainResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RootDomainResponse {
    pub id: String,
    pub hostname: String,
    pub status: i32,
    #[serde(rename = "subdomainCount", with = "sdkwork_utils_rust::serde_int64")]
    pub subdomain_count: i64,
    #[serde(
        rename = "boundSubdomainCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub bound_subdomain_count: i64,
    #[serde(
        rename = "verifiedSubdomainCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub verified_subdomain_count: i64,
    #[serde(
        rename = "httpsSubdomainCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub https_subdomain_count: i64,
    #[serde(
        rename = "activeDeploymentCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub active_deployment_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RootDomainPage {
    pub items: Vec<RootDomainResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRootDomainRequest {
    pub hostname: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRootDomainHostnameRequest {
    #[serde(rename = "recordName")]
    pub record_name: String,
    #[serde(rename = "applicationId", default)]
    pub application_id: Option<String>,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
    #[serde(rename = "sslEnabled", default = "default_true")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", default)]
    pub ssl_provider: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDomainRequest {
    pub hostname: String,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
    #[serde(rename = "sslEnabled", default = "default_true")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", default)]
    pub ssl_provider: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateManagedDomainRequest {
    pub hostname: String,
    #[serde(rename = "applicationId", default)]
    pub application_id: Option<String>,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
    #[serde(rename = "sslEnabled", default = "default_true")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", default)]
    pub ssl_provider: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateDomainApplicationBindingRequest {
    #[serde(rename = "applicationId")]
    pub application_id: String,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
}

fn default_true() -> bool {
    true
}

pub(crate) fn default_page() -> i32 {
    1
}

pub(crate) fn default_page_size() -> i32 {
    20
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainVerifyResponse {
    pub verified: bool,
    pub status: String,
    pub method: String,
    #[serde(rename = "recordName")]
    pub record_name: String,
    #[serde(rename = "recordValue")]
    pub record_value: String,
    #[serde(rename = "attemptCount")]
    pub attempt_count: i32,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
    #[serde(rename = "nextAttemptAt", skip_serializing_if = "Option::is_none")]
    pub next_attempt_at: Option<String>,
    #[serde(rename = "checkedAt", skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
    #[serde(rename = "failureCode", skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVersionConfigSnapshot {
    #[serde(rename = "appConfigPath", default = "default_app_config_path")]
    pub app_config_path: String,
    #[serde(
        rename = "deploymentConfigPath",
        default = "default_deployment_config_path"
    )]
    pub deployment_config_path: String,
    #[serde(rename = "appConfigDetected", default)]
    pub app_config_detected: bool,
    #[serde(rename = "deploymentConfigDetected", default)]
    pub deployment_config_detected: bool,
}

fn default_app_config_path() -> String {
    "sdkwork.app.config.json".to_string()
}

fn default_deployment_config_path() -> String {
    "etc/sdkwork.deployment.config.json".to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceVersionResponse {
    pub id: String,
    #[serde(rename = "applicationId")]
    pub application_id: String,
    #[serde(rename = "versionTag")]
    pub version_tag: String,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceRef", skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(rename = "commitHash", skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(rename = "artifactDriveUri")]
    pub artifact_drive_uri: String,
    #[serde(rename = "artifactSize", with = "sdkwork_utils_rust::serde_int64")]
    pub artifact_size: i64,
    #[serde(rename = "artifactHash")]
    pub artifact_hash: String,
    #[serde(rename = "configSnapshot")]
    pub config_snapshot: SourceVersionConfigSnapshot,
    pub status: i32,
    pub retained: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceVersionPage {
    pub items: Vec<SourceVersionResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSourceVersionRequest {
    #[serde(rename = "versionTag")]
    pub version_tag: String,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceRef", default)]
    pub source_ref: Option<String>,
    #[serde(rename = "commitHash", default)]
    pub commit_hash: Option<String>,
    #[serde(rename = "artifactDriveUri")]
    pub artifact_drive_uri: String,
    #[serde(rename = "artifactSize", with = "sdkwork_utils_rust::serde_int64")]
    pub artifact_size: i64,
    #[serde(rename = "artifactHash")]
    pub artifact_hash: String,
    #[serde(rename = "configSnapshot", default)]
    pub config_snapshot: SourceVersionConfigSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportGitSourceVersionRequest {
    #[serde(rename = "versionTag")]
    pub version_tag: String,
    #[serde(rename = "repositoryUrl")]
    pub repository_url: String,
    #[serde(rename = "gitRef", default)]
    pub git_ref: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub id: String,
    #[serde(rename = "applicationId")]
    pub application_id: String,
    pub status: i32,
    #[serde(rename = "deployType")]
    pub deploy_type: i32,
    #[serde(rename = "sourceVersionId", skip_serializing_if = "Option::is_none")]
    pub source_version_id: Option<String>,
    pub environment: String,
    #[serde(rename = "versionTag", skip_serializing_if = "Option::is_none")]
    pub version_tag: Option<String>,
    #[serde(rename = "commitHash", skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(rename = "sourceRef", skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(
        rename = "rollbackFromDeploymentId",
        skip_serializing_if = "Option::is_none"
    )]
    pub rollback_from_deployment_id: Option<String>,
    #[serde(rename = "artifactDriveUri", skip_serializing_if = "Option::is_none")]
    pub artifact_drive_uri: Option<String>,
    #[serde(
        rename = "artifactSize",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub artifact_size: Option<i64>,
    #[serde(rename = "artifactHash", skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(
        rename = "durationMs",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub duration_ms: Option<i64>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeploymentPage {
    pub items: Vec<DeploymentResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDeploymentRequest {
    #[serde(rename = "deployType")]
    pub deploy_type: i32,
    #[serde(rename = "sourceVersionId", default)]
    pub source_version_id: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(rename = "versionTag", default)]
    pub version_tag: Option<String>,
    #[serde(rename = "commitHash", default)]
    pub commit_hash: Option<String>,
    #[serde(rename = "sourceRef", default)]
    pub source_ref: Option<String>,
    #[serde(rename = "artifactDriveUri", default)]
    pub artifact_drive_uri: Option<String>,
    #[serde(
        rename = "artifactSize",
        with = "sdkwork_utils_rust::serde_int64::option",
        default
    )]
    pub artifact_size: Option<i64>,
    #[serde(rename = "artifactHash", default)]
    pub artifact_hash: Option<String>,
    /// Framework-scoped idempotency identity used by the repository for durable deployment deduplication.
    /// This value is injected from the validated Header context and is never accepted from JSON input.
    #[serde(skip)]
    pub idempotency_key: Option<String>,
}

fn default_deploy_type() -> i32 {
    1
}

impl Default for CreateDeploymentRequest {
    fn default() -> Self {
        Self {
            deploy_type: default_deploy_type(),
            source_version_id: None,
            environment: None,
            version_tag: None,
            commit_hash: None,
            source_ref: None,
            artifact_drive_uri: None,
            artifact_size: None,
            artifact_hash: None,
            idempotency_key: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnvVariableResponse {
    pub id: String,
    pub key: String,
    pub value: String,
    pub environment: String,
    #[serde(rename = "isSecret")]
    pub is_secret: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnvVariablePage {
    pub items: Vec<EnvVariableResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEnvVariableRequest {
    pub key: String,
    pub value: String,
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
}

/// Environment variable rotation: replaces the stored value (encrypted when
/// secret) without changing the key or environment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateEnvVariableRequest {
    pub value: String,
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
}

fn default_environment() -> String {
    "production".to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateIdentifierResponse {
    #[serde(rename = "domainId")]
    pub domain_id: String,
    pub hostname: String,
    #[serde(rename = "identifierType")]
    pub identifier_type: String,
    pub position: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateResponse {
    pub id: String,
    #[serde(rename = "certName")]
    pub cert_name: String,
    pub identifiers: Vec<CertificateIdentifierResponse>,
    #[serde(rename = "certType", skip_serializing_if = "Option::is_none")]
    pub cert_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,
    #[serde(rename = "notBefore", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
    #[serde(rename = "autoRenew", skip_serializing_if = "Option::is_none")]
    pub auto_renew: Option<bool>,
    #[serde(rename = "renewalStatus", skip_serializing_if = "Option::is_none")]
    pub renewal_status: Option<String>,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateOperationAcceptedResponse {
    pub accepted: bool,
    pub operation_id: String,
    pub status: String,
}

/// Decrypted node-scoped TLS assignment material projected from the control
/// plane for the self-hosted TLS runtime snapshot. Private key material is
/// transported only inside the process boundary and never leaves the node.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsCertificateAssignmentMaterial {
    pub certificate_id: String,
    pub version_uuid: String,
    pub cert_name: String,
    pub hostnames: Vec<String>,
    pub fingerprint_sha256: String,
    pub not_before: String,
    pub not_after: String,
    pub fullchain_pem: String,
    pub private_key_pem: String,
}

/// Certificate revocation request. `reason` selects one of the RFC 5280
/// §5.3.1 revocation reasons accepted by the control plane.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeCertificateRequest {
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateOperationResponse {
    pub id: String,
    pub certificate_id: String,
    pub operation_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub next_attempt_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificatePage {
    pub items: Vec<CertificateResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCertificateRequest {
    #[serde(rename = "domainIds")]
    pub domain_ids: Vec<String>,
    #[serde(rename = "certType")]
    pub cert_type: i32,
    #[serde(rename = "keyAlgorithm", default = "default_certificate_key_algorithm")]
    pub key_algorithm: String,
    #[serde(rename = "autoRenew", default = "default_true")]
    pub auto_renew: bool,
}

fn default_certificate_key_algorithm() -> String {
    "ECDSA".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateCertificateRequest {
    #[serde(rename = "autoRenew")]
    pub auto_renew: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateListenerCertificateBindingRequest {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(
        rename = "certificateVersionId",
        skip_serializing_if = "Option::is_none"
    )]
    pub certificate_version_id: Option<String>,
    #[serde(default = "default_certificate_binding_priority")]
    pub priority: i32,
    #[serde(rename = "isDefault", default)]
    pub is_default: bool,
}

fn default_certificate_binding_priority() -> i32 {
    100
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListenerCertificateBindingResponse {
    pub id: String,
    #[serde(rename = "applicationId")]
    pub application_id: String,
    #[serde(rename = "domainId")]
    pub domain_id: String,
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "desiredCertificateVersionId")]
    pub desired_certificate_version_id: String,
    #[serde(
        rename = "currentCertificateVersionId",
        skip_serializing_if = "Option::is_none"
    )]
    pub current_certificate_version_id: Option<String>,
    #[serde(rename = "desiredCertificate")]
    pub desired_certificate: ListenerCertificateSummaryResponse,
    #[serde(rename = "currentCertificate", skip_serializing_if = "Option::is_none")]
    pub current_certificate: Option<ListenerCertificateSummaryResponse>,
    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,
    pub priority: i32,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    pub status: String,
    #[serde(rename = "activatedAt", skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListenerCertificateSummaryResponse {
    #[serde(rename = "certName")]
    pub cert_name: String,
    pub identifiers: Vec<CertificateIdentifierResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListenerCertificateBindingPage {
    pub items: Vec<ListenerCertificateBindingResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateDistributionResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,
    #[serde(rename = "serverName")]
    pub server_name: String,
    pub host: String,
    #[serde(rename = "desiredSyncVersion")]
    pub desired_sync_version: String,
    #[serde(rename = "appliedSyncVersion", skip_serializing_if = "Option::is_none")]
    pub applied_sync_version: Option<String>,
    pub status: String,
    #[serde(rename = "lastHeartbeatAt", skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateDistributionPage {
    pub items: Vec<CertificateDistributionResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug)]
pub struct CertificateIssueUpdate {
    pub cert_name: String,
    pub cert_type: i32,
    pub issuer: String,
    pub subject: String,
    pub serial_sha256: String,
    pub fingerprint_sha256: String,
    pub spki_sha256: String,
    pub chain_sha256: String,
    pub key_algorithm: String,
    pub fullchain_pem: String,
    pub private_key_pem: String,
    pub not_before: String,
    pub not_after: String,
    pub auto_renew: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub id: String,
    #[serde(rename = "checkType")]
    pub check_type: i32,
    #[serde(rename = "checkUrl")]
    pub check_url: String,
    #[serde(rename = "checkInterval")]
    pub check_interval: i32,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i32,
    #[serde(rename = "retryCount")]
    pub retry_count: i32,
    pub status: i32,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthCheckPage {
    pub items: Vec<HealthCheckResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHealthCheckRequest {
    #[serde(rename = "checkType")]
    pub check_type: i32,
    #[serde(rename = "checkUrl")]
    pub check_url: String,
    #[serde(rename = "checkInterval", default = "default_health_check_interval")]
    pub check_interval: i32,
    #[serde(rename = "timeoutMs", default = "default_health_check_timeout_ms")]
    pub timeout_ms: i32,
    #[serde(rename = "retryCount", default = "default_health_check_retry_count")]
    pub retry_count: i32,
}

fn default_health_check_interval() -> i32 {
    60
}

fn default_health_check_timeout_ms() -> i32 {
    5_000
}

fn default_health_check_retry_count() -> i32 {
    3
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NginxConfigResponse {
    pub id: String,
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "configName")]
    pub config_name: String,
    #[serde(rename = "configType")]
    pub config_type: i32,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    pub status: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NginxConfigPage {
    pub items: Vec<NginxConfigResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListNginxConfigsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(rename = "site_id", default)]
    pub site_id: Option<String>,
    #[serde(rename = "config_type", default)]
    pub config_type: Option<i32>,
    #[serde(rename = "is_active", default)]
    pub is_active: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateNginxConfigRequest {
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "configName")]
    pub config_name: String,
    #[serde(rename = "configType")]
    pub config_type: i32,
    #[serde(rename = "configContent")]
    pub config_content: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateNginxConfigRequest {
    #[serde(rename = "configName", default)]
    pub config_name: Option<String>,
    #[serde(rename = "configContent", default)]
    pub config_content: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxValidateResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxReloadResponse {
    pub reloaded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxStatusResponse {
    pub running: bool,
    #[serde(rename = "activeConfigs", with = "sdkwork_utils_rust::serde_int64")]
    pub active_configs: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerResponse {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(rename = "tenantScopeHash")]
    pub tenant_scope_hash: String,
    #[serde(rename = "sshPort")]
    pub ssh_port: i32,
    pub status: i32,
    #[serde(rename = "lastHeartbeatAt", skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateServerResponse {
    #[serde(flatten)]
    pub server: ServerResponse,
    #[serde(rename = "agentToken")]
    pub agent_token: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerPage {
    pub items: Vec<ServerResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateServerRequest {
    pub name: String,
    pub host: String,
    #[serde(rename = "tenantScopeHash")]
    pub tenant_scope_hash: String,
    #[serde(rename = "sshPort", default = "default_ssh_port")]
    pub ssh_port: i32,
}

fn default_ssh_port() -> i32 {
    22
}

#[derive(Clone, Debug)]
pub struct CertificateRenewalCandidate {
    pub tenant_id: i64,
    pub certificate_id: String,
    pub cert_type: i32,
    pub cert_name: String,
    pub hostnames: Vec<String>,
    pub key_algorithm: String,
    pub auto_renew: bool,
    pub not_after: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateOperationCycleReport {
    pub scheduled: usize,
    pub claimed: usize,
    pub succeeded: usize,
    pub retried: usize,
    pub failed: usize,
}

#[derive(Clone, Debug)]
pub struct CertificateOperationLease {
    pub tenant_id: i64,
    pub operation_id: String,
    pub certificate_id: String,
    pub operation_type: String,
    pub cert_type: i32,
    pub cert_name: String,
    pub hostnames: Vec<String>,
    pub key_algorithm: String,
    pub auto_renew: bool,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub lease_owner: String,
    pub fencing_token: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentHeartbeatRequest {
    #[serde(rename = "agentVersion", skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
    #[serde(rename = "nginxEnabled", skip_serializing_if = "Option::is_none")]
    pub nginx_enabled: Option<bool>,
    #[serde(
        rename = "activeConfigs",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub active_configs: Option<i64>,
    #[serde(rename = "lastSyncVersion", skip_serializing_if = "Option::is_none")]
    pub last_sync_version: Option<String>,
    #[serde(rename = "certificateObservations", default)]
    pub certificate_observations: Vec<AgentCertificateObservation>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCertificateObservation {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    pub fingerprint: String,
    #[serde(rename = "syncVersion")]
    pub sync_version: String,
    pub state: String,
    #[serde(rename = "observedAt")]
    pub observed_at: String,
    #[serde(rename = "failureCode", skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentHeartbeatResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,
    pub status: i32,
    #[serde(rename = "acknowledgedAt")]
    pub acknowledged_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentSyncResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,
    #[serde(rename = "syncVersion")]
    pub sync_version: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub unchanged: bool,
    #[serde(rename = "nginxConfigs")]
    pub nginx_configs: Vec<AgentNginxConfigBundle>,
    pub certificates: Vec<AgentCertificateBundle>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentNginxConfigBundle {
    #[serde(rename = "configId")]
    pub config_id: String,
    pub domain: String,
    #[serde(rename = "configContent")]
    pub config_content: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fingerprint: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub version: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentCertificateBundle {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "certName")]
    pub cert_name: String,
    pub fingerprint: String,
    pub hostnames: Vec<String>,
    #[serde(rename = "fullchainPem")]
    pub fullchain_pem: String,
    #[serde(rename = "privkeyPem")]
    pub privkey_pem: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditLogResponse {
    pub id: String,
    pub action: String,
    pub resource: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditLogPage {
    pub items: Vec<AuditLogResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_totals_serialize_as_decimal_strings() {
        let page = ApplicationPage {
            items: Vec::new(),
            total: 1_234_567_890_123,
            page: 1,
            page_size: 20,
        };
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["total"], serde_json::json!("1234567890123"));
        assert_eq!(json["page"], serde_json::json!(1));
    }

    #[test]
    fn agent_nginx_config_bundle_version_round_trips_as_string() {
        let bundle = AgentNginxConfigBundle {
            config_id: "cfg-1".into(),
            domain: "example.com".into(),
            config_content: "server {}".into(),
            fingerprint: "abc".into(),
            version: 9_876_543_210_987,
        };
        let json = serde_json::to_string(&bundle).unwrap();
        assert!(json.contains(r#""version":"9876543210987""#));
        let parsed: AgentNginxConfigBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, bundle.version);
    }

    #[test]
    fn agent_heartbeat_request_optional_int64_round_trips() {
        let request = AgentHeartbeatRequest {
            agent_version: Some("0.1".into()),
            nginx_enabled: Some(true),
            active_configs: Some(42),
            last_sync_version: Some("v1".into()),
            certificate_observations: Vec::new(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""activeConfigs":"42""#));
        let parsed: AgentHeartbeatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.active_configs, Some(42));
    }

    #[test]
    fn rejects_non_numeric_int64_string_input() {
        let json = r#"{"items":[],"total":"not-a-number","page":1,"pageSize":20}"#;
        let result: Result<ApplicationPage, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
