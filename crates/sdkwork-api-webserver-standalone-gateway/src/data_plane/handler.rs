use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{
        header::{
            AUTHORIZATION, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, EXPECT, HOST, LOCATION,
            RETRY_AFTER, TE, TRANSFER_ENCODING, USER_AGENT, WWW_AUTHENTICATE,
        },
        HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode, Version,
    },
};
use futures_util::StreamExt;
use sdkwork_webserver_core::{
    apply_rewrites, evaluate_access, evaluate_auth_basic, normalize_authority_host,
    prefer_h5_surface, verify_secure_link,
    website_runtime::{ProviderResourceReference, WebsiteProviderType},
    AccessDecision, AuthBasicDecision, ErrorPageConfig, ResourceConfig, RewriteOutcome,
    RoutePathType, SecureLinkFailure, SecurityHeadersConfig, SelectedRoute, XFrameOptions,
    MAX_REWRITE_INTERNAL_REDIRECTS,
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

async fn route_admitted_request(
    peer: SocketAddr,
    transport_peer: SocketAddr,
    state: ListenerState,
    request: Request<Body>,
    admitted: &mut super::request_gate::RequestAdmissionPermit,
) -> Response<Body> {
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
    let normalized_path = match validate_request_uri(request.uri(), &generation.app.config().limits)
    {
        Ok(path) => path,
        Err(error) => {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            return request_uri_error_response(request.version(), error);
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
    let method = request.method().as_str().to_owned();
    let mut path = normalized_path;
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
    if let Some(host) = registered_tunnel_host(&state, &authority) {
        if !website_surface_declares_host(&state, &authority) {
            return relay_registered_tunnel(&state, admitted, &host, client_ip, request).await;
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
                        request,
                    )
                    .await;
                    drop(lease);
                    match outcome {
                        Ok(response) => response,
                        Err(crate::tunnel_bridge::TunnelRelayError::NoRoute) => {
                            text_response(StatusCode::NOT_FOUND, "cluster route is unavailable\n")
                        }
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
    let selected = loop {
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
            return text_response(StatusCode::NOT_FOUND, "route was not found\n");
        };
        if candidate.route.rewrite.is_empty() {
            break candidate;
        }
        let query = request.uri().query();
        match apply_rewrites(&path, query, &candidate.route.rewrite) {
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
                return redirect_response(
                    status,
                    &location,
                    scheme.as_str(),
                    &authority,
                    &path,
                    request.uri().query(),
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
    if evaluate_access(client_ip, &selected.route.access) == AccessDecision::Deny {
        if let Some(response) = classify_request(&state, admitted, false, request.version()) {
            return response;
        }
        state
            .runtime
            .metrics
            .record_request_rejection(RequestRejection::AccessDenied);
        if let Some(paged) = resolve_error_pages(
            &state,
            &generation,
            &selected,
            &authority,
            StatusCode::FORBIDDEN,
            request.method(),
            request.headers(),
        )
        .await
        {
            return paged;
        }
        return text_response(StatusCode::FORBIDDEN, "access denied\n");
    }
    match evaluate_auth_basic(
        request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        selected.route.auth_basic.as_ref(),
    ) {
        AuthBasicDecision::Inactive | AuthBasicDecision::Allow => {}
        AuthBasicDecision::Challenge => {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
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
            return auth_basic_challenge_response(realm);
        }
    }
    if !selected.route.limit_req.is_empty() {
        let limit_req = state.runtime.limit_req.load();
        if limit_req.admit(client_ip, &selected.route.limit_req) == LimitReqDecision::Reject {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            state
                .runtime
                .metrics
                .record_request_rejection(RequestRejection::RateLimited);
            return limit_req_rejected_response(request.version());
        }
    }
    // nginx error_page applies to generated errors; keep the request
    // identity for the internal redirect before any dispatch consumes the
    // request. Captured only when the route or host declares error pages.
    let error_page_saved =
        if selected.route.error_pages.is_empty() && selected.virtual_host.error_pages.is_empty() {
            None
        } else {
            Some((request.method().clone(), request.headers().clone()))
        };
    let limit_conn_lease = match state
        .runtime
        .limit_conn
        .load()
        .admit(client_ip, &selected.route.limit_conn)
    {
        Ok(lease) => lease,
        Err(LimitConnDecision::Reject) => {
            if let Some(response) = classify_request(&state, admitted, false, request.version()) {
                return response;
            }
            state
                .runtime
                .metrics
                .record_request_rejection(RequestRejection::ConnectionLimited);
            return limit_conn_rejected_response(request.version());
        }
        Err(LimitConnDecision::Allow) => {
            unreachable!("limit_conn admission only errors with Reject")
        }
    };
    if let Some(secure_link) = selected.route.secure_link.clone() {
        let prefix = selected.route.route_match.path.as_str();
        let query = request.uri().query();
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
                if let Some(response) = classify_request(&state, admitted, false, request.version())
                {
                    return response;
                }
                state
                    .runtime
                    .metrics
                    .record_request_rejection(RequestRejection::LinkInvalid);
                return secure_link_rejected_response(request.version());
            }
        }
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
            body,
            ..
        } => match drain_bounded_request_body(
            request,
            generation.app.config().limits.max_request_body_bytes,
            &request_failure,
        )
        .await
        {
            Ok(_) => fixed_response(*status, content_type, body, method == "HEAD"),
            Err(response) => response,
        },
        ResourceConfig::Redirect {
            status, location, ..
        } => {
            let query = request.uri().query().map(str::to_owned);
            match drain_bounded_request_body(
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
                ),
                Err(response) => response,
            }
        }
        ResourceConfig::Static {
            id,
            strip_prefix,
            spa_fallback,
            ..
        } => {
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
            let Some(root) = (if prefer_h5 {
                generation
                    .app
                    .static_h5_root(id)
                    .or_else(|| generation.app.static_root(id))
            } else {
                generation.app.static_root(id)
            }) else {
                return text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "static resource is unavailable\n",
                );
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
                        spa_fallback.as_deref(),
                        &path,
                        request,
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
                    request_failure,
                    tunnel_supervisor: &state.runtime.tunnel_supervisor,
                    metrics: &state.runtime.metrics,
                    cache: generation.proxy_cache.clone(),
                },
                request,
            )
            .await;
            // nginx `proxy_intercept_errors on`: a proxied response whose
            // status has an error_page mapping is replaced by the page.
            let intercepted = if *proxy_intercept_errors {
                if let Some((method, headers)) = error_page_saved.as_ref() {
                    resolve_error_pages(
                        &state,
                        &generation,
                        &selected,
                        &authority,
                        proxy_response.status(),
                        method,
                        headers,
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
    // Route-level `sub_filter` rides on the response so the substitution
    // layer can apply it without re-selecting the route.
    if let Some(sub_filter) = selected.route.sub_filter.clone() {
        response
            .extensions_mut()
            .insert(super::sub_filter::SubFilterExtension(sub_filter));
    }
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
    // Keep the limit_conn slot held for the whole response lifetime: the
    // lease drops when the response body completes or is abandoned.
    response.map(|body| {
        Body::new(super::limit_conn::LeaseBody {
            inner: body,
            lease: limit_conn_lease,
        })
    })
}

/// One nginx `error_page` hop: resolve the mapping for `status`, re-run
/// location selection for the target URI, and serve the target route.
/// Returns `None` when no mapping applies or the target is not a static
/// resource (v1). The response keeps the target's own status; the caller
/// applies the `=code` override only when the target served successfully.
async fn run_error_page_hop<'a>(
    state: &ListenerState,
    generation: &'a RuntimeGeneration,
    pages: &'a [ErrorPageConfig],
    authority: &str,
    status: StatusCode,
    method: &Method,
    headers: &HeaderMap,
) -> Option<(Response<Body>, StatusCode, &'a [ErrorPageConfig])> {
    let entry = pages
        .iter()
        .rev()
        .find(|page| page.codes.contains(&status.as_u16()))?;
    let candidate =
        generation
            .app
            .select_route(&state.listener_id, authority, &entry.uri, method.as_str())?;
    let ResourceConfig::Static {
        root,
        strip_prefix,
        spa_fallback,
        ..
    } = candidate.resource
    else {
        return None;
    };
    // Conditionals and ranges are stripped so the error page serves cleanly
    // instead of re-triggering the original 304/416 semantics.
    let mut rebuilt_headers = headers.clone();
    rebuilt_headers.remove(axum::http::header::RANGE);
    rebuilt_headers.remove(axum::http::header::IF_RANGE);
    rebuilt_headers.remove(axum::http::header::IF_NONE_MATCH);
    rebuilt_headers.remove(axum::http::header::IF_MODIFIED_SINCE);
    let rebuilt = {
        let mut builder = Request::builder()
            .method(method.clone())
            .uri(entry.uri.as_str());
        *builder.headers_mut()? = rebuilt_headers;
        builder.body(Body::empty()).ok()?
    };
    let response = serve_static(
        std::path::Path::new(root),
        candidate.route,
        *strip_prefix,
        spa_fallback.as_deref(),
        &entry.uri,
        rebuilt,
    )
    .await;
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
/// to the mapped page, honoring `=code` overrides and, when the host enables
/// `recursive_error_pages`, mapping an error produced while serving the page
/// itself (bounded hops). Returns `None` when nothing maps and the original
/// response must stand.
async fn resolve_error_pages(
    state: &ListenerState,
    generation: &RuntimeGeneration,
    selected: &SelectedRoute<'_>,
    authority: &str,
    status: StatusCode,
    method: &Method,
    headers: &HeaderMap,
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
        let Some(entry) = pages
            .iter()
            .rev()
            .find(|page| page.codes.contains(&current_status.as_u16()))
        else {
            return None;
        };
        let (hop_response, target_status, next_pages) = run_error_page_hop(
            state,
            generation,
            pages,
            authority,
            current_status,
            method,
            headers,
        )
        .await?;
        let served_cleanly = !(target_status.is_client_error() || target_status.is_server_error());
        if served_cleanly {
            let final_status = entry
                .response_code
                .and_then(|code| StatusCode::from_u16(code).ok())
                .unwrap_or(current_status);
            let (mut parts, body) = hop_response.into_parts();
            parts.status = final_status;
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

/// Applies the route/host `error_page` mapping to a generated static error
/// response when the request identity was captured for the redirect.
async fn apply_error_pages(
    state: &ListenerState,
    generation: &RuntimeGeneration,
    selected: &SelectedRoute<'_>,
    authority: &str,
    response: Response<Body>,
    saved: Option<&(Method, HeaderMap)>,
) -> Response<Body> {
    let status = response.status();
    let Some((method, headers)) = saved else {
        return response;
    };
    if !(status.is_client_error() || status.is_server_error()) {
        return response;
    }
    let Some(paged) = resolve_error_pages(
        state, generation, selected, authority, status, method, headers,
    )
    .await
    else {
        return response;
    };
    paged
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
    for custom in &security_headers.custom_headers {
        let Ok(name) = HeaderName::from_bytes(custom.name.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(&custom.value) else {
            continue;
        };
        headers.insert(name, value);
    }
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

/// Normalized host of a registered tunnel route for this request's authority,
/// or `None` when no agent has registered the host on this gateway.
///
/// Borrows instead of taking the request so a caller can decide whether to
/// hand the request over before giving up ownership of it.
fn registered_tunnel_host(state: &ListenerState, authority: &str) -> Option<String> {
    let tunnel_shared = state.runtime.tunnel.as_ref()?;
    let host = sdkwork_webserver_core::normalize_authority_host(authority)
        .unwrap_or_else(|| authority.to_ascii_lowercase());
    tunnel_shared.registry.match_domain(&host).map(|_| host)
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
    request: Request<Body>,
) -> Response<Body> {
    let Some(tunnel_shared) = state.runtime.tunnel.as_ref() else {
        return text_response(StatusCode::NOT_FOUND, "tunnel route is not registered\n");
    };
    if let Some(response) = classify_request(state, admitted, false, request.version()) {
        return response;
    }
    let generation = state.runtime.current();
    match crate::tunnel_bridge::relay_tunnel_http(
        tunnel_shared,
        &tunnel_shared.metrics,
        host,
        client_ip,
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
        Err(crate::tunnel_bridge::TunnelRelayError::Failure(error)) => {
            tracing::warn!(host = %host, error = %error, "tunnel relay failed");
            text_response(StatusCode::BAD_GATEWAY, "tunnel relay failed\n")
        }
    }
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

fn limit_conn_rejected_response(version: Version) -> Response<Body> {
    let mut response = fixed_response(
        503,
        "text/plain; charset=utf-8",
        "too many connections",
        false,
    );
    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
    if version == Version::HTTP_09 {
        response.headers_mut().remove(CONTENT_TYPE);
    }
    response
}

fn limit_req_rejected_response(version: Version) -> Response<Body> {
    let mut response = text_response(StatusCode::SERVICE_UNAVAILABLE, "limiting requests\n");
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
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let suppress_body =
        head || status == StatusCode::NO_CONTENT || status == StatusCode::NOT_MODIFIED;
    let mut response = Response::new(if suppress_body {
        Body::empty()
    } else {
        Body::from(body.to_owned())
    });
    *response.status_mut() = status;
    if let Ok(value) = HeaderValue::from_str(content_type) {
        response.headers_mut().insert(CONTENT_TYPE, value);
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
fn redirect_response(
    status: u16,
    location: &str,
    scheme: &str,
    authority: &str,
    path: &str,
    query: Option<&str>,
) -> Response<Body> {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::TEMPORARY_REDIRECT);
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    let expanded = expand_redirect_variables(location, scheme, authority, path, query);
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
) -> String {
    let request_uri = match query {
        Some(query) if !query.is_empty() => format!("{path}?{query}"),
        _ => path.to_owned(),
    };
    let mut output = String::with_capacity(location.len() + 16);
    let mut remainder = location;
    while let Some(index) = remainder.find('$') {
        output.push_str(&remainder[..index]);
        let rest = &remainder[index..];
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
            }],
        }
    }

    fn base_response() -> Response<Body> {
        Response::new(Body::from("ok"))
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
        assert_eq!(response.headers().get("x-custom-tag").unwrap(), "web-1");
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
