use std::{sync::Arc, time::Duration};

use sdkwork_webserver_contract::provider::{
    ResolveWebsiteStaticPathRequest, ResolveWebsiteWikiRouteRequest, WebsiteContentResolution,
    WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderResult,
    WebsiteStaticContentProvider, WebsiteWikiProvider, WebsiteWikiRouteResolution,
};
use sdkwork_webserver_core::website_runtime::WebsiteProviderType;
use tokio::time::timeout;

use crate::WebsiteDeliveryRouteIdentity;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ProviderCacheIdentity {
    pub(super) provider_type: WebsiteProviderType,
    pub(super) provider_resource_uuid: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ResolutionCacheKind {
    Static,
    Wiki,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ResolutionCacheKey {
    runtime_set_generation: u64,
    revision_uuid: String,
    tenant_scope_hash: String,
    app_uuid: String,
    binding_uuid: String,
    variant_uuid: String,
    mount_uuid: String,
    resource_uuid: String,
    provider: ProviderCacheIdentity,
    provider_contract_version: String,
    path: String,
    locale: Option<String>,
    kind: ResolutionCacheKind,
}

impl ResolutionCacheKey {
    pub(crate) fn static_path(route: &WebsiteDeliveryRouteIdentity, path: &str) -> Self {
        Self::new(route, path, None, ResolutionCacheKind::Static)
    }

    pub(crate) fn wiki_route(
        route: &WebsiteDeliveryRouteIdentity,
        path: &str,
        locale: Option<&str>,
    ) -> Self {
        Self::new(
            route,
            path,
            locale.map(str::to_owned),
            ResolutionCacheKind::Wiki,
        )
    }

    fn new(
        route: &WebsiteDeliveryRouteIdentity,
        path: &str,
        locale: Option<String>,
        kind: ResolutionCacheKind,
    ) -> Self {
        Self {
            runtime_set_generation: route.runtime_set_generation,
            revision_uuid: route.revision_uuid.clone(),
            tenant_scope_hash: route.tenant_scope_hash.clone(),
            app_uuid: route.app_uuid.clone(),
            binding_uuid: route.binding_uuid.clone(),
            variant_uuid: route.variant_uuid.clone(),
            mount_uuid: route.mount_uuid.clone(),
            resource_uuid: route.resource_uuid.clone(),
            provider: ProviderCacheIdentity {
                provider_type: route.provider.provider_type,
                provider_resource_uuid: route.provider.provider_resource_uuid.clone(),
            },
            provider_contract_version: route.provider.provider_contract_version.clone(),
            path: path.to_owned(),
            locale,
            kind,
        }
    }

    pub(super) fn provider(&self) -> &ProviderCacheIdentity {
        &self.provider
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolutionCachePolicy {
    pub(super) metadata_ttl: Duration,
    pub(super) negative_ttl: Duration,
    pub(super) stale_while_revalidate: Duration,
}

impl ResolutionCachePolicy {
    pub(crate) fn from_seconds(
        metadata_ttl_seconds: u32,
        negative_ttl_seconds: u32,
        stale_while_revalidate_seconds: u32,
    ) -> Self {
        Self {
            metadata_ttl: Duration::from_secs(u64::from(metadata_ttl_seconds)),
            negative_ttl: Duration::from_secs(u64::from(negative_ttl_seconds)),
            stale_while_revalidate: Duration::from_secs(u64::from(stale_while_revalidate_seconds)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CachedResolution {
    Static(WebsiteContentResolution),
    Wiki(WebsiteWikiRouteResolution),
    Negative,
}

impl CachedResolution {
    pub(super) fn is_positive_cacheable(&self) -> bool {
        matches!(
            self,
            Self::Static(WebsiteContentResolution::Found(_))
                | Self::Wiki(
                    WebsiteWikiRouteResolution::Content(_)
                        | WebsiteWikiRouteResolution::Redirect(_)
                )
        )
    }
}

pub(super) enum ResolutionOrigin {
    Static {
        provider: Arc<dyn WebsiteStaticContentProvider>,
        request: ResolveWebsiteStaticPathRequest,
    },
    Wiki {
        provider: Arc<dyn WebsiteWikiProvider>,
        request: ResolveWebsiteWikiRouteRequest,
    },
}

impl ResolutionOrigin {
    pub(super) fn revalidation(mut self) -> Self {
        match &mut self {
            Self::Static { request, .. } => {
                request.context.purpose =
                    sdkwork_webserver_contract::provider::WebsiteProviderPurpose::Revalidation;
            }
            Self::Wiki { request, .. } => {
                request.context.purpose =
                    sdkwork_webserver_contract::provider::WebsiteProviderPurpose::Revalidation;
            }
        }
        self
    }

    pub(super) async fn call(self, deadline_ms: u64) -> WebsiteProviderResult<CachedResolution> {
        if deadline_ms == 0 {
            return Err(deadline_exceeded());
        }
        match self {
            Self::Static { provider, request } => timeout(
                Duration::from_millis(deadline_ms),
                provider.resolve_static_path(&request),
            )
            .await
            .map_err(|_| deadline_exceeded())?
            .map(CachedResolution::Static),
            Self::Wiki { provider, request } => timeout(
                Duration::from_millis(deadline_ms),
                provider.resolve_wiki_route(&request),
            )
            .await
            .map_err(|_| deadline_exceeded())?
            .map(CachedResolution::Wiki),
        }
    }
}

pub(super) fn normalize_origin_result(
    result: WebsiteProviderResult<CachedResolution>,
) -> WebsiteProviderResult<CachedResolution> {
    match result {
        Err(error)
            if matches!(
                error.kind,
                WebsiteProviderErrorKind::NotFound
                    | WebsiteProviderErrorKind::NotPublic
                    | WebsiteProviderErrorKind::Revoked
            ) =>
        {
            Ok(CachedResolution::Negative)
        }
        other => other,
    }
}

pub(super) fn deadline_exceeded() -> WebsiteProviderError {
    WebsiteProviderError::new(WebsiteProviderErrorKind::DeadlineExceeded)
}

pub(super) fn unavailable() -> WebsiteProviderError {
    WebsiteProviderError::new(WebsiteProviderErrorKind::Unavailable)
}

pub(super) fn contract_mismatch() -> WebsiteProviderError {
    WebsiteProviderError::new(WebsiteProviderErrorKind::ContractMismatch)
}
