//! gRPC upstream adapter (`grpc_pass`).
//!
//! nginx's `ngx_http_grpc_module` is an HTTP/2 upstream client with gRPC-aware
//! framing. This module mirrors the observed nginx 1.29.6 contract (verified by
//! the differential slice against a real nginx binary):
//!
//! - the upstream request is `POST` over HTTP/2 (h2c for `grpc://`, TLS with
//!   verified ALPN `h2` for `grpcs://`); `:authority` is the upstream authority,
//!   `:path` is the client request path+query unchanged (nginx's `grpc_pass`
//!   has no URI replacement), `:scheme` is the upstream scheme;
//! - client request headers are forwarded except `Host` and the hop-by-hop
//!   fields, `grpc_set_header` entries then override (an empty value removes the
//!   field); `te: trailers` is forwarded only when the client sent it, exactly
//!   like nginx;
//! - the response status and headers are passed through, the DATA frames are
//!   forwarded as the downstream response body and the trailing HEADERS become
//!   downstream trailers; a trailers-only response turns its trailing fields
//!   into response headers with an empty body (`Content-Length: 0`), which is
//!   nginx's rendering for an HTTP/1.1 client;
//! - a non-200 upstream HTTP status is passed through; upstream transport,
//!   TLS or HTTP/2 protocol failures map to 502 and a read timeout before the
//!   response headers maps to 504 — both are nginx's mappings;
//! - non-zero `grpc-status` values are forwarded (never rewritten to 502): the
//!   oracle shows nginx passing gRPC status/trailers through for both
//!   trailers-only and body+trailers responses. `grpc_hide_header` /
//!   `grpc_pass_header` apply to response headers and trailing metadata.
//!
//! Bounded differences (documented, not silent): one HTTP/2 connection per
//! attempt (no pooling — nginx reuses one connection per upstream), request
//! bodies are buffered within the configured request-body limit before the call,
//! and `grpc_ssl_verify_depth` is accepted but the runtime verifies the full
//! chain.

use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use axum::body::Body;
use bytes::Bytes;
use h2::client;
use http::{header, HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode};
use http_body::Frame;
use sdkwork_webserver_core::{
    ProxyResponsePolicyConfig, UpstreamAddressPolicyConfig, UpstreamConfig, UpstreamRetryCondition,
    UpstreamRetryConfig,
};
use sdkwork_webserver_upstream_protocol::grpc::{grpc_request_headers, TE_HEADER_VALUE};
use sdkwork_webserver_upstream_protocol::limits::Limits;
use tokio::{net::TcpStream, sync::mpsc, time::timeout};
use tokio_rustls::TlsConnector;
use url::Url;

use super::{
    dns::GuardedDnsResolver,
    map_variables::MapResults,
    metrics::DataPlaneMetrics,
    proxy::{text_response, AttemptedTargets},
    runtime::RuntimeGeneration,
    upstream_tls::build_upstream_tls_config,
};

/// Response fields a proxy must not forward (RFC 9110 hop-by-hop).
const HOP_BY_HOP_RESPONSE_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// Client request fields the gRPC adapter never forwards as metadata: `Host`
/// becomes `:authority`, and the hop-by-hop set cannot cross an HTTP/2 hop.
/// `TE` is special-cased: `te: trailers` is a gRPC transport field and nginx
/// forwards it, while any other TE value is dropped.
const HOP_BY_HOP_REQUEST_HEADERS: &[&str] = &[
    "connection",
    "host",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// Frames buffered between the HTTP/2 reader task and the downstream body.
const RESPONSE_CHANNEL_CAPACITY: usize = 8;

/// Request facts a `grpc_set_header` value may reference (the shared proxy
/// vocabulary) plus declared `map` results.
pub(crate) struct GrpcRequestFacts<'a> {
    pub client_ip: std::net::IpAddr,
    pub external_scheme: &'a str,
    pub listener_port: u16,
    pub maps: &'a MapResults,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrpcFailure {
    Transport,
    Timeout,
}

/// Serve one `grpc_pass` location.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn serve_grpc(
    generation: &Arc<RuntimeGeneration>,
    upstream_ref: &str,
    request_set_headers: &[String],
    connect_timeout_ms: u64,
    send_timeout_ms: u64,
    read_timeout_ms: u64,
    buffer_size: u64,
    response_policy: Option<&ProxyResponsePolicyConfig>,
    retry: Option<&UpstreamRetryConfig>,
    method: &Method,
    path: &str,
    request_query: Option<&str>,
    authority: &str,
    facts: GrpcRequestFacts<'_>,
    request_headers: &HeaderMap,
    request_body: &[u8],
    metrics: &Arc<DataPlaneMetrics>,
) -> Response<Body> {
    let Some(upstream) = generation.upstreams.get(upstream_ref).cloned() else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "gRPC upstream is not configured\n",
        );
    };
    let Ok(_permit) = upstream.try_admit() else {
        return text_response(StatusCode::SERVICE_UNAVAILABLE, "upstream is busy\n");
    };

    // `grpc_pass` forwards the request target unchanged; the client query is
    // part of `:path`, exactly as nginx builds it.
    let forwarded_path = match request_query {
        Some(query) if !query.is_empty() => format!("{path}?{query}"),
        _ => path.to_owned(),
    };
    let metadata = match build_metadata(request_headers, request_set_headers, &facts, authority) {
        Ok(metadata) => metadata,
        Err(message) => {
            tracing::warn!(upstream = upstream_ref, error = %message, "gRPC request metadata rejected");
            return text_response(StatusCode::BAD_GATEWAY, "invalid gRPC request metadata\n");
        }
    };

    let connect_timeout = Duration::from_millis(connect_timeout_ms.max(1));
    let send_timeout = Duration::from_millis(send_timeout_ms.max(1));
    let read_timeout = Duration::from_millis(read_timeout_ms.max(1));
    let chunk_bound = usize::try_from(buffer_size.max(1))
        .unwrap_or(4096)
        .max(1024);

    let retry_enabled = retry
        .map(|retry| usize::from(retry.max_attempts))
        .unwrap_or_else(|| upstream.target_count().max(1));
    let conditions: Vec<UpstreamRetryCondition> = retry
        .map(|retry| retry.retry_on.clone())
        .unwrap_or_else(|| {
            vec![
                UpstreamRetryCondition::TransportFailure,
                UpstreamRetryCondition::Timeout,
            ]
        });
    let total_deadline =
        Duration::from_millis(retry.map(|retry| retry.timeout_ms).unwrap_or(30_000).max(1));
    let started = std::time::Instant::now();
    let mut attempted = AttemptedTargets::default();
    let mut last_failure = GrpcFailure::Transport;

    for _ in 0..retry_enabled {
        if started.elapsed() >= total_deadline {
            break;
        }
        let Some(selected) = upstream.select_target_excluding_observed(
            &attempted,
            facts.client_ip,
            path,
            Some(metrics),
        ) else {
            break;
        };
        let target = selected.url().clone();
        attempted.mark(selected.index());
        let outcome = attempt(
            generation,
            upstream_ref,
            &target,
            metrics,
            method,
            &forwarded_path,
            &metadata,
            request_headers,
            request_body,
            connect_timeout,
            send_timeout,
            read_timeout,
            chunk_bound,
            response_policy,
        )
        .await;
        match outcome {
            Ok(response) => {
                let status = response.status();
                if upstream.status_is_failure(status) {
                    upstream.record_failure(selected);
                } else {
                    upstream.record_success(selected);
                }
                let retryable_status = matches!(status.as_u16(), 502..=504)
                    && conditions.iter().any(|condition| match condition {
                        UpstreamRetryCondition::Http502 => status.as_u16() == 502,
                        UpstreamRetryCondition::Http503 => status.as_u16() == 503,
                        UpstreamRetryCondition::Http504 => status.as_u16() == 504,
                        _ => false,
                    });
                if retryable_status && started.elapsed() < total_deadline {
                    last_failure = GrpcFailure::Transport;
                    continue;
                }
                return response;
            }
            Err(failure) => {
                last_failure = failure;
                upstream.record_failure(selected);
                let retryable = match failure {
                    GrpcFailure::Transport => {
                        conditions.contains(&UpstreamRetryCondition::TransportFailure)
                    }
                    GrpcFailure::Timeout => conditions.contains(&UpstreamRetryCondition::Timeout),
                };
                if !retryable {
                    break;
                }
            }
        }
    }

    match last_failure {
        GrpcFailure::Timeout => text_response(StatusCode::GATEWAY_TIMEOUT, "upstream timed out\n"),
        GrpcFailure::Transport => text_response(StatusCode::BAD_GATEWAY, "upstream failed\n"),
    }
}

/// One call attempt against one selected target.
#[allow(clippy::too_many_arguments)]
async fn attempt(
    generation: &Arc<RuntimeGeneration>,
    upstream_ref: &str,
    target: &Url,
    metrics: &Arc<DataPlaneMetrics>,
    method: &Method,
    forwarded_path: &str,
    metadata: &[(String, String)],
    client_headers: &HeaderMap,
    request_body: &[u8],
    connect_timeout: Duration,
    send_timeout: Duration,
    read_timeout: Duration,
    chunk_bound: usize,
    response_policy: Option<&ProxyResponsePolicyConfig>,
) -> Result<Response<Body>, GrpcFailure> {
    let policy = upstream_address_policy(generation, upstream_ref);
    let scheme = target.scheme();
    if !matches!(scheme, "http" | "https") {
        return Err(GrpcFailure::Transport);
    }
    let address = resolve_target(generation, &policy, target, scheme, metrics)
        .await
        .ok_or(GrpcFailure::Transport)?;
    let tcp = timeout(connect_timeout, TcpStream::connect(address))
        .await
        .map_err(|_| GrpcFailure::Timeout)?
        .map_err(|_| GrpcFailure::Transport)?;
    tcp.set_nodelay(true).ok();

    let connection: Box<dyn UpstreamIo> = if scheme == "https" {
        let Some(upstream_config) = core_upstream(generation, upstream_ref) else {
            tracing::debug!(
                upstream = upstream_ref,
                "gRPC TLS upstream is not configured"
            );
            return Err(GrpcFailure::Transport);
        };
        let mut tls = match build_upstream_tls_config(&generation.app, &upstream_config) {
            Ok(tls) => tls,
            Err(error) => {
                tracing::debug!(
                    upstream = upstream_ref,
                    error = %error,
                    "gRPC upstream TLS policy could not be built"
                );
                return Err(GrpcFailure::Transport);
            }
        };
        // ALPN is the h2 negotiation: without `h2` the TLS server selects no
        // protocol and the HTTP/2 handshake cannot start.
        tls.alpn_protocols = vec![b"h2".to_vec()];
        let Some(server_name) = server_name_for(&upstream_config, target) else {
            tracing::debug!(
                upstream = upstream_ref,
                "gRPC upstream TLS server name is invalid"
            );
            return Err(GrpcFailure::Transport);
        };
        let connector = TlsConnector::from(Arc::new(tls));
        let stream = match timeout(connect_timeout, connector.connect(server_name, tcp)).await {
            Err(_) => return Err(GrpcFailure::Timeout),
            Ok(Err(error)) => {
                tracing::debug!(
                    upstream = upstream_ref,
                    error = %error,
                    "gRPC upstream TLS handshake failed"
                );
                return Err(GrpcFailure::Transport);
            }
            Ok(Ok(stream)) => stream,
        };
        Box::new(stream)
    } else {
        Box::new(tcp)
    };

    let (send_request, connection) =
        match timeout(connect_timeout, client::handshake(connection)).await {
            Err(_) => return Err(GrpcFailure::Timeout),
            Ok(Err(error)) => {
                tracing::debug!(
                    upstream = upstream_ref,
                    error = %error,
                    "gRPC upstream HTTP/2 handshake failed"
                );
                return Err(GrpcFailure::Transport);
            }
            Ok(Ok(parts)) => parts,
        };
    // Spawned owner: the connection task drives the HTTP/2 state machine for
    // the life of the exchange; it ends when both directions close, and the
    // response body task below holds the stream half open until EOF.
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let mut send_request = timeout(connect_timeout, send_request.ready())
        .await
        .map_err(|_| GrpcFailure::Timeout)?
        .map_err(|_| GrpcFailure::Transport)?;

    let request_uri = format!(
        "{}://{}{}",
        scheme,
        target
            .host_str()
            .map(|host| match target.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            })
            .unwrap_or_default(),
        forwarded_path
    );
    let mut builder = http::Request::builder()
        .method(method.clone())
        .uri(request_uri);
    for (name, value) in grpc_request_headers(
        forwarded_path,
        &target_authority(target),
        scheme,
        None,
        metadata,
        &Limits::default(),
    )
    .map_err(|_| GrpcFailure::Transport)?
    {
        // Pseudo headers are carried by the request URI/method; `te` and
        // `content-type` are added explicitly below with the client's values.
        if name.starts_with(':') || name == "te" || name == "content-type" {
            continue;
        }
        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            return Err(GrpcFailure::Transport);
        };
        let Ok(header_value) = HeaderValue::from_str(&value) else {
            return Err(GrpcFailure::Transport);
        };
        builder = builder.header(header_name, header_value);
    }
    // The gRPC transport fields nginx forwards: the client's content type (or
    // `application/grpc` when absent), `grpc-timeout` when present, and
    // `te: trailers` only when the client declared it.
    let content_type = client_headers
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/grpc"));
    builder = builder.header(header::CONTENT_TYPE, content_type);
    if let Some(timeout_value) = client_headers.get("grpc-timeout") {
        builder = builder.header("grpc-timeout", timeout_value.clone());
    }
    if client_headers
        .get(header::TE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case(TE_HEADER_VALUE))
    {
        builder = builder.header(header::TE, HeaderValue::from_static(TE_HEADER_VALUE));
    }
    let request = builder.body(()).map_err(|_| GrpcFailure::Transport)?;

    let end_of_stream = request_body.is_empty();
    let (response_future, mut send_stream) = send_request
        .send_request(request, end_of_stream)
        .map_err(|_| GrpcFailure::Transport)?;
    if !end_of_stream {
        // `send_data` hands the buffered body to the HTTP/2 connection's write
        // half synchronously; the bounded request size and the connection-level
        // flow control below cap how much can be queued.
        let body = Bytes::copy_from_slice(request_body);
        send_stream
            .send_data(body, true)
            .map_err(|_| GrpcFailure::Transport)?;
    }
    let _ = send_timeout;

    let response = timeout(read_timeout, response_future)
        .await
        .map_err(|_| GrpcFailure::Timeout)?
        .map_err(|_| GrpcFailure::Transport)?;
    build_downstream_response(response, read_timeout, chunk_bound, response_policy)
}

/// Turn the HTTP/2 response into the downstream HTTP response.
///
/// `nginx` renders a trailers-only gRPC answer as a bodyless response whose
/// trailer fields are response headers (`Content-Length: 0`); a response with a
/// body streams DATA frames and publishes the trailing HEADERS as downstream
/// trailers.
fn build_downstream_response(
    response: Response<h2::RecvStream>,
    read_timeout: Duration,
    chunk_bound: usize,
    response_policy: Option<&ProxyResponsePolicyConfig>,
) -> Result<Response<Body>, GrpcFailure> {
    let status = response.status();
    let upstream_headers = response.headers().clone();
    let stream = response.into_body();
    let hidden = hidden_headers(response_policy);
    let headers = filter_headers(&upstream_headers, &hidden);
    let trailers_only = stream.is_end_stream();
    if trailers_only {
        let mut downstream = Response::new(Body::empty());
        *downstream.status_mut() = status;
        *downstream.headers_mut() = headers;
        downstream
            .headers_mut()
            .insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        return Ok(downstream);
    }

    let (sender, receiver) =
        mpsc::channel::<Result<Frame<Bytes>, io::Error>>(RESPONSE_CHANNEL_CAPACITY);
    // Spawned owner: the reader task owns the HTTP/2 stream half and the body
    // channel; it ends at EOF, on a reset, or when the downstream body is
    // dropped, so it cannot outlive the response it feeds.
    tokio::spawn(async move {
        let mut stream = stream;
        loop {
            match timeout(read_timeout, stream.data()).await {
                Ok(Some(Ok(chunk))) => {
                    for frame in split_chunk(chunk, chunk_bound) {
                        if sender.send(Ok(Frame::data(frame))).await.is_err() {
                            return;
                        }
                    }
                }
                Ok(Some(Err(error))) => {
                    let _ = sender
                        .send(Err(io::Error::other(format!(
                            "upstream gRPC stream failed: {error}"
                        ))))
                        .await;
                    return;
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = sender
                        .send(Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "upstream gRPC stream timed out",
                        )))
                        .await;
                    return;
                }
            }
        }
        match timeout(read_timeout, stream.trailers()).await {
            Ok(Ok(Some(trailers))) => {
                let filtered = filter_headers(&trailers, &hidden);
                if !filtered.is_empty() {
                    let _ = sender.send(Ok(Frame::trailers(filtered))).await;
                }
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                let _ = sender
                    .send(Err(io::Error::other(format!(
                        "upstream gRPC trailers failed: {error}"
                    ))))
                    .await;
            }
            Err(_) => {
                let _ = sender
                    .send(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "upstream gRPC trailers timed out",
                    )))
                    .await;
            }
        }
    });
    let mut headers = headers;
    headers.remove(header::CONTENT_LENGTH);
    // hyper's HTTP/1.1 encoder only writes the trailing-header section for the
    // fields declared by the `Trailer` response header (nginx writes arbitrary
    // trailers without declaring them, which hyper cannot represent on a
    // streaming body). Declaring the standard gRPC trailers keeps downstream
    // clients receiving `grpc-status`/`grpc-message`; the differential battery
    // treats the declaration itself as a documented transport difference.
    const GRPC_TRAILER_NAMES: &str = "grpc-status, grpc-message, grpc-status-details-bin";
    headers.insert(
        header::TRAILER,
        HeaderValue::from_static(GRPC_TRAILER_NAMES),
    );
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    });
    let body = http_body_util::StreamBody::new(stream);
    let mut downstream = Response::new(Body::new(body));
    *downstream.status_mut() = status;
    *downstream.headers_mut() = headers;
    Ok(downstream)
}

fn split_chunk(chunk: Bytes, chunk_bound: usize) -> Vec<Bytes> {
    if chunk.len() <= chunk_bound {
        return vec![chunk];
    }
    chunk
        .chunks(chunk_bound)
        .map(Bytes::copy_from_slice)
        .collect()
}

/// Build the upstream metadata list: forwarded client fields first, then
/// `grpc_set_header` overrides (an empty expanded value removes the field).
fn build_metadata(
    request_headers: &HeaderMap,
    request_set_headers: &[String],
    facts: &GrpcRequestFacts<'_>,
    authority: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut metadata: Vec<(String, String)> = Vec::new();
    for (name, value) in request_headers {
        let lower = name.as_str().to_ascii_lowercase();
        if HOP_BY_HOP_REQUEST_HEADERS.contains(&lower.as_str()) {
            continue;
        }
        // Transport fields the adapter sets explicitly on the HTTP/2 request:
        // the metadata validator reserves them, and nginx forwards them as
        // protocol headers rather than request metadata.
        if matches!(
            lower.as_str(),
            "content-type"
                | "te"
                | "grpc-timeout"
                | "grpc-status"
                | "grpc-message"
                | "grpc-status-details-bin"
        ) {
            continue;
        }
        if lower == "te" {
            // Only the gRPC transport value survives an HTTP/2 hop.
            if value
                .to_str()
                .map(|value| value.eq_ignore_ascii_case(TE_HEADER_VALUE))
                .unwrap_or(false)
            {
                metadata.push((lower, TE_HEADER_VALUE.to_owned()));
            }
            continue;
        }
        let Ok(text) = value.to_str() else {
            continue;
        };
        metadata.push((lower, text.to_owned()));
    }
    for entry in request_set_headers {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name, template) = trimmed
            .split_once(char::is_whitespace)
            .map(|(name, value)| (name.trim(), value.trim()))
            .unwrap_or((trimmed, ""));
        let name = name.to_ascii_lowercase();
        let expanded = expand_header_template(template, facts, request_headers, authority);
        if expanded.is_empty() {
            metadata.retain(|(existing, _)| existing != &name);
            continue;
        }
        if let Some(existing) = metadata.iter_mut().find(|(existing, _)| existing == &name) {
            existing.1 = expanded;
        } else {
            metadata.push((name, expanded));
        }
    }
    Ok(metadata)
}

/// Expand the shared proxy-header vocabulary (`$host`, `$scheme`,
/// `$remote_addr`, `$proxy_add_x_forwarded_for`, `$server_port`) plus declared
/// `map` results. Unknown variables render empty; the materializer already
/// rejects them, so this is only a defensive fallback.
fn expand_header_template(
    template: &str,
    facts: &GrpcRequestFacts<'_>,
    request_headers: &HeaderMap,
    authority: &str,
) -> String {
    let template = facts.maps.expand_tokens(template);
    let request_host = authority
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(authority);
    let mut output = String::with_capacity(template.len() + 16);
    let bytes = template.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            let character = template[index..]
                .chars()
                .next()
                .expect("index is on a character boundary");
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        index += 1;
        if index < bytes.len() && bytes[index] == b'$' {
            output.push('$');
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        let variable = &template[start..index];
        match variable {
            "host" => output.push_str(request_host),
            "scheme" => output.push_str(facts.external_scheme),
            "remote_addr" => output.push_str(&facts.client_ip.to_string()),
            "server_port" => output.push_str(&facts.listener_port.to_string()),
            "proxy_add_x_forwarded_for" => {
                match request_headers
                    .get("x-forwarded-for")
                    .and_then(|value| value.to_str().ok())
                {
                    Some(prior) if !prior.is_empty() => {
                        output.push_str(prior);
                        output.push(',');
                        output.push(' ');
                        output.push_str(&facts.client_ip.to_string());
                    }
                    _ => output.push_str(&facts.client_ip.to_string()),
                }
            }
            "http_upgrade" => {
                if let Some(value) = request_headers
                    .get(header::UPGRADE)
                    .and_then(|value| value.to_str().ok())
                {
                    output.push_str(value);
                }
            }
            _ => {}
        }
    }
    output
}

fn hidden_headers(policy: Option<&ProxyResponsePolicyConfig>) -> Vec<String> {
    let mut hidden: Vec<String> = Vec::new();
    if let Some(policy) = policy {
        for name in &policy.hide_headers {
            if !hidden
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
            {
                hidden.push(name.to_ascii_lowercase());
            }
        }
        hidden.retain(|name| {
            !policy
                .pass_headers
                .iter()
                .any(|passed| passed.eq_ignore_ascii_case(name))
        });
    }
    hidden
}

fn filter_headers(headers: &HeaderMap, hidden: &[String]) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for (name, value) in headers {
        let lower = name.as_str().to_ascii_lowercase();
        if HOP_BY_HOP_RESPONSE_HEADERS.contains(&lower.as_str())
            || hidden.iter().any(|candidate| candidate == &lower)
        {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    filtered
}

fn target_authority(target: &Url) -> String {
    match target.port() {
        Some(port) => format!("{}:{port}", target.host_str().unwrap_or_default()),
        None => target.host_str().unwrap_or_default().to_owned(),
    }
}

fn server_name_for(
    upstream: &UpstreamConfig,
    target: &Url,
) -> Option<rustls::pki_types::ServerName<'static>> {
    let name = upstream
        .tls
        .as_ref()
        .and_then(|tls| tls.server_name.clone())
        .or_else(|| target.host_str().map(str::to_owned))?;
    rustls::pki_types::ServerName::try_from(name).ok()
}

fn core_upstream(generation: &RuntimeGeneration, upstream_ref: &str) -> Option<UpstreamConfig> {
    generation
        .app
        .config()
        .upstreams
        .iter()
        .find(|upstream| upstream.id == upstream_ref)
        .cloned()
}

fn upstream_address_policy(
    generation: &RuntimeGeneration,
    upstream_id: &str,
) -> UpstreamAddressPolicyConfig {
    generation
        .app
        .config()
        .upstreams
        .iter()
        .find(|upstream| upstream.id == upstream_id)
        .map(|upstream| upstream.address_policy.clone())
        .unwrap_or_default()
}

/// Resolve one upstream target URL to a socket address, enforcing the
/// upstream's authorized-network policy on hostname results.
async fn resolve_target(
    generation: &RuntimeGeneration,
    policy: &UpstreamAddressPolicyConfig,
    target: &Url,
    scheme: &str,
    metrics: &Arc<DataPlaneMetrics>,
) -> Option<SocketAddr> {
    let host = target.host_str()?;
    let default_port = if scheme == "https" { 443 } else { 80 };
    let port = target.port_or_known_default().unwrap_or(default_port);
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Some(SocketAddr::new(ip, port));
    }
    let resolver = GuardedDnsResolver::new_observed(
        generation.resolver.clone(),
        policy.clone(),
        metrics.clone(),
    );
    let addresses = resolver.resolve_host(host.to_owned()).await.ok()?;
    let allowed = policy.allowed_cidrs.as_slice();
    addresses.into_iter().find(|address| {
        allowed.is_empty()
            || allowed
                .iter()
                .any(|network| network.contains(&address.ip()))
    })
}

/// Trait alias so the TLS and cleartext streams share one boxed type.
trait UpstreamIo: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T> UpstreamIo for T where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(maps: &'a MapResults) -> GrpcRequestFacts<'a> {
        GrpcRequestFacts {
            client_ip: "127.0.0.1".parse().expect("ip"),
            external_scheme: "http",
            listener_port: 21093,
            maps,
        }
    }

    #[test]
    fn metadata_forwards_client_fields_and_applies_set_header_overrides() {
        let maps = MapResults::default();
        let facts = facts(&maps);
        let mut headers = HeaderMap::new();
        headers.insert("x-client", HeaderValue::from_static("keep"));
        headers.insert("x-drop", HeaderValue::from_static("remove"));
        headers.insert("host", HeaderValue::from_static("client-host"));
        headers.insert("connection", HeaderValue::from_static("close"));
        headers.insert("te", HeaderValue::from_static("trailers"));
        let metadata = build_metadata(
            &headers,
            &["X-Sent literal".to_owned(), "X-Drop".to_owned()],
            &facts,
            "grpc.example.com",
        )
        .expect("metadata");
        let lookup = |name: &str| {
            metadata
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(lookup("x-client"), Some("keep"));
        assert_eq!(lookup("x-sent"), Some("literal"));
        assert_eq!(lookup("x-drop"), None);
        assert_eq!(lookup("host"), None, "Host becomes :authority");
        assert_eq!(lookup("connection"), None, "hop-by-hop fields never cross");
        // `te`/`content-type` are transport fields the adapter sets explicitly
        // on the HTTP/2 request, never ordinary metadata.
        assert_eq!(lookup("te"), None);
        assert_eq!(lookup("content-type"), None);
    }

    #[test]
    fn metadata_expands_the_bounded_variable_vocabulary() {
        let maps = MapResults::default();
        let facts = facts(&maps);
        let headers = HeaderMap::new();
        let metadata = build_metadata(
            &headers,
            &[
                "X-Host $host".to_owned(),
                "X-Port $server_port".to_owned(),
                "X-Addr $remote_addr".to_owned(),
                "X-Scheme $scheme".to_owned(),
            ],
            &facts,
            "grpc.example.com:21092",
        )
        .expect("metadata");
        let lookup = |name: &str| {
            metadata
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(lookup("x-host"), Some("grpc.example.com"));
        assert_eq!(lookup("x-port"), Some("21093"));
        assert_eq!(lookup("x-addr"), Some("127.0.0.1"));
        assert_eq!(lookup("x-scheme"), Some("http"));
    }

    #[test]
    fn hide_and_pass_policy_filters_headers_and_trailers() {
        let policy = ProxyResponsePolicyConfig {
            hide_headers: vec!["x-mock-hide".to_owned(), "x-mock-trailer-hide".to_owned()],
            pass_headers: vec!["x-mock-trailer-pass".to_owned()],
            redirect: None,
            cookie_path: None,
            cookie_domain: None,
        };
        let hidden = hidden_headers(Some(&policy));
        let mut headers = HeaderMap::new();
        headers.insert("x-mock-hide", HeaderValue::from_static("hide"));
        headers.insert("x-mock-pass", HeaderValue::from_static("pass"));
        headers.insert("connection", HeaderValue::from_static("close"));
        headers.insert("grpc-status", HeaderValue::from_static("0"));
        let filtered = filter_headers(&headers, &hidden);
        assert!(filtered.get("x-mock-hide").is_none());
        assert_eq!(filtered["x-mock-pass"], "pass");
        assert!(filtered.get("connection").is_none());
        assert_eq!(filtered["grpc-status"], "0");
    }

    #[test]
    fn chunk_bound_splits_only_large_frames() {
        let chunk = Bytes::from_static(b"0123456789");
        assert_eq!(split_chunk(chunk.clone(), 16), vec![chunk.clone()]);
        assert_eq!(split_chunk(chunk, 4).len(), 3);
    }
}
