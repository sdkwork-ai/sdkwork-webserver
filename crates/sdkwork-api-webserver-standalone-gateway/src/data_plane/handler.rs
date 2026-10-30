use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{
        header::{
            ACCEPT_ENCODING, ACCEPT_RANGES, AUTHORIZATION, CONNECTION, CONTENT_LENGTH,
            CONTENT_TYPE, EXPECT, HOST, LAST_MODIFIED, LOCATION, RETRY_AFTER, TE,
            TRANSFER_ENCODING, USER_AGENT, WWW_AUTHENTICATE,
        },
        HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode, Version,
    },
};
use futures_util::StreamExt;
use sdkwork_webserver_core::{
    apply_rewrites_with, evaluate_access, evaluate_auth_basic, normalize_authority_host,
    prefer_h5_surface, verify_secure_link,
    website_runtime::{ProviderResourceReference, WebsiteProviderType},
    AccessDecision, AuthBasicDecision, CustomHeaderConfig, ErrorPageConfig, RedirectPolicyConfig,
    ResourceConfig, RewriteOutcome, RoutePathType, SecureLinkFailure, SecurityHeadersConfig,
    SelectedRoute, XFrameOptions, DEFAULT_MAX_RANGES, MAX_REWRITE_INTERNAL_REDIRECTS,
};
use sdkwork_webserver_delivery_runtime::{
    AppConfigProviderPolicy, AppConfigResourceHandler, AppConfigResourceRoute,
};
use sdkwork_webserver_drive_provider::DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION;
use sdkwork_webserver_knowledgebase_provider::KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION;

use super::{
    forwarded_scheme::resolve_request_scheme,
    limit_conn::LimitConnDecision,
    limit_req::LimitReqDecision,
    metrics::RequestRejection,
    proxy::{request_body_timeout_response, text_response},
    proxy_body::RequestBodyFailure,
    proxy_protocol::DownstreamConnectionInfo,
    real_ip::resolve_client_ip,
    request_admission::hold_request_permit,
    request_body_timeout::RequestBodyTimeout,
    request_gate::RequestAdmissionRejection,
    request_uri::{validate_request_uri, RequestUriError},
    runtime::RuntimeGeneration,
    static_files::serve_static,
    website_delivery::serve_website_request,
    ListenerState,
};

pub async fn route_request(
    ConnectInfo(connection): ConnectInfo<DownstreamConnectionInfo>,
    State(state): State<ListenerState>,
    request: Request<Body>,
) -> Response<Body> {
    let peer = connection.client_peer;
    let transport_peer = connection.transport_peer;
    let _proxy_protocol = connection.proxy_protocol;
    let version = request.version();
    // Traffic usage metering inputs captured before the request is consumed.
    let metering_host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(sdkwork_webserver_core::normalize_authority_host);
    let metering_ingress = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    // Captured before `state` and `request` move into the routing call.
    let metering_is_website_listener = state.website_delivery.is_some();
    let metering_meter = state.usage_meter.clone();
    let metering_fallback = state.deploy_fallback.clone();
    let metering_listener_id = state.listener_id.clone();
    let mut admitted = match state.runtime.request_gate.try_begin() {
        Ok(admitted) => admitted,
        Err(RequestAdmissionRejection::Saturated) => {
            state
                .runtime
                .metrics
                .record_request_rejection(RequestRejection::Capacity);
            return overload_response(version);
        }
        Err(RequestAdmissionRejection::ResourcePressure) => {
            state
                .runtime
                .metrics
                .record_request_rejection(RequestRejection::ResourcePressure);
            return resource_pressure_response(version);
        }
    };
    let response_body_idle_timeout = Duration::from_millis(
        state
            .runtime
            .current()
            .app
            .config()
            .limits
            .response_body_idle_timeout_ms,
    );
    let response =
        route_admitted_request(peer, transport_peer, state, request, &mut admitted).await;
    // App-config listeners (no website delivery executor) meter every served
    // response here; website listeners meter inside the delivery layer where
    // route identity and exact bytes are known. App-domain fallback responses
    // carry the Deploy attribution cached by the resolver.
    if !metering_is_website_listener {
        if let Some(meter) = metering_meter {
            if let Some(hostname) = metering_host {
                let attribution = metering_fallback
                    .as_ref()
                    .and_then(|fallback| fallback.attribution(&hostname))
                    .unwrap_or_default();
                let egress_bytes = response
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                meter.record(crate::usage_metering::MeteredRequest {
                    hostname: &hostname,
                    server_ip: transport_peer.ip(),
                    server_port: transport_peer.port(),
                    listener_id: &metering_listener_id,
                    attribution: &attribution,
                    ingress_bytes: metering_ingress,
                    egress_bytes,
                    status_class: status_class_label(response.status().as_u16()),
                });
            }
        }
    }
    hold_request_permit(response, admitted, response_body_idle_timeout)
}

/// Whether a request for `host` must be forwarded to a sibling instance.
///
/// `serves_host` is true when the cluster overlay claims the host;
/// `from_sibling` is true when the request carried the east-west hop marker
/// ([`crate::tunnel_bridge::CLUSTER_HOP_HEADER`]).
///
/// A hop is taken **at most once**. A sibling that receives the hop also sees
/// the host in its own overlay, so without the second condition two instances
/// whose round-robin cursors point at each other would bounce the same request
/// forever (and an instance that picked itself would relay to itself).
///
/// The provenance comes from the marker rather than from the peer address on
/// purpose: an address-based rule misfires whenever a legitimate client shares
/// an address with an instance — the edge and its instances on one host, a
/// client behind the same NAT, a same-subnet visitor — and silently drops that
/// traffic out of the balancing pool. The marker's own trust model is stated on
/// the constant.
#[must_use]
const fn should_relay_to_sibling(serves_host: bool, from_sibling: bool) -> bool {
    serves_host && !from_sibling
}

/// Resolve the virtual host whose nginx request-boundary policies
/// (`merge_slashes`, `underscores_in_headers`) govern this request.
///
/// The host comes from the request `Host` header when it selects a declared
/// virtual host; otherwise the listener's default server applies, which is the
/// host nginx would use while parsing the request line and headers.
fn select_policy_host<'a>(
    generation: &'a RuntimeGeneration,
    listener_id: &str,
    authority: Option<&str>,
) -> Option<&'a sdkwork_webserver_core::VirtualHostConfig> {
    if let Some(authority) = authority {
        if let Some(host) = generation.app.select_virtual_host(listener_id, authority) {
            return Some(host);
        }
    }
    let default_ref = generation
        .app
        .listener(listener_id)?
        .default_virtual_host_ref
        .as_deref()?;
    generation
        .app
        .config()
        .virtual_hosts
        .iter()
        .find(|host| host.id == default_ref)
}

async fn route_admitted_request(
    peer: SocketAddr,
    transport_peer: SocketAddr,
    state: ListenerState,
    mut request: Request<Body>,
    admitted: &mut super::request_gate::RequestAdmissionPermit,
) -> Response<Body> {
    // `$invalid_referer` needs the inbound `Referer`; capture the headers
    // before dispatch consumes the request.
    let request_headers = request.headers().clone();
    // Captured before dispatch consumes the request: the compression layer
    // negotiates after the handler returns, so the handler needs the same input
    // to publish a validator that matches the negotiated representation.
    let accept_encoding = request.headers().get(ACCEPT_ENCODING).cloned();
    // nginx `gzip_disable` / `gzip_proxied` inspect request headers
    // (`User-Agent`, `Via`, `Authorization`); the compression predicate only
    // sees the response, so the facts ride along as a response extension.
    let gzip_facts = super::gzip_predicate::GzipRequestFacts {
        user_agent: request
            .headers()
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        via: request.headers().contains_key(axum::http::header::VIA),
        authorization: request.headers().contains_key(AUTHORIZATION),
    };
    // Captured before dispatch consumes the request: nginx `add_header` values
    // may reference `$request_uri`/`$args`/`$request_method`, and the response
    // header choke point runs after every content handler has taken the
    // request. `$request_uri` is nginx's *original* target (normalization and
    // rewrites do not change it), so it is captured from the raw URI.
    let raw_request_uri = request
        .uri()
        .path_and_query()
        .map(|target| target.as_str().to_owned())
        .unwrap_or_else(|| request.uri().path().to_owned());
    let mut request_query = request.uri().query().map(str::to_owned);
    // A request that arrives carrying the east-west hop marker was already
    // relayed once by a sibling, so this process is the last hop and serves it
    // locally (see `should_relay_to_sibling`). One listener accepts both
    // visitor and sibling traffic, so a visitor can present the marker too;
    // that is the same trust level as `x-cluster-lb-strategy`, and the effect
    // is confined to the request presenting it.
    let from_cluster_sibling = request
        .headers()
        .contains_key(crate::tunnel_bridge::CLUSTER_HOP_HEADER);
    if let Err((status, message)) = validate_request_framing(request.headers(), request.version()) {
        if let Some(response) = classify_request(&state, admitted, false, request.version()) {
            return response;
        }
        return text_response(status, message);
    }
    let generation = state.runtime.current();
    let Some(listener) = generation.app.listener(&state.listener_id) else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "listener configuration is unavailable\n",
        );
    };
    // nginx `$server_port`: the port this listener accepts on.
    let listener_port = listener.port;
    // nginx request-boundary policies (`underscores_in_headers`,
    // `merge_slashes`) resolve against the selected virtual host. The header
    // policy is applied before any module observes the request headers, which
    // is where nginx drops an underscore header during parsing. When the host
    // is not declared the listener's default server governs, matching nginx's
    // parse-before-virtual-server-selection behavior for the URI.
    let peeked_authority = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(sdkwork_webserver_core::normalize_authority_host);
    let policy_host =
        select_policy_host(&generation, &state.listener_id, peeked_authority.as_deref());
    if policy_host.is_some_and(|host| !host.underscores_in_headers) {
        let underscored: Vec<HeaderName> = request
            .headers()
            .keys()
            .filter(|name| name.as_str().contains('_'))
            .cloned()
            .collect();
        for name in underscored {
            request.headers_mut().remove(name);
        }
    }
    let merge_slashes = policy_host.is_none_or(|host| host.merge_slashes);
    let client_ip = match resolve_client_ip(
        peer.ip(),
        request.headers(),
        listener.trusted_proxy.as_ref(),
    ) {
        Ok(client_ip) => client_ip,
        Err(_) => {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            return invalid_forwarded_identity_response(request.version());
        }
    };
    let scheme = match resolve_request_scheme(
        transport_peer.ip(),
        request.headers(),
        listener.trusted_proxy.as_ref(),
        state.is_tls,
    ) {
        Ok(scheme) => scheme,
        Err(_) => {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            return invalid_forwarded_scheme_response(request.version());
        }
    };
    if content_length_exceeds(
        request.headers(),
        generation.app.config().limits.max_request_body_bytes,
    ) {
        if let Some(response) = classify_request(&state, admitted, false, request.version()) {
            return response;
        }
        return text_response(StatusCode::PAYLOAD_TOO_LARGE, "request body is too large\n");
    }
    let authority = match request_authority(&request) {
        Ok(authority) => authority,
        Err((status, message)) => {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            return text_response(status, message);
        }
    };
    let normalized_path = match validate_request_uri(
        request.uri(),
        &generation.app.config().limits,
        merge_slashes,
    ) {
        Ok(path) => path,
        Err(error) => {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            return request_uri_error_response(request.version(), error);
        }
    };
    let method = request.method().as_str().to_owned();
    let mut path = normalized_path;
    // nginx `map` results for this request. The executable source vocabulary
    // is the request facts below; every declared map is resolved once, before
    // the rewrite loop, and reused by the rewrite phase and by every response
    // layer. Bounded gap: a map keyed on `$uri`/`$document_uri` observes the
    // pre-rewrite URI (nginx evaluates the variable lazily at first use).
    let map_results = if generation.app.config().maps.is_empty() {
        super::map_variables::MapResults::default()
    } else {
        let host_server_name = generation
            .app
            .select_virtual_host(&state.listener_id, &authority)
            .and_then(|host| host.server_names.first().cloned())
            .unwrap_or_default();
        let facts = super::map_variables::MapRequestFacts {
            scheme: scheme.as_str(),
            authority: &authority,
            client_ip,
            listener_port,
            request_uri: &raw_request_uri,
            path: &path,
            query: request_query.as_deref(),
            method: &method,
            server_name: &host_server_name,
            headers: request.headers(),
        };
        super::map_variables::MapResults::evaluate(generation.app.config(), &facts)
    };
    // nginx: once a rewrite changes the URI, proxying forwards the rewritten
    // URI; without rewrites the raw request path is preserved verbatim.
    let mut path_rewritten = false;
    if super::acme_challenge::acme_http01_request_enabled(listener, &path) {
        if let Some(response) = classify_request(&state, admitted, false, request.version()) {
            return response;
        }
        if let Some(response) =
            super::acme_challenge::serve_acme_http01_challenge(&state, &path, &method).await
        {
            return response;
        }
    }
    // 微信公众号域名验证文件：与 ACME 挑战同级的窄优先级命名空间。只在
    // "单段 .txt 根路径 + 该主机名的 zone 上报了同名文件"时接管；其余请求
    // 原样放行给宿主应用。共享源未装载（非 management 构建/无共享库）时
    // 整段跳过。
    if path.ends_with(".txt") {
        if let Some(source) = super::wechat_verify::shared_source() {
            let host = sdkwork_webserver_core::normalize_authority_host(&authority)
                .unwrap_or_else(|| authority.to_ascii_lowercase());
            if let Some(response) = super::wechat_verify::serve_wechat_verification_file(
                source.as_ref(),
                &host,
                &path,
                &method,
            )
            .await
            {
                if let Some(response) = classify_request(&state, admitted, false, request.version())
                {
                    return response;
                }
                return response;
            }
        }
    }
    // Tunnel routes: a Host with a registered tunnel route is relayed toward
    // the owning agent (PRD §30) — but never a host the app-publishing surface
    // serves. Tunnel agent credentials are shared (see
    // `sdkwork_webserver_tunnel::security`), so a tunnel route must not shadow a
    // published host. The surface's own route table settles that, not the status
    // it would have returned: a tunnel host and a served host with a missing
    // page both answer 404, and a host nobody serves and a served host whose
    // provider is briefly down both answer 503. On an entry point with no
    // delivery surface the probe is false, so this is the plain "check the
    // registry before local routing" of the non-publishing shape.
    //
    // The path is part of the question: a route may claim only some paths of
    // its host (`locations`, FRP parity), and a path it does not claim must
    // keep falling through to the local routing instead of being answered by
    // the tunnel.
    if let Some(host) = registered_tunnel_host(&state, &authority, &path) {
        if !website_surface_declares_host(&state, &authority) {
            // The visitor port is only knowable when the connection peer *is*
            // the visitor: behind a trusted proxy the forwarded headers carry
            // an address, never a port, and inventing one would put a wrong
            // source port into a PROXY-protocol header. `0` is the protocol's
            // own "unknown" encoding.
            let visitor_port = if peer.ip() == client_ip {
                peer.port()
            } else {
                0
            };
            return relay_registered_tunnel(
                &state,
                admitted,
                &host,
                client_ip,
                visitor_port,
                request,
            )
            .await;
        }
    }
    if let Some(executor) = state.website_delivery.clone() {
        if let Some(response) = classify_request(&state, admitted, false, request.version()) {
            return response;
        }
        let request_failure = RequestBodyFailure::default();
        let (parts, body) = request.into_parts();
        let limits = &generation.app.config().limits;
        let body = RequestBodyTimeout::new_observed(
            body,
            Duration::from_millis(limits.request_body_start_timeout_ms),
            Duration::from_millis(limits.request_body_idle_timeout_ms),
            request_failure.clone(),
            state.runtime.metrics.clone(),
        );
        let request = Request::from_parts(parts, Body::new(body));
        let request = match drain_bounded_request_body(
            request,
            limits.max_request_body_bytes,
            &request_failure,
        )
        .await
        {
            Ok(request) => request,
            Err(response) => return response,
        };
        let query = request.uri().query().map(str::to_owned);
        let delivery_method = request.method().clone();
        let delivery_headers = request.headers().clone();
        let metering =
            state
                .usage_meter
                .as_ref()
                .map(|meter| crate::usage_metering::MeteringContext {
                    server_ip: transport_peer.ip(),
                    server_port: transport_peer.port(),
                    listener_id: state.listener_id.clone(),
                    meter: meter.clone(),
                    count_not_found: state.deploy_fallback.is_none(),
                });
        let response = serve_website_request(
            executor,
            scheme.website_delivery_scheme(),
            authority.clone(),
            path.clone(),
            query.clone(),
            delivery_method.clone(),
            delivery_headers.clone(),
            metering,
        )
        .await;
        if response.status() == StatusCode::NOT_FOUND {
            if let Some(fallback_response) = serve_deploy_fallback(
                &state,
                scheme.website_delivery_scheme(),
                authority,
                path,
                query,
                &delivery_method,
                &delivery_headers,
                transport_peer,
            )
            .await
            {
                if generation.app.config().observability.access_log {
                    tracing::info!(
                        config_generation = generation.id,
                        config_revision = %generation.revision,
                        listener_id = %state.listener_id,
                        scheme = scheme.as_str(),
                        method = %method,
                        status = fallback_response.status().as_u16(),
                        "app-domain fallback request served"
                    );
                }
                return fallback_response;
            }
        }
        if generation.app.config().observability.access_log {
            tracing::info!(
                config_generation = generation.id,
                config_revision = %generation.revision,
                listener_id = %state.listener_id,
                scheme = scheme.as_str(),
                method = %method,
                status = response.status().as_u16(),
                "website request served"
            );
        }
        return response;
    }
    // Cluster auto-routing: a Host served by the cluster's discovered
    // instances (topology overlay, refreshed from the registry) is picked
    // by the configured load balancing strategy and relayed to the winning
    // instance over the internal network — at most once per request.
    {
        let snapshot = state.runtime.cluster_overlay.load_full();
        let host = sdkwork_webserver_core::normalize_authority_host(&authority)
            .unwrap_or_else(|| authority.to_ascii_lowercase());
        let serves_host = snapshot.group_for_host(&host).is_some();
        let relay = should_relay_to_sibling(serves_host, from_cluster_sibling);
        if relay {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            let strategy_override = request
                .headers()
                .get("x-cluster-lb-strategy")
                .and_then(|value| value.to_str().ok())
                .map(sdkwork_webserver_cluster::LoadBalancingStrategy::parse)
                .filter(|(_, known)| *known)
                .map(|(strategy, _)| strategy);
            let pick_key = sdkwork_webserver_cluster::PickKey::ClientIp(&client_ip.to_string());
            return match sdkwork_webserver_cluster::ClusterLoadBalancer::pick(
                &snapshot,
                &host,
                pick_key,
                strategy_override,
            ) {
                Some(lease) => {
                    if generation.app.config().observability.access_log {
                        tracing::info!(
                            host = %host,
                            instance = %lease.instance().id,
                            strategy = lease.decision().strategy.as_str(),
                            "cluster request routed"
                        );
                    }
                    let outcome = crate::tunnel_bridge::relay_cluster_http(
                        lease.endpoint(),
                        client_ip,
                        crate::tunnel_bridge::RelayBodyLimits::from_config(
                            generation.app.config().limits.clone(),
                        ),
                        request,
                    )
                    .await;
                    drop(lease);
                    match outcome {
                        Ok(response) => response,
                        Err(crate::tunnel_bridge::TunnelRelayError::NoRoute) => {
                            text_response(StatusCode::NOT_FOUND, "cluster route is unavailable\n")
                        }
                        Err(crate::tunnel_bridge::TunnelRelayError::Denied { .. }) => {
                            // Cluster relays carry the member's own credential,
                            // never a route policy, so a denial here means the
                            // lease raced teardown.
                            text_response(StatusCode::FORBIDDEN, "cluster relay denied\n")
                        }
                        Err(crate::tunnel_bridge::TunnelRelayError::Unavailable) => text_response(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "no healthy cluster instance is available\n",
                        ),
                        Err(crate::tunnel_bridge::TunnelRelayError::Failure(error)) => {
                            tracing::warn!(
                                host = %host,
                                error = %error,
                                "cluster relay failed"
                            );
                            text_response(StatusCode::BAD_GATEWAY, "cluster relay failed\n")
                        }
                    }
                }
                None => text_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no healthy cluster instance is available\n",
                ),
            };
        } else if serves_host {
            tracing::debug!(
                host = %host,
                peer = %transport_peer,
                "cluster hop received; serving locally"
            );
        }
    }
    let mut rewrite_redirects = 0_u32;
    // nginx `r->internal`: set once the request has been internally redirected
    // (`rewrite … last`, `try_files` named target, `error_page`). Only then may
    // an `internal` location serve it.
    let mut internal_request = false;
    // Request variables for the bounded rewrite/variable engine. Server-level
    // `set`/`if`/`rewrite` instructions run before location selection, exactly
    // like nginx's server rewrite phase.
    let phase_server_name = generation
        .app
        .select_virtual_host(&state.listener_id, &authority)
        .and_then(|host| host.server_names.first().cloned())
        .unwrap_or_default();
    let server_protocol = match request.version() {
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_2 => "HTTP/2.0",
        Version::HTTP_3 => "HTTP/3.0",
        _ => "HTTP/1.1",
    };
    let mut request_variables = super::request_variables::RequestVariables::new(
        request.headers(),
        request.method(),
        scheme.as_str(),
        &authority,
        client_ip,
        listener_port,
        &path,
        &raw_request_uri,
        request_query.as_deref(),
        &phase_server_name,
        server_protocol,
    );
    // Declared `map` results are variables too: seed them so `set`/`if`/
    // dynamic targets can reference them through the same lookup.
    for (name, value) in map_results.pairs() {
        request_variables.seed(name, value.clone());
    }
    let mut phase_headers: Vec<CustomHeaderConfig> = Vec::new();
    if let Some(host_config) = generation
        .app
        .select_virtual_host(&state.listener_id, &authority)
    {
        if !host_config.rewrite_phase.is_empty() {
            match super::rewrite_phase::execute(
                &host_config.rewrite_phase,
                &mut request_variables,
                &mut phase_headers,
                request.version(),
            ) {
                super::rewrite_phase::PhaseControl::Continue => {}
                super::rewrite_phase::PhaseControl::Respond(response) => {
                    return super::rewrite_phase::apply_headers(response, &phase_headers);
                }
                super::rewrite_phase::PhaseControl::Rewrite {
                    path: next_path,
                    query: next_query,
                    reselect,
                } => {
                    path_rewritten = true;
                    path = next_path;
                    request_query = next_query;
                    request_variables.apply_rewrite(&path, request_query.as_deref());
                    if reselect {
                        rewrite_redirects = rewrite_redirects.saturating_add(1);
                        internal_request = true;
                        if rewrite_redirects > MAX_REWRITE_INTERNAL_REDIRECTS {
                            return text_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "rewrite redirect limit exceeded\n",
                            );
                        }
                    }
                }
            }
        }
    }
    let mut selected = loop {
        let Some(candidate) =
            generation
                .app
                .select_route(&state.listener_id, &authority, &path, &method)
        else {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            // App publishing domain fallback: the host is not declared in the
            // local configuration; ask the Deploy control plane for a server
            // before falling back to 404.
            if let Some(fallback_response) = serve_deploy_fallback(
                &state,
                scheme.website_delivery_scheme(),
                authority.clone(),
                path.clone(),
                request.uri().query().map(str::to_owned),
                request.method(),
                request.headers(),
                transport_peer,
            )
            .await
            {
                return fallback_response;
            }
            // The host *is* configured but no location matched. nginx still
            // runs its headers filter for the generated 404, so the matched
            // server's `add_header` policy applies here too.
            if let Some(host_config) = generation
                .app
                .select_virtual_host(&state.listener_id, &authority)
            {
                let policy = host_config
                    .security_headers
                    .as_ref()
                    .map(|security_headers| security_headers.custom_headers.as_slice())
                    .filter(|headers| !headers.is_empty());
                if policy.is_some() {
                    let variables = ResponseHeaderVariables {
                        scheme: scheme.as_str(),
                        authority: &authority,
                        client_ip,
                        server_port: listener_port,
                        method: &method,
                        request_uri: &raw_request_uri,
                        server_name: host_config.server_names.first().map_or("", String::as_str),
                        maps: Some(&map_results),
                        invalid_referer: referer_invalid_for(host_config, None, &request_headers),
                    };
                    return apply_nginx_response_headers(
                        text_response(StatusCode::NOT_FOUND, "route was not found\n"),
                        policy,
                        &variables,
                        &path,
                        request_query.as_deref(),
                    );
                }
            }
            return text_response(StatusCode::NOT_FOUND, "route was not found\n");
        };
        // nginx location `client_max_body_size`: the effective per-location
        // limit is enforced once the location is known (the pre-routing check
        // can only use the app-level maximum declared across locations).
        if let Some(limit) = candidate.route.max_request_body_bytes {
            if content_length_exceeds(request.headers(), limit) {
                return text_response(StatusCode::PAYLOAD_TOO_LARGE, "request body is too large\n");
            }
        }
        // nginx `internal`: an external request that matches the location is
        // answered with the generated 404 before the rewrite phase runs. The
        // internal location still owns the response policy: its `add_header`
        // array and `error_page` mappings apply to that 404
        // (differential-confirmed against nginx 1.29.6).
        if candidate.route.internal && !internal_request {
            let response = text_response(StatusCode::NOT_FOUND, "route was not found\n");
            let error_page_saved = if candidate.route.error_pages.is_empty()
                && candidate.virtual_host.error_pages.is_empty()
            {
                None
            } else {
                Some((
                    request.method().clone(),
                    request.headers().clone(),
                    request_query.clone(),
                ))
            };
            let response = apply_error_pages(
                &state,
                &generation,
                &candidate,
                &authority,
                response,
                error_page_saved.as_ref(),
                &path,
                request_query.as_deref(),
                scheme.as_str(),
                &map_results,
            )
            .await;
            let variables = ResponseHeaderVariables {
                scheme: scheme.as_str(),
                authority: &authority,
                client_ip,
                server_port: listener_port,
                method: &method,
                request_uri: &raw_request_uri,
                server_name: candidate
                    .virtual_host
                    .server_names
                    .first()
                    .map_or("", String::as_str),
                maps: Some(&map_results),
                invalid_referer: referer_invalid_for(
                    candidate.virtual_host,
                    Some(candidate.route),
                    &request_headers,
                ),
            };
            return finalize_response_headers(
                response,
                &candidate,
                &variables,
                &path,
                request_query.as_deref(),
            );
        }
        // nginx location rewrite phase: the location regex captures are
        // available to `set`/`if`/`rewrite` and to the dynamic target/root.
        let location_captures = super::rewrite_phase::location_captures(candidate.route, &path);
        request_variables.set_captures(location_captures);
        if !candidate.route.rewrite_phase.is_empty() {
            match super::rewrite_phase::execute(
                &candidate.route.rewrite_phase,
                &mut request_variables,
                &mut phase_headers,
                request.version(),
            ) {
                super::rewrite_phase::PhaseControl::Continue => {}
                super::rewrite_phase::PhaseControl::Respond(response) => {
                    let error_page_saved = if candidate.route.error_pages.is_empty()
                        && candidate.virtual_host.error_pages.is_empty()
                    {
                        None
                    } else {
                        Some((
                            request.method().clone(),
                            request.headers().clone(),
                            request_query.clone(),
                        ))
                    };
                    let response = apply_error_pages(
                        &state,
                        &generation,
                        &candidate,
                        &authority,
                        response,
                        error_page_saved.as_ref(),
                        &path,
                        request_query.as_deref(),
                        scheme.as_str(),
                        &map_results,
                    )
                    .await;
                    return super::rewrite_phase::apply_headers(response, &phase_headers);
                }
                super::rewrite_phase::PhaseControl::Rewrite {
                    path: next_path,
                    query: next_query,
                    reselect,
                } => {
                    path_rewritten = true;
                    path = next_path;
                    request_query = next_query;
                    request_variables.apply_rewrite(&path, request_query.as_deref());
                    if reselect {
                        rewrite_redirects = rewrite_redirects.saturating_add(1);
                        internal_request = true;
                        if rewrite_redirects > MAX_REWRITE_INTERNAL_REDIRECTS {
                            return text_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "rewrite redirect limit exceeded\n",
                            );
                        }
                        continue;
                    }
                    // `rewrite … break` stops the rewrite module and keeps this
                    // location; the content handler sees the rewritten path.
                    break candidate;
                }
            }
        }
        if candidate.route.rewrite.is_empty() {
            break candidate;
        }
        let query = request.uri().query();
        // nginx `rewrite` replacements are scripts: captures plus request
        // variables. Resolve declared map results in the replacement after the
        // captures have been substituted.
        let expanded_replacement = |template: &str| map_results.expand_tokens(template);
        match apply_rewrites_with(
            &path,
            query,
            &candidate.route.rewrite,
            &expanded_replacement,
        ) {
            Ok(RewriteOutcome::Continue {
                path: next_path, ..
            }) => {
                path_rewritten |= next_path != path;
                path = next_path;
                break candidate;
            }
            Ok(RewriteOutcome::Reselect {
                path: next_path, ..
            }) => {
                rewrite_redirects = rewrite_redirects.saturating_add(1);
                internal_request = true;
                if rewrite_redirects > MAX_REWRITE_INTERNAL_REDIRECTS {
                    if let Some(response) =
                        classify_request(&state, admitted, false, request.version())
                    {
                        return response;
                    }
                    return text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "rewrite redirect limit exceeded\n",
                    );
                }
                path_rewritten = true;
                path = next_path;
            }
            Ok(RewriteOutcome::Redirect { status, location }) => {
                if let Some(response) = classify_request(&state, admitted, false, request.version())
                {
                    return response;
                }
                let response = redirect_response(
                    status,
                    &location,
                    scheme.as_str(),
                    &authority,
                    &path,
                    request.uri().query(),
                    &map_results,
                );
                // nginx's headers filter runs for these redirect statuses too
                // (301/302/303/307/308 are in the default `add_header` status
                // list), and `error_page` can map a 3xx final status exactly as
                // it maps a 4xx/5xx one (differential-confirmed against nginx
                // 1.29.6: a mapped 301 keeps its `Location` and serves the
                // mapped page under the 301 status).
                let error_page_saved = if candidate.route.error_pages.is_empty()
                    && candidate.virtual_host.error_pages.is_empty()
                {
                    None
                } else {
                    Some((
                        request.method().clone(),
                        request.headers().clone(),
                        request_query.clone(),
                    ))
                };
                let response = apply_error_pages(
                    &state,
                    &generation,
                    &candidate,
                    &authority,
                    response,
                    error_page_saved.as_ref(),
                    &path,
                    request_query.as_deref(),
                    scheme.as_str(),
                    &map_results,
                )
                .await;
                let variables = ResponseHeaderVariables {
                    scheme: scheme.as_str(),
                    authority: &authority,
                    client_ip,
                    server_port: listener_port,
                    method: &method,
                    request_uri: &raw_request_uri,
                    server_name: candidate
                        .virtual_host
                        .server_names
                        .first()
                        .map_or("", String::as_str),
                    maps: Some(&map_results),
                    invalid_referer: referer_invalid_for(
                        candidate.virtual_host,
                        Some(candidate.route),
                        &request_headers,
                    ),
                };
                let response = apply_security_headers(
                    response,
                    candidate.virtual_host.security_headers.as_ref(),
                    scheme.as_str(),
                );
                return finalize_response_headers(
                    response,
                    &candidate,
                    &variables,
                    &path,
                    request_query.as_deref(),
                );
            }
            Err(_) => {
                if let Some(response) = classify_request(&state, admitted, false, request.version())
                {
                    return response;
                }
                return text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "rewrite processing failed\n",
                );
            }
        }
    };
    // Request facts an `add_header` value can reference. The matched host (and
    // therefore `$server_name`) is fixed by host selection above, so this one
    // binding serves the gate rejections, the named-location gates, and the
    // final response choke point.
    let response_variables = ResponseHeaderVariables {
        scheme: scheme.as_str(),
        authority: &authority,
        client_ip,
        server_port: listener_port,
        method: &method,
        request_uri: &raw_request_uri,
        server_name: selected
            .virtual_host
            .server_names
            .first()
            .map_or("", String::as_str),
        maps: Some(&map_results),
        invalid_referer: referer_invalid_for(
            selected.virtual_host,
            Some(selected.route),
            &request_headers,
        ),
    };
    // nginx location-scoped gates run in the access/preaccess phases, before
    // the content handler and before `try_files` can redirect internally.
    let mut location_leases: Vec<super::limit_conn::ConnectionLease> = Vec::new();
    let gate_outcome = match apply_location_gates(
        &state,
        admitted,
        &generation,
        &selected,
        client_ip,
        request.version(),
        request.method(),
        request.headers(),
        &authority,
        &path,
        request.uri().query(),
        scheme.as_str(),
        &map_results,
    )
    .await
    {
        Ok(outcome) => outcome,
        // An access/auth/limit rejection is a final nginx response: its
        // location's `add_header` policy applies with the same status gating
        // (`always` fields are emitted on 401/403/503).
        Err(response) => {
            return finalize_response_headers(
                response,
                &selected,
                &response_variables,
                &path,
                request_query.as_deref(),
            )
        }
    };
    path = gate_outcome.path;
    let mut error_page_saved = gate_outcome.error_page_saved;
    if let Some(lease) = gate_outcome.limit_conn_lease {
        location_leases.push(lease);
    }
    // A static location with `try_files … @name;` redirects once on a probe
    // miss. Selecting the named target before the request body is consumed
    // keeps the request intact for the target's own serving behavior
    // (including `proxy_pass`). nginx restarts the request phases after an
    // internal redirect, so the same gates are re-applied to the target
    // location: a named location can never bypass access control, auth_basic,
    // limit_req, limit_conn, or secure_link.
    if let Some(named) = named_fallback_selection(
        &generation,
        selected,
        &state.listener_id,
        &authority,
        &path,
        &method,
    )
    .await
    {
        // nginx executes a named location's rewrite-phase `return` before the
        // access phase, so a `return` target short-circuits the gates. A
        // static or proxy target runs the normal access/preaccess phases, so
        // its filters are re-applied exactly like nginx after the internal
        // redirect.
        let return_target = matches!(
            *named.resource,
            ResourceConfig::Respond { .. } | ResourceConfig::Redirect { .. }
        );
        if !return_target {
            let named_outcome = match apply_location_gates(
                &state,
                admitted,
                &generation,
                &named,
                client_ip,
                request.version(),
                request.method(),
                request.headers(),
                &authority,
                &path,
                request.uri().query(),
                scheme.as_str(),
                &map_results,
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(response) => {
                    return finalize_response_headers(
                        response,
                        &named,
                        &response_variables,
                        &path,
                        request_query.as_deref(),
                    )
                }
            };
            path = named_outcome.path;
            error_page_saved = named_outcome.error_page_saved;
            if let Some(lease) = named_outcome.limit_conn_lease {
                location_leases.push(lease);
            }
        }
        // Named locations are only reachable through this internal redirect, so
        // the selected named route needs no further `internal` check; a
        // following path re-selection cannot happen (redirect depth is one).
        selected = named;
    }
    let operations_reserved = is_operations_candidate(&request)
        && selected.route.route_match.path_type == RoutePathType::Exact
        && is_operations_path(&selected.route.route_match.path)
        && matches!(selected.resource, ResourceConfig::Respond { .. });
    if let Some(response) =
        classify_request(&state, admitted, operations_reserved, request.version())
    {
        return response;
    }

    // App-domain fallback parity for the app-config path (see the 404 handling
    // after the resource dispatch): a route selected locally can still end in
    // 404 because its static root or provider object is missing, and that is
    // the same "no server configuration can serve this" situation the
    // website-runtime path above already falls back on. The request body is
    // consumed by the dispatch below, so snapshot the pieces the fallback
    // needs first. Nothing is cloned when no fallback resolver is installed.
    let deploy_fallback_input = state.deploy_fallback.as_ref().map(|_| {
        (
            request.method().clone(),
            request.headers().clone(),
            request.uri().query().map(str::to_owned),
        )
    });
    let request_failure = RequestBodyFailure::default();
    let (parts, body) = request.into_parts();
    let limits = &generation.app.config().limits;
    let body = RequestBodyTimeout::new_observed(
        body,
        Duration::from_millis(limits.request_body_start_timeout_ms),
        Duration::from_millis(limits.request_body_idle_timeout_ms),
        request_failure.clone(),
        state.runtime.metrics.clone(),
    );
    let request = Request::from_parts(parts, Body::new(body));

    let virtual_host_id = selected.virtual_host.id.clone();
    let route_id = selected.route.id.clone();
    let response = match selected.resource {
        ResourceConfig::Respond {
            status,
            content_type,
            content_type_from_uri,
            body,
            ..
        } => {
            // A `return <status> "<body>"` body is a script in nginx; the
            // runtime expands the response-header variable vocabulary plus the
            // declared map results.
            let expanded_body = response_variables.expand(body, &path, request_query.as_deref());
            // nginx derives a `return` body's content type from the request URI
            // extension through the resolved `types` table; the typed plane
            // keeps declaring its content type explicitly.
            let derived_type = content_type_from_uri.then(|| {
                super::delivery_policy::MimePolicy::resolve(
                    generation.app.config(),
                    selected.virtual_host,
                    selected.route,
                )
                .content_type_for_uri(&path)
            });
            let content_type_value = derived_type
                .clone()
                .or_else(|| HeaderValue::from_str(content_type).ok());
            let response = match drain_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok(_) => fixed_response_with_type(
                    *status,
                    content_type_value.as_ref(),
                    &expanded_body,
                    method == "HEAD",
                ),
                Err(response) => response,
            };
            // nginx maps `error_page` for a status produced by a rewrite-phase
            // `return`, not only for statuses generated by a resource handler:
            // the internal redirect re-selects the target location and serves
            // it under the original status (differential-confirmed against
            // nginx 1.29.6 for `return 404`).
            apply_error_pages(
                &state,
                &generation,
                &selected,
                &authority,
                response,
                error_page_saved.as_ref(),
                &path,
                request_query.as_deref(),
                scheme.as_str(),
                &map_results,
            )
            .await
        }
        ResourceConfig::Redirect {
            status, location, ..
        } => {
            let query = request.uri().query().map(str::to_owned);
            let response = match drain_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok(_) => redirect_response(
                    *status,
                    location,
                    scheme.as_str(),
                    &authority,
                    &path,
                    query.as_deref(),
                    &map_results,
                ),
                Err(response) => response,
            };
            // nginx maps `error_page` for a `return <3xx> <url>` final status as
            // well (probed against nginx 1.29.6: the mapped page keeps the
            // redirect's `Location`); without a mapping this is a no-op.
            apply_error_pages(
                &state,
                &generation,
                &selected,
                &authority,
                response,
                error_page_saved.as_ref(),
                &path,
                query.as_deref(),
                scheme.as_str(),
                &map_results,
            )
            .await
        }
        ResourceConfig::StubStatus { .. } => {
            if !client_ip.is_loopback() {
                return text_response(
                    StatusCode::FORBIDDEN,
                    "stub_status is available from loopback only\n",
                );
            }
            match drain_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok(_) if matches!(method.as_str(), "GET" | "HEAD") => {
                    let snapshot = state.runtime.metrics.stub_status_snapshot();
                    let body = format!(
                        "Active connections: {} \nserver accepts handled requests\n {} {} {} \nReading: {} Writing: {} Waiting: {} \n",
                        snapshot.active,
                        snapshot.accepts,
                        snapshot.handled,
                        snapshot.requests,
                        snapshot.reading,
                        snapshot.writing,
                        snapshot.waiting,
                    );
                    fixed_response_with_type(
                        200,
                        Some(&HeaderValue::from_static("text/plain")),
                        &body,
                        method == "HEAD",
                    )
                }
                Ok(_) => text_response(StatusCode::METHOD_NOT_ALLOWED, "method is not allowed\n"),
                Err(response) => response,
            }
        }
        ResourceConfig::EmptyGif { .. } => {
            // nginx `empty_gif` handler: GET/HEAD serve the canonical 43-byte
            // 1x1 transparent GIF; any other method is 405.
            match drain_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok(_) if matches!(method.as_str(), "GET" | "HEAD") => {
                    empty_gif_response(method == "HEAD")
                }
                Ok(_) => text_response(StatusCode::METHOD_NOT_ALLOWED, "method is not allowed\n"),
                Err(response) => response,
            }
        }
        ResourceConfig::Static {
            id,
            strip_prefix,
            spa_fallback,
            dynamic_root,
            ..
        } => {
            // `@name` is a named-location internal redirect handled before
            // dispatch; it must never be resolved as a document-root path.
            let static_spa_fallback = spa_fallback
                .as_deref()
                .filter(|fallback| !fallback.starts_with('@'));
            let prefer_h5 = prefer_h5_surface(
                request
                    .headers()
                    .get(USER_AGENT)
                    .and_then(|value| value.to_str().ok()),
                request
                    .headers()
                    .get("sec-ch-ua-mobile")
                    .and_then(|value| value.to_str().ok()),
            );
            // nginx `root $variable` / `alias $variable`: resolve the template
            // with this request's rewrite-phase variables and confine the
            // evaluated path to the operator `documentRootAllowlist`.
            let dynamic_root_override: Option<std::path::PathBuf> = match dynamic_root.as_deref() {
                Some(template) => match request_variables.resolve_document_root(
                    template,
                    &generation.app.config().nginx.document_root_allowlist,
                ) {
                    Ok(root) => Some(std::path::PathBuf::from(root)),
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            template = template,
                            "request-time document root rejected"
                        );
                        return text_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "document root is not available\n",
                        );
                    }
                },
                None => None,
            };
            let default_root = if prefer_h5 {
                generation
                    .app
                    .static_h5_root(id)
                    .or_else(|| generation.app.static_root(id))
            } else {
                generation.app.static_root(id)
            };
            let root: &std::path::Path = match dynamic_root_override.as_deref() {
                Some(root) => root,
                None => match default_root {
                    Some(root) => root,
                    None => {
                        return text_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "static resource is unavailable\n",
                        )
                    }
                },
            };
            let static_response = match drain_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok(request) => {
                    serve_static(
                        root,
                        selected.route,
                        *strip_prefix,
                        static_spa_fallback,
                        &path,
                        request,
                        static_delivery_policy(&generation, &selected),
                    )
                    .await
                }
                Err(response) => response,
            };
            apply_error_pages(
                &state,
                &generation,
                &selected,
                &authority,
                static_response,
                error_page_saved.as_ref(),
                &path,
                None,
                scheme.as_str(),
                &map_results,
            )
            .await
        }
        ResourceConfig::Proxy {
            upstream_ref,
            strip_prefix,
            target_uri,
            request_set_headers,
            dynamic_target,
            proxy_pass_request_headers,
            proxy_intercept_errors,
            proxy_response_policy,
            proxy_method,
            ..
        } => {
            let proxy_response = super::proxy::proxy_request_cached(
                super::proxy::ProxyRequestContext {
                    generation: &generation,
                    upstream_ref,
                    strip_prefix: *strip_prefix,
                    target_uri: target_uri.as_deref(),
                    request_set_headers,
                    dynamic_target: dynamic_target.as_deref(),
                    proxy_pass_request_headers: *proxy_pass_request_headers,
                    route: selected.route,
                    client_ip,
                    external_scheme: scheme.as_str(),
                    external_authority: &authority,
                    listener_port: state
                        .runtime
                        .current()
                        .app
                        .listeners()
                        .find(|listener| listener.id == state.listener_id)
                        .map(|listener| listener.port)
                        .unwrap_or(0),
                    normalized_path: &path,
                    path_rewritten,
                    variables: Some(&request_variables),
                    request_failure,
                    tunnel_supervisor: &state.runtime.tunnel_supervisor,
                    metrics: &state.runtime.metrics,
                    cache: generation.proxy_cache.clone(),
                    map_results: &map_results,
                    response_policy: proxy_response_policy.as_ref(),
                    proxy_method: proxy_method.as_deref(),
                },
                request,
            )
            .await;
            // nginx `proxy_intercept_errors on`: a proxied response whose
            // status has an error_page mapping is replaced by the page.
            let intercepted = if *proxy_intercept_errors {
                if let Some((method, headers, query)) = error_page_saved.as_ref() {
                    resolve_error_pages(
                        &state,
                        &generation,
                        &selected,
                        &authority,
                        proxy_response.status(),
                        method,
                        headers,
                        &path,
                        query.as_deref(),
                        scheme.as_str(),
                        &map_results,
                    )
                    .await
                } else {
                    None
                }
            } else {
                None
            };
            match intercepted {
                Some(paged) => paged,
                None => proxy_response,
            }
        }
        ResourceConfig::FastCgi {
            upstream_ref,
            params,
            index,
            split_path_info,
            document_root,
            pass_request_headers,
            keep_conn,
            bind_address,
            socket_keepalive,
            buffering: _,
            request_buffering: _,
            connect_timeout_ms,
            send_timeout_ms,
            read_timeout_ms,
            response_policy,
            intercept_errors,
            retry,
            cache,
            buffer_policy,
            pass_request_body,
            store,
            limit_rate_bytes_per_second,
            force_ranges,
            ..
        } => {
            let fastcgi_policy = super::cgi_upstream::CgiResponsePolicy::new(
                buffer_policy.as_ref(),
                *pass_request_body,
                store.as_ref(),
                generation.app.config().nginx.store_root.as_deref(),
                *limit_rate_bytes_per_second,
                *force_ranges,
                None,
            );
            let fastcgi_arm = match collect_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok((request, request_body)) => {
                    let response_variables = super::cgi_upstream::CgiRequestFacts {
                        method: request.method(),
                        scheme: scheme.as_str(),
                        authority: &authority,
                        path: &path,
                        raw_request_uri: &raw_request_uri,
                        query: request_query.as_deref(),
                        client_ip,
                        listener_port: state
                            .runtime
                            .current()
                            .app
                            .listeners()
                            .find(|listener| listener.id == state.listener_id)
                            .map(|listener| listener.port)
                            .unwrap_or(0),
                        server_protocol: match request.version() {
                            axum::http::Version::HTTP_11 => "HTTP/1.1",
                            axum::http::Version::HTTP_10 => "HTTP/1.0",
                            axum::http::Version::HTTP_2 => "HTTP/2",
                            axum::http::Version::HTTP_3 => "HTTP/3",
                            _ => "HTTP/1.1",
                        },
                        server_name: selected
                            .virtual_host
                            .server_names
                            .first()
                            .map(String::as_str)
                            .unwrap_or(""),
                        headers: request.headers(),
                        document_root: document_root.as_deref(),
                        script_name: path.clone(),
                        path_info: String::new(),
                        maps: &map_results,
                    };
                    let cache_lookup = super::cgi_cache::begin(
                        &generation,
                        cache.as_ref(),
                        &response_variables,
                        request.method(),
                        request.headers(),
                    )
                    .await;
                    let fastcgi_response = match cache_lookup {
                        super::cgi_cache::CgiCacheLookup::Hit(response) => response,
                        super::cgi_cache::CgiCacheLookup::Disabled => {
                            super::cgi_upstream::serve_fastcgi(
                                &generation,
                                upstream_ref,
                                params,
                                split_path_info.as_deref(),
                                index.as_deref(),
                                *pass_request_headers,
                                *keep_conn,
                                *bind_address,
                                socket_keepalive.unwrap_or(false),
                                *connect_timeout_ms,
                                *send_timeout_ms,
                                *read_timeout_ms,
                                response_policy.as_ref(),
                                retry.as_ref(),
                                &fastcgi_policy,
                                response_variables,
                                &request_body,
                                &state.runtime.metrics,
                            )
                            .await
                        }
                        super::cgi_cache::CgiCacheLookup::Miss(plan) => {
                            let upstream_method = plan.upstream_method();
                            let upstream_headers = plan.upstream_headers();
                            let effective_facts = response_variables.with_overrides(
                                &upstream_method,
                                upstream_headers.unwrap_or(request.headers()),
                            );
                            let response = super::cgi_upstream::serve_fastcgi(
                                &generation,
                                upstream_ref,
                                params,
                                split_path_info.as_deref(),
                                index.as_deref(),
                                *pass_request_headers,
                                *keep_conn,
                                *bind_address,
                                socket_keepalive.unwrap_or(false),
                                *connect_timeout_ms,
                                *send_timeout_ms,
                                *read_timeout_ms,
                                response_policy.as_ref(),
                                retry.as_ref(),
                                &fastcgi_policy,
                                effective_facts,
                                &request_body,
                                &state.runtime.metrics,
                            )
                            .await;
                            plan.finish(response).await
                        }
                    };
                    // nginx `fastcgi_intercept_errors on`: an upstream error status is
                    // replaced by the matched `error_page` target.
                    let intercepted = if *intercept_errors {
                        if let Some((method, headers, query)) = error_page_saved.as_ref() {
                            resolve_error_pages(
                                &state,
                                &generation,
                                &selected,
                                &authority,
                                fastcgi_response.status(),
                                method,
                                headers,
                                &path,
                                query.as_deref(),
                                scheme.as_str(),
                                &map_results,
                            )
                            .await
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    match intercepted {
                        Some(paged) => paged,
                        None => fastcgi_response,
                    }
                }
                Err(response) => response,
            };
            fastcgi_arm
        }
        ResourceConfig::CgiUpstream {
            protocol,
            upstream_ref,
            params,
            document_root,
            pass_request_headers,
            bind_address,
            socket_keepalive,
            buffering: _,
            request_buffering: _,
            connect_timeout_ms,
            send_timeout_ms,
            read_timeout_ms,
            response_policy,
            intercept_errors,
            retry,
            modifier1,
            modifier2,
            extra_string,
            cache,
            buffer_policy,
            pass_request_body,
            store,
            limit_rate_bytes_per_second,
            force_ranges,
            ..
        } => {
            let cgi_policy = super::cgi_upstream::CgiResponsePolicy::new(
                buffer_policy.as_ref(),
                *pass_request_body,
                store.as_ref(),
                generation.app.config().nginx.store_root.as_deref(),
                *limit_rate_bytes_per_second,
                *force_ranges,
                None,
            );
            let cgi_arm = match collect_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok((request, request_body)) => {
                    let response_variables = super::cgi_upstream::CgiRequestFacts {
                        method: request.method(),
                        scheme: scheme.as_str(),
                        authority: &authority,
                        path: &path,
                        raw_request_uri: &raw_request_uri,
                        query: request_query.as_deref(),
                        client_ip,
                        listener_port: state
                            .runtime
                            .current()
                            .app
                            .listeners()
                            .find(|listener| listener.id == state.listener_id)
                            .map(|listener| listener.port)
                            .unwrap_or(0),
                        server_protocol: match request.version() {
                            axum::http::Version::HTTP_11 => "HTTP/1.1",
                            axum::http::Version::HTTP_10 => "HTTP/1.0",
                            axum::http::Version::HTTP_2 => "HTTP/2",
                            axum::http::Version::HTTP_3 => "HTTP/3",
                            _ => "HTTP/1.1",
                        },
                        server_name: selected
                            .virtual_host
                            .server_names
                            .first()
                            .map(String::as_str)
                            .unwrap_or(""),
                        headers: request.headers(),
                        document_root: document_root.as_deref(),
                        script_name: path.clone(),
                        path_info: String::new(),
                        maps: &map_results,
                    };
                    let cache_lookup = super::cgi_cache::begin(
                        &generation,
                        cache.as_ref(),
                        &response_variables,
                        request.method(),
                        request.headers(),
                    )
                    .await;
                    let cgi_response = match cache_lookup {
                        super::cgi_cache::CgiCacheLookup::Hit(response) => response,
                        super::cgi_cache::CgiCacheLookup::Disabled => {
                            super::cgi_upstream::serve_cgi(
                                &generation,
                                *protocol,
                                upstream_ref,
                                params,
                                *pass_request_headers,
                                *bind_address,
                                socket_keepalive.unwrap_or(false),
                                *connect_timeout_ms,
                                *send_timeout_ms,
                                *read_timeout_ms,
                                response_policy.as_ref(),
                                retry.as_ref(),
                                &cgi_policy,
                                *modifier1,
                                *modifier2,
                                extra_string.as_deref(),
                                response_variables,
                                &request_body,
                                &state.runtime.metrics,
                            )
                            .await
                        }
                        super::cgi_cache::CgiCacheLookup::Miss(plan) => {
                            let upstream_method = plan.upstream_method();
                            let upstream_headers = plan.upstream_headers();
                            let effective_facts = response_variables.with_overrides(
                                &upstream_method,
                                upstream_headers.unwrap_or(request.headers()),
                            );
                            let response = super::cgi_upstream::serve_cgi(
                                &generation,
                                *protocol,
                                upstream_ref,
                                params,
                                *pass_request_headers,
                                *bind_address,
                                socket_keepalive.unwrap_or(false),
                                *connect_timeout_ms,
                                *send_timeout_ms,
                                *read_timeout_ms,
                                response_policy.as_ref(),
                                retry.as_ref(),
                                &cgi_policy,
                                *modifier1,
                                *modifier2,
                                extra_string.as_deref(),
                                effective_facts,
                                &request_body,
                                &state.runtime.metrics,
                            )
                            .await;
                            plan.finish(response).await
                        }
                    };
                    // nginx `uwsgi_intercept_errors` / `scgi_intercept_errors`.
                    let intercepted = if *intercept_errors {
                        if let Some((method, headers, query)) = error_page_saved.as_ref() {
                            resolve_error_pages(
                                &state,
                                &generation,
                                &selected,
                                &authority,
                                cgi_response.status(),
                                method,
                                headers,
                                &path,
                                query.as_deref(),
                                scheme.as_str(),
                                &map_results,
                            )
                            .await
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    match intercepted {
                        Some(paged) => paged,
                        None => cgi_response,
                    }
                }
                Err(response) => response,
            };
            cgi_arm
        }
        ResourceConfig::Grpc {
            upstream_ref,
            request_set_headers,
            connect_timeout_ms,
            send_timeout_ms,
            read_timeout_ms,
            buffer_size,
            response_policy,
            intercept_errors,
            retry,
            ..
        } => {
            let grpc_arm = match collect_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok((request, request_body)) => {
                    let grpc_response = super::grpc_upstream::serve_grpc(
                        &generation,
                        upstream_ref,
                        request_set_headers,
                        *connect_timeout_ms,
                        *send_timeout_ms,
                        *read_timeout_ms,
                        *buffer_size,
                        response_policy.as_ref(),
                        retry.as_ref(),
                        request.method(),
                        &path,
                        request_query.as_deref(),
                        &authority,
                        super::grpc_upstream::GrpcRequestFacts {
                            client_ip,
                            external_scheme: scheme.as_str(),
                            listener_port: state
                                .runtime
                                .current()
                                .app
                                .listeners()
                                .find(|listener| listener.id == state.listener_id)
                                .map(|listener| listener.port)
                                .unwrap_or(0),
                            maps: &map_results,
                        },
                        request.headers(),
                        &request_body,
                        &state.runtime.metrics,
                    )
                    .await;
                    // nginx `grpc_intercept_errors on`: an upstream HTTP error
                    // status is replaced by the matched `error_page` target. A
                    // non-zero gRPC status is not an HTTP error and is forwarded.
                    let intercepted = if *intercept_errors {
                        if let Some((method, headers, query)) = error_page_saved.as_ref() {
                            resolve_error_pages(
                                &state,
                                &generation,
                                &selected,
                                &authority,
                                grpc_response.status(),
                                method,
                                headers,
                                &path,
                                query.as_deref(),
                                scheme.as_str(),
                                &map_results,
                            )
                            .await
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    match intercepted {
                        Some(paged) => paged,
                        None => grpc_response,
                    }
                }
                Err(response) => response,
            };
            grpc_arm
        }
        ResourceConfig::Memcached {
            upstream_ref,
            key_template,
            bind_address,
            socket_keepalive,
            buffering: _,
            connect_timeout_ms,
            send_timeout_ms,
            read_timeout_ms,
            retry,
            buffer_bytes,
            gzip_flag,
            ..
        } => {
            // The upstream exchange is GET/HEAD only; the bounded adapter
            // answers other methods with 405 before touching the network.
            let mut memcached_policy = super::cgi_upstream::CgiResponsePolicy::new(
                None, true, None, None, None, false, *gzip_flag,
            );
            if let Some(bytes) = buffer_bytes {
                if let Ok(bytes) = usize::try_from(*bytes) {
                    memcached_policy.limits.max_header_block_bytes = bytes;
                }
            }
            let memcached_arm = match collect_bounded_request_body(
                request,
                generation.app.config().limits.max_request_body_bytes,
                &request_failure,
            )
            .await
            {
                Ok((request, _request_body)) => {
                    let response_variables = super::cgi_upstream::CgiRequestFacts {
                        method: request.method(),
                        scheme: scheme.as_str(),
                        authority: &authority,
                        path: &path,
                        raw_request_uri: &raw_request_uri,
                        query: request_query.as_deref(),
                        client_ip,
                        listener_port: state
                            .runtime
                            .current()
                            .app
                            .listeners()
                            .find(|listener| listener.id == state.listener_id)
                            .map(|listener| listener.port)
                            .unwrap_or(0),
                        server_protocol: match request.version() {
                            axum::http::Version::HTTP_11 => "HTTP/1.1",
                            axum::http::Version::HTTP_10 => "HTTP/1.0",
                            axum::http::Version::HTTP_2 => "HTTP/2",
                            axum::http::Version::HTTP_3 => "HTTP/3",
                            _ => "HTTP/1.1",
                        },
                        server_name: selected
                            .virtual_host
                            .server_names
                            .first()
                            .map(String::as_str)
                            .unwrap_or(""),
                        headers: request.headers(),
                        document_root: None,
                        script_name: path.clone(),
                        path_info: String::new(),
                        maps: &map_results,
                    };
                    let method = response_variables.method.clone();
                    super::cgi_upstream::serve_memcached(
                        &generation,
                        upstream_ref,
                        key_template,
                        *bind_address,
                        socket_keepalive.unwrap_or(false),
                        *connect_timeout_ms,
                        *send_timeout_ms,
                        *read_timeout_ms,
                        retry.as_ref(),
                        &memcached_policy,
                        response_variables,
                        &method,
                        &state.runtime.metrics,
                    )
                    .await
                }
                Err(response) => response,
            };
            memcached_arm
        }
        ResourceConfig::Drive {
            id,
            resource_subpath,
            index_files,
            spa_fallback,
            cache,
            ..
        } => {
            let provider_path = super::provider_resource::translate_provider_path(
                selected.route,
                &path,
                resource_subpath.as_deref(),
            );
            let route = AppConfigResourceRoute {
                virtual_host_id: virtual_host_id.clone(),
                route_id: route_id.clone(),
                resource_id: id.clone(),
                provider: ProviderResourceReference {
                    provider_type: WebsiteProviderType::Drive,
                    provider_resource_uuid: selected
                        .resource
                        .provider_resource_uuid()
                        .expect("drive resource uuid")
                        .to_owned(),
                    provider_contract_version: DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION
                        .to_owned(),
                },
                handler: AppConfigResourceHandler::Static,
                provider_relative_path: provider_path,
                index_files: index_files.clone(),
                spa_fallback: spa_fallback.clone(),
                directory_request: path.ends_with('/'),
                locale: None,
                cache: cache.unwrap_or_default(),
            };
            serve_provider_backed_resource(
                &state,
                &generation,
                &method,
                request,
                request_failure,
                route,
            )
            .await
        }
        ResourceConfig::Knowledgebase {
            id, locale, cache, ..
        } => {
            let route = AppConfigResourceRoute {
                virtual_host_id: virtual_host_id.clone(),
                route_id: route_id.clone(),
                resource_id: id.clone(),
                provider: ProviderResourceReference {
                    provider_type: WebsiteProviderType::Knowledgebase,
                    provider_resource_uuid: selected
                        .resource
                        .provider_resource_uuid()
                        .expect("knowledgebase resource uuid")
                        .to_owned(),
                    provider_contract_version: KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION
                        .to_owned(),
                },
                handler: AppConfigResourceHandler::Wiki,
                provider_relative_path: path.clone(),
                index_files: Vec::new(),
                spa_fallback: None,
                directory_request: false,
                locale: locale.clone(),
                cache: cache.unwrap_or_default(),
            };
            serve_provider_backed_resource(
                &state,
                &generation,
                &method,
                request,
                request_failure,
                route,
            )
            .await
        }
    };

    // A *resource-backed* route that resolved nothing locally (missing static
    // root, absent Drive object, unpublished wiki page) is the same situation
    // the website-runtime path falls back on, so consult Deploy before
    // answering 404. Deliberate `Respond` routes and upstream `Proxy` replies
    // are never second-guessed: their status is authoritative, and a 404 from
    // an upstream is a real answer rather than a local configuration miss.
    if response.status() == StatusCode::NOT_FOUND
        && matches!(
            selected.resource,
            ResourceConfig::Static { .. }
                | ResourceConfig::Drive { .. }
                | ResourceConfig::Knowledgebase { .. }
        )
    {
        if let Some((fallback_method, fallback_headers, fallback_query)) =
            deploy_fallback_input.as_ref()
        {
            if let Some(fallback_response) = serve_deploy_fallback(
                &state,
                scheme.website_delivery_scheme(),
                authority.clone(),
                path.clone(),
                fallback_query.clone(),
                fallback_method,
                fallback_headers,
                transport_peer,
            )
            .await
            {
                return fallback_response;
            }
        }
    }
    if generation.app.config().observability.access_log {
        tracing::info!(
            config_generation = generation.id,
            config_revision = %generation.revision,
            listener_id = %state.listener_id,
            scheme = scheme.as_str(),
            virtual_host_id = %virtual_host_id,
            route_id = %route_id,
            method = %method,
            status = response.status().as_u16(),
            "request served"
        );
    }
    let mut response = apply_security_headers(
        response,
        selected.virtual_host.security_headers.as_ref(),
        scheme.as_str(),
    );
    // nginx `expires` rewrites `Expires`/`Cache-Control` on the final response
    // whatever produced it (`ngx_http_headers_filter` runs after every content
    // handler), so it is applied here rather than inside the static layer.
    // A route-level declaration replaces the host policy.
    if let Some(policy) = selected
        .route
        .cache_policy
        .as_ref()
        .or(selected.virtual_host.cache_policy.as_ref())
    {
        super::cache_policy::apply_response_cache_policy(
            &mut response,
            policy,
            std::time::SystemTime::now(),
        );
    }
    // nginx `add_header` (`ngx_http_headers_filter`) on the final response of
    // every content kind — static, proxy, cache, `return`/redirect, error page,
    // and the generated errors that reached this choke point. The policy is the
    // selected location's array when it declares one, otherwise the inherited
    // host array; an `error_page` internal redirect may have attached the
    // target location's policy instead.
    // nginx `add_header` executed inside a matched `if` block replaces the
    // inherited array for the rest of the request (the `if` block is its own
    // configuration context); values were expanded during the rewrite phase.
    if !phase_headers.is_empty() {
        response
            .extensions_mut()
            .insert(ResponseHeaderPolicyOverride(Some(phase_headers.clone())));
    }
    let mut response = finalize_response_headers(
        response,
        &selected,
        &response_variables,
        &path,
        request_query.as_deref(),
    );
    response.extensions_mut().insert(gzip_facts.clone());
    // Route-level `sub_filter` rides on the response so the substitution
    // layer can apply it without re-selecting the route.
    if let Some(sub_filter) = selected.route.sub_filter.clone() {
        response
            .extensions_mut()
            .insert(super::sub_filter::SubFilterExtension(sub_filter));
    }
    // nginx publishes a weak entity tag for a representation it gzips. The
    // compression layer runs after the handler, so the validator is weakened
    // here, using the same policy and negotiation input the predicate uses.
    let negotiated_compression = selected
        .route
        .compression
        .as_ref()
        .or(selected.virtual_host.compression.as_ref());
    let mut response = super::gzip_predicate::weaken_etag_for_negotiated_gzip(
        response,
        &generation.app.config().gzip,
        negotiated_compression,
        accept_encoding.as_ref(),
        Some(&gzip_facts),
    );
    // Route the per-vhost compression policy (server-level nginx `gzip`) to
    // the compression layer; `None` inherits the app-level policy.
    response
        .extensions_mut()
        .insert(super::gzip_predicate::CompressionOverride(
            selected
                .route
                .compression
                .clone()
                .or_else(|| selected.virtual_host.compression.clone()),
        ));
    // nginx `gzip_vary off`: the compression layer always declares
    // `Vary: Accept-Encoding` when it selects a representation, so the handler
    // states the exception for the outer suppression middleware.
    if !super::delivery_policy::effective_gzip(
        generation.app.config(),
        selected.virtual_host,
        selected.route,
    )
    .vary
    {
        response
            .extensions_mut()
            .insert(super::gzip_predicate::SuppressGzipVary);
    }
    // Keep every limit_conn slot held for the whole response lifetime: the
    // leases drop when the response body completes or is abandoned. A bounded
    // named-location redirect can hold one lease for the path location and one
    // for the named location, mirroring nginx re-running the limit_conn
    // handler after the internal redirect.
    let mut response = response;
    for lease in location_leases.into_iter().rev() {
        response =
            response.map(|body| Body::new(super::limit_conn::LeaseBody { inner: body, lease }));
    }
    response
}

/// Effective static delivery policy for one selected route (nginx `types` /
/// `default_type` / `gzip_static` / `gzip_vary`).
fn static_delivery_policy<'a>(
    generation: &'a RuntimeGeneration,
    selected: &'a SelectedRoute<'a>,
) -> super::static_files::StaticDeliveryPolicy<'a> {
    let app = generation.app.config();
    super::static_files::StaticDeliveryPolicy {
        mime: super::delivery_policy::MimePolicy::resolve(
            app,
            selected.virtual_host,
            selected.route,
        ),
        gzip: super::delivery_policy::effective_gzip(app, selected.virtual_host, selected.route),
        max_ranges: selected
            .route
            .max_ranges
            .or(selected.virtual_host.max_ranges)
            .unwrap_or(DEFAULT_MAX_RANGES),
        autoindex: selected
            .route
            .autoindex
            .as_ref()
            .or(selected.virtual_host.autoindex.as_ref()),
        index_files: match selected.resource {
            ResourceConfig::Static { index_files, .. } => index_files,
            _ => &[],
        },
    }
}

/// Result of running nginx's access/preaccess gates for one selected
/// location.
struct LocationGateOutcome {
    /// The request path after `secure_link` URI substitution.
    path: String,
    /// Request identity kept for an `error_page` internal redirect. Captured
    /// only when the route or host declares error-page mappings.
    error_page_saved: Option<(Method, HeaderMap, Option<String>)>,
    /// The `limit_conn` slot(s) taken for this location; held until the
    /// response body is finished.
    limit_conn_lease: Option<super::limit_conn::ConnectionLease>,
}

/// Run the nginx location-scoped gates: access rules, `auth_basic`,
/// `limit_req`, `limit_conn`, and `secure_link`.
///
/// These are the phases nginx runs before the content handler. They are one
/// function because a bounded named-location internal redirect re-runs them
/// for the target location, exactly like nginx restarts the request phases
/// after an internal redirect.
#[allow(clippy::too_many_arguments)]
async fn apply_location_gates(
    state: &ListenerState,
    admitted: &mut super::request_gate::RequestAdmissionPermit,
    generation: &RuntimeGeneration,
    selected: &SelectedRoute<'_>,
    client_ip: IpAddr,
    version: Version,
    method: &Method,
    headers: &HeaderMap,
    authority: &str,
    requested_path: &str,
    query: Option<&str>,
    scheme: &str,
    maps: &super::map_variables::MapResults,
) -> Result<LocationGateOutcome, Response<Body>> {
    let mut path = requested_path.to_owned();
    if evaluate_access(client_ip, &selected.route.access) == AccessDecision::Deny {
        if let Some(response) = classify_request(state, admitted, false, version) {
            return Err(response);
        }
        state
            .runtime
            .metrics
            .record_request_rejection(RequestRejection::AccessDenied);
        if let Some(paged) = resolve_error_pages(
            state,
            generation,
            selected,
            authority,
            StatusCode::FORBIDDEN,
            method,
            headers,
            &path,
            query,
            scheme,
            maps,
        )
        .await
        {
            return Err(paged);
        }
        return Err(text_response(StatusCode::FORBIDDEN, "access denied\n"));
    }
    match evaluate_auth_basic(
        headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        selected.route.auth_basic.as_ref(),
    ) {
        AuthBasicDecision::Inactive | AuthBasicDecision::Allow => {}
        AuthBasicDecision::Challenge => {
            if let Some(response) = classify_request(state, admitted, false, version) {
                return Err(response);
            }
            state
                .runtime
                .metrics
                .record_request_rejection(RequestRejection::AuthRequired);
            let realm = selected
                .route
                .auth_basic
                .as_ref()
                .map(|config| config.realm.as_str())
                .unwrap_or("Restricted");
            return Err(auth_basic_challenge_response(realm));
        }
    }
    if !selected.route.limit_req.is_empty()
        && generation
            .limit_req
            .admit(client_ip, &selected.route.limit_req)
            == LimitReqDecision::Reject
    {
        if let Some(response) = classify_request(state, admitted, false, version) {
            return Err(response);
        }
        state
            .runtime
            .metrics
            .record_request_rejection(RequestRejection::RateLimited);
        let status = selected
            .route
            .limit_req_status
            .or(selected.virtual_host.limit_req_status)
            .unwrap_or(503);
        return Err(limit_req_rejected_response(version, status));
    }
    // nginx error_page applies to generated errors; keep the request identity
    // for the internal redirect before any dispatch consumes the request.
    // Captured only when the route or host declares error pages.
    let error_page_saved =
        if selected.route.error_pages.is_empty() && selected.virtual_host.error_pages.is_empty() {
            None
        } else {
            Some((method.clone(), headers.clone(), query.map(str::to_owned)))
        };
    let limit_conn_lease = match generation
        .limit_conn
        .admit(client_ip, &selected.route.limit_conn)
    {
        Ok(lease) => Some(lease),
        Err(LimitConnDecision::Reject) => {
            if let Some(response) = classify_request(state, admitted, false, version) {
                return Err(response);
            }
            state
                .runtime
                .metrics
                .record_request_rejection(RequestRejection::ConnectionLimited);
            let status = selected
                .route
                .limit_conn_status
                .or(selected.virtual_host.limit_conn_status)
                .unwrap_or(503);
            return Err(limit_conn_rejected_response(version, status));
        }
        Err(LimitConnDecision::Allow) => {
            unreachable!("limit_conn admission only errors with Reject")
        }
    };
    if let Some(secure_link) = selected.route.secure_link.clone() {
        let prefix = selected.route.route_match.path.as_str();
        let now_unix_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        match verify_secure_link(
            &path,
            &client_ip.to_string(),
            query,
            &secure_link,
            prefix,
            now_unix_seconds,
        ) {
            Ok(Some(serving_uri)) => {
                // secure_link_secret: serve the rewritten URI (`/prefix/<rest>`).
                path = serving_uri;
            }
            Ok(None) => {}
            Err(SecureLinkFailure) => {
                if let Some(response) = classify_request(state, admitted, false, version) {
                    return Err(response);
                }
                state
                    .runtime
                    .metrics
                    .record_request_rejection(RequestRejection::LinkInvalid);
                return Err(secure_link_rejected_response(version));
            }
        }
    }
    Ok(LocationGateOutcome {
        path,
        error_page_saved,
        limit_conn_lease,
    })
}

/// Resolve the one-hop `try_files … @name;` internal redirect for a static
/// location whose document probes miss.
///
/// The probe runs before the request is consumed, so the named target executes
/// with the intact request (a `proxy_pass` target still forwards the body).
/// A named location is never redirected again: the redirect depth is one.
async fn named_fallback_selection<'a>(
    generation: &'a RuntimeGeneration,
    selected: SelectedRoute<'a>,
    listener_id: &str,
    authority: &str,
    path: &str,
    method: &str,
) -> Option<SelectedRoute<'a>> {
    // A named location is already the redirect target; one hop per selection.
    if selected.route.route_match.path.starts_with('@') {
        return None;
    }
    let ResourceConfig::Static {
        id,
        strip_prefix,
        spa_fallback,
        ..
    } = selected.resource
    else {
        return None;
    };
    let name = spa_fallback.as_deref()?;
    if !name.starts_with('@') {
        return None;
    }
    let root = generation.app.static_root(id)?;
    let relative = super::static_files::relative_request_path(selected.route, *strip_prefix, path);
    let miss = matches!(
        super::static_path::open_static_path(
            root,
            relative,
            path.ends_with('/'),
            None,
            false,
            None
        )
        .await,
        Err(super::static_path::StaticPathError::NotFound)
    );
    if !miss {
        return None;
    }
    generation
        .app
        .select_named_route(listener_id, authority, name, method)
}

/// One nginx `error_page` hop: resolve the mapping for `status`, run the
/// target (an absolute `/path` or a named `@location`), and return the target
/// response with the target's own status. Returns `None` when no mapping
/// applies or the target cannot be executed here: a named proxy target is
/// rejected at configuration time because the request body has already been
/// consumed by the time error-page processing runs.
#[allow(clippy::too_many_arguments)]
async fn run_error_page_hop<'a>(
    state: &ListenerState,
    generation: &'a RuntimeGeneration,
    pages: &'a [ErrorPageConfig],
    authority: &str,
    status: StatusCode,
    method: &Method,
    headers: &HeaderMap,
    requested_path: &str,
    query: Option<&str>,
    scheme: &str,
    maps: &super::map_variables::MapResults,
) -> Option<(Response<Body>, StatusCode, &'a [ErrorPageConfig])> {
    let entry = pages
        .iter()
        .rev()
        .find(|page| page.codes.contains(&status.as_u16()))?;
    // A named target keeps the current request URI (nginx
    // `ngx_http_named_location`); an absolute path becomes the new URI. A
    // variable target (`error_page 405 =200 $uri;`) is resolved against the
    // request URI before selection.
    let resolved_uri = if entry.uri.contains('$') {
        entry
            .uri
            .replace("${request_uri}", requested_path)
            .replace("${uri}", requested_path)
            .replace("$request_uri", requested_path)
            .replace("$uri", requested_path)
    } else {
        entry.uri.clone()
    };
    let named_target = resolved_uri.starts_with('@');
    let target_path = if named_target {
        requested_path
    } else {
        resolved_uri.as_str()
    };
    let candidate = if named_target {
        generation.app.select_named_route(
            &state.listener_id,
            authority,
            &resolved_uri,
            method.as_str(),
        )?
    } else {
        generation.app.select_route(
            &state.listener_id,
            authority,
            &resolved_uri,
            method.as_str(),
        )?
    };
    let mut response = match candidate.resource {
        ResourceConfig::Static {
            root,
            strip_prefix,
            spa_fallback,
            ..
        } => {
            // Conditionals and ranges are stripped so the error page serves
            // cleanly instead of re-triggering the original 304/416 semantics.
            let mut rebuilt_headers = headers.clone();
            rebuilt_headers.remove(axum::http::header::RANGE);
            rebuilt_headers.remove(axum::http::header::IF_RANGE);
            rebuilt_headers.remove(axum::http::header::IF_NONE_MATCH);
            rebuilt_headers.remove(axum::http::header::IF_MODIFIED_SINCE);
            let rebuilt = {
                let mut builder = Request::builder().method(method.clone()).uri(target_path);
                *builder.headers_mut()? = rebuilt_headers;
                builder.body(Body::empty()).ok()?
            };
            let spa_fallback = spa_fallback
                .as_deref()
                .filter(|fallback| !fallback.starts_with('@'));
            serve_static(
                std::path::Path::new(root),
                candidate.route,
                *strip_prefix,
                spa_fallback,
                target_path,
                rebuilt,
                static_delivery_policy(generation, &candidate),
            )
            .await
        }
        ResourceConfig::Respond {
            status,
            content_type,
            content_type_from_uri,
            body,
            ..
        } => {
            let expanded_body = maps.expand_tokens(body);
            let derived_type = content_type_from_uri.then(|| {
                super::delivery_policy::MimePolicy::resolve(
                    generation.app.config(),
                    candidate.virtual_host,
                    candidate.route,
                )
                .content_type_for_uri(target_path)
            });
            let declared_type = HeaderValue::from_str(content_type).ok();
            fixed_response_with_type(
                *status,
                derived_type.as_ref().or(declared_type.as_ref()),
                &expanded_body,
                method == Method::HEAD,
            )
        }
        ResourceConfig::Redirect {
            status, location, ..
        } => redirect_response(
            *status,
            location,
            scheme,
            authority,
            requested_path,
            query,
            maps,
        ),
        // Proxy/provider targets cannot rebuild the consumed request body, so
        // they are not executable as an error page. Configuration validation
        // rejects a named proxy error_page target; a literal URI that selects
        // one falls back to the original error response.
        _ => return None,
    };
    // The internal redirect re-runs location selection, so the *target*
    // location's `add_header` policy decides the final response headers. Attach
    // it even when the target has none: that "no headers" answer must suppress
    // the original location's array rather than inheriting it.
    response
        .extensions_mut()
        .insert(ResponseHeaderPolicyOverride(
            effective_response_headers(&candidate).map(<[CustomHeaderConfig]>::to_vec),
        ));
    let target_status = response.status();
    // A recursive chain re-enters mapping with the target route's own
    // error_page set when the target declares one (nginx: each internal
    // redirect re-runs location selection).
    let next_pages = if candidate.route.error_pages.is_empty() {
        candidate.virtual_host.error_pages.as_slice()
    } else {
        candidate.route.error_pages.as_slice()
    };
    Some((response, target_status, next_pages))
}

/// nginx `error_page` resolution for a generated error: internally redirect
/// to the mapped page, honoring `=code` / bare-`=` overrides and, when the host
/// enables `recursive_error_pages`, mapping an error produced while serving
/// the page itself (bounded hops). Returns `None` when nothing maps and the
/// original response must stand.
#[allow(clippy::too_many_arguments)]
async fn resolve_error_pages(
    state: &ListenerState,
    generation: &RuntimeGeneration,
    selected: &SelectedRoute<'_>,
    authority: &str,
    status: StatusCode,
    method: &Method,
    headers: &HeaderMap,
    requested_path: &str,
    query: Option<&str>,
    scheme: &str,
    maps: &super::map_variables::MapResults,
) -> Option<Response<Body>> {
    // nginx inheritance: a location that declares error_page replaces the
    // host-level set for that route.
    let pages = if selected.route.error_pages.is_empty() {
        &selected.virtual_host.error_pages
    } else {
        &selected.route.error_pages
    };
    if pages.is_empty() {
        return None;
    }
    let max_hops = if selected.virtual_host.recursive_error_pages {
        3
    } else {
        1
    };
    let mut current_status = status;
    let mut pages: &[ErrorPageConfig] = pages;
    let mut hops = 0usize;
    loop {
        let entry = pages
            .iter()
            .rev()
            .find(|page| page.codes.contains(&current_status.as_u16()))?;
        let (hop_response, target_status, next_pages) = run_error_page_hop(
            state,
            generation,
            pages,
            authority,
            current_status,
            method,
            headers,
            requested_path,
            query,
            scheme,
            maps,
        )
        .await?;
        let served_cleanly = !(target_status.is_client_error() || target_status.is_server_error());
        if served_cleanly {
            // nginx `error_page <code> = <target>;` (bare `=`) uses the
            // target's own status; `=code` overrides it; a plain mapping keeps
            // the original error status.
            let final_status = if entry.from_target {
                target_status
            } else {
                entry
                    .response_code
                    .and_then(|code| StatusCode::from_u16(code).ok())
                    .unwrap_or(current_status)
            };
            let (mut parts, body) = hop_response.into_parts();
            parts.status = final_status;
            if !final_status.is_success() {
                // nginx keeps the original error (or redirect) status when
                // `error_page` names a URI without `=code`; the response is
                // then the page body under that status, and nginx's header
                // filters do not publish the representation validators for it
                // (the static handler's `Last-Modified`/`Accept-Ranges` never
                // reach the client, while the `ETag` does). Differentially
                // confirmed against nginx 1.29.6 for 404 and 301 mappings; a
                // `=200` override keeps them.
                parts.headers.remove(LAST_MODIFIED);
                parts.headers.remove(ACCEPT_RANGES);
            }
            return Some(Response::from_parts(parts, body));
        }
        // The target itself failed: with recursion the failure maps again
        // (with the target route's own mapping set); otherwise the target's
        // own error stands (nginx default).
        hops += 1;
        if hops >= max_hops || !selected.virtual_host.recursive_error_pages {
            return Some(hop_response);
        }
        current_status = target_status;
        pages = next_pages;
    }
}

/// Applies the route/host `error_page` mapping to a generated error or redirect
/// response when the request identity was captured for the redirect.
#[allow(clippy::too_many_arguments)]
async fn apply_error_pages(
    state: &ListenerState,
    generation: &RuntimeGeneration,
    selected: &SelectedRoute<'_>,
    authority: &str,
    response: Response<Body>,
    saved: Option<&(Method, HeaderMap, Option<String>)>,
    requested_path: &str,
    query: Option<&str>,
    scheme: &str,
    maps: &super::map_variables::MapResults,
) -> Response<Body> {
    let status = response.status();
    let Some((method, headers, saved_query)) = saved else {
        return response;
    };
    // nginx maps any final status, including a redirect produced by a
    // rewrite-phase `return` (probed against nginx 1.29.6: `error_page 301`
    // serves the mapped page under the 301 status).
    if !(status.is_redirection() || status.is_client_error() || status.is_server_error()) {
        return response;
    }
    // A redirect's `Location` is not part of the representation the mapping
    // replaces: nginx keeps the handler-computed target while swapping in the
    // mapped page's body.
    let redirect_location = status
        .is_redirection()
        .then(|| response.headers().get(LOCATION).cloned())
        .flatten();
    let Some(mut paged) = resolve_error_pages(
        state,
        generation,
        selected,
        authority,
        status,
        method,
        headers,
        requested_path,
        saved_query.as_deref().or(query),
        scheme,
        maps,
    )
    .await
    else {
        return response;
    };
    if let Some(location) = redirect_location {
        if !paged.headers().contains_key(LOCATION) {
            paged.headers_mut().insert(LOCATION, location);
        }
    }
    paged
}

/// nginx `add_header` policy of the location an `error_page` internal redirect
/// resolved to.
///
/// nginx re-runs location selection for the redirect, so the *target*
/// location's `add_header` array (or the host array it inherits) decides the
/// response headers; the original location's array must not leak through. The
/// wrapper exists even when the target has no policy at all, because that
/// "no headers" answer must suppress the original location's headers instead
/// of falling back to them.
#[derive(Clone)]
struct ResponseHeaderPolicyOverride(Option<Vec<CustomHeaderConfig>>);

/// Request facts an `add_header` value may reference.
///
/// `$uri`/`$document_uri` are passed at application time because they follow
/// an internal rewrite (`secure_link`), while `$request_uri` keeps the original
/// request target. Only the variables validated by
/// [`CustomHeaderConfig::validate_declaration`] are expanded.
struct ResponseHeaderVariables<'a> {
    scheme: &'a str,
    authority: &'a str,
    client_ip: IpAddr,
    server_port: u16,
    method: &'a str,
    request_uri: &'a str,
    server_name: &'a str,
    /// nginx `map` results for this request; a result variable is expanded
    /// exactly like a built-in request variable in an `add_header` value.
    maps: Option<&'a super::map_variables::MapResults>,
    /// nginx `$invalid_referer` for this request (`valid_referers` policy).
    invalid_referer: bool,
}

impl ResponseHeaderVariables<'_> {
    fn expand(&self, template: &str, uri: &str, query: Option<&str>) -> String {
        let mut output = String::with_capacity(template.len() + 16);
        let mut remainder = template;
        while let Some(index) = remainder.find('$') {
            output.push_str(&remainder[..index]);
            let rest = &remainder[index + 1..];
            if let Some(literal) = rest.strip_prefix('$') {
                output.push('$');
                remainder = literal;
                continue;
            }
            let matched = CustomHeaderConfig::SUPPORTED_VARIABLES
                .iter()
                .find(|name| {
                    rest.strip_prefix(**name).is_some_and(|after| {
                        after.chars().next().is_none_or(|character| {
                            !(character.is_ascii_alphanumeric() || character == '_')
                        })
                    })
                })
                .copied();
            let Some(variable) = matched else {
                // A declared nginx `map` result is expanded here, exactly like
                // a built-in request variable.
                let name_end = rest
                    .find(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_')
                    })
                    .unwrap_or(rest.len());
                let name = &rest[..name_end];
                if let Some(value) = self.maps.and_then(|maps| maps.lookup(name)) {
                    output.push_str(value);
                    remainder = &rest[name_end..];
                    continue;
                }
                // Configuration validation rejects unknown variables, so this
                // is unreachable; keep the `$` literal rather than dropping
                // the rest of the value if an input ever bypasses validation.
                output.push('$');
                remainder = rest;
                continue;
            };
            match variable {
                "host" => output.push_str(
                    &normalize_authority_host(self.authority)
                        .unwrap_or_else(|| self.authority.to_ascii_lowercase()),
                ),
                "scheme" => output.push_str(self.scheme),
                "remote_addr" => output.push_str(&self.client_ip.to_string()),
                "server_port" => output.push_str(&self.server_port.to_string()),
                "request_uri" => output.push_str(self.request_uri),
                "uri" | "document_uri" => output.push_str(uri),
                "args" => output.push_str(query.unwrap_or("")),
                "is_args" => {
                    if query.is_some_and(|query| !query.is_empty()) {
                        output.push('?');
                    }
                }
                "request_method" => output.push_str(self.method),
                "server_name" => output.push_str(self.server_name),
                "invalid_referer" => {
                    output.push_str(if self.invalid_referer { "1" } else { "" });
                }
                _ => {}
            }
            remainder = &rest[variable.len()..];
        }
        output.push_str(remainder);
        output
    }
}

/// Evaluate nginx `$invalid_referer` for one request: the location policy
/// replaces the host policy; otherwise the host policy applies.
fn referer_invalid_for(
    virtual_host: &sdkwork_webserver_core::VirtualHostConfig,
    route: Option<&sdkwork_webserver_core::RouteConfig>,
    headers: &axum::http::HeaderMap,
) -> bool {
    let policy = route
        .and_then(|route| route.referer.as_ref())
        .or(virtual_host.referer.as_ref());
    let referer = headers
        .get(axum::http::header::REFERER)
        .and_then(|value| value.to_str().ok());
    sdkwork_webserver_core::invalid_referer(policy, &virtual_host.server_names, referer)
}

/// The effective nginx `add_header` policy for a selected route: a location
/// that declares any `add_header` replaces the inherited host array (nginx
/// inheritance), so the route list wins when present and the host's
/// `securityHeaders.customHeaders` is used only when it is empty.
fn effective_response_headers<'a>(
    selected: &'a SelectedRoute<'_>,
) -> Option<&'a [CustomHeaderConfig]> {
    if !selected.route.response_headers.is_empty() {
        return Some(selected.route.response_headers.as_slice());
    }
    selected
        .virtual_host
        .security_headers
        .as_ref()
        .map(|security_headers| security_headers.custom_headers.as_slice())
        .filter(|headers| !headers.is_empty())
}

/// Apply the nginx `add_header` policy at the response choke point.
///
/// This is `ngx_http_headers_filter` as observed on nginx 1.29.6:
/// - a field without `always` is emitted only for the statuses in
///   [`CustomHeaderConfig::NGINX_DEFAULT_STATUSES`];
/// - `always` fields are emitted for every status, including 4xx/5xx;
/// - entries are emitted in declaration order and duplicate names are
///   preserved as separate fields;
/// - `$variables` are expanded from the request, and an empty value is
///   omitted (nginx parses `add_header X ""` but sends no field).
fn apply_nginx_response_headers(
    mut response: Response<Body>,
    policy: Option<&[CustomHeaderConfig]>,
    variables: &ResponseHeaderVariables<'_>,
    uri: &str,
    query: Option<&str>,
) -> Response<Body> {
    let Some(policy) = policy else {
        return response;
    };
    let status = response.status().as_u16();
    let default_status = CustomHeaderConfig::NGINX_DEFAULT_STATUSES.contains(&status);
    for header in policy {
        if !header.always && !default_status {
            continue;
        }
        if header.value.is_empty() {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(header.name.as_bytes()) else {
            continue;
        };
        let value = variables.expand(&header.value, uri, query);
        // nginx omits a field whose expanded value is empty, whether the
        // emptiness is declared (`add_header X "";`) or produced by a variable
        // or map result that resolves to the empty string.
        if value.is_empty() {
            continue;
        }
        if let Ok(value) = HeaderValue::from_str(&value) {
            // `append`, not `insert`: nginx emits duplicate add_header fields
            // as separate lines and preserves declaration order.
            response.headers_mut().append(name, value);
        }
    }
    response
}

/// Apply nginx response headers to a response produced before the main dispatch
/// choke point — an access/auth/limit rejection, or an `error_page`-generated
/// response — honoring an error-page target's overriding policy.
fn finalize_response_headers(
    mut response: Response<Body>,
    selected: &SelectedRoute<'_>,
    variables: &ResponseHeaderVariables<'_>,
    uri: &str,
    query: Option<&str>,
) -> Response<Body> {
    let policy_override = response
        .extensions_mut()
        .remove::<ResponseHeaderPolicyOverride>()
        .map(|override_value| override_value.0);
    let policy = match &policy_override {
        Some(policy) => policy.as_deref(),
        None => effective_response_headers(selected),
    };
    let response = apply_redirect_policy(response, effective_redirect_policy(selected), variables);
    // nginx `charset_filter`: the charset header filter runs for every content
    // handler's response (static, proxy, `return`, mapped error page); a
    // location-level declaration replaces the host value, which itself is the
    // resolved http/server policy.
    let response = match effective_charset(selected) {
        Some(charset) => super::charset_policy::apply_charset_policy(response, charset),
        None => response,
    };
    apply_nginx_response_headers(response, policy, variables, uri, query)
}

/// Effective nginx charset policy for one selected route: a location-level
/// declaration wins over the host's resolved policy.
fn effective_charset<'a>(
    selected: &SelectedRoute<'a>,
) -> Option<&'a sdkwork_webserver_core::CharsetConfig> {
    selected
        .route
        .charset
        .as_ref()
        .or(selected.virtual_host.charset.as_ref())
}

/// Effective redirect construction policy for one selected route: a
/// location-level declaration wins over the host's resolved
/// `absolute_redirect` / `port_in_redirect` / `server_name_in_redirect`
/// policy; a host that declares none keeps the nginx defaults.
fn effective_redirect_policy(selected: &SelectedRoute<'_>) -> RedirectPolicyConfig {
    selected
        .route
        .redirect_policy
        .or(selected.virtual_host.redirect_policy)
        .unwrap_or(RedirectPolicyConfig::NGINX_DEFAULT)
}

/// Rewrite a relative `Location` into the absolute form nginx's header filter
/// would send (`ngx_http_header_filter`, differential-confirmed against nginx
/// 1.29.6).
///
/// Only a target that starts with `/` is rewritten; an absolute upstream
/// target is passed through untouched. With `absolute_redirect off` the
/// relative target is kept. With `absolute_redirect on` the target becomes
/// `<scheme>://<host>[:<port>]<path>`, where the host is the first
/// `server_name` under `server_name_in_redirect on` (otherwise the request
/// `Host`, or the request URI authority when a client speaks HTTP/2) and the
/// port is the listener port, omitted when it is the scheme default
/// (80/http, 443/https) or when `port_in_redirect off`.
///
/// The client-facing scheme is used for both the URL scheme and the
/// scheme-default port check, which is the edge-proxy equivalent of nginx's
/// `c->ssl` test (nginx cannot see a trusted proxy's `X-Forwarded-Proto`).
fn apply_redirect_policy(
    mut response: Response<Body>,
    policy: RedirectPolicyConfig,
    variables: &ResponseHeaderVariables<'_>,
) -> Response<Body> {
    let Some(location) = response.headers().get(LOCATION) else {
        return response;
    };
    let Ok(location) = location.to_str() else {
        return response;
    };
    let Some(absolute) = absolute_redirect_location(location, policy, variables) else {
        return response;
    };
    if let Ok(value) = HeaderValue::from_str(&absolute) {
        response.headers_mut().insert(LOCATION, value);
    }
    response
}

/// The absolute `Location` nginx's header filter would build for a relative
/// target, or `None` when the target must stay as authored.
fn absolute_redirect_location(
    location: &str,
    policy: RedirectPolicyConfig,
    variables: &ResponseHeaderVariables<'_>,
) -> Option<String> {
    if !policy.absolute_redirect || !location.starts_with('/') {
        return None;
    }
    let host = if policy.server_name_in_redirect {
        variables.server_name
    } else {
        authority_host_without_port(variables.authority)
    };
    if host.is_empty() {
        return None;
    }
    let default_port = match variables.scheme {
        "https" => 443,
        "http" => 80,
        _ => 0,
    };
    let port = if policy.port_in_redirect && variables.server_port != default_port {
        Some(variables.server_port)
    } else {
        None
    };
    let mut absolute =
        String::with_capacity(variables.scheme.len() + host.len() + location.len() + 8);
    absolute.push_str(variables.scheme);
    absolute.push_str("://");
    absolute.push_str(host);
    if let Some(port) = port {
        absolute.push(':');
        absolute.push_str(&port.to_string());
    }
    absolute.push_str(location);
    Some(absolute)
}

/// The authority's host part (port removed, IPv6 brackets kept, case
/// preserved exactly as received — nginx echoes the `Host` header's host).
fn authority_host_without_port(authority: &str) -> &str {
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') {
        if let Some(end) = authority.find(']') {
            return &authority[..=end];
        }
        return authority;
    }
    match authority.rsplit_once(':') {
        Some((host, port))
            if !host.contains(':') && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            host
        }
        _ => authority,
    }
}

/// Applies the selected virtual host's security response headers. HSTS is
/// emitted only for HTTPS responses; `x_content_type_options` defaults to
/// `nosniff` and is only suppressed when explicitly disabled.
fn apply_security_headers(
    mut response: Response<Body>,
    security_headers: Option<&SecurityHeadersConfig>,
    scheme: &str,
) -> Response<Body> {
    let Some(security_headers) = security_headers else {
        return response;
    };
    let headers = response.headers_mut();
    if let Some(hsts) = &security_headers.strict_transport_security {
        if scheme == "https" {
            let mut value = format!("max-age={}", hsts.max_age_seconds);
            if hsts.include_sub_domains {
                value.push_str("; includeSubDomains");
            }
            if hsts.preload {
                value.push_str("; preload");
            }
            if let Ok(value) = HeaderValue::from_str(&value) {
                headers.insert(HeaderName::from_static("strict-transport-security"), value);
            }
        }
    }
    if let Some(frame_options) = security_headers.x_frame_options {
        let value = match frame_options {
            XFrameOptions::Deny => "DENY",
            XFrameOptions::SameOrigin => "SAMEORIGIN",
        };
        headers.insert(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static(value),
        );
    }
    if let Some(policy) = &security_headers.content_security_policy {
        if let Ok(value) = HeaderValue::from_str(policy) {
            headers.insert(HeaderName::from_static("content-security-policy"), value);
        }
    }
    if let Some(policy) = &security_headers.referrer_policy {
        if let Ok(value) = HeaderValue::from_str(policy) {
            headers.insert(HeaderName::from_static("referrer-policy"), value);
        }
    }
    if security_headers.x_content_type_options {
        headers.insert(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        );
    }
    // `securityHeaders.customHeaders` is deliberately *not* applied here: those
    // entries are nginx `add_header` semantics (status gating, `always`,
    // duplicate order, variable expansion) and are applied by
    // [`apply_nginx_response_headers`] at the response choke point so a
    // non-`always` security header is not emitted on a 4xx/5xx response.
    response
}

fn invalid_forwarded_identity_response(version: Version) -> Response<Body> {
    let mut response = text_response(
        StatusCode::BAD_REQUEST,
        "forwarded client identity is invalid\n",
    );
    if version != Version::HTTP_2 {
        response
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

fn invalid_forwarded_scheme_response(version: Version) -> Response<Body> {
    let mut response = text_response(
        StatusCode::BAD_REQUEST,
        "forwarded request metadata is invalid\n",
    );
    if version != Version::HTTP_2 {
        response
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

/// True when the app-publishing surface mounted on this entry point declares
/// routes for `authority` on this node.
///
/// `false` on an entry point with no delivery surface, and on a node with no
/// runtime set loaded — in both shapes nothing here owns the host.
fn website_surface_declares_host(state: &ListenerState, authority: &str) -> bool {
    state
        .website_delivery
        .as_ref()
        .and_then(|executor| executor.current_runtime_set())
        .is_some_and(|runtime_set| runtime_set.declares_authority(authority))
}

/// Normalized host of a registered tunnel route that serves this request's
/// path, or `None` when no agent has registered it on this gateway.
///
/// Borrows instead of taking the request so a caller can decide whether to
/// hand the request over before giving up ownership of it.
fn registered_tunnel_host(state: &ListenerState, authority: &str, path: &str) -> Option<String> {
    let tunnel_shared = state.runtime.tunnel.as_ref()?;
    let host = sdkwork_webserver_core::normalize_authority_host(authority)
        .unwrap_or_else(|| authority.to_ascii_lowercase());
    tunnel_shared
        .registry
        .match_domain_path(&host, path)
        .map(|_| host)
}

/// Relays a request whose Host matches a registered tunnel route to the owning
/// agent (PRD §30).
///
/// Always answers. A route that was registered but cannot be reached fails here
/// (404 "route is not registered" / 502 relay failure) rather than falling
/// through: the host *was* claimed, so handing the caller back a second,
/// misleading answer for a request the tunnel already owns would be wrong.
async fn relay_registered_tunnel(
    state: &ListenerState,
    admitted: &mut super::request_gate::RequestAdmissionPermit,
    host: &str,
    client_ip: IpAddr,
    visitor_port: u16,
    request: Request<Body>,
) -> Response<Body> {
    let Some(tunnel_shared) = state.runtime.tunnel.as_ref() else {
        return text_response(StatusCode::NOT_FOUND, "tunnel route is not registered\n");
    };
    if let Some(response) = classify_request(state, admitted, false, request.version()) {
        return response;
    }
    let generation = state.runtime.current();
    let relay_limits =
        crate::tunnel_bridge::RelayBodyLimits::from_config(generation.app.config().limits.clone());
    match crate::tunnel_bridge::relay_tunnel_http(
        tunnel_shared,
        &tunnel_shared.metrics,
        relay_limits,
        host,
        client_ip,
        visitor_port,
        request,
    )
    .await
    {
        Ok(response) => {
            if generation.app.config().observability.access_log {
                tracing::info!(
                    config_generation = generation.id,
                    listener_id = %state.listener_id,
                    host = %host,
                    status = response.status().as_u16(),
                    "tunnel request relayed"
                );
            }
            response
        }
        Err(crate::tunnel_bridge::TunnelRelayError::NoRoute) => {
            text_response(StatusCode::NOT_FOUND, "tunnel route is not registered\n")
        }
        Err(crate::tunnel_bridge::TunnelRelayError::Denied { bearer_required }) => {
            denied_tunnel_response(bearer_required)
        }
        Err(crate::tunnel_bridge::TunnelRelayError::Unavailable) => {
            // The host *is* claimed by the tunnel; every replica behind it is
            // down. 404 would say "nobody serves this host" and 502 would say
            // "the relay broke" — neither is true.
            tracing::warn!(host = %host, "tunnel relay refused: no healthy replica");
            text_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "no healthy replica is available for this tunnel route\n",
            )
        }
        Err(crate::tunnel_bridge::TunnelRelayError::Failure(error)) => {
            tracing::warn!(host = %host, error = %error, "tunnel relay failed");
            text_response(StatusCode::BAD_GATEWAY, "tunnel relay failed\n")
        }
    }
}

/// Response for a visitor turned away by a route's admission policy: 401 with
/// a challenge when the route wants a bearer token, 403 otherwise.
///
/// A denial is not a relay failure — answering 502 told the visitor the tunnel
/// was broken while the tunnel was behaving exactly as configured.
fn denied_tunnel_response(bearer_required: bool) -> Response<Body> {
    if bearer_required {
        let mut response = text_response(
            StatusCode::UNAUTHORIZED,
            "a visitor token is required for this tunnel route\n",
        );
        response.headers_mut().insert(
            axum::http::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"tunnel\""),
        );
        return response;
    }
    text_response(
        StatusCode::FORBIDDEN,
        "this tunnel route does not admit this visitor\n",
    )
}

fn classify_request(
    state: &ListenerState,
    admitted: &mut super::request_gate::RequestAdmissionPermit,
    operations_reserved: bool,
    version: Version,
) -> Option<Response<Body>> {
    state
        .runtime
        .request_gate
        .classify(admitted, operations_reserved)
        .err()
        .map(|rejection| match rejection {
            RequestAdmissionRejection::Saturated => {
                state
                    .runtime
                    .metrics
                    .record_request_rejection(RequestRejection::Capacity);
                overload_response(version)
            }
            RequestAdmissionRejection::ResourcePressure => {
                state
                    .runtime
                    .metrics
                    .record_request_rejection(RequestRejection::ResourcePressure);
                resource_pressure_response(version)
            }
        })
}

fn overload_response(version: Version) -> Response<Body> {
    let mut response = text_response(StatusCode::SERVICE_UNAVAILABLE, "server is overloaded\n");
    response
        .headers_mut()
        .insert(RETRY_AFTER, HeaderValue::from_static("1"));
    if version != Version::HTTP_2 {
        response
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

fn resource_pressure_response(version: Version) -> Response<Body> {
    let mut response = text_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "server resource pressure is active\n",
    );
    response
        .headers_mut()
        .insert(RETRY_AFTER, HeaderValue::from_static("1"));
    if version != Version::HTTP_2 {
        response
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

fn secure_link_rejected_response(version: Version) -> Response<Body> {
    let mut response = fixed_response(403, "text/plain; charset=utf-8", "forbidden", false);
    if version == Version::HTTP_09 {
        response.headers_mut().remove(CONTENT_TYPE);
    }
    response
}

fn limit_conn_rejected_response(version: Version, status: u16) -> Response<Body> {
    // nginx `limit_conn_status` (default 503): the configured code is the
    // whole client-observable rejection contract.
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
    let mut response = fixed_response(
        status.as_u16(),
        "text/plain; charset=utf-8",
        "too many connections",
        false,
    );
    *response.status_mut() = status;
    if version == Version::HTTP_09 {
        response.headers_mut().remove(CONTENT_TYPE);
    }
    response
}

fn limit_req_rejected_response(version: Version, status: u16) -> Response<Body> {
    // nginx's `limit_req` rejection is a bare status with no `Retry-After`
    // (the delay that would give one is not known: the zone recovers
    // continuously). The admission-pressure responses above do publish it, but
    // this path is the nginx-compatible rate-limit answer and must match the
    // oracle. `limit_req_status` (default 503) makes the code configurable.
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
    let mut response = text_response(status, "limiting requests\n");
    if version != Version::HTTP_2 {
        response
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

fn auth_basic_challenge_response(realm: &str) -> Response<Body> {
    let mut response = text_response(StatusCode::UNAUTHORIZED, "authorization required\n");
    let value = format!(
        "Basic realm=\"{}\", charset=\"UTF-8\"",
        escape_auth_realm(realm)
    );
    if let Ok(header) = HeaderValue::from_str(&value) {
        response.headers_mut().insert(WWW_AUTHENTICATE, header);
    }
    response
}

fn escape_auth_realm(realm: &str) -> String {
    realm.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_operations_candidate(request: &Request<Body>) -> bool {
    matches!(request.method().as_str(), "GET" | "HEAD") && is_operations_path(request.uri().path())
}

fn is_operations_path(path: &str) -> bool {
    matches!(path, "/healthz" | "/readyz" | "/livez")
}

fn request_uri_error_response(version: Version, error: RequestUriError) -> Response<Body> {
    let (status, message) = match error {
        RequestUriError::Invalid => (StatusCode::BAD_REQUEST, "request URI is invalid\n"),
        RequestUriError::TooLong => (StatusCode::URI_TOO_LONG, "request URI exceeds limits\n"),
    };
    let mut response = text_response(status, message);
    if version != Version::HTTP_2 {
        response
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

/// Collect the whole request body (bounded before allocation growth) for
/// upstream protocol adapters that need the raw bytes (FastCGI `STDIN`).
async fn collect_bounded_request_body(
    request: Request<Body>,
    maximum: u64,
    failure: &RequestBodyFailure,
) -> Result<(Request<Body>, Vec<u8>), Response<Body>> {
    let (parts, body) = request.into_parts();
    let version = parts.version;
    let mut stream = body.into_data_stream();
    let mut collected = Vec::new();
    let mut observed = 0_u64;
    while let Some(frame) = stream.next().await {
        let bytes = frame.map_err(|_| {
            if failure.timed_out() {
                request_body_timeout_response(version)
            } else {
                text_response(StatusCode::BAD_REQUEST, "request body framing is invalid\n")
            }
        })?;
        observed = observed.saturating_add(bytes.len() as u64);
        if observed > maximum {
            return Err(text_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body is too large\n",
            ));
        }
        collected.extend_from_slice(&bytes);
    }
    Ok((Request::from_parts(parts, Body::empty()), collected))
}

async fn drain_bounded_request_body(
    request: Request<Body>,
    maximum: u64,
    failure: &RequestBodyFailure,
) -> Result<Request<Body>, Response<Body>> {
    let (parts, body) = request.into_parts();
    let version = parts.version;
    let mut stream = body.into_data_stream();
    let mut observed = 0_u64;
    while let Some(frame) = stream.next().await {
        let bytes = frame.map_err(|_| {
            if failure.timed_out() {
                request_body_timeout_response(version)
            } else {
                text_response(StatusCode::BAD_REQUEST, "request body framing is invalid\n")
            }
        })?;
        observed = observed.saturating_add(bytes.len() as u64);
        if observed > maximum {
            return Err(text_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body is too large\n",
            ));
        }
    }
    Ok(Request::from_parts(parts, Body::empty()))
}

/// Drains the request body and serves one provider-backed resource through
/// the application-config provider executor. The provider executor is
/// assembled at bootstrap when the configuration references provider
/// resources, so its absence here is a server-internal invariant violation.
async fn serve_provider_backed_resource(
    state: &ListenerState,
    generation: &RuntimeGeneration,
    method: &str,
    request: Request<Body>,
    request_failure: RequestBodyFailure,
    route: AppConfigResourceRoute,
) -> Response<Body> {
    let Some(executor) = state.provider_resources.clone() else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "provider resource is unavailable\n",
        );
    };
    let limits = &generation.app.config().limits;
    let policy = AppConfigProviderPolicy {
        provider_timeout_ms: limits.provider_timeout_ms,
        maximum_object_bytes: limits.max_response_body_bytes,
    };
    match drain_bounded_request_body(request, limits.max_request_body_bytes, &request_failure).await
    {
        Ok(request) => {
            let query = request.uri().query().map(str::to_owned);
            super::provider_resource::serve_provider_resource(
                executor,
                method,
                query,
                request.headers().clone(),
                route,
                policy,
            )
            .await
        }
        Err(response) => response,
    }
}

fn content_length_exceeds(headers: &HeaderMap, maximum: u64) -> bool {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > maximum)
}

fn validate_request_framing(
    headers: &HeaderMap,
    version: Version,
) -> Result<(), (StatusCode, &'static str)> {
    validate_expectation(headers, version)?;

    let mut content_lengths = headers.get_all(CONTENT_LENGTH).iter();
    let has_content_length = content_lengths.next().is_some();
    if content_lengths.next().is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "multiple Content-Length headers are forbidden\n",
        ));
    }
    let has_transfer_encoding = headers.contains_key(TRANSFER_ENCODING);
    if has_content_length && has_transfer_encoding {
        return Err((
            StatusCode::BAD_REQUEST,
            "Transfer-Encoding with Content-Length is forbidden\n",
        ));
    }
    if version != Version::HTTP_11 && has_transfer_encoding {
        return Err((
            StatusCode::BAD_REQUEST,
            "Transfer-Encoding requires HTTP/1.1\n",
        ));
    }
    for value in headers.get_all(TE) {
        let value = value
            .to_str()
            .map_err(|_| (StatusCode::BAD_REQUEST, "invalid TE header\n"))?;
        if value
            .split(',')
            .map(str::trim)
            .any(|token| !token.eq_ignore_ascii_case("trailers"))
        {
            return Err((StatusCode::BAD_REQUEST, "only TE: trailers is supported\n"));
        }
    }
    Ok(())
}

fn validate_expectation(
    headers: &HeaderMap,
    version: Version,
) -> Result<(), (StatusCode, &'static str)> {
    let mut expectations = headers.get_all(EXPECT).iter();
    let Some(expectation) = expectations.next() else {
        return Ok(());
    };
    if expectations.next().is_some()
        || version != Version::HTTP_11
        || expectation
            .to_str()
            .map(|value| !value.eq_ignore_ascii_case("100-continue"))
            .unwrap_or(true)
    {
        return Err((
            StatusCode::EXPECTATION_FAILED,
            "request expectation is not supported\n",
        ));
    }
    Ok(())
}

fn request_authority(request: &Request<Body>) -> Result<String, (StatusCode, &'static str)> {
    let mut host_values = request.headers().get_all(HOST).iter();
    let header_authority = match host_values.next() {
        Some(value) => Some(
            value
                .to_str()
                .map_err(|_| (StatusCode::BAD_REQUEST, "invalid Host header\n"))?,
        ),
        None => None,
    };
    if host_values.next().is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "multiple Host headers are forbidden\n",
        ));
    }

    let uri_authority = request.uri().authority().map(|value| value.as_str());
    if let (Some(uri), Some(header)) = (uri_authority, header_authority) {
        if normalize_authority_host(uri) != normalize_authority_host(header) {
            return Err((
                StatusCode::BAD_REQUEST,
                "request authority conflicts with Host\n",
            ));
        }
    }
    let authority = uri_authority.or(header_authority).unwrap_or_default();
    if matches!(request.version(), Version::HTTP_11 | Version::HTTP_2)
        && normalize_authority_host(authority).is_none()
    {
        return Err((StatusCode::BAD_REQUEST, "request authority is required\n"));
    }
    Ok(authority.to_owned())
}

fn fixed_response(status: u16, content_type: &str, body: &str, head: bool) -> Response<Body> {
    fixed_response_with_type(
        status,
        HeaderValue::from_str(content_type).ok().as_ref(),
        body,
        head,
    )
}

/// [`fixed_response`] with an explicit optional `Content-Type`. `None` omits
/// the field (nginx sends no `Content-Type` for a bodyless 204/304, and a
/// nginx `return` body derives its type from the request URI, which can be
/// unknown when the URI carries no extension and no `default_type`).
fn fixed_response_with_type(
    status: u16,
    content_type: Option<&HeaderValue>,
    body: &str,
    head: bool,
) -> Response<Body> {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let suppress_body =
        head || status == StatusCode::NO_CONTENT || status == StatusCode::NOT_MODIFIED;
    let mut response = Response::new(if suppress_body {
        Body::empty()
    } else {
        Body::from(body.to_owned())
    });
    *response.status_mut() = status;
    if let Some(value) = content_type {
        if !matches!(status, StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED) {
            response.headers_mut().insert(CONTENT_TYPE, value.clone());
        }
    }
    let declared_length = if matches!(status, StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT) {
        0
    } else {
        body.len()
    };
    if let Ok(value) = HeaderValue::from_str(&declared_length.to_string()) {
        response.headers_mut().insert(CONTENT_LENGTH, value);
    }
    response
}

/// Renders a redirect response. nginx-compatible `return` URLs may contain
/// `$host`, `$request_uri`, and `$scheme` variables, expanded from the
/// current request (nginx `return` semantics).
/// nginx `ngx_http_empty_gif_module`: the minimal single-pixel transparent
/// GIF (43 bytes).
const EMPTY_GIF_BYTES: &[u8] = &[
    b'G', b'I', b'F', b'8', b'9', b'a', 0x01, 0x00, 0x01, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xff, 0xff, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x01, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x4c, 0x01, 0x00, 0x3b,
];

fn empty_gif_response(head: bool) -> Response<Body> {
    let mut response = Response::new(if head {
        Body::empty()
    } else {
        Body::from(EMPTY_GIF_BYTES)
    });
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("image/gif"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_LENGTH,
        HeaderValue::from_static("43"),
    );
    // nginx sets `last_modified_time = 23349600` for the canned GIF, and its
    // header filter then publishes that as the `Last-Modified` field.
    response.headers_mut().insert(
        axum::http::header::LAST_MODIFIED,
        HeaderValue::from_static("Mon, 28 Sep 1970 06:00:00 GMT"),
    );
    response
}

fn redirect_response(
    status: u16,
    location: &str,
    scheme: &str,
    authority: &str,
    path: &str,
    query: Option<&str>,
    maps: &super::map_variables::MapResults,
) -> Response<Body> {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::TEMPORARY_REDIRECT);
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    let expanded = expand_redirect_variables(location, scheme, authority, path, query, maps);
    if let Ok(value) = HeaderValue::from_str(&expanded) {
        response.headers_mut().insert(LOCATION, value);
    }
    response
}

/// Expand the nginx `return` variable subset `$host`, `$request_uri`, and
/// `$scheme` in a location template.
fn expand_redirect_variables(
    location: &str,
    scheme: &str,
    authority: &str,
    path: &str,
    query: Option<&str>,
    maps: &super::map_variables::MapResults,
) -> String {
    let request_uri = match query {
        Some(query) if !query.is_empty() => format!("{path}?{query}"),
        _ => path.to_owned(),
    };
    // Expand declared map results first; a `$` inside a resolved value is
    // escaped so the built-in pass below cannot re-expand it.
    let location = maps.expand_tokens_escaped(location);
    let mut output = String::with_capacity(location.len() + 16);
    let mut remainder = location.as_str();
    while let Some(index) = remainder.find('$') {
        output.push_str(&remainder[..index]);
        let rest = &remainder[index..];
        if let Some(escaped) = rest.strip_prefix("$$") {
            output.push('$');
            remainder = escaped;
            continue;
        }
        let variable = if rest.starts_with("$request_uri") {
            "$request_uri"
        } else if rest.starts_with("$host") {
            "$host"
        } else if rest.starts_with("$scheme") {
            "$scheme"
        } else {
            output.push('$');
            remainder = &rest[1..];
            continue;
        };
        match variable {
            "$host" => output.push_str(authority),
            "$request_uri" => output.push_str(&request_uri),
            "$scheme" => output.push_str(scheme),
            _ => {}
        }
        remainder = &rest[variable.len()..];
    }
    output.push_str(remainder);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_core::{
        CustomHeaderConfig, SecurityHeadersConfig, StrictTransportSecurityConfig, XFrameOptions,
    };

    fn security_config() -> SecurityHeadersConfig {
        SecurityHeadersConfig {
            strict_transport_security: Some(StrictTransportSecurityConfig {
                max_age_seconds: 31_536_000,
                include_sub_domains: true,
                preload: true,
            }),
            x_frame_options: Some(XFrameOptions::Deny),
            content_security_policy: Some("default-src 'self'".to_owned()),
            referrer_policy: Some("strict-origin-when-cross-origin".to_owned()),
            x_content_type_options: true,
            custom_headers: vec![CustomHeaderConfig {
                name: "X-Custom-Tag".to_owned(),
                value: "web-1".to_owned(),
                always: false,
            }],
        }
    }

    fn base_response() -> Response<Body> {
        Response::new(Body::from("ok"))
    }

    fn response_header(name: &str, value: &str, always: bool) -> CustomHeaderConfig {
        CustomHeaderConfig {
            name: name.to_owned(),
            value: value.to_owned(),
            always,
        }
    }

    fn response_header_variables() -> ResponseHeaderVariables<'static> {
        ResponseHeaderVariables {
            scheme: "http",
            authority: "headers.example.com",
            client_ip: "127.0.0.1".parse().unwrap(),
            server_port: 8080,
            method: "GET",
            request_uri: "/request-path?q=1",
            server_name: "headers.example.com",
            maps: None,
            invalid_referer: false,
        }
    }

    fn response_with_status(status: u16) -> Response<Body> {
        let mut response = base_response();
        *response.status_mut() = StatusCode::from_u16(status).expect("status");
        response
    }

    fn redirect_response_with_location(location: &str) -> Response<Body> {
        let mut response = response_with_status(301);
        response
            .headers_mut()
            .insert(LOCATION, HeaderValue::from_str(location).expect("location"));
        response
    }

    fn redirect_variables(
        scheme: &'static str,
        authority: &'static str,
        server_port: u16,
        server_name: &'static str,
    ) -> ResponseHeaderVariables<'static> {
        ResponseHeaderVariables {
            scheme,
            authority,
            client_ip: "127.0.0.1".parse().unwrap(),
            server_port,
            method: "GET",
            request_uri: "/dir",
            server_name,
            maps: None,
            invalid_referer: false,
        }
    }

    #[test]
    fn redirect_policy_constructs_the_nginx_absolute_location() {
        // Differential-confirmed against nginx 1.29.6
        // (`ngx_http_header_filter`): a relative target becomes
        // `<scheme>://<host>[:<port>]<path>`, the host is the request `Host`
        // by default, the port is the listener port, and an absolute target is
        // passed through untouched.
        let variables =
            redirect_variables("http", "first.example.com:9999", 8080, "first.example.com");
        let default = RedirectPolicyConfig::NGINX_DEFAULT;
        let response = apply_redirect_policy(
            redirect_response_with_location("/dir/"),
            default,
            &variables,
        );
        assert_eq!(
            response.headers()[LOCATION],
            "http://first.example.com:8080/dir/"
        );

        // An absolute target is never rewritten.
        let response = apply_redirect_policy(
            redirect_response_with_location("https://other.example.test/x"),
            default,
            &variables,
        );
        assert_eq!(response.headers()[LOCATION], "https://other.example.test/x");

        // A scheme-default listener port is omitted.
        let variables = redirect_variables("http", "first.example.com", 80, "first.example.com");
        let response = apply_redirect_policy(
            redirect_response_with_location("/dir/"),
            default,
            &variables,
        );
        assert_eq!(
            response.headers()[LOCATION],
            "http://first.example.com/dir/"
        );

        // `port_in_redirect off` omits the listener port.
        let variables = redirect_variables("http", "first.example.com", 8080, "first.example.com");
        let response = apply_redirect_policy(
            redirect_response_with_location("/dir/"),
            RedirectPolicyConfig {
                port_in_redirect: false,
                ..RedirectPolicyConfig::NGINX_DEFAULT
            },
            &variables,
        );
        assert_eq!(
            response.headers()[LOCATION],
            "http://first.example.com/dir/"
        );

        // `server_name_in_redirect on` uses the first server_name, not `Host`.
        let response = apply_redirect_policy(
            redirect_response_with_location("/dir/"),
            RedirectPolicyConfig {
                server_name_in_redirect: true,
                ..RedirectPolicyConfig::NGINX_DEFAULT
            },
            &variables,
        );
        assert_eq!(
            response.headers()[LOCATION],
            "http://first.example.com:8080/dir/"
        );

        // `absolute_redirect off` keeps the relative target.
        let response = apply_redirect_policy(
            redirect_response_with_location("/dir/"),
            RedirectPolicyConfig {
                absolute_redirect: false,
                ..RedirectPolicyConfig::NGINX_DEFAULT
            },
            &variables,
        );
        assert_eq!(response.headers()[LOCATION], "/dir/");
    }

    #[test]
    fn limit_status_resolution_prefers_the_location_then_the_host() {
        // Documented resolution order used by `apply_location_gates`:
        // route `limitReqStatus`/`limitConnStatus` wins, then the virtual
        // host value, then the nginx default 503. The gate itself needs a
        // running runtime, so the resolution rule is asserted here against
        // the model fields it reads.
        let host = sdkwork_webserver_core::VirtualHostConfig {
            id: "vh".to_owned(),
            listener_refs: Vec::new(),
            server_names: vec!["example.test".to_owned()],
            routes: Vec::new(),
            rewrite_phase: Vec::new(),
            security_headers: None,
            compression: None,
            mime_types: None,
            default_type: None,
            error_pages: Vec::new(),
            recursive_error_pages: false,
            cache_policy: None,
            redirect_policy: None,
            limit_req_status: Some(429),
            limit_conn_status: Some(444),
            max_ranges: None,
            merge_slashes: true,
            underscores_in_headers: false,
            ignore_invalid_headers: true,
            charset: None,
            autoindex: None,
            referer: None,
        };
        let mut route = sdkwork_webserver_core::RouteConfig {
            id: "r".to_owned(),
            route_match: sdkwork_webserver_core::RouteMatchConfig {
                path_type: sdkwork_webserver_core::RoutePathType::Prefix,
                path: "/".to_owned(),
                methods: None,
            },
            resource_ref: "res".to_owned(),
            access: Vec::new(),
            limit_req: Vec::new(),
            limit_conn: Vec::new(),
            rewrite: Vec::new(),
            auth_basic: None,
            sub_filter: None,
            secure_link: None,
            compression: None,
            mime_types: None,
            default_type: None,
            error_pages: Vec::new(),
            response_headers: Vec::new(),
            cache_policy: None,
            rewrite_phase: Vec::new(),
            max_request_body_bytes: None,
            redirect_policy: None,
            limit_req_status: None,
            limit_conn_status: Some(503),
            max_ranges: None,
            internal: false,
            charset: None,
            autoindex: None,
            referer: None,
        };
        assert_eq!(
            route
                .limit_req_status
                .or(host.limit_req_status)
                .unwrap_or(503),
            429
        );
        assert_eq!(
            route
                .limit_conn_status
                .or(host.limit_conn_status)
                .unwrap_or(503),
            503
        );
        route.limit_req_status = Some(400);
        assert_eq!(
            route
                .limit_req_status
                .or(host.limit_req_status)
                .unwrap_or(503),
            400
        );
    }

    #[test]
    fn add_header_status_gating_matches_nginx() {
        // nginx `ngx_http_headers_filter` default status list (differentially
        // confirmed against nginx 1.29.6).
        let policy = vec![
            response_header("X-Default", "d", false),
            response_header("X-Always", "a", true),
        ];
        for status in [200_u16, 201, 204, 206, 301, 302, 303, 304, 307, 308] {
            let response = apply_nginx_response_headers(
                response_with_status(status),
                Some(&policy),
                &response_header_variables(),
                "/request-path",
                Some("q=1"),
            );
            assert_eq!(response.headers()["x-default"], "d", "status {status}");
            assert_eq!(response.headers()["x-always"], "a", "status {status}");
        }
        for status in [400_u16, 401, 403, 404, 416, 500, 502, 503] {
            let response = apply_nginx_response_headers(
                response_with_status(status),
                Some(&policy),
                &response_header_variables(),
                "/request-path",
                Some("q=1"),
            );
            assert!(
                response.headers().get("x-default").is_none(),
                "status {status} must not receive a non-`always` field"
            );
            assert_eq!(response.headers()["x-always"], "a", "status {status}");
        }
    }

    #[test]
    fn add_header_preserves_declaration_order_and_duplicates() {
        let policy = vec![
            response_header("X-Dup", "one", false),
            response_header("X-Other", "middle", false),
            response_header("X-Dup", "two", false),
        ];
        let response = apply_nginx_response_headers(
            response_with_status(200),
            Some(&policy),
            &response_header_variables(),
            "/request-path",
            None,
        );
        let duplicates = response
            .headers()
            .get_all("x-dup")
            .iter()
            .map(|value| value.to_str().expect("header value"))
            .collect::<Vec<_>>();
        assert_eq!(duplicates, vec!["one", "two"]);
        assert_eq!(response.headers()["x-other"], "middle");
    }

    #[test]
    fn add_header_expands_the_supported_variables() {
        let template = "$host|$request_uri|$scheme|$server_port|$remote_addr|$uri|$args|$is_args|$request_method|$server_name|$$literal";
        let policy = vec![response_header("X-Vars", template, false)];
        let response = apply_nginx_response_headers(
            response_with_status(200),
            Some(&policy),
            &response_header_variables(),
            "/current-uri",
            Some("a=1"),
        );
        assert_eq!(
            response.headers()["x-vars"],
            "headers.example.com|/request-path?q=1|http|8080|127.0.0.1|/current-uri|a=1|?|GET|headers.example.com|$literal"
        );

        // `$host` is the request host without its port (nginx lowercases it);
        // absent query arguments expand to empty strings.
        let variables = ResponseHeaderVariables {
            authority: "Headers.Example.com:8443",
            ..response_header_variables()
        };
        let response = apply_nginx_response_headers(
            response_with_status(200),
            Some(&[response_header("X-Args", "$host|$args|$is_args", false)]),
            &variables,
            "/current-uri",
            None,
        );
        assert_eq!(response.headers()["x-args"], "headers.example.com||");
    }

    #[test]
    fn add_header_omits_an_empty_value_like_nginx() {
        // Probed against nginx 1.29.6: `add_header X-Empty "";` is accepted by
        // the parser but sends no field at all.
        let response = apply_nginx_response_headers(
            response_with_status(200),
            Some(&[response_header("X-Empty", "", false)]),
            &response_header_variables(),
            "/request-path",
            None,
        );
        assert!(response.headers().get("x-empty").is_none());
    }

    #[test]
    fn security_headers_applied_only_for_the_configured_host() {
        let response = apply_security_headers(base_response(), None, "https");
        assert!(response
            .headers()
            .get("strict-transport-security")
            .is_none());
        assert!(response.headers().get("x-frame-options").is_none());
    }

    #[test]
    fn hsts_emitted_only_over_https() {
        let config = security_config();
        let https = apply_security_headers(base_response(), Some(&config), "https");
        assert_eq!(
            https.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000; includeSubDomains; preload"
        );
        let http = apply_security_headers(base_response(), Some(&config), "http");
        assert!(http.headers().get("strict-transport-security").is_none());
    }

    #[test]
    fn fixed_security_headers_and_defaults_are_applied() {
        let config = security_config();
        let response = apply_security_headers(base_response(), Some(&config), "https");
        assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
        assert_eq!(
            response.headers().get("content-security-policy").unwrap(),
            "default-src 'self'"
        );
        assert_eq!(
            response.headers().get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        // Custom headers are nginx `add_header` entries and are applied by the
        // status-gated choke point, never unconditionally here.
        assert!(response.headers().get("x-custom-tag").is_none());
    }

    #[test]
    fn x_content_type_options_can_be_explicitly_disabled() {
        let mut config = security_config();
        config.x_content_type_options = false;
        let response = apply_security_headers(base_response(), Some(&config), "https");
        assert!(response.headers().get("x-content-type-options").is_none());
    }

    #[test]
    fn a_cluster_hop_is_taken_exactly_once() {
        // A request that carries the hop marker was relayed by a sibling, so
        // this instance serves it. Dropping that rule lets two instances whose
        // round-robin cursors point at each other (or an instance that picks
        // itself) bounce the same request forever; the marker's trust model is
        // stated on CLUSTER_HOP_HEADER.
        assert!(
            should_relay_to_sibling(true, false),
            "the first hop to a sibling is taken"
        );
        assert!(
            !should_relay_to_sibling(true, true),
            "a hop already taken is never repeated"
        );
        assert!(
            !should_relay_to_sibling(false, false),
            "a host the cluster does not serve is never relayed"
        );
        assert!(
            !should_relay_to_sibling(false, true),
            "the marker never invents a relay for an unserved host"
        );
    }
}

/// App publishing domain fallback: resolve a host that no local
/// configuration declares through the Deploy control plane and serve the
/// resolved site (GET/HEAD only; other methods keep the regular 404).
async fn serve_deploy_fallback(
    state: &ListenerState,
    scheme: sdkwork_webserver_delivery_runtime::WebsiteDeliveryScheme,
    authority: String,
    path: String,
    query: Option<String>,
    method: &Method,
    headers: &HeaderMap,
    server_addr: std::net::SocketAddr,
) -> Option<Response<Body>> {
    let fallback = state.deploy_fallback.as_ref()?;
    let delivery_method = match *method {
        Method::GET => sdkwork_webserver_delivery_runtime::WebsiteDeliveryMethod::Get,
        Method::HEAD => sdkwork_webserver_delivery_runtime::WebsiteDeliveryMethod::Head,
        _ => return None,
    };
    let request_id = sdkwork_web_core::new_request_id();
    let trace_context = sdkwork_web_core::resolve_trace_context(headers, &request_id);
    let trace_id = sdkwork_web_core::trace_id_from_traceparent(&trace_context.traceparent)
        .unwrap_or(request_id.as_str())
        .to_owned();
    let request = super::website_delivery::delivery_request(
        scheme,
        authority,
        path,
        delivery_method,
        request_id,
        trace_id,
        headers,
    )
    .ok()?;
    let served = match fallback.serve(&request).await {
        Ok(outcome) => Some(outcome),
        Err(error) => {
            tracing::warn!(error = ?error, "app-domain fallback failed; serving 404");
            None
        }
    };
    // App-config listeners meter every response in `route_request`; here
    // only website listeners record (and only actually-served outcomes —
    // a failed fallback leaves the caller's 404 unrecorded instead of
    // inventing a phantom 5xx).
    if state.website_delivery.is_some() && served.is_some() {
        if let Some(meter) = state.usage_meter.clone() {
            if let Some(hostname) =
                sdkwork_webserver_core::normalize_authority_host(&request.authority)
            {
                let attribution = fallback.attribution(&hostname).unwrap_or_default();
                let (status_class, egress_bytes) = match &served {
                    Some(sdkwork_webserver_delivery_runtime::WebsiteDeliveryOutcome::Content(
                        content,
                    )) => ("2xx", content.response_content_length),
                    Some(sdkwork_webserver_delivery_runtime::WebsiteDeliveryOutcome::Redirect(
                        _,
                    )) => ("3xx", 0),
                    Some(sdkwork_webserver_delivery_runtime::WebsiteDeliveryOutcome::NotFound) => {
                        ("4xx", 0)
                    }
                    Some(
                        sdkwork_webserver_delivery_runtime::WebsiteDeliveryOutcome::NotModified,
                    ) => ("2xx", 0),
                    None => ("5xx", 0),
                };
                meter.record(crate::usage_metering::MeteredRequest {
                    hostname: &hostname,
                    server_ip: server_addr.ip(),
                    server_port: server_addr.port(),
                    listener_id: &state.listener_id,
                    attribution: &attribution,
                    ingress_bytes: 0,
                    egress_bytes,
                    status_class,
                });
            }
        }
    }
    served.map(|outcome| super::website_delivery::outcome_response(outcome, query.as_deref()))
}

fn status_class_label(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        _ => "5xx",
    }
}
