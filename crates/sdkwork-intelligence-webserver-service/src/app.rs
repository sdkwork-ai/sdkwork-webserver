//! App-api service surface implementation.

use async_trait::async_trait;
use sdkwork_webserver_contract::{
    ApplicationStoreListing, CreatePlatformTargetRequest, CreateApplicationRequest, CreateDeploymentRequest,
    CreateDomainRequest, CreateEnvVariableRequest, CreateHealthCheckRequest,
    CreateListenerCertificateBindingRequest, CreateSourceVersionRequest,
    ImportGitSourceVersionRequest, IssueCertificateRequest, ListApplicationsQuery, MediaResource,
    UpdateApplicationRequest, WebAppApi, WebAppRequestContext, WebAppResourceScope,
    WebServiceError, WebServiceResult,
};
use std::collections::HashSet;

use crate::{AuditLogWrite, GitSourceImportRequest, WebService};

const MAX_DEPLOYMENT_ARTIFACT_BYTES: i64 = 64 * 1024 * 1024;
const MAX_ENV_VARIABLE_VALUE_BYTES: usize = 64 * 1024;
const MAX_ICON_BYTES: u64 = 2 * 1024 * 1024;
const MAX_STORE_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_STORE_PREVIEWS: usize = 8;
const MAX_CERTIFICATE_IDENTIFIERS: usize = 8;
const DEFAULT_SOURCE_VERSION_RETENTION_LIMIT: i32 = 5;
const MAX_SOURCE_VERSION_RETENTION_LIMIT: i32 = 50;

impl WebService {
    fn require_tenant(context: &WebAppRequestContext) -> WebServiceResult<i64> {
        if context.tenant_id <= 0 {
            return Err(sdkwork_webserver_contract::WebServiceError::Forbidden);
        }
        Ok(context.tenant_id)
    }

    pub(crate) fn owner_filter(context: &WebAppRequestContext) -> WebServiceResult<Option<i64>> {
        match context.resource_scope {
            WebAppResourceScope::Owner => context
                .actor_id
                .filter(|actor_id| *actor_id > 0)
                .map(Some)
                .ok_or(sdkwork_webserver_contract::WebServiceError::Forbidden),
            WebAppResourceScope::Tenant => Ok(None),
        }
    }

    async fn require_application_access(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<(i64, String)> {
        let tenant_id = Self::require_tenant(context)?;
        let owner_id = Self::owner_filter(context)?;
        self.repository
            .retrieve_application(tenant_id, owner_id, application_id)
            .await?;
        let site_id = self
            .repository
            .resolve_site_id(tenant_id, application_id)
            .await?;
        Ok((tenant_id, site_id))
    }

    pub(crate) fn validate_app_kind(value: &str) -> WebServiceResult<()> {
        if sdkwork_webserver_contract::AppKind::parse(value).is_some() {
            return Ok(());
        }
        Err(sdkwork_webserver_contract::WebServiceError::validation(
            "appKind must be one of STATIC_WEB, SPA_WEB, API_SERVICE, WECHAT_MINIPROGRAM, DOUYIN_MINIPROGRAM, IOS_APP, ANDROID_APP, HARMONYOS_APP",
        ))
    }

    pub(crate) fn validate_platform_target_request(
        request: &CreatePlatformTargetRequest,
    ) -> WebServiceResult<()> {
        use sdkwork_webserver_contract::{Platform, WebServiceError};
        let platform = Platform::parse(&request.platform).ok_or_else(|| {
            WebServiceError::validation(
                "platform must be one of WEB, API, WECHAT, DOUYIN, IOS, ANDROID, HARMONYOS",
            )
        })?;
        if request
            .tech_stack
            .as_deref()
            .is_some_and(|stack| sdkwork_webserver_contract::TechStack::parse(stack).is_none())
        {
            return Err(WebServiceError::validation(
                "techStack must be one of FLUTTER, NATIVE, UNI_APP, NODE, RUST, GO, JAVA, OTHER",
            ));
        }
        if request.target_key.trim().is_empty() || request.target_key.len() > 120 {
            return Err(WebServiceError::validation(
                "targetKey must contain 1..120 characters",
            ));
        }
        // Platform identity is mandatory per platform (mirrors the deploy
        // module platform-target contract).
        let identity_ok = match platform {
            Platform::Ios => request
                .bundle_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            Platform::Android => request
                .package_name
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            Platform::Wechat | Platform::Douyin => request
                .app_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            Platform::Harmonyos => request
                .bundle_name
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            Platform::Web | Platform::Api => true,
        };
        if !identity_ok {
            let field = match platform {
                Platform::Ios => "bundleId",
                Platform::Android => "packageName",
                Platform::Wechat | Platform::Douyin => "appId",
                Platform::Harmonyos => "bundleName",
                Platform::Web | Platform::Api => "",
            };
            return Err(WebServiceError::validation(format!(
                "{field} is required for platform {}",
                platform.as_str()
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_store_listing(
        listing: Option<&ApplicationStoreListing>,
        require_icon: bool,
    ) -> WebServiceResult<()> {
        let Some(listing) = listing else {
            return if require_icon {
                Err(sdkwork_webserver_contract::WebServiceError::conflict(
                    "a square 1:1 PNG application icon is required before deployment or activation",
                ))
            } else {
                Ok(())
            };
        };

        if require_icon && listing.icon.is_none() {
            return Err(sdkwork_webserver_contract::WebServiceError::conflict(
                "a square 1:1 PNG application icon is required before deployment or activation",
            ));
        }
        if let Some(icon) = listing.icon.as_ref() {
            validate_square_store_image(icon, "storeListing.icon", &["image/png"], MAX_ICON_BYTES)?;
        }
        if let Some(cover) = listing.cover.as_ref() {
            validate_store_image(
                cover,
                "storeListing.cover",
                1024,
                500,
                &["image/png", "image/jpeg", "image/webp"],
                MAX_STORE_IMAGE_BYTES,
            )?;
        }
        if listing.previews.len() > MAX_STORE_PREVIEWS {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "storeListing.previews must contain at most 8 images",
            ));
        }
        let mut preview_ids = HashSet::with_capacity(listing.previews.len());
        for (index, preview) in listing.previews.iter().enumerate() {
            validate_preview_image(preview, index)?;
            let id = preview.id.as_deref().unwrap_or_default();
            if !preview_ids.insert(id) {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "storeListing.previews must not contain duplicate Drive resources",
                ));
            }
        }

        validate_store_text(
            "storeListing.shortDescription",
            listing.short_description.as_deref(),
            80,
        )?;
        validate_store_text(
            "storeListing.fullDescription",
            listing.full_description.as_deref(),
            4_000,
        )?;
        validate_store_text(
            "storeListing.releaseNotes",
            listing.release_notes.as_deref(),
            4_000,
        )?;
        validate_store_text("storeListing.category", listing.category.as_deref(), 80)?;
        if listing.keywords.len() > 10 {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "storeListing.keywords must contain at most 10 values",
            ));
        }
        let mut keywords = HashSet::with_capacity(listing.keywords.len());
        for keyword in &listing.keywords {
            validate_store_text("storeListing.keywords", Some(keyword), 40)?;
            if !keywords.insert(keyword.to_lowercase()) {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "storeListing.keywords must not contain duplicates",
                ));
            }
        }
        for (field, value) in [
            ("storeListing.supportUrl", listing.support_url.as_deref()),
            (
                "storeListing.privacyPolicyUrl",
                listing.privacy_policy_url.as_deref(),
            ),
            (
                "storeListing.officialWebsiteUrl",
                listing.official_website_url.as_deref(),
            ),
        ] {
            validate_store_https_url(field, value)?;
        }
        Ok(())
    }

    pub(crate) fn validate_deployment_request(
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<()> {
        if !matches!(request.deploy_type, 1..=4) {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "deployType must be 1 (manual), 2 (git), 3 (ci-cd), or 4 (api)",
            ));
        }
        validate_idempotency_key(request.idempotency_key.as_deref())?;
        if let Some(environment) = request.environment.as_deref() {
            if environment != environment.trim()
                || !matches!(
                    environment,
                    "development" | "test" | "staging" | "production"
                )
            {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "environment must be development, test, staging, or production",
                ));
            }
        }

        validate_optional_deployment_text("versionTag", request.version_tag.as_deref(), 100)?;
        validate_optional_deployment_text("sourceRef", request.source_ref.as_deref(), 500)?;
        if let Some(source_version_id) = request.source_version_id.as_deref() {
            validate_resource_id("sourceVersionId", source_version_id)?;
        }
        if let Some(commit_hash) = request.commit_hash.as_deref() {
            let hash = commit_hash.trim();
            if hash != commit_hash
                || !(7..=64).contains(&hash.len())
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "commitHash must be a 7..64 character lowercase hexadecimal digest",
                ));
            }
        }

        let artifact_fields = [
            request.artifact_drive_uri.is_some(),
            request.artifact_size.is_some(),
            request.artifact_hash.is_some(),
        ];
        let artifact_count = artifact_fields
            .into_iter()
            .filter(|present| *present)
            .count();
        if artifact_count != 0 && artifact_count != artifact_fields.len() {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "artifactDriveUri, artifactSize, and artifactHash must be provided together",
            ));
        }
        if request.source_version_id.is_some() {
            if artifact_count != 0 || request.source_ref.is_some() || request.commit_hash.is_some()
            {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "sourceVersionId cannot be combined with source or artifact fields",
                ));
            }
        } else if request.deploy_type == 2 {
            let source_ref = request.source_ref.as_deref().ok_or_else(|| {
                sdkwork_webserver_contract::WebServiceError::validation(
                    "sourceRef is required for Git deployments",
                )
            })?;
            validate_git_repository_url(source_ref)?;
        } else if artifact_count == 0 {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "artifactDriveUri, artifactSize, and artifactHash are required for non-Git deployments",
            ));
        }

        if let Some(uri) = request.artifact_drive_uri.as_deref() {
            if parse_drive_uri(uri).is_none() {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "artifactDriveUri must use drive://spaces/{spaceId}/nodes/{nodeId}",
                ));
            }
        }
        if request
            .artifact_size
            .is_some_and(|size| !(1..=MAX_DEPLOYMENT_ARTIFACT_BYTES).contains(&size))
        {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "artifactSize must be between 1 byte and 64 MiB",
            ));
        }
        if let Some(hash) = request.artifact_hash.as_deref() {
            let hash = hash.trim();
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "artifactHash must be a lowercase SHA-256 hexadecimal digest",
                ));
            }
        }
        Ok(())
    }

    fn validate_source_version_request(
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<()> {
        validate_required_text("versionTag", &request.version_tag, 100)?;
        if !matches!(request.source_type.as_str(), "ARCHIVE" | "DIRECTORY") {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "sourceType must be ARCHIVE or DIRECTORY; Git sources use the import endpoint",
            ));
        }
        validate_optional_deployment_text("sourceRef", request.source_ref.as_deref(), 500)?;
        if parse_drive_uri(&request.artifact_drive_uri).is_none() {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "artifactDriveUri must use drive://spaces/{spaceId}/nodes/{nodeId}",
            ));
        }
        if !(1..=MAX_DEPLOYMENT_ARTIFACT_BYTES).contains(&request.artifact_size) {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "artifactSize must be between 1 byte and 64 MiB",
            ));
        }
        validate_sha256("artifactHash", &request.artifact_hash)
    }

    fn source_version_retention_limit(
        runtime_config: Option<&serde_json::Value>,
    ) -> WebServiceResult<i32> {
        let Some(value) =
            runtime_config.and_then(|config| config.get("sourceVersionRetentionLimit"))
        else {
            return Ok(DEFAULT_SOURCE_VERSION_RETENTION_LIMIT);
        };
        let limit = value.as_i64().ok_or_else(|| {
            sdkwork_webserver_contract::WebServiceError::validation(
                "sourceVersionRetentionLimit must be an integer",
            )
        })?;
        if !(1..=i64::from(MAX_SOURCE_VERSION_RETENTION_LIMIT)).contains(&limit) {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "sourceVersionRetentionLimit must be between 1 and 50",
            ));
        }
        Ok(limit as i32)
    }

    pub(crate) fn validate_health_check_request(
        request: &CreateHealthCheckRequest,
    ) -> WebServiceResult<()> {
        if !matches!(request.check_type, 1..=3) {
            return Err(WebServiceError::validation(
                "checkType must be 1 (HTTP), 2 (TCP), or 3 (ping)",
            ));
        }
        if request.check_url.is_empty()
            || request.check_url != request.check_url.trim()
            || request.check_url.len() > 2_000
            || request.check_url.chars().any(char::is_control)
        {
            return Err(WebServiceError::validation(
                "checkUrl must contain 1..2000 non-control characters",
            ));
        }
        if request.check_type == 1 {
            // HTTP checks run from the platform, so the target URL must be a
            // credentialed-free http(s) URL without query or fragment to avoid
            // turning a stored target into a platform-side SSRF primitive.
            let parsed = url::Url::parse(&request.check_url).ok();
            if parsed.as_ref().is_none_or(|parsed| {
                !matches!(parsed.scheme(), "http" | "https")
                    || parsed.host_str().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                    || parsed.query().is_some()
                    || parsed.fragment().is_some()
            }) {
                return Err(WebServiceError::validation(
                    "checkUrl must be an HTTP(S) URL without credentials, query parameters, or fragments",
                ));
            }
        }
        if !(5..=86_400).contains(&request.check_interval) {
            return Err(WebServiceError::validation(
                "checkInterval must be between 5 and 86400 seconds",
            ));
        }
        if !(100..=60_000).contains(&request.timeout_ms)
            || i64::from(request.timeout_ms) > i64::from(request.check_interval) * 1_000
        {
            return Err(WebServiceError::validation(
                "timeoutMs must be between 100 and 60000 and not exceed checkInterval",
            ));
        }
        if !(0..=10).contains(&request.retry_count) {
            return Err(WebServiceError::validation(
                "retryCount must be between 0 and 10",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_domain_request(request: &CreateDomainRequest) -> WebServiceResult<()> {
        let hostname = request.hostname.as_str();
        if hostname.is_empty()
            || hostname != hostname.trim()
            || hostname.len() > 253
            || hostname.starts_with('.')
            || hostname.ends_with('.')
            || hostname.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "hostname must be a safe ASCII DNS name",
            ));
        }
        if request
            .ssl_provider
            .as_deref()
            .is_some_and(|provider| !matches!(provider, "letsencrypt" | "custom" | "none"))
        {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "sslProvider must be letsencrypt, custom, or none",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_env_variable_request(
        request: &CreateEnvVariableRequest,
    ) -> WebServiceResult<()> {
        if request.key.is_empty()
            || request.key.len() > 200
            || !request.key.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
            })
        {
            return Err(WebServiceError::validation(
                "key must be a 1..200 character environment variable name",
            ));
        }
        if !matches!(
            request.environment.as_str(),
            "development" | "test" | "staging" | "production"
        ) {
            return Err(WebServiceError::validation(
                "environment must be development, test, staging, or production",
            ));
        }
        Self::validate_env_variable_value(&request.value)
    }

    pub(crate) fn validate_env_variable_value(value: &str) -> WebServiceResult<()> {
        if value.len() > MAX_ENV_VARIABLE_VALUE_BYTES || value.contains('\0') {
            return Err(WebServiceError::validation(
                "value must not exceed 64 KiB or contain NUL",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_certificate_issue_request(
        request: &IssueCertificateRequest,
    ) -> WebServiceResult<()> {
        if !matches!(request.cert_type, 1 | 3) {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "certType must be 1 (Let's Encrypt) or 3 (self-signed)",
            ));
        }
        if request.cert_type == 3 && request.auto_renew {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "automatic renewal is unavailable for self-signed certificates",
            ));
        }
        if request.domain_ids.is_empty() || request.domain_ids.len() > MAX_CERTIFICATE_IDENTIFIERS {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "domainIds must contain between 1 and 8 identifiers",
            ));
        }
        let mut unique_domain_ids = HashSet::with_capacity(request.domain_ids.len());
        for domain_id in &request.domain_ids {
            if domain_id.is_empty()
                || domain_id != domain_id.trim()
                || domain_id.len() > 64
                || domain_id.chars().any(char::is_control)
                || !unique_domain_ids.insert(domain_id)
            {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "domainIds must contain unique valid identifiers",
                ));
            }
        }
        if !matches!(request.key_algorithm.as_str(), "ECDSA" | "RSA") {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "keyAlgorithm must be ECDSA or RSA",
            ));
        }
        Ok(())
    }

    async fn audit_site_action(
        &self,
        context: &WebAppRequestContext,
        action: &str,
        target_uuid: &str,
    ) {
        let operator_id = context.actor_id.unwrap_or(0);
        let _ = self
            .record_audit_log(AuditLogWrite {
                tenant_id: context.tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                operator_id,
                operator_type: "USER",
                action,
                target_type: "site",
                target_id: None,
                target_uuid: Some(target_uuid),
                request_id: None,
                metadata_json: "{}",
            })
            .await;
    }
}

fn validate_optional_deployment_text(
    field: &str,
    value: Option<&str>,
    max_characters: usize,
) -> WebServiceResult<()> {
    if let Some(value) = value {
        if value.is_empty()
            || value != value.trim()
            || value.chars().count() > max_characters
            || value.chars().any(char::is_control)
        {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                format!("{field} must contain 1..{max_characters} non-control characters"),
            ));
        }
    }
    Ok(())
}

fn validate_required_text(field: &str, value: &str, max_characters: usize) -> WebServiceResult<()> {
    validate_optional_deployment_text(field, Some(value), max_characters)
}

fn validate_resource_id(field: &str, value: &str) -> WebServiceResult<()> {
    if value != value.trim()
        || !(1..=128).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must be a safe resource identifier"),
        ));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> WebServiceResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must be a lowercase SHA-256 hexadecimal digest"),
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: Option<&str>) -> WebServiceResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if value != value.trim() || !(1..=128).contains(&value.len()) {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            "idempotency key must contain between 1 and 128 bytes without surrounding whitespace",
        ));
    }
    Ok(())
}

fn validate_store_image(
    resource: &MediaResource,
    field: &str,
    expected_width: i32,
    expected_height: i32,
    accepted_mime_types: &[&str],
    maximum_bytes: u64,
) -> WebServiceResult<()> {
    validate_drive_image(resource, field, accepted_mime_types, maximum_bytes)?;
    if resource.width != Some(expected_width) || resource.height != Some(expected_height) {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must be {expected_width}x{expected_height} pixels"),
        ));
    }
    Ok(())
}

fn validate_square_store_image(
    resource: &MediaResource,
    field: &str,
    accepted_mime_types: &[&str],
    maximum_bytes: u64,
) -> WebServiceResult<()> {
    validate_drive_image(resource, field, accepted_mime_types, maximum_bytes)?;
    let (Some(width), Some(height)) = (resource.width, resource.height) else {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must include width and height"),
        ));
    };
    if width != height {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must be square with a 1:1 aspect ratio"),
        ));
    }
    Ok(())
}

fn validate_preview_image(resource: &MediaResource, index: usize) -> WebServiceResult<()> {
    let field = format!("storeListing.previews[{index}]");
    validate_drive_image(
        resource,
        &field,
        &["image/png", "image/jpeg", "image/webp"],
        MAX_STORE_IMAGE_BYTES,
    )?;
    let (Some(width), Some(height)) = (resource.width, resource.height) else {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must include width and height"),
        ));
    };
    if !(320..=3_840).contains(&width)
        || !(320..=3_840).contains(&height)
        || i64::from(width.max(height)) > i64::from(width.min(height)) * 5 / 2
    {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} dimensions must be 320..3840 pixels with an aspect ratio no greater than 2.5:1"),
        ));
    }
    Ok(())
}

fn validate_drive_image(
    resource: &MediaResource,
    field: &str,
    accepted_mime_types: &[&str],
    maximum_bytes: u64,
) -> WebServiceResult<()> {
    if resource.kind != "image" || resource.source != "drive" {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must be a Drive-backed image MediaResource"),
        ));
    }
    if resource.url.is_some() || resource.public_url.is_some() {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must not persist delivery or presigned URLs"),
        ));
    }
    let uri = resource.uri.as_deref().ok_or_else(|| {
        sdkwork_webserver_contract::WebServiceError::validation(format!("{field}.uri is required"))
    })?;
    let (space_id, node_id) = parse_drive_uri(uri).ok_or_else(|| {
        sdkwork_webserver_contract::WebServiceError::validation(format!(
            "{field}.uri must use drive://spaces/{{spaceId}}/nodes/{{nodeId}}"
        ))
    })?;
    if resource.id.as_deref() != Some(node_id) {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field}.id must equal the Drive node id"),
        ));
    }
    let mime_type = resource.mime_type.as_deref().ok_or_else(|| {
        sdkwork_webserver_contract::WebServiceError::validation(format!(
            "{field}.mimeType is required"
        ))
    })?;
    if !accepted_mime_types.contains(&mime_type) {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field}.mimeType is not supported"),
        ));
    }
    resource
        .size_bytes
        .as_deref()
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|size| (1..=maximum_bytes).contains(size))
        .ok_or_else(|| {
            sdkwork_webserver_contract::WebServiceError::validation(format!(
                "{field}.sizeBytes must be between 1 and {maximum_bytes}"
            ))
        })?;
    if let Some(file_name) = resource.file_name.as_deref() {
        validate_store_text(&format!("{field}.fileName"), Some(file_name), 512)?;
    }
    if let Some(alt_text) = resource.alt_text.as_deref() {
        validate_store_text(&format!("{field}.altText"), Some(alt_text), 512)?;
    }
    if let Some(title) = resource.title.as_deref() {
        validate_store_text(&format!("{field}.title"), Some(title), 255)?;
    }
    if let Some(checksum) = resource.checksum.as_ref() {
        if checksum.algorithm != "sha256"
            || checksum.value.len() != 64
            || !checksum
                .value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                format!("{field}.checksum must be a lowercase sha256 digest"),
            ));
        }
    }
    let drive = resource
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("drive"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            sdkwork_webserver_contract::WebServiceError::validation(format!(
                "{field}.metadata.drive is required"
            ))
        })?;
    if drive.get("spaceId").and_then(serde_json::Value::as_str) != Some(space_id)
        || drive.get("nodeId").and_then(serde_json::Value::as_str) != Some(node_id)
    {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field}.metadata.drive must match the stable Drive URI"),
        ));
    }
    Ok(())
}

fn parse_drive_uri(uri: &str) -> Option<(&str, &str)> {
    let (space_id, node_id) = uri.strip_prefix("drive://spaces/")?.split_once("/nodes/")?;
    if space_id.is_empty()
        || node_id.is_empty()
        || space_id.contains('/')
        || node_id.contains('/')
        || uri.contains(['?', '#'])
        || uri.len() > 500
        || !space_id.bytes().all(is_safe_drive_identifier_byte)
        || !node_id.bytes().all(is_safe_drive_identifier_byte)
    {
        return None;
    }
    Some((space_id, node_id))
}

fn validate_store_text(
    field: &str,
    value: Option<&str>,
    maximum_chars: usize,
) -> WebServiceResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if value != value.trim()
        || value.is_empty()
        || value.chars().count() > maximum_chars
        || value.chars().any(char::is_control)
    {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must contain 1..{maximum_chars} trimmed characters without control characters"),
        ));
    }
    Ok(())
}

fn validate_store_https_url(field: &str, value: Option<&str>) -> WebServiceResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let parsed = url::Url::parse(value).ok();
    if value != value.trim()
        || value.len() > 2_000
        || parsed.as_ref().is_none_or(|parsed| {
            parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.fragment().is_some()
        })
    {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            format!("{field} must be an HTTPS URL without credentials or fragments"),
        ));
    }
    Ok(())
}

fn validate_git_repository_url(value: &str) -> WebServiceResult<()> {
    let parsed = url::Url::parse(value).ok();
    if parsed.as_ref().is_none_or(|parsed| {
        parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || parsed.path().is_empty()
            || parsed.path() == "/"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
    }) {
        return Err(sdkwork_webserver_contract::WebServiceError::validation(
            "sourceRef must be an HTTPS Git repository URL without credentials, query parameters, or fragments",
        ));
    }
    Ok(())
}

fn is_safe_drive_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
}

#[cfg(test)]
mod resource_scope_tests {
    use super::*;
    use sdkwork_webserver_contract::WebServiceError;

    #[test]
    fn owner_scope_requires_a_valid_actor() {
        let context = WebAppRequestContext {
            tenant_id: 1,
            resource_scope: WebAppResourceScope::Owner,
            ..WebAppRequestContext::default()
        };

        assert!(matches!(
            WebService::owner_filter(&context),
            Err(WebServiceError::Forbidden)
        ));
    }

    #[test]
    fn tenant_scope_does_not_apply_an_owner_filter() {
        let context = WebAppRequestContext {
            tenant_id: 1,
            resource_scope: WebAppResourceScope::Tenant,
            ..WebAppRequestContext::default()
        };

        assert_eq!(WebService::owner_filter(&context).unwrap(), None);
    }
}

#[async_trait]
impl WebAppApi for WebService {
    async fn list_applications(
        &self,
        context: &WebAppRequestContext,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationPage> {
        let tenant_id = Self::require_tenant(context)?;
        if let Some(application_type) = query.application_type.as_deref() {
            Self::validate_app_kind(application_type)?;
        }
        let owner_id = Self::owner_filter(context)?;
        self.repository
            .list_applications(tenant_id, owner_id, query)
            .await
    }

    async fn create_application(
        &self,
        context: &WebAppRequestContext,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let owner_id = Self::owner_filter(context)?;
        Self::validate_app_kind(&request.app_kind)?;
        Self::validate_store_listing(request.store_listing.as_ref(), false)?;
        let site = self
            .repository
            .create_application(tenant_id, context.organization_id, owner_id, request)
            .await?;
        self.audit_site_action(context, "applications.create", &site.id)
            .await;
        Ok(site)
    }

    async fn retrieve_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let owner_id = Self::owner_filter(context)?;
        self.repository
            .retrieve_application(tenant_id, owner_id, application_id)
            .await
    }

    async fn update_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        Self::validate_store_listing(request.store_listing.as_ref(), false)?;
        let tenant_id = self
            .require_application_access(context, application_id)
            .await?
            .0;
        let site = self
            .repository
            .update_application(tenant_id, application_id, request)
            .await?;
        self.audit_site_action(context, "applications.update", application_id)
            .await;
        Ok(site)
    }

    async fn delete_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<()> {
        let tenant_id = self
            .require_application_access(context, application_id)
            .await?
            .0;
        self.repository
            .delete_application(tenant_id, application_id, context.actor_id)
            .await?;
        self.audit_site_action(context, "applications.delete", application_id)
            .await;
        Ok(())
    }

    async fn activate_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let owner_id = Self::owner_filter(context)?;
        let site = self
            .repository
            .retrieve_application(tenant_id, owner_id, application_id)
            .await?;
        Self::validate_store_listing(site.store_listing.as_ref(), true)?;
        let (_, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        let successful_deployments = self
            .repository
            .list_deployments(tenant_id, &site_id, 1, 1, Some(2), None)
            .await?;
        if successful_deployments.total == 0 {
            return Err(sdkwork_webserver_contract::WebServiceError::conflict(
                "at least one successful deployment is required before activation",
            ));
        }
        let site = self
            .repository
            .set_application_status(tenant_id, application_id, 1)
            .await?;
        self.audit_site_action(context, "applications.activate", application_id)
            .await;
        Ok(site)
    }

    async fn pause_application(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        let tenant_id = self
            .require_application_access(context, application_id)
            .await?
            .0;
        let site = self
            .repository
            .set_application_status(tenant_id, application_id, 2)
            .await?;
        self.audit_site_action(context, "applications.pause", application_id)
            .await;
        Ok(site)
    }

    async fn list_domains(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainPage> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .list_domains(tenant_id, &site_id, page, page_size)
            .await
    }

    async fn list_certificate_domains(
        &self,
        context: &WebAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainPage> {
        if context.tenant_id <= 0 {
            return Err(sdkwork_webserver_contract::WebServiceError::Forbidden);
        }
        let owner_id = Self::owner_filter(context)?;
        self.repository
            .list_certificate_domains(context.tenant_id, owner_id, page, page_size)
            .await
    }

    async fn create_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        Self::validate_domain_request(request)?;
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .create_domain(tenant_id, &site_id, request)
            .await
    }

    async fn retrieve_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .retrieve_domain(tenant_id, &site_id, domain_id)
            .await
    }

    async fn delete_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .delete_domain(tenant_id, &site_id, domain_id)
            .await
    }

    async fn verify_domain(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainVerifyResponse> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        let challenge = self
            .repository
            .prepare_domain_verification(tenant_id, &site_id, domain_id)
            .await?;
        self.execute_domain_verification(tenant_id, challenge).await
    }

    async fn list_source_versions(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionPage> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .list_source_versions(tenant_id, &site_id, page, page_size, cursor)
            .await
    }

    async fn create_source_version(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionResponse> {
        Self::validate_source_version_request(request)?;
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        let site = self
            .repository
            .retrieve_application(tenant_id, None, application_id)
            .await?;
        let retention_limit = Self::source_version_retention_limit(site.runtime_config.as_ref())?;
        self.repository
            .create_source_version(
                tenant_id,
                &site_id,
                context.actor_id,
                retention_limit,
                request,
            )
            .await
    }

    async fn import_git_source_version(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &ImportGitSourceVersionRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionResponse> {
        validate_required_text("versionTag", &request.version_tag, 100)?;
        validate_git_repository_url(&request.repository_url)?;
        if let Some(git_ref) = request.git_ref.as_deref() {
            if git_ref != git_ref.trim()
                || !(1..=200).contains(&git_ref.len())
                || git_ref.chars().any(char::is_control)
                || git_ref.starts_with('-')
            {
                return Err(sdkwork_webserver_contract::WebServiceError::validation(
                    "gitRef must contain 1..200 safe characters",
                ));
            }
        }
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        let site = self
            .repository
            .retrieve_application(tenant_id, None, application_id)
            .await?;
        let imported = self
            .source_importer
            .import_git(&GitSourceImportRequest {
                tenant_id,
                organization_id: context.organization_id,
                actor_id: context.actor_id,
                application_id: site_id.to_string(),
                version_tag: request.version_tag.clone(),
                repository_url: request.repository_url.clone(),
                git_ref: request.git_ref.clone(),
            })
            .await?;
        let retention_limit = Self::source_version_retention_limit(site.runtime_config.as_ref())?;
        self.repository
            .create_source_version(
                tenant_id,
                &site_id,
                context.actor_id,
                retention_limit,
                &CreateSourceVersionRequest {
                    version_tag: request.version_tag.clone(),
                    source_type: "GIT".to_string(),
                    source_ref: Some(request.repository_url.clone()),
                    commit_hash: Some(imported.commit_hash),
                    artifact_drive_uri: imported.artifact_drive_uri,
                    artifact_size: imported.artifact_size,
                    artifact_hash: imported.artifact_hash,
                    config_snapshot: imported.config_snapshot,
                },
            )
            .await
    }

    async fn retrieve_source_version(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        source_version_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::SourceVersionResponse> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .retrieve_source_version(tenant_id, &site_id, source_version_id)
            .await
    }

    async fn list_deployments(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentPage> {
        if status.is_some_and(|status| !(0..=6).contains(&status)) {
            return Err(sdkwork_webserver_contract::WebServiceError::validation(
                "status must be between 0 and 6",
            ));
        }
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .list_deployments(tenant_id, &site_id, page, page_size, status, cursor)
            .await
    }

    async fn create_deployment(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        let mut request = request.clone();
        if let Some(idempotency_key) = &context.idempotency_key {
            request.idempotency_key = Some(idempotency_key.clone());
        }
        Self::validate_deployment_request(&request)?;
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        let site = self
            .repository
            .retrieve_application(tenant_id, None, application_id)
            .await?;
        Self::validate_store_listing(site.store_listing.as_ref(), true)?;
        if let Some(source_version_id) = request.source_version_id.as_deref() {
            let source_version = self
                .repository
                .retrieve_source_version(tenant_id, &site_id, source_version_id)
                .await?;
            if source_version.status != 1 || !source_version.retained {
                return Err(sdkwork_webserver_contract::WebServiceError::conflict(
                    "source version is not ready or is outside the retained release window",
                ));
            }
            request.deploy_type = if source_version.source_type == "GIT" {
                2
            } else {
                1
            };
            request.version_tag = request
                .version_tag
                .or_else(|| Some(source_version.version_tag.clone()));
            request.commit_hash = source_version.commit_hash;
            request.source_ref = source_version.source_ref;
            request.artifact_drive_uri = Some(source_version.artifact_drive_uri);
            request.artifact_size = Some(source_version.artifact_size);
            request.artifact_hash = Some(source_version.artifact_hash);
        }
        self.repository
            .create_deployment(tenant_id, &site_id, context.actor_id, &request)
            .await
    }

    async fn retrieve_deployment(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .retrieve_deployment(tenant_id, &site_id, deployment_id)
            .await
    }

    async fn rollback_deployment(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        validate_idempotency_key(context.idempotency_key.as_deref())?;
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .rollback_deployment(
                tenant_id,
                &site_id,
                deployment_id,
                context.actor_id,
                context.idempotency_key.as_deref(),
            )
            .await
    }

    async fn list_env_variables(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        environment: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::EnvVariablePage> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .list_env_variables(tenant_id, &site_id, environment)
            .await
    }

    async fn create_env_variable(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::EnvVariableResponse> {
        Self::validate_env_variable_request(request)?;
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .create_env_variable(tenant_id, &site_id, request)
            .await
    }

    async fn update_env_variable(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        variable_id: &str,
        request: &sdkwork_webserver_contract::UpdateEnvVariableRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::EnvVariableResponse> {
        Self::validate_env_variable_value(&request.value)?;
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .update_env_variable(tenant_id, &site_id, variable_id, request)
            .await
    }

    async fn delete_env_variable(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        variable_id: &str,
    ) -> WebServiceResult<()> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .delete_env_variable(tenant_id, &site_id, variable_id)
            .await
    }

    async fn list_certificates(
        &self,
        context: &WebAppRequestContext,
        site_id: Option<&str>,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificatePage> {
        let tenant_id = if let Some(site_id) = site_id {
            self.require_application_access(context, site_id).await?.0
        } else {
            Self::require_tenant(context)?
        };
        let owner_id = Self::owner_filter(context)?;
        self.repository
            .list_certificates(tenant_id, owner_id, site_id, domain_id, page, page_size)
            .await
    }

    async fn issue_certificate(
        &self,
        context: &WebAppRequestContext,
        request: &IssueCertificateRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateOperationAcceptedResponse> {
        self.enqueue_certificate_issue(context, request).await
    }

    async fn retrieve_certificate_operation(
        &self,
        context: &WebAppRequestContext,
        operation_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateOperationResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let owner_id = Self::owner_filter(context)?;
        self.repository
            .retrieve_certificate_operation(tenant_id, owner_id, operation_id)
            .await
    }

    async fn list_listener_certificate_bindings(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::ListenerCertificateBindingPage> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .list_listener_certificate_bindings(tenant_id, &site_id, domain_id, page, page_size)
            .await
    }

    async fn bind_listener_certificate(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ListenerCertificateBindingResponse> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        let binding = self
            .repository
            .bind_listener_certificate(tenant_id, &site_id, domain_id, request)
            .await?;
        self.audit_site_action(
            context,
            "sites.domains.listener_certificate_bindings.create",
            &site_id,
        )
        .await;
        // A binding change alters the node's served certificate set; publish
        // the TLS material snapshot immediately so the data plane converges
        // without waiting for the next certificate operation.
        self.publish_node_tls_material_best_effort("listener_certificate_bind")
            .await;
        Ok(binding)
    }

    async fn unbind_listener_certificate(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        domain_id: &str,
        binding_id: &str,
    ) -> WebServiceResult<()> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .unbind_listener_certificate(tenant_id, &site_id, domain_id, binding_id)
            .await?;
        self.audit_site_action(
            context,
            "sites.domains.listener_certificate_bindings.delete",
            &site_id,
        )
        .await;
        self.publish_node_tls_material_best_effort("listener_certificate_unbind")
            .await;
        Ok(())
    }

    async fn list_health_checks(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::HealthCheckPage> {
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .list_health_checks(tenant_id, &site_id)
            .await
    }

    async fn create_health_check(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::HealthCheckResponse> {
        Self::validate_health_check_request(request)?;
        let (tenant_id, site_id) = self
            .require_application_access(context, application_id)
            .await?;
        self.repository
            .create_health_check(tenant_id, &site_id, request)
            .await
    }

    async fn create_platform_target(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        request: &CreatePlatformTargetRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::PlatformTargetResponse> {
        let tenant_id = self.require_application_access(context, application_id).await?.0;
        Self::validate_platform_target_request(request)?;
        self.repository
            .create_platform_target(tenant_id, application_id, request)
            .await
    }

    async fn list_platform_targets(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::PlatformTargetPage> {
        let tenant_id = self.require_application_access(context, application_id).await?.0;
        self.repository
            .list_platform_targets(tenant_id, application_id, page, page_size)
            .await
    }

    async fn retrieve_platform_target(
        &self,
        context: &WebAppRequestContext,
        application_id: &str,
        platform_target_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::PlatformTargetResponse> {
        let tenant_id = self.require_application_access(context, application_id).await?.0;
        self.repository
            .retrieve_platform_target(tenant_id, application_id, platform_target_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::{WebService, MAX_DEPLOYMENT_ARTIFACT_BYTES, MAX_ENV_VARIABLE_VALUE_BYTES};
    use sdkwork_webserver_contract::{
        ApplicationStoreListing, CreateDeploymentRequest, CreateDomainRequest,
        CreateEnvVariableRequest, CreateHealthCheckRequest, IssueCertificateRequest, MediaResource,
    };

    #[test]
    fn application_type_is_limited_to_public_business_types() {
        assert!(WebService::validate_app_kind("WEB").is_ok());
        assert!(WebService::validate_app_kind("API").is_ok());
        for invalid in ["web", "STATIC", "", "OTHER"] {
            assert!(WebService::validate_app_kind(invalid).is_err());
        }
    }

    #[test]
    fn deployment_artifact_identity_is_canonical_and_bounded() {
        let valid = CreateDeploymentRequest {
            artifact_drive_uri: Some("drive://spaces/space-1/nodes/node-1".to_owned()),
            artifact_size: Some(1024),
            artifact_hash: Some("a".repeat(64)),
            ..CreateDeploymentRequest::default()
        };
        assert!(WebService::validate_deployment_request(&valid).is_ok());
        assert!(
            WebService::validate_deployment_request(&CreateDeploymentRequest {
                deploy_type: 2,
                source_ref: Some("https://github.com/sdkwork/example.git".to_owned()),
                ..CreateDeploymentRequest::default()
            })
            .is_ok()
        );

        for invalid in [
            CreateDeploymentRequest::default(),
            CreateDeploymentRequest {
                deploy_type: 0,
                ..valid.clone()
            },
            CreateDeploymentRequest {
                environment: Some("prod".to_string()),
                ..valid.clone()
            },
            CreateDeploymentRequest {
                artifact_hash: Some("A".repeat(64)),
                ..valid.clone()
            },
            CreateDeploymentRequest {
                artifact_size: Some(MAX_DEPLOYMENT_ARTIFACT_BYTES + 1),
                ..valid.clone()
            },
            CreateDeploymentRequest {
                artifact_hash: None,
                ..valid.clone()
            },
            CreateDeploymentRequest {
                version_tag: Some(" release ".to_string()),
                ..valid.clone()
            },
            CreateDeploymentRequest {
                commit_hash: Some("not-a-commit".to_string()),
                ..valid.clone()
            },
            CreateDeploymentRequest {
                deploy_type: 2,
                ..CreateDeploymentRequest::default()
            },
        ] {
            assert!(WebService::validate_deployment_request(&invalid).is_err());
        }

        for artifact_drive_uri in [
            "https://example.test/package.zip",
            "drive://spaces/space-1/nodes/",
            "drive://spaces/space-1/nodes/node-1?token=secret",
        ] {
            assert!(
                WebService::validate_deployment_request(&CreateDeploymentRequest {
                    artifact_drive_uri: Some(artifact_drive_uri.to_owned()),
                    ..valid.clone()
                })
                .is_err()
            );
        }

        for source_ref in [
            "http://github.com/sdkwork/example.git",
            "https://user:secret@github.com/sdkwork/example.git",
            "https://github.com/sdkwork/example.git?token=secret",
            "https://github.com/sdkwork/example.git#main",
            "https://github.com/",
        ] {
            assert!(
                WebService::validate_deployment_request(&CreateDeploymentRequest {
                    deploy_type: 2,
                    source_ref: Some(source_ref.to_owned()),
                    ..CreateDeploymentRequest::default()
                })
                .is_err()
            );
        }

        for idempotency_key in ["", " padded", &"x".repeat(129)] {
            assert!(
                WebService::validate_deployment_request(&CreateDeploymentRequest {
                    idempotency_key: Some(idempotency_key.to_owned()),
                    ..valid.clone()
                })
                .is_err()
            );
        }
    }

    #[test]
    fn deployment_request_deserialization_matches_the_strict_openapi_schema() {
        let valid = serde_json::json!({
            "deployType": 1,
            "artifactDriveUri": "drive://spaces/space-1/nodes/node-1",
            "artifactSize": "1024",
            "artifactHash": "a".repeat(64)
        });
        assert!(serde_json::from_value::<CreateDeploymentRequest>(valid.clone()).is_ok());
        assert!(
            serde_json::from_value::<CreateDeploymentRequest>(serde_json::json!({
                "deployType": 2,
                "sourceRef": "https://github.com/sdkwork/example.git"
            }))
            .is_ok()
        );

        let mut missing_type = valid.clone();
        missing_type.as_object_mut().unwrap().remove("deployType");
        assert!(serde_json::from_value::<CreateDeploymentRequest>(missing_type).is_err());

        let mut unknown = valid;
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CreateDeploymentRequest>(unknown).is_err());
    }

    #[test]
    fn store_listing_requires_a_canonical_icon_for_release() {
        assert!(WebService::validate_store_listing(None, false).is_ok());
        assert!(WebService::validate_store_listing(None, true).is_err());
        assert!(WebService::validate_store_listing(
            Some(&ApplicationStoreListing::default()),
            true,
        )
        .is_err());

        let valid = ApplicationStoreListing {
            icon: Some(test_store_image("icon-1", "image/png", 1024, 1024)),
            cover: Some(test_store_image("cover-1", "image/jpeg", 1024, 500)),
            previews: vec![test_store_image("preview-1", "image/webp", 1290, 2796)],
            short_description: Some("A production application".to_string()),
            full_description: Some("A complete store description.".to_string()),
            release_notes: Some("Initial production release.".to_string()),
            category: Some("Developer Tools".to_string()),
            keywords: vec!["hosting".to_string(), "deployment".to_string()],
            support_url: Some("https://support.example.test/help".to_string()),
            privacy_policy_url: Some("https://example.test/privacy".to_string()),
            official_website_url: Some("https://example.test/".to_string()),
        };
        assert!(WebService::validate_store_listing(Some(&valid), true).is_ok());

        let mut invalid_source = valid.clone();
        invalid_source.icon.as_mut().unwrap().source = "external_url".to_string();
        assert!(WebService::validate_store_listing(Some(&invalid_source), true).is_err());

        let mut invalid_uri = valid.clone();
        invalid_uri.icon.as_mut().unwrap().uri =
            Some("drive://spaces/store-assets/nodes/icon-1?token=secret".to_string());
        assert!(WebService::validate_store_listing(Some(&invalid_uri), true).is_err());

        let mut invalid_dimensions = valid.clone();
        invalid_dimensions.icon.as_mut().unwrap().width = Some(512);
        assert!(WebService::validate_store_listing(Some(&invalid_dimensions), true).is_err());

        let mut square_icon = valid.clone();
        square_icon.icon.as_mut().unwrap().width = Some(512);
        square_icon.icon.as_mut().unwrap().height = Some(512);
        assert!(WebService::validate_store_listing(Some(&square_icon), true).is_ok());

        let mut invalid_mime = valid.clone();
        invalid_mime.icon.as_mut().unwrap().mime_type = Some("image/jpeg".to_string());
        assert!(WebService::validate_store_listing(Some(&invalid_mime), true).is_err());

        let mut invalid_size = valid.clone();
        invalid_size.icon.as_mut().unwrap().size_bytes = Some("2097153".to_string());
        assert!(WebService::validate_store_listing(Some(&invalid_size), true).is_err());

        let mut duplicate_previews = valid;
        duplicate_previews
            .previews
            .push(duplicate_previews.previews[0].clone());
        assert!(WebService::validate_store_listing(Some(&duplicate_previews), true).is_err());
    }

    fn test_store_image(node_id: &str, mime_type: &str, width: i32, height: i32) -> MediaResource {
        MediaResource {
            id: Some(node_id.to_string()),
            kind: "image".to_string(),
            source: "drive".to_string(),
            uri: Some(format!("drive://spaces/store-assets/nodes/{node_id}")),
            file_name: Some("store-image.png".to_string()),
            mime_type: Some(mime_type.to_string()),
            size_bytes: Some("4096".to_string()),
            width: Some(width),
            height: Some(height),
            alt_text: Some("Store image".to_string()),
            metadata: Some(serde_json::json!({
                "drive": { "spaceId": "store-assets", "nodeId": node_id }
            })),
            ..MediaResource::default()
        }
    }

    #[test]
    fn health_check_configuration_is_bounded() {
        let valid = CreateHealthCheckRequest {
            check_type: 1,
            check_url: "https://example.test/ready".to_string(),
            check_interval: 30,
            timeout_ms: 5_000,
            retry_count: 3,
        };
        assert!(WebService::validate_health_check_request(&valid).is_ok());

        for invalid in [
            CreateHealthCheckRequest {
                check_type: 0,
                ..valid.clone()
            },
            CreateHealthCheckRequest {
                check_url: "".to_string(),
                ..valid.clone()
            },
            CreateHealthCheckRequest {
                check_interval: 4,
                ..valid.clone()
            },
            CreateHealthCheckRequest {
                timeout_ms: 30_001,
                ..valid.clone()
            },
            CreateHealthCheckRequest {
                retry_count: 11,
                ..valid.clone()
            },
        ] {
            assert!(WebService::validate_health_check_request(&invalid).is_err());
        }
    }

    #[test]
    fn domain_environment_and_certificate_inputs_are_fail_closed() {
        let domain = CreateDomainRequest {
            hostname: "api.example.test".to_owned(),
            is_primary: false,
            ssl_enabled: true,
            ssl_provider: Some("letsencrypt".to_owned()),
        };
        assert!(WebService::validate_domain_request(&domain).is_ok());
        assert!(WebService::validate_domain_request(&CreateDomainRequest {
            hostname: "bad host".to_owned(),
            ..domain.clone()
        })
        .is_err());

        let variable = CreateEnvVariableRequest {
            key: "API_BASE_URL".to_owned(),
            value: "https://api.example.test".to_owned(),
            environment: "production".to_owned(),
            is_secret: false,
        };
        assert!(WebService::validate_env_variable_request(&variable).is_ok());
        assert!(
            WebService::validate_env_variable_request(&CreateEnvVariableRequest {
                key: "INVALID-KEY".to_owned(),
                ..variable.clone()
            })
            .is_err()
        );
        assert!(
            WebService::validate_env_variable_request(&CreateEnvVariableRequest {
                value: "x".repeat(MAX_ENV_VARIABLE_VALUE_BYTES + 1),
                ..variable
            })
            .is_err()
        );

        assert!(
            WebService::validate_certificate_issue_request(&IssueCertificateRequest {
                domain_ids: vec!["domain-1".to_owned(), "domain-2".to_owned()],
                cert_type: 1,
                key_algorithm: "ECDSA".to_owned(),
                auto_renew: true,
            })
            .is_ok()
        );
        assert!(
            WebService::validate_certificate_issue_request(&IssueCertificateRequest {
                domain_ids: vec!["domain-1".to_owned()],
                cert_type: 3,
                key_algorithm: "RSA".to_owned(),
                auto_renew: true,
            })
            .is_err()
        );
    }
}
