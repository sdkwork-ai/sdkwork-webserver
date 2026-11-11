//! Provider transport selection for the website data plane.
//!
//! MODULE_SPEC.md §4.2 makes the in-process transport the default: the Web
//! Server reaches Drive and Knowledgebase through their own service/repository
//! ports on the shared PostgreSQL pool. A **deliberate split deployment** (the
//! website data plane running as its own container, with neither module in the
//! same process) may still call the sibling Internal APIs — but only through an
//! explicit opt-in, and only when every endpoint it needs is configured; the
//! selection fails at startup, never at the first business request.
//!
//! This is the only module allowed to name a sibling module endpoint
//! (`specs/backend-module-call-boundary.allow.json`).

/// Sibling endpoint variables, read only when the matching
/// `*_PROVIDER_MODE=http` opt-in is set.
pub const KNOWLEDGEBASE_INTERNAL_API_BASE_URL_ENV: &str =
    "SDKWORK_WEBSERVER_KNOWLEDGEBASE_INTERNAL_API_BASE_URL";
pub const KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV: &str =
    "SDKWORK_WEBSERVER_KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE";
pub const DRIVE_INTERNAL_API_BASE_URL_ENV: &str = "SDKWORK_WEBSERVER_DRIVE_INTERNAL_API_BASE_URL";
pub const DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV: &str =
    "SDKWORK_WEBSERVER_DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE";

/// `in_process` (default) or `http` (explicit split deployment).
pub const DRIVE_PROVIDER_MODE_ENV: &str = "SDKWORK_WEBSERVER_DRIVE_PROVIDER_MODE";
pub const KNOWLEDGEBASE_PROVIDER_MODE_ENV: &str = "SDKWORK_WEBSERVER_KNOWLEDGEBASE_PROVIDER_MODE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebsiteProviderTransport {
    /// Sibling module reachable in this process on the shared pool (default).
    InProcess,
    /// Split deployment: call the sibling Internal API. Explicit opt-in only.
    Http,
}

/// Split-deployment timeout for one provider call. Identical to the historical
/// HTTP transport so the in-process default did not tighten a live deadline.
pub const PROVIDER_HTTP_TIMEOUT_MS: u64 = 30_000;

/// Builds the Drive Internal API client for the explicit split deployment.
///
/// The caller validates `base_url`/`ingress_token` from the opt-in endpoint
/// variables; this function only constructs the generated client so every
/// sibling-endpoint name stays in this module.
pub fn drive_http_client(
    base_url: &str,
    ingress_token: String,
    maximum_content_bytes: u64,
) -> Result<std::sync::Arc<dyn sdkwork_webserver_drive_provider::DriveWebsiteSdkClient>, String> {
    use sdkwork_drive_internal_sdk::{
        SdkworkConfig as DriveSdkworkConfig, SdkworkCustomClient as DriveSdkworkCustomClient,
    };

    let mut sdk_config = DriveSdkworkConfig::new(sdkwork_utils_rust::service_base_url::registry_base_url_or(
        "sdkwork-drive",
        base_url,
    ));
    sdk_config.timeout_ms = PROVIDER_HTTP_TIMEOUT_MS;
    sdk_config.max_response_body_bytes = usize::try_from(maximum_content_bytes)
        .map_err(|_| "Drive provider content ceiling does not fit usize".to_owned())?;
    let client = DriveSdkworkCustomClient::new(sdk_config)
        .map_err(|_| "generated Drive Internal SDK client construction failed".to_owned())?;
    client.set_api_key(ingress_token);
    Ok(std::sync::Arc::new(client.drive_internal_publishing()))
}

/// Builds the Knowledgebase Internal API client for the explicit split
/// deployment.
pub fn knowledgebase_http_client(
    base_url: &str,
    ingress_token: String,
    maximum_response_body_bytes: usize,
) -> Result<
    std::sync::Arc<dyn sdkwork_webserver_knowledgebase_provider::KnowledgebaseWikiSdkClient>,
    String,
> {
    use sdkwork_knowledgebase_internal_sdk::{
        SdkworkConfig as KnowledgebaseSdkworkConfig,
        SdkworkCustomClient as KnowledgebaseSdkworkCustomClient,
    };

    let mut sdk_config =
        KnowledgebaseSdkworkConfig::new(sdkwork_utils_rust::service_base_url::registry_base_url_or(
            "sdkwork-knowledgebase",
            base_url,
        ));
    sdk_config.timeout_ms = PROVIDER_HTTP_TIMEOUT_MS;
    sdk_config.max_response_body_bytes = maximum_response_body_bytes;
    let client = KnowledgebaseSdkworkCustomClient::new(sdk_config).map_err(|_| {
        "generated Knowledgebase Internal SDK client construction failed".to_owned()
    })?;
    client.set_api_key(ingress_token);
    Ok(std::sync::Arc::new(client.knowledgebase_internal_wiki()))
}

/// Resolves the transport for one provider family.
///
/// Unset defaults to in-process. `in_process` is accepted explicitly so a
/// deployment can pin the default; every other value is a startup error — a typo
/// must never silently pick a transport.
pub fn provider_transport(
    mode_key: &'static str,
) -> Result<WebsiteProviderTransport, String> {
    match std::env::var(mode_key) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "in_process" | "in-process" | "embedded" => {
                Ok(WebsiteProviderTransport::InProcess)
            }
            "http" => Ok(WebsiteProviderTransport::Http),
            other => Err(format!("{mode_key} must be in_process or http (got {other})")),
        },
        Err(std::env::VarError::NotPresent) => Ok(WebsiteProviderTransport::InProcess),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{mode_key} must be UTF-8"))
        }
    }
}
