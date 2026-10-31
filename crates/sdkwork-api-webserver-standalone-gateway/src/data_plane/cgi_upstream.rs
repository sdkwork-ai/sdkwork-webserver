//! FastCGI 1.0 upstream adapter (`fastcgi_pass`).
//!
//! The wire codec lives in `sdkwork-webserver-upstream-protocol`; this module
//! owns the nginx-compatible request construction and response mapping:
//!
//! - the effective `fastcgi_param` list is evaluated against the request
//!   variable vocabulary (plus declared `map` results), and nginx's automatic
//!   `HTTP_<name>` parameters are appended for every request header that is not
//!   already covered by a declared parameter (with nginx's `HTTP_HOST` value
//!   rule and duplicate-header joining);
//! - `fastcgi_split_path_info` computes `$fastcgi_script_name` /
//!   `$fastcgi_path_info` from the current URI, and `fastcgi_index` is appended
//!   when the script name ends with `/` (nginx variable order);
//! - the request is sent over a bounded TCP connection using the upstream
//!   selection, passive-health and retry policy already configured for the
//!   upstream, and the FastCGI response records are parsed into an HTTP
//!   response with `fastcgi_hide_header` / `fastcgi_pass_header` applied.
//!
//! Transport (Wave 7A): `*_bind` binds the upstream source address through
//! `TcpSocket` (IP literal only), `*_socket_keepalive` sets `SO_KEEPALIVE`, and
//! `fastcgi_keep_conn` returns a cleanly completed connection to the bounded
//! per-generation pool so the next request on the same target can reuse it.
//! Unix-socket pass targets are typed-plane only (the nginx compatibility plane
//! fails closed with a located diagnostic pointing at `[http] unixSocketRoot`)
//! and are connected through the same seam with `UnixStream`.
//!
//! Bounded differences (documented, not silent): uWSGI/SCGI responses have no
//! protocol-level end marker (nginx never reuses those connections either), so
//! they are never pooled.
//!
//! Wave 7D response policy (executed, nginx-oracle differential):
//! `*_buffer_size`/`*_buffers`/`*_busy_buffers_size` bound the response header
//! block and read chunks; `*_store`/`*_store_access` write the upstream body
//! through a bounded temp file and an atomic rename (confinement re-checked at
//! write time, `nginx.storeRoot` for the typed plane);
//! `*_limit_rate` paces the relay through a token-bucket body stream;
//! `*_force_ranges` (plus an upstream `Accept-Ranges: bytes`) enables nginx's
//! single/suffix range semantics (multi-range declines to the full
//! representation with `Accept-Ranges`, exactly like the FastCGI path of nginx
//! 1.29.6); `*_ignore_headers` is accepted for nginx's recognised field set
//! (the runtime never processes those upstream-driven subsystems, which is the
//! ignored state the directive requests); `*_pass_request_body off` suppresses
//! the request body; `memcached_gzip_flag` derives `Content-Encoding` from the
//! `VALUE` flags field.
//!
//! Wave 14A temp spool (executed, nginx-oracle differential): a decoded
//! upstream body that exceeds the resource's in-memory response buffer bank
//! (`*_buffer_size + *_buffers`) is written to a bounded `0600` temp file under
//! the resource's `*_temp_path` (confined to `[http] tempRoot`) or the
//! per-generation spool directory, capped by `*_max_temp_file_size` (0
//! disables the tier, exactly like nginx), written in `*_temp_file_write_size`
//! chunks, and served byte-identically with the exact `Content-Length` (ranges
//! read only the requested slice from disk). Every spool file is removed on
//! completion, error or drop; a global per-generation budget bounds in-flight
//! bytes and files.
//!
//! Wave 14A connection control: `*_ignore_client_abort on` runs each upstream
//! attempt on a bounded detached task, so a client connection abort no longer
//! cancels the in-flight upstream request (nginx semantics `off` = abort, the
//! default); `fastcgi_catch_stderr` searches every `FCGI_STDERR` record for the
//! configured substring, logs it and makes the response invalid (nginx answers
//! 502 and does not retry the next upstream by default); `fastcgi_send_lowat`
//! is accepted-unenforced with nginx-on-Linux semantics.
//!
//! Carried-forward bounded differences (explicit, not fake):
//! `*_buffering off` and `*_request_buffering off` are materialized but the
//! adapter still buffers the bounded response and request body before the
//! exchange completes. Honouring them needs a body-streaming seam in the
//! handler + exchange (response chunks forwarded as they arrive with
//! backpressure, FastCGI `STDIN` streamed from the client body); the rest of
//! Wave 7D (store/rate/range/pass_request_body) does not depend on it. A
//! response that exceeds `*_max_temp_file_size` completes in memory (nginx
//! switches to an unbuffered passthrough, which this buffering seam cannot
//! express) and is still served byte-identically; only the storage tier
//! differs.

use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use rustls::pki_types::ServerName;
use sdkwork_webserver_core::CgiUpstreamProtocol;
use sdkwork_webserver_core::{
    ProxyResponsePolicyConfig, UpstreamAddressPolicyConfig, UpstreamBufferingConfig,
    UpstreamConfig, UpstreamParamConfig, UpstreamRetryCondition, UpstreamRetryConfig,
    UpstreamStoreConfig, UpstreamTempPolicyConfig,
};
use sdkwork_webserver_upstream_protocol::{
    fastcgi::{
        encode_request, FastCgiParams, FastCgiResponseEvent, FastCgiResponseParser,
        RequestIdAllocator,
    },
    memcached::{
        encode_get as encode_memcached_get, MemcachedResponseEvent, MemcachedResponseParser,
    },
    scgi::{encode_request as encode_scgi_request, ScgiResponseEvent, ScgiResponseParser},
    uwsgi::{encode_request as encode_uwsgi_request, UwsgiResponseEvent, UwsgiResponseParser},
    Limits,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};
use url::Url;

use super::{
    dns::GuardedDnsResolver,
    map_variables::MapResults,
    metrics::DataPlaneMetrics,
    protocol_pool::{connect_stream, connect_tls_stream, PoolKey, ProtocolPool, ResolvedTarget},
    proxy::text_response,
    runtime::RuntimeGeneration,
    temp_spool::{
        BodyBufferPolicy, BufferedResponseBody, SpoolingBodyBuffer, TempSpool, DEFAULT_WRITE_CHUNK,
    },
    upstream_tls::build_upstream_tls_config,
};

/// nginx `ngx_http_fastcgi_hide_headers` default-hidden response fields (the
/// synthetic `Status` field is consumed by the response parser before this
/// list applies).
const DEFAULT_HIDDEN_RESPONSE_HEADERS: &[&str] = &[
    "x-accel-expires",
    "x-accel-redirect",
    "x-accel-limit-rate",
    "x-accel-buffering",
    "x-accel-charset",
];

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

/// Request facts the FastCGI parameter expander and script-name computation
/// need. Every field is borrowed from the live request.
pub(crate) struct CgiRequestFacts<'a> {
    pub method: &'a Method,
    pub scheme: &'a str,
    pub authority: &'a str,
    /// Current normalized URI path (`$uri`, affected by internal rewrites).
    pub path: &'a str,
    /// Original request target (`$request_uri`).
    pub raw_request_uri: &'a str,
    pub query: Option<&'a str>,
    pub client_ip: IpAddr,
    pub listener_port: u16,
    pub server_protocol: &'a str,
    pub server_name: &'a str,
    pub headers: &'a HeaderMap,
    pub document_root: Option<&'a str>,
    pub script_name: String,
    pub path_info: String,
    pub maps: &'a MapResults,
}

impl CgiRequestFacts<'_> {
    pub(super) fn expand(&self, template: &str) -> String {
        let mut output = String::with_capacity(template.len() + 16);
        let mut remainder = template;
        while let Some(index) = remainder.find('$') {
            output.push_str(&remainder[..index]);
            let rest = &remainder[index + 1..];
            if let Some(escaped) = rest.strip_prefix('$') {
                output.push('$');
                remainder = escaped;
                continue;
            }
            let end = rest
                .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .unwrap_or(rest.len());
            if end == 0 {
                output.push('$');
                remainder = rest;
                continue;
            }
            let name = &rest[..end];
            match self.variable(name) {
                Some(value) => output.push_str(&value),
                None => {
                    output.push('$');
                    output.push_str(name);
                }
            }
            remainder = &rest[end..];
        }
        output.push_str(remainder);
        output
    }

    /// Clone the facts with a different effective request method and header
    /// map (CGI cache HEAD→GET conversion and conditional revalidation).
    pub(crate) fn with_overrides<'b>(
        &'b self,
        method: &'b Method,
        headers: &'b HeaderMap,
    ) -> CgiRequestFacts<'b> {
        CgiRequestFacts {
            method,
            scheme: self.scheme,
            authority: self.authority,
            path: self.path,
            raw_request_uri: self.raw_request_uri,
            query: self.query,
            client_ip: self.client_ip,
            listener_port: self.listener_port,
            server_protocol: self.server_protocol,
            server_name: self.server_name,
            headers,
            document_root: self.document_root,
            script_name: self.script_name.clone(),
            path_info: self.path_info.clone(),
            maps: self.maps,
        }
    }

    pub(crate) fn variable(&self, name: &str) -> Option<String> {
        let value = match name {
            "host" => self
                .authority
                .rsplit_once(':')
                .filter(|(_, port)| port.parse::<u16>().is_ok())
                .map_or(self.authority, |(host, _)| host)
                .to_ascii_lowercase(),
            "scheme" => self.scheme.to_owned(),
            "remote_addr" => self.client_ip.to_string(),
            "server_port" => self.listener_port.to_string(),
            "request_method" => self.method.as_str().to_owned(),
            "server_name" => self.server_name.to_owned(),
            "server_protocol" => self.server_protocol.to_owned(),
            "request_uri" => self.raw_request_uri.to_owned(),
            "uri" | "document_uri" => self.path.to_owned(),
            "args" | "query_string" => self.query.unwrap_or("").to_owned(),
            "is_args" => self
                .query
                .filter(|query| !query.is_empty())
                .map_or_else(String::new, |_| "?".to_owned()),
            "content_type" => self.header_value("content-type"),
            "content_length" => self.header_value("content-length"),
            "document_root" => self.document_root.unwrap_or("").to_owned(),
            "request_filename" => {
                format!("{}{}", self.document_root.unwrap_or(""), self.path)
            }
            "fastcgi_script_name" => self.script_name.clone(),
            "fastcgi_path_info" => self.path_info.clone(),
            "https" => {
                if self.scheme.eq_ignore_ascii_case("https") {
                    "on".to_owned()
                } else {
                    String::new()
                }
            }
            _ => {
                if let Some(header_name) = name.strip_prefix("http_") {
                    return Some(joined_header(self.headers, &header_name.replace('_', "-")));
                }
                if let Some(argument) = name.strip_prefix("arg_") {
                    return Some(raw_argument(self.query, argument));
                }
                if let Some(cookie) = name.strip_prefix("cookie_") {
                    return Some(raw_cookie(self.headers, cookie));
                }
                return self.maps.lookup(name);
            }
        };
        Some(value)
    }

    fn header_value(&self, name: &str) -> String {
        joined_header(self.headers, name)
    }
}

/// nginx `$http_<name>`: repeated fields joined with `", "`, OWS-trimmed.
fn joined_header(headers: &HeaderMap, name: &str) -> String {
    let mut output = String::new();
    for value in headers.get_all(name) {
        if !output.is_empty() {
            output.push_str(", ");
        }
        output.push_str(value.to_str().unwrap_or_default().trim());
    }
    output
}

/// nginx `$arg_<name>`: raw (not percent-decoded) first occurrence.
fn raw_argument(query: Option<&str>, name: &str) -> String {
    let Some(query) = query else {
        return String::new();
    };
    for pair in query.split('&') {
        let (candidate, value) = match pair.split_once('=') {
            Some((candidate, value)) => (candidate, value),
            None => (pair, ""),
        };
        if candidate == name {
            return value.to_owned();
        }
    }
    String::new()
}

/// nginx `$cookie_<name>`: raw first matching cookie.
fn raw_cookie(headers: &HeaderMap, name: &str) -> String {
    for header in headers.get_all("cookie") {
        let Ok(header) = header.to_str() else {
            continue;
        };
        for pair in header.split(';') {
            let pair = pair.trim();
            if let Some((candidate, value)) = pair.split_once('=') {
                if candidate.trim() == name {
                    return value.trim().to_owned();
                }
            }
        }
    }
    String::new()
}

/// Compute nginx's `$fastcgi_script_name` / `$fastcgi_path_info`.
///
/// nginx applies `fastcgi_split_path_info` to the current URI first (two
/// captures become script name / path info; no match keeps the full URI and an
/// empty path info), then appends `fastcgi_index` when the script name still
/// ends with `/`.
fn compute_script_names(
    path: &str,
    split_path_info: Option<&str>,
    index: Option<&str>,
) -> (String, String) {
    let mut script_name = path.to_owned();
    let mut path_info = String::new();
    if let Some(pattern) = split_path_info {
        if let Ok(regex) = regex::Regex::new(pattern) {
            if let Some(captures) = regex.captures(path) {
                script_name = captures
                    .get(1)
                    .map(|capture| capture.as_str().to_owned())
                    .unwrap_or_else(|| path.to_owned());
                path_info = captures
                    .get(2)
                    .map(|capture| capture.as_str().to_owned())
                    .unwrap_or_default();
            }
        }
    }
    if script_name.ends_with('/') {
        if let Some(index) = index {
            script_name.push_str(index);
        }
    }
    (script_name, path_info)
}

/// Build the effective FastCGI parameter list.
fn build_parameters(
    declared: &[UpstreamParamConfig],
    facts: &CgiRequestFacts<'_>,
    pass_request_headers: bool,
) -> Result<FastCgiParams, String> {
    let mut variables: Vec<(String, String)> = Vec::with_capacity(declared.len() + 16);
    let mut declared_names: HashSet<String> = HashSet::with_capacity(declared.len());
    for param in declared {
        let value = facts.expand(&param.value);
        if param.if_not_empty && value.is_empty() {
            continue;
        }
        declared_names.insert(param.name.to_ascii_uppercase());
        variables.push((param.name.clone(), value));
    }
    if pass_request_headers {
        let request_port = facts
            .authority
            .rsplit_once(':')
            .filter(|(_, port)| port.parse::<u16>().is_ok())
            .map(|(_, port)| port);
        let host = facts
            .authority
            .rsplit_once(':')
            .filter(|(_, port)| port.parse::<u16>().is_ok())
            .map_or(facts.authority, |(host, _)| host)
            .to_ascii_lowercase();
        let mut seen: HashSet<String> = HashSet::new();
        for name in facts.headers.keys() {
            let variable = format!(
                "HTTP_{}",
                name.as_str().to_ascii_uppercase().replace('-', "_")
            );
            if !seen.insert(variable.clone()) {
                continue;
            }
            if declared_names.contains(&variable) {
                continue;
            }
            let value = if name == header::HOST {
                match request_port {
                    Some(port) => format!("{host}:{port}"),
                    None => host.clone(),
                }
            } else if name.as_str().eq_ignore_ascii_case("cookie") {
                let mut values = Vec::new();
                for value in facts.headers.get_all(name) {
                    values.push(value.to_str().unwrap_or_default().trim());
                }
                values.join("; ")
            } else {
                let mut values = Vec::new();
                for value in facts.headers.get_all(name) {
                    values.push(value.to_str().unwrap_or_default().trim());
                }
                values.join(", ")
            };
            variables.push((variable, value));
        }
    }
    FastCgiParams::from_variables(&variables, &Limits::default()).map_err(|error| error.to_string())
}

/// Resolve one upstream target URL to a stream destination. `unix:` targets are
/// validated again here against the typed `nginx.unixSocketRoot` before any
/// socket is opened (defense in depth: semantic validation already confined
/// them).
async fn resolve_stream_target(
    generation: &RuntimeGeneration,
    policy: &UpstreamAddressPolicyConfig,
    metrics: &Arc<DataPlaneMetrics>,
    target: &Url,
) -> Result<ResolvedTarget, ()> {
    if target.scheme() == "unix" {
        let path = sdkwork_webserver_core::unix_target_path(target.as_str()).ok_or(())?;
        let root = generation
            .app
            .config()
            .nginx
            .unix_socket_root
            .as_deref()
            .ok_or(())?;
        if !sdkwork_webserver_core::unix_path_is_confined(root, &path) {
            return Err(());
        }
        #[cfg(unix)]
        {
            return Ok(ResolvedTarget::Unix(std::path::PathBuf::from(path)));
        }
        #[cfg(not(unix))]
        {
            return Err(());
        }
    }
    let address = resolve_target(generation, policy, metrics, target).await?;
    Ok(ResolvedTarget::Tcp(address))
}

/// Resolve one upstream target URL to a socket address, enforcing the
/// upstream's authorized-network policy on hostname results.
async fn resolve_target(
    generation: &RuntimeGeneration,
    policy: &UpstreamAddressPolicyConfig,
    metrics: &Arc<DataPlaneMetrics>,
    target: &Url,
) -> Result<SocketAddr, ()> {
    let host = target.host_str().ok_or(())?;
    let port = target.port_or_known_default().unwrap_or(80);
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    let resolver = GuardedDnsResolver::new_observed(
        generation.resolver.clone(),
        policy.clone(),
        metrics.clone(),
    );
    let addresses = resolver
        .resolve_host(host.to_owned())
        .await
        .map_err(|_| ())?;
    let allowed = policy.allowed_cidrs.as_slice();
    addresses
        .into_iter()
        .find(|address| {
            allowed.is_empty()
                || allowed
                    .iter()
                    .any(|network| network.contains(&address.ip()))
        })
        .ok_or(())
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

/// Wire protocol of one CGI-family exchange.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WireProtocol {
    FastCgi,
    Uwsgi,
    Scgi,
}

/// Incremental response parser for the CGI-family protocols that answer with a
/// raw HTTP response (`uwsgi` / `scgi`); FastCGI keeps its record parser.
enum ResponseParser {
    FastCgi(FastCgiResponseParser),
    Uwsgi(UwsgiResponseParser),
    Scgi(ScgiResponseParser),
}

impl ResponseParser {
    fn new(protocol: WireProtocol, limits: Limits) -> Self {
        match protocol {
            WireProtocol::FastCgi => Self::FastCgi(FastCgiResponseParser::new(limits)),
            WireProtocol::Uwsgi => Self::Uwsgi(UwsgiResponseParser::new(limits)),
            WireProtocol::Scgi => Self::Scgi(ScgiResponseParser::new(limits)),
        }
    }

    /// Feed bytes into the parser; returns `true` when the response is complete
    /// and false when more bytes are needed (`uwsgi`/`scgi` stream a raw HTTP
    /// body, so completion is decided by the caller from Content-Length/EOF).
    ///
    /// The body sink writes the decoded bytes into the bounded spool tier, and
    /// every FastCGI `STDERR` record is logged and evaluated against
    /// `fastcgi_catch_stderr` exactly where nginx does it (before the response
    /// head is accepted).
    #[allow(clippy::too_many_arguments)]
    async fn feed(
        &mut self,
        input: &[u8],
        maximum_body: u64,
        head: &mut Option<sdkwork_webserver_upstream_protocol::HttpResponseHead>,
        body: &mut SpoolingBodyBuffer,
        catch_stderr: &[String],
    ) -> Result<bool, ExchangeFailure> {
        match self {
            Self::FastCgi(parser) => {
                for event in parser.feed(input).map_err(|_| ExchangeFailure::Protocol)? {
                    match event {
                        FastCgiResponseEvent::Head(value) => *head = Some(value),
                        FastCgiResponseEvent::Body(chunk) => {
                            append_body(body, &chunk, maximum_body).await?;
                        }
                        FastCgiResponseEvent::Stderr(chunk) => {
                            record_stderr(&chunk, catch_stderr)?;
                        }
                        FastCgiResponseEvent::End(end) => {
                            if !end.is_complete() {
                                return Err(ExchangeFailure::Protocol);
                            }
                        }
                    }
                }
                Ok(parser.is_complete())
            }
            Self::Uwsgi(parser) => {
                for event in parser.feed(input).map_err(|_| ExchangeFailure::Protocol)? {
                    match event {
                        UwsgiResponseEvent::Head(value) => *head = Some(value),
                        UwsgiResponseEvent::Body(chunk) => {
                            append_body(body, &chunk, maximum_body).await?;
                        }
                    }
                }
                Ok(false)
            }
            Self::Scgi(parser) => {
                for event in parser.feed(input).map_err(|_| ExchangeFailure::Protocol)? {
                    match event {
                        ScgiResponseEvent::Head(value) => *head = Some(value),
                        ScgiResponseEvent::Body(chunk) => {
                            append_body(body, &chunk, maximum_body).await?;
                        }
                    }
                }
                Ok(false)
            }
        }
    }

    fn finish(&self) -> Result<(), ExchangeFailure> {
        match self {
            Self::FastCgi(parser) => parser.finish().map_err(|_| ExchangeFailure::Protocol),
            Self::Uwsgi(parser) => parser.finish().map_err(|_| ExchangeFailure::Protocol),
            Self::Scgi(parser) => parser.finish().map_err(|_| ExchangeFailure::Protocol),
        }
    }
}

async fn append_body(
    body: &mut SpoolingBodyBuffer,
    chunk: &[u8],
    maximum_body: u64,
) -> Result<(), ExchangeFailure> {
    body.append(chunk, maximum_body)
        .await
        .map_err(|_| ExchangeFailure::Protocol)
}

/// nginx `ngx_http_fastcgi_process_header` STDERR handling: the message is
/// logged at error level (nginx `NGX_LOG_ERR`) with trailing `LF`/`CR`/`.`/space
/// trimmed, then every `fastcgi_catch_stderr` pattern is searched inside that
/// trimmed message; a match makes the response invalid
/// (`NGX_HTTP_UPSTREAM_INVALID_HEADER` → 502, no default `next_upstream`).
fn record_stderr(chunk: &[u8], catch_stderr: &[String]) -> Result<(), ExchangeFailure> {
    let trimmed = trim_stderr(chunk);
    if trimmed.is_empty() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(trimmed);
    // Bound the log line: a misbehaving FastCGI app must not be able to emit
    // unbounded error-log volume from one record.
    let logged: String = message.chars().take(1024).collect();
    tracing::error!(stderr = %logged, "FastCGI sent in stderr");
    for pattern in catch_stderr {
        if message.contains(pattern.as_str()) {
            tracing::error!(
                pattern = %pattern,
                "fastcgi_catch_stderr matched; the upstream response is invalid (502)"
            );
            return Err(ExchangeFailure::InvalidHeader);
        }
    }
    Ok(())
}

/// nginx trims trailing line terminators, dots and spaces from an STDERR
/// message before logging/searching it.
fn trim_stderr(chunk: &[u8]) -> &[u8] {
    let mut end = chunk.len();
    while end > 0 && matches!(chunk[end - 1], b'\n' | b'\r' | b'.' | b' ') {
        end -= 1;
    }
    &chunk[..end]
}

fn declared_body_length(
    head: &sdkwork_webserver_upstream_protocol::HttpResponseHead,
) -> Option<u64> {
    head.header("content-length")
        .and_then(|value| value.trim().parse::<u64>().ok())
}

/// Idle-pool policy for one `*_keep_conn` resource.
#[derive(Clone)]
struct PoolSpec {
    pool: Arc<ProtocolPool>,
    per_key_max: usize,
    idle_timeout: Duration,
}

/// Everything one CGI-family exchange needs, owned so an
/// `*_ignore_client_abort on` resource can run the exchange on a detached
/// bounded task without borrowing the request.
#[derive(Clone)]
struct ExchangeContext {
    generation: Arc<RuntimeGeneration>,
    upstream_id: String,
    address_policy: UpstreamAddressPolicyConfig,
    metrics: Arc<DataPlaneMetrics>,
    protocol: WireProtocol,
    connect_timeout: Duration,
    send_timeout: Duration,
    read_timeout: Duration,
    bind_address: Option<IpAddr>,
    socket_keepalive: bool,
    pool: Option<PoolSpec>,
    buffer: BodyBufferPolicy,
    limits: Limits,
    catch_stderr: Arc<Vec<String>>,
}

/// One completed upstream exchange: status, forwarded response headers and the
/// buffered (memory or spooled) body.
type ExchangeOutcome = (StatusCode, HeaderMap, BufferedResponseBody);

/// Execute one FastCGI/CGI-family request/response exchange over a fresh
/// connection.
async fn exchange(
    context: ExchangeContext,
    target: Url,
    wire: Arc<Vec<u8>>,
) -> Result<ExchangeOutcome, ExchangeFailure> {
    let generation = &context.generation;
    let resolved = resolve_stream_target(
        generation,
        &context.address_policy,
        &context.metrics,
        &target,
    )
    .await
    .map_err(|_| ExchangeFailure::Transport)?;
    // A CGI target is TLS when its URL uses the `https` scheme: the nginx
    // materializer emits `https://` for `uwsgi_pass suwsgi://…` and the typed
    // plane marks an explicitly TLS CGI target the same way. The TLS policy is
    // the same per-upstream policy the HTTP proxy and gRPC adapters use.
    let tls_dial = if target.scheme() == "https" {
        let Some(upstream_config) = core_upstream(generation, &context.upstream_id) else {
            tracing::debug!(
                upstream = %context.upstream_id,
                "CGI TLS upstream is not configured"
            );
            return Err(ExchangeFailure::Transport);
        };
        let config = match build_upstream_tls_config(&generation.app, &upstream_config) {
            Ok(config) => Arc::new(config),
            Err(error) => {
                tracing::debug!(
                    upstream = %context.upstream_id,
                    error = %error,
                    "CGI upstream TLS policy could not be built"
                );
                return Err(ExchangeFailure::Transport);
            }
        };
        let Some(server_name) = cgi_server_name_for(&upstream_config, &target) else {
            tracing::debug!(
                upstream = %context.upstream_id,
                "CGI upstream TLS server name is invalid"
            );
            return Err(ExchangeFailure::Transport);
        };
        Some((config, server_name))
    } else {
        None
    };
    let pool_key = context.pool.as_ref().map(|_policy| PoolKey {
        upstream: context.upstream_id.clone(),
        target: pool_target_key(&target),
        bind: context.bind_address,
    });
    let connect = || async {
        match &tls_dial {
            Some((config, server_name)) => {
                connect_tls_stream(
                    &resolved,
                    context.bind_address,
                    context.socket_keepalive,
                    Arc::clone(config),
                    server_name.clone(),
                )
                .await
            }
            None => connect_stream(&resolved, context.bind_address, context.socket_keepalive).await,
        }
    };
    let mut stream = match (context.pool.as_ref(), pool_key.as_ref()) {
        (Some(spec), Some(key)) => match spec.pool.take(key) {
            Some(stream) => stream,
            None => timeout(context.connect_timeout, connect())
                .await
                .map_err(|_| ExchangeFailure::Timeout)?
                .map_err(|_| ExchangeFailure::Transport)?,
        },
        _ => timeout(context.connect_timeout, connect())
            .await
            .map_err(|_| ExchangeFailure::Timeout)?
            .map_err(|_| ExchangeFailure::Transport)?,
    };
    timeout(context.send_timeout, stream.write_all(&wire))
        .await
        .map_err(|_| ExchangeFailure::Timeout)?
        .map_err(|_| ExchangeFailure::Transport)?;
    timeout(context.send_timeout, stream.flush())
        .await
        .map_err(|_| ExchangeFailure::Timeout)?
        .map_err(|_| ExchangeFailure::Transport)?;

    let maximum_body = generation.app.config().limits.max_response_body_bytes;
    let mut parser = ResponseParser::new(context.protocol, context.limits);
    let mut head: Option<sdkwork_webserver_upstream_protocol::HttpResponseHead> = None;
    let mut body = context.buffer.buffer();
    let mut buffer = [0u8; 16 * 1024];
    // FastCGI responses are framed by `END_REQUEST`; the connection may be
    // pooled only after that record was parsed (a Content-Length break would
    // leave the END record unread and misalign the socket).
    let mut fastcgi_complete = false;
    loop {
        let read = timeout(context.read_timeout, stream.read(&mut buffer))
            .await
            .map_err(|_| ExchangeFailure::Timeout)?
            .map_err(|_| ExchangeFailure::Transport)?;
        if read == 0 {
            // `uwsgi`/`scgi` answers are raw HTTP bodies: EOF terminates a
            // response that declared no Content-Length; a truncated declared
            // body fails closed.
            parser.finish()?;
            match &head {
                Some(value) => {
                    if let Some(expected) = declared_body_length(value) {
                        if body.total() < expected {
                            return Err(ExchangeFailure::Protocol);
                        }
                    }
                    break;
                }
                None => return Err(ExchangeFailure::Protocol),
            }
        }
        if parser
            .feed(
                &buffer[..read],
                maximum_body,
                &mut head,
                &mut body,
                &context.catch_stderr,
            )
            .await?
        {
            fastcgi_complete = context.protocol == WireProtocol::FastCgi;
            break;
        }
        if context.protocol != WireProtocol::FastCgi {
            if let Some(value) = &head {
                if let Some(expected) = declared_body_length(value) {
                    if body.total() >= expected {
                        break;
                    }
                }
            }
        }
    }
    let Some(head) = head else {
        return Err(ExchangeFailure::Protocol);
    };
    // A complete, clean `END_REQUEST` with no trailing bytes means the socket is
    // aligned for the next request; only then is it returned to the pool. An app
    // that ignores the `keep_conn` flag closes the socket instead and the idle
    // health probe discards it on the next take. uWSGI/SCGI responses have no
    // protocol-level end marker (nginx never reuses those connections), so they
    // are never pooled.
    if context.protocol == WireProtocol::FastCgi && fastcgi_complete {
        if let (Some(spec), Some(key)) = (context.pool.as_ref(), pool_key) {
            spec.pool
                .put(key, stream, spec.per_key_max, spec.idle_timeout);
        }
    }
    let status = StatusCode::from_u16(head.status).map_err(|_| ExchangeFailure::Protocol)?;
    let mut headers = HeaderMap::new();
    for (name, value) in &head.headers {
        let lower = name.to_ascii_lowercase();
        if HOP_BY_HOP_RESPONSE_HEADERS.contains(&lower.as_str()) {
            continue;
        }
        let Ok(header_name) = header::HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(value) else {
            continue;
        };
        headers.append(header_name, header_value);
    }
    let body = body.finish().await.map_err(|error| {
        tracing::debug!(error = %error, "CGI upstream body could not be finalized");
        ExchangeFailure::Transport
    })?;
    Ok((status, headers, body))
}

/// Run one bounded exchange attempt.
///
/// With `*_ignore_client_abort off` (nginx's default) the attempt is awaited
/// inline: if the client connection aborts, hyper drops the handler future and
/// the upstream socket is closed with it. With `on`, the attempt runs on a
/// tokio task whose handle the handler only awaits, so a client abort detaches
/// the upstream exchange instead of cancelling it; the task is bounded by the
/// upstream connect/send/read timeouts and the retry deadline (a documented
/// owner, never an unbounded leak).
async fn exchange_attempt<F>(
    ignore_client_abort: bool,
    exchange: F,
) -> Result<ExchangeOutcome, ExchangeFailure>
where
    F: std::future::Future<Output = Result<ExchangeOutcome, ExchangeFailure>> + Send + 'static,
{
    if !ignore_client_abort {
        return exchange.await;
    }
    let detached = tokio::spawn(exchange);
    match detached.await {
        Ok(outcome) => outcome,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "detached CGI upstream exchange task failed"
            );
            Err(ExchangeFailure::Transport)
        }
    }
}

/// Stable pool identity for one upstream target: scheme/host/port/path from the
/// selected target URL, with any trailing slash removed so repeated selections
/// of the same target share their cached connections.
fn pool_target_key(target: &Url) -> String {
    target.as_str().trim_end_matches('/').to_owned()
}

/// TLS verification/SNI name for one CGI upstream target: the configured
/// `*_ssl_name` override, else the target host.
fn cgi_server_name_for(upstream: &UpstreamConfig, target: &Url) -> Option<ServerName<'static>> {
    let name = upstream
        .tls
        .as_ref()
        .and_then(|tls| tls.server_name.clone())
        .or_else(|| target.host_str().map(str::to_owned))?;
    ServerName::try_from(name).ok()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExchangeFailure {
    Transport,
    Timeout,
    Protocol,
    /// `fastcgi_catch_stderr` matched: nginx treats the response as having an
    /// invalid header and does not retry it (`invalid_header` is not part of
    /// the default `fastcgi_next_upstream error timeout` list).
    InvalidHeader,
}

/// Serve one `fastcgi_pass` location.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn serve_fastcgi(
    generation: &Arc<RuntimeGeneration>,
    upstream_ref: &str,
    params: &[UpstreamParamConfig],
    split_path_info: Option<&str>,
    index: Option<&str>,
    pass_request_headers: bool,
    keep_conn: bool,
    bind_address: Option<IpAddr>,
    socket_keepalive: bool,
    connect_timeout_ms: u64,
    send_timeout_ms: u64,
    read_timeout_ms: u64,
    response_policy: Option<&ProxyResponsePolicyConfig>,
    retry: Option<&UpstreamRetryConfig>,
    policy: &CgiResponsePolicy,
    facts: CgiRequestFacts<'_>,
    request_body: &[u8],
    metrics: &Arc<DataPlaneMetrics>,
) -> Response<Body> {
    let Some(upstream) = generation.upstreams.get(upstream_ref).cloned() else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FastCGI upstream is not configured\n",
        );
    };
    // `fastcgi_keep_conn on` sets the wire flag; reuse additionally needs the
    // upstream's idle cache (nginx requires `upstream { keepalive N; }` too).
    let (pool_per_key_max, pool_idle_timeout_ms) = generation
        .app
        .config()
        .upstreams
        .iter()
        .find(|configured| configured.id == upstream_ref)
        .map(|configured| {
            (
                configured.max_idle_connections,
                configured.idle_connection_timeout_ms,
            )
        })
        .unwrap_or((0, 0));
    let pool = keep_conn.then(|| PoolSpec {
        pool: Arc::clone(&generation.protocol_pool),
        per_key_max: pool_per_key_max,
        idle_timeout: Duration::from_millis(pool_idle_timeout_ms),
    });
    let (script_name, path_info) = compute_script_names(facts.path, split_path_info, index);
    let facts = CgiRequestFacts {
        script_name,
        path_info,
        ..facts
    };
    let Ok(parameters) = build_parameters(params, &facts, pass_request_headers) else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FastCGI parameters could not be encoded\n",
        );
    };
    let mut request_ids = RequestIdAllocator::new();
    let Ok(request_id) = request_ids.allocate() else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FastCGI request id space is exhausted\n",
        );
    };
    let Ok(wire) = encode_request(
        request_id,
        keep_conn,
        &parameters,
        if policy.pass_request_body {
            request_body
        } else {
            &[]
        },
        &policy.limits,
    ) else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FastCGI request could not be encoded\n",
        );
    };
    let Ok(_permit) = upstream.try_admit() else {
        return text_response(StatusCode::SERVICE_UNAVAILABLE, "upstream is busy\n");
    };
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
    let mut attempted = super::proxy::AttemptedTargets::default();
    let mut last_failure = ExchangeFailure::Transport;
    let context = ExchangeContext {
        generation: Arc::clone(generation),
        upstream_id: upstream_ref.to_owned(),
        address_policy: upstream_address_policy(generation, upstream_ref),
        metrics: Arc::clone(metrics),
        protocol: WireProtocol::FastCgi,
        connect_timeout: Duration::from_millis(connect_timeout_ms),
        send_timeout: Duration::from_millis(send_timeout_ms),
        read_timeout: Duration::from_millis(read_timeout_ms),
        bind_address,
        socket_keepalive,
        pool,
        buffer: policy.body_buffer(),
        limits: policy.limits,
        catch_stderr: Arc::clone(&policy.catch_stderr),
    };
    let wire = Arc::new(wire);
    for _ in 0..retry_enabled {
        if started.elapsed() >= total_deadline {
            break;
        }
        let Some(selected) = upstream.select_target_excluding_observed(
            &attempted,
            facts.client_ip,
            facts.path,
            Some(metrics),
        ) else {
            break;
        };
        let target = selected.url().clone();
        attempted.mark(selected.index());
        let outcome = exchange_attempt(
            policy.ignore_client_abort,
            exchange(context.clone(), target, Arc::clone(&wire)),
        )
        .await;
        match outcome {
            Ok((status, headers, mut body)) => {
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
                    last_failure = ExchangeFailure::Transport;
                    continue;
                }
                store_upstream_body(policy, &facts, status, &mut body).await;
                let (status, headers, body) =
                    apply_ranges(&facts, policy, status, headers, body).await;
                return build_response(
                    status,
                    headers,
                    body,
                    facts.method,
                    response_policy,
                    Some(policy),
                );
            }
            Err(failure) => {
                last_failure = failure;
                upstream.record_failure(selected);
                let retryable = match failure {
                    ExchangeFailure::Transport | ExchangeFailure::Protocol => {
                        conditions.contains(&UpstreamRetryCondition::TransportFailure)
                    }
                    ExchangeFailure::Timeout => {
                        conditions.contains(&UpstreamRetryCondition::Timeout)
                    }
                    // nginx does not retry an invalid header (a
                    // `fastcgi_catch_stderr` match) by default.
                    ExchangeFailure::InvalidHeader => false,
                };
                if !retryable {
                    break;
                }
            }
        }
    }
    match last_failure {
        ExchangeFailure::Timeout => {
            text_response(StatusCode::GATEWAY_TIMEOUT, "upstream timed out\n")
        }
        _ => text_response(StatusCode::BAD_GATEWAY, "upstream failed\n"),
    }
}

/// Serve one `uwsgi_pass` / `scgi_pass` location.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn serve_cgi(
    generation: &Arc<RuntimeGeneration>,
    protocol: CgiUpstreamProtocol,
    upstream_ref: &str,
    params: &[UpstreamParamConfig],
    pass_request_headers: bool,
    bind_address: Option<IpAddr>,
    socket_keepalive: bool,
    connect_timeout_ms: u64,
    send_timeout_ms: u64,
    read_timeout_ms: u64,
    response_policy: Option<&ProxyResponsePolicyConfig>,
    retry: Option<&UpstreamRetryConfig>,
    policy: &CgiResponsePolicy,
    modifier1: Option<u8>,
    modifier2: Option<u8>,
    extra_string: Option<&str>,
    facts: CgiRequestFacts<'_>,
    request_body: &[u8],
    metrics: &Arc<DataPlaneMetrics>,
) -> Response<Body> {
    let Some(upstream) = generation.upstreams.get(upstream_ref).cloned() else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CGI-family upstream is not configured\n",
        );
    };
    let Ok(parameters) = build_parameters(params, &facts, pass_request_headers) else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CGI-family parameters could not be encoded\n",
        );
    };
    let (wire_protocol, wire) = match protocol {
        CgiUpstreamProtocol::Uwsgi => {
            let variables = parameters.entries().to_vec();
            let extra = extra_string.unwrap_or("");
            match encode_uwsgi_request(
                &variables,
                extra.as_bytes(),
                modifier1.unwrap_or(0),
                modifier2.unwrap_or(0),
                &Limits::default(),
            ) {
                Ok(mut wire) => {
                    // The uWSGI request body follows the packet and is not part
                    // of `datasize` (nginx writes it after the packet).
                    wire.extend_from_slice(request_body);
                    (WireProtocol::Uwsgi, wire)
                }
                Err(_) => {
                    return text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "uwsgi request could not be encoded\n",
                    )
                }
            }
        }
        CgiUpstreamProtocol::Scgi => {
            // nginx always emits the mandatory `CONTENT_LENGTH` from the request
            // body length; a declared value is replaced, not trusted.
            let mut variables = parameters.entries().to_vec();
            variables.retain(|(name, _)| !name.eq_ignore_ascii_case("CONTENT_LENGTH"));
            variables.insert(
                0,
                ("CONTENT_LENGTH".to_owned(), request_body.len().to_string()),
            );
            match encode_scgi_request(&variables, request_body, &Limits::default()) {
                Ok(wire) => (WireProtocol::Scgi, wire),
                Err(_) => {
                    return text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "scgi request could not be encoded\n",
                    )
                }
            }
        }
    };
    let Ok(_permit) = upstream.try_admit() else {
        return text_response(StatusCode::SERVICE_UNAVAILABLE, "upstream is busy\n");
    };
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
    let mut attempted = super::proxy::AttemptedTargets::default();
    let mut last_failure = ExchangeFailure::Transport;
    let context = ExchangeContext {
        generation: Arc::clone(generation),
        upstream_id: upstream_ref.to_owned(),
        address_policy: upstream_address_policy(generation, upstream_ref),
        metrics: Arc::clone(metrics),
        protocol: wire_protocol,
        connect_timeout: Duration::from_millis(connect_timeout_ms),
        send_timeout: Duration::from_millis(send_timeout_ms),
        read_timeout: Duration::from_millis(read_timeout_ms),
        bind_address,
        socket_keepalive,
        pool: None,
        buffer: policy.body_buffer(),
        limits: policy.limits,
        catch_stderr: Arc::new(Vec::new()),
    };
    let wire = Arc::new(wire);
    for _ in 0..retry_enabled {
        if started.elapsed() >= total_deadline {
            break;
        }
        let Some(selected) = upstream.select_target_excluding_observed(
            &attempted,
            facts.client_ip,
            facts.path,
            Some(metrics),
        ) else {
            break;
        };
        let target = selected.url().clone();
        attempted.mark(selected.index());
        let outcome = exchange_attempt(
            policy.ignore_client_abort,
            exchange(context.clone(), target, Arc::clone(&wire)),
        )
        .await;
        match outcome {
            Ok((status, headers, mut body)) => {
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
                    last_failure = ExchangeFailure::Transport;
                    continue;
                }
                store_upstream_body(policy, &facts, status, &mut body).await;
                let (status, headers, body) =
                    apply_ranges(&facts, policy, status, headers, body).await;
                return build_response(
                    status,
                    headers,
                    body,
                    facts.method,
                    response_policy,
                    Some(policy),
                );
            }
            Err(failure) => {
                last_failure = failure;
                upstream.record_failure(selected);
                let retryable = match failure {
                    ExchangeFailure::Transport | ExchangeFailure::Protocol => {
                        conditions.contains(&UpstreamRetryCondition::TransportFailure)
                    }
                    ExchangeFailure::Timeout => {
                        conditions.contains(&UpstreamRetryCondition::Timeout)
                    }
                    ExchangeFailure::InvalidHeader => false,
                };
                if !retryable {
                    break;
                }
            }
        }
    }
    match last_failure {
        ExchangeFailure::Timeout => {
            text_response(StatusCode::GATEWAY_TIMEOUT, "upstream timed out\n")
        }
        _ => text_response(StatusCode::BAD_GATEWAY, "upstream failed\n"),
    }
}

/// Serve one `memcached_pass` location (bounded text-protocol GET/HEAD).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn serve_memcached(
    generation: &Arc<RuntimeGeneration>,
    upstream_ref: &str,
    key_template: &str,
    bind_address: Option<IpAddr>,
    socket_keepalive: bool,
    connect_timeout_ms: u64,
    send_timeout_ms: u64,
    read_timeout_ms: u64,
    retry: Option<&UpstreamRetryConfig>,
    policy: &CgiResponsePolicy,
    facts: CgiRequestFacts<'_>,
    facts_method: &Method,
    metrics: &Arc<DataPlaneMetrics>,
) -> Response<Body> {
    if *facts_method != Method::GET && *facts_method != Method::HEAD {
        let mut response = text_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n");
        response
            .headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
        return response;
    }
    let Some(upstream) = generation.upstreams.get(upstream_ref).cloned() else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "memcached upstream is not configured\n",
        );
    };
    let key = facts.expand(key_template);
    let Ok(wire) = encode_memcached_get(&key, &policy.limits) else {
        return text_response(StatusCode::BAD_REQUEST, "invalid memcached key\n");
    };
    let Ok(_permit) = upstream.try_admit() else {
        return text_response(StatusCode::SERVICE_UNAVAILABLE, "upstream is busy\n");
    };
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
    let mut attempted = super::proxy::AttemptedTargets::default();
    let mut last_failure = ExchangeFailure::Transport;
    for _ in 0..retry_enabled {
        if started.elapsed() >= total_deadline {
            break;
        }
        let Some(selected) = upstream.select_target_excluding_observed(
            &attempted,
            facts.client_ip,
            facts.path,
            Some(metrics),
        ) else {
            break;
        };
        let target = selected.url().clone();
        attempted.mark(selected.index());
        let Some(address_policy) = generation
            .app
            .config()
            .upstreams
            .iter()
            .find(|upstream| upstream.id == upstream_ref)
            .map(|upstream| upstream.address_policy.clone())
        else {
            break;
        };
        let outcome = memcached_exchange(
            generation,
            upstream_ref,
            &address_policy,
            metrics,
            &target,
            &key,
            &wire,
            Duration::from_millis(connect_timeout_ms),
            Duration::from_millis(send_timeout_ms),
            Duration::from_millis(read_timeout_ms),
            bind_address,
            socket_keepalive,
            &policy.limits,
        )
        .await;
        match outcome {
            Ok(MemcachedOutcome::Hit { value, flags }) => {
                upstream.record_success(selected);
                let content_type = generation
                    .app
                    .config()
                    .default_type
                    .clone()
                    .unwrap_or_else(|| "text/plain".to_owned());
                let mut headers = HeaderMap::new();
                if let Ok(value) = HeaderValue::from_str(&content_type) {
                    headers.insert(header::CONTENT_TYPE, value);
                }
                // nginx's memcached handler advertises byte ranges on a hit,
                // which makes the range filter honour client ranges.
                headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
                if let Some(flag) = policy.gzip_flag {
                    if flags & flag != 0 {
                        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
                    }
                }
                let (status, headers, body) = apply_ranges(
                    &facts,
                    policy,
                    StatusCode::OK,
                    headers,
                    BufferedResponseBody::Memory(value),
                )
                .await;
                let declared = body.len();
                let mut response = Response::new(if *facts_method == Method::HEAD {
                    // Dropping the buffered body also releases a spool file.
                    Body::empty()
                } else {
                    body.into_body(policy.limit_rate_bytes_per_second, policy.serve_chunk())
                });
                *response.status_mut() = status;
                *response.headers_mut() = headers;
                if let Ok(length) = HeaderValue::from_str(&declared.to_string()) {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_LENGTH, length);
                }
                return response;
            }
            Ok(MemcachedOutcome::Miss) => {
                upstream.record_success(selected);
                return text_response(StatusCode::NOT_FOUND, "memcached key was not found\n");
            }
            Err(failure) => {
                last_failure = failure;
                upstream.record_failure(selected);
                let retryable = match failure {
                    ExchangeFailure::Transport | ExchangeFailure::Protocol => {
                        conditions.contains(&UpstreamRetryCondition::TransportFailure)
                    }
                    ExchangeFailure::Timeout => {
                        conditions.contains(&UpstreamRetryCondition::Timeout)
                    }
                    ExchangeFailure::InvalidHeader => false,
                };
                if !retryable {
                    break;
                }
            }
        }
    }
    match last_failure {
        ExchangeFailure::Timeout => {
            text_response(StatusCode::GATEWAY_TIMEOUT, "upstream timed out\n")
        }
        _ => text_response(StatusCode::BAD_GATEWAY, "upstream failed\n"),
    }
}

/// Inputs for one CGI-family response policy.
pub(crate) struct CgiPolicyInputs<'a> {
    pub buffering: Option<&'a UpstreamBufferingConfig>,
    pub pass_request_body: bool,
    pub store: Option<&'a UpstreamStoreConfig>,
    pub store_root: Option<&'a str>,
    pub limit_rate_bytes_per_second: Option<u64>,
    pub force_ranges: bool,
    /// `memcached_gzip_flag` (memcached only).
    pub gzip_flag: Option<u32>,
    /// Executed `*_temp_path` / `*_max_temp_file_size` /
    /// `*_temp_file_write_size` policy (`None` = nginx defaults).
    pub temp: Option<&'a UpstreamTempPolicyConfig>,
    /// `[http] tempRoot` confinement root.
    pub temp_root: Option<&'a str>,
    /// The generation's bounded spool.
    pub spool: &'a Arc<TempSpool>,
    /// `*_ignore_client_abort on`.
    pub ignore_client_abort: bool,
    /// `fastcgi_catch_stderr` patterns.
    pub catch_stderr: &'a [String],
}

/// Wave 7D/14A response policy for one CGI-family/memcached resource: the
/// bounded buffering sizes, request-body forwarding, atomic `*_store` write,
/// `*_limit_rate` pacing, `*_force_ranges`/Accept-Ranges handling, the
/// temp-spool tier and the connection-control switches.
pub(crate) struct CgiResponsePolicy {
    pub limits: Limits,
    pub pass_request_body: bool,
    pub store: Option<UpstreamStoreConfig>,
    pub store_root: Option<String>,
    pub limit_rate_bytes_per_second: Option<u64>,
    pub force_ranges: bool,
    /// `memcached_gzip_flag`: `Content-Encoding` is derived from the flags
    /// field of a memcached hit when any of these bits is set.
    pub gzip_flag: Option<u32>,
    /// Resolved temp-spool tier (nginx defaults when undeclared).
    buffer: BodyBufferPolicy,
    /// `*_ignore_client_abort on`.
    pub ignore_client_abort: bool,
    /// `fastcgi_catch_stderr` patterns (empty for uwsgi/scgi/memcached).
    pub catch_stderr: Arc<Vec<String>>,
}

impl Default for CgiResponsePolicy {
    fn default() -> Self {
        // Unit-test default: no spool file is ever opened because a zero
        // in-memory bound is impossible and the spool is only reached past the
        // configured buffer bank; the Arc keeps the constructor infallible.
        let spool = Arc::new(TempSpool::new(None, 0, 0, 1));
        Self {
            limits: Limits::default(),
            pass_request_body: true,
            store: None,
            store_root: None,
            limit_rate_bytes_per_second: None,
            force_ranges: false,
            gzip_flag: None,
            buffer: BodyBufferPolicy {
                spool,
                spool_dir: None,
                memory_bound: usize::MAX,
                write_chunk: DEFAULT_WRITE_CHUNK,
                max_file_bytes: 0,
            },
            ignore_client_abort: false,
            catch_stderr: Arc::new(Vec::new()),
        }
    }
}

impl CgiResponsePolicy {
    /// Build the policy for one resource.
    pub(crate) fn new(inputs: CgiPolicyInputs<'_>) -> Self {
        let mut limits = Limits::default();
        let (header_bytes, chunk_bytes, chunks) = inputs
            .buffering
            .map(|policy| (policy.header_bytes, policy.chunk_bytes, policy.chunks))
            .unwrap_or((4096, 4096, 8));
        if let Some(policy) = inputs.buffering {
            // `*_buffer_size` is nginx's maximum upstream header block; the
            // runtime applies it to the response-header parser. `*_buffers`
            // sizes the response read chunk (never below the protocol's
            // built-in protocol frame bounds).
            if let Ok(header_bytes) = usize::try_from(policy.header_bytes) {
                limits.max_header_block_bytes = header_bytes;
            }
            if let Ok(chunk_bytes) = usize::try_from(policy.chunk_bytes) {
                limits.max_stdin_chunk_bytes = chunk_bytes;
                limits.max_header_line_bytes =
                    limits.max_header_line_bytes.min(chunk_bytes.max(512));
            }
        }
        // nginx's response buffer bank: the body stays in memory until the
        // configured buffers (`*_buffer_size` + `*_buffers`) are exhausted.
        let buffer_bank =
            header_bytes.saturating_add(chunk_bytes.saturating_mul(u64::from(chunks)));
        let buffer_max = header_bytes.max(chunk_bytes);
        let (max_file_bytes, write_chunk_bytes, spool_dir) = match inputs.temp {
            Some(policy) => (
                policy.max_file_bytes,
                policy.write_chunk_bytes,
                resolve_spool_dir(policy.path.as_deref(), inputs.temp_root),
            ),
            // nginx defaults: 1 GiB per file, `2 * size` writes.
            None => (1024 * 1024 * 1024, buffer_max.saturating_mul(2), None),
        };
        Self {
            limits,
            pass_request_body: inputs.pass_request_body,
            store: inputs.store.cloned(),
            store_root: inputs.store_root.map(str::to_owned),
            limit_rate_bytes_per_second: inputs.limit_rate_bytes_per_second,
            force_ranges: inputs.force_ranges,
            gzip_flag: inputs.gzip_flag,
            buffer: BodyBufferPolicy {
                spool: Arc::clone(inputs.spool),
                spool_dir,
                memory_bound: usize::try_from(buffer_bank).unwrap_or(usize::MAX),
                write_chunk: usize::try_from(write_chunk_bytes.max(512))
                    .unwrap_or(DEFAULT_WRITE_CHUNK),
                max_file_bytes,
            },
            ignore_client_abort: inputs.ignore_client_abort,
            catch_stderr: Arc::new(inputs.catch_stderr.to_vec()),
        }
    }

    /// The response-body sink for one exchange.
    fn body_buffer(&self) -> BodyBufferPolicy {
        self.buffer.clone()
    }

    /// Chunk size used when spooled bodies are served back to the client.
    fn serve_chunk(&self) -> usize {
        self.buffer.write_chunk
    }
}

/// Resolve an executed `*_temp_path` against the `[http] tempRoot` confinement
/// root (defense in depth: semantic validation already proved the relation).
///
/// A path that is not absolute, contains traversal/line breaks, or escapes the
/// configured root is rejected and the per-generation directory is used
/// instead, so a spool file can never be created outside the operator root.
fn resolve_spool_dir(path: Option<&str>, temp_root: Option<&str>) -> Option<PathBuf> {
    let path = path?;
    let unsafe_path = !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
        || path.split('/').any(|component| component == "..");
    if unsafe_path {
        tracing::warn!(
            path,
            "configured CGI temp path is not a safe absolute path; using the per-generation spool directory"
        );
        return None;
    }
    if let Some(root) = temp_root {
        let root = root.trim_end_matches('/');
        let candidate = path.trim_end_matches('/');
        if !(candidate == root || candidate.starts_with(&format!("{root}/"))) {
            tracing::warn!(
                path,
                root,
                "configured CGI temp path escapes nginx.tempRoot; using the per-generation spool directory"
            );
            return None;
        }
    }
    Some(PathBuf::from(path))
}

/// Expand an nginx `*_store` path template with the values the materializer
/// proved executable, then apply the confinement rules again at write time.
fn expand_store_path(
    template: &str,
    facts: &CgiRequestFacts<'_>,
    store_root: Option<&str>,
) -> Option<std::path::PathBuf> {
    let document_root = facts.document_root.unwrap_or("");
    let mut expanded = String::with_capacity(template.len() + facts.path.len());
    let mut rest = template;
    while let Some(index) = rest.find('$') {
        expanded.push_str(&rest[..index]);
        let name: String = rest[index + 1..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        let replacement = match name.as_str() {
            "uri" => facts.path,
            "request_uri" => facts.raw_request_uri,
            "document_root" => document_root,
            "fastcgi_script_name" | "uwsgi_script_name" | "scgi_script_name" => {
                facts.script_name.as_str()
            }
            _ => return None,
        };
        expanded.push_str(replacement);
        rest = &rest[index + 1 + name.len()..];
    }
    expanded.push_str(rest);
    if !expanded.starts_with('/')
        || expanded
            .bytes()
            .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
        || expanded.split('/').any(|component| component == "..")
    {
        return None;
    }
    if let Some(root) = store_root {
        let root = root.trim_end_matches('/');
        if !(expanded == root || expanded.starts_with(&format!("{root}/"))) {
            return None;
        }
    }
    Some(std::path::PathBuf::from(expanded))
}

/// Atomically write the upstream response body to the configured `*_store`
/// path (bounded temp file in the destination directory + rename), preserving
/// the client response. Only `200 OK` responses are stored, matching nginx.
/// A spooled body is stream-copied, never materialized in memory again.
async fn store_upstream_body(
    policy: &CgiResponsePolicy,
    facts: &CgiRequestFacts<'_>,
    status: StatusCode,
    body: &mut BufferedResponseBody,
) {
    let Some(store) = policy.store.as_ref() else {
        return;
    };
    if status != StatusCode::OK || body.len() == 0 {
        return;
    }
    let Some(path) = expand_store_path(&store.path, facts, policy.store_root.as_deref()) else {
        tracing::debug!(
            template = %store.path,
            "CGI store path template could not be expanded safely"
        );
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(error) = tokio::fs::create_dir_all(parent).await {
        tracing::debug!(path = %path.display(), error = %error, "CGI store directory could not be created");
        return;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "stored".to_owned());
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp = parent.join(format!(".{file_name}.{unique}.tmp"));
    let mut temp_file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
    {
        Ok(file) => file,
        Err(error) => {
            tracing::debug!(path = %temp.display(), error = %error, "CGI store temp file could not be created");
            return;
        }
    };
    let written = body.copy_to(&mut temp_file).await;
    if let Err(error) = written {
        tracing::debug!(path = %temp.display(), error = %error, "CGI store temp file could not be written");
        drop(temp_file);
        let _ = tokio::fs::remove_file(&temp).await;
        return;
    }
    if let Err(error) = temp_file.flush().await {
        tracing::debug!(path = %temp.display(), error = %error, "CGI store temp file could not be flushed");
        drop(temp_file);
        let _ = tokio::fs::remove_file(&temp).await;
        return;
    }
    drop(temp_file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = store.access_mode & 0o7777;
        if let Err(error) =
            tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(mode)).await
        {
            tracing::debug!(path = %temp.display(), error = %error, "CGI store mode could not be applied");
        }
    }
    if let Err(error) = tokio::fs::rename(&temp, &path).await {
        tracing::debug!(path = %path.display(), error = %error, "CGI store rename failed");
        let _ = tokio::fs::remove_file(&temp).await;
    }
}

/// One parsed byte range, `end` exclusive.
#[derive(Clone, Copy)]
struct ByteRange {
    start: u64,
    end: u64,
}

/// Parse an nginx-compatible `Range: bytes=…` header against `total`.
///
/// Returns `None` when the request must be answered with the full
/// representation (non-`bytes` unit, malformed header, `If-Range` mismatch or a
/// zero-length representation); `Some(Err(()))` when nginx answers 416.
fn parse_byte_ranges(
    headers: &HeaderMap,
    total: u64,
    if_range: Option<&HeaderValue>,
) -> Option<Result<Vec<ByteRange>, ()>> {
    if total == 0 {
        return None;
    }
    if let Some(if_range) = if_range {
        let matches = |name: header::HeaderName| {
            headers
                .get(name)
                .map(|value| value.as_bytes() == if_range.as_bytes())
                .unwrap_or(false)
        };
        let date_matches = headers
            .get(header::LAST_MODIFIED)
            .map(|value| value.as_bytes() == if_range.as_bytes())
            .unwrap_or(false);
        if !(matches(header::ETAG) || date_matches) {
            return None;
        }
    }
    let value = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())?;
    let list = value.strip_prefix("bytes=")?;
    let mut ranges = Vec::new();
    for entry in list.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (start_text, end_text) = entry.split_once('-')?;
        let (start, end) = if start_text.trim().is_empty() {
            let Ok(suffix) = end_text.trim().parse::<u64>() else {
                return Some(Err(()));
            };
            if suffix == 0 {
                return Some(Err(()));
            }
            (total.saturating_sub(suffix), total)
        } else {
            let Ok(start) = start_text.trim().parse::<u64>() else {
                return Some(Err(()));
            };
            let end = if end_text.trim().is_empty() {
                total
            } else {
                let Ok(end) = end_text.trim().parse::<u64>() else {
                    return Some(Err(()));
                };
                end.saturating_add(1).min(total)
            };
            (start, end)
        };
        if start >= total {
            continue;
        }
        if start < end {
            ranges.push(ByteRange { start, end });
        }
    }
    if ranges.is_empty() {
        return Some(Err(()));
    }
    if ranges.iter().any(|range| range.end > total) {
        return Some(Err(()));
    }
    Some(Ok(ranges))
}

/// Apply nginx range semantics to a buffered CGI-family response.
///
/// Ranges are honoured when the resource sets `*_force_ranges on` or when the
/// upstream response itself declared `Accept-Ranges: bytes` (nginx's
/// `r->allow_ranges`); a response with unknown length never gets ranges. A
/// single range of a spooled body is streamed from disk (only the requested
/// slice is read), so ranges do not re-materialize an oversized body.
async fn apply_ranges(
    facts: &CgiRequestFacts<'_>,
    policy: &CgiResponsePolicy,
    status: StatusCode,
    mut headers: HeaderMap,
    body: BufferedResponseBody,
) -> (StatusCode, HeaderMap, BufferedResponseBody) {
    if status != StatusCode::OK || facts.method != Method::GET {
        return (status, headers, body);
    }
    let upstream_allows = headers
        .get(header::ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("bytes"));
    if !policy.force_ranges && !upstream_allows {
        return (status, headers, body);
    }
    let if_range = facts.headers.get(header::IF_RANGE);
    let total = body.len();
    let Some(parsed) = parse_byte_ranges(facts.headers, total, if_range) else {
        return (status, headers, body);
    };
    let ranges = match parsed {
        Ok(ranges) => ranges,
        Err(()) => {
            headers.remove(header::CONTENT_LENGTH);
            headers.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{total}"))
                    .unwrap_or_else(|_| HeaderValue::from_static("bytes */*0")),
            );
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                headers,
                BufferedResponseBody::Memory(Vec::new()),
            );
        }
    };
    headers.remove(header::CONTENT_LENGTH);
    if ranges.len() > 1 {
        // nginx's multipart range path requires the whole representation in a
        // single buffer; the CGI-family upstream path declines a multi-range
        // request and answers the full representation with `Accept-Ranges`
        // (differential-confirmed against nginx 1.29.6), so the bounded adapter
        // mirrors that instead of inventing a multipart body.
        headers
            .entry(header::ACCEPT_RANGES)
            .or_insert_with(|| HeaderValue::from_static("bytes"));
        return (StatusCode::OK, headers, body);
    }
    if ranges.len() == 1 {
        let range = ranges[0];
        let slice = match body.slice(range.start, range.end).await {
            Ok(slice) => slice,
            Err(body) => {
                // A range that cannot be served within the body bounds
                // declines to the full representation (the same bounded
                // behaviour as a multi-range request).
                headers
                    .entry(header::ACCEPT_RANGES)
                    .or_insert_with(|| HeaderValue::from_static("bytes"));
                return (StatusCode::OK, headers, body);
            }
        };
        headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!(
                "bytes {}-{}/{}",
                range.start,
                range.end - 1,
                total
            ))
            .unwrap_or_else(|_| HeaderValue::from_static("bytes 0-0/0")),
        );
        return (StatusCode::PARTIAL_CONTENT, headers, slice);
    }
    (StatusCode::OK, headers, body)
}

/// Why `memcached_exchange` is written separately from the CGI-family exchange:
/// the memcached text protocol has no header block, only a terminator.
enum MemcachedOutcome {
    Hit { value: Vec<u8>, flags: u32 },
    Miss,
}

#[allow(clippy::too_many_arguments)]
async fn memcached_exchange(
    generation: &RuntimeGeneration,
    upstream_id: &str,
    policy: &UpstreamAddressPolicyConfig,
    metrics: &Arc<DataPlaneMetrics>,
    target: &Url,
    requested_key: &str,
    wire: &[u8],
    connect_timeout: Duration,
    send_timeout: Duration,
    read_timeout: Duration,
    bind_address: Option<IpAddr>,
    socket_keepalive: bool,
    limits: &Limits,
) -> Result<MemcachedOutcome, ExchangeFailure> {
    let resolved = resolve_stream_target(generation, policy, metrics, target)
        .await
        .map_err(|_| ExchangeFailure::Transport)?;
    let mut stream = timeout(
        connect_timeout,
        connect_stream(&resolved, bind_address, socket_keepalive),
    )
    .await
    .map_err(|_| ExchangeFailure::Timeout)?
    .map_err(|_| ExchangeFailure::Transport)?;
    timeout(send_timeout, stream.write_all(wire))
        .await
        .map_err(|_| ExchangeFailure::Timeout)?
        .map_err(|_| ExchangeFailure::Transport)?;
    timeout(send_timeout, stream.flush())
        .await
        .map_err(|_| ExchangeFailure::Timeout)?
        .map_err(|_| ExchangeFailure::Transport)?;
    let mut parser = MemcachedResponseParser::new(*limits);
    let mut outcome = None;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = timeout(read_timeout, stream.read(&mut buffer))
            .await
            .map_err(|_| ExchangeFailure::Timeout)?
            .map_err(|_| ExchangeFailure::Transport)?;
        if read == 0 {
            return Err(ExchangeFailure::Protocol);
        }
        for event in parser
            .feed(&buffer[..read])
            .map_err(|_| ExchangeFailure::Protocol)?
        {
            match event {
                MemcachedResponseEvent::Value(value) => {
                    if value.key != requested_key {
                        return Err(ExchangeFailure::Protocol);
                    }
                    outcome = Some(MemcachedOutcome::Hit {
                        value: value.data,
                        flags: value.flags,
                    });
                }
                MemcachedResponseEvent::Miss => outcome = Some(MemcachedOutcome::Miss),
                MemcachedResponseEvent::Stored | MemcachedResponseEvent::NotStored => {
                    return Err(ExchangeFailure::Protocol)
                }
            }
        }
        if parser.is_complete() {
            break;
        }
    }
    let _ = upstream_id;
    outcome.ok_or(ExchangeFailure::Protocol)
}

fn build_response(
    status: StatusCode,
    mut headers: HeaderMap,
    body: BufferedResponseBody,
    method: &Method,
    response_policy: Option<&ProxyResponsePolicyConfig>,
    pacing: Option<&CgiResponsePolicy>,
) -> Response<Body> {
    // FastCGI default-hidden fields plus the resource's hide/pass policy.
    let mut hidden: Vec<String> = DEFAULT_HIDDEN_RESPONSE_HEADERS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    if let Some(policy) = response_policy {
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
    let remove: Vec<header::HeaderName> = headers
        .keys()
        .filter(|name| {
            hidden
                .iter()
                .any(|candidate| name.as_str().eq_ignore_ascii_case(candidate))
        })
        .cloned()
        .collect();
    for name in remove {
        headers.remove(name);
    }
    let upstream_length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    headers.remove(header::CONTENT_LENGTH);
    headers.remove(header::TRANSFER_ENCODING);

    if body.is_spooled() {
        tracing::debug!(
            bytes = body.len(),
            "serving a CGI upstream response from the temp spool"
        );
    }
    let bodyless = method == Method::HEAD
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::NOT_MODIFIED;
    let declared_length = if bodyless {
        // nginx keeps the upstream `Content-Length` on a HEAD response (the
        // body is discarded but the representation length is still declared).
        if method == Method::HEAD {
            upstream_length.and_then(|length| length.parse::<u64>().ok())
        } else {
            None
        }
    } else {
        Some(body.len())
    };
    let mut response = Response::new(if bodyless {
        // Dropping the buffered body here also removes a spooled temp file.
        Body::empty()
    } else {
        match pacing {
            Some(policy) => {
                body.into_body(policy.limit_rate_bytes_per_second, policy.serve_chunk())
            }
            None => body.into_body(None, DEFAULT_WRITE_CHUNK),
        }
    });
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    if let Some(length) = declared_length {
        response.headers_mut().insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&length.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::HOST;

    fn facts<'a>(
        headers: &'a HeaderMap,
        path: &'a str,
        query: Option<&'a str>,
        maps: &'a MapResults,
    ) -> CgiRequestFacts<'a> {
        CgiRequestFacts {
            method: &Method::GET,
            scheme: "http",
            authority: "example.com",
            path,
            raw_request_uri: "/index.php/path?q=1",
            query,
            client_ip: "127.0.0.1".parse().expect("ip"),
            listener_port: 8080,
            server_protocol: "HTTP/1.1",
            server_name: "example.com",
            headers,
            document_root: Some("/srv"),
            script_name: path.to_owned(),
            path_info: String::new(),
            maps,
        }
    }

    #[test]
    fn script_name_split_and_index_follow_nginx_order() {
        let (script, path_info) = compute_script_names(
            "/app.php/extra/path",
            Some(r"^(/.+\.php)(/.*)$"),
            Some("index.php"),
        );
        assert_eq!(script, "/app.php");
        assert_eq!(path_info, "/extra/path");

        let (script, path_info) = compute_script_names("/app.php/extra", None, Some("index.php"));
        assert_eq!(script, "/app.php/extra");
        assert_eq!(path_info, "");

        let (script, _) = compute_script_names("/dir/", None, Some("index.php"));
        assert_eq!(script, "/dir/index.php");

        // A non-matching split keeps the full URI (nginx fallback).
        let (script, path_info) =
            compute_script_names("/plain/path", Some(r"^(/.+\.php)(/.*)$"), None);
        assert_eq!(script, "/plain/path");
        assert_eq!(path_info, "");
    }

    #[test]
    fn parameter_expansion_supports_the_nginx_vocabulary() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("example.com:8080"));
        headers.append("x-tag", HeaderValue::from_static("one"));
        headers.append("x-tag", HeaderValue::from_static("two"));
        headers.append("cookie", HeaderValue::from_static("a=1; b=2"));
        let maps = MapResults::default();
        let facts = facts(&headers, "/index.php", Some("q=1"), &maps);
        let declared = vec![
            UpstreamParamConfig {
                name: "SCRIPT_FILENAME".to_owned(),
                value: "$document_root$fastcgi_script_name".to_owned(),
                if_not_empty: false,
            },
            UpstreamParamConfig {
                name: "QUERY_STRING".to_owned(),
                value: "$query_string".to_owned(),
                if_not_empty: false,
            },
            UpstreamParamConfig {
                name: "HTTP_HOST".to_owned(),
                value: "declared".to_owned(),
                if_not_empty: false,
            },
            UpstreamParamConfig {
                name: "EMPTY".to_owned(),
                value: "$arg_missing".to_owned(),
                if_not_empty: true,
            },
        ];
        let params = build_parameters(&declared, &facts, true).expect("params");
        let lookup = |name: &str| {
            params
                .entries()
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(lookup("SCRIPT_FILENAME"), Some("/srv/index.php"));
        assert_eq!(lookup("QUERY_STRING"), Some("q=1"));
        assert_eq!(lookup("HTTP_HOST"), Some("declared"));
        assert_eq!(lookup("EMPTY"), None);
        // Auto HTTP_ parameters: joined duplicates and the nginx HTTP_HOST rule.
        assert_eq!(lookup("HTTP_X_TAG"), Some("one, two"));
        assert_eq!(lookup("HTTP_COOKIE"), Some("a=1; b=2"));
    }

    #[test]
    fn auto_host_parameter_uses_normalized_host_and_request_port() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("Example.COM:8080"));
        let maps = MapResults::default();
        let mut facts = facts(&headers, "/", None, &maps);
        facts.authority = "Example.COM:8080";
        let params = build_parameters(&[], &facts, true).expect("params");
        let host = params
            .entries()
            .iter()
            .find(|(name, _)| name == "HTTP_HOST")
            .map(|(_, value)| value.as_str());
        assert_eq!(host, Some("example.com:8080"));
    }

    #[test]
    fn catch_stderr_search_matches_the_trimmed_nginx_message() {
        let patterns = vec!["PHP Fatal error:  ".to_owned(), "BOOM".to_owned()];
        // A substring match anywhere in the message is invalid (nginx's
        // `ngx_strnstr` over the trimmed message).
        assert!(matches!(
            record_stderr(b"PHP Fatal error:  boom\n", &patterns),
            Err(ExchangeFailure::InvalidHeader)
        ));
        assert!(matches!(
            record_stderr(b"prefix BOOM suffix\r\n", &patterns),
            Err(ExchangeFailure::InvalidHeader)
        ));
        // The trailing `LF`/`CR`/`.`/space suffix is trimmed before the search,
        // exactly like `ngx_http_fastcgi_process_header`, so a pattern ending
        // in a space does not match a message that only ends with the pattern.
        assert!(matches!(
            record_stderr(b"PHP Fatal error:  \n", &patterns),
            Ok(())
        ));
        // A whitespace-only stderr record is a no-op.
        assert!(matches!(record_stderr(b" \r\n. ", &patterns), Ok(())));
        assert!(matches!(
            record_stderr(b"nothing to see\n", &patterns),
            Ok(())
        ));
    }

    #[test]
    fn trim_stderr_matches_nginx_suffix_rules() {
        assert_eq!(trim_stderr(b"error\n"), b"error");
        assert_eq!(trim_stderr(b"error.\r\n "), b"error");
        assert_eq!(trim_stderr(b""), b"");
        assert_eq!(trim_stderr(b"\n\r. "), b"");
        // Interior characters are preserved.
        assert_eq!(trim_stderr(b"a.b c\n"), b"a.b c");
    }

    #[test]
    fn temp_path_resolution_confines_to_the_operator_root() {
        assert_eq!(
            resolve_spool_dir(Some("/var/cache/nginx/fastcgi"), None),
            Some(PathBuf::from("/var/cache/nginx/fastcgi"))
        );
        assert_eq!(
            resolve_spool_dir(Some("/var/cache/nginx/fastcgi"), Some("/var/cache/nginx")),
            Some(PathBuf::from("/var/cache/nginx/fastcgi"))
        );
        // The root itself is accepted (nginx allows the temp path to be the
        // configured directory).
        assert_eq!(
            resolve_spool_dir(Some("/var/cache/nginx/"), Some("/var/cache/nginx")),
            Some(PathBuf::from("/var/cache/nginx/"))
        );
        // Escapes, traversal and relative paths use the generation directory.
        assert_eq!(
            resolve_spool_dir(Some("/etc/nginx-spool"), Some("/var/cache/nginx")),
            None
        );
        assert_eq!(
            resolve_spool_dir(Some("/var/cache/nginx/../etc"), Some("/var/cache/nginx")),
            None
        );
        assert_eq!(resolve_spool_dir(Some("relative/spool"), None), None);
        assert_eq!(resolve_spool_dir(None, Some("/var/cache/nginx")), None);
    }

    /// `*_ignore_client_abort off` (nginx default) must cancel the in-flight
    /// upstream exchange when the handler future is dropped (a client abort),
    /// while `on` must keep it running to completion.
    #[tokio::test]
    async fn ignore_client_abort_decouples_the_upstream_exchange() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        for (ignore_client_abort, expected_completion) in [(false, 0), (true, 1)] {
            let completed = Arc::new(AtomicUsize::new(0));
            let flag = Arc::clone(&completed);
            let upstream = async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                flag.fetch_add(1, Ordering::SeqCst);
                Ok((
                    StatusCode::OK,
                    HeaderMap::new(),
                    BufferedResponseBody::Memory(b"ok".to_vec()),
                ))
            };
            let runner =
                tokio::spawn(async move { exchange_attempt(ignore_client_abort, upstream).await });
            // Simulate the client abort: hyper drops the handler future.
            tokio::time::sleep(Duration::from_millis(30)).await;
            runner.abort();
            let _ = runner.await;
            tokio::time::sleep(Duration::from_millis(250)).await;
            assert_eq!(
                completed.load(Ordering::SeqCst),
                expected_completion,
                "ignore_client_abort={ignore_client_abort} must                  {expected_completion:+} complete the upstream exchange"
            );
        }
    }

    #[test]
    fn pass_request_headers_off_suppresses_auto_parameters() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("example.com"));
        let maps = MapResults::default();
        let facts = facts(&headers, "/", None, &maps);
        let params = build_parameters(&[], &facts, false).expect("params");
        assert!(params.is_empty());
    }
}
