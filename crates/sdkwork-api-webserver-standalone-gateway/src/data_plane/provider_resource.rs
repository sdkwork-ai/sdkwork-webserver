//! Provider-backed application-config resource request serving.
//!
//! Bridges the data-plane HTTP request path to the shared provider executor
//! for `drive` and `knowledgebase` resources declared in
//! `sdkwork.webserver.config.json`. Request header parsing and outcome
//! response assembly reuse the website-delivery helpers so both paths expose
//! identical conditional-request, Range, redirect, and error semantics.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{
        header::{
            ACCEPT_LANGUAGE, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_RANGE,
            IF_UNMODIFIED_SINCE, RANGE,
        },
        HeaderMap, Response, StatusCode,
    },
};
use sdkwork_web_core::{new_request_id, resolve_trace_context, trace_id_from_traceparent};
use sdkwork_webserver_contract::provider::WebsiteRequestConditions;
use sdkwork_webserver_core::RouteConfig;
use sdkwork_webserver_delivery_runtime::{
    join_canonical_paths, AppConfigProviderPolicy, AppConfigProviderRequest,
    AppConfigResourceExecutor, AppConfigResourceHandler, AppConfigResourceRoute,
    WebsiteDeliveryMethod,
};

use super::{
    static_files::relative_request_path,
    website_delivery::{
        bounded_header, delivery_error_response, finalize_response, method_not_allowed,
        navigation_request, outcome_response, parse_locale, parse_range, range_not_satisfiable,
        text_response, RequestHeaderError,
    },
};

const MAXIMUM_CONDITION_HEADER_BYTES: usize = 4 * 1024;
const MAXIMUM_RANGE_HEADER_BYTES: usize = 128;
const MAXIMUM_ACCEPT_LANGUAGE_BYTES: usize = 256;

/// Translates a request path to the provider-relative path for a Drive
/// resource: the route prefix is stripped like a local static resource, then
/// the configured provider subpath is prepended (root semantics). An empty
/// remainder maps to the resource root `/`.
pub(crate) fn translate_provider_path(
    route: &RouteConfig,
    request_path: &str,
    resource_subpath: Option<&str>,
) -> String {
    // Drive resources keep alias-style prefix stripping (the matched route
    // prefix is removed before the provider subpath is prepended).
    let relative = relative_request_path(route, true, request_path);
    match resource_subpath {
        None if relative.is_empty() => "/".to_owned(),
        None => relative.to_owned(),
        Some(subpath) if relative.is_empty() => subpath.to_owned(),
        Some(subpath) => join_canonical_paths(subpath, relative),
    }
}

/// Serves one provider-backed resource request through the application-config
/// provider executor. Only GET and HEAD are allowed; all other methods return
/// `405 Allow: GET, HEAD`.
pub(crate) async fn serve_provider_resource(
    executor: Arc<AppConfigResourceExecutor>,
    method: &str,
    query: Option<String>,
    headers: HeaderMap,
    route: AppConfigResourceRoute,
    policy: AppConfigProviderPolicy,
) -> Response<Body> {
    let request_id = new_request_id();
    let trace_context = resolve_trace_context(&headers, &request_id);
    let trace_id = trace_id_from_traceparent(&trace_context.traceparent)
        .unwrap_or(request_id.as_str())
        .to_owned();
    let suppress_body = method == "HEAD";
    let delivery_method = match method {
        "GET" => WebsiteDeliveryMethod::Get,
        "HEAD" => WebsiteDeliveryMethod::Head,
        _ => return finalize_response(method_not_allowed(), &request_id, false),
    };
    let provider_request = match provider_request(
        delivery_method,
        request_id.clone(),
        trace_id,
        &headers,
        &route,
    ) {
        Ok(request) => request,
        Err(RequestHeaderError::Range) => {
            return finalize_response(range_not_satisfiable(), &request_id, suppress_body)
        }
        Err(RequestHeaderError::Invalid) => {
            return finalize_response(
                text_response(StatusCode::BAD_REQUEST),
                &request_id,
                suppress_body,
            )
        }
    };
    let outcome = match route.handler {
        AppConfigResourceHandler::Static => {
            executor
                .serve_static(&route, &policy, &provider_request)
                .await
        }
        AppConfigResourceHandler::Wiki => {
            executor
                .serve_wiki(&route, &policy, &provider_request)
                .await
        }
    };
    let response = match outcome {
        Ok(outcome) => outcome_response(outcome, query.as_deref()),
        Err(error) => delivery_error_response(error),
    };
    finalize_response(response, &request_id, suppress_body)
}

fn provider_request(
    method: WebsiteDeliveryMethod,
    request_id: String,
    trace_id: String,
    headers: &HeaderMap,
    route: &AppConfigResourceRoute,
) -> Result<AppConfigProviderRequest, RequestHeaderError> {
    let conditions = WebsiteRequestConditions {
        if_match: bounded_header(headers, IF_MATCH, MAXIMUM_CONDITION_HEADER_BYTES)?,
        if_none_match: bounded_header(headers, IF_NONE_MATCH, MAXIMUM_CONDITION_HEADER_BYTES)?,
        if_modified_since: bounded_header(
            headers,
            IF_MODIFIED_SINCE,
            MAXIMUM_CONDITION_HEADER_BYTES,
        )?,
        if_unmodified_since: bounded_header(
            headers,
            IF_UNMODIFIED_SINCE,
            MAXIMUM_CONDITION_HEADER_BYTES,
        )?,
        if_range: bounded_header(headers, IF_RANGE, MAXIMUM_CONDITION_HEADER_BYTES)?,
    };
    let range = if method == WebsiteDeliveryMethod::Get {
        bounded_header(headers, RANGE, MAXIMUM_RANGE_HEADER_BYTES)?
            .map(|value| parse_range(&value))
            .transpose()?
    } else {
        None
    };
    // Request-level Accept-Language takes precedence; the resource-level
    // default locale applies when the client sends no preference.
    let locale = bounded_header(headers, ACCEPT_LANGUAGE, MAXIMUM_ACCEPT_LANGUAGE_BYTES)?
        .map(|value| parse_locale(&value))
        .transpose()?
        .flatten()
        .or_else(|| route.locale.clone());
    let spa_fallback_eligible = navigation_request(headers)?;
    Ok(AppConfigProviderRequest {
        method,
        request_id,
        trace_id,
        conditions,
        range,
        locale,
        spa_fallback_eligible,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::http::{header::IF_NONE_MATCH, HeaderValue};
    use http_body_util::BodyExt;
    use sdkwork_webserver_contract::provider::{
        OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
        ResolveWebsiteWikiRouteRequest, ResolvedWebsiteWikiContent, ValidateWebsiteResourceRequest,
        ValidatedWebsiteResource, WebsiteByteRange, WebsiteContentMetadata,
        WebsiteContentResolution, WebsiteProviderContentHandle, WebsiteProviderContentStream,
        WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderResult,
        WebsiteResourceProvider, WebsiteStaticContentProvider, WebsiteWikiContentKind,
        WebsiteWikiProvider, WebsiteWikiRouteResolution,
    };
    use sdkwork_webserver_core::{
        config::ProviderCachePolicy,
        website_runtime::{ProviderResourceReference, WebsiteProviderType},
        RouteConfig, RouteMatchConfig, RoutePathType,
    };
    use sdkwork_webserver_delivery_runtime::{
        AppConfigProviderPolicy, AppConfigResourceExecutor, AppConfigResourceHandler,
        AppConfigResourceRoute, WebsiteProviderRegistry,
    };

    use super::*;

    const TENANT_SCOPE_HASH: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";

    struct StaticOnlyProvider;

    struct OneChunkStream(Vec<u8>);

    #[async_trait]
    impl WebsiteProviderContentStream for OneChunkStream {
        async fn next_chunk(&mut self) -> WebsiteProviderResult<Option<Vec<u8>>> {
            Ok((!self.0.is_empty()).then(|| std::mem::take(&mut self.0)))
        }
    }

    #[async_trait]
    impl WebsiteResourceProvider for StaticOnlyProvider {
        fn maximum_content_bytes(&self) -> u64 {
            1024 * 1024
        }

        async fn validate_resource(
            &self,
            _request: &ValidateWebsiteResourceRequest,
        ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
            Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::Unavailable,
            ))
        }
    }

    #[async_trait]
    impl WebsiteStaticContentProvider for StaticOnlyProvider {
        async fn resolve_static_path(
            &self,
            request: &ResolveWebsiteStaticPathRequest,
        ) -> WebsiteProviderResult<WebsiteContentResolution> {
            if request.provider_relative_path != "/site/docs/index.html" {
                return Err(WebsiteProviderError::new(
                    WebsiteProviderErrorKind::NotFound,
                ));
            }
            if request
                .conditions
                .if_none_match
                .as_deref()
                .is_some_and(|value| value == "\"docs-v1\"")
            {
                return Ok(WebsiteContentResolution::NotModified);
            }
            Ok(WebsiteContentResolution::Found(
                sdkwork_webserver_contract::provider::ResolvedWebsiteContent {
                    content_handle: WebsiteProviderContentHandle::new("handle-1".to_owned())
                        .unwrap(),
                    metadata: WebsiteContentMetadata {
                        content_type: "text/html; charset=utf-8".to_owned(),
                        content_length: 5,
                        etag: "\"docs-v1\"".to_owned(),
                        last_modified: "Wed, 21 Oct 2015 07:28:00 GMT".to_owned(),
                        content_version: "v1".to_owned(),
                        provider_generation: "1".to_owned(),
                        range_supported: true,
                    },
                },
            ))
        }

        async fn open_static_content(
            &self,
            request: &OpenWebsiteContentRequest,
        ) -> WebsiteProviderResult<OpenedWebsiteContent> {
            let bytes = match request.range {
                Some(WebsiteByteRange {
                    start,
                    end_inclusive,
                    ..
                }) => b"hello"[start as usize..=end_inclusive.unwrap_or(4) as usize].to_vec(),
                None => b"hello".to_vec(),
            };
            Ok(OpenedWebsiteContent {
                stream: Box::new(OneChunkStream(bytes.clone())),
                content_length: bytes.len() as u64,
                content_range: request.range.map(|range| {
                    sdkwork_webserver_contract::provider::WebsiteContentRange {
                        start: range.start,
                        end_inclusive: range.end_inclusive.unwrap_or(4).min(4),
                        complete_length: 5,
                    }
                }),
            })
        }
    }

    struct FakeWikiProvider {
        last_locale: std::sync::Mutex<Option<String>>,
    }

    impl FakeWikiProvider {
        fn new() -> Self {
            Self {
                last_locale: std::sync::Mutex::new(None),
            }
        }

        fn observed_locale(&self) -> Option<String> {
            self.last_locale.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl WebsiteResourceProvider for FakeWikiProvider {
        fn maximum_content_bytes(&self) -> u64 {
            1024 * 1024
        }

        async fn validate_resource(
            &self,
            _request: &ValidateWebsiteResourceRequest,
        ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
            Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::Unavailable,
            ))
        }
    }

    #[async_trait]
    impl WebsiteWikiProvider for FakeWikiProvider {
        async fn resolve_wiki_route(
            &self,
            request: &ResolveWebsiteWikiRouteRequest,
        ) -> WebsiteProviderResult<WebsiteWikiRouteResolution> {
            *self.last_locale.lock().unwrap() = request.locale.clone();
            if request.route != "/getting-started" {
                return Err(WebsiteProviderError::new(
                    WebsiteProviderErrorKind::NotFound,
                ));
            }
            Ok(WebsiteWikiRouteResolution::Content(Box::new(
                ResolvedWebsiteWikiContent {
                    content_handle: WebsiteProviderContentHandle::new("wiki-handle".to_owned())
                        .unwrap(),
                    kind: WebsiteWikiContentKind::Html,
                    canonical_route: "/getting-started".to_owned(),
                    page_uuid: Some("page-1".to_owned()),
                    public_page_version: "3".to_owned(),
                    renderer_version: "2".to_owned(),
                    navigation_generation: "1".to_owned(),
                    search_generation: "1".to_owned(),
                    metadata: WebsiteContentMetadata {
                        content_type: "text/html; charset=utf-8".to_owned(),
                        content_length: 5,
                        etag: "\"wiki-v3\"".to_owned(),
                        last_modified: "Wed, 21 Oct 2015 07:28:00 GMT".to_owned(),
                        content_version: "v3".to_owned(),
                        provider_generation: "1".to_owned(),
                        range_supported: false,
                    },
                },
            )))
        }

        async fn open_wiki_content(
            &self,
            _request: &OpenWebsiteContentRequest,
        ) -> WebsiteProviderResult<OpenedWebsiteContent> {
            Ok(OpenedWebsiteContent {
                stream: Box::new(OneChunkStream(b"guide".to_vec())),
                content_length: 5,
                content_range: None,
            })
        }

        async fn retrieve_navigation(
            &self,
            _request: &sdkwork_webserver_contract::provider::WebsiteWikiCollectionRequest,
        ) -> WebsiteProviderResult<sdkwork_webserver_contract::provider::WebsiteWikiCollectionPage>
        {
            Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ))
        }

        async fn search_wiki(
            &self,
            _request: &sdkwork_webserver_contract::provider::WebsiteWikiCollectionRequest,
        ) -> WebsiteProviderResult<sdkwork_webserver_contract::provider::WebsiteWikiCollectionPage>
        {
            Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ))
        }
    }

    fn wiki_executor() -> (Arc<AppConfigResourceExecutor>, Arc<FakeWikiProvider>) {
        let provider = Arc::new(FakeWikiProvider::new());
        let mut registry = WebsiteProviderRegistry::new();
        registry
            .register_wiki(WebsiteProviderType::Knowledgebase, provider.clone())
            .unwrap();
        (
            Arc::new(
                AppConfigResourceExecutor::new(
                    Arc::new(registry),
                    "sdkwork-test-web".to_owned(),
                    TENANT_SCOPE_HASH.to_owned(),
                )
                .unwrap(),
            ),
            provider,
        )
    }

    fn route_config(path: &str, path_type: RoutePathType) -> RouteConfig {
        RouteConfig {
            id: "docs-route".to_owned(),
            route_match: RouteMatchConfig {
                path_type,
                path: path.to_owned(),
                methods: None,
            },
            resource_ref: "docs-drive".to_owned(),
            access: Vec::new(),
            limit_req: Vec::new(),
            limit_conn: Vec::new(),
            rewrite: Vec::new(),
            auth_basic: None,
            sub_filter: None,
            secure_link: None,
        }
    }

    fn executor() -> Arc<AppConfigResourceExecutor> {
        let mut registry = WebsiteProviderRegistry::new();
        registry
            .register_static(WebsiteProviderType::Drive, Arc::new(StaticOnlyProvider))
            .unwrap();
        Arc::new(
            AppConfigResourceExecutor::new(
                Arc::new(registry),
                "sdkwork-test-web".to_owned(),
                TENANT_SCOPE_HASH.to_owned(),
            )
            .unwrap(),
        )
    }

    fn drive_route(provider_path: String) -> AppConfigResourceRoute {
        AppConfigResourceRoute {
            virtual_host_id: "mixed-web".to_owned(),
            route_id: "docs-route".to_owned(),
            resource_id: "docs-drive".to_owned(),
            provider: ProviderResourceReference {
                provider_type: WebsiteProviderType::Drive,
                provider_resource_uuid: "11111111-1111-4111-8111-111111111701".to_owned(),
                provider_contract_version: "drive.website-root.v1".to_owned(),
            },
            handler: AppConfigResourceHandler::Static,
            provider_relative_path: provider_path,
            index_files: vec!["index.html".to_owned()],
            spa_fallback: Some("index.html".to_owned()),
            directory_request: true,
            locale: None,
            cache: ProviderCachePolicy::default(),
        }
    }

    fn wiki_route(route_path: String, locale: Option<String>) -> AppConfigResourceRoute {
        AppConfigResourceRoute {
            virtual_host_id: "mixed-web".to_owned(),
            route_id: "wiki-route".to_owned(),
            resource_id: "wiki-knowledgebase".to_owned(),
            provider: ProviderResourceReference {
                provider_type: WebsiteProviderType::Knowledgebase,
                provider_resource_uuid: "11111111-1111-4111-8111-111111111501".to_owned(),
                provider_contract_version: "knowledgebase.wiki-publication.v1".to_owned(),
            },
            handler: AppConfigResourceHandler::Wiki,
            provider_relative_path: route_path,
            index_files: Vec::new(),
            spa_fallback: None,
            directory_request: false,
            locale,
            cache: ProviderCachePolicy::default(),
        }
    }

    fn policy() -> AppConfigProviderPolicy {
        AppConfigProviderPolicy {
            provider_timeout_ms: 5_000,
            maximum_object_bytes: 1024 * 1024,
        }
    }

    #[tokio::test]
    async fn drive_route_serves_translated_provider_path_with_headers() {
        // /docs/ -> route prefix stripped, subpath joined -> /site/docs/,
        // directory request -> /site/docs/index.html candidate.
        assert_eq!(
            translate_provider_path(
                &route_config("/docs", RoutePathType::Prefix),
                "/docs/",
                Some("/site/docs"),
            ),
            "/site/docs"
        );
        let response = serve_provider_resource(
            executor(),
            "GET",
            None,
            HeaderMap::new(),
            drive_route("/site/docs".to_owned()),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            response.headers().get(axum::http::header::ETAG).unwrap(),
            "\"docs-v1\""
        );
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            b"hello"
        );
    }

    #[tokio::test]
    async fn drive_route_supports_conditions_and_range() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"docs-v1\""));
        let response = serve_provider_resource(
            executor(),
            "GET",
            None,
            headers,
            drive_route("/site/docs".to_owned()),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

        let mut headers = HeaderMap::new();
        headers.insert(RANGE, HeaderValue::from_static("bytes=1-3"));
        let response = serve_provider_resource(
            executor(),
            "GET",
            None,
            headers,
            drive_route("/site/docs".to_owned()),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_RANGE)
                .unwrap(),
            "bytes 1-3/5"
        );
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            b"ell"
        );
    }

    #[tokio::test]
    async fn drive_route_missing_provider_path_is_not_found() {
        let response = serve_provider_resource(
            executor(),
            "GET",
            None,
            HeaderMap::new(),
            drive_route("/site/docs/missing.html".to_owned()),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn provider_resources_reject_non_get_head_methods() {
        let response = serve_provider_resource(
            executor(),
            "POST",
            None,
            HeaderMap::new(),
            drive_route("/site/docs".to_owned()),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            response.headers().get(axum::http::header::ALLOW).unwrap(),
            "GET, HEAD"
        );
    }

    #[tokio::test]
    async fn head_suppresses_body_and_keeps_content_length() {
        let response = serve_provider_resource(
            executor(),
            "HEAD",
            None,
            HeaderMap::new(),
            drive_route("/site/docs".to_owned()),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .unwrap(),
            "5"
        );
        assert!(response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty());
    }

    #[tokio::test]
    async fn wiki_route_serves_content_with_resource_level_locale_default() {
        let (executor, provider) = wiki_executor();
        let response = serve_provider_resource(
            executor,
            "GET",
            None,
            HeaderMap::new(),
            wiki_route("/getting-started".to_owned(), Some("zh-CN".to_owned())),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_LOCATION)
                .unwrap(),
            "/getting-started"
        );
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            b"guide"
        );
        // No Accept-Language header: the resource-level locale applies.
        assert_eq!(provider.observed_locale().as_deref(), Some("zh-CN"));
    }

    #[tokio::test]
    async fn wiki_route_request_locale_takes_precedence_over_resource_default() {
        let (executor, provider) = wiki_executor();
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US"),
        );
        let response = serve_provider_resource(
            executor,
            "GET",
            None,
            headers,
            wiki_route("/getting-started".to_owned(), Some("zh-CN".to_owned())),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(provider.observed_locale().as_deref(), Some("en-US"));
    }

    #[tokio::test]
    async fn wiki_route_missing_route_is_not_found() {
        let (executor, _) = wiki_executor();
        let response = serve_provider_resource(
            executor,
            "GET",
            None,
            HeaderMap::new(),
            wiki_route("/nope".to_owned(), None),
            policy(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn translate_provider_path_root_and_empty_remainder() {
        let route = route_config("/docs", RoutePathType::Exact);
        assert_eq!(translate_provider_path(&route, "/docs", None), "/");
        assert_eq!(
            translate_provider_path(&route, "/docs", Some("/site/docs")),
            "/site/docs"
        );
        assert_eq!(
            translate_provider_path(&route, "/docs/page.html", Some("/site/docs")),
            "/site/docs/page.html"
        );
    }
}
