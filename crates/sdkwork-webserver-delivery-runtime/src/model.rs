use sdkwork_webserver_contract::provider::{
    ProviderResourceReference, WebsiteByteRange, WebsiteContentMetadata, WebsiteContentRange,
    WebsiteProviderContentStream, WebsiteRequestConditions, WebsiteWikiContentKind,
};
use sdkwork_webserver_core::website_runtime::{
    WebsiteClientClass, WebsiteClientClassificationSource, WebsiteProviderType,
    WebsiteRedirectScheme, WebsiteVariantSelectionReason,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebsiteDeliveryMethod {
    Get,
    Head,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebsiteDeliveryScheme {
    Http,
    Https,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebsiteDeliveryRoutingContext {
    pub verified_preferred_variant_uuid: Option<String>,
    pub client_class: Option<WebsiteClientClass>,
    pub client_classification_source: Option<WebsiteClientClassificationSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebsiteDeliveryRequest {
    pub authority: String,
    pub path: String,
    pub scheme: WebsiteDeliveryScheme,
    pub method: WebsiteDeliveryMethod,
    pub request_id: String,
    pub trace_id: String,
    pub routing: WebsiteDeliveryRoutingContext,
    pub conditions: WebsiteRequestConditions,
    pub range: Option<WebsiteByteRange>,
    pub locale: Option<String>,
    pub spa_fallback_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebsiteDeliveryRouteIdentity {
    pub runtime_set_generation: u64,
    pub revision_uuid: String,
    pub tenant_scope_hash: String,
    pub app_uuid: String,
    pub binding_uuid: String,
    pub variant_uuid: String,
    pub mount_uuid: String,
    pub resource_uuid: String,
    pub provider: ProviderResourceReference,
    pub provider_relative_path: String,
    pub variant_reason: WebsiteVariantSelectionReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebsiteDeliveryContentKind {
    Static,
    Wiki(WebsiteWikiContentKind),
}

pub struct WebsiteDeliveryContent {
    pub route: WebsiteDeliveryRouteIdentity,
    pub kind: WebsiteDeliveryContentKind,
    pub metadata: WebsiteContentMetadata,
    pub response_content_length: u64,
    pub content_range: Option<WebsiteContentRange>,
    pub canonical_route: Option<String>,
    pub page_uuid: Option<String>,
    pub public_page_version: Option<String>,
    pub renderer_version: Option<String>,
    pub navigation_generation: Option<String>,
    pub search_generation: Option<String>,
    pub body: Option<Box<dyn WebsiteProviderContentStream>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebsiteDeliveryRedirect {
    Binding {
        status_code: u16,
        scheme: WebsiteRedirectScheme,
        hostname: String,
        path: String,
        preserve_query: bool,
    },
    Wiki {
        route: Box<WebsiteDeliveryRouteIdentity>,
        status_code: u16,
        canonical_route: String,
        preserve_query: bool,
    },
}

pub enum WebsiteDeliveryOutcome {
    NotFound,
    NotModified,
    Redirect(WebsiteDeliveryRedirect),
    Content(Box<WebsiteDeliveryContent>),
}

impl WebsiteDeliveryRouteIdentity {
    pub fn provider_type(&self) -> WebsiteProviderType {
        self.provider.provider_type
    }
}
