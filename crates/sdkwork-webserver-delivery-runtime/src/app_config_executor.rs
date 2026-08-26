//! Application-config provider resource executor.
//!
//! Executes `drive` and `knowledgebase` resources declared directly in
//! `sdkwork.webserver.config.json` through the shared website provider
//! registry and resolution cache. This is the local data-plane counterpart
//! of [`crate::WebsiteDeliveryExecutor`], which serves compiled cloud
//! runtime-set descriptors instead. Both share the provider contracts, the
//! bounded resolution cache, and the buffered-content admission budget.
//!
//! The executor is provider-agnostic: the data-plane bootstrap assembles the
//! registry from environment-owned SDK connections, validates every referenced
//! resource (fail closed), and then routes requests per route/resource.

use std::sync::Arc;

use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, ResolveWebsiteStaticPathRequest, ResolveWebsiteWikiRouteRequest,
    ResolvedWebsiteContent, ResolvedWebsiteWikiContent, WebsiteByteRange, WebsiteContentResolution,
    WebsiteProviderErrorKind, WebsiteProviderPurpose, WebsiteProviderResult,
    WebsiteProviderRuntimeContext, WebsiteRequestConditions, WebsiteStaticContentProvider,
    WebsiteWikiProvider, WebsiteWikiRouteResolution,
};
use sdkwork_webserver_core::website_runtime::{
    ProviderResourceReference, WebsiteProviderType, WebsiteVariantSelectionReason,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::{
    executor::{
        enforce_content_policy, join_provider_path, opened_body_fields, provider_contract_mismatch,
        provider_error_is_not_found, provider_error_outcome, provider_unavailable,
        request_conditions_are_cacheable, ProviderDeadline,
        DEFAULT_PROVIDER_BUFFERED_CONTENT_BYTES, DEFAULT_PROVIDER_RESOLUTION_CACHE_ENTRIES,
        MAXIMUM_PROVIDER_RESOLUTION_CACHE_ENTRIES,
    },
    resolution_cache::{ResolutionCacheKey, ResolutionCachePolicy, WebsiteProviderResolutionCache},
    stream::AdmittedProviderContentStream,
    WebsiteDeliveryContent, WebsiteDeliveryContentKind, WebsiteDeliveryError,
    WebsiteDeliveryExecutorConfigError, WebsiteDeliveryMethod, WebsiteDeliveryOutcome,
    WebsiteDeliveryRedirect, WebsiteDeliveryRouteIdentity, WebsiteProviderRegistry,
    WebsiteProviderResolutionCacheSnapshot, WebsiteRuntimeProviderValidationError,
};

/// Content handler of an application-config provider resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppConfigResourceHandler {
    /// Drive WebsiteRoot static content with index/SPA candidate chains.
    Static,
    /// Knowledgebase WikiPublication route resolution and content.
    Wiki,
}

/// Per-request route context for one provider-backed application-config
/// resource. The data-plane handler builds this from the compiled route and
/// the translated provider path.
#[derive(Clone, Debug)]
pub struct AppConfigResourceRoute {
    /// Virtual host id (binding identity).
    pub virtual_host_id: String,
    /// Route id (mount identity).
    pub route_id: String,
    /// Resource id (resource identity).
    pub resource_id: String,
    /// Provider reference resolved at bootstrap (type, resource uuid, and
    /// contract version).
    pub provider: ProviderResourceReference,
    pub handler: AppConfigResourceHandler,
    /// Provider-relative path after route prefix stripping and resource
    /// subpath translation.
    pub provider_relative_path: String,
    /// Directory-request index candidates (Drive only).
    pub index_files: Vec<String>,
    /// SPA fallback candidate (Drive only).
    pub spa_fallback: Option<String>,
    pub directory_request: bool,
    /// Resource-level default locale (Knowledgebase only). The request-level
    /// `Accept-Language` header takes precedence when present.
    pub locale: Option<String>,
    /// Resolution-cache policy; defaults apply when the resource does not
    /// configure one.
    pub cache: sdkwork_webserver_core::config::ProviderCachePolicy,
}

/// Delivery policy for one provider-backed resource, derived from the
/// application configuration limits.
#[derive(Clone, Copy, Debug)]
pub struct AppConfigProviderPolicy {
    pub provider_timeout_ms: u64,
    pub maximum_object_bytes: u64,
}

/// A request to a provider-backed application-config resource. The data-plane
/// handler fills this from the raw HTTP request; the executor performs the
/// provider operations and produces a standard delivery outcome.
#[derive(Clone, Debug)]
pub struct AppConfigProviderRequest {
    pub method: WebsiteDeliveryMethod,
    pub request_id: String,
    pub trace_id: String,
    pub conditions: WebsiteRequestConditions,
    pub range: Option<WebsiteByteRange>,
    pub locale: Option<String>,
    pub spa_fallback_eligible: bool,
}

/// Per-request serving context shared by the content-open helpers.
struct ServeContext<'a> {
    policy: &'a AppConfigProviderPolicy,
    request: &'a AppConfigProviderRequest,
}

/// Executes provider-backed resources declared in the application Web Server
/// configuration. One instance is shared by all listeners and reload
/// generations; the provider registry and cache outlive configuration
/// generations while their content is generation-keyed.
pub struct AppConfigResourceExecutor {
    provider_registry: Arc<WebsiteProviderRegistry>,
    buffered_content_admission: Arc<Semaphore>,
    resolution_cache: Arc<WebsiteProviderResolutionCache>,
    app_uuid: String,
    tenant_scope_hash: String,
}

impl AppConfigResourceExecutor {
    pub fn new(
        provider_registry: Arc<WebsiteProviderRegistry>,
        app_uuid: String,
        tenant_scope_hash: String,
    ) -> Result<Self, WebsiteDeliveryExecutorConfigError> {
        Self::with_provider_runtime_limits(
            provider_registry,
            app_uuid,
            tenant_scope_hash,
            DEFAULT_PROVIDER_BUFFERED_CONTENT_BYTES,
            DEFAULT_PROVIDER_RESOLUTION_CACHE_ENTRIES,
        )
    }

    pub fn with_provider_runtime_limits(
        provider_registry: Arc<WebsiteProviderRegistry>,
        app_uuid: String,
        tenant_scope_hash: String,
        maximum_buffered_content_bytes: usize,
        maximum_resolution_cache_entries: usize,
    ) -> Result<Self, WebsiteDeliveryExecutorConfigError> {
        if maximum_buffered_content_bytes == 0 || maximum_buffered_content_bytes > u32::MAX as usize
        {
            return Err(
                WebsiteDeliveryExecutorConfigError::InvalidBufferedContentBudget {
                    configured_bytes: maximum_buffered_content_bytes,
                    maximum_bytes: u32::MAX as usize,
                },
            );
        }
        Ok(Self {
            provider_registry,
            buffered_content_admission: Arc::new(Semaphore::new(maximum_buffered_content_bytes)),
            resolution_cache: Arc::new(WebsiteProviderResolutionCache::new(
                maximum_resolution_cache_entries,
                MAXIMUM_PROVIDER_RESOLUTION_CACHE_ENTRIES,
            )?),
            app_uuid,
            tenant_scope_hash,
        })
    }

    pub async fn provider_resolution_cache_snapshot(
        &self,
    ) -> WebsiteProviderResolutionCacheSnapshot {
        self.resolution_cache.snapshot().await
    }

    /// Validates one provider-backed resource before the data plane starts
    /// serving it. Fails closed on provider absence, validation failure, or
    /// contract mismatch.
    pub async fn validate_resource(
        &self,
        resource_id: &str,
        provider: &ProviderResourceReference,
        handler: AppConfigResourceHandler,
        provider_timeout_ms: u64,
    ) -> Result<(), WebsiteRuntimeProviderValidationError> {
        let required_capabilities =
            sdkwork_webserver_core::website_runtime::WebsiteResourceCapabilities {
                static_content: handler == AppConfigResourceHandler::Static,
                wiki_routes: handler == AppConfigResourceHandler::Wiki,
                wiki_search: false,
                range_requests: handler == AppConfigResourceHandler::Static,
            };
        let identity = format!("app-config-activation-{resource_id}");
        let context = WebsiteProviderRuntimeContext {
            tenant_scope_hash: self.tenant_scope_hash.clone(),
            app_uuid: self.app_uuid.clone(),
            binding_uuid: format!("binding:{resource_id}"),
            variant_uuid: "default".to_owned(),
            mount_uuid: format!("route:{resource_id}"),
            resource_uuid: resource_id.to_owned(),
            request_id: identity.clone(),
            trace_id: identity,
            deadline_ms: provider_timeout_ms,
            purpose: WebsiteProviderPurpose::Activation,
        };
        self.provider_registry
            .validate_provider_resource(provider, &required_capabilities, context)
            .await
            .map(|_| ())
    }

    /// Whether every provider type referenced by a candidate configuration
    /// generation is registered. The watch loop rejects reloads that would
    /// introduce an unassembled provider.
    pub fn can_serve_config(&self, provider_types: &[WebsiteProviderType]) -> bool {
        provider_types
            .iter()
            .all(|provider_type| match provider_type {
                WebsiteProviderType::Drive => {
                    self.provider_registry.supports_static(*provider_type)
                }
                WebsiteProviderType::Knowledgebase => {
                    self.provider_registry.supports_wiki(*provider_type)
                }
            })
    }

    pub async fn serve_static(
        &self,
        route: &AppConfigResourceRoute,
        policy: &AppConfigProviderPolicy,
        request: &AppConfigProviderRequest,
    ) -> Result<WebsiteDeliveryOutcome, WebsiteDeliveryError> {
        validate_request_identity(request)?;
        let provider = self
            .provider_registry
            .static_provider(route.provider.provider_type)
            .ok_or(WebsiteDeliveryError::ProviderNotRegistered {
                provider_type: route.provider.provider_type,
                capability: "static-content",
            })?;
        let deadline = ProviderDeadline::new(policy.provider_timeout_ms);
        let mut context = self.provider_context(route, request, WebsiteProviderPurpose::Request);
        let cache_policy = self.cache_policy(route);
        let mut identity = self.route_identity(route, route.provider_relative_path.clone());
        for candidate in static_candidates(route, request.spa_fallback_eligible) {
            context.deadline_ms = deadline.remaining_ms()?;
            identity.provider_relative_path = candidate.clone();
            let resolve_request = ResolveWebsiteStaticPathRequest {
                context: context.clone(),
                provider: route.provider.clone(),
                provider_relative_path: candidate.clone(),
                conditions: request.conditions.clone(),
            };
            let resolution = if request_conditions_are_cacheable(&request.conditions) {
                self.resolution_cache
                    .resolve_static(
                        ResolutionCacheKey::static_path(&identity, &candidate),
                        cache_policy,
                        Arc::clone(&provider),
                        resolve_request,
                        deadline.remaining_ms()?,
                    )
                    .await
            } else {
                deadline
                    .call(provider.resolve_static_path(&resolve_request))
                    .await
            };
            let content = match resolution {
                Ok(WebsiteContentResolution::Found(content)) => content,
                Ok(WebsiteContentResolution::NotModified) => {
                    return Ok(WebsiteDeliveryOutcome::NotModified)
                }
                Err(error) if provider_error_is_not_found(&error) => continue,
                Err(error) if error.kind == WebsiteProviderErrorKind::NotModified => {
                    return Ok(WebsiteDeliveryOutcome::NotModified)
                }
                Err(error) => return Err(error.into()),
            };
            enforce_content_policy(
                &content.metadata,
                policy.maximum_object_bytes,
                request.range,
            )?;
            identity.provider_relative_path = candidate;
            return self
                .open_static_content(
                    provider,
                    identity,
                    context,
                    &deadline,
                    content,
                    ServeContext { policy, request },
                )
                .await;
        }
        Ok(WebsiteDeliveryOutcome::NotFound)
    }

    pub async fn serve_wiki(
        &self,
        route: &AppConfigResourceRoute,
        policy: &AppConfigProviderPolicy,
        request: &AppConfigProviderRequest,
    ) -> Result<WebsiteDeliveryOutcome, WebsiteDeliveryError> {
        validate_request_identity(request)?;
        let provider = self
            .provider_registry
            .wiki_provider(route.provider.provider_type)
            .ok_or(WebsiteDeliveryError::ProviderNotRegistered {
                provider_type: route.provider.provider_type,
                capability: "wiki",
            })?;
        let deadline = ProviderDeadline::new(policy.provider_timeout_ms);
        let context = self.provider_context(route, request, WebsiteProviderPurpose::Request);
        let identity = self.route_identity(route, route.provider_relative_path.clone());
        let cache_policy = self.cache_policy(route);
        let resolve_request = ResolveWebsiteWikiRouteRequest {
            context: context.clone(),
            provider: route.provider.clone(),
            route: route.provider_relative_path.clone(),
            locale: request.locale.clone(),
            conditions: request.conditions.clone(),
        };
        let resolution = if request_conditions_are_cacheable(&request.conditions) {
            self.resolution_cache
                .resolve_wiki(
                    ResolutionCacheKey::wiki_route(
                        &identity,
                        &route.provider_relative_path,
                        request.locale.as_deref(),
                    ),
                    cache_policy,
                    Arc::clone(&provider),
                    resolve_request,
                    deadline.remaining_ms()?,
                )
                .await
        } else {
            deadline
                .call(provider.resolve_wiki_route(&resolve_request))
                .await
        };
        match resolution {
            Ok(WebsiteWikiRouteResolution::NotModified) => Ok(WebsiteDeliveryOutcome::NotModified),
            Ok(WebsiteWikiRouteResolution::Redirect(redirect)) => Ok(
                WebsiteDeliveryOutcome::Redirect(WebsiteDeliveryRedirect::Wiki {
                    route: Box::new(identity),
                    status_code: redirect.status_code,
                    canonical_route: redirect.canonical_route,
                    preserve_query: true,
                }),
            ),
            Ok(WebsiteWikiRouteResolution::Content(content)) => {
                enforce_content_policy(
                    &content.metadata,
                    policy.maximum_object_bytes,
                    request.range,
                )?;
                let opened = self
                    .open_wiki_body(
                        provider,
                        identity.clone(),
                        context,
                        &deadline,
                        &content,
                        ServeContext { policy, request },
                    )
                    .await?;
                let canonical_route = Some(content.canonical_route.clone());
                let opened = opened_body_fields(
                    opened,
                    &content.metadata,
                    request.range,
                    request.conditions.if_range.is_some(),
                    policy.maximum_object_bytes,
                    policy.provider_timeout_ms,
                )?;
                Ok(WebsiteDeliveryOutcome::Content(Box::new(
                    WebsiteDeliveryContent {
                        route: identity,
                        kind: WebsiteDeliveryContentKind::Wiki(content.kind),
                        metadata: content.metadata,
                        response_content_length: opened.content_length,
                        content_range: opened.content_range,
                        canonical_route,
                        page_uuid: content.page_uuid,
                        public_page_version: Some(content.public_page_version),
                        renderer_version: Some(content.renderer_version),
                        navigation_generation: Some(content.navigation_generation),
                        search_generation: Some(content.search_generation),
                        body: opened.stream,
                    },
                )))
            }
            Err(error) => provider_error_outcome(error),
        }
    }

    async fn open_static_content(
        &self,
        provider: Arc<dyn WebsiteStaticContentProvider>,
        identity: WebsiteDeliveryRouteIdentity,
        mut context: WebsiteProviderRuntimeContext,
        deadline: &ProviderDeadline,
        content: ResolvedWebsiteContent,
        serve: ServeContext<'_>,
    ) -> Result<WebsiteDeliveryOutcome, WebsiteDeliveryError> {
        let ServeContext { policy, request } = serve;
        let expected_bytes = content.metadata.content_length;
        let if_range_present = request.conditions.if_range.is_some();
        let opened = if request.method == WebsiteDeliveryMethod::Head {
            None
        } else {
            let permit = self.acquire_buffered_content(policy.maximum_object_bytes)?;
            context.deadline_ms = deadline.remaining_ms()?;
            let open_request = OpenWebsiteContentRequest {
                context,
                provider: identity.provider.clone(),
                provider_relative_path: identity.provider_relative_path.clone(),
                content_handle: content.content_handle,
                range: request.range,
                conditions: request.conditions.clone(),
                maximum_bytes: policy.maximum_object_bytes,
            };
            let mut opened = deadline
                .call(provider.open_static_content(&open_request))
                .await?;
            if request.range.is_none() && opened.content_length != expected_bytes {
                return Err(provider_contract_mismatch());
            }
            opened.stream = Box::new(AdmittedProviderContentStream::new(opened.stream, permit));
            Some(opened)
        };
        let opened = opened_body_fields(
            opened,
            &content.metadata,
            request.range,
            if_range_present,
            policy.maximum_object_bytes,
            policy.provider_timeout_ms,
        )?;
        Ok(WebsiteDeliveryOutcome::Content(Box::new(
            WebsiteDeliveryContent {
                route: identity,
                kind: WebsiteDeliveryContentKind::Static,
                metadata: content.metadata,
                response_content_length: opened.content_length,
                content_range: opened.content_range,
                canonical_route: None,
                page_uuid: None,
                public_page_version: None,
                renderer_version: None,
                navigation_generation: None,
                search_generation: None,
                body: opened.stream,
            },
        )))
    }

    async fn open_wiki_body(
        &self,
        provider: Arc<dyn WebsiteWikiProvider>,
        identity: WebsiteDeliveryRouteIdentity,
        mut context: WebsiteProviderRuntimeContext,
        deadline: &ProviderDeadline,
        content: &ResolvedWebsiteWikiContent,
        serve: ServeContext<'_>,
    ) -> Result<
        Option<sdkwork_webserver_contract::provider::OpenedWebsiteContent>,
        WebsiteDeliveryError,
    > {
        let ServeContext { policy, request } = serve;
        if request.method == WebsiteDeliveryMethod::Head {
            return Ok(None);
        }
        let expected_bytes = content.metadata.content_length;
        let permit = self.acquire_buffered_content(policy.maximum_object_bytes)?;
        context.deadline_ms = deadline.remaining_ms()?;
        let open_request = OpenWebsiteContentRequest {
            context,
            provider: identity.provider.clone(),
            provider_relative_path: identity.provider_relative_path.clone(),
            content_handle: content.content_handle.clone(),
            range: request.range,
            conditions: request.conditions.clone(),
            maximum_bytes: policy.maximum_object_bytes,
        };
        let mut opened = deadline
            .call(provider.open_wiki_content(&open_request))
            .await?;
        if request.range.is_none() && opened.content_length != expected_bytes {
            return Err(provider_contract_mismatch());
        }
        opened.stream = Box::new(AdmittedProviderContentStream::new(opened.stream, permit));
        Ok(Some(opened))
    }

    fn acquire_buffered_content(
        &self,
        reserved_bytes: u64,
    ) -> WebsiteProviderResult<OwnedSemaphorePermit> {
        let permits = u32::try_from(reserved_bytes.max(1)).map_err(|_| provider_unavailable())?;
        Arc::clone(&self.buffered_content_admission)
            .try_acquire_many_owned(permits)
            .map_err(|_| provider_unavailable())
    }

    fn provider_context(
        &self,
        route: &AppConfigResourceRoute,
        request: &AppConfigProviderRequest,
        purpose: WebsiteProviderPurpose,
    ) -> WebsiteProviderRuntimeContext {
        WebsiteProviderRuntimeContext {
            tenant_scope_hash: self.tenant_scope_hash.clone(),
            app_uuid: self.app_uuid.clone(),
            binding_uuid: route.virtual_host_id.clone(),
            variant_uuid: "default".to_owned(),
            mount_uuid: route.route_id.clone(),
            resource_uuid: route.resource_id.clone(),
            request_id: request.request_id.clone(),
            trace_id: request.trace_id.clone(),
            deadline_ms: 0,
            purpose,
        }
    }

    fn route_identity(
        &self,
        route: &AppConfigResourceRoute,
        provider_relative_path: String,
    ) -> WebsiteDeliveryRouteIdentity {
        WebsiteDeliveryRouteIdentity {
            runtime_set_generation: 0,
            revision_uuid: route.resource_id.clone(),
            tenant_scope_hash: self.tenant_scope_hash.clone(),
            app_uuid: self.app_uuid.clone(),
            binding_uuid: route.virtual_host_id.clone(),
            variant_uuid: "default".to_owned(),
            mount_uuid: route.route_id.clone(),
            resource_uuid: route.resource_id.clone(),
            provider: route.provider.clone(),
            provider_relative_path,
            variant_reason: WebsiteVariantSelectionReason::BindingDefault,
        }
    }

    fn cache_policy(&self, route: &AppConfigResourceRoute) -> ResolutionCachePolicy {
        ResolutionCachePolicy::from_seconds(
            route.cache.metadata_ttl_seconds,
            route.cache.negative_ttl_seconds,
            route.cache.stale_while_revalidate_seconds,
        )
    }
}

fn static_candidates(route: &AppConfigResourceRoute, spa_fallback_eligible: bool) -> Vec<String> {
    let mut candidates = if route.directory_request {
        route
            .index_files
            .iter()
            .map(|index| join_provider_path(&route.provider_relative_path, index))
            .collect::<Vec<_>>()
    } else {
        vec![route.provider_relative_path.clone()]
    };
    if route.handler == AppConfigResourceHandler::Static && spa_fallback_eligible {
        // The application-config `spaFallback` is a single-segment filename
        // resolved against the resource root (like the local static
        // resource), unlike the cloud descriptor's absolute provider path.
        if let Some(fallback) = &route.spa_fallback {
            let candidate = join_provider_path("/", fallback);
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

fn validate_request_identity(
    request: &AppConfigProviderRequest,
) -> Result<(), WebsiteDeliveryError> {
    if !valid_bounded_identity(&request.request_id) || !valid_bounded_identity(&request.trace_id) {
        return Err(WebsiteDeliveryError::InvalidRequestIdentity);
    }
    Ok(())
}

fn valid_bounded_identity(value: &str) -> bool {
    const MAXIMUM_IDENTITY_BYTES: usize = 256;
    !value.is_empty()
        && value.len() <= MAXIMUM_IDENTITY_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control())
}
