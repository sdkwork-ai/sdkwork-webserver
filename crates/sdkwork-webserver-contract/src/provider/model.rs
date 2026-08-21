use std::fmt;

use serde::{Deserialize, Serialize};

pub use sdkwork_webserver_core::website_runtime::{
    ProviderResourceReference, WebsiteResourceCapabilities,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteProviderPurpose {
    Activation,
    Request,
    Revalidation,
    Reconciliation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteProviderRuntimeContext {
    pub tenant_scope_hash: String,
    pub site_uuid: String,
    pub binding_uuid: String,
    pub variant_uuid: String,
    pub mount_uuid: String,
    pub resource_uuid: String,
    pub request_id: String,
    pub trace_id: String,
    pub deadline_ms: u64,
    pub purpose: WebsiteProviderPurpose,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidateWebsiteResourceRequest {
    pub context: WebsiteProviderRuntimeContext,
    pub provider: ProviderResourceReference,
    pub required_capabilities: WebsiteResourceCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedWebsiteResource {
    pub provider_resource_uuid: String,
    pub provider_generation: String,
    pub public_generation: String,
    pub capabilities: WebsiteResourceCapabilities,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteRequestConditions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_match: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_none_match: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_modified_since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_unmodified_since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_range: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteByteRange {
    pub start: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_inclusive: Option<u64>,
    /// Suffix length from `bytes=-N` (RFC 9110 §14.1.2): the last N bytes of
    /// the representation. The delivery executor resolves it to an explicit
    /// `start`/`end_inclusive` range once the content length is known;
    /// providers only ever receive explicit ranges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteContentRange {
    pub start: u64,
    pub end_inclusive: u64,
    pub complete_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteContentMetadata {
    pub content_type: String,
    pub content_length: u64,
    pub etag: String,
    pub last_modified: String,
    pub content_version: String,
    pub provider_generation: String,
    pub range_supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveWebsiteStaticPathRequest {
    pub context: WebsiteProviderRuntimeContext,
    pub provider: ProviderResourceReference,
    pub provider_relative_path: String,
    pub conditions: WebsiteRequestConditions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedWebsiteContent {
    pub content_handle: WebsiteProviderContentHandle,
    pub metadata: WebsiteContentMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteContentResolution {
    Found(ResolvedWebsiteContent),
    NotModified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenWebsiteContentRequest {
    pub context: WebsiteProviderRuntimeContext,
    pub provider: ProviderResourceReference,
    pub provider_relative_path: String,
    pub content_handle: WebsiteProviderContentHandle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<WebsiteByteRange>,
    pub conditions: WebsiteRequestConditions,
    pub maximum_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteWikiContentKind {
    Html,
    Asset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveWebsiteWikiRouteRequest {
    pub context: WebsiteProviderRuntimeContext,
    pub provider: ProviderResourceReference,
    pub route: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    pub conditions: WebsiteRequestConditions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedWebsiteWikiContent {
    pub content_handle: WebsiteProviderContentHandle,
    pub kind: WebsiteWikiContentKind,
    pub canonical_route: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_uuid: Option<String>,
    pub public_page_version: String,
    pub renderer_version: String,
    pub navigation_generation: String,
    pub search_generation: String,
    pub metadata: WebsiteContentMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteWikiRedirect {
    pub status_code: u16,
    pub canonical_route: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebsiteWikiRouteResolution {
    Content(Box<ResolvedWebsiteWikiContent>),
    Redirect(WebsiteWikiRedirect),
    NotModified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteWikiCollectionRequest {
    pub context: WebsiteProviderRuntimeContext,
    pub provider: ProviderResourceReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub page_size: WebsiteProviderPageSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct WebsiteProviderPageSize(u16);

impl WebsiteProviderPageSize {
    pub const DEFAULT: Self = Self(20);
    pub const MAXIMUM: u16 = 200;

    pub fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for WebsiteProviderPageSize {
    type Error = String;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if (1..=Self::MAXIMUM).contains(&value) {
            Ok(Self(value))
        } else {
            Err(format!("page size must be between 1 and {}", Self::MAXIMUM))
        }
    }
}

impl From<WebsiteProviderPageSize> for u16 {
    fn from(value: WebsiteProviderPageSize) -> Self {
        value.0
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WebsiteProviderContentHandle(String);

impl WebsiteProviderContentHandle {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Self::try_from(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for WebsiteProviderContentHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WebsiteProviderContentHandle([REDACTED])")
    }
}

impl TryFrom<String> for WebsiteProviderContentHandle {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 512
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            Err("content handle must be non-empty, bounded, and control-free".to_owned())
        } else {
            Ok(Self(value))
        }
    }
}

impl From<WebsiteProviderContentHandle> for String {
    fn from(value: WebsiteProviderContentHandle) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteWikiCollectionItem {
    pub page_uuid: String,
    pub canonical_route: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub public_page_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteWikiCollectionPage {
    pub items: Vec<WebsiteWikiCollectionItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub generation: String,
}
