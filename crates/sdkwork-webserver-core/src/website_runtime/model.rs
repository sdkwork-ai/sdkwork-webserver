use serde::{Deserialize, Serialize};

pub const WEBSITE_RUNTIME_DESCRIPTOR_KIND: &str = "sdkwork.website-runtime.descriptor";
pub const WEBSITE_RUNTIME_SCHEMA_VERSION: &str = "sdkwork.website-runtime.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteRuntimeDescriptor {
    pub schema_version: String,
    pub kind: String,
    pub revision_uuid: String,
    pub app_uuid: String,
    pub tenant_scope_hash: String,
    pub environment: WebsiteRuntimeEnvironment,
    pub generated_at: String,
    pub compiler_version: String,
    pub descriptor_sha256: String,
    pub app_default_variant_uuid: String,
    pub bindings: Vec<WebsiteBinding>,
    pub variants: Vec<WebsiteVariant>,
    pub variant_rules: Vec<WebsiteVariantRule>,
    pub resources: Vec<WebsiteResource>,
    pub mounts: Vec<WebsiteMount>,
    pub delivery_policy: WebsiteDeliveryPolicy,
    pub security_policy: WebsiteSecurityPolicy,
    pub limits: WebsiteRuntimeLimits,
    pub observability_policy: WebsiteObservabilityPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebsiteRuntimeEnvironment {
    Development,
    Test,
    Staging,
    Production,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteBinding {
    pub binding_uuid: String,
    pub hostname: String,
    pub path_prefix: String,
    pub action: WebsiteBindingAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum WebsiteBindingAction {
    Serve {
        #[serde(
            rename = "defaultVariantUuid",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        default_variant_uuid: Option<String>,
        #[serde(
            rename = "forcedVariantUuid",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        forced_variant_uuid: Option<String>,
    },
    Redirect {
        #[serde(rename = "statusCode")]
        status_code: u16,
        scheme: WebsiteRedirectScheme,
        hostname: String,
        #[serde(rename = "pathPrefix")]
        path_prefix: String,
        #[serde(rename = "preservePath")]
        preserve_path: bool,
        #[serde(rename = "preserveQuery")]
        preserve_query: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebsiteRedirectScheme {
    Http,
    Https,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteVariant {
    pub variant_uuid: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteVariantRule {
    pub rule_uuid: String,
    pub variant_uuid: String,
    pub priority: u16,
    #[serde(rename = "match")]
    pub matcher: WebsiteVariantRuleMatcher,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum WebsiteVariantRuleMatcher {
    PathPrefix {
        #[serde(rename = "pathPrefix")]
        path_prefix: String,
    },
    ClientClass {
        #[serde(rename = "clientClass")]
        client_class: WebsiteClientClass,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteClientClass {
    Desktop,
    Mobile,
    Tablet,
    Tv,
    Bot,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteResource {
    pub resource_uuid: String,
    pub provider: ProviderResourceReference,
    pub capabilities: WebsiteResourceCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderResourceReference {
    pub provider_type: WebsiteProviderType,
    pub provider_resource_uuid: String,
    pub provider_contract_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteProviderType {
    Drive,
    Knowledgebase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteResourceCapabilities {
    pub static_content: bool,
    pub wiki_routes: bool,
    pub wiki_search: bool,
    pub range_requests: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteMount {
    pub mount_uuid: String,
    pub variant_uuid: String,
    pub path_prefix: String,
    pub resource_uuid: String,
    pub handler: WebsiteHandler,
    pub translation: WebsiteMountTranslation,
    pub index_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spa_fallback: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteHandler {
    Static,
    Spa,
    Wiki,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteMountTranslation {
    pub mode: WebsiteMountMode,
    pub resource_subpath: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteMountMode {
    Root,
    Alias,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteDeliveryPolicy {
    pub provider_timeout_ms: u64,
    pub metadata_cache_ttl_seconds: u32,
    pub negative_cache_ttl_seconds: u32,
    pub stale_while_revalidate_seconds: u32,
    pub maximum_object_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteSecurityPolicy {
    pub force_https: bool,
    pub deny_dot_files: bool,
    pub denied_path_prefixes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteRuntimeLimits {
    pub maximum_bindings: usize,
    pub maximum_variants: usize,
    pub maximum_variant_rules: usize,
    pub maximum_resources: usize,
    pub maximum_mounts: usize,
    pub maximum_index_files_per_mount: usize,
    pub maximum_path_bytes: usize,
    pub maximum_path_segments: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteObservabilityPolicy {
    pub access_log_enabled: bool,
    pub usage_metering_enabled: bool,
    pub trace_sample_rate_per_mille: u16,
}
