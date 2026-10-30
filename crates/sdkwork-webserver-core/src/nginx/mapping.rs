//! Materialize parsed nginx `http`- and `stream`-context configuration into
//! the runtime `WebServerAppConfig` model.
//!
//! Supported directives (everything else fails closed with a diagnostic):
//!
//! - `upstream { server <addr> [weight=…] [max_fails=…] [fail_timeout=…]; keepalive …; }`
//! - `server { listen …; server_name …; ssl_certificate(_key) …; location …; }`
//!   where `server_name` accepts the exact, `*.suffix`, `prefix.*`, `.suffix`,
//!   and `~regex` nginx forms (host selection follows nginx precedence; see
//!   `crate::config::server_name`)
//! - `location <match> { proxy_pass http(s)://<upstream|host:port>[/uri]; … }`
//!   including variable `proxy_pass` targets
//! - `location <match> { return <code> <url-with-$host/$request_uri/$scheme>; }`
//! - `location <match> { root <absolute>; try_files $uri $uri/ /index.html; }`
//! - `location <match> { try_files $uri … @fallback; }` with a named location
//!   (`location @fallback { proxy_pass | return | root | alias | try_files }`)
//!   as the one-hop internal redirect target
//! - `location ^~ /.well-known/acme-challenge/ { root <webroot>; }` — the
//!   reserved ACME HTTP-01 namespace. It declares the listener's
//!   `acmeHttp01.webroot` instead of an ordinary static route, so the nginx
//!   artifact and the JSON artifact describe the same narrow-precedence
//!   challenge endpoint.
//! - `location <match> { alias <absolute-dir>; }` (nginx prefix replacement)
//! - `location` `rewrite`, `allow`/`deny`, `limit_req`, `limit_conn`,
//!   `auth_basic` + `auth_basic_user_file` (htpasswd loaded at materialize),
//!   `sub_filter`, `gzip`, `expires`/`etag`/`if_modified_since`
//! - http `limit_req_zone`, `limit_conn_zone`, `gzip` / `gzip_types` /
//!   `gzip_min_length`
//! - http `proxy_cache_path` / `proxy_cache` / `proxy_cache_valid`
//! - http/server/location `proxy_set_header` (validated supported `$vars`)
//! - `stream { upstream …; server { listen; proxy_pass; ssl_preread; … } }`
//!
//! Safe nginx tuning directives are accepted and ignored (the runtime owns
//! timeouts, buffering, and TLS defaults). Directives the runtime cannot
//! execute in an active context are rejected with a source-located diagnostic.

use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};
use thiserror::Error;

use crate::config::{
    cache_policy::{parse_expires_arguments, parse_if_modified_since_token},
    format_proxy_set_header_entry_with_maps, hostname_upstream_allowed_cidrs,
    map_variables::{map_result_name, validate_map_definitions, validate_map_source},
    parse_htpasswd, parse_limit_conn, parse_limit_conn_zone, parse_limit_req, parse_limit_req_zone,
    AutoindexConfig, AutoindexFormat, CachePolicyConfig, CharsetConfig, ConfigDiagnostic,
    CookieRewritePolicyConfig, CustomHeaderConfig, ExpiresMode, GeoConfig, GeoEntryConfig,
    GzipProxiedConfig, GzipStaticMode, IfModifiedSinceMode, MapConfig, MapEntryConfig,
    MimeTypeConfig, ProxyRedirectPolicyConfig, ProxyResponsePolicyConfig, ProxyRewriteRuleConfig,
    RedirectPolicyConfig, RefererConfig, RefererHostEntry, SplitClientsConfig,
    SplitClientsPartConfig, StreamSetVariableConfig, StreamTargetConfig, StreamTlsMode, TlsVersion,
    UpstreamRetryCondition, UpstreamRetryConfig, WebServerAppConfig, WebServerConfigError,
    ACME_HTTP_01_CHALLENGE_PREFIX,
};

use super::parser::NginxDirective;

const ACCEPTED_IGNORED: &[&str] = &[
    "user",
    "worker_processes",
    "worker_rlimit_nofile",
    "worker_connections",
    "pid",
    "error_log",
    "access_log",
    "sendfile",
    "tcp_nopush",
    "tcp_nodelay",
    "keepalive_timeout",
    "keepalive_requests",
    "server_tokens",
    "log_format",
    "events",
    "so_keepalive",
    "resolver",
    "resolver_timeout",
    "client_body_timeout",
    "client_header_timeout",
    "client_header_buffer_size",
    "large_client_header_buffers",
    "reset_timedout_connection",
    "server_names_hash_max_size",
    "proxy_http_version",
    "proxy_buffering",
    "proxy_request_buffering",
    "proxy_next_upstream",
    "proxy_connect_timeout",
    "proxy_read_timeout",
    "proxy_send_timeout",
    "proxy_buffer_size",
    "proxy_buffers",
    "ssl_prefer_server_ciphers",
    "ssl_session_cache",
    "ssl_session_timeout",
    "ssl_session_tickets",
    "ssl_stapling",
    "ssl_stapling_verify",
    "ssl_trusted_certificate",
    "ssl_ciphers",
    "ssl_verify_depth",
    "ssl_dhparam",
    "ssl_ecdh_curve",
    "http2",
    "http2_max_field_size",
    "http2_max_header_size",
    "http2_max_requests",
    "http2_idle_timeout",
    "http2_recv_timeout",
    "http2_max_concurrent_pushes",
    "http2_push",
    "http2_push_preload",
    "http2_recv_buffer_size",
    "http2_body_preread_size",
    "http2_pool_size",
    "http2_streams_index_size",
    "keepalive",
    "client_body_buffer_size",
    "send_timeout",
    "sendfile_max_chunk",
    "open_file_cache",
    "open_file_cache_valid",
    "open_file_cache_min_uses",
    "limit_conn_log_level",
    "limit_req_log_level",
    "log_not_found",
    // Stock nginx.conf / distro hash-table and buffering tuning; the runtime
    // owns its hash tables and I/O buffering.
    "server_names_hash_bucket_size",
    "types_hash_max_size",
    "types_hash_bucket_size",
    "variables_hash_max_size",
    "variables_hash_bucket_size",
    "map_hash_max_size",
    "map_hash_bucket_size",
    "proxy_headers_hash_max_size",
    "proxy_headers_hash_bucket_size",
    "gzip_http_version",
    "gzip_comp_level",
    "gzip_buffers",
    "gzip_window",
    "charset_map",
    "keepalive_disable",
    "keepalive_time",
    "lingering_time",
    "lingering_timeout",
    "lingering_close",
    "connection_pool_size",
    "request_pool_size",
    "output_buffers",
    "postpone_output",
    "read_ahead",
    "send_lowat",
    "directio",
    "directio_alignment",
    "log_subrequest",
    "msie_padding",
    "msie_refresh",
    "chunked_transfer_encoding",
    "proxy_temp_path",
    "proxy_max_temp_file_size",
    "proxy_temp_file_write_size",
    "proxy_send_lowat",
    "ssl_conf_command",
    "proxy_pass_header",
    "accept_mutex_delay",
    "ssl_buffer_size",
    "proxy_ssl_protocols",
    "proxy_ssl_ciphers",
    "proxy_ssl_session_reuse",
    "proxy_ssl_verify_depth",
    // Main-context process/OS knobs nginx accepts at the top of nginx.conf;
    // the runtime owns process management, so they are safe to accept.
    "daemon",
    "master_process",
    "env",
    "pcre_jit",
    "ssl_engine",
    "timer_resolution",
    "lock_file",
    "worker_priority",
    "worker_cpu_affinity",
    "worker_shutdown_timeout",
    "worker_aio_requests",
    "worker_rlimit_core",
    "working_directory",
    "epoll_events",
    // Response-behavior knobs the runtime owns via its own defaults (error
    // page mapping, directory autoindex). Accepted and ignored like the safe
    // tuning directives above; the conformance corpus
    // (config-source-fixtures/nginx/full-nginx.conf) exercises `autoindex on`
    // as part of the accepted surface, and the TOML spec treats autoindex as
    // an operator policy knob (§11.2).
    //
    // `expires` / `etag` / `if_modified_since` are NOT here: they compile to
    // `CachePolicyConfig` on the route or virtual host so the data plane can
    // emit the freshness headers instead of dropping the operator's policy.
    // events / OS tuning
    "use",
    "accept_mutex",
    "multi_accept",
    "disable_symlinks",
];

const UNSUPPORTED_SECURITY: &[&str] = &[];

/// One explicit compatibility diagnostic per imported directive the runtime
/// accepts but does not enforce.
///
/// PRD §3 and §9 require that no unsupported nginx directive is *silently*
/// accepted. The migration importer keeps accepting the tuning/behaviour
/// directives an operator's stock configuration contains, but every such
/// directive now surfaces a diagnostic with its source location and the reason
/// its value has no effect, so a migration review can decide whether the
/// equivalent SDKWork setting must be configured explicitly.
/// Directives that the pinned nginx 1.29.6 reference build itself accepts as
/// **obsolete no-ops** (`ngx_http_v2_obsolete`). nginx logs a deprecation
/// warning and ignores the value, so accepting them without effect is exact
/// nginx parity, not a runtime gap.
const NGINX_OBSOLETE_NOOP: &[&str] = &[
    "http2_max_field_size",
    "http2_max_header_size",
    "http2_max_requests",
    "http2_idle_timeout",
    "http2_recv_timeout",
    "http2_max_concurrent_pushes",
    "http2_push",
    "http2_push_preload",
];

/// TLS directives whose only effect is OCSP stapling. The pinned rustls TLS
/// stack exposes no stapling/OCSP fetch surface, so the value is accepted but
/// has no effect — the diagnostic states that explicitly instead of implying an
/// equivalent runtime setting exists.
const NGINX_OCSP_DYNAMIC_UNSUPPORTED: &[&str] = &["ssl_stapling", "ssl_stapling_verify"];

/// `ssl_trusted_certificate` is consumed as an extra PEM chain only.
const NGINX_TRUSTED_CERTIFICATE_CHAIN_ONLY: &[&str] = &["ssl_trusted_certificate"];

/// Static-file descriptor/metadata cache. The runtime opens static paths per
/// request through `cap-std` and owns no cross-request descriptor cache; nginx's
/// only observable contract here is a staleness window, which the runtime will
/// not fake.
const OPEN_FILE_CACHE_FAMILY: &[&str] = &[
    "open_file_cache",
    "open_file_cache_valid",
    "open_file_cache_min_uses",
    "open_file_cache_errors",
];

/// Response framing switches the pinned hyper HTTP/1.1 encoder cannot express:
/// hyper only emits a close-delimited response body for HTTP/1.0 responses, so
/// an HTTP/1.1 response with an unknown body length always uses chunked framing.
const HYPER_FRAMING_LIMITED: &[&str] = &["chunked_transfer_encoding"];

/// Directives that tune nginx's own buffers/index tables. The runtime owns its
/// buffer and hash-table sizing, so the value is accepted-unenforced.
const RUNTIME_OWNED_MEMORY_TUNING: &[&str] = &[
    "http2_recv_buffer_size",
    "http2_body_preread_size",
    "http2_pool_size",
    "http2_streams_index_size",
];

pub(crate) fn ignored_directive_diagnostics(
    directives: &[NginxDirective],
) -> Vec<ConfigDiagnostic> {
    fn push(out: &mut Vec<ConfigDiagnostic>, directive: &NginxDirective) {
        let message = if NGINX_OBSOLETE_NOOP.contains(&directive.name.as_str()) {
            format!(
                "`{}` is obsolete in the pinned nginx 1.29.6 reference build (nginx accepts it as a deprecated no-op and ignores the value); the runtime likewise executes nothing for it",
                directive.name
            )
        } else if NGINX_OCSP_DYNAMIC_UNSUPPORTED.contains(&directive.name.as_str()) {
            format!(
                "`{}` itself has no effect: dynamic OCSP fetching/verification is not implemented. The runtime serves only a static `ssl_stapling_file` DER response per certificate (loaded when TLS material is built; refresh requires a reload)",
                directive.name
            )
        } else if NGINX_TRUSTED_CERTIFICATE_CHAIN_ONLY.contains(&directive.name.as_str()) {
            format!(
                "`{}` is consumed only as an extra PEM chain for the served certificate (`chainFile`); OCSP verification is not implemented",
                directive.name
            )
        } else if OPEN_FILE_CACHE_FAMILY.contains(&directive.name.as_str()) {
            format!(
                "`{}` has no effect: it configures nginx's static-file descriptor/metadata cache, whose only observable contract is a staleness window; the runtime opens static paths per request through cap-std and does not fake cached-fd staleness",
                directive.name
            )
        } else if HYPER_FRAMING_LIMITED.contains(&directive.name.as_str()) {
            format!(
                "`{}` cannot be honored by the pinned hyper HTTP/1.1 encoder: hyper emits a close-delimited body only for HTTP/1.0 responses, so an HTTP/1.1 response with an unknown body length always uses `Transfer-Encoding: chunked`; the value is accepted but has no effect",
                directive.name
            )
        } else if RUNTIME_OWNED_MEMORY_TUNING.contains(&directive.name.as_str()) {
            format!(
                "`{}` is accepted for nginx compatibility; the runtime owns its buffer and hash-table sizing, so the imported value has no effect",
                directive.name
            )
        } else {
            format!(
                "`{}` is accepted for nginx compatibility but is not enforced by the SDKWork runtime; the imported value has no effect and the equivalent runtime setting must be configured explicitly",
                directive.name
            )
        };
        out.push(ConfigDiagnostic::new(
            format!("{}:{}", directive.source.display(), directive.line),
            message,
        ));
    }
    fn walk(directives: &[NginxDirective], out: &mut Vec<ConfigDiagnostic>, in_stream: bool) {
        for directive in directives {
            // `map` entries and `types` rows are data, not directives: the
            // first token of a map entry is an authored key, so a key that
            // happens to spell an ignored directive name must not be reported
            // as an accepted-but-unenforced directive.
            if matches!(directive.name.as_str(), "map" | "types") {
                // Both `map` planes are executed (HTTP map engine / bounded
                // stream variable engine); `types` rows are data, not
                // directives. Neither is an accepted-unenforced diagnostic.
                let _ = in_stream;
                continue;
            }
            if ACCEPTED_IGNORED.contains(&directive.name.as_str()) {
                push(out, directive);
            }
            walk(
                &directive.children,
                out,
                in_stream || directive.name == "stream",
            );
        }
    }
    let mut out = Vec::new();
    walk(directives, &mut out, false);
    out
}

#[derive(Debug, Error)]
pub enum NginxConfigError {
    #[error("{path}:{line}: {message}")]
    Unsupported {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error(transparent)]
    Config(#[from] WebServerConfigError),

    #[error("Web Server config failed validation: {0}")]
    ValidationFailed(String),
}

impl NginxConfigError {
    fn unsupported(directive: &NginxDirective, message: impl std::fmt::Display) -> Self {
        Self::Unsupported {
            path: directive.source.clone(),
            line: directive.line,
            message: message.to_string(),
        }
    }
}

impl From<NginxConfigError> for WebServerConfigError {
    fn from(error: NginxConfigError) -> Self {
        match error {
            NginxConfigError::Unsupported {
                path,
                line,
                message,
            } => WebServerConfigError::Nginx {
                diagnostic: ConfigDiagnostic::new(format!("{}:{line}", path.display()), message),
            },
            NginxConfigError::Config(error) => error,
            NginxConfigError::ValidationFailed(message) => {
                WebServerConfigError::Materialize(message)
            }
        }
    }
}

/// Materialize parsed nginx directives (either an `http` block's children or
/// a directory of `sites-enabled`-style `*.conf` files) into the runtime
/// model. `base_dir` anchors relative certificate and root paths.
/// Operator-provided compatibility settings for the nginx plane.
///
/// These are typed-plane settings that a stock nginx file cannot express (the
/// nginx grammar has no equivalent directive), so the operator supplies them
/// out of band. The same values exist in the typed configuration model
/// (`nginx.documentRootAllowlist`); the nginx plane must receive them from the
/// process environment.
#[derive(Debug, Clone, Default)]
pub struct NginxCompatOptions {
    /// Absolute directory prefixes a request-time `root $variable;` /
    /// `alias $variable;` may resolve to. Empty means variable document roots
    /// fail closed at materialization.
    pub document_root_allowlist: Vec<String>,
}

/// Materialize with default (empty) operator compatibility settings.
pub fn materialize_nginx_app_default(
    directives: &[NginxDirective],
    base_dir: &Path,
    app_key: &str,
) -> Result<WebServerAppConfig, NginxConfigError> {
    materialize_nginx_app(
        directives,
        base_dir,
        app_key,
        &NginxCompatOptions::default(),
    )
}

/// Materialize with explicit operator compatibility settings.
pub fn materialize_nginx_app(
    directives: &[NginxDirective],
    base_dir: &Path,
    app_key: &str,
    options: &NginxCompatOptions,
) -> Result<WebServerAppConfig, NginxConfigError> {
    let http_directives = extract_http_context(directives)?;
    let stream_directives = extract_stream_context(directives)?;
    let mail_directives = extract_mail_context(directives)?;
    let mut mapper = Mapper::new(app_key, base_dir);
    mapper.document_root_allowlist = options.document_root_allowlist.clone();
    // nginx `map` declarations register their result variables at
    // configuration init, so a value template may reference one declared later
    // in `http {}` (differential-confirmed against nginx 1.29.6). Collect them
    // before any header/return value is validated.
    for directive in &http_directives {
        if directive.name == "map" {
            let map = parse_map(directive)?;
            if let Some(result) = map_result_name(&map) {
                // nginx map variables are changeable: a later declaration of
                // the same result variable replaces the earlier one instead of
                // failing the whole configuration.
                if mapper.map_results.iter().any(|existing| existing == result) {
                    mapper
                        .maps
                        .retain(|existing| map_result_name(existing) != Some(result));
                } else {
                    mapper.map_results.push(result.to_owned());
                }
            }
            mapper.maps.push(map);
        }
    }
    // nginx `geo` / `split_clients` also register variables at configuration
    // init, so a header template may reference them before/after declaration.
    for directive in &http_directives {
        // Deliberately `if` chains rather than a `match`: the directive-matrix
        // gate derives the http-context execute set from the match arms in this
        // function, and a preliminary catch-all match would hide them.
        if directive.name == "geo" {
            let geo = parse_geo(directive)?;
            if let Some(result) = geo.result.strip_prefix('$') {
                if mapper.map_results.iter().any(|name| name == result) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                            "geo result variable `${result}` is already declared by another variable block; the runtime rejects duplicate variable definitions"
                        ),
                    ));
                }
                mapper.map_results.push(result.to_owned());
            }
            mapper.geo.push(geo);
        } else if directive.name == "split_clients" {
            let split = parse_split_clients(directive)?;
            if let Some(result) = split.result.strip_prefix('$') {
                if mapper.map_results.iter().any(|name| name == result) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                            "split_clients result variable `${result}` is already declared by another variable block; the runtime rejects duplicate variable definitions"
                        ),
                    ));
                }
                mapper.map_results.push(result.to_owned());
            }
            mapper.split_clients.push(split);
        }
    }
    // nginx `add_header` inherits from the http context into every server that
    // declares none. Directive order inside `http {}` is irrelevant to that
    // merge, so collect the http-level array before materializing any server.
    for directive in &http_directives {
        if directive.name == "add_header" {
            let map_variables = mapper.map_variable_refs();
            mapper
                .http_add_headers
                .push(parse_add_header(directive, &map_variables)?);
        }
    }
    for (server_index, directive) in http_directives.iter().enumerate() {
        match directive.name.as_str() {
            "upstream" => mapper.materialize_upstream(directive)?,
            "server" => mapper.materialize_server(directive, server_index)?,
            "limit_req_zone" => mapper.materialize_limit_req_zone(directive)?,
            "limit_conn_zone" => mapper.materialize_limit_conn_zone(directive)?,
            "types" => mapper.materialize_types(directive)?,
            "default_type" => mapper.materialize_default_type(directive)?,
            "ssl_stapling_file" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "http-level `ssl_stapling_file` is not supported; declare it in the `server` block that declares `ssl_certificate` so it applies to a concrete certificate",
                ))
            }
            "gzip" => mapper.materialize_gzip(directive)?,
            "gzip_types" => mapper.materialize_gzip_types(directive)?,
            "gzip_min_length" => mapper.materialize_gzip_min_length(directive)?,
            "gzip_vary" => mapper.materialize_gzip_vary(directive)?,
            "gzip_static" => mapper.materialize_gzip_static(directive)?,
            "gzip_disable" => {
                let (patterns, msie6) = parse_gzip_disable(directive)?;
                mapper.http_gzip_refinements.disable = patterns;
                mapper.http_gzip_refinements.disable_msie6 = msie6;
            }
            "gzip_proxied" => {
                mapper.http_gzip_refinements.proxied = Some(parse_gzip_proxied(directive)?);
            }
            "proxy_cache_path" => mapper.materialize_proxy_cache_path(directive)?,
            "proxy_cache_valid" => mapper.apply_proxy_cache_valid(directive)?,
            "proxy_cache" => mapper.materialize_proxy_cache(directive)?,
            // Collected by the order-independent pre-scan above.
            "map" | "geo" | "split_clients" => {}
            "fastcgi_pass" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`fastcgi_pass` is only allowed inside a `location` block",
                ));
            }
            name if name.starts_with("fastcgi_") => {
                let map_results = mapper.map_results.clone();
                let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                mapper.http_fastcgi.apply(directive, &map_variables)?;
            }
            "uwsgi_pass" | "scgi_pass" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{}` is only allowed inside a `location` block",
                        directive.name
                    ),
                ));
            }
            name if name.starts_with("uwsgi_") || name.starts_with("scgi_") => {
                let Some(kind) = CgiKind::from_directive(name) else {
                    unreachable!();
                };
                let map_results = mapper.map_results.clone();
                let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                mapper.http_cgi[kind.index()].apply(kind, directive, &map_variables)?;
            }
            "memcached_pass" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`memcached_pass` is only allowed inside a `location` block",
                ));
            }
            name if name.starts_with("memcached_") => {
                mapper.http_memcached.apply(directive)?;
            }
            "grpc_pass" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`grpc_pass` is only allowed inside a `location` block",
                ));
            }
            name if name.starts_with("grpc_ssl_") => {
                mapper.http_grpc_ssl.apply(directive)?;
            }
            name if name.starts_with("grpc_") => {
                let map_results = mapper.map_results.clone();
                let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                mapper.http_grpc.apply(directive, &map_variables)?;
            }
            "proxy_set_header" => {
                let map_variables = mapper.map_variable_refs();
                let entry =
                    format_proxy_set_header_entry_with_maps(&directive.args, &map_variables)
                        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
                mapper.http_proxy_set_headers.push(entry);
            }
            "client_max_body_size" => {
                mapper.note_client_max_body_size(parse_body_size(directive)?);
            }
            "error_page" => {
                mapper.http_error_pages.push(parse_error_page(directive)?);
            }
            // Collected by the order-independent pre-scan above.
            "add_header" => {}
            "recursive_error_pages" => {
                mapper.http_recursive_error_pages =
                    parse_on_off(directive, "recursive_error_pages")?;
            }
            "proxy_intercept_errors" => {
                mapper.http_proxy_intercept_errors =
                    parse_on_off(directive, "proxy_intercept_errors")?;
            }
            "proxy_ssl_verify"
            | "proxy_ssl_trusted_certificate"
            | "proxy_ssl_certificate"
            | "proxy_ssl_certificate_key"
            | "proxy_ssl_server_name" => {
                parse_proxy_ssl_directive(&mut mapper.http_proxy_ssl, directive)?;
            }
            "http2" => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "http2 requires on or off",
                    ));
                };
                match value.as_str() {
                    "on" => mapper.http_http2 = Some(true),
                    "off" => mapper.http_http2 = Some(false),
                    other => {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            format!("http2 accepts on|off, found `{other}`"),
                        ));
                    }
                }
            }
            "proxy_ssl_name" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`proxy_ssl_name` (custom upstream SNI name) is not supported; the runtime sends the upstream hostname as SNI",
                ));
            }
            // nginx HTTP/2 ingress tuning. `http2_max_concurrent_streams` maps
            // onto the runtime ingress profile (the value hyper/h2 publishes
            // as SETTINGS_MAX_CONCURRENT_STREAMS). `http2_chunk_size` controls
            // DATA-frame chunking, which the pinned hyper/h2 ingress does not
            // expose to callers, so it fails closed instead of being faked.
            "http2_max_concurrent_streams" => {
                mapper.note_http2_max_concurrent_streams(parse_http2_max_concurrent_streams(
                    directive,
                )?);
            }
            "http2_chunk_size" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`http2_chunk_size` is not exposed by the pinned hyper/h2 ingress (it controls DATA-frame chunking); remove it or declare `http2_max_concurrent_streams` instead",
                ));
            }
            "proxy_method" => {
                mapper.http_proxy_method = Some(parse_proxy_method(directive)?);
            }
            "ssl_protocols" => {
                mapper.http_ssl_protocols = Some(parse_ssl_protocols(directive)?);
            }
            // nginx `expires` / `etag` / `if_modified_since` are each
            // inherited down the http → server → location chain.
            "expires" => {
                mapper.http_cache.expires = Some(parse_expires_directive(directive)?);
            }
            "etag" => {
                mapper.http_cache.etag = Some(parse_on_off(directive, "etag")?);
            }
            "if_modified_since" => {
                mapper.http_cache.if_modified_since = Some(parse_if_modified_since(directive)?);
            }
            "set_real_ip_from" | "real_ip_header" | "real_ip_recursive" => {
                parse_real_ip_directive(&mut mapper.http_real_ip, directive)?;
            }
            // nginx redirect construction policy (default: absolute Location,
            // listener port included off the scheme default, request Host as
            // the authority). Inherited http → server → location.
            "absolute_redirect" => {
                mapper.http_redirect_policy.absolute_redirect =
                    Some(parse_on_off(directive, "absolute_redirect")?);
            }
            "port_in_redirect" => {
                mapper.http_redirect_policy.port_in_redirect =
                    Some(parse_on_off(directive, "port_in_redirect")?);
            }
            "server_name_in_redirect" => {
                mapper.http_redirect_policy.server_name_in_redirect =
                    Some(parse_on_off(directive, "server_name_in_redirect")?);
            }
            "limit_req_status" => {
                mapper.http_limit_req_status = Some(parse_limit_status(directive)?);
            }
            "limit_conn_status" => {
                mapper.http_limit_conn_status = Some(parse_limit_status(directive)?);
            }
            "max_ranges" => {
                mapper.http_max_ranges = Some(parse_max_ranges(directive)?);
            }
            // nginx proxy response-header policy. Each directive family
            // inherits http → server → location with nginx's replace-on-
            // declare rule; the effective policy is attached to every proxy
            // resource.
            "proxy_hide_header" => mapper
                .http_proxy_response
                .hide_headers
                .get_or_insert_with(Vec::new)
                .push(parse_proxy_header_name(directive)?),
            "proxy_pass_header" => mapper
                .http_proxy_response
                .pass_headers
                .get_or_insert_with(Vec::new)
                .push(parse_proxy_header_name(directive)?),
            "proxy_redirect" => {
                apply_proxy_redirect_directive(&mut mapper.http_proxy_response, directive)?;
            }
            "proxy_cookie_path" => apply_cookie_rewrite_directive(
                directive,
                &mut mapper.http_proxy_response.cookie_path,
                false,
            )?,
            "proxy_cookie_domain" => apply_cookie_rewrite_directive(
                directive,
                &mut mapper.http_proxy_response.cookie_domain,
                true,
            )?,
            // nginx request-boundary policies (http/server context only).
            "merge_slashes" => {
                mapper.http_merge_slashes = Some(parse_on_off(directive, "merge_slashes")?);
            }
            "underscores_in_headers" => {
                mapper.http_underscores_in_headers =
                    Some(parse_on_off(directive, "underscores_in_headers")?);
            }
            "ignore_invalid_headers" => {
                mapper.http_ignore_invalid_headers =
                    Some(parse_on_off(directive, "ignore_invalid_headers")?);
            }
            // nginx charset family: header-level behaviour is executed; the
            // `charset_map` table itself stays accepted-unenforced (byte
            // transcoding is out of scope).
            "charset" => {
                mapper.http_charset.charset = Some(parse_charset_value(directive, true)?);
            }
            "charset_types" => {
                mapper.http_charset.types = Some(parse_charset_types(directive)?);
            }
            "source_charset" => {
                mapper.http_charset.source_charset = Some(parse_charset_value(directive, true)?);
            }
            "override_charset" => {
                mapper.http_charset.override_charset =
                    Some(parse_on_off(directive, "override_charset")?);
            }
            // nginx autoindex family.
            "autoindex" => {
                mapper.http_autoindex.enabled = Some(parse_on_off(directive, "autoindex")?);
            }
            "autoindex_exact_size" => {
                mapper.http_autoindex.exact_size =
                    Some(parse_on_off(directive, "autoindex_exact_size")?);
            }
            "autoindex_localtime" => {
                mapper.http_autoindex.localtime =
                    Some(parse_on_off(directive, "autoindex_localtime")?);
            }
            "autoindex_format" => {
                mapper.http_autoindex.format = Some(parse_autoindex_format(directive)?);
            }
            "stream" | "events" | "http" | "mail" => {}
            name if ACCEPTED_IGNORED.contains(&name) => {}
            name if UNSUPPORTED_SECURITY.contains(&name) => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("`{name}` cannot be enforced by the SDKWork runtime; remove it or serve this surface through stock nginx"),
                ));
            }
            name => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("unsupported directive `{name}`"),
                ));
            }
        }
    }
    mapper.materialize_stream_context(&stream_directives)?;
    mapper.materialize_mail_context(&mail_directives)?;
    if mapper.virtual_hosts.is_empty()
        && mapper.streams.is_empty()
        && mapper.mail_servers.is_empty()
    {
        return Err(NginxConfigError::unsupported(
            &NginxDirective {
                name: "server".to_owned(),
                args: Vec::new(),
                children: Vec::new(),
                line: 0,
                source: base_dir.to_path_buf(),
            },
            "no `server`, `stream`, or `mail` blocks were materialized",
        ));
    }
    mapper.finish()
}

/// When the input is a full `nginx.conf`, unwrap the `http { }` block; site
/// directories are already http-context content.
fn extract_http_context(
    directives: &[NginxDirective],
) -> Result<Vec<NginxDirective>, NginxConfigError> {
    let http_blocks = directives
        .iter()
        .filter(|directive| directive.name == "http")
        .collect::<Vec<_>>();
    if http_blocks.is_empty() {
        return Ok(directives.to_vec());
    }
    if http_blocks.len() > 1 {
        return Err(NginxConfigError::unsupported(
            http_blocks[1],
            "multiple `http` blocks are not supported",
        ));
    }
    // Fail closed on top-level directives outside `http {}` that the
    // runtime does not consume (load_module, ...): silently dropping them
    // would hide configuration that is not executed. `stream {}` and `mail {}`
    // are extracted by their own context wrappers.
    for directive in directives {
        if directive.name == "http" || directive.name == "stream" || directive.name == "mail" {
            continue;
        }
        if ACCEPTED_IGNORED.contains(&directive.name.as_str()) {
            continue;
        }
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "top-level directive `{}` outside `http {{}}` is not supported",
                directive.name
            ),
        ));
    }
    Ok(http_blocks[0].children.clone())
}

/// When the input is a full `nginx.conf`, unwrap the `mail { }` block.
/// Site directories (`sites-enabled`) have no mail wrapper, so the list is
/// empty.
fn extract_mail_context(
    directives: &[NginxDirective],
) -> Result<Vec<NginxDirective>, NginxConfigError> {
    let mail_blocks = directives
        .iter()
        .filter(|directive| directive.name == "mail")
        .collect::<Vec<_>>();
    if mail_blocks.is_empty() {
        return Ok(Vec::new());
    }
    if mail_blocks.len() > 1 {
        return Err(NginxConfigError::unsupported(
            mail_blocks[1],
            "multiple `mail` blocks are not supported",
        ));
    }
    Ok(mail_blocks[0].children.clone())
}

/// When the input is a full `nginx.conf`, unwrap the `stream { }` block.
/// Site directories (`sites-enabled`) have no stream wrapper, so the list is
/// empty.
fn extract_stream_context(
    directives: &[NginxDirective],
) -> Result<Vec<NginxDirective>, NginxConfigError> {
    let stream_blocks = directives
        .iter()
        .filter(|directive| directive.name == "stream")
        .collect::<Vec<_>>();
    if stream_blocks.is_empty() {
        return Ok(Vec::new());
    }
    if stream_blocks.len() > 1 {
        return Err(NginxConfigError::unsupported(
            stream_blocks[1],
            "multiple `stream` blocks are not supported",
        ));
    }
    Ok(stream_blocks[0].children.clone())
}

#[derive(Clone, Default)]
struct LocationExtras {
    rewrite: Vec<Value>,
    access: Vec<Value>,
    limit_req: Vec<Value>,
    limit_conn: Vec<Value>,
    auth_basic: Option<Value>,
    sub_filter: Option<Value>,
    secure_link: Option<Value>,
    gzip: Option<bool>,
    gzip_types: Option<Vec<String>>,
    gzip_min_length: Option<u64>,
    gzip_vary: Option<bool>,
    gzip_static: Option<GzipStaticMode>,
    /// Location-level `types` block (replaces the inherited table).
    mime_types: Option<Vec<MimeTypeConfig>>,
    /// Location-level `default_type`.
    default_type: Option<String>,
    error_pages: Vec<Value>,
    /// Location-level `add_header` entries; a non-empty list replaces the
    /// inherited host policy (nginx `add_header` inheritance).
    response_headers: Vec<Value>,
    /// Location-level `expires` / `etag` / `if_modified_since` declarations;
    /// the route builder overlays them on the resolved host policy.
    cache: CacheDirectives,
    /// Location-level redirect construction declarations.
    redirect_policy: RedirectPolicyDeclarations,
    /// Location-level nginx `limit_req_status`.
    limit_req_status: Option<u16>,
    /// Location-level nginx `limit_conn_status`.
    limit_conn_status: Option<u16>,
    /// Location-level nginx `max_ranges`.
    max_ranges: Option<u64>,
    /// nginx `internal`: reachable only through an internal redirect.
    internal: bool,
    /// Location-level proxy response-header directives.
    proxy_response: ProxyResponseDirectives,
    /// Location-level nginx charset-family declarations.
    charset: CharsetDeclarations,
    /// Location-level nginx gzip refinement declarations.
    gzip_refinements: GzipRefinementDeclarations,
    /// Location-level nginx autoindex-family declarations.
    autoindex: AutoindexDeclarations,
    /// nginx `empty_gif` location handler.
    empty_gif: bool,
    /// nginx `stub_status` location handler.
    stub_status: bool,
    /// Location-level nginx `valid_referers` policy (replaces the server one).
    referer: Option<RefererConfig>,
    /// Ordered location-level `set` / `if` rewrite-phase instructions.
    rewrite_phase: Vec<Value>,
    /// Effective location `client_max_body_size` (inherited when undeclared).
    max_request_body_bytes: Option<u64>,
}

/// Resolved nginx `ssl_protocols` policy as an inclusive version window.
///
/// nginx stores the directive as a bitmask, but the runtime TLS stack
/// (`rustls`) offers only TLS 1.2 and 1.3, so the declaration is read as a set
/// and intersected with that supported window. The intersection can only
/// *narrow* what a client may negotiate, so an honored declaration is never
/// less secure than the operator asked for, and a declaration that keeps
/// neither 1.2 nor 1.3 is refused instead of silently reverting to the broad
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SslProtocolRange {
    minimum: TlsVersion,
    maximum: TlsVersion,
}

impl Default for SslProtocolRange {
    fn default() -> Self {
        Self {
            minimum: TlsVersion::Tls12,
            maximum: TlsVersion::Tls13,
        }
    }
}

/// Every token nginx accepts in `ssl_protocols`, oldest first, mapped to the
/// runtime version that can serve it. `None` marks a protocol the rustls core
/// cannot enable (TLS 1.0/1.1); declaring one stays legal because narrowing is
/// the only safe reading, but it contributes no protocol to the window.
const NGINX_TLS_PROTOCOLS: &[(&str, Option<TlsVersion>)] = &[
    ("TLSv1", None),
    ("TLSv1.1", None),
    ("TLSv1.2", Some(TlsVersion::Tls12)),
    ("TLSv1.3", Some(TlsVersion::Tls13)),
];

fn parse_ssl_protocols(directive: &NginxDirective) -> Result<SslProtocolRange, NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "ssl_protocols requires at least one protocol token",
        ));
    }
    let mut minimum: Option<TlsVersion> = None;
    let mut maximum: Option<TlsVersion> = None;
    for token in &directive.args {
        let Some((_, supported)) = NGINX_TLS_PROTOCOLS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
        else {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("ssl_protocols accepts TLSv1|TLSv1.1|TLSv1.2|TLSv1.3, found `{token}`"),
            ));
        };
        if let Some(version) = supported {
            minimum = Some(minimum.map_or(*version, |current| current.min(*version)));
            maximum = Some(maximum.map_or(*version, |current| current.max(*version)));
        }
    }
    let (Some(minimum), Some(maximum)) = (minimum, maximum) else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "ssl_protocols `{}` enables only TLS 1.0/1.1, which the rustls-based runtime cannot serve; declare TLSv1.2 and/or TLSv1.3",
                directive.args.join(" ")
            ),
        ));
    };
    Ok(SslProtocolRange { minimum, maximum })
}

/// nginx `expires` / `etag` / `if_modified_since` declarations seen at one
/// configuration level.
///
/// Each of the three directives owns an independent nginx inheritance chain
/// (http → server → location, replace-on-declare), so they are carried as
/// three separate `Option`s until the effective triple is assembled. A
/// location that declares only `etag off` therefore keeps the server's
/// `expires 1d` instead of silently resetting it (which is what a single
/// combined `Option<CachePolicyConfig>` would do).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CacheDirectives {
    expires: Option<(ExpiresMode, i64)>,
    etag: Option<bool>,
    if_modified_since: Option<IfModifiedSinceMode>,
}

impl CacheDirectives {
    /// Overlay `inner` (the more specific level) on `self`, the way nginx
    /// merges http → server → location: a directive declared at the inner
    /// level replaces the inherited value; an undeclared directive keeps it.
    fn overlay(self, inner: Self) -> Self {
        Self {
            expires: inner.expires.or(self.expires),
            etag: inner.etag.or(self.etag),
            if_modified_since: inner.if_modified_since.or(self.if_modified_since),
        }
    }

    fn is_empty(self) -> bool {
        self.expires.is_none() && self.etag.is_none() && self.if_modified_since.is_none()
    }

    /// Materialize the effective triple. `None` means *no* level declared any
    /// of the three directives, so the route or host keeps the
    /// deployment-level defaults instead of pinning nginx defaults.
    fn resolve(self) -> Option<CachePolicyConfig> {
        if self.is_empty() {
            return None;
        }
        let (expires, expires_seconds) = self.expires.unwrap_or((ExpiresMode::Off, 0));
        Some(CachePolicyConfig {
            expires,
            expires_seconds,
            etag: self.etag.unwrap_or(true),
            if_modified_since: self.if_modified_since.unwrap_or(IfModifiedSinceMode::Exact),
        })
    }
}

/// Parse one `expires` directive into its mode and signed seconds argument.
///
/// The grammar itself lives in `config::cache_policy`, shared with the typed
/// TOML plane (`expires = "1d"`), so the two configuration surfaces cannot
/// drift into accepting different subsets of the nginx syntax
/// (NGINX_SPEC.md §3 response behavior; SDKWORK_WEBSERVER_SPEC.md §11.2).
fn parse_expires_directive(
    directive: &NginxDirective,
) -> Result<(ExpiresMode, i64), NginxConfigError> {
    parse_expires_arguments(&directive.args)
        .map_err(|detail| NginxConfigError::unsupported(directive, detail))
}

/// `CachePolicyConfig` as it appears in the materialized JSON plane. Kept in
/// step with the model's `camelCase`/`kebab-case` serde names by
/// `cache_policy_json_matches_the_model_wire_form`.
fn cache_policy_json(policy: &CachePolicyConfig) -> Value {
    json!({
        "expires": policy.expires.wire(),
        "expiresSeconds": policy.expires_seconds,
        "etag": policy.etag,
        "ifModifiedSince": policy.if_modified_since.wire(),
    })
}

fn parse_if_modified_since(
    directive: &NginxDirective,
) -> Result<IfModifiedSinceMode, NginxConfigError> {
    match directive.args.first().map(String::as_str) {
        Some(token) => parse_if_modified_since_token(token).ok_or_else(|| {
            NginxConfigError::unsupported(
                directive,
                format!("if_modified_since accepts off|exact|before, found `{token}`"),
            )
        }),
        None => Err(NginxConfigError::unsupported(
            directive,
            "if_modified_since requires off|exact|before",
        )),
    }
}

/// nginx redirect construction declarations at one configuration level
/// (`absolute_redirect` / `port_in_redirect` / `server_name_in_redirect`).
/// Each field is `Some` only when that level declares it.
#[derive(Clone, Copy, Default)]
struct RedirectPolicyDeclarations {
    absolute_redirect: Option<bool>,
    port_in_redirect: Option<bool>,
    server_name_in_redirect: Option<bool>,
}

impl RedirectPolicyDeclarations {
    fn is_empty(self) -> bool {
        self.absolute_redirect.is_none()
            && self.port_in_redirect.is_none()
            && self.server_name_in_redirect.is_none()
    }

    /// Resolve this level's declarations over the inherited policy: the nginx
    /// defaults when nothing is inherited, then the declarations that exist.
    fn resolve_over(self, inherited: Option<RedirectPolicyConfig>) -> Option<RedirectPolicyConfig> {
        if self.is_empty() {
            return None;
        }
        let mut policy = inherited.unwrap_or(RedirectPolicyConfig::NGINX_DEFAULT);
        if let Some(value) = self.absolute_redirect {
            policy.absolute_redirect = value;
        }
        if let Some(value) = self.port_in_redirect {
            policy.port_in_redirect = value;
        }
        if let Some(value) = self.server_name_in_redirect {
            policy.server_name_in_redirect = value;
        }
        Some(policy)
    }
}

struct Mapper<'a> {
    app_key: &'a str,
    base_dir: &'a Path,
    proxy_cache_enabled: bool,
    proxy_cache_disk_path: Option<String>,
    proxy_cache_ttl_seconds: u64,
    proxy_cache_stale_ttl_seconds: u64,
    proxy_cache_max_entries: usize,
    /// Cache zone names declared by `proxy_cache_path … keys_zone=<name>:<size>`.
    proxy_cache_zone_names: Vec<String>,
    gzip_enabled: bool,
    gzip_types: Vec<String>,
    gzip_min_length: u64,
    /// http-level `gzip_vary` (nginx default off).
    gzip_vary: bool,
    /// http-level `gzip_static` mode (nginx default off).
    gzip_static: GzipStaticMode,
    /// http-level `fastcgi_*` settings inherited by every server.
    http_fastcgi: FastCgiSettings,
    /// http → server resolved `fastcgi_*` settings inherited by locations.
    server_fastcgi: FastCgiSettings,
    /// http-level `uwsgi_*` (index 0) / `scgi_*` (index 1) settings.
    http_cgi: [CgiSettings; 2],
    /// http → server resolved `uwsgi_*` / `scgi_*` settings.
    server_cgi: [CgiSettings; 2],
    /// http-level `memcached_*` settings.
    http_memcached: MemcachedSettings,
    /// http → server resolved `memcached_*` settings.
    server_memcached: MemcachedSettings,
    /// http-level `grpc_*` settings.
    http_grpc: GrpcSettings,
    /// http → server resolved `grpc_*` settings inherited by locations.
    server_grpc: GrpcSettings,
    /// http-level `grpc_ssl_*` upstream TLS settings.
    http_grpc_ssl: GrpcSslSettings,
    /// http → server resolved `grpc_ssl_*` settings inherited by locations.
    server_grpc_ssl: GrpcSslSettings,
    /// http-level `map` declarations collected in a pre-scan so their result
    /// variables are known before any `add_header`/`proxy_set_header` value is
    /// validated (nginx resolves the variable at configuration init, so
    /// declaration order inside `http {}` does not matter).
    maps: Vec<MapConfig>,
    geo: Vec<GeoConfig>,
    split_clients: Vec<SplitClientsConfig>,
    /// Result variable names (without `$`) declared by `self.maps`.
    map_results: Vec<String>,
    /// Operator-provided `documentRootAllowlist` for variable document roots.
    document_root_allowlist: Vec<String>,
    /// http-level `types` table; `None` keeps the runtime's built-in table.
    mime_types: Option<Vec<MimeTypeConfig>>,
    /// http-level `default_type`.
    default_type: Option<String>,
    /// http-level `map` declarations inherited by every server
    /// (nginx http context inheritance).
    http_proxy_set_headers: Vec<String>,
    /// http-level `error_page` mappings inherited by every server
    /// (nginx http context inheritance, declaration order preserved).
    http_error_pages: Vec<Value>,
    /// http-level `add_header` entries inherited by every server that declares
    /// none (nginx http context inheritance, declaration order preserved).
    http_add_headers: Vec<Value>,
    /// http-level `recursive_error_pages` inherited by every server.
    http_recursive_error_pages: bool,
    /// http-level `proxy_intercept_errors` inherited by every server.
    http_proxy_intercept_errors: bool,
    /// http-level `proxy_ssl_*` upstream TLS settings inherited by every
    /// server and location.
    http_proxy_ssl: ProxySslSettings,
    /// http-level `http2 on|off` inherited by every server's ssl listeners
    /// (nginx 1.25.1+ `http2` directive).
    http_http2: Option<bool>,
    /// http-level `ssl_protocols` window inherited by every server that does
    /// not declare its own (nginx http-context inheritance).
    http_ssl_protocols: Option<SslProtocolRange>,
    /// http-level `expires` / `etag` / `if_modified_since`, inherited by every
    /// server and location that does not declare its own.
    http_cache: CacheDirectives,
    /// http-level redirect construction declarations, inherited by every
    /// server and location (nginx core-location merge).
    http_redirect_policy: RedirectPolicyDeclarations,
    /// http-level nginx `limit_req_status` (default 503).
    http_limit_req_status: Option<u16>,
    /// http-level nginx `limit_conn_status` (default 503).
    http_limit_conn_status: Option<u16>,
    /// http-level nginx `max_ranges` (default: effectively unbounded).
    http_max_ranges: Option<u64>,
    /// http-level proxy response-header directives (`proxy_hide_header`,
    /// `proxy_pass_header`, `proxy_redirect`, `proxy_cookie_path`,
    /// `proxy_cookie_domain`); each directive family inherits with nginx's
    /// replace-on-declare rule.
    http_proxy_response: ProxyResponseDirectives,
    /// Proxy response-header directives resolved for the server currently being
    /// materialized (http ⊕ server). Locations overlay this.
    server_proxy_response: ProxyResponseDirectives,
    /// http-level `merge_slashes` declaration (nginx default `on`).
    http_merge_slashes: Option<bool>,
    /// http-level `underscores_in_headers` declaration (nginx default `off`).
    http_underscores_in_headers: Option<bool>,
    /// http-level `ignore_invalid_headers` declaration (nginx default `on`).
    http_ignore_invalid_headers: Option<bool>,
    /// http-level gzip refinement declarations (`gzip_comp_level`,
    /// `gzip_disable`, `gzip_proxied`), inherited by every server.
    http_gzip_refinements: GzipRefinementDeclarations,
    /// Gzip refinement declarations resolved for the server currently being
    /// materialized (http ⊕ server); locations overlay this.
    server_gzip_refinements: GzipRefinementDeclarations,
    /// http-level nginx autoindex-family declarations, inherited by every server.
    http_autoindex: AutoindexDeclarations,
    /// Autoindex-family declarations resolved for the server currently being
    /// materialized (http ⊕ server); locations overlay this.
    server_autoindex: AutoindexDeclarations,
    /// http-level nginx charset-family declarations, inherited by every server.
    http_charset: CharsetDeclarations,
    /// Charset-family declarations resolved for the server currently being
    /// materialized (http ⊕ server); locations overlay this.
    server_charset: CharsetDeclarations,
    /// http-level `set_real_ip_from` / `real_ip_header` /
    /// `real_ip_recursive` settings inherited by every server.
    http_real_ip: RealIpSettings,
    /// Attached `trustedProxy` settings per (bind, port): a second server on
    /// the same socket with different real_ip settings is an nginx ambiguity.
    real_ip_by_listener: HashMap<(String, u16), RealIpSettings>,
    listeners: Vec<Value>,
    listeners_by_port: HashMap<(String, u16), String>,
    /// Explicit `listen … default_server` owners per (bind, port): a second
    /// `default_server` on the same socket is a nginx config error.
    explicit_defaults: HashMap<(String, u16), String>,
    certificates: Vec<Value>,
    tls_policies: Vec<Value>,
    resources: Vec<Value>,
    upstreams: Vec<Value>,
    upstream_names: Vec<String>,
    virtual_hosts: Vec<Value>,
    streams: Vec<Value>,
    /// `mail { server { … } }` listeners materialized into the mail plane.
    mail_servers: Vec<Value>,
    limit_req_zones: Vec<Value>,
    limit_req_zone_names: Vec<String>,
    limit_conn_zones: Vec<Value>,
    limit_conn_zone_names: Vec<String>,
    /// `http2_max_concurrent_streams` seen while materializing nginx config
    /// (nginx http/server context; the runtime model owns one ingress value).
    http2_max_concurrent_streams: Option<u32>,
    /// Set when different `http2_max_concurrent_streams` values are declared at
    /// different levels; the runtime model owns one global value.
    http2_max_concurrent_streams_conflict: bool,
    /// `proxy_method` inherited from the http level.
    http_proxy_method: Option<String>,
    /// Server-level nginx `valid_referers` policy currently being
    /// materialized (nginx allows the directive at server/location only, so
    /// there is no http-level inheritance).
    server_referer: Option<RefererConfig>,
    /// Ordered nginx stream-context `allow`/`deny` rules inherited by every
    /// stream server that declares none.
    stream_access: Vec<Value>,
    /// nginx stream `map` blocks (bounded per-connection variable engine).
    stream_maps: Vec<MapConfig>,
    stream_map_results: Vec<String>,
    /// nginx stream `limit_conn_zone` declarations (separate from HTTP zones).
    stream_limit_conn_zones: Vec<Value>,
    stream_limit_conn_zone_names: Vec<String>,
    /// nginx stream-context `limit_conn_dry_run` inherited by stream servers.
    stream_limit_conn_dry_run: bool,
    /// nginx stream-context `preread_timeout` inherited by stream servers.
    stream_preread_timeout_ms: Option<u64>,
    /// nginx stream-context `preread_buffer_size` inherited by stream servers.
    stream_preread_buffer_bytes: Option<usize>,
    /// `proxy_method` resolved for the server currently being materialized.
    server_proxy_method: Option<String>,
    /// Largest `client_max_body_size` seen while materializing nginx config.
    client_max_body_size: Option<u64>,
}

impl<'a> Mapper<'a> {
    fn new(app_key: &'a str, base_dir: &'a Path) -> Self {
        Self {
            app_key,
            base_dir,
            proxy_cache_enabled: false,
            proxy_cache_disk_path: None,
            proxy_cache_ttl_seconds: 60,
            proxy_cache_stale_ttl_seconds: 60,
            proxy_cache_max_entries: 4_096,
            proxy_cache_zone_names: Vec::new(),
            gzip_enabled: false,
            gzip_types: Vec::new(),
            gzip_min_length: 20,
            gzip_vary: false,
            gzip_static: GzipStaticMode::Off,
            http_fastcgi: FastCgiSettings::default(),
            server_fastcgi: FastCgiSettings::default(),
            http_cgi: [CgiSettings::default(), CgiSettings::default()],
            server_cgi: [CgiSettings::default(), CgiSettings::default()],
            http_memcached: MemcachedSettings::default(),
            server_memcached: MemcachedSettings::default(),
            http_grpc: GrpcSettings::default(),
            server_grpc: GrpcSettings::default(),
            http_grpc_ssl: GrpcSslSettings::default(),
            server_grpc_ssl: GrpcSslSettings::default(),
            maps: Vec::new(),
            geo: Vec::new(),
            split_clients: Vec::new(),
            map_results: Vec::new(),
            document_root_allowlist: Vec::new(),
            mime_types: None,
            default_type: None,
            http_proxy_set_headers: Vec::new(),
            http_error_pages: Vec::new(),
            http_add_headers: Vec::new(),
            http_recursive_error_pages: false,
            http_proxy_intercept_errors: false,
            http_proxy_ssl: ProxySslSettings::default(),
            http_http2: None,
            http_ssl_protocols: None,
            http_cache: CacheDirectives::default(),
            http_redirect_policy: RedirectPolicyDeclarations::default(),
            http_limit_req_status: None,
            http_limit_conn_status: None,
            http_max_ranges: None,
            http_proxy_response: ProxyResponseDirectives::default(),
            server_proxy_response: ProxyResponseDirectives::default(),
            http_merge_slashes: None,
            http_underscores_in_headers: None,
            http_ignore_invalid_headers: None,
            http_charset: CharsetDeclarations::default(),
            server_charset: CharsetDeclarations::default(),
            http_autoindex: AutoindexDeclarations::default(),
            server_autoindex: AutoindexDeclarations::default(),
            http_gzip_refinements: GzipRefinementDeclarations::default(),
            server_gzip_refinements: GzipRefinementDeclarations::default(),
            http_real_ip: RealIpSettings::default(),
            real_ip_by_listener: HashMap::new(),
            listeners: Vec::new(),
            listeners_by_port: HashMap::new(),
            explicit_defaults: HashMap::new(),
            certificates: Vec::new(),
            tls_policies: Vec::new(),
            resources: Vec::new(),
            upstreams: Vec::new(),
            upstream_names: Vec::new(),
            virtual_hosts: Vec::new(),
            streams: Vec::new(),
            mail_servers: Vec::new(),
            limit_req_zones: Vec::new(),
            limit_req_zone_names: Vec::new(),
            limit_conn_zones: Vec::new(),
            limit_conn_zone_names: Vec::new(),
            client_max_body_size: None,
            http2_max_concurrent_streams: None,
            http2_max_concurrent_streams_conflict: false,
            http_proxy_method: None,
            server_proxy_method: None,
            server_referer: None,
            stream_access: Vec::new(),
            stream_maps: Vec::new(),
            stream_map_results: Vec::new(),
            stream_limit_conn_zones: Vec::new(),
            stream_limit_conn_zone_names: Vec::new(),
            stream_limit_conn_dry_run: false,
            stream_preread_timeout_ms: None,
            stream_preread_buffer_bytes: None,
        }
    }

    /// Result variable names as `&str` for the validation helpers that accept
    /// the nginx `map` vocabulary.
    fn map_variable_refs(&self) -> Vec<&str> {
        self.map_results.iter().map(String::as_str).collect()
    }

    /// Resolve the proxy response-header policy attached to one proxy
    /// resource.
    ///
    /// nginx's compiled default `proxy_redirect default` rule is expanded here
    /// into its literal `<from> <to>` pair (nginx expands it at configuration
    /// time too, from the raw `proxy_pass` argument and the location name), so
    /// the data plane never needs the original directive text. A variable
    /// `proxy_pass` target has no known upstream URL at configuration time:
    /// nginx then builds no implicit default redirect rule and rejects an
    /// explicit `proxy_redirect default` ("cannot be used with proxy_pass
    /// directive with variables"), which `default_rule = None` mirrors.
    fn resolve_proxy_response_policy(
        &self,
        directives: &ProxyResponseDirectives,
        default_rule: Option<ProxyRewriteRuleConfig>,
        location: &NginxDirective,
    ) -> Result<ProxyResponsePolicyConfig, NginxConfigError> {
        let mut policy = directives.resolve();
        match (policy.redirect.take(), default_rule) {
            (None, Some(rule)) | (Some(ProxyRedirectPolicyConfig::Default), Some(rule)) => {
                policy.redirect = Some(ProxyRedirectPolicyConfig::Rules(vec![rule]));
            }
            (None, None) => {
                // No rule can be derived from an unknown target URL; nginx
                // performs no rewriting for such a location.
                policy.redirect = Some(ProxyRedirectPolicyConfig::Off);
            }
            (Some(ProxyRedirectPolicyConfig::Default), None) => {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`proxy_redirect default` cannot be used with a variable `proxy_pass` target; declare explicit `proxy_redirect <from> <to>` rules",
                ));
            }
            (Some(other), _) => policy.redirect = Some(other),
        }
        Ok(policy)
    }

    fn note_http2_max_concurrent_streams(&mut self, streams: u32) {
        match self.http2_max_concurrent_streams {
            Some(existing) if existing != streams => {
                self.http2_max_concurrent_streams_conflict = true;
            }
            None => self.http2_max_concurrent_streams = Some(streams),
            _ => {}
        }
    }

    fn note_client_max_body_size(&mut self, bytes: Option<u64>) {
        if let Some(bytes) = bytes {
            // The app-level limit is the largest declared value; each
            // location's own effective limit is stored on its route and
            // enforced there (the pre-routing check can only use the global
            // bound).
            self.client_max_body_size = Some(
                self.client_max_body_size
                    .map_or(bytes, |existing| existing.max(bytes)),
            );
        }
    }

    /// Append `certificate_id` to the TLS policy attached to an existing shared
    /// listener (SNI multi-`server` on the same `listen … ssl` port).
    fn merge_certificate_into_shared_listener(
        &mut self,
        listener_id: &str,
        certificate_id: &str,
    ) -> Result<(), NginxConfigError> {
        let policy_id = self
            .listeners
            .iter()
            .find(|listener| listener.get("id").and_then(Value::as_str) == Some(listener_id))
            .and_then(|listener| listener.get("tlsPolicyRef"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(policy_id) = policy_id else {
            return Ok(());
        };
        let Some(policy) = self
            .tls_policies
            .iter_mut()
            .find(|policy| policy.get("id").and_then(Value::as_str) == Some(policy_id.as_str()))
        else {
            return Ok(());
        };
        let refs = match policy.get_mut("certificateRefs") {
            Some(Value::Array(array)) => array,
            _ => {
                policy["certificateRefs"] = Value::Array(Vec::new());
                policy
                    .get_mut("certificateRefs")
                    .and_then(Value::as_array_mut)
                    .expect("certificateRefs array")
            }
        };
        let already = refs
            .iter()
            .any(|value| value.as_str() == Some(certificate_id));
        if !already {
            refs.push(Value::String(certificate_id.to_owned()));
        }
        Ok(())
    }

    fn materialize_upstream(&mut self, directive: &NginxDirective) -> Result<(), NginxConfigError> {
        let name =
            directive.args.first().cloned().ok_or_else(|| {
                NginxConfigError::unsupported(directive, "upstream requires a name")
            })?;
        if self.upstream_names.contains(&name) {
            return Ok(());
        }
        let mut targets = Vec::new();
        let mut load_balancing = "round-robin".to_owned();
        let mut hash_config: Option<Value> = None;
        for child in &directive.children {
            match child.name.as_str() {
                "server" => {
                    let Some(address) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "upstream server requires an address",
                        ));
                    };
                    if address.starts_with("unix:") {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "`upstream { server unix:…; }` sockets are typed-plane only: set `unixSocketRoot` under `[http]` in the typed TOML configuration and declare `unix:/absolute/path` on the typed upstream target; a stock nginx file cannot state the confinement root",
                        ));
                    }
                    let url = if address.contains("://") {
                        address.clone()
                    } else {
                        format!("http://{address}")
                    };
                    let mut weight = Value::Null;
                    let mut backup = false;
                    let mut down = false;
                    let mut max_conns: Option<u64> = None;
                    let mut slow_start_ms: Option<u64> = None;
                    for argument in child.args.iter().skip(1) {
                        if let Some(value) = argument.strip_prefix("weight=") {
                            weight = parse_u64(value).map_or(Value::Null, Value::from);
                        } else if argument == "backup" {
                            backup = true;
                        } else if argument == "down" {
                            // nginx `down`: declared but never selected. The
                            // runtime model filters the target out, matching
                            // server.toml `down = true`.
                            down = true;
                        } else if argument.starts_with("max_fails=")
                            || argument.starts_with("fail_timeout=")
                        {
                            // Accepted for nginx compatibility; the runtime
                            // owns its own failure/ejection policy.
                        } else if let Some(value) = argument.strip_prefix("max_conns=") {
                            if parse_u64(value).is_none() {
                                return Err(NginxConfigError::unsupported(
                                    child,
                                    format!("invalid max_conns value `{value}`"),
                                ));
                            }
                            max_conns = parse_u64(value);
                        } else if let Some(value) = argument.strip_prefix("slow_start=") {
                            let Some(seconds) = parse_nginx_time_seconds(value) else {
                                return Err(NginxConfigError::unsupported(
                                    child,
                                    format!("invalid slow_start duration `{value}`"),
                                ));
                            };
                            slow_start_ms = Some(seconds.saturating_mul(1_000));
                        } else {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!(
                                    "unsupported upstream server parameter `{argument}` (supported: weight= backup down max_fails= fail_timeout= max_conns= slow_start=)"
                                ),
                            ));
                        }
                    }
                    if down {
                        continue;
                    }
                    // Normalize hostnames like `server.sdkwork.com` to explicit
                    // targets; a bare port (`server 8080;`) is invalid in nginx.
                    if url == format!("http://{address}") && !address.contains(':') {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("upstream address `{address}` must include a port"),
                        ));
                    }
                    let mut entry = json!({ "url": url });
                    if !weight.is_null() {
                        entry["weight"] = weight;
                    }
                    if backup {
                        entry["backup"] = Value::Bool(true);
                    }
                    if let Some(max_conns) = max_conns {
                        entry["maxConnections"] = Value::from(max_conns);
                    }
                    if let Some(slow_start_ms) = slow_start_ms {
                        entry["slowStartMs"] = Value::from(slow_start_ms);
                    }
                    targets.push(entry);
                }
                "ip_hash" => {
                    load_balancing = "ip-hash".to_owned();
                    hash_config = None;
                }
                "least_conn" => {
                    load_balancing = "least-connections".to_owned();
                    hash_config = None;
                }
                "random" => {
                    // nginx `random [two least_conn]`; `two least_time` is a
                    // Plus-only variant and fails closed.
                    let unsupported = child
                        .args
                        .iter()
                        .any(|argument| argument != "two" && argument != "least_conn");
                    if unsupported {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "random accepts only `two least_conn`",
                        ));
                    }
                    load_balancing = "random-two-least-connections".to_owned();
                    hash_config = None;
                }
                "hash" => {
                    let Some(key) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "hash requires a key variable",
                        ));
                    };
                    let key_var = match key.as_str() {
                        "$request_uri" | "$uri" | "$remote_addr" | "$host" => key.as_str(),
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!(
                                    "unsupported hash key `{other}`; supported: $request_uri $uri $remote_addr $host"
                                ),
                            ));
                        }
                    };
                    let consistent = child.args.iter().any(|argument| argument == "consistent");
                    load_balancing = "hash".to_owned();
                    hash_config = Some(json!({
                        "key": key_var,
                        "consistent": consistent,
                    }));
                }
                "keepalive" | "keepalive_timeout" | "keepalive_requests" | "keepalive_time" => {}
                name if ACCEPTED_IGNORED.contains(&name) => {}
                name => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!("unsupported upstream directive `{name}`"),
                    ));
                }
            }
        }
        if targets.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("upstream `{name}` has no server targets"),
            ));
        }
        // IP literals authorize themselves (/32|/128). Hostname targets (Docker
        // Compose DNS such as `gateway:3900` / `sdkwork-api-cloud-gateway:3900`)
        // authorize the standard private/loopback ranges so resolved bridge
        // addresses pass the SSRF guard — matching server_toml materialize.
        let mut authorized_cidrs = Vec::new();
        let mut has_hostname_target = false;
        for target in &targets {
            let Some(url) = target.get("url").and_then(Value::as_str) else {
                continue;
            };
            let Some(host) = url
                .strip_prefix("http://")
                .or_else(|| url.strip_prefix("https://"))
                .and_then(|rest| rest.rsplit_once(':').map(|(host, _)| host))
            else {
                continue;
            };
            match host.parse::<std::net::IpAddr>() {
                Ok(std::net::IpAddr::V4(ip)) => authorized_cidrs.push(format!("{ip}/32")),
                Ok(std::net::IpAddr::V6(ip)) => authorized_cidrs.push(format!("{ip}/128")),
                Err(_) => has_hostname_target = true,
            }
        }
        if has_hostname_target {
            for network in hostname_upstream_allowed_cidrs() {
                authorized_cidrs.push(network.to_string());
            }
        }
        let mut upstream = json!({
            "id": name,
            "targets": targets,
            "loadBalancing": load_balancing,
        });
        // nginx defaults `proxy_next_upstream error timeout`: transport
        // failures and timeouts fail over to the next target, and each
        // request may try every declared server once.
        upstream["retry"] = json!({
            "maxAttempts": targets.len().min(u8::MAX as usize),
            "timeoutMs": default_nginx_retry_timeout_ms(),
            "retryOn": ["error", "timeout"],
        });
        if let Some(hash) = hash_config {
            upstream["hash"] = hash;
        }
        if !authorized_cidrs.is_empty() {
            upstream["addressPolicy"] = json!({ "allowedCidrs": authorized_cidrs });
        }
        self.upstreams.push(upstream);
        self.upstream_names.push(name);
        Ok(())
    }

    /// Recognize the reserved ACME HTTP-01 challenge location and take it out of
    /// the ordinary location list.
    ///
    /// nginx would serve `location ^~ /.well-known/acme-challenge/ { root X; }`
    /// as a static file tree. The runtime must not: the challenge namespace is
    /// served by a dedicated endpoint with narrow precedence that reads one
    /// bounded regular file, rejects any token outside the ACME character set,
    /// and fails closed instead of falling through to unrelated routes.
    /// `acmeHttp01.webroot` is how that endpoint is configured, so a sidecar
    /// asking for nginx's behavior has to declare it here rather than silently
    /// receiving a weaker generic static route.
    ///
    /// A challenge location that also declares `proxy_pass`, `alias`,
    /// `try_files`, `return`, or `rewrite` is left in the ordinary list: the
    /// operator asked for a different mechanism, and reinterpreting it as a
    /// filesystem challenge root would be worse than honoring it as written.
    fn take_acme_challenge_webroot(
        &self,
        locations: &mut Vec<&NginxDirective>,
    ) -> Result<Option<String>, NginxConfigError> {
        let Some(index) = locations
            .iter()
            .position(|location| is_acme_challenge_location(location))
        else {
            return Ok(None);
        };
        let location = locations[index];
        let mut root_directive = None;
        for child in &location.children {
            match child.name.as_str() {
                "root" => root_directive = Some(child),
                "proxy_pass" | "alias" | "try_files" | "return" | "rewrite" => return Ok(None),
                _ => {}
            }
        }
        let Some(root_directive) = root_directive else {
            return Err(NginxConfigError::unsupported(
                location,
                format!(
                    "the ACME challenge location `{ACME_HTTP_01_CHALLENGE_PREFIX}` requires `root <webroot>`; the runtime serves the exact challenge path from that directory"
                ),
            ));
        };
        let webroot = self.resolve_path(root_directive)?;
        locations.remove(index);
        Ok(Some(webroot))
    }

    /// Declare `acmeHttp01.webroot` on one listener.
    ///
    /// One listener serves one webroot. The certificate worker writes every
    /// challenge for its hosts into a single directory; a listener that served
    /// two would make the CA's fetch depend on which server block happened to
    /// match, so a second and different declaration is a configuration error
    /// rather than a silent last-wins.
    fn declare_listener_acme_webroot(
        &mut self,
        context: &NginxDirective,
        listener_id: &str,
        webroot: &str,
    ) -> Result<(), NginxConfigError> {
        let declared = json!({ "webroot": webroot });
        let Some(listener) = self
            .listeners
            .iter_mut()
            .find(|listener| listener.get("id").and_then(Value::as_str) == Some(listener_id))
        else {
            return Ok(());
        };
        match listener.get("acmeHttp01") {
            None => listener["acmeHttp01"] = declared,
            Some(existing) if *existing == declared => {}
            Some(existing) => {
                let previous = existing
                    .get("webroot")
                    .and_then(Value::as_str)
                    .unwrap_or("(unreadable)");
                return Err(NginxConfigError::unsupported(
                    context,
                    format!(
                        "listener `{listener_id}` already serves ACME HTTP-01 challenges from `{previous}`; one listener serves one webroot"
                    ),
                ));
            }
        }
        Ok(())
    }

    fn materialize_server(
        &mut self,
        directive: &NginxDirective,
        server_index: usize,
    ) -> Result<(), NginxConfigError> {
        let mut listen_specs = Vec::new();
        let mut server_names: Vec<String> = Vec::new();
        let mut certificate_file: Option<String> = None;
        let mut certificate_key: Option<String> = None;
        let mut certificate_name: Option<String> = None;
        let mut locations = Vec::new();
        let mut named_locations = Vec::new();
        let mut client_max_body_size: Option<u64> = None;
        let mut server_root: Option<String> = None;
        let mut server_root_dynamic: Option<String> = None;
        let mut server_rewrite_phase: Vec<Value> = Vec::new();
        let mut server_rewrite_variables: Vec<String> = self.map_results.clone();
        let mut server_try_files: Vec<String> = Vec::new();
        let mut server_index_files: Vec<String> = Vec::new();
        let mut add_headers: Vec<Value> = Vec::new();
        let mut inherited_access = Vec::new();
        let mut inherited_limit_req = Vec::new();
        let mut inherited_auth_realm: Option<String> = None;
        let mut inherited_auth_file: Option<String> = None;
        let mut inherited_auth_off = false;
        let mut inherited_proxy_set_headers: Vec<String> = Vec::new();
        let mut server_http2: Option<bool> = None;
        let mut server_proxy_ssl = ProxySslSettings::default();
        let mut server_real_ip = RealIpSettings::default();
        let mut ssl_verify_client: Option<&str> = None;
        let mut ssl_client_certificate: Option<String> = None;
        let mut ocsp_stapling_file: Option<String> = None;
        let mut server_ssl_protocols: Option<SslProtocolRange> = None;
        let mut server_cache = CacheDirectives::default();
        // nginx http→server→location inheritance for the wave-2 response
        // policy directives.
        let mut server_redirect_policy = RedirectPolicyDeclarations::default();
        let mut server_limit_req_status: Option<u16> = None;
        let mut server_limit_conn_status: Option<u16> = None;
        let mut server_max_ranges: Option<u64> = None;
        // nginx http→server→location inheritance for the proxy
        // response-header policy and the request-boundary policies.
        let mut server_proxy_response = ProxyResponseDirectives::default();
        let mut server_fastcgi_local = FastCgiSettings::default();
        let mut server_cgi_local = [CgiSettings::default(), CgiSettings::default()];
        let mut server_memcached_local = MemcachedSettings::default();
        let mut server_grpc_local = GrpcSettings::default();
        let mut server_grpc_ssl_local = GrpcSslSettings::default();
        let mut server_merge_slashes: Option<bool> = None;
        let mut server_underscores_in_headers: Option<bool> = None;
        let mut server_ignore_invalid_headers: Option<bool> = None;
        let mut server_charset = CharsetDeclarations::default();
        let mut server_autoindex = AutoindexDeclarations::default();
        // nginx `valid_referers` has no http context; each server starts unset.
        self.server_referer = None;
        let mut server_proxy_method: Option<String> = None;
        // nginx http-level gzip state snapshot: server-level `gzip*`
        // directives overlay it (single-value on/off inherits; the type list
        // and min length replace when declared).
        let http_gzip = (
            self.gzip_enabled,
            self.gzip_types.clone(),
            self.gzip_min_length,
        );
        let http_gzip_vary = self.gzip_vary;
        let http_gzip_static = self.gzip_static;
        let mut server_gzip: Option<bool> = None;
        let mut server_gzip_types: Option<Vec<String>> = None;
        let mut server_gzip_min_length: Option<u64> = None;
        let mut server_gzip_vary: Option<bool> = None;
        let mut server_gzip_static: Option<GzipStaticMode> = None;
        let mut server_gzip_refinements = GzipRefinementDeclarations::default();
        let mut server_mime_types: Option<Vec<MimeTypeConfig>> = None;
        let mut server_default_type: Option<String> = None;
        let mut server_rewrite_rules: Vec<Value> = Vec::new();
        let mut server_error_pages: Vec<Value> = Vec::new();
        let mut server_return: Option<Vec<String>> = None;
        let mut server_sub_filter: Option<Value> = None;
        let mut server_recursive_error_pages: Option<bool> = None;
        let mut server_proxy_intercept_errors: Option<bool> = None;

        for child in &directive.children {
            match child.name.as_str() {
                "listen" => {
                    if child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "listen requires an address",
                        ));
                    }
                    let spec = parse_listen_spec(&child.args.join(" "))
                        .map_err(|message| NginxConfigError::unsupported(child, message))?;
                    listen_specs.push(spec);
                }
                "server_name" => {
                    server_names.extend(child.args.iter().cloned());
                }
                "ssl_stapling_file" => {
                    let path = self.resolve_path(child)?;
                    ocsp_stapling_file = Some(validate_ocsp_stapling_file(child, &path)?);
                }
                "ssl_certificate" => {
                    if child.args.len() > 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "multiple ssl_certificate files (dual-certificate) are not supported; use one certificate chain file",
                        ));
                    }
                    certificate_file = Some(self.resolve_path(child)?);
                    certificate_name = Some(format!("nginx-{}", certificate_name_counter()));
                }
                "ssl_certificate_key" => {
                    if child.args.len() > 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "multiple ssl_certificate_key files are not supported",
                        ));
                    }
                    certificate_key = Some(self.resolve_path(child)?);
                }
                "ssl_verify_client" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "ssl_verify_client requires on|optional|off",
                        ));
                    };
                    ssl_verify_client = match value.as_str() {
                        "on" => Some("required"),
                        "optional" => Some("optional"),
                        "off" => None,
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!(
                                    "ssl_verify_client accepts on|optional|off, found `{other}`"
                                ),
                            ))
                        }
                    };
                }
                "ssl_client_certificate" => {
                    let Some(_path) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "ssl_client_certificate requires a CA file",
                        ));
                    };
                    ssl_client_certificate = Some(self.resolve_path(child)?);
                }
                "location" => {
                    if child.args.first().is_some_and(|arg| arg.starts_with('@')) {
                        named_locations.push(child);
                    } else {
                        locations.push(child);
                    }
                }
                // Server-level `root`/`try_files` are the defaults for every
                // location that does not declare its own (nginx semantics).
                "root" => {
                    let Some(root_path) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "root requires a directory",
                        ));
                    };
                    if root_path.contains('$') {
                        if self.document_root_allowlist.is_empty() {
                            return Err(NginxConfigError::unsupported(
                                child,
                                "`root $variable` requires the operator documentRootAllowlist (typed `[http] documentRootAllowlist` / SDKWORK_WEBSERVER_DOCUMENT_ROOT_ALLOWLIST); without it the runtime cannot confine the evaluated path",
                            ));
                        }
                        validate_rewrite_template(child, root_path, &server_rewrite_variables)?;
                        server_root_dynamic = Some(root_path.clone());
                        server_root = None;
                    } else {
                        server_root = Some(self.resolve_path(child)?);
                    }
                }
                "try_files" => {
                    validate_try_files(child)?;
                    server_try_files = child.args.clone();
                }
                "index" => {
                    server_index_files = child.args.clone();
                }
                // `add_header Name value [always];` -> ordered response-header
                // policy with nginx status gating (`always`).
                "add_header" => {
                    let map_variables = self.map_variable_refs();
                    add_headers.push(parse_add_header(child, &map_variables)?);
                }
                "client_max_body_size" => {
                    client_max_body_size = parse_body_size(child)?;
                }
                "proxy_set_header" => {
                    let map_variables = self.map_variable_refs();
                    let entry =
                        format_proxy_set_header_entry_with_maps(&child.args, &map_variables)
                            .map_err(|message| NginxConfigError::unsupported(child, message))?;
                    inherited_proxy_set_headers.push(entry);
                }
                "proxy_hide_header" => server_proxy_response
                    .hide_headers
                    .get_or_insert_with(Vec::new)
                    .push(parse_proxy_header_name(child)?),
                "proxy_pass_header" => server_proxy_response
                    .pass_headers
                    .get_or_insert_with(Vec::new)
                    .push(parse_proxy_header_name(child)?),
                "proxy_redirect" => {
                    apply_proxy_redirect_directive(&mut server_proxy_response, child)?;
                }
                "proxy_cookie_path" => apply_cookie_rewrite_directive(
                    child,
                    &mut server_proxy_response.cookie_path,
                    false,
                )?,
                "proxy_cookie_domain" => apply_cookie_rewrite_directive(
                    child,
                    &mut server_proxy_response.cookie_domain,
                    true,
                )?,
                "merge_slashes" => {
                    server_merge_slashes = Some(parse_on_off(child, "merge_slashes")?);
                }
                "underscores_in_headers" => {
                    server_underscores_in_headers =
                        Some(parse_on_off(child, "underscores_in_headers")?);
                }
                "ignore_invalid_headers" => {
                    server_ignore_invalid_headers =
                        Some(parse_on_off(child, "ignore_invalid_headers")?);
                }
                "charset" => {
                    server_charset.charset = Some(parse_charset_value(child, true)?);
                }
                "charset_types" => {
                    server_charset.types = Some(parse_charset_types(child)?);
                }
                "source_charset" => {
                    server_charset.source_charset = Some(parse_charset_value(child, true)?);
                }
                "override_charset" => {
                    server_charset.override_charset =
                        Some(parse_on_off(child, "override_charset")?);
                }
                "autoindex" => {
                    server_autoindex.enabled = Some(parse_on_off(child, "autoindex")?);
                }
                "autoindex_exact_size" => {
                    server_autoindex.exact_size =
                        Some(parse_on_off(child, "autoindex_exact_size")?);
                }
                "autoindex_localtime" => {
                    server_autoindex.localtime = Some(parse_on_off(child, "autoindex_localtime")?);
                }
                "autoindex_format" => {
                    server_autoindex.format = Some(parse_autoindex_format(child)?);
                }
                "valid_referers" => {
                    self.server_referer = Some(parse_valid_referers(child)?);
                }
                "http2_max_concurrent_streams" => {
                    self.note_http2_max_concurrent_streams(parse_http2_max_concurrent_streams(
                        child,
                    )?);
                }
                "http2_chunk_size" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "`http2_chunk_size` is not exposed by the pinned hyper/h2 ingress (it controls DATA-frame chunking); remove it or declare `http2_max_concurrent_streams` instead",
                    ));
                }
                "proxy_method" => {
                    server_proxy_method = Some(parse_proxy_method(child)?);
                }
                "access_log"
                | "error_log"
                | "proxy_http_version"
                | "proxy_buffering"
                | "proxy_read_timeout"
                | "proxy_send_timeout"
                | "proxy_connect_timeout"
                | "ssl_prefer_server_ciphers"
                | "ssl_session_cache"
                | "ssl_trusted_certificate"
                | "client_body_timeout"
                | "client_header_timeout" => {}
                // nginx inheritance: an explicit server-level `ssl_protocols`
                // replaces the http-level window.
                "ssl_protocols" => {
                    server_ssl_protocols = Some(parse_ssl_protocols(child)?);
                }
                "expires" => {
                    server_cache.expires = Some(parse_expires_directive(child)?);
                }
                "etag" => {
                    server_cache.etag = Some(parse_on_off(child, "etag")?);
                }
                "if_modified_since" => {
                    server_cache.if_modified_since = Some(parse_if_modified_since(child)?);
                }
                "http2" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "http2 requires on or off",
                        ));
                    };
                    match value.as_str() {
                        "on" => server_http2 = Some(true),
                        "off" => server_http2 = Some(false),
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!("http2 accepts on|off, found `{other}`"),
                            ))
                        }
                    }
                }
                "allow" | "deny" => {
                    inherited_access.push(parse_access_rule(child)?);
                }
                "limit_req" => {
                    inherited_limit_req.push(self.parse_limit_req_rule(child)?);
                }
                "proxy_cache" => {
                    self.materialize_proxy_cache(child)?;
                }
                "proxy_ssl_verify"
                | "proxy_ssl_trusted_certificate"
                | "proxy_ssl_certificate"
                | "proxy_ssl_certificate_key"
                | "proxy_ssl_server_name" => {
                    parse_proxy_ssl_directive(&mut server_proxy_ssl, child)?;
                }
                "proxy_ssl_name" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "`proxy_ssl_name` (custom upstream SNI name) is not supported; the runtime sends the upstream hostname as SNI",
                    ));
                }
                "set_real_ip_from" | "real_ip_header" | "real_ip_recursive" => {
                    parse_real_ip_directive(&mut server_real_ip, child)?;
                }
                "auth_basic" => {
                    let realm = child.args.first().cloned().unwrap_or_default();
                    if realm.eq_ignore_ascii_case("off") {
                        inherited_auth_off = true;
                        inherited_auth_realm = None;
                    } else {
                        inherited_auth_off = false;
                        inherited_auth_realm = Some(realm);
                    }
                }
                "auth_basic_user_file" => {
                    inherited_auth_file = Some(self.resolve_path(child)?);
                }
                "absolute_redirect" => {
                    server_redirect_policy.absolute_redirect =
                        Some(parse_on_off(child, "absolute_redirect")?);
                }
                "port_in_redirect" => {
                    server_redirect_policy.port_in_redirect =
                        Some(parse_on_off(child, "port_in_redirect")?);
                }
                "server_name_in_redirect" => {
                    server_redirect_policy.server_name_in_redirect =
                        Some(parse_on_off(child, "server_name_in_redirect")?);
                }
                "limit_req_status" => {
                    server_limit_req_status = Some(parse_limit_status(child)?);
                }
                "limit_conn_status" => {
                    server_limit_conn_status = Some(parse_limit_status(child)?);
                }
                "max_ranges" => {
                    server_max_ranges = Some(parse_max_ranges(child)?);
                }
                "types" => {
                    server_mime_types = Some(parse_types_block(child)?);
                }
                "default_type" => {
                    if child.args.len() != 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "default_type takes exactly one MIME type",
                        ));
                    }
                    validate_media_type(child, &child.args[0])?;
                    server_default_type = Some(child.args[0].clone());
                }
                "fastcgi_pass" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "`fastcgi_pass` is only allowed inside a `location` block",
                    ));
                }
                name if name.starts_with("fastcgi_") => {
                    let map_results = self.map_results.clone();
                    let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                    server_fastcgi_local.apply(child, &map_variables)?;
                }
                "uwsgi_pass" | "scgi_pass" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!("`{}` is only allowed inside a `location` block", child.name),
                    ));
                }
                name if name.starts_with("uwsgi_") || name.starts_with("scgi_") => {
                    let Some(kind) = CgiKind::from_directive(name) else {
                        unreachable!();
                    };
                    let map_results = self.map_results.clone();
                    let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                    server_cgi_local[kind.index()].apply(kind, child, &map_variables)?;
                }
                "memcached_pass" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "`memcached_pass` is only allowed inside a `location` block",
                    ));
                }
                "grpc_pass" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "`grpc_pass` is only allowed inside a `location` block",
                    ));
                }
                name if name.starts_with("grpc_ssl_") => {
                    server_grpc_ssl_local.apply(child)?;
                }
                name if name.starts_with("grpc_") => {
                    let map_results = self.map_results.clone();
                    let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                    server_grpc_local.apply(child, &map_variables)?;
                }
                name if name.starts_with("memcached_") => {
                    server_memcached_local.apply(child)?;
                }
                "gzip" => match child.args.first().map(String::as_str) {
                    Some("on") => server_gzip = Some(true),
                    Some("off") => server_gzip = Some(false),
                    _ => {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "gzip requires on or off",
                        ));
                    }
                },
                "gzip_types" => {
                    if child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "gzip_types requires at least one MIME type",
                        ));
                    }
                    server_gzip_types = Some(child.args.clone());
                }
                "gzip_min_length" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "gzip_min_length requires a size",
                        ));
                    };
                    let Some(bytes) = parse_size_bytes(value) else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("invalid gzip_min_length `{value}`"),
                        ));
                    };
                    server_gzip_min_length = Some(bytes);
                }
                "gzip_vary" => {
                    server_gzip_vary = Some(parse_on_off(child, "gzip_vary")?);
                }
                "gzip_static" => {
                    server_gzip_static = Some(parse_gzip_static(child)?);
                }
                "gzip_disable" => {
                    let (patterns, msie6) = parse_gzip_disable(child)?;
                    server_gzip_refinements.disable = patterns;
                    server_gzip_refinements.disable_msie6 = msie6;
                }
                "gzip_proxied" => {
                    server_gzip_refinements.proxied = Some(parse_gzip_proxied(child)?);
                }
                "rewrite" => {
                    server_rewrite_rules.push(parse_rewrite_rule(child)?);
                }
                "error_page" => {
                    server_error_pages.push(parse_error_page(child)?);
                }
                "return" => {
                    server_return = Some(child.args.clone());
                }
                "sub_filter" => {
                    apply_sub_filter_entry(&mut server_sub_filter, child)?;
                }
                "sub_filter_once" => {
                    apply_sub_filter_once(&mut server_sub_filter, child)?;
                }
                "sub_filter_types" => {
                    apply_sub_filter_types(&mut server_sub_filter, child)?;
                }
                "proxy_intercept_errors" => {
                    server_proxy_intercept_errors =
                        Some(parse_on_off(child, "proxy_intercept_errors")?);
                }
                "recursive_error_pages" => {
                    server_recursive_error_pages =
                        Some(parse_on_off(child, "recursive_error_pages")?);
                }
                name if ACCEPTED_IGNORED.contains(&name) => {}
                name if UNSUPPORTED_SECURITY.contains(&name) => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!("`{name}` cannot be enforced by the SDKWork runtime"),
                    ));
                }
                "set" => {
                    let operation = parse_set_operation(child, &mut server_rewrite_variables)?;
                    if server_rewrite_phase.len() >= crate::config::MAX_REWRITE_PHASE_OPS {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!(
                                "server declares more than {} rewrite-phase instructions",
                                crate::config::MAX_REWRITE_PHASE_OPS
                            ),
                        ));
                    }
                    server_rewrite_phase.push(operation);
                }
                "if" => {
                    let map_variables = self.map_variable_refs();
                    let operation = parse_if_operation(
                        child,
                        &mut server_rewrite_variables,
                        &map_variables,
                        false,
                    )?;
                    if server_rewrite_phase.len() >= crate::config::MAX_REWRITE_PHASE_OPS {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!(
                                "server declares more than {} rewrite-phase instructions",
                                crate::config::MAX_REWRITE_PHASE_OPS
                            ),
                        ));
                    }
                    server_rewrite_phase.push(operation);
                }
                name => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!("unsupported server directive `{name}`"),
                    ));
                }
            }
        }

        validate_server_names(directive, &server_names)?;
        // The ACME challenge namespace is a listener property, not a route, so
        // it is extracted before locations are materialized.
        let acme_webroot = self.take_acme_challenge_webroot(&mut locations)?;
        let primary_name = server_names[0].clone();
        // nginx inheritance: server-level `http2` overrides the http level.
        let http2_on = server_http2.or(self.http_http2).unwrap_or(false);
        let first_port = listen_specs.first().map(|spec| spec.port).unwrap_or(0);
        // The virtual host id is the default-server target on every listener
        // this server owns (nginx: the first server for a listen address is
        // its default, overridden by an explicit `default_server`).
        let virtual_host_id = format!("{}-{first_port}", sanitize_id(&primary_name));
        // nginx server-level `return`: the rewrite phase answers every
        // request before location selection, so the whole host collapses to
        // one catch-all response route and its locations are unreachable.
        let catch_all_return = match server_return {
            Some(args) => {
                let map_variables = self.map_variable_refs();
                Some(materialize_server_return(
                    directive,
                    server_index,
                    &args,
                    &map_variables,
                )?)
            }
            None => None,
        };
        let catch_all_iterations = if catch_all_return.is_none() {
            usize::MAX
        } else {
            0
        };
        // nginx inheritance: the location flag overrides the server value,
        // which overrides the http-level value.
        let inherited_proxy_intercept_errors =
            server_proxy_intercept_errors.unwrap_or(self.http_proxy_intercept_errors);

        let has_ssl = listen_specs.iter().any(|spec| spec.ssl);
        if has_ssl {
            let Some(certificate_file) = certificate_file.as_deref() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "server listens with ssl but declares no ssl_certificate",
                ));
            };
            let Some(certificate_key) = certificate_key.as_deref() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "server listens with ssl but declares no ssl_certificate_key",
                ));
            };
            let certificate_name = certificate_name
                .clone()
                .unwrap_or_else(|| format!("nginx-{}", certificate_name_counter()));
            let any_http2 = listen_specs.iter().any(|spec| spec.ssl && spec.http2)
                || (http2_on && listen_specs.iter().any(|spec| spec.ssl));
            // The SNI certificate index carries exact names, one-level leading
            // wildcards, and authored-order regex patterns; `.suffix` expands
            // to exact+wildcard, a trailing wildcard fails closed.
            let (certificate_names, certificate_regexes) =
                certificate_server_names(directive, &server_names)?;
            let mut certificate_source = json!({
                "type": "protected-file",
                "certificateFile": certificate_file,
                "privateKeyFile": certificate_key,
            });
            // nginx `ssl_stapling_file`: a static DER OCSP response for this
            // certificate, served as the stapled response.
            if let Some(ocsp_file) = &ocsp_stapling_file {
                certificate_source["ocspStaplingFile"] = Value::String(ocsp_file.clone());
            }
            let mut certificate = json!({
                "id": certificate_name,
                "serverNames": certificate_names,
                "source": certificate_source,
            });
            if !certificate_regexes.is_empty() {
                certificate["serverNameRegexes"] =
                    Value::Array(certificate_regexes.into_iter().map(Value::String).collect());
            }
            self.certificates.push(certificate);
            // nginx inheritance: an explicit server-level `ssl_protocols`
            // replaces the http-level window; the runtime default applies when
            // neither level declares one.
            let ssl_protocols = server_ssl_protocols
                .or(self.http_ssl_protocols)
                .unwrap_or_default();
            let mut tls_policy = json!({
                "id": format!("tls-{certificate_name}"),
                "certificateRefs": [certificate_name],
                "minimumVersion": ssl_protocols.minimum.wire(),
                "maximumVersion": ssl_protocols.maximum.wire(),
                // ALPN MUST match the listener protocols exactly (http2 only
                // when `listen … ssl http2` or `http2 on;` is declared).
                "alpn": if any_http2 {
                    json!(["h2", "http/1.1"])
                } else {
                    json!(["http/1.1"])
                },
            });
            if let Some(mode) = ssl_verify_client {
                let Some(ca_file) = ssl_client_certificate.as_deref() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "ssl_verify_client requires ssl_client_certificate",
                    ));
                };
                tls_policy["clientAuth"] = json!({
                    "mode": mode,
                    "caCertificateFiles": [ca_file],
                });
            } else if ssl_client_certificate.is_some() {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "ssl_client_certificate requires ssl_verify_client",
                ));
            }
            self.tls_policies.push(tls_policy);
        }

        // nginx inheritance: `proxy_set_header` directives are inherited
        // from the previous level only when the current level declares none.
        let inherited_proxy_set_headers = inherit_proxy_set_headers_nginx(
            &self.http_proxy_set_headers,
            &inherited_proxy_set_headers,
        );
        // http ⊕ server: location-level values override per directive.
        let effective_proxy_ssl = server_proxy_ssl.merge(&self.http_proxy_ssl);
        let mut effective_real_ip = self.http_real_ip.clone();
        for cidr in server_real_ip.trusted_cidrs {
            if !effective_real_ip.trusted_cidrs.contains(&cidr) {
                effective_real_ip.trusted_cidrs.push(cidr);
            }
        }
        if server_real_ip.recursive {
            effective_real_ip.recursive = true;
        }

        // Resolve the http→server policy once; locations overlay it.
        let effective_redirect_policy =
            server_redirect_policy.resolve_over(self.http_redirect_policy.resolve_over(None));
        let effective_limit_req_status = server_limit_req_status.or(self.http_limit_req_status);
        let effective_limit_conn_status = server_limit_conn_status.or(self.http_limit_conn_status);
        let effective_max_ranges = server_max_ranges.or(self.http_max_ranges);
        self.server_proxy_response = self
            .http_proxy_response
            .clone()
            .overlay(server_proxy_response);
        self.server_fastcgi = self.http_fastcgi.overlay(&server_fastcgi_local);
        self.server_cgi = [
            self.http_cgi[0].overlay(&server_cgi_local[0]),
            self.http_cgi[1].overlay(&server_cgi_local[1]),
        ];
        self.server_memcached = self.http_memcached.overlay(&server_memcached_local);
        self.server_grpc = self.http_grpc.overlay(&server_grpc_local);
        self.server_grpc_ssl = server_grpc_ssl_local.merge(&self.http_grpc_ssl);
        let effective_merge_slashes =
            server_merge_slashes.unwrap_or(self.http_merge_slashes.unwrap_or(true));
        let effective_underscores_in_headers = server_underscores_in_headers
            .unwrap_or(self.http_underscores_in_headers.unwrap_or(false));
        let effective_ignore_invalid_headers = server_ignore_invalid_headers
            .unwrap_or(self.http_ignore_invalid_headers.unwrap_or(true));
        // nginx charset family: http declarations are inherited by the server
        // and handed down to locations through `self.server_charset`.
        self.server_charset = self.http_charset.clone().overlay(server_charset);
        let effective_charset = self.server_charset.resolve();
        // nginx autoindex family: http declarations are inherited by the
        // server and handed down to locations through `self.server_autoindex`.
        self.server_autoindex = self.http_autoindex.clone().overlay(server_autoindex);
        let effective_autoindex = self.server_autoindex.resolve();
        // nginx `proxy_method`: http declaration inherited by the server unless
        // it declares its own; locations overlay this through the mapper field.
        self.server_proxy_method = server_proxy_method.or_else(|| self.http_proxy_method.clone());
        // nginx gzip refinements: http ⊕ server declarations are handed down to
        // locations through `self.server_gzip_refinements`.
        self.server_gzip_refinements = self
            .http_gzip_refinements
            .clone()
            .overlay(server_gzip_refinements.clone());
        let effective_gzip_refinements = self.server_gzip_refinements.resolve();

        let mut listener_refs = Vec::new();
        for spec in &listen_specs {
            let key = (spec.bind.clone(), spec.port);
            self.check_real_ip_conflict(directive, &key, &effective_real_ip)?;
            let listener_id = if let Some(existing) = self.listeners_by_port.get(&key).cloned() {
                // nginx SNI: multiple `server { listen 443 ssl; }` blocks share
                // one listener; merge each server's certificate into the
                // existing TLS policy so every server_name is covered.
                if spec.ssl {
                    if let Some(cert_name) = certificate_name.as_deref() {
                        self.merge_certificate_into_shared_listener(&existing, cert_name)?;
                    }
                }
                if spec.default_server {
                    // nginx: an explicit `default_server` overrides the
                    // first-server default; a second one is an nginx error.
                    if let Some(previous) = self.explicit_defaults.get(&key) {
                        if previous != &virtual_host_id {
                            return Err(NginxConfigError::unsupported(
                                directive,
                                format!(
                                    "duplicate `default_server` on {}:{} (already owned by {previous})",
                                    spec.bind, spec.port
                                ),
                            ));
                        }
                    } else {
                        self.explicit_defaults
                            .insert(key.clone(), virtual_host_id.clone());
                        if let Some(listener) = self.listeners.iter_mut().find(|listener| {
                            listener.get("id").and_then(Value::as_str) == Some(existing.as_str())
                        }) {
                            listener["defaultVirtualHostRef"] =
                                Value::String(virtual_host_id.clone());
                        }
                    }
                }
                existing
            } else {
                if spec.default_server {
                    if let Some(previous) = self.explicit_defaults.get(&key) {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            format!(
                                "duplicate `default_server` on {}:{} (already owned by {previous})",
                                spec.bind, spec.port
                            ),
                        ));
                    }
                    self.explicit_defaults
                        .insert(key.clone(), virtual_host_id.clone());
                }
                let id = format!("listener-{}-{}", sanitize_id(&spec.bind), spec.port);
                let mut listener = json!({
                    "id": id,
                    "bind": spec.bind,
                    "port": spec.port,
                    "protocols": if (spec.http2 || http2_on) && spec.ssl {
                        vec!["http1", "http2"]
                    } else {
                        vec!["http1"]
                    },
                    // nginx default server: the first server declaring this
                    // listen wins unless `default_server` says otherwise.
                    "defaultVirtualHostRef": virtual_host_id,
                });
                if !spec.ssl {
                    listener["allowPlaintextHttp"] = Value::Bool(true);
                }
                if spec.ssl {
                    let policy = format!("tls-{}", certificate_name.as_deref().unwrap_or(""));
                    listener["tlsPolicyRef"] = Value::String(policy);
                }
                self.listeners.push(listener);
                self.listeners_by_port.insert(key, id.clone());
                id
            };
            if let Some(webroot) = acme_webroot.as_deref() {
                self.declare_listener_acme_webroot(directive, &listener_id, webroot)?;
            }
            if !listener_refs.contains(&listener_id) {
                listener_refs.push(listener_id.clone());
            }
            self.apply_real_ip_to_listener(&listener_id);
        }

        // Named locations (`location @name { … }`). Every declared name is
        // valid on its own; only a reference (try_files / error_page) has to
        // resolve. `@pc` / `@h5` keep their Adaptive Web placeholder meaning
        // when they have the historical `root` (+ optional index/try_files)
        // shape; anything else materializes as an ordinary named target.
        let mut named_pc_root = None;
        let mut named_h5_root = None;
        let mut named_location_names: Vec<String> = Vec::new();
        let mut generic_named_locations: Vec<&NginxDirective> = Vec::new();
        for named in &named_locations {
            let name = named_location_name(named)?;
            if named_location_names.contains(&name) {
                return Err(NginxConfigError::unsupported(
                    named,
                    format!("duplicate named location `{name}`"),
                ));
            }
            named_location_names.push(name.clone());
            if matches!(name.as_str(), "@pc" | "@h5") && named_location_has_adaptive_shape(named) {
                let (_, root) = named_adaptive_location_root(named)?;
                if name == "@pc" {
                    named_pc_root = Some(root);
                } else {
                    named_h5_root = Some(root);
                }
            } else {
                generic_named_locations.push(named);
            }
        }

        let mut location_extras = Vec::new();
        for (index, location) in locations.iter().enumerate().take(catch_all_iterations) {
            let extras = self.materialize_location(
                location,
                &primary_name,
                server_index,
                index,
                server_root.as_deref(),
                server_root_dynamic.as_deref(),
                client_max_body_size,
                &server_rewrite_variables,
                &server_try_files,
                &server_index_files,
                &inherited_access,
                &inherited_limit_req,
                inherited_auth_realm.as_deref(),
                inherited_auth_file.as_deref(),
                inherited_auth_off,
                server_sub_filter.as_ref(),
                inherited_proxy_intercept_errors,
                &inherited_proxy_set_headers,
                &effective_proxy_ssl,
                named_pc_root.as_deref(),
                named_h5_root.as_deref(),
                &named_location_names,
            )?;
            location_extras.push(extras);
        }

        if locations.is_empty() && catch_all_return.is_none() {
            // A server whose only behavior is a server-level `rewrite` (nginx's
            // HTTP→HTTPS redirect pattern) has no locations; execute the
            // authored rules in the server rewrite phase.
            for rule in &server_rewrite_rules {
                let mut operation = rule.clone();
                operation["action"] = Value::String("rewrite".to_owned());
                server_rewrite_phase.push(operation);
            }
        }
        if locations.is_empty() && catch_all_return.is_none() && server_rewrite_phase.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "server requires at least one location (or a server-level return/rewrite)",
            ));
        }

        let mut virtual_host = json!({
            "id": virtual_host_id,
            "listenerRefs": listener_refs,
            "serverNames": server_names,
            "routes": [],
        });
        if !server_rewrite_phase.is_empty() {
            virtual_host["rewritePhase"] = Value::Array(server_rewrite_phase);
        }
        // nginx response headers: a level that declares any `add_header`
        // replaces the inherited array, so the server-level list (or the
        // http-level list when the server declares none) becomes the host
        // policy. `always` and declaration order are preserved; the runtime
        // applies nginx's status gating at the response choke point.
        let effective_add_headers = if add_headers.is_empty() {
            self.http_add_headers.clone()
        } else {
            add_headers
        };
        if !effective_add_headers.is_empty() {
            // The runtime `securityHeaders` model defaults
            // `xContentTypeOptions` to `true`, which emits an unconditional
            // nosniff. That is a runtime-native security default, not an
            // imported nginx declaration: nginx only sends nosniff when the
            // configuration asks for it, and then with `add_header`'s status
            // gating. Pin the dedicated default off so the imported policy is
            // the single source (an explicitly configured
            // `add_header X-Content-Type-Options nosniff` becomes an ordinary
            // entry instead of an unconditional duplicate).
            virtual_host["securityHeaders"] = json!({
                "xContentTypeOptions": false,
                "customHeaders": effective_add_headers,
            });
        }
        // Server-level `gzip` resolution: emit a per-host policy only when
        // this server declared a gzip directive; otherwise the runtime
        // inherits the http-level (app-wide) policy.
        if server_gzip.is_some()
            || server_gzip_types.is_some()
            || server_gzip_min_length.is_some()
            || server_gzip_vary.is_some()
            || server_gzip_static.is_some()
            || !self.server_gzip_refinements.is_empty()
        {
            let enabled = server_gzip.unwrap_or(http_gzip.0);
            let mut types = server_gzip_types.unwrap_or_else(|| http_gzip.1.clone());
            if enabled
                && !types
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case("text/html"))
            {
                types.push("text/html".to_owned());
            }
            if types.is_empty() {
                types.push("text/html".to_owned());
            }
            let min_length = server_gzip_min_length.unwrap_or(http_gzip.2);
            let vary = server_gzip_vary.unwrap_or(http_gzip_vary);
            let static_files = server_gzip_static.unwrap_or(http_gzip_static);
            let mut compression = json!({
                "enabled": enabled,
                "types": types,
                "minLength": min_length,
                "vary": vary,
                "staticFiles": static_files,
            });
            set_gzip_refinement_fields(&mut compression, &effective_gzip_refinements)?;
            virtual_host["compression"] = compression;
        }
        // nginx `types` / `default_type`: each level's table replaces the
        // inherited one, while `default_type` inherits independently.
        if let Some(types) = &server_mime_types {
            virtual_host["mimeTypes"] = serde_json::to_value(types).map_err(|error| {
                NginxConfigError::ValidationFailed(format!("nginx materialization failed: {error}"))
            })?;
        }
        if let Some(default_type) = &server_default_type {
            virtual_host["defaultType"] = Value::String(default_type.clone());
        }
        // nginx `error_page`: http-level mappings inherit and server-level
        // entries append; the runtime resolves the last declaration per code.
        if !server_error_pages.is_empty() || !self.http_error_pages.is_empty() {
            let mut pages = self.http_error_pages.clone();
            pages.extend(server_error_pages);
            virtual_host["errorPages"] = Value::Array(pages);
        }
        virtual_host["recursiveErrorPages"] =
            Value::Bool(server_recursive_error_pages.unwrap_or(self.http_recursive_error_pages));
        // nginx inherits `expires`/`etag`/`if_modified_since` from the http
        // context; a server-level declaration replaces the inherited value per
        // directive. Emitting the resolved triple here (instead of only when
        // the server itself declares one) is what makes an http-level policy
        // actually reach the routes.
        let effective_cache = self.http_cache.overlay(server_cache);
        if let Some(policy) = effective_cache.resolve() {
            virtual_host["cachePolicy"] = cache_policy_json(&policy);
        }
        if let Some(policy) = effective_redirect_policy {
            virtual_host["redirectPolicy"] = json!({
                "absoluteRedirect": policy.absolute_redirect,
                "portInRedirect": policy.port_in_redirect,
                "serverNameInRedirect": policy.server_name_in_redirect,
            });
        }
        if let Some(status) = effective_limit_req_status {
            virtual_host["limitReqStatus"] = json!(status);
        }
        if let Some(status) = effective_limit_conn_status {
            virtual_host["limitConnStatus"] = json!(status);
        }
        if let Some(max_ranges) = effective_max_ranges {
            virtual_host["maxRanges"] = json!(max_ranges);
        }
        // nginx `merge_slashes` / `underscores_in_headers` /
        // `ignore_invalid_headers` resolved for this host; only the non-default
        // value is emitted (the model defaults are on / off / on).
        if !effective_merge_slashes {
            virtual_host["mergeSlashes"] = Value::Bool(false);
        }
        // Always explicit: the model's typed-plane default forwards the field,
        // while nginx's directive default drops it, so the imported plane must
        // state the nginx value even when it equals `on`.
        virtual_host["underscoresInHeaders"] = Value::Bool(effective_underscores_in_headers);
        if !effective_ignore_invalid_headers {
            virtual_host["ignoreInvalidHeaders"] = Value::Bool(false);
        }
        if let Some(charset) = &effective_charset {
            virtual_host["charset"] = serde_json::to_value(charset).map_err(|error| {
                NginxConfigError::ValidationFailed(format!("nginx materialization failed: {error}"))
            })?;
        }
        if let Some(autoindex) = &effective_autoindex {
            virtual_host["autoindex"] = serde_json::to_value(autoindex).map_err(|error| {
                NginxConfigError::ValidationFailed(format!("nginx materialization failed: {error}"))
            })?;
        }
        if let Some(referer) = &self.server_referer {
            virtual_host["referer"] = serde_json::to_value(referer).map_err(|error| {
                NginxConfigError::ValidationFailed(format!("nginx materialization failed: {error}"))
            })?;
        }
        // Named locations are materialized after the path locations so their
        // resource ids never collide; they are appended to `routes` with an
        // exact `@name` matcher (unreachable by request path).
        let mut named_route_entries = Vec::new();
        if catch_all_return.is_none() {
            for (named_index, named) in generic_named_locations.iter().enumerate() {
                let index = locations.len() + named_index;
                let extras = self.materialize_location(
                    named,
                    &primary_name,
                    server_index,
                    index,
                    server_root.as_deref(),
                    server_root_dynamic.as_deref(),
                    client_max_body_size,
                    &server_rewrite_variables,
                    &server_try_files,
                    &server_index_files,
                    &inherited_access,
                    &inherited_limit_req,
                    inherited_auth_realm.as_deref(),
                    inherited_auth_file.as_deref(),
                    inherited_auth_off,
                    server_sub_filter.as_ref(),
                    inherited_proxy_intercept_errors,
                    &inherited_proxy_set_headers,
                    &effective_proxy_ssl,
                    named_pc_root.as_deref(),
                    named_h5_root.as_deref(),
                    &named_location_names,
                )?;
                if !extras.rewrite.is_empty() {
                    return Err(NginxConfigError::unsupported(
                        named,
                        "`rewrite` inside a named location is not supported; nginx does not run the rewrite phase for named locations, so executing it would differ",
                    ));
                }
                if extras.secure_link.is_some() {
                    return Err(NginxConfigError::unsupported(
                        named,
                        "`secure_link` inside a named location is not supported; only proxy_pass | return | root | alias | try_files serving behaviors are executed on the internal redirect",
                    ));
                }
                let (path_type, path) = parse_location_match(named)?;
                let mut route = json!({
                    "id": format!("route-{server_index}-{index}"),
                    "match": {"pathType": path_type, "path": path},
                    "resourceRef": format!("loc-{server_index}-{index}"),
                });
                self.attach_location_extras(
                    &mut route,
                    &extras,
                    virtual_host.get("compression"),
                    effective_cache,
                    None,
                    effective_redirect_policy,
                )?;
                named_route_entries.push(route);
            }
        }
        // Rewrite route matches with their actual path types (exact/prefix).
        let mut route_entries = Vec::new();
        for (index, location) in locations.iter().enumerate().take(catch_all_iterations) {
            let (path_type, path) = parse_location_match(location)?;
            let mut route = json!({
                "id": format!("route-{server_index}-{index}"),
                "match": {"pathType": path_type, "path": path},
                "resourceRef": format!("loc-{server_index}-{index}"),
            });
            self.attach_location_extras(
                &mut route,
                &location_extras[index],
                virtual_host.get("compression"),
                effective_cache,
                Some(&server_rewrite_rules),
                effective_redirect_policy,
            )?;
            route_entries.push(route);
        }
        if let Some((_route, resource)) = &catch_all_return {
            self.resources.push(resource.clone());
        }
        virtual_host["routes"] = match &catch_all_return {
            Some((route, _resource)) => Value::Array(vec![route.clone()]),
            None => {
                route_entries.extend(named_route_entries);
                Value::Array(route_entries)
            }
        };
        self.note_client_max_body_size(client_max_body_size);
        self.virtual_hosts.push(virtual_host);
        Ok(())
    }

    /// Attach the per-location policy shared by path routes and named
    /// locations: `rewrite`, `error_page`, `gzip`, access/limit/auth filters,
    /// `sub_filter`, `secure_link`, and the location cache policy.
    #[allow(clippy::too_many_arguments)]
    fn attach_location_extras(
        &self,
        route: &mut Value,
        extras: &LocationExtras,
        host_compression: Option<&Value>,
        effective_cache: CacheDirectives,
        inherited_rewrites: Option<&[Value]>,
        inherited_redirect_policy: Option<RedirectPolicyConfig>,
    ) -> Result<(), NginxConfigError> {
        let mut route_rewrites: Vec<Value> = inherited_rewrites
            .map(<[Value]>::to_vec)
            .unwrap_or_default();
        route_rewrites.extend(extras.rewrite.iter().cloned());
        if !route_rewrites.is_empty() {
            route["rewrite"] = Value::Array(route_rewrites);
        }
        if !extras.error_pages.is_empty() {
            // nginx: a location that declares error_page replaces the
            // inherited set for that location.
            route["errorPages"] = Value::Array(extras.error_pages.clone());
        }
        if !extras.rewrite_phase.is_empty() {
            route["rewritePhase"] = Value::Array(extras.rewrite_phase.clone());
        }
        if let Some(limit) = extras.max_request_body_bytes {
            route["maxRequestBodyBytes"] = Value::from(limit);
        }
        if !extras.response_headers.is_empty() {
            // nginx: a location that declares any add_header replaces the
            // inherited array; the runtime treats a non-empty route list as
            // the complete policy for that route.
            route["responseHeaders"] = Value::Array(extras.response_headers.clone());
        }
        if let Some(types) = &extras.mime_types {
            if let Ok(value) = serde_json::to_value(types) {
                route["mimeTypes"] = value;
            }
        }
        if let Some(default_type) = &extras.default_type {
            route["defaultType"] = Value::String(default_type.clone());
        }
        if extras.gzip.is_some()
            || extras.gzip_types.is_some()
            || extras.gzip_min_length.is_some()
            || extras.gzip_vary.is_some()
            || extras.gzip_static.is_some()
            || !extras.gzip_refinements.is_empty()
        {
            // Location gzip overlays the host policy (on/off inherits;
            // types/min length replace when declared).
            let (host_enabled, host_types, host_min) = match host_compression {
                Some(value) => (
                    value
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    value
                        .get("types")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    value.get("minLength").and_then(Value::as_u64),
                ),
                None => (
                    self.gzip_enabled,
                    self.gzip_types.iter().cloned().map(Value::String).collect(),
                    Some(self.gzip_min_length),
                ),
            };
            let enabled = extras.gzip.unwrap_or(host_enabled);
            let mut types = extras
                .gzip_types
                .clone()
                .map(|values| values.into_iter().map(Value::String).collect())
                .unwrap_or(host_types);
            if enabled
                && !types.iter().any(|value| {
                    value
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case("text/html"))
                })
            {
                types.push(Value::String("text/html".to_owned()));
            }
            if types.is_empty() {
                types.push(Value::String("text/html".to_owned()));
            }
            // nginx gzip_min_length default (20 bytes) matches the mapper's
            // http-level initial state.
            let min_length = extras.gzip_min_length.or(host_min).unwrap_or(20);
            let (host_vary, host_static) = match host_compression {
                Some(value) => (
                    value.get("vary").and_then(Value::as_bool).unwrap_or(false),
                    value
                        .get("staticFiles")
                        .cloned()
                        .unwrap_or_else(|| json!("off")),
                ),
                None => (self.gzip_vary, json!(self.gzip_static)),
            };
            let vary = extras.gzip_vary.unwrap_or(host_vary);
            let static_files = extras
                .gzip_static
                .map(|mode| json!(mode))
                .unwrap_or(host_static);
            let mut compression = json!({
                "enabled": enabled,
                "types": types,
                "minLength": min_length,
                "vary": vary,
                "staticFiles": static_files,
            });
            set_gzip_refinement_fields(
                &mut compression,
                &self
                    .server_gzip_refinements
                    .clone()
                    .overlay(extras.gzip_refinements.clone())
                    .resolve(),
            )?;
            route["compression"] = compression;
        }
        if !extras.access.is_empty() {
            route["access"] = Value::Array(extras.access.clone());
        }
        if !extras.limit_req.is_empty() {
            route["limitReq"] = Value::Array(extras.limit_req.clone());
        }
        if !extras.limit_conn.is_empty() {
            route["limitConn"] = Value::Array(extras.limit_conn.clone());
        }
        if let Some(auth_basic) = &extras.auth_basic {
            route["authBasic"] = auth_basic.clone();
        }
        if let Some(sub_filter) = &extras.sub_filter {
            route["subFilter"] = sub_filter.clone();
        }
        if let Some(secure_link) = &extras.secure_link {
            route["secureLink"] = secure_link.clone();
        }
        // Location-level `expires`/`etag`/`if_modified_since` overlay the
        // host policy per directive; a location that declares none keeps the
        // host policy by omitting the key entirely.
        if let Some(policy) = effective_cache
            .overlay(extras.cache)
            .resolve()
            .filter(|_| !extras.cache.is_empty())
        {
            route["cachePolicy"] = cache_policy_json(&policy);
        }
        // Redirect construction / limit status / max_ranges: a location that
        // declares any of the directives gets the fully resolved policy; one
        // that declares none inherits the host (or app) value.
        if let Some(policy) = extras
            .redirect_policy
            .resolve_over(inherited_redirect_policy)
        {
            route["redirectPolicy"] = json!({
                "absoluteRedirect": policy.absolute_redirect,
                "portInRedirect": policy.port_in_redirect,
                "serverNameInRedirect": policy.server_name_in_redirect,
            });
        }
        if let Some(status) = extras.limit_req_status {
            route["limitReqStatus"] = json!(status);
        }
        if let Some(status) = extras.limit_conn_status {
            route["limitConnStatus"] = json!(status);
        }
        if let Some(max_ranges) = extras.max_ranges {
            route["maxRanges"] = json!(max_ranges);
        }
        if extras.internal {
            route["internal"] = Value::Bool(true);
        }
        // nginx charset family: a location that declares any of the directives
        // gets the fully resolved policy (its declarations over the inherited
        // http/server ones).
        if !extras.charset.is_empty() {
            if let Some(charset) = self
                .server_charset
                .clone()
                .overlay(extras.charset.clone())
                .resolve()
            {
                route["charset"] = serde_json::to_value(&charset).map_err(|error| {
                    NginxConfigError::ValidationFailed(format!(
                        "nginx materialization failed: {error}"
                    ))
                })?;
            }
        }
        // nginx `valid_referers`: a location declaration replaces the server
        // policy; otherwise the server policy applies.
        if let Some(referer) = extras.referer.as_ref().or(self.server_referer.as_ref()) {
            route["referer"] = serde_json::to_value(referer).map_err(|error| {
                NginxConfigError::ValidationFailed(format!("nginx materialization failed: {error}"))
            })?;
        }
        // nginx autoindex family: a location that declares any directive gets
        // the fully resolved policy (its declarations over the inherited ones).
        if !extras.autoindex.is_empty() {
            if let Some(autoindex) = self
                .server_autoindex
                .clone()
                .overlay(extras.autoindex.clone())
                .resolve()
            {
                route["autoindex"] = serde_json::to_value(&autoindex).map_err(|error| {
                    NginxConfigError::ValidationFailed(format!(
                        "nginx materialization failed: {error}"
                    ))
                })?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn materialize_location(
        &mut self,
        location: &NginxDirective,
        server_name: &str,
        server_index: usize,
        index: usize,
        inherited_root: Option<&str>,
        inherited_dynamic_root: Option<&str>,
        inherited_client_max_body_size: Option<u64>,
        server_rewrite_variables: &[String],
        inherited_try_files: &[String],
        inherited_index_files: &[String],
        inherited_access: &[Value],
        inherited_limit_req: &[Value],
        inherited_auth_realm: Option<&str>,
        inherited_auth_file: Option<&str>,
        inherited_auth_off: bool,
        inherited_sub_filter: Option<&Value>,
        inherited_proxy_intercept_errors: bool,
        inherited_proxy_set_headers: &[String],
        inherited_proxy_ssl: &ProxySslSettings,
        named_pc_root: Option<&str>,
        named_h5_root: Option<&str>,
        named_location_names: &[String],
    ) -> Result<LocationExtras, NginxConfigError> {
        let (path_type, location_path) = parse_location_match(location)?;
        let is_named_location = location_path.starts_with('@');
        let resource_id = format!("loc-{server_index}-{index}");
        let mut proxy_pass = None;
        let mut dynamic_target = None;
        let mut proxy_pass_request_headers = true;
        let mut proxy_intercept_errors = inherited_proxy_intercept_errors;
        let mut return_directive = None;
        let mut root = None;
        let mut alias = None;
        let mut dynamic_root: Option<String> = None;
        let mut location_rewrite_phase: Vec<Value> = Vec::new();
        let mut location_rewrite_variables: Vec<String> = server_rewrite_variables.to_vec();
        let mut try_files = Vec::new();
        let mut index_files = Vec::new();
        let mut extras = LocationExtras::default();
        let mut client_max_body_size: Option<u64> = None;
        let mut location_access = Vec::new();
        let mut location_limit_req = Vec::new();
        let mut location_proxy_set_headers: Vec<String> = Vec::new();
        let mut location_proxy_method: Option<String> = None;
        let mut location_fastcgi = FastCgiSettings::default();
        let mut fastcgi_pass: Option<String> = None;
        let mut location_cgi = [CgiSettings::default(), CgiSettings::default()];
        let mut cgi_pass: [Option<String>; 2] = [None, None];
        let mut location_memcached = MemcachedSettings::default();
        let mut memcached_pass: Option<String> = None;
        let mut location_grpc = GrpcSettings::default();
        let mut grpc_pass: Option<String> = None;
        let mut location_grpc_ssl = GrpcSslSettings::default();
        let mut memcached_key_template: Option<String> = None;
        let mut auth_realm = inherited_auth_realm.map(str::to_owned);
        let mut auth_file = inherited_auth_file.map(str::to_owned);
        let mut auth_off = inherited_auth_off;
        let mut location_proxy_ssl = ProxySslSettings::default();
        for child in &location.children {
            match child.name.as_str() {
                "fastcgi_pass" => {
                    if child.args.len() != 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "fastcgi_pass takes exactly one target",
                        ));
                    }
                    fastcgi_pass = Some(child.args[0].clone());
                }
                name if name.starts_with("fastcgi_") => {
                    let map_results = self.map_results.clone();
                    let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                    location_fastcgi.apply(child, &map_variables)?;
                }
                "uwsgi_pass" | "scgi_pass" => {
                    if child.args.len() != 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("{} takes exactly one target", child.name),
                        ));
                    }
                    let Some(kind) = CgiKind::from_directive(&child.name) else {
                        unreachable!();
                    };
                    cgi_pass[kind.index()] = Some(child.args[0].clone());
                }
                name if name.starts_with("uwsgi_") || name.starts_with("scgi_") => {
                    let Some(kind) = CgiKind::from_directive(name) else {
                        unreachable!();
                    };
                    let map_results = self.map_results.clone();
                    let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                    location_cgi[kind.index()].apply(kind, child, &map_variables)?;
                }
                "memcached_pass" => {
                    if child.args.len() != 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "memcached_pass takes exactly one target",
                        ));
                    }
                    memcached_pass = Some(child.args[0].clone());
                }
                "grpc_pass" => {
                    if child.args.len() != 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "grpc_pass takes exactly one target",
                        ));
                    }
                    grpc_pass = Some(child.args[0].clone());
                }
                name if name.starts_with("grpc_ssl_") => {
                    location_grpc_ssl.apply(child)?;
                }
                name if name.starts_with("grpc_") => {
                    let map_results = self.map_results.clone();
                    let map_variables: Vec<&str> = map_results.iter().map(String::as_str).collect();
                    location_grpc.apply(child, &map_variables)?;
                }
                "set" => {
                    let (name, value) = match child.args.as_slice() {
                        [name, value] => (name, value),
                        _ => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                "set requires `$variable value`",
                            ))
                        }
                    };
                    if name == "$memcached_key" {
                        // `set $memcached_key` is the memcached module's request
                        // key, not a general rewrite variable.
                        let map_results = self.map_results.clone();
                        let map_variables: Vec<&str> =
                            map_results.iter().map(String::as_str).collect();
                        crate::config::validate_param_value(value, &map_variables)
                            .map_err(|message| NginxConfigError::unsupported(child, message))?;
                        memcached_key_template = Some(value.clone());
                    } else {
                        let operation =
                            parse_set_operation(child, &mut location_rewrite_variables)?;
                        if location_rewrite_phase.len() >= crate::config::MAX_REWRITE_PHASE_OPS {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!(
                                    "location declares more than {} rewrite-phase instructions",
                                    crate::config::MAX_REWRITE_PHASE_OPS
                                ),
                            ));
                        }
                        location_rewrite_phase.push(operation);
                    }
                }
                "if" => {
                    let map_variables = self.map_variable_refs();
                    let operation = parse_if_operation(
                        child,
                        &mut location_rewrite_variables,
                        &map_variables,
                        false,
                    )?;
                    if location_rewrite_phase.len() >= crate::config::MAX_REWRITE_PHASE_OPS {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!(
                                "location declares more than {} rewrite-phase instructions",
                                crate::config::MAX_REWRITE_PHASE_OPS
                            ),
                        ));
                    }
                    location_rewrite_phase.push(operation);
                }
                name if name.starts_with("memcached_") => {
                    location_memcached.apply(child)?;
                }
                "proxy_pass" => {
                    let Some(target) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "proxy_pass requires a target",
                        ));
                    };
                    if target.contains('$') {
                        validate_dynamic_proxy_target(child, target, &location_rewrite_variables)?;
                        dynamic_target = Some(target.clone());
                    } else {
                        proxy_pass = Some(target.clone());
                    }
                }
                "proxy_method" => {
                    location_proxy_method = Some(parse_proxy_method(child)?);
                }
                "proxy_pass_request_headers" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "proxy_pass_request_headers requires on|off",
                        ));
                    };
                    match value.as_str() {
                        "on" => proxy_pass_request_headers = true,
                        "off" => proxy_pass_request_headers = false,
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!(
                                    "proxy_pass_request_headers accepts on|off, found `{other}`"
                                ),
                            ))
                        }
                    }
                }
                "return" => {
                    return_directive = Some(child.args.clone());
                }
                "root" | "alias" => {
                    let Some(argument) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("{} requires a directory", child.name),
                        ));
                    };
                    if argument.contains('$') {
                        if self.document_root_allowlist.is_empty() {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!(
                                    "`{} $variable` requires the operator documentRootAllowlist (typed `[http] documentRootAllowlist` / SDKWORK_WEBSERVER_DOCUMENT_ROOT_ALLOWLIST); without it the runtime cannot confine the evaluated path",
                                    child.name
                                ),
                            ));
                        }
                        validate_rewrite_template(child, argument, &location_rewrite_variables)?;
                        if child.name == "root" {
                            root = Some(String::new());
                        } else {
                            alias = Some(String::new());
                        }
                        dynamic_root = Some(argument.clone());
                    } else if child.name == "root" {
                        root = Some(self.resolve_path(child)?);
                    } else {
                        alias = Some(self.resolve_path(child)?);
                    }
                }
                "try_files" => {
                    validate_try_files(child)?;
                    try_files = child.args.clone();
                }
                "index" => {
                    index_files = child.args.clone();
                }
                "rewrite" => {
                    extras.rewrite.push(parse_rewrite_rule(child)?);
                }
                "allow" | "deny" => {
                    location_access.push(parse_access_rule(child)?);
                }
                "limit_req" => {
                    location_limit_req.push(self.parse_limit_req_rule(child)?);
                }
                "limit_conn" => {
                    extras.limit_conn.push(self.parse_limit_conn_rule(child)?);
                }
                "auth_basic" => {
                    let realm = child.args.first().cloned().unwrap_or_default();
                    if realm.eq_ignore_ascii_case("off") {
                        auth_off = true;
                        auth_realm = None;
                    } else {
                        auth_off = false;
                        auth_realm = Some(realm);
                    }
                }
                "auth_basic_user_file" => {
                    auth_file = Some(self.resolve_path(child)?);
                }
                "client_max_body_size" => {
                    client_max_body_size = parse_body_size(child)?;
                }
                "add_header" => {
                    // Location-level policy replaces the inherited host array.
                    let map_variables = self.map_variable_refs();
                    extras
                        .response_headers
                        .push(parse_add_header(child, &map_variables)?);
                }
                "proxy_set_header" => {
                    let map_variables = self.map_variable_refs();
                    let entry =
                        format_proxy_set_header_entry_with_maps(&child.args, &map_variables)
                            .map_err(|message| NginxConfigError::unsupported(child, message))?;
                    location_proxy_set_headers.push(entry);
                }
                "proxy_hide_header" => extras
                    .proxy_response
                    .hide_headers
                    .get_or_insert_with(Vec::new)
                    .push(parse_proxy_header_name(child)?),
                "proxy_pass_header" => extras
                    .proxy_response
                    .pass_headers
                    .get_or_insert_with(Vec::new)
                    .push(parse_proxy_header_name(child)?),
                "proxy_redirect" => {
                    apply_proxy_redirect_directive(&mut extras.proxy_response, child)?;
                }
                "proxy_cookie_path" => apply_cookie_rewrite_directive(
                    child,
                    &mut extras.proxy_response.cookie_path,
                    false,
                )?,
                "proxy_cookie_domain" => apply_cookie_rewrite_directive(
                    child,
                    &mut extras.proxy_response.cookie_domain,
                    true,
                )?,
                "internal" => {
                    if !child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "internal takes no arguments",
                        ));
                    }
                    extras.internal = true;
                }
                "charset" => {
                    extras.charset.charset = Some(parse_charset_value(child, true)?);
                }
                "charset_types" => {
                    extras.charset.types = Some(parse_charset_types(child)?);
                }
                "source_charset" => {
                    extras.charset.source_charset = Some(parse_charset_value(child, true)?);
                }
                "override_charset" => {
                    extras.charset.override_charset =
                        Some(parse_on_off(child, "override_charset")?);
                }
                "autoindex" => {
                    extras.autoindex.enabled = Some(parse_on_off(child, "autoindex")?);
                }
                "autoindex_exact_size" => {
                    extras.autoindex.exact_size =
                        Some(parse_on_off(child, "autoindex_exact_size")?);
                }
                "autoindex_localtime" => {
                    extras.autoindex.localtime = Some(parse_on_off(child, "autoindex_localtime")?);
                }
                "autoindex_format" => {
                    extras.autoindex.format = Some(parse_autoindex_format(child)?);
                }
                "valid_referers" => {
                    extras.referer = Some(parse_valid_referers(child)?);
                }
                "empty_gif" => {
                    if !child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "empty_gif takes no arguments",
                        ));
                    }
                    extras.empty_gif = true;
                }
                "stub_status" => {
                    if !child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "stub_status takes no arguments",
                        ));
                    }
                    extras.stub_status = true;
                }
                "sub_filter" => {
                    let Some(from) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "sub_filter requires a pattern to replace",
                        ));
                    };
                    if from.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "sub_filter pattern must not be empty",
                        ));
                    }
                    let to = child.args.get(1).cloned().unwrap_or_default();
                    let entry = extras.sub_filter.get_or_insert_with(|| {
                        json!({ "rules": [], "once": true, "types": ["text/html"], "lastModified": false })
                    });
                    entry["rules"]
                        .as_array_mut()
                        .expect("rules array")
                        .push(json!({ "from": from, "to": to }));
                }
                "types" => {
                    extras.mime_types = Some(parse_types_block(child)?);
                }
                "default_type" => {
                    if child.args.len() != 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "default_type takes exactly one MIME type",
                        ));
                    }
                    validate_media_type(child, &child.args[0])?;
                    extras.default_type = Some(child.args[0].clone());
                }
                "gzip_vary" => {
                    extras.gzip_vary = Some(parse_on_off(child, "gzip_vary")?);
                }
                "gzip_static" => {
                    extras.gzip_static = Some(parse_gzip_static(child)?);
                }
                "gzip_disable" => {
                    let (patterns, msie6) = parse_gzip_disable(child)?;
                    extras.gzip_refinements.disable = patterns;
                    extras.gzip_refinements.disable_msie6 = msie6;
                }
                "gzip_proxied" => {
                    extras.gzip_refinements.proxied = Some(parse_gzip_proxied(child)?);
                }
                "gzip" => {
                    extras.gzip = Some(match child.args.first().map(String::as_str) {
                        Some("on") => true,
                        Some("off") => false,
                        _ => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                "gzip requires on or off",
                            ));
                        }
                    });
                }
                "gzip_types" => {
                    if child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "gzip_types requires at least one MIME type",
                        ));
                    }
                    extras.gzip_types = Some(child.args.clone());
                }
                "gzip_min_length" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "gzip_min_length requires a size",
                        ));
                    };
                    let Some(bytes) = parse_size_bytes(value) else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("invalid gzip_min_length `{value}`"),
                        ));
                    };
                    extras.gzip_min_length = Some(bytes);
                }
                "absolute_redirect" => {
                    extras.redirect_policy.absolute_redirect =
                        Some(parse_on_off(child, "absolute_redirect")?);
                }
                "port_in_redirect" => {
                    extras.redirect_policy.port_in_redirect =
                        Some(parse_on_off(child, "port_in_redirect")?);
                }
                "server_name_in_redirect" => {
                    extras.redirect_policy.server_name_in_redirect =
                        Some(parse_on_off(child, "server_name_in_redirect")?);
                }
                "limit_req_status" => {
                    extras.limit_req_status = Some(parse_limit_status(child)?);
                }
                "limit_conn_status" => {
                    extras.limit_conn_status = Some(parse_limit_status(child)?);
                }
                "max_ranges" => {
                    extras.max_ranges = Some(parse_max_ranges(child)?);
                }
                "error_page" => {
                    extras.error_pages.push(parse_error_page(child)?);
                }
                // nginx inheritance: a location-level declaration replaces the
                // server/http value for that directive only.
                "expires" => {
                    extras.cache.expires = Some(parse_expires_directive(child)?);
                }
                "etag" => {
                    extras.cache.etag = Some(parse_on_off(child, "etag")?);
                }
                "if_modified_since" => {
                    extras.cache.if_modified_since = Some(parse_if_modified_since(child)?);
                }
                "proxy_intercept_errors" => {
                    proxy_intercept_errors = parse_on_off(child, "proxy_intercept_errors")?;
                }
                "sub_filter_once" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "sub_filter_once requires on|off",
                        ));
                    };
                    let enabled = match value.as_str() {
                        "on" => true,
                        "off" => false,
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!("sub_filter_once accepts on|off, found `{other}`"),
                            ))
                        }
                    };
                    extras.sub_filter.get_or_insert_with(|| {
                        json!({ "rules": [], "once": true, "types": ["text/html"], "lastModified": false })
                    })["once"] = Value::Bool(enabled);
                }
                "sub_filter_types" => {
                    if child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "sub_filter_types requires at least one MIME type",
                        ));
                    }
                    let types: Vec<Value> = child.args.iter().cloned().map(Value::String).collect();
                    extras.sub_filter.get_or_insert_with(|| {
                        json!({ "rules": [], "once": true, "types": ["text/html"], "lastModified": false })
                    })["types"] = Value::Array(types);
                }
                "secure_link_secret" => {
                    let Some(secret) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link_secret requires a secret word",
                        ));
                    };
                    if extras.secure_link.is_some() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link_secret conflicts with secure_link/secure_link_md5",
                        ));
                    }
                    extras.secure_link = Some(json!({
                        "mode": "secret",
                        "secret": secret,
                    }));
                }
                "secure_link" => {
                    let Some(argument) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link requires a variable argument like $arg_st",
                        ));
                    };
                    let Some(name) = argument.strip_prefix("$arg_") else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("secure_link argument `{argument}` must be `$arg_<name>`"),
                        ));
                    };
                    if name.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link argument name must not be empty",
                        ));
                    }
                    let entry = extras.secure_link.get_or_insert_with(|| {
                        json!({ "mode": "md5", "argument": "st", "template": "", "expiresArgument": null })
                    });
                    if entry["mode"] != "md5" {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link conflicts with secure_link_secret",
                        ));
                    }
                    entry["argument"] = Value::String(name.to_owned());
                }
                "secure_link_md5" => {
                    let Some(template) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link_md5 requires a template",
                        ));
                    };
                    crate::config::validate_md5_template(template)
                        .map_err(|message| NginxConfigError::unsupported(child, message))?;
                    let entry = extras.secure_link.get_or_insert_with(|| {
                        json!({ "mode": "md5", "argument": "st", "template": "", "expiresArgument": null })
                    });
                    if entry["mode"] != "md5" {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link_md5 conflicts with secure_link_secret",
                        ));
                    }
                    entry["template"] = Value::String(template.clone());
                }
                "secure_link_expires" => {
                    let Some(argument) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link_expires requires a variable argument like $arg_e",
                        ));
                    };
                    let Some(name) = argument.strip_prefix("$arg_") else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!(
                                "secure_link_expires argument `{argument}` must be `$arg_<name>`"
                            ),
                        ));
                    };
                    let entry = extras.secure_link.get_or_insert_with(|| {
                        json!({ "mode": "md5", "argument": "st", "template": "", "expiresArgument": null })
                    });
                    if entry["mode"] != "md5" {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "secure_link_expires conflicts with secure_link_secret",
                        ));
                    }
                    entry["expiresArgument"] = Value::String(name.to_owned());
                }
                "sub_filter_last_modified" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "sub_filter_last_modified requires on|off",
                        ));
                    };
                    let enabled = match value.as_str() {
                        "on" => true,
                        "off" => false,
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!("sub_filter_last_modified accepts on|off, found `{other}`"),
                            ))
                        }
                    };
                    extras.sub_filter.get_or_insert_with(|| {
                        json!({ "rules": [], "once": true, "types": ["text/html"], "lastModified": false })
                    })["lastModified"] = Value::Bool(enabled);
                }
                "proxy_http_version"
                | "proxy_buffering"
                | "proxy_read_timeout"
                | "proxy_send_timeout"
                | "proxy_connect_timeout"
                | "proxy_request_buffering" => {}
                "proxy_cache" => {
                    self.materialize_proxy_cache(child)?;
                }
                "proxy_cache_key" => {
                    self.proxy_cache_enabled = true;
                }
                "proxy_ssl_verify"
                | "proxy_ssl_trusted_certificate"
                | "proxy_ssl_certificate"
                | "proxy_ssl_certificate_key"
                | "proxy_ssl_server_name" => {
                    parse_proxy_ssl_directive(&mut location_proxy_ssl, child)?;
                }
                "proxy_ssl_name" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "`proxy_ssl_name` (custom upstream SNI name) is not supported; the runtime sends the upstream hostname as SNI",
                    ));
                }
                "proxy_cache_use_stale" => {
                    self.proxy_cache_enabled = true;
                    // Accept nginx condition list; runtime serves stale on 5xx
                    // within `staleTtlSeconds` (defaults to default TTL).
                    if self.proxy_cache_stale_ttl_seconds == 0 {
                        self.proxy_cache_stale_ttl_seconds = self.proxy_cache_ttl_seconds.max(60);
                    }
                }
                "proxy_cache_valid" => {
                    self.proxy_cache_enabled = true;
                    self.apply_proxy_cache_valid(child)?;
                }
                name if ACCEPTED_IGNORED.contains(&name) => {}
                name if UNSUPPORTED_SECURITY.contains(&name) => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!("`{name}` cannot be enforced by the SDKWork runtime"),
                    ));
                }
                name => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!("unsupported location directive `{name}`"),
                    ));
                }
            }
        }
        extras.access = if location_access.is_empty() {
            inherited_access.to_vec()
        } else {
            location_access
        };
        extras.limit_req = if location_limit_req.is_empty() {
            inherited_limit_req.to_vec()
        } else {
            location_limit_req
        };
        // http ⊕ server ⊕ location upstream TLS settings.
        let effective_proxy_ssl = location_proxy_ssl.merge(inherited_proxy_ssl);
        extras.auth_basic = if auth_off {
            None
        } else if let Some(realm) = auth_realm {
            Some(self.load_auth_basic(&realm, auth_file.as_deref(), location)?)
        } else if auth_file.is_some() {
            return Err(NginxConfigError::unsupported(
                location,
                "`auth_basic_user_file` requires `auth_basic`",
            ));
        } else {
            None
        };
        let serving = [
            proxy_pass.is_some() || dynamic_target.is_some(),
            fastcgi_pass.is_some(),
            cgi_pass[0].is_some() || cgi_pass[1].is_some(),
            memcached_pass.is_some(),
            grpc_pass.is_some(),
            return_directive.is_some(),
            root.is_some(),
            alias.is_some(),
            extras.empty_gif,
            extras.stub_status,
        ]
        .iter()
        .filter(|present| **present)
        .count();
        // A location with only `try_files` inherits the server-level root
        // (nginx SPA layout), which counts as static serving.
        let inherits_static = serving == 0 && inherited_root.is_some();
        let effective_try_files_for_check: &[String] = if try_files.is_empty() {
            inherited_try_files
        } else {
            try_files.as_slice()
        };
        // `try_files … @name;` is an internal redirect to a named location.
        // The bounded subset executes it from a static location (root or
        // alias, declared or inherited): the data plane probes the request
        // path first and redirects once on a miss.
        let named_fallback = effective_try_files_for_check
            .last()
            .filter(|entry| entry.starts_with('@'))
            .cloned();
        // Adaptive Web dispatch keeps its old placeholder behavior: the
        // document roots live on `@pc` / `@h5` and the try_files sentinel
        // target is not a declared location.
        let adaptive_named_dispatch = !is_named_location
            && serving == 0
            && inherited_root.is_none()
            && named_fallback.is_some()
            && (named_pc_root.is_some() || named_h5_root.is_some());
        if serving > 1 {
            return Err(NginxConfigError::unsupported(
                location,
                "a location must declare exactly one of proxy_pass | fastcgi_pass | uwsgi_pass | scgi_pass | memcached_pass | grpc_pass | return | root | alias",
            ));
        }
        // A location with no serving behavior, no inherited root, and no
        // `try_files` has nothing to execute: nginx would serve it from its
        // compile-time default document root, which the runtime does not own.
        // Materialize the only safe, deterministic outcome (404) instead of
        // rejecting an otherwise-valid production configuration.
        let default_root_missing = serving == 0
            && !inherits_static
            && !adaptive_named_dispatch
            && named_fallback.is_none()
            && effective_try_files_for_check.is_empty();
        if serving == 0
            && !inherits_static
            && !adaptive_named_dispatch
            && named_fallback.is_none()
            && !default_root_missing
        {
            return Err(NginxConfigError::unsupported(
                location,
                "a location must declare exactly one of proxy_pass | fastcgi_pass | uwsgi_pass | scgi_pass | memcached_pass | grpc_pass | return | root | alias (or inherit the server root with try_files, or Adaptive Web try_files → @named)",
            ));
        }
        self.note_client_max_body_size(client_max_body_size);
        let _ = server_name;

        // nginx resolves a static location through `clcf->alias` first and
        // only then through `clcf->root` (`ngx_http_static_handler`): a
        // location that declares `alias` must never fall back to an
        // inherited server/http `root`. Falling back here silently rewrote
        // the document root, which both 404s working configurations and can
        // expose files that `alias` deliberately points outside the
        // document root.
        let effective_root: Option<String> = if alias.is_some() {
            None
        } else {
            root.or_else(|| inherited_root.map(str::to_owned))
        };
        let effective_dynamic_root: Option<String> = dynamic_root
            .clone()
            .or_else(|| inherited_dynamic_root.map(str::to_owned));
        if effective_dynamic_root.is_some()
            && (proxy_pass.is_some()
                || dynamic_target.is_some()
                || fastcgi_pass.is_some()
                || cgi_pass.iter().any(Option::is_some)
                || grpc_pass.is_some()
                || return_directive.is_some())
        {
            return Err(NginxConfigError::unsupported(
                location,
                "a request-time `root`/`alias $variable` is only executable for static content in the bounded runtime; it cannot be combined with proxy_pass/fastcgi_pass/uwsgi_pass/scgi_pass/grpc_pass/return",
            ));
        }

        // Validate the named internal redirect now that the effective
        // static root is known.
        if let Some(target) = &named_fallback {
            if is_named_location {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "named location must not internally redirect to {target}; the internal redirect depth bound is one"
                    ),
                ));
            }
            if adaptive_named_dispatch {
                // Adaptive Web sentinel: not an ordinary named reference.
            } else if !named_location_names.iter().any(|name| name == target) {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!("try_files named fallback `{target}` is not a declared named location"),
                ));
            } else if proxy_pass.is_some() || dynamic_target.is_some() || return_directive.is_some()
            {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "try_files `{target}` dispatch cannot be combined with proxy_pass or return; declare the serving behavior on the named location `{target}` instead"
                    ),
                ));
            } else if effective_root.is_none() && alias.is_none() {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "try_files `{target}` dispatch requires a root or alias (nginx probes the request path under a document root before it redirects)"
                    ),
                ));
            }
        }

        // nginx `proxy_method`: a location declaration wins over the resolved
        // http/server value.
        let effective_proxy_method =
            location_proxy_method.or_else(|| self.server_proxy_method.clone());
        // nginx inheritance: the resolved http ⊕ server proxy response-header
        // directives are overlaid by this location's declarations.
        let proxy_response_directives = self
            .server_proxy_response
            .clone()
            .overlay(extras.proxy_response.clone());

        if let Some(target) = dynamic_target {
            if !effective_proxy_ssl.is_empty() {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`proxy_ssl_*` with variable `proxy_pass` targets is not supported; use a literal http(s):// target",
                ));
            }
            // Variable proxy_pass: the URL is evaluated per request; the
            // materialized model carries the template.
            let request_set_headers = inherit_proxy_set_headers_nginx(
                inherited_proxy_set_headers,
                &location_proxy_set_headers,
            );
            let mut proxy_resource = json!({
                "id": resource_id,
                "type": "proxy",
                "stripPrefix": false,
                "upstreamRef": "",
                "dynamicTarget": target,
                "proxyPassRequestHeaders": proxy_pass_request_headers,
                "proxyInterceptErrors": proxy_intercept_errors,
            });
            if let Some(method) = &effective_proxy_method {
                proxy_resource["proxyMethod"] = Value::String(method.clone());
            }
            if !request_set_headers.is_empty() {
                proxy_resource["requestSetHeaders"] =
                    Value::Array(request_set_headers.into_iter().map(Value::String).collect());
            }
            let policy =
                self.resolve_proxy_response_policy(&proxy_response_directives, None, location)?;
            proxy_resource["proxyResponsePolicy"] = proxy_response_policy_json(&policy)?;
            self.resources.push(proxy_resource);
        } else if let Some(target) = proxy_pass {
            // nginx `proxy_pass` URI replacement: a target with a URI part
            // (`http://backend/`, `http://backend/api`) replaces the
            // location-matched prefix with that URI; without a URI part the
            // full request URI is forwarded unchanged.
            let (upstream_target, target_uri) = split_proxy_pass_target(location, &target)?;
            let upstream_ref = if let Some(rest) = upstream_target
                .strip_prefix("http://")
                .or_else(|| upstream_target.strip_prefix("https://"))
            {
                rest.to_owned()
            } else {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "proxy_pass `{target}` must be http(s)://upstream or http(s)://host:port"
                    ),
                ));
            };
            if !effective_proxy_ssl.is_empty() && !upstream_target.starts_with("https://") {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`proxy_ssl_*` settings require an `https://` `proxy_pass` target (the runtime model attaches one TLS policy per upstream)",
                ));
            }
            let request_set_headers = inherit_proxy_set_headers_nginx(
                inherited_proxy_set_headers,
                &location_proxy_set_headers,
            );
            let mut proxy_resource = json!({
                "id": resource_id,
                "type": "proxy",
                "stripPrefix": false,
                "proxyPassRequestHeaders": proxy_pass_request_headers,
                "proxyInterceptErrors": proxy_intercept_errors,
            });
            if let Some(method) = &effective_proxy_method {
                proxy_resource["proxyMethod"] = Value::String(method.clone());
            }
            if let Some(uri) = &target_uri {
                proxy_resource["targetUri"] = Value::String(uri.clone());
            }
            if !request_set_headers.is_empty() {
                proxy_resource["requestSetHeaders"] =
                    Value::Array(request_set_headers.into_iter().map(Value::String).collect());
            }
            // nginx `proxy_redirect default` expands to a literal rule at
            // configuration time: the pattern is the raw `proxy_pass` argument
            // (plus `/` when it carries no URI part) and the replacement is the
            // location name (or `/` for a URI-less target).
            let default_redirect_rule = ProxyRewriteRuleConfig {
                from: if target_uri.is_some() {
                    target.clone()
                } else {
                    format!("{target}/")
                },
                to: if target_uri.is_some() {
                    location_path.clone()
                } else {
                    "/".to_owned()
                },
            };
            let policy = self.resolve_proxy_response_policy(
                &proxy_response_directives,
                Some(default_redirect_rule),
                location,
            )?;
            proxy_resource["proxyResponsePolicy"] = proxy_response_policy_json(&policy)?;
            if upstream_ref.contains(':') {
                let literal_id = format!("literal-{}", sanitize_id(&upstream_ref));
                if !self.upstream_names.contains(&literal_id) {
                    let host = upstream_ref.rsplit_once(':').map_or("", |(host, _)| host);
                    let allowed_cidrs: Vec<String> = match host.parse::<std::net::IpAddr>() {
                        Ok(std::net::IpAddr::V4(ip)) => vec![format!("{ip}/32")],
                        Ok(std::net::IpAddr::V6(ip)) => vec![format!("{ip}/128")],
                        Err(_) => hostname_upstream_allowed_cidrs()
                            .into_iter()
                            .map(|network| network.to_string())
                            .collect(),
                    };
                    // The literal upstream target never carries the
                    // proxy_pass URI part; replacement is applied per request.
                    let mut literal_upstream = json!({
                        "id": literal_id,
                        "targets": [{"url": upstream_target}],
                        "loadBalancing": "round-robin",
                    });
                    if !allowed_cidrs.is_empty() {
                        literal_upstream["addressPolicy"] =
                            json!({ "allowedCidrs": allowed_cidrs });
                    }
                    self.upstreams.push(literal_upstream);
                    self.upstream_names.push(literal_id.clone());
                }
                proxy_resource["upstreamRef"] = Value::String(literal_id.clone());
                self.apply_upstream_tls(location, &literal_id, &effective_proxy_ssl)?;
                self.resources.push(proxy_resource);
            } else if self.upstream_names.contains(&upstream_ref) {
                proxy_resource["upstreamRef"] = Value::String(upstream_ref.clone());
                self.apply_upstream_tls(location, &upstream_ref, &effective_proxy_ssl)?;
                self.resources.push(proxy_resource);
            } else {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!("proxy_pass references undefined upstream `{upstream_ref}`"),
                ));
            }
        } else if let Some(fastcgi_target) = fastcgi_pass {
            let effective_fastcgi = self.server_fastcgi.overlay(&location_fastcgi);
            if fastcgi_target.contains('$') {
                return Err(NginxConfigError::unsupported(
                    location,
                    "variable `fastcgi_pass` targets are not executable by the bounded FastCGI adapter; use a literal `host:port` or a declared `upstream`",
                ));
            }
            if fastcgi_target.starts_with("unix:") {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`fastcgi_pass unix:` sockets are typed-plane only: set `unixSocketRoot` under `[http]` in the typed TOML configuration and declare the socket on a typed upstream target (`unix:/absolute/path`); a stock nginx file cannot state the confinement root, so it fails closed here",
                ));
            }
            let (upstream_ref, upstream_target) = if fastcgi_target.contains(':') {
                let Some((host, port)) = fastcgi_target.rsplit_once(':') else {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("fastcgi_pass `{fastcgi_target}` must be host:port"),
                    ));
                };
                if host.is_empty() || port.parse::<u16>().map(|port| port == 0).unwrap_or(true) {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("fastcgi_pass `{fastcgi_target}` must be a valid host:port"),
                    ));
                }
                let literal_id = format!("literal-{}", sanitize_id(&fastcgi_target));
                if !self.upstream_names.contains(&literal_id) {
                    let allowed_cidrs: Vec<String> = match host.parse::<std::net::IpAddr>() {
                        Ok(std::net::IpAddr::V4(ip)) => vec![format!("{ip}/32")],
                        Ok(std::net::IpAddr::V6(ip)) => vec![format!("{ip}/128")],
                        Err(_) => hostname_upstream_allowed_cidrs()
                            .into_iter()
                            .map(|network| network.to_string())
                            .collect(),
                    };
                    let mut literal_upstream = json!({
                        "id": literal_id,
                        "targets": [{"url": format!("http://{fastcgi_target}")}],
                        "loadBalancing": "round-robin",
                    });
                    if !allowed_cidrs.is_empty() {
                        literal_upstream["addressPolicy"] =
                            json!({ "allowedCidrs": allowed_cidrs });
                    }
                    self.upstreams.push(literal_upstream);
                    self.upstream_names.push(literal_id.clone());
                }
                (literal_id, format!("http://{fastcgi_target}"))
            } else if self.upstream_names.contains(&fastcgi_target) {
                (fastcgi_target.clone(), String::new())
            } else {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!("fastcgi_pass references undefined upstream `{fastcgi_target}`"),
                ));
            };
            let document_root = alias.clone().or_else(|| effective_root.clone());
            let mut params = Vec::new();
            for param in effective_fastcgi.params.clone().unwrap_or_default() {
                params.push(json!({
                    "name": param.name,
                    "value": param.value,
                    "ifNotEmpty": param.if_not_empty,
                }));
            }
            let hide_headers = effective_fastcgi.hide_headers.clone().unwrap_or_default();
            let pass_headers = effective_fastcgi.pass_headers.clone().unwrap_or_default();
            let response_policy = ProxyResponsePolicyConfig {
                hide_headers,
                pass_headers,
                redirect: None,
                cookie_path: None,
                cookie_domain: None,
            };
            let target_count = if upstream_target.is_empty() {
                self.upstreams
                    .iter()
                    .find(|upstream| {
                        upstream.get("id").and_then(Value::as_str) == Some(upstream_ref.as_str())
                    })
                    .and_then(|upstream| upstream.get("targets"))
                    .and_then(Value::as_array)
                    .map(|targets| targets.len())
                    .unwrap_or(1)
            } else {
                1
            };
            let retry_conditions = effective_fastcgi.next_upstream.clone().unwrap_or_else(|| {
                vec![
                    UpstreamRetryCondition::TransportFailure,
                    UpstreamRetryCondition::Timeout,
                ]
            });
            let retry_condition_names: Vec<&str> = retry_conditions
                .iter()
                .map(|condition| match condition {
                    UpstreamRetryCondition::TransportFailure => "error",
                    UpstreamRetryCondition::Timeout => "timeout",
                    UpstreamRetryCondition::Http502 => "http_502",
                    UpstreamRetryCondition::Http503 => "http_503",
                    UpstreamRetryCondition::Http504 => "http_504",
                })
                .collect();
            let target_count_u8 = u8::try_from(target_count).unwrap_or(u8::MAX);
            let max_attempts = effective_fastcgi
                .next_upstream_tries
                .filter(|tries| *tries != 0)
                .unwrap_or(target_count_u8)
                .clamp(1, target_count_u8.max(1));
            let retry = UpstreamRetryConfig {
                max_attempts,
                timeout_ms: effective_fastcgi.next_upstream_timeout_ms.unwrap_or(30_000),
                retry_on: retry_conditions,
            };
            let resource = json!({
                "id": resource_id,
                "type": "fast-cgi",
                "upstreamRef": upstream_ref,
                "params": params,
                "index": effective_fastcgi.index,
                "splitPathInfo": effective_fastcgi.split_path_info,
                "documentRoot": document_root,
                "passRequestHeaders": effective_fastcgi.pass_request_headers.unwrap_or(true),
                "keepConn": effective_fastcgi.keep_conn.unwrap_or(false),
                "bindAddress": effective_fastcgi.bind_address.flatten().map(|address| address.to_string()),
                "socketKeepalive": effective_fastcgi.socket_keepalive,
                "buffering": effective_fastcgi.buffering.unwrap_or(true),
                "requestBuffering": effective_fastcgi.request_buffering.unwrap_or(true),
                "connectTimeoutMs": effective_fastcgi.connect_timeout_ms.unwrap_or(60_000),
                "sendTimeoutMs": effective_fastcgi.send_timeout_ms.unwrap_or(60_000),
                "readTimeoutMs": effective_fastcgi.read_timeout_ms.unwrap_or(60_000),
                "responsePolicy": proxy_response_policy_json(&response_policy)?,
                "interceptErrors": effective_fastcgi.intercept_errors.unwrap_or(false),
                "retry": json!({
                    "maxAttempts": retry.max_attempts,
                    "timeoutMs": retry.timeout_ms,
                    "retryOn": retry_condition_names,
                }),
            });
            self.resources.push(resource);
        } else if let Some((cgi_kind, cgi_target)) = cgi_pass[0]
            .clone()
            .map(|target| (CgiKind::Uwsgi, target))
            .or_else(|| cgi_pass[1].clone().map(|target| (CgiKind::Scgi, target)))
        {
            let prefix = cgi_kind.prefix();
            let effective_cgi =
                self.server_cgi[cgi_kind.index()].overlay(&location_cgi[cgi_kind.index()]);
            if cgi_target.contains('$') {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "variable `{prefix}_pass` targets are not executable by the bounded {prefix} adapter; use a literal `host:port` or a declared `upstream`"
                    ),
                ));
            }
            if cgi_target.starts_with("unix:") {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "`{prefix}_pass unix:` sockets are typed-plane only: set `unixSocketRoot` under `[http]` in the typed TOML configuration and declare the socket on a typed upstream target (`unix:/absolute/path`); a stock nginx file cannot state the confinement root, so it fails closed here"
                    ),
                ));
            }
            let upstream_ref = if cgi_target.contains(':') {
                let Some((host, port)) = cgi_target.rsplit_once(':') else {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("{prefix}_pass `{cgi_target}` must be host:port"),
                    ));
                };
                if host.is_empty() || port.parse::<u16>().map(|port| port == 0).unwrap_or(true) {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("{prefix}_pass `{cgi_target}` must be a valid host:port"),
                    ));
                }
                let literal_id = format!("literal-{}", sanitize_id(&cgi_target));
                if !self.upstream_names.contains(&literal_id) {
                    let allowed_cidrs: Vec<String> = match host.parse::<std::net::IpAddr>() {
                        Ok(std::net::IpAddr::V4(ip)) => vec![format!("{ip}/32")],
                        Ok(std::net::IpAddr::V6(ip)) => vec![format!("{ip}/128")],
                        Err(_) => hostname_upstream_allowed_cidrs()
                            .into_iter()
                            .map(|network| network.to_string())
                            .collect(),
                    };
                    let mut literal_upstream = json!({
                        "id": literal_id,
                        "targets": [{"url": format!("http://{cgi_target}")}],
                        "loadBalancing": "round-robin",
                    });
                    if !allowed_cidrs.is_empty() {
                        literal_upstream["addressPolicy"] =
                            json!({ "allowedCidrs": allowed_cidrs });
                    }
                    self.upstreams.push(literal_upstream);
                    self.upstream_names.push(literal_id.clone());
                }
                literal_id
            } else if self.upstream_names.contains(&cgi_target) {
                cgi_target.clone()
            } else {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!("{prefix}_pass references undefined upstream `{cgi_target}`"),
                ));
            };
            let document_root = alias.clone().or_else(|| effective_root.clone());
            let mut params = Vec::new();
            for param in effective_cgi.params.clone().unwrap_or_default() {
                params.push(json!({
                    "name": param.name,
                    "value": param.value,
                    "ifNotEmpty": param.if_not_empty,
                }));
            }
            let response_policy = ProxyResponsePolicyConfig {
                hide_headers: effective_cgi.hide_headers.clone().unwrap_or_default(),
                pass_headers: effective_cgi.pass_headers.clone().unwrap_or_default(),
                redirect: None,
                cookie_path: None,
                cookie_domain: None,
            };
            let target_count = self
                .upstreams
                .iter()
                .find(|upstream| {
                    upstream.get("id").and_then(Value::as_str) == Some(upstream_ref.as_str())
                })
                .and_then(|upstream| upstream.get("targets"))
                .and_then(Value::as_array)
                .map(|targets| targets.len())
                .unwrap_or(1);
            let retry_conditions = effective_cgi.next_upstream.clone().unwrap_or_else(|| {
                vec![
                    UpstreamRetryCondition::TransportFailure,
                    UpstreamRetryCondition::Timeout,
                ]
            });
            let retry_condition_names: Vec<&str> = retry_conditions
                .iter()
                .map(|condition| match condition {
                    UpstreamRetryCondition::TransportFailure => "error",
                    UpstreamRetryCondition::Timeout => "timeout",
                    UpstreamRetryCondition::Http502 => "http_502",
                    UpstreamRetryCondition::Http503 => "http_503",
                    UpstreamRetryCondition::Http504 => "http_504",
                })
                .collect();
            let target_count_u8 = u8::try_from(target_count).unwrap_or(u8::MAX);
            let max_attempts = effective_cgi
                .next_upstream_tries
                .filter(|tries| *tries != 0)
                .unwrap_or(target_count_u8)
                .clamp(1, target_count_u8.max(1));
            let retry = UpstreamRetryConfig {
                max_attempts,
                timeout_ms: effective_cgi.next_upstream_timeout_ms.unwrap_or(30_000),
                retry_on: retry_conditions,
            };
            let mut resource = json!({
                "id": resource_id,
                "type": "cgi-upstream",
                "protocol": cgi_kind.protocol(),
                "upstreamRef": upstream_ref,
                "params": params,
                "documentRoot": document_root,
                "passRequestHeaders": effective_cgi.pass_request_headers.unwrap_or(true),
                "bindAddress": effective_cgi.bind_address.flatten().map(|address| address.to_string()),
                "socketKeepalive": effective_cgi.socket_keepalive,
                "buffering": effective_cgi.buffering.unwrap_or(true),
                "requestBuffering": effective_cgi.request_buffering.unwrap_or(true),
                "connectTimeoutMs": effective_cgi.connect_timeout_ms.unwrap_or(60_000),
                "sendTimeoutMs": effective_cgi.send_timeout_ms.unwrap_or(60_000),
                "readTimeoutMs": effective_cgi.read_timeout_ms.unwrap_or(60_000),
                "responsePolicy": proxy_response_policy_json(&response_policy)?,
                "interceptErrors": effective_cgi.intercept_errors.unwrap_or(false),
                "retry": json!({
                    "maxAttempts": retry.max_attempts,
                    "timeoutMs": retry.timeout_ms,
                    "retryOn": retry_condition_names,
                }),
            });
            if cgi_kind == CgiKind::Uwsgi {
                if let Some(modifier1) = effective_cgi.modifier1 {
                    resource["modifier1"] = json!(modifier1);
                }
                if let Some(modifier2) = effective_cgi.modifier2 {
                    resource["modifier2"] = json!(modifier2);
                }
                if let Some(extra) = &effective_cgi.extra_string {
                    resource["extraString"] = Value::String(extra.clone());
                }
            }
            self.resources.push(resource);
        } else if let Some(memcached_target) = memcached_pass {
            let Some(key_template) = memcached_key_template.clone() else {
                return Err(NginxConfigError::unsupported(
                    location,
                    "memcached_pass requires `set $memcached_key <value>` in the same location",
                ));
            };
            let effective_memcached = self.server_memcached.overlay(&location_memcached);
            if memcached_target.contains('$') {
                return Err(NginxConfigError::unsupported(
                    location,
                    "variable `memcached_pass` targets are not executable by the bounded adapter; use a literal `host:port` or a declared `upstream`",
                ));
            }
            if memcached_target.starts_with("unix:") {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`memcached_pass unix:` sockets are typed-plane only: set `unixSocketRoot` under `[http]` in the typed TOML configuration and declare the socket on a typed upstream target (`unix:/absolute/path`); a stock nginx file cannot state the confinement root, so it fails closed here",
                ));
            }
            let upstream_ref = if memcached_target.contains(':') {
                let Some((host, port)) = memcached_target.rsplit_once(':') else {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("memcached_pass `{memcached_target}` must be host:port"),
                    ));
                };
                if host.is_empty() || port.parse::<u16>().map(|port| port == 0).unwrap_or(true) {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("memcached_pass `{memcached_target}` must be a valid host:port"),
                    ));
                }
                let literal_id = format!("literal-{}", sanitize_id(&memcached_target));
                if !self.upstream_names.contains(&literal_id) {
                    let allowed_cidrs: Vec<String> = match host.parse::<std::net::IpAddr>() {
                        Ok(std::net::IpAddr::V4(ip)) => vec![format!("{ip}/32")],
                        Ok(std::net::IpAddr::V6(ip)) => vec![format!("{ip}/128")],
                        Err(_) => hostname_upstream_allowed_cidrs()
                            .into_iter()
                            .map(|network| network.to_string())
                            .collect(),
                    };
                    let mut literal_upstream = json!({
                        "id": literal_id,
                        "targets": [{"url": format!("http://{memcached_target}")}],
                        "loadBalancing": "round-robin",
                    });
                    if !allowed_cidrs.is_empty() {
                        literal_upstream["addressPolicy"] =
                            json!({ "allowedCidrs": allowed_cidrs });
                    }
                    self.upstreams.push(literal_upstream);
                    self.upstream_names.push(literal_id.clone());
                }
                literal_id
            } else if self.upstream_names.contains(&memcached_target) {
                memcached_target.clone()
            } else {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!("memcached_pass references undefined upstream `{memcached_target}`"),
                ));
            };
            let target_count = self
                .upstreams
                .iter()
                .find(|upstream| {
                    upstream.get("id").and_then(Value::as_str) == Some(upstream_ref.as_str())
                })
                .and_then(|upstream| upstream.get("targets"))
                .and_then(Value::as_array)
                .map(|targets| targets.len())
                .unwrap_or(1);
            let retry_conditions = effective_memcached
                .next_upstream
                .clone()
                .unwrap_or_else(|| {
                    vec![
                        UpstreamRetryCondition::TransportFailure,
                        UpstreamRetryCondition::Timeout,
                    ]
                });
            let retry_condition_names: Vec<&str> = retry_conditions
                .iter()
                .map(|condition| match condition {
                    UpstreamRetryCondition::TransportFailure => "error",
                    UpstreamRetryCondition::Timeout => "timeout",
                    UpstreamRetryCondition::Http502 => "http_502",
                    UpstreamRetryCondition::Http503 => "http_503",
                    UpstreamRetryCondition::Http504 => "http_504",
                })
                .collect();
            let target_count_u8 = u8::try_from(target_count).unwrap_or(u8::MAX);
            let max_attempts = effective_memcached
                .next_upstream_tries
                .filter(|tries| *tries != 0)
                .unwrap_or(target_count_u8)
                .clamp(1, target_count_u8.max(1));
            let retry = UpstreamRetryConfig {
                max_attempts,
                timeout_ms: effective_memcached
                    .next_upstream_timeout_ms
                    .unwrap_or(30_000),
                retry_on: retry_conditions,
            };
            self.resources.push(json!({
                "id": resource_id,
                "type": "memcached",
                "upstreamRef": upstream_ref,
                "keyTemplate": key_template,
                "bindAddress": effective_memcached.bind_address.flatten().map(|address| address.to_string()),
                "socketKeepalive": effective_memcached.socket_keepalive,
                "buffering": effective_memcached.buffering.unwrap_or(true),
                "connectTimeoutMs": effective_memcached.connect_timeout_ms.unwrap_or(60_000),
                "sendTimeoutMs": effective_memcached.send_timeout_ms.unwrap_or(60_000),
                "readTimeoutMs": effective_memcached.read_timeout_ms.unwrap_or(60_000),
                "retry": json!({
                    "maxAttempts": retry.max_attempts,
                    "timeoutMs": retry.timeout_ms,
                    "retryOn": retry_condition_names,
                }),
            }));
        } else if let Some(grpc_target) = grpc_pass {
            let effective_grpc = self.server_grpc.overlay(&location_grpc);
            let effective_grpc_ssl = location_grpc_ssl.merge(&self.server_grpc_ssl);
            let tls_policy = materialize_grpc_tls(location, &effective_grpc_ssl, self.base_dir)?;
            let upstream_ref = match split_grpc_target(location, &grpc_target)? {
                GrpcTarget::Literal { scheme, authority } => {
                    // nginx applies `grpc_ssl_*` per location, while the runtime
                    // model owns one TLS policy per upstream. A literal target
                    // therefore gets a policy-scoped id: two locations that point
                    // at the same authority with different TLS policies stay two
                    // upstreams instead of failing the whole configuration.
                    let literal_base = format!("literal-{}", sanitize_id(&authority));
                    let literal_id = match &tls_policy {
                        Some(tls) => {
                            use std::hash::{Hash, Hasher};
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            tls.to_string().hash(&mut hasher);
                            format!("{literal_base}-tls-{:016x}", hasher.finish())
                        }
                        None => literal_base,
                    };
                    let (host, _) = authority
                        .rsplit_once(':')
                        .expect("validated grpc authority");
                    if !self.upstream_names.contains(&literal_id) {
                        let allowed_cidrs: Vec<String> = match host.parse::<std::net::IpAddr>() {
                            Ok(std::net::IpAddr::V4(ip)) => vec![format!("{ip}/32")],
                            Ok(std::net::IpAddr::V6(ip)) => vec![format!("{ip}/128")],
                            Err(_) => hostname_upstream_allowed_cidrs()
                                .into_iter()
                                .map(|network| network.to_string())
                                .collect(),
                        };
                        let mut literal_upstream = json!({
                            "id": literal_id,
                            "targets": [{"url": format!("{scheme}://{authority}")}],
                            "loadBalancing": "round-robin",
                        });
                        if !allowed_cidrs.is_empty() {
                            literal_upstream["addressPolicy"] =
                                json!({ "allowedCidrs": allowed_cidrs });
                        }
                        self.upstreams.push(literal_upstream);
                        self.upstream_names.push(literal_id.clone());
                    }
                    literal_id
                }
                GrpcTarget::Upstream(name) => {
                    if !self.upstream_names.iter().any(|known| known == name) {
                        return Err(NginxConfigError::unsupported(
                            location,
                            format!("grpc_pass references undefined upstream `{name}`"),
                        ));
                    }
                    name.to_owned()
                }
            };
            if let Some(tls) = tls_policy {
                let Some(upstream) = self.upstreams.iter_mut().find(|upstream| {
                    upstream.get("id").and_then(Value::as_str) == Some(upstream_ref.as_str())
                }) else {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("grpc_pass upstream `{upstream_ref}` is not materialized"),
                    ));
                };
                match upstream.get("tls") {
                    Some(existing) if existing != &tls => {
                        return Err(NginxConfigError::unsupported(
                            location,
                            format!(
                                "conflicting `grpc_ssl_*` settings for upstream `{upstream_ref}`; the runtime model owns one TLS policy per upstream"
                            ),
                        ));
                    }
                    _ => upstream["tls"] = tls,
                }
            }
            let response_policy = ProxyResponsePolicyConfig {
                hide_headers: effective_grpc.hide_headers.clone().unwrap_or_default(),
                pass_headers: effective_grpc.pass_headers.clone().unwrap_or_default(),
                redirect: None,
                cookie_path: None,
                cookie_domain: None,
            };
            let target_count = self
                .upstreams
                .iter()
                .find(|upstream| {
                    upstream.get("id").and_then(Value::as_str) == Some(upstream_ref.as_str())
                })
                .and_then(|upstream| upstream.get("targets"))
                .and_then(Value::as_array)
                .map(|targets| targets.len())
                .unwrap_or(1);
            let retry_conditions = effective_grpc.next_upstream.clone().unwrap_or_else(|| {
                vec![
                    UpstreamRetryCondition::TransportFailure,
                    UpstreamRetryCondition::Timeout,
                ]
            });
            let retry_condition_names: Vec<&str> = retry_conditions
                .iter()
                .map(|condition| match condition {
                    UpstreamRetryCondition::TransportFailure => "error",
                    UpstreamRetryCondition::Timeout => "timeout",
                    UpstreamRetryCondition::Http502 => "http_502",
                    UpstreamRetryCondition::Http503 => "http_503",
                    UpstreamRetryCondition::Http504 => "http_504",
                })
                .collect();
            let target_count_u8 = u8::try_from(target_count).unwrap_or(u8::MAX);
            let max_attempts = effective_grpc
                .next_upstream_tries
                .filter(|tries| *tries != 0)
                .unwrap_or(target_count_u8)
                .clamp(1, target_count_u8.max(1));
            let retry = UpstreamRetryConfig {
                max_attempts,
                timeout_ms: effective_grpc.next_upstream_timeout_ms.unwrap_or(30_000),
                retry_on: retry_conditions,
            };
            self.resources.push(json!({
                "id": resource_id,
                "type": "grpc",
                "upstreamRef": upstream_ref,
                "requestSetHeaders": effective_grpc.set_headers.clone().unwrap_or_default(),
                "connectTimeoutMs": effective_grpc.connect_timeout_ms.unwrap_or(60_000),
                "sendTimeoutMs": effective_grpc.send_timeout_ms.unwrap_or(60_000),
                "readTimeoutMs": effective_grpc.read_timeout_ms.unwrap_or(60_000),
                "bufferSize": effective_grpc.buffer_size.unwrap_or(4096),
                "responsePolicy": proxy_response_policy_json(&response_policy)?,
                "interceptErrors": effective_grpc.intercept_errors.unwrap_or(false),
                "retry": json!({
                    "maxAttempts": retry.max_attempts,
                    "timeoutMs": retry.timeout_ms,
                    "retryOn": retry_condition_names,
                }),
            }));
        } else if let Some(return_args) = return_directive {
            let Some(first) = return_args.first() else {
                return Err(NginxConfigError::unsupported(
                    location,
                    "return requires a status or URL",
                ));
            };
            // nginx `return URL;` form: a first argument that is not a
            // status code is a 302 redirect to that URL.
            let Ok(status) = first.parse::<u16>() else {
                if return_args.len() != 1 {
                    return Err(NginxConfigError::unsupported(
                        location,
                        "the `return URL` form takes exactly one argument",
                    ));
                }
                let map_variables = self.map_variable_refs();
                if first.contains('$') && !redirect_variables_ok_with_maps(first, &map_variables) {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("return URL `{first}` uses unsupported variables; supported: $host $request_uri $scheme or a declared map result"),
                    ));
                }
                self.resources.push(json!({
                    "id": resource_id,
                    "type": "redirect",
                    "status": 302,
                    "location": first,
                }));
                let _ = (server_name, client_max_body_size);
                return Ok(extras);
            };
            if status == 444 {
                return Err(NginxConfigError::unsupported(
                    location,
                    "return 444 (close the connection without a response) is not supported by the runtime model",
                ));
            }
            match (status, return_args.get(1)) {
                (301 | 302 | 303 | 307 | 308, Some(url)) => {
                    let map_variables = self.map_variable_refs();
                    if url.contains('$') && !redirect_variables_ok_with_maps(url, &map_variables) {
                        return Err(NginxConfigError::unsupported(
                            location,
                            format!("return URL `{url}` uses unsupported variables; supported: $host $request_uri $scheme or a declared map result"),
                        ));
                    }
                    self.resources.push(json!({
                        "id": resource_id,
                        "type": "redirect",
                        "status": status,
                        "location": url,
                    }));
                }
                (200..=599, _) if return_args.len() == 1 => {
                    self.resources.push(json!({
                        "id": resource_id,
                        "type": "respond",
                        "status": status,
                        "contentType": "text/plain; charset=utf-8",
                        "contentTypeFromUri": true,
                        "body": "",
                    }));
                }
                (200..=599, Some(body)) => {
                    let map_variables = self.map_variable_refs();
                    if body.contains('$') && !return_body_variables_ok(body, &map_variables) {
                        return Err(NginxConfigError::unsupported(
                            location,
                            format!(
                                "return body `{body}` uses variables the runtime does not expand; supported: the declared map results (write `$$` for a literal dollar sign)"
                            ),
                        ));
                    }
                    self.resources.push(json!({
                        "id": resource_id,
                        "type": "respond",
                        "status": status,
                        "contentType": "text/plain; charset=utf-8",
                        "contentTypeFromUri": true,
                        "body": body,
                    }));
                }
                _ => {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!(
                            "return {status} requires a URL for redirects or a body for responses"
                        ),
                    ));
                }
            }
        } else if extras.stub_status {
            // nginx `stub_status`: a bounded, loopback-gated status page.
            self.resources.push(json!({
                "id": resource_id,
                "type": "stub-status",
            }));
        } else if default_root_missing {
            // No document root exists for this location; answer 404 like nginx
            // does for a file missing from its default document root.
            self.resources.push(json!({
                "id": resource_id,
                "type": "respond",
                "status": 404,
                "contentType": "text/plain; charset=utf-8",
                "body": "not found\n",
            }));
        } else if extras.empty_gif {
            // nginx `empty_gif`: a location handler with no body input that
            // serves the canonical 1x1 GIF (GET/HEAD only).
            self.resources.push(json!({
                "id": resource_id,
                "type": "empty-gif",
            }));
        } else if alias.is_none() && (effective_root.is_some() || effective_dynamic_root.is_some())
        {
            // nginx `root` uses POSIX path semantics: a leading `/` is
            // absolute regardless of the host platform, and the full request
            // path is appended to the root (no prefix stripping). A
            // `root $variable` template is resolved per request against the
            // operator allowlist instead.
            let root = effective_root.clone().unwrap_or_default();
            if effective_dynamic_root.is_none() && !root.starts_with('/') {
                return Err(NginxConfigError::unsupported(
                    location,
                    "root must be an absolute directory for nginx compatibility",
                ));
            }
            let effective_try_files: &[String] = if try_files.is_empty() {
                inherited_try_files
            } else {
                try_files.as_slice()
            };
            let effective_index_files: &[String] = if index_files.is_empty() {
                inherited_index_files
            } else {
                index_files.as_slice()
            };
            // `@name` is the bounded named-location internal redirect;
            // a trailing literal path is the SPA document fallback and
            // `=code` keeps the miss behavior.
            let spa_fallback = if let Some(target) = &named_fallback {
                Some(target.clone())
            } else {
                match effective_try_files.last() {
                    Some(last) if last.starts_with('/') && !last.starts_with('$') => {
                        Some(last.trim_start_matches('/').to_owned())
                    }
                    _ => None,
                }
            };
            let index_files: Vec<String> = if effective_index_files.is_empty() {
                vec!["index.html".to_owned()]
            } else {
                effective_index_files.to_vec()
            };
            let mut resource = json!({
                "id": resource_id,
                "type": "static",
                "root": if effective_dynamic_root.is_some() { String::new() } else { root },
                "indexFiles": index_files,
                "spaFallback": spa_fallback,
            });
            if let Some(dynamic) = &effective_dynamic_root {
                resource["dynamicRoot"] = Value::String(dynamic.clone());
            }
            self.resources.push(resource);
        } else if adaptive_named_dispatch {
            // `location / { try_files … @$sdkwork_webserver_surface_final; }`
            // plus Adaptive Web `location @pc` / `location @h5` document roots.
            let pc_root = named_pc_root
                .or(named_h5_root)
                .unwrap_or("/usr/share/sdkwork/web/pc");
            let mut resource = json!({
                "id": resource_id,
                "type": "static",
                "root": pc_root,
                "indexFiles": ["index.html"],
                "spaFallback": "index.html",
            });
            if let Some(h5_root) = named_h5_root {
                resource["h5Root"] = Value::String(h5_root.to_owned());
            }
            self.resources.push(resource);
        } else if let Some(alias) = alias {
            if effective_dynamic_root.is_none() && !alias.starts_with('/') {
                return Err(NginxConfigError::unsupported(
                    location,
                    "alias must be an absolute directory for nginx compatibility",
                ));
            }
            if matches!(path_type, "regex" | "regex-ignore-case") {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`alias` with regex location match is not supported; use a prefix/`^~` location",
                ));
            }
            // nginx replaces the location-matched prefix with the alias value
            // and appends the remainder verbatim. When the alias has no
            // trailing slash but the location prefix does, nginx glues the
            // remainder onto the alias (`/data/w3/imagetop.gif`); the runtime
            // joins by directory traversal, so that footgun fails closed.
            if effective_dynamic_root.is_none()
                && !alias.ends_with('/')
                && location_path.ends_with('/')
            {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "alias `{alias}` without a trailing slash on location `{location_path}` would glue the remainder onto the alias (nginx quirk); add the trailing slash to the alias or the location"
                    ),
                ));
            }
            let mut alias_index_files = if index_files.is_empty() {
                inherited_index_files.to_vec()
            } else {
                index_files
            };
            if alias_index_files.is_empty() {
                alias_index_files = vec!["index.html".to_owned()];
            }
            let alias_effective_try_files: &[String] = if try_files.is_empty() {
                inherited_try_files
            } else {
                try_files.as_slice()
            };
            let alias_spa_fallback = if let Some(target) = &named_fallback {
                Some(target.clone())
            } else {
                match alias_effective_try_files.last() {
                    Some(last) if last.starts_with('/') && !last.starts_with('$') => {
                        Some(last.trim_start_matches('/').to_owned())
                    }
                    _ => None,
                }
            };
            let mut alias_resource = json!({
                "id": resource_id,
                "type": "static",
                "root": if effective_dynamic_root.is_some() { String::new() } else { alias },
                "indexFiles": alias_index_files,
                "stripPrefix": true,
            });
            if let Some(dynamic) = &effective_dynamic_root {
                alias_resource["dynamicRoot"] = Value::String(dynamic.clone());
            }
            if let Some(fallback) = alias_spa_fallback {
                alias_resource["spaFallback"] = Value::String(fallback);
            }
            self.resources.push(alias_resource);
        }
        // nginx inheritance: `sub_filter` directives inherit from the
        // previous level only when the current level declares none.
        if extras.sub_filter.is_none() {
            extras.sub_filter = inherited_sub_filter.cloned();
        }
        let effective_client_max_body_size =
            client_max_body_size.or(inherited_client_max_body_size);
        self.note_client_max_body_size(effective_client_max_body_size);
        extras.max_request_body_bytes = effective_client_max_body_size;
        extras.rewrite_phase = location_rewrite_phase;
        let _ = (server_name, client_max_body_size);
        Ok(extras)
    }

    /// Attach the materialized `proxy_ssl_*` settings to the referenced
    /// upstream (named or literal). nginx applies these per location; the
    /// runtime model owns one TLS policy per upstream, so conflicting
    /// settings for the same upstream fail closed instead of silently
    /// applying one of them.
    fn apply_upstream_tls(
        &mut self,
        location: &NginxDirective,
        upstream_id: &str,
        settings: &ProxySslSettings,
    ) -> Result<(), NginxConfigError> {
        let Some(tls) = materialize_upstream_tls(location, settings)? else {
            return Ok(());
        };
        let Some(upstream) = self
            .upstreams
            .iter_mut()
            .find(|upstream| upstream.get("id").and_then(Value::as_str) == Some(upstream_id))
        else {
            return Ok(());
        };
        match upstream.get("tls") {
            Some(existing) if existing != &tls => {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!(
                        "conflicting `proxy_ssl_*` settings for upstream `{upstream_id}`; the runtime model owns one TLS policy per upstream"
                    ),
                ));
            }
            _ => {
                upstream["tls"] = tls;
            }
        }
        Ok(())
    }

    /// Record nginx realip settings (`set_real_ip_from` / `real_ip_header` /
    /// `real_ip_recursive`) for one (bind, port). The first server on a
    /// socket owns the policy; a second server declaring different settings
    /// is an nginx ambiguity and fails closed.
    fn check_real_ip_conflict(
        &mut self,
        directive: &NginxDirective,
        key: &(String, u16),
        settings: &RealIpSettings,
    ) -> Result<(), NginxConfigError> {
        if settings.trusted_cidrs.is_empty() {
            return Ok(());
        }
        if let Some(previous) = self.real_ip_by_listener.get(key) {
            if previous != settings {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "conflicting `set_real_ip_from`/`real_ip_header` settings on {}:{}; the runtime owns one trusted-proxy policy per listener",
                        key.0, key.1
                    ),
                ));
            }
            return Ok(());
        }
        self.real_ip_by_listener
            .insert(key.clone(), settings.clone());
        Ok(())
    }

    /// Attach the recorded realip settings to the listener JSON once the
    /// listener exists (nginx `set_real_ip_from` etc. become the runtime
    /// `trustedProxy` policy).
    fn apply_real_ip_to_listener(&mut self, listener_id: &str) {
        let Some(settings) = self
            .real_ip_by_listener
            .iter()
            .find(|(key, _)| format!("listener-{}-{}", sanitize_id(&key.0), key.1) == listener_id)
            .map(|(_, settings)| settings.clone())
        else {
            return;
        };
        if let Some(listener) = self
            .listeners
            .iter_mut()
            .find(|listener| listener.get("id").and_then(Value::as_str) == Some(listener_id))
        {
            listener["trustedProxy"] = json!({
                "trustedCidrs": settings.trusted_cidrs,
                "header": "x-forwarded-for",
                "recursive": settings.recursive,
            });
        }
    }

    fn resolve_path(&self, directive: &NginxDirective) -> Result<String, NginxConfigError> {
        let Some(value) = directive.args.first() else {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("{} requires a path", directive.name),
            ));
        };
        if value.starts_with('/') || value.starts_with('\\') {
            Ok(value.clone())
        } else {
            Ok(self.base_dir.join(value).to_string_lossy().into_owned())
        }
    }

    fn materialize_gzip(&mut self, directive: &NginxDirective) -> Result<(), NginxConfigError> {
        let Some(flag) = directive.args.first() else {
            return Err(NginxConfigError::unsupported(
                directive,
                "gzip requires on or off",
            ));
        };
        match flag.as_str() {
            "on" => self.gzip_enabled = true,
            "off" => self.gzip_enabled = false,
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("unsupported gzip value `{other}`"),
                ));
            }
        }
        Ok(())
    }

    fn materialize_types(&mut self, directive: &NginxDirective) -> Result<(), NginxConfigError> {
        self.mime_types = Some(parse_types_block(directive)?);
        Ok(())
    }

    fn materialize_default_type(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        let Some(value) = directive.args.first() else {
            return Err(NginxConfigError::unsupported(
                directive,
                "default_type requires a MIME type",
            ));
        };
        if directive.args.len() != 1 {
            return Err(NginxConfigError::unsupported(
                directive,
                "default_type takes exactly one MIME type",
            ));
        }
        validate_media_type(directive, value)?;
        self.default_type = Some(value.clone());
        Ok(())
    }

    fn materialize_gzip_types(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        if directive.args.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "gzip_types requires at least one MIME type",
            ));
        }
        self.gzip_types = directive.args.clone();
        Ok(())
    }

    fn materialize_gzip_min_length(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        let Some(value) = directive.args.first() else {
            return Err(NginxConfigError::unsupported(
                directive,
                "gzip_min_length requires a size",
            ));
        };
        let Some(bytes) = parse_size_bytes(value) else {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("invalid gzip_min_length `{value}`"),
            ));
        };
        self.gzip_min_length = bytes;
        Ok(())
    }

    fn materialize_gzip_vary(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        self.gzip_vary = parse_on_off(directive, "gzip_vary")?;
        Ok(())
    }

    fn materialize_gzip_static(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        self.gzip_static = parse_gzip_static(directive)?;
        Ok(())
    }

    fn materialize_proxy_cache(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        let Some(zone) = directive.args.first() else {
            return Err(NginxConfigError::unsupported(
                directive,
                "proxy_cache requires a cache zone name",
            ));
        };
        if zone == "off" {
            self.proxy_cache_enabled = false;
            return Ok(());
        }
        if !self.proxy_cache_zone_names.contains(zone) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "proxy_cache references undefined cache zone `{zone}`; declare it with proxy_cache_path … keys_zone={zone}:<size>"
                ),
            ));
        }
        self.proxy_cache_enabled = true;
        Ok(())
    }

    fn materialize_proxy_cache_path(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        let Some(path) = directive.args.first() else {
            return Err(NginxConfigError::unsupported(
                directive,
                "proxy_cache_path requires a directory",
            ));
        };
        let resolved = if path.starts_with('/') || path.starts_with('\\') {
            path.clone()
        } else {
            self.base_dir.join(path).to_string_lossy().into_owned()
        };
        self.proxy_cache_disk_path = Some(resolved);
        self.proxy_cache_enabled = true;
        for argument in directive.args.iter().skip(1) {
            if let Some(rest) = argument.strip_prefix("inactive=") {
                if let Some(seconds) = parse_nginx_time_seconds(rest) {
                    self.proxy_cache_ttl_seconds = seconds;
                }
            } else if let Some(rest) = argument.strip_prefix("keys_zone=") {
                // keys_zone=name:size — approximate shared-memory capacity as
                // maxEntries (~256 bytes per index entry).
                if let Some((name, size)) = rest.split_once(':') {
                    if !name.is_empty() && !self.proxy_cache_zone_names.contains(&name.to_owned()) {
                        self.proxy_cache_zone_names.push(name.to_owned());
                    }
                    if let Some(bytes) = parse_size_bytes(size) {
                        let entries = (bytes / 256).clamp(64, 1_048_576) as usize;
                        self.proxy_cache_max_entries = entries;
                    }
                }
            } else if argument.starts_with("levels=")
                || argument.starts_with("max_size=")
                || argument.starts_with("use_temp_path=")
                || argument.starts_with("manager_files=")
                || argument.starts_with("manager_sleep=")
                || argument.starts_with("manager_threshold=")
                || argument.starts_with("loader_files=")
                || argument.starts_with("loader_sleep=")
                || argument.starts_with("loader_threshold=")
            {
                // Accepted for nginx compatibility; runtime owns eviction.
            } else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("unsupported proxy_cache_path parameter `{argument}`"),
                ));
            }
        }
        Ok(())
    }

    fn apply_proxy_cache_valid(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        if directive.args.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "proxy_cache_valid requires a duration",
            ));
        }
        self.proxy_cache_enabled = true;
        let Some(duration) = directive.args.last() else {
            return Ok(());
        };
        let Some(seconds) = parse_nginx_time_seconds(duration) else {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("invalid proxy_cache_valid duration `{duration}`"),
            ));
        };
        // Prefer the last declared validity as the default TTL used when the
        // upstream omits Cache-Control/Expires.
        self.proxy_cache_ttl_seconds = seconds;
        Ok(())
    }

    fn materialize_limit_req_zone(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        let entry = directive.args.join(" ");
        let zone = parse_limit_req_zone(&entry).map_err(|error| {
            NginxConfigError::unsupported(directive, format!("invalid limit_req_zone: {error}"))
        })?;
        if self.limit_req_zone_names.contains(&zone.name) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("duplicate limit_req_zone `{}`", zone.name),
            ));
        }
        self.limit_req_zone_names.push(zone.name.clone());
        self.limit_req_zones.push(json!({
            "name": zone.name,
            "key": zone.key,
            "maxKeys": zone.max_keys,
            "ratePerSecond": zone.rate_per_second,
        }));
        Ok(())
    }

    fn parse_limit_req_rule(&self, directive: &NginxDirective) -> Result<Value, NginxConfigError> {
        let entry = directive.args.join(" ");
        let rule = parse_limit_req(&entry).map_err(|error| {
            NginxConfigError::unsupported(directive, format!("invalid limit_req: {error}"))
        })?;
        if !self.limit_req_zone_names.contains(&rule.zone) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("limit_req references undefined zone `{}`", rule.zone),
            ));
        }
        Ok(json!({
            "zone": rule.zone,
            "burst": rule.burst,
            "nodelay": rule.nodelay,
        }))
    }

    fn materialize_limit_conn_zone(
        &mut self,
        directive: &NginxDirective,
    ) -> Result<(), NginxConfigError> {
        let entry = directive.args.join(" ");
        let zone = parse_limit_conn_zone(&entry).map_err(|error| {
            NginxConfigError::unsupported(directive, format!("invalid limit_conn_zone: {error}"))
        })?;
        if self.limit_conn_zone_names.contains(&zone.name) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("duplicate limit_conn_zone `{}`", zone.name),
            ));
        }
        self.limit_conn_zone_names.push(zone.name.clone());
        self.limit_conn_zones.push(json!({
            "name": zone.name,
            "key": zone.key,
            "maxKeys": zone.max_keys,
        }));
        Ok(())
    }

    fn parse_limit_conn_rule(&self, directive: &NginxDirective) -> Result<Value, NginxConfigError> {
        let entry = directive.args.join(" ");
        let rule = parse_limit_conn(&entry).map_err(|error| {
            NginxConfigError::unsupported(directive, format!("invalid limit_conn: {error}"))
        })?;
        if !self.limit_conn_zone_names.contains(&rule.zone) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("limit_conn references undefined zone `{}`", rule.zone),
            ));
        }
        Ok(json!({
            "zone": rule.zone,
            "maxConnections": rule.max_connections,
        }))
    }

    fn load_auth_basic(
        &self,
        realm: &str,
        auth_file: Option<&str>,
        location: &NginxDirective,
    ) -> Result<Value, NginxConfigError> {
        let Some(path) = auth_file else {
            return Err(NginxConfigError::unsupported(
                location,
                "`auth_basic` requires `auth_basic_user_file`",
            ));
        };
        let contents = crate::fs_bounds::read_text_capped(
            Path::new(path),
            crate::fs_bounds::MAX_IMPORT_SOURCE_BYTES,
        )
        .map_err(|error| {
            NginxConfigError::unsupported(
                location,
                format!("cannot read auth_basic_user_file `{path}`: {error}"),
            )
        })?;
        let users = parse_htpasswd(&contents).map_err(|error| {
            NginxConfigError::unsupported(
                location,
                format!("invalid auth_basic_user_file `{path}`: {error}"),
            )
        })?;
        Ok(json!({
            "realm": realm,
            "users": users.iter().map(|user| json!({
                "username": user.username,
                "passwordHash": user.password_hash,
            })).collect::<Vec<_>>(),
        }))
    }

    fn materialize_stream_context(
        &mut self,
        directives: &[NginxDirective],
    ) -> Result<(), NginxConfigError> {
        let mut stream_index = 0usize;
        for directive in directives {
            match directive.name.as_str() {
                "upstream" => self.materialize_upstream(directive)?,
                "server" => {
                    self.materialize_stream_server(directive, stream_index)?;
                    stream_index += 1;
                }
                "log_format" | "access_log" | "error_log" | "resolver" => {}
                // nginx `stream` context `map`: executable over the bounded
                // per-connection variable set (accept/preread facts).
                "map" => {
                    let map = parse_stream_map(directive)?;
                    if let Some(result) = map.result.strip_prefix('$') {
                        if self.stream_map_results.iter().any(|name| name == result) {
                            return Err(NginxConfigError::unsupported(
                                directive,
                                format!("duplicate stream map result variable `${result}`"),
                            ));
                        }
                        self.stream_map_results.push(result.to_owned());
                    }
                    self.stream_maps.push(map);
                }
                // nginx stream access phase: ordered peer allow/deny rules.
                "allow" | "deny" => self.stream_access.push(parse_access_rule(directive)?),
                "limit_conn_zone" => {
                    let entry = directive.args.join(" ");
                    let zone = parse_limit_conn_zone(&entry).map_err(|error| {
                        NginxConfigError::unsupported(
                            directive,
                            format!("invalid stream limit_conn_zone: {error}"),
                        )
                    })?;
                    if self.stream_limit_conn_zone_names.contains(&zone.name) {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            format!("duplicate stream limit_conn_zone `{}`", zone.name),
                        ));
                    }
                    self.stream_limit_conn_zone_names.push(zone.name.clone());
                    self.stream_limit_conn_zones.push(json!({
                        "name": zone.name,
                        "key": zone.key,
                        "maxKeys": zone.max_keys,
                    }));
                }
                "limit_conn_dry_run" => {
                    self.stream_limit_conn_dry_run = parse_on_off(directive, "limit_conn_dry_run")?;
                }
                "preread_timeout" => {
                    let Some(value) = directive.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            "preread_timeout requires a duration",
                        ));
                    };
                    let Some(seconds) = parse_nginx_time_seconds(value) else {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            format!("invalid preread_timeout `{value}`"),
                        ));
                    };
                    self.stream_preread_timeout_ms = Some(seconds.saturating_mul(1000));
                }
                "preread_buffer_size" => {
                    let Some(bytes) = parse_body_size(directive)? else {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            "preread_buffer_size requires a size",
                        ));
                    };
                    if !(512..=1024 * 1024).contains(&bytes) {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            "preread_buffer_size must be between 512 bytes and 1 MiB",
                        ));
                    }
                    self.stream_preread_buffer_bytes = Some(bytes as usize);
                }
                "proxy_protocol_timeout" => {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "`proxy_protocol_timeout` bounds reading an inbound PROXY protocol header, which the runtime does not parse; the runtime emits PROXY protocol v1 to the upstream (`proxy_protocol on`) and has no inbound header to time out",
                    ));
                }
                name if ACCEPTED_IGNORED.contains(&name) => {}
                name => {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("unsupported stream directive `{name}`"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn materialize_stream_server(
        &mut self,
        directive: &NginxDirective,
        index: usize,
    ) -> Result<(), NginxConfigError> {
        let mut listen_specs = Vec::new();
        let mut proxy_pass = None;
        let mut proxy_timeout_ms = 60_000_u64;
        let mut proxy_protocol = false;
        let mut ssl_preread = false;
        let mut certificate_file = None;
        let mut certificate_key = None;
        let mut ssl_verify_client: Option<&str> = None;
        let mut ssl_client_certificate: Option<String> = None;
        let mut access: Vec<Value> = Vec::new();
        let mut return_value: Option<String> = None;
        let mut limit_conn: Vec<Value> = Vec::new();
        let mut limit_conn_dry_run: Option<bool> = None;
        let mut set_variables: Vec<StreamSetVariableConfig> = Vec::new();
        let mut preread_timeout_ms: Option<u64> = None;
        let mut preread_buffer_bytes: Option<usize> = None;
        // Variables a `set`/`return` template may reference: stream map
        // results plus earlier `set` assignments of this server.
        let mut available_variables: Vec<String> = self.stream_map_results.clone();
        for child in &directive.children {
            match child.name.as_str() {
                "listen" => {
                    if child.args.is_empty() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "stream listen requires an address",
                        ));
                    }
                    let spec = parse_stream_listen_spec(child)?;
                    listen_specs.push(spec);
                }
                "proxy_pass" => {
                    let Some(target) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "stream proxy_pass requires a target",
                        ));
                    };
                    proxy_pass = Some(target.clone());
                }
                "proxy_timeout" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "proxy_timeout requires a duration",
                        ));
                    };
                    let Some(seconds) = parse_nginx_time_seconds(value) else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("invalid proxy_timeout `{value}`"),
                        ));
                    };
                    proxy_timeout_ms = seconds.saturating_mul(1_000);
                }
                "proxy_protocol" => {
                    // nginx stream `proxy_protocol on|off|v2`: v1 PROXY
                    // protocol text header is sent to the upstream; v2
                    // (binary) cannot be emitted by the runtime.
                    let flag = child.args.first().map(String::as_str).unwrap_or("on");
                    match flag {
                        "on" => proxy_protocol = true,
                        "off" => proxy_protocol = false,
                        "v2" => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                "stream `proxy_protocol v2` (binary PROXY protocol) is not supported; use `on` (v1)",
                            ))
                        }
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!("proxy_protocol accepts on|off|v2, found `{other}`"),
                            ))
                        }
                    }
                }
                "ssl_preread" => {
                    let flag = child.args.first().map(String::as_str).unwrap_or("on");
                    ssl_preread = match flag {
                        "on" => true,
                        "off" => false,
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!("ssl_preread accepts on|off, found `{other}`"),
                            ))
                        }
                    };
                }
                "set" => {
                    let [name, value] = child.args.as_slice() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "stream `set` takes `set $variable value`",
                        ));
                    };
                    let Some(variable) = name.strip_prefix('$') else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("stream `set` target `{name}` must be a `$variable`"),
                        ));
                    };
                    crate::config::validate_stream_set(variable, value, &available_variables)
                        .map_err(|message| NginxConfigError::unsupported(child, message))?;
                    set_variables.push(StreamSetVariableConfig {
                        name: variable.to_owned(),
                        value: value.clone(),
                    });
                    available_variables.push(variable.to_owned());
                }
                "allow" | "deny" => access.push(parse_access_rule(child)?),
                "limit_conn" => {
                    let entry = child.args.join(" ");
                    let rule = parse_limit_conn(&entry).map_err(|error| {
                        NginxConfigError::unsupported(
                            child,
                            format!("invalid stream limit_conn: {error}"),
                        )
                    })?;
                    if !self.stream_limit_conn_zone_names.contains(&rule.zone) {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!(
                                "stream limit_conn references undefined zone `{}`",
                                rule.zone
                            ),
                        ));
                    }
                    limit_conn.push(json!({
                        "zone": rule.zone,
                        "maxConnections": rule.max_connections,
                    }));
                }
                "limit_conn_dry_run" => {
                    limit_conn_dry_run = Some(parse_on_off(child, "limit_conn_dry_run")?);
                }
                "preread_timeout" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "preread_timeout requires a duration",
                        ));
                    };
                    let Some(seconds) = parse_nginx_time_seconds(value) else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            format!("invalid preread_timeout `{value}`"),
                        ));
                    };
                    preread_timeout_ms = Some(seconds.saturating_mul(1000));
                }
                "preread_buffer_size" => {
                    let Some(bytes) = parse_body_size(child)? else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "preread_buffer_size requires a size",
                        ));
                    };
                    if !(512..=1024 * 1024).contains(&bytes) {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "preread_buffer_size must be between 512 bytes and 1 MiB",
                        ));
                    }
                    preread_buffer_bytes = Some(bytes as usize);
                }
                "proxy_protocol_timeout" => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "`proxy_protocol_timeout` bounds reading an inbound PROXY protocol header, which the runtime does not parse; the runtime emits PROXY protocol v1 to the upstream (`proxy_protocol on`) and has no inbound header to time out",
                    ));
                }
                "return" => {
                    // nginx stream `return <value>`: write the value to the
                    // connection and close it. The value may not use stream
                    // variables (no stream-variable engine in the runtime).
                    if child.args.len() != 1 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "stream `return` takes exactly one value",
                        ));
                    }
                    let value = &child.args[0];
                    crate::config::validate_stream_return(value, &available_variables)
                        .map_err(|message| NginxConfigError::unsupported(child, message))?;
                    return_value = Some(value.clone());
                }
                "ssl_certificate" => {
                    certificate_file = Some(self.resolve_path(child)?);
                }
                "ssl_certificate_key" => {
                    certificate_key = Some(self.resolve_path(child)?);
                }
                "ssl_verify_client" => {
                    let Some(value) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "ssl_verify_client requires on|optional|off",
                        ));
                    };
                    ssl_verify_client = match value.as_str() {
                        "on" => Some("required"),
                        "optional" => Some("optional"),
                        "off" => None,
                        other => {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!(
                                    "ssl_verify_client accepts on|optional|off, found `{other}`"
                                ),
                            ))
                        }
                    };
                }
                "ssl_client_certificate" => {
                    let Some(_path) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "ssl_client_certificate requires a CA file",
                        ));
                    };
                    ssl_client_certificate = Some(self.resolve_path(child)?);
                }
                "ssl_protocols"
                | "ssl_ciphers"
                | "ssl_prefer_server_ciphers"
                | "proxy_connect_timeout"
                | "proxy_socket_keepalive"
                | "so_keepalive"
                | "access_log"
                | "error_log" => {}
                name if ACCEPTED_IGNORED.contains(&name) => {}
                name => {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!("unsupported stream server directive `{name}`"),
                    ));
                }
            }
        }
        if listen_specs.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "stream server requires listen",
            ));
        }
        if access.is_empty() {
            access = self.stream_access.clone();
        }
        if proxy_pass.is_some() && return_value.is_some() {
            return Err(NginxConfigError::unsupported(
                directive,
                "a stream server cannot combine `return` with `proxy_pass`",
            ));
        }
        let target =
            if let Some(target_name) = proxy_pass.as_ref().filter(|_| return_value.is_none()) {
                Some(if self.upstream_names.contains(target_name) {
                    json!({ "type": "upstream", "name": target_name })
                } else if let Some((host, port_text)) = target_name.rsplit_once(':') {
                    let port = port_text.parse::<u16>().map_err(|_| {
                        NginxConfigError::unsupported(
                            directive,
                            format!("invalid stream proxy_pass `{target_name}`"),
                        )
                    })?;
                    json!({ "type": "literal", "host": host, "port": port })
                } else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                        "stream proxy_pass `{target_name}` must reference an upstream or host:port"
                    ),
                    ));
                })
            } else {
                None
            };
        if target.is_none() && return_value.is_none() {
            return Err(NginxConfigError::unsupported(
                directive,
                "stream server requires proxy_pass or return",
            ));
        }
        // nginx allows one `stream server` block with several `listen`
        // directives; the runtime materializes one stream server per listen.
        for (listen_index, listen) in listen_specs.iter().enumerate() {
            let udp = listen.udp;
            let ssl = listen.ssl;
            if udp && (ssl || ssl_preread || proxy_protocol) {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "UDP stream listeners cannot combine `udp` with `ssl`, `ssl_preread`, or `proxy_protocol`",
                ));
            }
            let mut stream = json!({
                "id": format!("stream-{index}-{listen_index}-{}", listen.port),
                "bind": listen.bind,
                "port": listen.port,
                "protocol": if udp { "udp" } else { "tcp" },
                "proxyTimeoutMs": proxy_timeout_ms,
                "proxyProtocol": proxy_protocol,
            });
            if let Some(target) = &target {
                stream["target"] = target.clone();
            }
            if !access.is_empty() {
                stream["access"] = Value::Array(access.clone());
            }
            if let Some(value) = &return_value {
                stream["returnValue"] = Value::String(value.clone());
            }
            if !limit_conn.is_empty() {
                stream["limitConn"] = Value::Array(limit_conn.clone());
            }
            if limit_conn_dry_run.unwrap_or(self.stream_limit_conn_dry_run) {
                stream["limitConnDryRun"] = Value::Bool(true);
            }
            stream["prereadTimeoutMs"] = json!(
                preread_timeout_ms.unwrap_or(self.stream_preread_timeout_ms.unwrap_or(30_000))
            );
            stream["prereadBufferBytes"] = json!(preread_buffer_bytes
                .unwrap_or(self.stream_preread_buffer_bytes.unwrap_or(16 * 1024)));
            if !set_variables.is_empty() {
                stream["setVariables"] = Value::Array(
                    set_variables
                        .iter()
                        .map(|set| json!({"name": set.name, "value": set.value}))
                        .collect(),
                );
            }
            if ssl_preread {
                stream["tls"] = json!({ "mode": "preread" });
            } else if ssl {
                let Some(certificate_file) = certificate_file.as_deref() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "stream listen … ssl requires ssl_certificate",
                    ));
                };
                let Some(certificate_key) = certificate_key.as_deref() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "stream listen … ssl requires ssl_certificate_key",
                    ));
                };
                // Global counter: ids stay unique when stream servers from
                // several site files are merged into one app.
                let certificate_name = format!("stream-cert-{}", certificate_name_counter());
                self.certificates.push(json!({
                    "id": certificate_name,
                    "serverNames": ["stream.local"],
                    "source": {
                        "type": "protected-file",
                        "certificateFile": certificate_file,
                        "privateKeyFile": certificate_key,
                    },
                }));
                let mut tls_entry = json!({
                    "mode": "terminate",
                    "certificateRef": certificate_name,
                });
                if let Some(mode) = ssl_verify_client {
                    let Some(ca_file) = ssl_client_certificate.as_deref() else {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            "ssl_verify_client requires ssl_client_certificate",
                        ));
                    };
                    tls_entry["clientAuth"] = json!({
                        "mode": mode,
                        "caCertificateFiles": [ca_file],
                    });
                } else if ssl_client_certificate.is_some() {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "ssl_client_certificate requires ssl_verify_client",
                    ));
                }
                stream["tls"] = tls_entry;
            }
            // Type-check against the runtime model early for clearer diagnostics.
            if let Some(target) = &target {
                let _: StreamTargetConfig =
                    serde_json::from_value(target.clone()).map_err(|error| {
                        NginxConfigError::unsupported(
                            directive,
                            format!("stream target materialization failed: {error}"),
                        )
                    })?;
            }
            if stream.get("tls").is_some() {
                let _: StreamTlsMode =
                    serde_json::from_value(stream["tls"].clone()).map_err(|error| {
                        NginxConfigError::unsupported(
                            directive,
                            format!("stream tls materialization failed: {error}"),
                        )
                    })?;
            }
            self.streams.push(stream);
        }
        Ok(())
    }

    /// Materialize the nginx `mail {}` context (mail proxy plane, Wave 9B).
    ///
    /// Executed: `auth_http`, `auth_http_header`, `auth_http_timeout`,
    /// `auth_http_pass_client_cert`, `ssl_certificate`,
    /// `ssl_certificate_key`, `ssl_protocols`, `starttls`, `xclient`,
    /// `proxy`, `proxy_buffer`, `proxy_timeout`, `proxy_pass_error_message`,
    /// `proxy_protocol`, `proxy_smtp_auth`, `max_errors`, and the Wave 9A
    /// listener/dialog knobs.
    ///
    /// Everything the runtime cannot honour fails closed with a located
    /// diagnostic: user-defined capability lists, mail realip, the mail
    /// resolver (only needed for SMTP PTR/forward-confirmed client host names
    /// and for `Auth-Server` host names, which the runtime resolves with its
    /// own bounded DNS policy), and the OpenSSL-only TLS knobs
    /// (`ssl_ciphers`, `ssl_conf_command`, `ssl_crl`, `ssl_dhparam`,
    /// `ssl_ecdh_curve`, `ssl_password_file`, `ssl_prefer_server_ciphers`,
    /// `ssl_session_*`, `ssl_certificate_compression`).
    fn materialize_mail_context(
        &mut self,
        directives: &[NginxDirective],
    ) -> Result<(), NginxConfigError> {
        let mut defaults = MailContextDefaults::default();
        // First pass: main-context directives are inherited by every server
        // that does not declare its own value (nginx mail merge semantics).
        for directive in directives {
            if directive.name == "server" {
                continue;
            }
            match directive.name.as_str() {
                "error_log" | "access_log" | "include" => {}
                _ => parse_mail_context_directive(self, directive, &mut defaults)?,
            }
        }
        let mut mail_index = 0usize;
        for directive in directives {
            if directive.name != "server" {
                continue;
            }
            self.materialize_mail_server(directive, mail_index, &defaults)?;
            mail_index += 1;
        }
        Ok(())
    }

    /// Materialize one `mail { server { … } }` block.
    fn materialize_mail_server(
        &mut self,
        directive: &NginxDirective,
        index: usize,
        defaults: &MailContextDefaults,
    ) -> Result<(), NginxConfigError> {
        let mut listen_specs: Vec<MailListenSpec> = Vec::new();
        let mut server: MailServerDraft = MailServerDraft::from_defaults(defaults);
        for child in &directive.children {
            if child.name == "listen" {
                listen_specs.push(parse_mail_listen_spec(child)?);
                continue;
            }
            parse_mail_server_directive(self, child, &mut server)?;
        }
        if listen_specs.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "mail server requires listen",
            ));
        }
        // nginx allows several `listen` directives in one mail server block;
        // the runtime materializes one mail listener per listen.
        let protocol = server.protocol.unwrap_or("imap");
        let auth_http_json = server.auth_http_json();
        let pop3_auth = normalize_mail_auth_mechanisms(directive, protocol, server.pop3_auth)?;
        let imap_auth = normalize_mail_auth_mechanisms(directive, protocol, server.imap_auth)?;
        let smtp_auth = normalize_mail_auth_mechanisms(directive, protocol, server.smtp_auth)?;
        for (listen_index, listen) in listen_specs.iter().enumerate() {
            let effective_proxy_protocol = server.inbound_proxy_protocol || listen.proxy_protocol;
            let effective_so_keepalive =
                listen.so_keepalive.or(server.so_keepalive).unwrap_or(false);
            let implicit_tls = listen.ssl;
            let mut tls = match (&server.tls_cert, &server.tls_key) {
                (Some(cert), Some(key)) => Some(json!({
                    "certificateFile": cert,
                    "privateKeyFile": key,
                    "protocols": server.tls_protocols.clone(),
                    "starttls": server.starttls.clone(),
                    "implicit": implicit_tls,
                })),
                (None, None) => None,
                _ => {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "mail TLS requires both `ssl_certificate` and `ssl_certificate_key`",
                    ))
                }
            };
            if implicit_tls && tls.is_none() {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "mail `listen … ssl` requires `ssl_certificate` and `ssl_certificate_key`",
                ));
            }
            if tls.is_none() && server.starttls != "off" {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "mail `starttls on|only` requires `ssl_certificate` and `ssl_certificate_key`",
                ));
            }
            if implicit_tls && server.starttls == "only" {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "mail `listen … ssl` and `starttls only` are mutually exclusive",
                ));
            }
            if let Some(tls) = tls.as_mut() {
                if !implicit_tls && server.starttls == "off" {
                    // TLS material is declared but unused by this listener;
                    // nginx accepts it (main-context certificate for a
                    // STARTTLS listener). Keep it for a later `starttls on` in
                    // the same server block: the value is not silently dropped.
                    let _ = tls;
                }
            }
            let mut entry = json!({
                "id": format!("mail-{index}-{listen_index}-{}", listen.port),
                "bind": listen.bind,
                "port": listen.port,
                "protocol": protocol,
                "timeoutMs": server.timeout_ms,
                "soKeepalive": effective_so_keepalive,
                "proxyProtocol": effective_proxy_protocol,
                "smtpClientBuffer": server.smtp_client_buffer.unwrap_or(4 * 1024),
                "imapClientBuffer": server.imap_client_buffer.unwrap_or(4 * 1024),
                "smtpGreetingDelayMs": server.smtp_greeting_delay_ms,
                "maxErrors": server.max_errors,
                "proxy": {
                    "enabled": server.proxy_enabled,
                    "xclient": server.xclient,
                    "smtpAuth": server.proxy_smtp_auth,
                    "proxyProtocol": server.outbound_proxy_protocol,
                    "passErrorMessage": server.pass_error_message,
                    "bufferBytes": server.proxy_buffer_bytes,
                    "timeoutMs": server.proxy_timeout_ms,
                },
            });
            if let Some(server_name) = &server.server_name {
                entry["serverName"] = Value::String(server_name.clone());
            }
            for (key, mechanisms) in [
                ("pop3Auth", &pop3_auth),
                ("imapAuth", &imap_auth),
                ("smtpAuth", &smtp_auth),
            ] {
                if let Some(mechanisms) = mechanisms {
                    entry[key] = Value::Array(mechanisms.clone());
                }
            }
            if let Some(auth_http) = &auth_http_json {
                entry["authHttp"] = auth_http.clone();
            }
            if let Some(tls) = tls {
                entry["tls"] = tls;
            }
            if let Some(proxy_tls) = &server.proxy_tls {
                entry["proxy"]["tls"] = proxy_tls.clone();
            }
            // Type-check against the runtime model early for clearer
            // diagnostics (deny_unknown_fields catches an unmodeled key here
            // rather than in the final app materialization).
            let _: crate::config::MailServerConfig = serde_json::from_value(entry.clone())
                .map_err(|error| {
                    NginxConfigError::unsupported(
                        directive,
                        format!("mail server materialization failed: {error}"),
                    )
                })?;
            self.mail_servers.push(entry);
        }
        Ok(())
    }

    fn finish(self) -> Result<WebServerAppConfig, NginxConfigError> {
        let mut proxy_cache = json!({
            "enabled": self.proxy_cache_enabled,
            "maxEntries": self.proxy_cache_max_entries,
            "maxObjectBytes": 1048576,
            "defaultTtlSeconds": self.proxy_cache_ttl_seconds,
            "staleTtlSeconds": self.proxy_cache_stale_ttl_seconds,
        });
        if let Some(disk_path) = self.proxy_cache_disk_path {
            proxy_cache["diskPath"] = Value::String(disk_path);
        }
        let mut nginx_section = json!({
            "enabled": true,
            "profile": "http-core-v1",
            "unknownDirectivePolicy": "error",
            "nativeCompatibility": true,
        });
        if !self.document_root_allowlist.is_empty() {
            nginx_section["documentRootAllowlist"] = Value::Array(
                self.document_root_allowlist
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            );
        }
        let instance = json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": self.app_key,
            "nginx": nginx_section,
            "maps": self.maps,
            "geo": self.geo,
            "splitClients": self.split_clients,
            "mimeTypes": self.mime_types,
            "defaultType": self.default_type,
            "gzip": {
                "enabled": self.gzip_enabled,
                "types": self.gzip_types,
                "minLength": self.gzip_min_length,
                "vary": self.gzip_vary,
                "staticFiles": self.gzip_static,
            },
            "limitReqZones": self.limit_req_zones,
            "limitConnZones": self.limit_conn_zones,
            "listeners": self.listeners,
            "certificates": self.certificates,
            "tlsPolicies": self.tls_policies,
            "resources": self.resources,
            "upstreams": self.upstreams,
            "virtualHosts": self.virtual_hosts,
            "streams": self.streams,
            "mail": { "servers": self.mail_servers },
            "streamLimitConnZones": self.stream_limit_conn_zones,
            "streamMaps": self.stream_maps,
            "proxyCache": proxy_cache,
            "metadata": { "source": "nginx configuration" },
        });
        let mut instance = instance;
        if self.http2_max_concurrent_streams_conflict {
            return Err(NginxConfigError::unsupported(
                &NginxDirective {
                    name: "http2_max_concurrent_streams".to_owned(),
                    args: Vec::new(),
                    children: Vec::new(),
                    line: 0,
                    source: self.base_dir.to_path_buf(),
                },
                "conflicting `http2_max_concurrent_streams` values at different levels; the runtime ingress model supports one global value — declare a single value",
            ));
        }
        let mut limits = serde_json::Map::new();
        if let Some(bytes) = self.client_max_body_size {
            limits.insert("maxRequestBodyBytes".to_owned(), json!(bytes));
        }
        if let Some(streams) = self.http2_max_concurrent_streams {
            limits.insert("http2MaxConcurrentStreams".to_owned(), json!(streams));
        }
        if !limits.is_empty() {
            instance["limits"] = Value::Object(limits);
        }
        let config: WebServerAppConfig = serde_json::from_value(instance).map_err(|source| {
            NginxConfigError::Config(WebServerConfigError::Materialize(format!(
                "nginx materialization failed: {source}"
            )))
        })?;
        crate::config::validate_webserver_config(&config).map_err(|error| {
            let diagnostics = error
                .diagnostics()
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.path, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ");
            NginxConfigError::ValidationFailed(diagnostics)
        })?;
        Ok(config)
    }
}

/// One parsed `listen` entry. Supported forms (nginx http module): `80`,
/// `127.0.0.1:8080`, `[::]:80`, `[::1]:8080`, a bare address without a port
/// (nginx defaults to port 80), and the flags `ssl`, `http2`,
/// `default_server`, and `reuseport` (runtime-owned OS knob).
#[derive(Debug, Clone)]
struct ListenSpec {
    bind: String,
    port: u16,
    ssl: bool,
    http2: bool,
    default_server: bool,
}

fn parse_listen_spec(spec: &str) -> Result<ListenSpec, String> {
    let mut parts = spec.split_whitespace();
    let address = parts
        .next()
        .ok_or_else(|| "listen entry is empty".to_owned())?;
    let mut ssl = false;
    let mut http2 = false;
    let mut default_server = false;
    for part in parts {
        match part {
            "ssl" => ssl = true,
            "http2" => http2 = true,
            "default_server" => default_server = true,
            // OS socket knobs the runtime owns (nginx accepts them on
            // `listen`); values are not enforced.
            "reuseport" | "bind" | "deferred" => {}
            other
                if other.starts_with("backlog=")
                    || other.starts_with("so_keepalive=")
                    || other.starts_with("ipv6only=")
                    || other.starts_with("fastopen=")
                    || other.starts_with("accept_filter=")
                    || other.starts_with("setfib=")
                    || other.starts_with("rcvbuf=")
                    || other.starts_with("sndbuf=") => {}
            "proxy_protocol" => {
                return Err(
                    "listen parameter `proxy_protocol` requires explicit trusted source CIDRs; configure the listener proxyProtocol policy in the runtime app config".to_owned(),
                );
            }
            other => {
                return Err(format!(
                    "unsupported listen parameter `{other}` in `{spec}`"
                ));
            }
        }
    }
    let (bind, port) = parse_listen_address(address, spec)?;
    Ok(ListenSpec {
        bind,
        port,
        ssl,
        http2,
        default_server,
    })
}

/// Location/server/http-level `proxy_ssl_*` settings for one proxied
/// surface (nginx `proxy_ssl_verify`, `proxy_ssl_trusted_certificate`,
/// `proxy_ssl_certificate(_key)`, `proxy_ssl_server_name`). Fields inherit
/// independently per directive: a location value overrides the server value,
/// which overrides the http value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ProxySslSettings {
    verify: Option<bool>,
    trusted_certificate: Option<String>,
    certificate: Option<String>,
    certificate_key: Option<String>,
    server_name: Option<bool>,
}

impl ProxySslSettings {
    fn merge(&self, lower: &ProxySslSettings) -> ProxySslSettings {
        ProxySslSettings {
            verify: self.verify.or(lower.verify),
            trusted_certificate: self
                .trusted_certificate
                .clone()
                .or_else(|| lower.trusted_certificate.clone()),
            certificate: self
                .certificate
                .clone()
                .or_else(|| lower.certificate.clone()),
            certificate_key: self
                .certificate_key
                .clone()
                .or_else(|| lower.certificate_key.clone()),
            server_name: self.server_name.or(lower.server_name),
        }
    }

    fn is_empty(&self) -> bool {
        self == &ProxySslSettings::default()
    }
}

/// nginx `grpc_*` settings accumulated over the http → server → location
/// inheritance chain (the module follows the proxy-family merge rules: a level
/// that declares `grpc_set_header` replaces the inherited list, single-value
/// directives override per directive, hide/pass lists append).
#[derive(Clone, Default)]
struct GrpcSettings {
    set_headers: Option<Vec<String>>,
    connect_timeout_ms: Option<u64>,
    send_timeout_ms: Option<u64>,
    read_timeout_ms: Option<u64>,
    buffer_size: Option<u64>,
    intercept_errors: Option<bool>,
    hide_headers: Option<Vec<String>>,
    pass_headers: Option<Vec<String>>,
    next_upstream: Option<Vec<UpstreamRetryCondition>>,
    next_upstream_tries: Option<u8>,
    next_upstream_timeout_ms: Option<u64>,
}

impl GrpcSettings {
    fn overlay(&self, inner: &Self) -> Self {
        Self {
            set_headers: inner
                .set_headers
                .clone()
                .or_else(|| self.set_headers.clone()),
            connect_timeout_ms: inner.connect_timeout_ms.or(self.connect_timeout_ms),
            send_timeout_ms: inner.send_timeout_ms.or(self.send_timeout_ms),
            read_timeout_ms: inner.read_timeout_ms.or(self.read_timeout_ms),
            buffer_size: inner.buffer_size.or(self.buffer_size),
            intercept_errors: inner.intercept_errors.or(self.intercept_errors),
            hide_headers: inner
                .hide_headers
                .clone()
                .or_else(|| self.hide_headers.clone()),
            pass_headers: inner
                .pass_headers
                .clone()
                .or_else(|| self.pass_headers.clone()),
            next_upstream: inner
                .next_upstream
                .clone()
                .or_else(|| self.next_upstream.clone()),
            next_upstream_tries: inner.next_upstream_tries.or(self.next_upstream_tries),
            next_upstream_timeout_ms: inner
                .next_upstream_timeout_ms
                .or(self.next_upstream_timeout_ms),
        }
    }

    fn apply(
        &mut self,
        directive: &NginxDirective,
        map_variables: &[&str],
    ) -> Result<(), NginxConfigError> {
        match directive.name.as_str() {
            "grpc_set_header" => {
                let entry =
                    format_proxy_set_header_entry_with_maps(&directive.args, map_variables)
                        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
                self.set_headers.get_or_insert_with(Vec::new).push(entry);
            }
            "grpc_connect_timeout" => {
                self.connect_timeout_ms = Some(parse_fastcgi_timeout(directive)?)
            }
            "grpc_send_timeout" => self.send_timeout_ms = Some(parse_fastcgi_timeout(directive)?),
            "grpc_read_timeout" => self.read_timeout_ms = Some(parse_fastcgi_timeout(directive)?),
            "grpc_buffer_size" => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "grpc_buffer_size requires a size",
                    ));
                };
                let Some(bytes) = parse_size_bytes(value) else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("invalid grpc_buffer_size `{value}`"),
                    ));
                };
                if bytes == 0 || bytes > 1_048_576 {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "grpc_buffer_size must be in 1..=1048576 bytes",
                    ));
                }
                self.buffer_size = Some(bytes);
            }
            "grpc_intercept_errors" => {
                self.intercept_errors = Some(parse_on_off(directive, "grpc_intercept_errors")?)
            }
            "grpc_hide_header" | "grpc_pass_header" => {
                let Some(name) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("{} requires a header name", directive.name),
                    ));
                };
                if !crate::config::header_block_token(name) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                            "{} `{name}` is not a valid HTTP field name",
                            directive.name
                        ),
                    ));
                }
                let list = if directive.name == "grpc_hide_header" {
                    self.hide_headers.get_or_insert_with(Vec::new)
                } else {
                    self.pass_headers.get_or_insert_with(Vec::new)
                };
                list.push(name.clone());
            }
            "grpc_next_upstream" => {
                self.next_upstream = Some(parse_fastcgi_next_upstream(directive)?);
            }
            "grpc_next_upstream_tries" => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "grpc_next_upstream_tries requires a count",
                    ));
                };
                let tries = value.parse::<u8>().map_err(|_| {
                    NginxConfigError::unsupported(
                        directive,
                        format!("invalid grpc_next_upstream_tries `{value}`"),
                    )
                })?;
                self.next_upstream_tries = Some(tries);
            }
            "grpc_next_upstream_timeout" => {
                self.next_upstream_timeout_ms = Some(parse_fastcgi_timeout(directive)?)
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{other}` is not executable by the bounded gRPC adapter; supported: grpc_pass/grpc_set_header/grpc_connect_timeout/grpc_send_timeout/grpc_read_timeout/grpc_buffer_size/grpc_hide_header/grpc_pass_header/grpc_intercept_errors/grpc_next_upstream/_tries/_timeout/grpc_ssl_*"
                    ),
                ))
            }
        }
        Ok(())
    }
}

/// nginx `grpc_ssl_*` upstream TLS settings (the gRPC module's TLS surface is a
/// superset of `proxy_ssl_*`: it also accepts a verification depth and a custom
/// SNI/verification name).
#[derive(Clone, Default, PartialEq)]
struct GrpcSslSettings {
    verify: Option<bool>,
    verify_depth: Option<u32>,
    trusted_certificate: Option<String>,
    certificate: Option<String>,
    certificate_key: Option<String>,
    server_name: Option<bool>,
    name: Option<String>,
    protocols: Option<SslProtocolRange>,
}

impl GrpcSslSettings {
    fn merge(&self, lower: &GrpcSslSettings) -> GrpcSslSettings {
        GrpcSslSettings {
            verify: self.verify.or(lower.verify),
            verify_depth: self.verify_depth.or(lower.verify_depth),
            trusted_certificate: self
                .trusted_certificate
                .clone()
                .or_else(|| lower.trusted_certificate.clone()),
            certificate: self
                .certificate
                .clone()
                .or_else(|| lower.certificate.clone()),
            certificate_key: self
                .certificate_key
                .clone()
                .or_else(|| lower.certificate_key.clone()),
            server_name: self.server_name.or(lower.server_name),
            name: self.name.clone().or_else(|| lower.name.clone()),
            protocols: self.protocols.or(lower.protocols),
        }
    }

    fn is_empty(&self) -> bool {
        self == &GrpcSslSettings::default()
    }

    fn apply(&mut self, directive: &NginxDirective) -> Result<(), NginxConfigError> {
        let Some(value) = directive.args.first() else {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("{} requires an argument", directive.name),
            ));
        };
        let resolve = |directive: &NginxDirective| -> Result<String, NginxConfigError> {
            let value = directive.args.first().expect("checked argument presence");
            if value.starts_with('/') || value.contains('\\') {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{}` file `{value}` must be relative to the loaded configuration directory: the runtime sandboxes upstream TLS material to the config directory",
                        directive.name
                    ),
                ));
            }
            Ok(value.clone())
        };
        match directive.name.as_str() {
            "grpc_ssl_verify" => match value.as_str() {
                "on" => self.verify = Some(true),
                "off" => self.verify = Some(false),
                other => {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("grpc_ssl_verify accepts on|off, found `{other}`"),
                    ))
                }
            },
            "grpc_ssl_verify_depth" => {
                let depth = value.parse::<u32>().map_err(|_| {
                    NginxConfigError::unsupported(
                        directive,
                        format!("invalid grpc_ssl_verify_depth `{value}`"),
                    )
                })?;
                if depth == 0 || depth > 16 {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "grpc_ssl_verify_depth must be in 1..=16; the runtime verifies the full chain",
                    ));
                }
                self.verify_depth = Some(depth);
            }
            "grpc_ssl_trusted_certificate" => self.trusted_certificate = Some(resolve(directive)?),
            "grpc_ssl_certificate" => self.certificate = Some(resolve(directive)?),
            "grpc_ssl_certificate_key" => self.certificate_key = Some(resolve(directive)?),
            "grpc_ssl_server_name" => match value.as_str() {
                "on" => self.server_name = Some(true),
                "off" => self.server_name = Some(false),
                other => {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("grpc_ssl_server_name accepts on|off, found `{other}`"),
                    ))
                }
            },
            "grpc_ssl_name" => self.name = Some(value.clone()),
            "grpc_ssl_protocols" => self.protocols = Some(parse_ssl_protocols(directive)?),
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{other}` cannot be enforced by the bounded gRPC TLS client; supported: grpc_ssl_verify/grpc_ssl_verify_depth/grpc_ssl_trusted_certificate/grpc_ssl_certificate/grpc_ssl_certificate_key/grpc_ssl_server_name/grpc_ssl_name/grpc_ssl_protocols"
                    ),
                ))
            }
        }
        Ok(())
    }
}

/// Server/http-level `set_real_ip_from` / `real_ip_header` /
/// `real_ip_recursive` settings (nginx realip module) attached to the
/// server's listeners as `trustedProxy`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RealIpSettings {
    trusted_cidrs: Vec<String>,
    recursive: bool,
}

/// One parsed stream `listen` entry (nginx stream core module). Supported
/// flags: `ssl`, `udp`, and the runtime-owned OS knobs `reuseport`, `bind`,
/// `backlog=`, `so_keepalive=`, `ipv6only=`.
#[derive(Debug, Clone)]
struct StreamListenSpec {
    bind: String,
    port: u16,
    ssl: bool,
    udp: bool,
}

fn parse_stream_listen_spec(
    directive: &NginxDirective,
) -> Result<StreamListenSpec, NginxConfigError> {
    let mut parts = directive.args.iter();
    let address = parts.next().ok_or_else(|| {
        NginxConfigError::unsupported(directive, "stream listen requires an address")
    })?;
    let mut ssl = false;
    let mut udp = false;
    for part in parts {
        match part.as_str() {
            "ssl" => ssl = true,
            "udp" => udp = true,
            "reuseport" | "bind" | "deferred" => {}
            "proxy_protocol" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "inbound PROXY protocol on stream listeners requires explicit trusted source CIDRs; configure the stream listener policy in the runtime app config",
                ));
            }
            other
                if other.starts_with("backlog=")
                    || other.starts_with("so_keepalive=")
                    || other.starts_with("ipv6only=")
                    || other.starts_with("fastopen=")
                    || other.starts_with("accept_filter=")
                    || other.starts_with("setfib=")
                    || other.starts_with("rcvbuf=")
                    || other.starts_with("sndbuf=") => {}
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("unsupported stream listen parameter `{other}`"),
                ));
            }
        }
    }
    let (bind, port) = parse_listen_address(address, &directive.args.join(" "))
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    Ok(StreamListenSpec {
        bind,
        port,
        ssl,
        udp,
    })
}

/// Parse the `listen` address part. nginx accepts `port`, `address:port`,
/// `[ipv6]:port`, and a bare address without a port (defaults to 80 for the
/// http module).
fn parse_listen_address(address: &str, spec: &str) -> Result<(String, u16), String> {
    if let Ok(port) = address.parse::<u16>() {
        return Ok(("0.0.0.0".to_owned(), port));
    }
    if let Some(rest) = address.strip_prefix('[') {
        let (host, port) = rest
            .split_once(']')
            .ok_or_else(|| format!("invalid listen address `{spec}`"))?;
        let port = match port.strip_prefix(':') {
            Some(port_text) => port_text
                .parse::<u16>()
                .map_err(|_| format!("invalid listen port in `{spec}`"))?,
            None => 80,
        };
        return Ok((host.to_owned(), port));
    }
    if address == "::" {
        return Ok(("::".to_owned(), 80));
    }
    if let Some((host, port_text)) = address.rsplit_once(':') {
        let port = port_text
            .parse::<u16>()
            .map_err(|_| format!("invalid listen port in `{spec}`"))?;
        let normalized = match host {
            "" | "0.0.0.0" | "*" => "0.0.0.0".to_owned(),
            other => other.to_owned(),
        };
        return Ok((normalized, port));
    }
    // A bare address without a port (nginx http default: port 80).
    Ok((address.to_owned(), 80))
}

/// Main-context nginx mail directives inherited by every server block.
///
/// nginx's mail module merges main-context `auth_http*`, `ssl_*`, `proxy_*`,
/// `xclient` and `max_errors` into each server before applying the server's own
/// declarations. This struct holds those resolved defaults.
#[derive(Clone)]
struct MailContextDefaults {
    server_name: Option<String>,
    timeout_ms: u64,
    auth_uri: Option<String>,
    auth_host: Option<String>,
    auth_headers: Vec<String>,
    auth_timeout_ms: u64,
    auth_pass_client_cert: bool,
    tls_cert: Option<String>,
    tls_key: Option<String>,
    tls_protocols: Vec<String>,
    starttls: String,
    proxy_enabled: bool,
    xclient: bool,
    proxy_smtp_auth: bool,
    outbound_proxy_protocol: bool,
    pass_error_message: bool,
    proxy_buffer_bytes: usize,
    proxy_timeout_ms: u64,
    max_errors: u32,
}

impl Default for MailContextDefaults {
    fn default() -> Self {
        Self {
            server_name: None,
            timeout_ms: 60_000,
            auth_uri: None,
            auth_host: None,
            auth_headers: Vec::new(),
            auth_timeout_ms: 60_000,
            auth_pass_client_cert: false,
            tls_cert: None,
            tls_key: None,
            tls_protocols: default_mail_tls_protocols(),
            starttls: "off".to_owned(),
            // nginx mail proxy module defaults (ngx_mail_proxy_module merge).
            proxy_enabled: true,
            xclient: true,
            proxy_smtp_auth: false,
            outbound_proxy_protocol: false,
            pass_error_message: false,
            proxy_buffer_bytes: 4096,
            proxy_timeout_ms: 24 * 60 * 60 * 1000,
            max_errors: 5,
        }
    }
}

/// One mail server's resolved configuration while directives are read.
struct MailServerDraft {
    protocol: Option<&'static str>,
    server_name: Option<String>,
    timeout_ms: u64,
    so_keepalive: Option<bool>,
    inbound_proxy_protocol: bool,
    smtp_client_buffer: Option<usize>,
    imap_client_buffer: Option<usize>,
    pop3_auth: Option<Vec<String>>,
    imap_auth: Option<Vec<String>>,
    smtp_auth: Option<Vec<String>>,
    smtp_greeting_delay_ms: u64,
    auth_uri: Option<String>,
    auth_host: Option<String>,
    auth_headers: Vec<String>,
    auth_headers_declared: bool,
    auth_timeout_ms: u64,
    auth_pass_client_cert: bool,
    tls_cert: Option<String>,
    tls_key: Option<String>,
    tls_protocols: Vec<String>,
    starttls: String,
    proxy_enabled: bool,
    xclient: bool,
    proxy_smtp_auth: bool,
    outbound_proxy_protocol: bool,
    pass_error_message: bool,
    proxy_buffer_bytes: usize,
    proxy_timeout_ms: u64,
    max_errors: u32,
    proxy_tls: Option<Value>,
}

impl MailServerDraft {
    fn from_defaults(defaults: &MailContextDefaults) -> Self {
        Self {
            protocol: None,
            server_name: defaults.server_name.clone(),
            timeout_ms: defaults.timeout_ms,
            so_keepalive: None,
            inbound_proxy_protocol: false,
            smtp_client_buffer: None,
            imap_client_buffer: None,
            pop3_auth: None,
            imap_auth: None,
            smtp_auth: None,
            smtp_greeting_delay_ms: 0,
            auth_uri: defaults.auth_uri.clone(),
            auth_host: defaults.auth_host.clone(),
            auth_headers: defaults.auth_headers.clone(),
            auth_headers_declared: false,
            auth_timeout_ms: defaults.auth_timeout_ms,
            auth_pass_client_cert: defaults.auth_pass_client_cert,
            tls_cert: defaults.tls_cert.clone(),
            tls_key: defaults.tls_key.clone(),
            tls_protocols: defaults.tls_protocols.clone(),
            starttls: defaults.starttls.clone(),
            proxy_enabled: defaults.proxy_enabled,
            xclient: defaults.xclient,
            proxy_smtp_auth: defaults.proxy_smtp_auth,
            outbound_proxy_protocol: defaults.outbound_proxy_protocol,
            pass_error_message: defaults.pass_error_message,
            proxy_buffer_bytes: defaults.proxy_buffer_bytes,
            proxy_timeout_ms: defaults.proxy_timeout_ms,
            max_errors: defaults.max_errors,
            proxy_tls: None,
        }
    }

    /// The resolved `auth_http` JSON value, if the service is configured.
    fn auth_http_json(&self) -> Option<Value> {
        let uri = self.auth_uri.as_ref()?;
        let mut value = json!({
            "uri": uri,
            "headers": self.auth_headers,
            "timeoutMs": self.auth_timeout_ms,
            "passClientCert": self.auth_pass_client_cert,
        });
        if let Some(host_header) = &self.auth_host {
            value["hostHeader"] = Value::String(host_header.clone());
        }
        Some(value)
    }
}

fn default_mail_tls_protocols() -> Vec<String> {
    vec!["TLSv1.2".to_owned(), "TLSv1.3".to_owned()]
}

/// Parse a main-context mail directive into the inherited defaults.
fn parse_mail_context_directive(
    mapper: &Mapper<'_>,
    directive: &NginxDirective,
    defaults: &mut MailContextDefaults,
) -> Result<(), NginxConfigError> {
    match directive.name.as_str() {
        "server_name" => {
            let [name] = directive.args.as_slice() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "mail `server_name` takes exactly one name",
                ));
            };
            defaults.server_name = Some(name.clone());
            Ok(())
        }
        "timeout" => {
            defaults.timeout_ms = parse_mail_timeout(directive)?;
            Ok(())
        }
        "auth_http" => parse_mail_auth_http(directive, &mut defaults.auth_uri),
        "auth_http_timeout" => {
            defaults.auth_timeout_ms = parse_mail_auth_http_timeout(directive)?;
            Ok(())
        }
        "auth_http_header" => parse_mail_auth_http_header(directive, &mut defaults.auth_headers),
        "auth_http_pass_client_cert" => {
            defaults.auth_pass_client_cert = parse_on_off(directive, "auth_http_pass_client_cert")?;
            Ok(())
        }
        "ssl_certificate" => {
            let path = parse_mail_single_path(mapper, directive)?;
            defaults.tls_cert = Some(path);
            Ok(())
        }
        "ssl_certificate_key" => {
            let path = parse_mail_single_path(mapper, directive)?;
            defaults.tls_key = Some(path);
            Ok(())
        }
        "ssl_protocols" => {
            defaults.tls_protocols = parse_mail_ssl_protocols(directive)?;
            Ok(())
        }
        "starttls" => {
            defaults.starttls = parse_mail_starttls(directive)?;
            Ok(())
        }
        "proxy" => {
            defaults.proxy_enabled = parse_on_off(directive, "proxy")?;
            Ok(())
        }
        "proxy_buffer" => {
            defaults.proxy_buffer_bytes = parse_mail_proxy_buffer(directive)?;
            Ok(())
        }
        "proxy_timeout" => {
            defaults.proxy_timeout_ms = parse_mail_proxy_timeout(directive)?;
            Ok(())
        }
        "proxy_pass_error_message" => {
            defaults.pass_error_message = parse_on_off(directive, "proxy_pass_error_message")?;
            Ok(())
        }
        "proxy_protocol" => {
            defaults.outbound_proxy_protocol = parse_on_off(directive, "proxy_protocol")?;
            Ok(())
        }
        "proxy_smtp_auth" => {
            defaults.proxy_smtp_auth = parse_on_off(directive, "proxy_smtp_auth")?;
            Ok(())
        }
        "xclient" => {
            defaults.xclient = parse_on_off(directive, "xclient")?;
            Ok(())
        }
        "max_errors" => {
            defaults.max_errors = parse_mail_max_errors(directive)?;
            Ok(())
        }
        name if ACCEPTED_IGNORED.contains(&name) => Ok(()),
        name => Err(NginxConfigError::unsupported(
            directive,
            format!("unsupported mail context directive `{name}`"),
        )),
    }
}

/// Parse a server-context mail directive into the draft.
fn parse_mail_server_directive(
    mapper: &Mapper<'_>,
    directive: &NginxDirective,
    server: &mut MailServerDraft,
) -> Result<(), NginxConfigError> {
    match directive.name.as_str() {
        "protocol" => {
            let Some(value) = directive.args.first() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "mail `protocol` requires pop3|imap|smtp",
                ));
            };
            server.protocol = Some(match value.as_str() {
                "pop3" => "pop3",
                "imap" => "imap",
                "smtp" => "smtp",
                other => {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("mail `protocol` accepts pop3|imap|smtp, found `{other}`"),
                    ))
                }
            });
            Ok(())
        }
        "server_name" => {
            let [name] = directive.args.as_slice() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "mail `server_name` takes exactly one name",
                ));
            };
            server.server_name = Some(name.clone());
            Ok(())
        }
        "timeout" => {
            server.timeout_ms = parse_mail_timeout(directive)?;
            Ok(())
        }
        "so_keepalive" => {
            server.so_keepalive = Some(parse_mail_so_keepalive(directive)?);
            Ok(())
        }
        "proxy_protocol" => {
            // nginx mail server-context `proxy_protocol` sends a PROXY header
            // to the backend; inbound PROXY acceptance is the
            // `listen … proxy_protocol` parameter.
            server.outbound_proxy_protocol = parse_on_off(directive, "proxy_protocol")?;
            Ok(())
        }
        "smtp_client_buffer" => {
            server.smtp_client_buffer = Some(parse_mail_client_buffer(directive)?);
            Ok(())
        }
        "imap_client_buffer" => {
            server.imap_client_buffer = Some(parse_mail_client_buffer(directive)?);
            Ok(())
        }
        "pop3_auth" => {
            server.pop3_auth = Some(require_mail_auth_args(directive)?);
            Ok(())
        }
        "imap_auth" => {
            server.imap_auth = Some(require_mail_auth_args(directive)?);
            Ok(())
        }
        "smtp_auth" => {
            server.smtp_auth = Some(require_mail_auth_args(directive)?);
            Ok(())
        }
        "smtp_greeting_delay" => {
            server.smtp_greeting_delay_ms = parse_mail_greeting_delay(directive)?;
            Ok(())
        }
        "auth_http" => parse_mail_auth_http(directive, &mut server.auth_uri),
        "auth_http_timeout" => {
            server.auth_timeout_ms = parse_mail_auth_http_timeout(directive)?;
            Ok(())
        }
        "auth_http_header" => {
            if !server.auth_headers_declared {
                server.auth_headers.clear();
                server.auth_headers_declared = true;
            }
            parse_mail_auth_http_header(directive, &mut server.auth_headers)
        }
        "auth_http_pass_client_cert" => {
            server.auth_pass_client_cert = parse_on_off(directive, "auth_http_pass_client_cert")?;
            Ok(())
        }
        "ssl_certificate" => {
            server.tls_cert = Some(parse_mail_single_path(mapper, directive)?);
            Ok(())
        }
        "ssl_certificate_key" => {
            server.tls_key = Some(parse_mail_single_path(mapper, directive)?);
            Ok(())
        }
        "ssl_protocols" => {
            server.tls_protocols = parse_mail_ssl_protocols(directive)?;
            Ok(())
        }
        "starttls" => {
            server.starttls = parse_mail_starttls(directive)?;
            Ok(())
        }
        "proxy" => {
            server.proxy_enabled = parse_on_off(directive, "proxy")?;
            Ok(())
        }
        "proxy_buffer" => {
            server.proxy_buffer_bytes = parse_mail_proxy_buffer(directive)?;
            Ok(())
        }
        "proxy_timeout" => {
            server.proxy_timeout_ms = parse_mail_proxy_timeout(directive)?;
            Ok(())
        }
        "proxy_pass_error_message" => {
            server.pass_error_message = parse_on_off(directive, "proxy_pass_error_message")?;
            Ok(())
        }
        "proxy_smtp_auth" => {
            server.proxy_smtp_auth = parse_on_off(directive, "proxy_smtp_auth")?;
            Ok(())
        }
        "xclient" => {
            server.xclient = parse_on_off(directive, "xclient")?;
            Ok(())
        }
        "max_errors" => {
            server.max_errors = parse_mail_max_errors(directive)?;
            Ok(())
        }
        "pop3_capabilities" | "imap_capabilities" | "smtp_capabilities" => {
            Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "mail `{}` (custom capability advertisement) is not executable; the runtime advertises nginx's default capability set",
                    directive.name
                ),
            ))
        }
        "ssl_client_certificate" | "ssl_verify_client" | "ssl_verify_depth" => {
            Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "mail `{}` (client certificate verification) is not executable yet; client-cert auth is not modeled in the mail plane",
                    directive.name
                ),
            ))
        }
        "ssl_ciphers"
        | "ssl_conf_command"
        | "ssl_crl"
        | "ssl_dhparam"
        | "ssl_ecdh_curve"
        | "ssl_password_file"
        | "ssl_prefer_server_ciphers"
        | "ssl_session_cache"
        | "ssl_session_timeout"
        | "ssl_session_tickets"
        | "ssl_session_ticket_key"
        | "ssl_certificate_compression" => Err(NginxConfigError::unsupported(
            directive,
            format!(
                "mail `{}` is not executable on the rustls TLS stack; the runtime pins TLSv1.2/1.3 and its own cipher/session policy",
                directive.name
            ),
        )),
        "resolver" | "resolver_timeout" => Err(NginxConfigError::unsupported(
            directive,
            format!(
                "mail `{}` (SMTP client-host PTR/forward-confirmed resolution) is not executable; the runtime resolves the `Auth-Server` backend with its own bounded DNS policy",
                directive.name
            ),
        )),
        "set_real_ip_from" => Err(NginxConfigError::unsupported(
            directive,
            "mail `set_real_ip_from` (mail realip module) is not executable",
        )),
        name if ACCEPTED_IGNORED.contains(&name) => Ok(()),
        name => Err(NginxConfigError::unsupported(
            directive,
            format!("unsupported mail server directive `{name}`"),
        )),
    }
}

fn parse_mail_auth_http(
    directive: &NginxDirective,
    target: &mut Option<String>,
) -> Result<(), NginxConfigError> {
    let Some(uri) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `auth_http` requires a service URL",
        ));
    };
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `auth_http` takes exactly one URL",
        ));
    }
    let uri = uri.strip_prefix("http://").unwrap_or(uri);
    if uri.trim().is_empty() || uri.contains(char::is_whitespace) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid mail auth_http URL `{uri}`"),
        ));
    }
    *target = Some(uri.to_owned());
    Ok(())
}

fn parse_mail_auth_http_header(
    directive: &NginxDirective,
    headers: &mut Vec<String>,
) -> Result<(), NginxConfigError> {
    if directive.args.len() != 2 {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `auth_http_header` requires a name and a value",
        ));
    }
    let name = &directive.args[0];
    let value = &directive.args[1];
    let valid_name = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid_name || value.contains(['\r', '\n']) {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `auth_http_header` requires a token name and a single-line value",
        ));
    }
    headers.push(format!("{name}: {value}"));
    Ok(())
}

fn parse_mail_auth_http_timeout(directive: &NginxDirective) -> Result<u64, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `auth_http_timeout` requires a time value",
        ));
    };
    let Some(seconds) = parse_nginx_time_seconds(value) else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid auth_http_timeout `{value}`"),
        ));
    };
    let millis = seconds.saturating_mul(1_000);
    if !(1..=600_000).contains(&millis) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("auth_http_timeout `{value}` is outside 1ms..=10m"),
        ));
    }
    Ok(millis)
}

fn parse_mail_single_path(
    mapper: &Mapper<'_>,
    directive: &NginxDirective,
) -> Result<String, NginxConfigError> {
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("mail `{}` takes exactly one file", directive.name),
        ));
    }
    mapper.resolve_path(directive)
}

fn parse_mail_ssl_protocols(directive: &NginxDirective) -> Result<Vec<String>, NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `ssl_protocols` requires at least one protocol token",
        ));
    }
    let mut protocols = Vec::new();
    for token in &directive.args {
        match token.as_str() {
            "TLSv1.2" | "TLSv1.3" => {
                if !protocols.contains(token) {
                    protocols.push(token.clone());
                }
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "mail `ssl_protocols` token `{other}` is not executable; the rustls stack pins TLSv1.2/TLSv1.3 (NGINX_SPEC section 3)"
                    ),
                ))
            }
        }
    }
    Ok(protocols)
}

fn parse_mail_starttls(directive: &NginxDirective) -> Result<String, NginxConfigError> {
    match directive.args.first().map(String::as_str) {
        Some("on") => Ok("on".to_owned()),
        Some("off") => Ok("off".to_owned()),
        Some("only") => Ok("only".to_owned()),
        other => Err(NginxConfigError::unsupported(
            directive,
            format!("mail `starttls` accepts on|off|only, found `{other:?}`"),
        )),
    }
}

fn parse_mail_timeout(directive: &NginxDirective) -> Result<u64, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `timeout` requires a time value",
        ));
    };
    let Some(seconds) = parse_nginx_time_seconds(value) else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid mail timeout `{value}`"),
        ));
    };
    let millis = seconds.saturating_mul(1_000);
    if !(1..=86_400_000).contains(&millis) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("mail timeout `{value}` is outside 1ms..=24h"),
        ));
    }
    Ok(millis)
}

fn parse_mail_proxy_buffer(directive: &NginxDirective) -> Result<usize, NginxConfigError> {
    let Some(bytes) = parse_body_size(directive)? else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `proxy_buffer` requires a size",
        ));
    };
    let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
    if !(512..=1024 * 1024).contains(&bytes) {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `proxy_buffer` must be between 512 bytes and 1 MiB",
        ));
    }
    Ok(bytes)
}

fn parse_mail_proxy_timeout(directive: &NginxDirective) -> Result<u64, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `proxy_timeout` requires a time value",
        ));
    };
    let Some(seconds) = parse_nginx_time_seconds(value) else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid proxy_timeout `{value}`"),
        ));
    };
    let millis = seconds.saturating_mul(1_000);
    if !(1..=86_400_000).contains(&millis) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("proxy_timeout `{value}` is outside 1ms..=24h"),
        ));
    }
    Ok(millis)
}

fn parse_mail_max_errors(directive: &NginxDirective) -> Result<u32, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `max_errors` requires a count",
        ));
    };
    let errors = value.parse::<u32>().map_err(|_| {
        NginxConfigError::unsupported(
            directive,
            format!("mail `max_errors` requires a positive integer, found `{value}`"),
        )
    })?;
    if !(1..=100).contains(&errors) {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `max_errors` must be between 1 and 100",
        ));
    }
    Ok(errors)
}

fn parse_mail_greeting_delay(directive: &NginxDirective) -> Result<u64, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail `smtp_greeting_delay` requires a time value",
        ));
    };
    let Some(millis) = parse_mail_delay_millis(value) else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid smtp_greeting_delay `{value}`"),
        ));
    };
    if millis > 60_000 {
        return Err(NginxConfigError::unsupported(
            directive,
            "smtp_greeting_delay must not exceed 60s",
        ));
    }
    Ok(millis)
}

/// One parsed mail `listen` entry.
#[derive(Debug, Clone)]
struct MailListenSpec {
    bind: String,
    port: u16,
    proxy_protocol: bool,
    so_keepalive: Option<bool>,
    /// `listen … ssl`: implicit TLS from the first byte.
    ssl: bool,
}

/// Parse `listen <address:port> [proxy_protocol] [so_keepalive=on|off]
/// [reuseport] [backlog=…]` in the nginx mail module.
///
/// nginx mail binds an ephemeral port when `listen` has no explicit port
/// (verified against nginx 1.29.6: `listen 127.0.0.1;` binds a random high
/// port). The runtime listener model requires a concrete port, so that form
/// fails closed with a located diagnostic instead of silently choosing a port.
fn parse_mail_listen_spec(directive: &NginxDirective) -> Result<MailListenSpec, NginxConfigError> {
    let mut parts = directive.args.iter();
    let address = parts.next().ok_or_else(|| {
        NginxConfigError::unsupported(directive, "mail listen requires an address")
    })?;
    let mut proxy_protocol = false;
    let mut so_keepalive = None;
    let mut ssl = false;
    for part in parts {
        match part.as_str() {
            "ssl" => ssl = true,
            "proxy_protocol" => proxy_protocol = true,
            "so_keepalive=on" => so_keepalive = Some(true),
            "so_keepalive=off" => so_keepalive = Some(false),
            "reuseport" | "bind" | "deferred" => {}
            other
                if other.starts_with("backlog=")
                    || other.starts_with("ipv6only=")
                    || other.starts_with("fastopen=")
                    || other.starts_with("so_keepalive=") =>
            {
                if other.starts_with("so_keepalive=") {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                            "mail listen parameter `{other}` (keepalive timers) is not modeled; use `so_keepalive=on|off`"
                        ),
                    ));
                }
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("unsupported mail listen parameter `{other}`"),
                ))
            }
        }
    }
    let spec = directive.args.join(" ");
    let (bind, port) = parse_mail_listen_address(address, &spec)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    Ok(MailListenSpec {
        bind,
        port,
        proxy_protocol,
        so_keepalive,
        ssl,
    })
}

/// Parse a mail listen address, requiring an explicit port.
fn parse_mail_listen_address(address: &str, spec: &str) -> Result<(String, u16), String> {
    if let Ok(port) = address.parse::<u16>() {
        return Ok(("0.0.0.0".to_owned(), port));
    }
    if let Some(rest) = address.strip_prefix('[') {
        let (host, port) = rest
            .split_once(']')
            .ok_or_else(|| format!("invalid mail listen address `{spec}`"))?;
        let Some(port_text) = port.strip_prefix(':') else {
            return Err(format!(
                "mail listen `{spec}` requires an explicit port; nginx binds an ephemeral port when it is omitted, which the runtime listener model does not support"
            ));
        };
        let port = port_text
            .parse::<u16>()
            .map_err(|_| format!("invalid mail listen port in `{spec}`"))?;
        return Ok((host.to_owned(), port));
    }
    if address == "::" {
        return Err(format!(
            "mail listen `{spec}` requires an explicit port; nginx binds an ephemeral port when it is omitted, which the runtime listener model does not support"
        ));
    }
    if let Some((host, port_text)) = address.rsplit_once(':') {
        let port = port_text
            .parse::<u16>()
            .map_err(|_| format!("invalid mail listen port in `{spec}`"))?;
        let normalized = match host {
            "" | "0.0.0.0" | "*" => "0.0.0.0".to_owned(),
            other => other.to_owned(),
        };
        return Ok((normalized, port));
    }
    Err(format!(
        "mail listen `{spec}` requires an explicit port; nginx binds an ephemeral port when it is omitted, which the runtime listener model does not support"
    ))
}

/// Parse mail `so_keepalive on|off`.
fn parse_mail_so_keepalive(directive: &NginxDirective) -> Result<bool, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "mail so_keepalive requires on|off",
        ));
    };
    match value.as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        other if other.contains(':') => Err(NginxConfigError::unsupported(
            directive,
            format!(
                "mail `so_keepalive {other}` (keepidle:keepintvl:keepcnt) is not modeled; use `on`/`off`"
            ),
        )),
        other => Err(NginxConfigError::unsupported(
            directive,
            format!("mail so_keepalive accepts on|off, found `{other}`"),
        )),
    }
}

/// Parse `smtp_client_buffer` / `imap_client_buffer` (bounded 512 B..1 MiB).
fn parse_mail_client_buffer(directive: &NginxDirective) -> Result<usize, NginxConfigError> {
    let Some(bytes) = parse_body_size(directive)? else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid {} size", directive.name),
        ));
    };
    if !(512..=1024 * 1024).contains(&bytes) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} must be between 512 bytes and 1 MiB", directive.name),
        ));
    }
    Ok(bytes as usize)
}

/// Collect the raw arguments of a mail `*_auth` directive.
fn require_mail_auth_args(directive: &NginxDirective) -> Result<Vec<String>, NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires at least one mechanism", directive.name),
        ));
    }
    Ok(directive.args.clone())
}

/// Normalize a declared mail `pop3_auth` / `imap_auth` / `smtp_auth` list into
/// the bounded runtime vocabulary (JSON mechanism strings).
///
/// The per-protocol grammar is nginx's: `pop3_auth` accepts
/// `plain|apop|cram-md5|external` (LOGIN is not configurable — nginx advertises
/// it implicitly alongside PLAIN, verified against nginx 1.29.6), while
/// `imap_auth`/`smtp_auth` accept `plain|login|cram-md5|external`.
///
/// `apop`/`cram-md5`/`external` need the `auth_http` credential exchange
/// (Wave 9B) and fail closed here instead of advertising a mechanism the 9A
/// frontend cannot complete.
fn normalize_mail_auth_mechanisms(
    directive: &NginxDirective,
    protocol: &'static str,
    declared: Option<Vec<String>>,
) -> Result<Option<Vec<Value>>, NginxConfigError> {
    let Some(declared) = declared else {
        return Ok(None);
    };
    let directive_name = match protocol {
        "pop3" => "pop3_auth",
        "smtp" => "smtp_auth",
        _ => "imap_auth",
    };
    let mut mechanisms: Vec<Value> = Vec::new();
    for token in declared {
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "plain" => {
                if !mechanisms.iter().any(|value| value == "plain") {
                    mechanisms.push(Value::String("plain".to_owned()));
                }
                // nginx advertises AUTH LOGIN for POP3 whenever PLAIN is
                // enabled; LOGIN is not a configurable pop3_auth token.
                if protocol == "pop3" && !mechanisms.iter().any(|value| value == "login") {
                    mechanisms.push(Value::String("login".to_owned()));
                }
            }
            "login" if protocol != "pop3" => {
                if !mechanisms.iter().any(|value| value == "login") {
                    mechanisms.push(Value::String("login".to_owned()));
                }
            }
            "login" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`pop3_auth` accepts plain|apop|cram-md5|external; nginx advertises AUTH LOGIN implicitly for POP3 and does not accept `login` as a pop3_auth value",
                ))
            }
            "apop" | "cram-md5" | "external" => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{directive_name}` mechanism `{token}` requires the auth_http credential exchange (Wave 9B); Wave 9A parses PLAIN and LOGIN only"
                    ),
                ))
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("`{directive_name}` accepts plain|login in Wave 9A, found `{other}`"),
                ))
            }
        }
    }
    Ok(Some(mechanisms))
}

/// Parse `smtp_greeting_delay` with nginx's millisecond-defaulted time grammar
/// (a bare number is milliseconds; `ms`/`s`/`m`/`h` suffixes are accepted).
fn parse_mail_delay_millis(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (number, multiplier) = if let Some(rest) = trimmed.strip_suffix("ms") {
        (rest, 1u64)
    } else if let Some(rest) = trimmed.strip_suffix('s') {
        (rest, 1_000)
    } else if let Some(rest) = trimmed.strip_suffix('m') {
        (rest, 60_000)
    } else if let Some(rest) = trimmed.strip_suffix('h') {
        (rest, 3_600_000)
    } else {
        (trimmed, 1)
    };
    number.trim().parse::<u64>().ok()?.checked_mul(multiplier)
}

/// Validate `server_name` names against the nginx host-selection forms the
/// runtime executes.
///
/// Supported: exact names, leading wildcards (`*.example.com`), trailing
/// wildcards (`www.example.*`), dot-prefixed names (`.example.com`), IP
/// literals, and regex names (`~pattern`, with captures). The empty
/// `server_name ""` form and nginx's `$hostname` shorthand are still rejected;
/// both are declared gaps, so a config that needs them fails closed at import
/// instead of silently selecting a different host.
fn validate_server_names(
    directive: &NginxDirective,
    server_names: &[String],
) -> Result<(), NginxConfigError> {
    if server_names.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "server requires server_name",
        ));
    }
    for name in server_names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "the empty `server_name \"\"` (requests without a Host header) is not supported by the runtime model",
            ));
        }
        if trimmed.eq_ignore_ascii_case("$hostname") {
            return Err(NginxConfigError::unsupported(
                directive,
                "`server_name $hostname` is not supported by the runtime model; declare the host names explicitly",
            ));
        }
        match crate::config::parse_server_name(trimmed) {
            Some(crate::config::ServerNameForm::Regex(regex)) => {
                if let Err(error) = regex::Regex::new(&regex.compiled_pattern()) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                            "regex server name `{trimmed}` is not supported by the runtime regex engine: {error}"
                        ),
                    ));
                }
            }
            Some(_) => {}
            None => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "server name `{trimmed}` is not a supported nginx form; use an exact name, `*.suffix`, `prefix.*`, `.suffix`, or `~regex`"
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Server names a TLS certificate can be selected for.
///
/// The certificate index only selects exact names and one-level leading
/// wildcards. A dot-prefixed HTTP name expands into the exact name plus the
/// leading wildcard; regex and trailing-wildcard names have no SNI
/// representation and fail closed on an `ssl` server instead of serving a
/// certificate that does not cover the requested name.
fn certificate_server_names(
    directive: &NginxDirective,
    server_names: &[String],
) -> Result<(Vec<String>, Vec<String>), NginxConfigError> {
    let mut names = Vec::new();
    let mut regexes = Vec::new();
    for name in server_names {
        let trimmed = name.trim();
        match crate::config::parse_server_name(trimmed) {
            Some(crate::config::ServerNameForm::Exact(name)) => names.push(name),
            Some(crate::config::ServerNameForm::LeadingWildcard(suffix)) => {
                names.push(format!("*.{suffix}"))
            }
            Some(crate::config::ServerNameForm::DotPrefix(suffix)) => {
                names.push(suffix.clone());
                names.push(format!("*.{suffix}"));
            }
            Some(crate::config::ServerNameForm::TrailingWildcard(_)) => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "trailing-wildcard server name `{trimmed}` cannot be represented by the SNI certificate index; use exact or leading-wildcard names on an ssl server"
                    ),
                ));
            }
            Some(crate::config::ServerNameForm::Regex(regex)) => {
                // nginx SNI selection evaluates regex server names in authored
                // order after the exact/wildcard tables; the certificate index
                // carries the same patterns for this certificate.
                regexes.push(regex.pattern);
            }
            None => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("server name `{trimmed}` is not a supported nginx form"),
                ));
            }
        }
    }
    Ok((names, regexes))
}

/// Validate `try_files` arguments against what the runtime can execute.
///
/// nginx checks each file argument in order (`$uri`, `$uri/`), then uses the
/// final argument as an internal redirect URI, a `@named` location, or
/// `=code`; the runtime executes the `$uri`/`$uri/` probes (directory index
/// included) and a literal fallback path, and materializes Adaptive Web
/// `try_files … @$surface` dispatch as a placeholder static surface.
/// Anything else fails closed instead of silently producing different
/// results.
fn validate_try_files(directive: &NginxDirective) -> Result<(), NginxConfigError> {
    let entries = &directive.args;
    if entries.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "try_files requires at least one argument",
        ));
    }
    for entry in &entries[..entries.len() - 1] {
        if entry != "$uri" && entry != "$uri/" && !entry.starts_with('/') {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "try_files entry `{entry}` is not supported; the runtime executes `$uri` and `$uri/` probes, literal file probes, and the fallback path"
                ),
            ));
        }
    }
    match entries.last().map(String::as_str) {
        Some(last) if last.starts_with('/') => Ok(()),
        // Adaptive Web dispatch target (`try_files … @$surface`).
        Some(last) if last.starts_with('@') => Ok(()),
        // nginx `=code` fallback (return the status when all probes fail).
        Some(last) if last.starts_with('=') => Ok(()),
        Some(last) => Err(NginxConfigError::unsupported(
            directive,
            format!(
                "try_files fallback `{last}` is not supported; the runtime supports a literal `/path` fallback (SPA fallback)"
            ),
        )),
        None => unreachable!("entries is non-empty"),
    }
}

/// Parse one `proxy_ssl_*` directive into a [`ProxySslSettings`] builder.
/// Relative certificate paths resolve against the loaded configuration
/// directory (nginx resolves them against the main config directory).
fn parse_proxy_ssl_directive(
    settings: &mut ProxySslSettings,
    directive: &NginxDirective,
) -> Result<(), NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires an argument", directive.name),
        ));
    };
    let resolve = |directive: &NginxDirective| -> Result<String, NginxConfigError> {
        let value = directive.args.first().expect("checked argument presence");
        if value.starts_with('/') || value.contains('\\') {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "`{}` file `{value}` must be relative to the loaded configuration directory: the runtime sandboxes upstream TLS material to the config directory",
                    directive.name
                ),
            ));
        }
        Ok(value.clone())
    };
    match directive.name.as_str() {
        "proxy_ssl_verify" => match value.as_str() {
            "on" => settings.verify = Some(true),
            "off" => settings.verify = Some(false),
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("proxy_ssl_verify accepts on|off, found `{other}`"),
                ));
            }
        },
        "proxy_ssl_trusted_certificate" => {
            settings.trusted_certificate = Some(resolve(directive)?);
        }
        "proxy_ssl_certificate" => {
            settings.certificate = Some(resolve(directive)?);
        }
        "proxy_ssl_certificate_key" => {
            settings.certificate_key = Some(resolve(directive)?);
        }
        "proxy_ssl_server_name" => match value.as_str() {
            "on" => settings.server_name = Some(true),
            "off" => settings.server_name = Some(false),
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("proxy_ssl_server_name accepts on|off, found `{other}`"),
                ));
            }
        },
        name => {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("unsupported proxy_ssl directive `{name}`"),
            ));
        }
    }
    Ok(())
}

/// Parse one realip-module directive into a [`RealIpSettings`] builder.
fn parse_real_ip_directive(
    settings: &mut RealIpSettings,
    directive: &NginxDirective,
) -> Result<(), NginxConfigError> {
    match directive.name.as_str() {
        "set_real_ip_from" => {
            let Some(network) = directive.args.first() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "set_real_ip_from requires an address or CIDR",
                ));
            };
            let cidr = if let Ok(ip) = network.parse::<IpAddr>() {
                match ip {
                    IpAddr::V4(ip) => format!("{ip}/32"),
                    IpAddr::V6(ip) => format!("{ip}/128"),
                }
            } else if let Ok(net) = network.parse::<ipnet::IpNet>() {
                net.to_string()
            } else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("invalid set_real_ip_from network `{network}`"),
                ));
            };
            if !settings.trusted_cidrs.contains(&cidr) {
                settings.trusted_cidrs.push(cidr);
            }
        }
        "real_ip_header" => {
            let Some(header) = directive.args.first() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "real_ip_header requires a header name",
                ));
            };
            if !header.eq_ignore_ascii_case("X-Forwarded-For") {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "real_ip_header `{header}` is not supported; the runtime resolves `X-Forwarded-For` only"
                    ),
                ));
            }
        }
        "real_ip_recursive" => {
            let Some(flag) = directive.args.first() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "real_ip_recursive requires on or off",
                ));
            };
            match flag.as_str() {
                "on" => settings.recursive = true,
                "off" => settings.recursive = false,
                other => {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("real_ip_recursive accepts on|off, found `{other}`"),
                    ));
                }
            }
        }
        name => {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("unsupported real_ip directive `{name}`"),
            ));
        }
    }
    Ok(())
}

/// Materialize validated `proxy_ssl_*` settings into the runtime upstream
/// `tls` object. The runtime always verifies upstream TLS (system roots by
/// default), so `proxy_ssl_verify off` and client certificates without
/// verification fail closed instead of silently weakening the handshake.
fn materialize_upstream_tls(
    location: &NginxDirective,
    settings: &ProxySslSettings,
) -> Result<Option<Value>, NginxConfigError> {
    if settings.is_empty() {
        return Ok(None);
    }
    if settings.verify == Some(false) {
        return Err(NginxConfigError::unsupported(
            location,
            "`proxy_ssl_verify off` cannot be honored: the runtime always verifies upstream TLS; remove the directive or use `proxy_ssl_verify on` with `proxy_ssl_trusted_certificate`",
        ));
    }
    if settings.server_name == Some(false) {
        return Err(NginxConfigError::unsupported(
            location,
            "`proxy_ssl_server_name off` cannot be honored: the runtime always sends the upstream hostname as SNI; remove the directive",
        ));
    }
    if settings.trusted_certificate.is_some() && settings.verify != Some(true) {
        return Err(NginxConfigError::unsupported(
            location,
            "`proxy_ssl_trusted_certificate` requires `proxy_ssl_verify on` (the runtime always verifies upstream TLS)",
        ));
    }
    match (&settings.certificate, &settings.certificate_key) {
        (Some(_), None) => {
            return Err(NginxConfigError::unsupported(
                location,
                "`proxy_ssl_certificate` requires `proxy_ssl_certificate_key`",
            ));
        }
        (None, Some(_)) => {
            return Err(NginxConfigError::unsupported(
                location,
                "`proxy_ssl_certificate_key` requires `proxy_ssl_certificate`",
            ));
        }
        (Some(_), Some(_)) if settings.verify != Some(true) => {
            return Err(NginxConfigError::unsupported(
                location,
                "`proxy_ssl_certificate`/`proxy_ssl_certificate_key` require `proxy_ssl_verify on` (upstream TLS is always verified)",
            ));
        }
        _ => {}
    }
    let mut tls = serde_json::Map::new();
    if let Some(ca_file) = &settings.trusted_certificate {
        tls.insert("trustMode".to_owned(), Value::String("custom".to_owned()));
        tls.insert(
            "caCertificateFiles".to_owned(),
            Value::Array(vec![Value::String(ca_file.clone())]),
        );
    } else {
        tls.insert("trustMode".to_owned(), Value::String("system".to_owned()));
    }
    if let (Some(certificate), Some(key)) = (&settings.certificate, &settings.certificate_key) {
        tls.insert(
            "clientCertificateFile".to_owned(),
            Value::String(certificate.clone()),
        );
        tls.insert(
            "clientPrivateKeyFile".to_owned(),
            Value::String(key.clone()),
        );
    }
    Ok(Some(Value::Object(tls)))
}

/// Parse one literal `grpc_pass` target.
///
/// nginx 1.29.6 accepts `grpc://host[:port]`, `grpcs://host[:port]` and a
/// declared `upstream` name; a URI part after the authority is rejected by
/// nginx itself ("invalid host in upstream"), so this materializer fails closed
/// with the same reading instead of inventing replacement semantics the module
/// does not have. Variable and `unix:` targets are outside the bounded adapter.
fn split_grpc_target<'a>(
    location: &NginxDirective,
    target: &'a str,
) -> Result<GrpcTarget<'a>, NginxConfigError> {
    if target.contains('$') {
        return Err(NginxConfigError::unsupported(
            location,
            "variable `grpc_pass` targets are not executable by the bounded gRPC adapter; use a literal grpc://host:port, grpcs://host:port, or a declared upstream",
        ));
    }
    if target.starts_with("unix:") {
        return Err(NginxConfigError::unsupported(
            location,
            "`grpc_pass unix:` sockets are not executable by the bounded gRPC adapter (TCP upstreams only)",
        ));
    }
    if let Some(rest) = target.strip_prefix("grpc://") {
        return Ok(GrpcTarget::Literal {
            scheme: "http",
            authority: validate_grpc_authority(location, target, rest)?,
        });
    }
    if let Some(rest) = target.strip_prefix("grpcs://") {
        return Ok(GrpcTarget::Literal {
            scheme: "https",
            authority: validate_grpc_authority(location, target, rest)?,
        });
    }
    if target.contains("://") {
        return Err(NginxConfigError::unsupported(
            location,
            format!("grpc_pass `{target}` must use grpc:// or grpcs://"),
        ));
    }
    Ok(GrpcTarget::Upstream(target))
}

enum GrpcTarget<'a> {
    Literal {
        scheme: &'static str,
        authority: String,
    },
    Upstream(&'a str),
}

fn validate_grpc_authority(
    location: &NginxDirective,
    target: &str,
    rest: &str,
) -> Result<String, NginxConfigError> {
    if rest.contains('/') {
        return Err(NginxConfigError::unsupported(
            location,
            format!(
                "grpc_pass `{target}` carries a URI part, which nginx 1.29.6 rejects (`invalid host in upstream`); `grpc_pass` has no URI replacement semantics"
            ),
        ));
    }
    let Some((host, port)) = rest.rsplit_once(':') else {
        return Err(NginxConfigError::unsupported(
            location,
            format!("grpc_pass `{target}` must be host:port"),
        ));
    };
    if host.is_empty() || port.parse::<u16>().map(|port| port == 0).unwrap_or(true) {
        return Err(NginxConfigError::unsupported(
            location,
            format!("grpc_pass `{target}` must be a valid host:port"),
        ));
    }
    Ok(rest.to_owned())
}

/// Materialize the resolved `grpc_ssl_*` settings into the runtime upstream TLS
/// policy. The runtime always verifies the upstream chain and always sends SNI,
/// so `grpc_ssl_verify off` / `grpc_ssl_server_name off` fail closed rather than
/// silently weakening verification (mirroring the `proxy_ssl_*` reading).
/// Resolve a relative upstream TLS material path against the loaded nginx
/// configuration directory. The compiled model for `NginxConf` uses `/` as its
/// base directory (materialization already resolved every nginx-relative path),
/// so the mapping must hand it absolute, conf-directory-anchored paths.
fn resolve_upstream_tls_file(base_dir: &std::path::Path, value: &str) -> String {
    base_dir.join(value).to_string_lossy().replace('\\', "/")
}

fn materialize_grpc_tls(
    location: &NginxDirective,
    settings: &GrpcSslSettings,
    base_dir: &std::path::Path,
) -> Result<Option<Value>, NginxConfigError> {
    // nginx materialization feeds the compiled model, whose `NginxConf` base
    // directory is `/` (every nginx-relative path is resolved here). Resolving
    // against the conf directory keeps the protected-file sandbox
    // (`resolve_protected_relative_file`) working instead of anchoring TLS
    // material to `/`.
    if settings.is_empty() {
        return Ok(None);
    }
    if settings.verify == Some(false) {
        return Err(NginxConfigError::unsupported(
            location,
            "`grpc_ssl_verify off` cannot be honored: the runtime always verifies upstream TLS; remove the directive or use `grpc_ssl_verify on` with `grpc_ssl_trusted_certificate`",
        ));
    }
    if settings.server_name == Some(false) {
        return Err(NginxConfigError::unsupported(
            location,
            "`grpc_ssl_server_name off` cannot be honored: the runtime always sends the upstream SNI name; remove the directive",
        ));
    }
    if settings.trusted_certificate.is_some() && settings.verify != Some(true) {
        return Err(NginxConfigError::unsupported(
            location,
            "`grpc_ssl_trusted_certificate` requires `grpc_ssl_verify on` (the runtime always verifies upstream TLS)",
        ));
    }
    match (&settings.certificate, &settings.certificate_key) {
        (Some(_), None) => {
            return Err(NginxConfigError::unsupported(
                location,
                "`grpc_ssl_certificate` requires `grpc_ssl_certificate_key`",
            ));
        }
        (None, Some(_)) => {
            return Err(NginxConfigError::unsupported(
                location,
                "`grpc_ssl_certificate_key` requires `grpc_ssl_certificate`",
            ));
        }
        (Some(_), Some(_)) if settings.verify != Some(true) => {
            return Err(NginxConfigError::unsupported(
                location,
                "`grpc_ssl_certificate`/`grpc_ssl_certificate_key` require `grpc_ssl_verify on` (upstream TLS is always verified)",
            ));
        }
        _ => {}
    }
    let mut tls = serde_json::Map::new();
    if let Some(ca_file) = &settings.trusted_certificate {
        tls.insert("trustMode".to_owned(), Value::String("custom".to_owned()));
        tls.insert(
            "caCertificateFiles".to_owned(),
            Value::Array(vec![Value::String(resolve_upstream_tls_file(
                base_dir, ca_file,
            ))]),
        );
    } else {
        tls.insert("trustMode".to_owned(), Value::String("system".to_owned()));
    }
    if let (Some(certificate), Some(key)) = (&settings.certificate, &settings.certificate_key) {
        tls.insert(
            "clientCertificateFile".to_owned(),
            Value::String(resolve_upstream_tls_file(base_dir, certificate)),
        );
        tls.insert(
            "clientPrivateKeyFile".to_owned(),
            Value::String(resolve_upstream_tls_file(base_dir, key)),
        );
    }
    if let Some(protocols) = settings.protocols {
        tls.insert(
            "minimumVersion".to_owned(),
            Value::String(protocols.minimum.wire().to_owned()),
        );
        tls.insert(
            "maximumVersion".to_owned(),
            Value::String(protocols.maximum.wire().to_owned()),
        );
    }
    if let Some(name) = &settings.name {
        tls.insert("serverName".to_owned(), Value::String(name.clone()));
    }
    Ok(Some(Value::Object(tls)))
}

/// Split a literal `proxy_pass` target into its upstream authority and its
/// optional URI part. nginx replaces the location-matched prefix with the
/// URI part when one is present; without one the full request URI is
/// forwarded. `unix:` sockets and query strings in the URI part are not
/// supported by the runtime model and fail closed.
fn split_proxy_pass_target(
    location: &NginxDirective,
    target: &str,
) -> Result<(String, Option<String>), NginxConfigError> {
    if target.contains("unix:") {
        return Err(NginxConfigError::unsupported(
            location,
            "unix: proxy_pass sockets are not supported by the runtime model",
        ));
    }
    let scheme = if target.starts_with("http://") {
        "http://"
    } else if target.starts_with("https://") {
        "https://"
    } else {
        return Err(NginxConfigError::unsupported(
            location,
            format!("proxy_pass `{target}` must be http(s)://upstream or http(s)://host:port"),
        ));
    };
    let rest = &target[scheme.len()..];
    let (authority, uri) = match rest.split_once('/') {
        Some((authority, path)) => (authority, Some(format!("/{path}"))),
        None => (rest, None),
    };
    if authority.is_empty() {
        return Err(NginxConfigError::unsupported(
            location,
            format!("proxy_pass `{target}` has an empty upstream authority"),
        ));
    }
    if uri.as_deref().is_some_and(|uri| uri.contains('?')) {
        return Err(NginxConfigError::unsupported(
            location,
            "query strings in the proxy_pass URI part are not supported by the runtime model",
        ));
    }
    Ok((format!("{scheme}{authority}"), uri))
}

/// The `@name` of an nginx named location directive.
fn named_location_name(location: &NginxDirective) -> Result<String, NginxConfigError> {
    let Some(name) = location.args.first() else {
        return Err(NginxConfigError::unsupported(
            location,
            "named location requires an `@name` argument",
        ));
    };
    let valid = name.strip_prefix('@').is_some_and(|rest| {
        !rest.is_empty()
            && name.len() <= 257
            && rest.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'~')
            })
    });
    if !valid {
        return Err(NginxConfigError::unsupported(
            location,
            format!("invalid named location `{name}`"),
        ));
    }
    Ok(name.clone())
}

/// Whether an `@pc` / `@h5` named location still has the Adaptive Web shape:
/// an absolute `root` plus optional `index`, `try_files`, and `add_header`.
/// Any other shape is treated as an ordinary named location.
fn named_location_has_adaptive_shape(location: &NginxDirective) -> bool {
    let mut has_root = false;
    for child in &location.children {
        match child.name.as_str() {
            "root" => {
                if child.args.is_empty() || !child.args[0].starts_with('/') {
                    return false;
                }
                has_root = true;
            }
            "index" | "try_files" | "add_header" => {}
            _ => return false,
        }
    }
    has_root
}

/// Parse Adaptive Web named locations (`@pc`, `@h5`) into a document root.
fn named_adaptive_location_root(
    location: &NginxDirective,
) -> Result<(String, String), NginxConfigError> {
    let Some(name) = location.args.first() else {
        return Err(NginxConfigError::unsupported(
            location,
            "named location requires @pc or @h5",
        ));
    };
    let mut root = None;
    for child in &location.children {
        match child.name.as_str() {
            "root" => {
                let Some(value) = child.args.first() else {
                    return Err(NginxConfigError::unsupported(child, "root requires a path"));
                };
                if !value.starts_with('/') {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "root must be an absolute directory for nginx compatibility",
                    ));
                }
                root = Some(value.clone());
            }
            "index" | "try_files" | "add_header" => {}
            _ => {
                return Err(NginxConfigError::unsupported(
                    child,
                    format!("unsupported named-location directive `{}`", child.name),
                ));
            }
        }
    }
    let Some(root) = root else {
        return Err(NginxConfigError::unsupported(
            location,
            format!("named location `{name}` must declare root"),
        ));
    };
    Ok((name.clone(), root))
}

/// Parse a location match into (path_type, path). `= /x` exact, `^~ /x` and
/// `/x` prefix, `~`/`~*` regex, and `@name` a named location (reached only by
/// a bounded internal redirect; the returned path is the literal `@name`).
fn parse_location_match(
    location: &NginxDirective,
) -> Result<(&'static str, String), NginxConfigError> {
    let Some(match_value) = location.args.first() else {
        return Err(NginxConfigError::unsupported(
            location,
            "location requires a match path",
        ));
    };
    if let Some(name) = match_value.strip_prefix('@') {
        if name.is_empty() || match_value.len() > 257 {
            return Err(NginxConfigError::unsupported(
                location,
                format!("invalid named location `{match_value}`"),
            ));
        }
        return Ok(("exact", match_value.clone()));
    }
    match (match_value.as_str(), location.args.get(1)) {
        ("=", Some(path)) => Ok(("exact", path.clone())),
        ("^~", Some(path)) => Ok(("prefix-exclusive", path.clone())),
        ("~", Some(pattern)) => Ok(("regex", pattern.clone())),
        ("~*", Some(pattern)) => Ok(("regex-ignore-case", pattern.clone())),
        (path, _) => {
            // nginx also accepts the modifier glued to the URI
            // (`location ^~/api/`, `location ~*\.(png|jpg)$`, `location =/x`)
            // because its parser strips the modifier characters before
            // reading the URI. Mirror that here instead of treating the
            // modifier as part of a prefix path.
            if let Some(rest) = path.strip_prefix("^~") {
                if !rest.is_empty() {
                    return Ok(("prefix-exclusive", rest.to_owned()));
                }
            }
            if let Some(rest) = path.strip_prefix("~*") {
                if !rest.is_empty() {
                    return Ok(("regex-ignore-case", rest.to_owned()));
                }
            }
            if let Some(rest) = path.strip_prefix('~') {
                if !rest.is_empty() {
                    return Ok(("regex", rest.to_owned()));
                }
            }
            if let Some(rest) = path.strip_prefix('=') {
                if !rest.is_empty() {
                    return Ok(("exact", rest.to_owned()));
                }
            }
            Ok(("prefix", path.to_owned()))
        }
    }
}

/// True when a location targets the reserved ACME HTTP-01 challenge namespace.
///
/// Only prefix matches qualify. An `=` exact match names a single token rather
/// than the namespace, and a regex match is not the directory the runtime
/// serves, so neither is read as an `acmeHttp01` declaration. The trailing
/// slash is optional because both spellings appear in the wild and nginx treats
/// them alike for a directory prefix.
fn is_acme_challenge_location(location: &NginxDirective) -> bool {
    let Ok((path_type, path)) = parse_location_match(location) else {
        return false;
    };
    matches!(path_type, "prefix" | "prefix-exclusive")
        && (path == ACME_HTTP_01_CHALLENGE_PREFIX
            || path == ACME_HTTP_01_CHALLENGE_PREFIX.trim_end_matches('/'))
}

/// Only the variable combinations the redirect data plane expands are
/// accepted in `return` URLs and TOML `returnLocation` values.
pub(crate) fn redirect_variables_ok(url: &str) -> bool {
    redirect_variables_ok_with_maps(url, &[])
}

/// [`redirect_variables_ok`] plus the nginx `map` result variables declared by
/// the same application config.
pub(crate) fn redirect_variables_ok_with_maps(url: &str, map_variables: &[&str]) -> bool {
    let mut remainder = url;
    while let Some(index) = remainder.find('$') {
        let rest = &remainder[index..];
        if let Some(stripped) = rest.strip_prefix("$$") {
            remainder = stripped;
            continue;
        }
        let variable = if rest.starts_with("$request_uri") {
            "$request_uri"
        } else if rest.starts_with("$host") {
            "$host"
        } else if rest.starts_with("$scheme") {
            "$scheme"
        } else {
            let name = rest[1..]
                .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .map_or_else(
                    || rest[1..].to_owned(),
                    |offset| rest[1..1 + offset].to_owned(),
                );
            if map_variables.contains(&name.as_str()) {
                remainder = &rest[1 + name.len()..];
                continue;
            }
            return false;
        };
        remainder = &rest[variable.len()..];
    }
    true
}

/// Validate a `return <status> <body>` body: the runtime expands the response
/// header variable vocabulary (`$host`, `$request_uri`, `$uri`, …) plus the
/// declared nginx `map` results; anything else fails closed.
fn return_body_variables_ok(body: &str, map_variables: &[&str]) -> bool {
    let mut remainder = body;
    while let Some(index) = remainder.find('$') {
        let rest = &remainder[index..];
        if let Some(stripped) = rest.strip_prefix("$$") {
            remainder = stripped;
            continue;
        }
        let end = rest[1..]
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .map_or(rest.len(), |offset| offset + 1);
        let name = &rest[1..end];
        if name.is_empty()
            || (!CustomHeaderConfig::SUPPORTED_VARIABLES.contains(&name)
                && !map_variables.contains(&name))
        {
            return false;
        }
        remainder = &rest[end..];
    }
    true
}

/// nginx `proxy_set_header` inheritance: these directives are inherited from
/// the previous configuration level if and only if there are no
/// `proxy_set_header` directives defined on the current level.
fn inherit_proxy_set_headers_nginx(inherited: &[String], current: &[String]) -> Vec<String> {
    if current.is_empty() {
        inherited.to_vec()
    } else {
        current.to_vec()
    }
}

fn parse_u64(value: &str) -> Option<u64> {
    value.parse::<u64>().ok()
}

/// Parse `client_max_body_size`; an unparsable value is an nginx config
/// error, not something to silently ignore.
fn parse_body_size(directive: &NginxDirective) -> Result<Option<u64>, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "client_max_body_size requires a size",
        ));
    };
    parse_size_bytes(value).map(Some).ok_or_else(|| {
        NginxConfigError::unsupported(directive, format!("invalid client_max_body_size `{value}`"))
    })
}

/// nginx leaves the total upstream retry budget to the runtime; the shared
/// model default (30s) matches `UpstreamRetryConfig`'s documented default.
fn default_nginx_retry_timeout_ms() -> u64 {
    30_000
}

fn parse_size_bytes(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    let (number, multiplier) = if let Some(rest) = trimmed
        .strip_suffix('k')
        .or_else(|| trimmed.strip_suffix('K'))
    {
        (rest, 1024u64)
    } else if let Some(rest) = trimmed
        .strip_suffix('m')
        .or_else(|| trimmed.strip_suffix('M'))
    {
        (rest, 1024 * 1024)
    } else if let Some(rest) = trimmed
        .strip_suffix('g')
        .or_else(|| trimmed.strip_suffix('G'))
    {
        (rest, 1024 * 1024 * 1024)
    } else {
        (trimmed, 1)
    };
    number.trim().parse::<u64>().ok()?.checked_mul(multiplier)
}

/// Parse a `types { <mime> <ext…>; … }` block into an ordered MIME table.
///
/// A declared block *replaces* the inherited table (including the runtime's
/// built-in default), which is why the model distinguishes "not declared"
/// (`None`) from "declared empty" (`Some(vec![])`). Extensions are stored
/// lowercased because the runtime's lookup lowercases the request extension.
fn parse_types_block(directive: &NginxDirective) -> Result<Vec<MimeTypeConfig>, NginxConfigError> {
    let mut types = Vec::with_capacity(directive.children.len());
    for child in &directive.children {
        validate_media_type(child, &child.name)?;
        if child.args.is_empty() {
            return Err(NginxConfigError::unsupported(
                child,
                format!(
                    "types entry `{}` requires at least one file extension",
                    child.name
                ),
            ));
        }
        let mut extensions = Vec::with_capacity(child.args.len());
        for extension in &child.args {
            if extension.starts_with('.')
                || extension.is_empty()
                || extension.len() > 64
                || !extension.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'+' | b'.')
                })
            {
                return Err(NginxConfigError::unsupported(
                    child,
                    format!(
                        "types entry `{}` declares an invalid extension `{extension}`; use the bare extension without a leading dot",
                        child.name
                    ),
                ));
            }
            extensions.push(extension.to_ascii_lowercase());
        }
        types.push(MimeTypeConfig {
            mime: child.name.to_ascii_lowercase(),
            extensions,
        });
    }
    Ok(types)
}

/// Validate a media type token (`type/subtype`, optional `;` parameters are
/// refused so the declared value always serializes as a single header value).
fn validate_media_type(directive: &NginxDirective, value: &str) -> Result<(), NginxConfigError> {
    let Some((media_type, subtype)) = value.split_once('/') else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("`{value}` is not a MIME type; expected `type/subtype`"),
        ));
    };
    let token_is_valid = |token: &str| {
        !token.is_empty()
            && token.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            })
    };
    if !token_is_valid(media_type) || !token_is_valid(subtype) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("`{value}` is not a MIME type; expected `type/subtype`"),
        ));
    }
    Ok(())
}

/// Parse one http-context `map` block into the runtime model.
///
/// Supported body statements: `default <value>;`, `<key> <value>;` (an empty
/// key is the empty-string key), and `hostnames;`. `volatile;`, `include`
/// (already expanded by the parser) and any other statement shape fail closed;
/// regex keys and unsupported variables are rejected by the shared map
/// validator with a located diagnostic.
/// Validate an nginx `ssl_stapling_file` path: the file must exist, be a
/// bounded DER `OCSPResponse` (ASN.1 SEQUENCE), and is read when TLS material
/// is built.
fn validate_ocsp_stapling_file(
    directive: &NginxDirective,
    path: &str,
) -> Result<String, NginxConfigError> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        NginxConfigError::unsupported(
            directive,
            format!("ssl_stapling_file `{path}` cannot be read: {error}"),
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 * 1024 {
        return Err(NginxConfigError::unsupported(
            directive,
            "ssl_stapling_file must be a non-empty DER OCSP response of at most 64 KiB",
        ));
    }
    let mut tag = [0_u8; 1];
    use std::io::Read;
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut tag))
        .map_err(|error| {
            NginxConfigError::unsupported(
                directive,
                format!("ssl_stapling_file `{path}` cannot be read: {error}"),
            )
        })?;
    if tag[0] != 0x30 {
        return Err(NginxConfigError::unsupported(
            directive,
            "ssl_stapling_file is not a DER OCSP response (missing SEQUENCE tag)",
        ));
    }
    Ok(path.to_owned())
}

/// Parse nginx stream context `map $source $result { … }`.
///
/// Bounded to the variables the stream plane knows at accept/preread time; a
/// source outside that set fails closed instead of silently evaluating to an
/// empty string.
fn parse_stream_map(directive: &NginxDirective) -> Result<MapConfig, NginxConfigError> {
    let [source, result] = directive.args.as_slice() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "stream map requires `map $source $result { … }`",
        ));
    };
    let Some(source_name) = source.strip_prefix('$') else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("stream map source `{source}` must be a `$variable`"),
        ));
    };
    if !crate::config::STREAM_MAP_SOURCES.contains(&source_name) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "stream map source `{source}` is not known at accept/preread time; supported: {}",
                crate::config::STREAM_MAP_SOURCES
                    .iter()
                    .map(|name| format!("${name}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ));
    }
    if !result.starts_with('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("stream map result `{result}` must be a `$variable`"),
        ));
    }
    if directive.children.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "stream map requires a `{ … }` block",
        ));
    }
    let mut default = None;
    let mut entries: Vec<MapEntryConfig> = Vec::new();
    for child in &directive.children {
        if child.name == "hostnames" {
            return Err(NginxConfigError::unsupported(
                child,
                "stream `map … hostnames` is not supported; declare explicit per-address entries",
            ));
        }
        if child.args.len() != 1 {
            return Err(NginxConfigError::unsupported(
                child,
                format!(
                    "stream map entry `{}` requires exactly one value",
                    child.name
                ),
            ));
        }
        let value = &child.args[0];
        if value.contains('$') {
            return Err(NginxConfigError::unsupported(
                child,
                "stream map values must be literal strings; variable values are not supported",
            ));
        }
        if child.name == "default" {
            if default.is_some() {
                return Err(NginxConfigError::unsupported(
                    child,
                    "stream map declares `default` more than once",
                ));
            }
            default = Some(value.clone());
            continue;
        }
        entries.push(MapEntryConfig {
            key: child.name.clone(),
            value: value.clone(),
        });
    }
    if entries.len() > 4096 {
        return Err(NginxConfigError::unsupported(
            directive,
            "stream map supports at most 4096 entries",
        ));
    }
    Ok(MapConfig {
        source: source.clone(),
        result: result.clone(),
        hostnames: false,
        default,
        entries,
    })
}

/// Parse nginx `geo [$address] $variable { … }` (http context).
fn parse_geo(directive: &NginxDirective) -> Result<GeoConfig, NginxConfigError> {
    if directive.children.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "geo requires a `{ … }` block",
        ));
    }
    let (source, result) = match directive.args.as_slice() {
        [result] => ("$remote_addr".to_owned(), result.clone()),
        [source, result] => (source.clone(), result.clone()),
        _ => {
            return Err(NginxConfigError::unsupported(
                directive,
                "geo takes `geo [$address] $variable { … }`",
            ))
        }
    };
    if !source.starts_with('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("geo source `{source}` must be a `$variable`"),
        ));
    }
    if !result.starts_with('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("geo result `{result}` must be a `$variable`"),
        ));
    }
    let mut ranges = false;
    let mut entries: Vec<GeoEntryConfig> = Vec::new();
    for child in &directive.children {
        if child.name == "ranges" {
            if !child.args.is_empty() {
                return Err(NginxConfigError::unsupported(
                    child,
                    "geo `ranges` takes no arguments",
                ));
            }
            ranges = true;
            continue;
        }
        if child.args.len() != 1 {
            return Err(NginxConfigError::unsupported(
                child,
                format!("geo entry `{}` requires exactly one value", child.name),
            ));
        }
        let value = &child.args[0];
        if value.contains('$') {
            return Err(NginxConfigError::unsupported(
                child,
                "geo values must be literal strings; variable values are not supported",
            ));
        }
        entries.push(GeoEntryConfig {
            key: child.name.clone(),
            value: value.clone(),
        });
    }
    let geo = GeoConfig {
        source,
        result,
        ranges,
        entries,
    };
    crate::config::validate_geo_config(&geo)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    Ok(geo)
}

/// Parse nginx `split_clients <source-template> $variable { <percent>% <value>; … }`.
fn parse_split_clients(directive: &NginxDirective) -> Result<SplitClientsConfig, NginxConfigError> {
    let [source, result] = directive.args.as_slice() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "split_clients takes `split_clients <source> $variable { … }`",
        ));
    };
    if !result.starts_with('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("split_clients result `{result}` must be a `$variable`"),
        ));
    }
    crate::config::map_variables::validate_map_value_template(source)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    if directive.children.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "split_clients requires a `{ … }` block",
        ));
    }
    let mut parts: Vec<SplitClientsPartConfig> = Vec::new();
    for child in &directive.children {
        let Some(percent) = crate::config::parse_split_percent(&child.name) else {
            return Err(NginxConfigError::unsupported(
                child,
                format!(
                    "invalid split_clients percentage `{}`; expected `NN%`, `NN.NN%`, or `*`",
                    child.name
                ),
            ));
        };
        if child.args.len() != 1 {
            return Err(NginxConfigError::unsupported(
                child,
                format!(
                    "split_clients bucket `{}` requires exactly one value",
                    child.name
                ),
            ));
        }
        let value = &child.args[0];
        if value.contains('$') {
            return Err(NginxConfigError::unsupported(
                child,
                "split_clients values must be literal strings; variable values are not supported",
            ));
        }
        parts.push(SplitClientsPartConfig {
            percent,
            value: value.clone(),
        });
    }
    let split = SplitClientsConfig {
        source: source.clone(),
        result: result.clone(),
        parts,
    };
    crate::config::validate_split_clients_config(&split)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    Ok(split)
}

fn parse_map(directive: &NginxDirective) -> Result<MapConfig, NginxConfigError> {
    if directive.args.len() != 2 {
        return Err(NginxConfigError::unsupported(
            directive,
            "map requires exactly two arguments: `map $source $result { … }`",
        ));
    }
    let source = &directive.args[0];
    let result = &directive.args[1];
    if !source.starts_with('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("map source `{source}` must be a `$variable`"),
        ));
    }
    if !result.starts_with('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("map result `{result}` must be a `$variable`"),
        ));
    }
    validate_map_source(source)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    if let Err(message) = crate::config::map_variables::parse_variable_reference(result) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("map result `{result}` is invalid: {message}"),
        ));
    }
    let mut hostnames = false;
    let mut default = None;
    let mut entries: Vec<MapEntryConfig> = Vec::new();
    for child in &directive.children {
        match child.name.as_str() {
            "hostnames" => {
                if !child.args.is_empty() {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "hostnames takes no arguments",
                    ));
                }
                hostnames = true;
            }
            "default" => {
                if child.args.len() != 1 {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "map `default` requires exactly one value",
                    ));
                }
                if default.is_some() {
                    return Err(NginxConfigError::unsupported(
                        child,
                        "map declares `default` more than once",
                    ));
                }
                default = Some(child.args[0].clone());
            }
            "volatile" => {
                return Err(NginxConfigError::unsupported(
                    child,
                    "`volatile;` (disable map result caching) has no runtime equivalent; remove it",
                ));
            }
            _ => {
                // The parser tokenises `<key> <value>;` as a directive whose
                // name is the key and whose single argument is the value (a
                // quoted empty key arrives as the empty directive name).
                if child.args.len() != 1 {
                    return Err(NginxConfigError::unsupported(
                        child,
                        format!(
                            "map entry `{}` must be `<key> <value>;` (found {} value argument(s))",
                            child.name,
                            child.args.len()
                        ),
                    ));
                }
                entries.push(MapEntryConfig {
                    key: child.name.clone(),
                    value: child.args[0].clone(),
                });
            }
        }
    }
    let map = MapConfig {
        source: source.clone(),
        result: result.clone(),
        hostnames,
        default,
        entries,
    };
    // Validate the declaration immediately so the diagnostic points at the
    // `map` block instead of surfacing later as a generic config failure.
    validate_map_definitions(std::slice::from_ref(&map))
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    Ok(map)
}

/// Parse `add_header <name> <value> [always];` into one ordered runtime
/// response-header entry.
///
/// nginx accepts exactly two arguments, or three with the literal `always`;
/// anything else is a configuration error (it is not silently ignored, because
/// a security header silently dropped is worse than a refused start). The
/// shared [`CustomHeaderConfig::validate_declaration`] check rejects an
/// invalid field name, an oversized value, and any `$variable` the runtime
/// cannot expand, so validation and execution always agree on the same
/// vocabulary.
fn parse_add_header(
    directive: &NginxDirective,
    map_variables: &[&str],
) -> Result<Value, NginxConfigError> {
    let always = match directive.args.as_slice() {
        [_, _] => false,
        [_, _, flag] if flag.eq_ignore_ascii_case("always") => true,
        _ => {
            return Err(NginxConfigError::unsupported(
                directive,
                "add_header requires `Name value [always]`",
            ))
        }
    };
    let name = &directive.args[0];
    let value = &directive.args[1];
    CustomHeaderConfig::validate_declaration_with_variables(name, value, map_variables)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    Ok(json!({ "name": name, "value": value, "always": always }))
}

/// Built-in request variables the bounded rewrite-phase engine resolves at
/// request time (the same vocabulary the CGI adapters already evaluate).
const REWRITE_REQUEST_VARIABLES: &[&str] = &[
    "host",
    "scheme",
    "remote_addr",
    "server_port",
    "request_method",
    "server_name",
    "server_protocol",
    "request_uri",
    "uri",
    "document_uri",
    "args",
    "query_string",
    "is_args",
    "content_type",
    "content_length",
    "document_root",
    "request_filename",
    "fastcgi_script_name",
    "fastcgi_path_info",
    "https",
];

fn rewrite_variable_is_known(name: &str, extra: &[String]) -> bool {
    crate::config::is_capture_variable(name)
        || REWRITE_REQUEST_VARIABLES.contains(&name)
        || extra.iter().any(|variable| variable == name)
        || name
            .strip_prefix("http_")
            .is_some_and(|suffix| !suffix.is_empty())
        || name
            .strip_prefix("arg_")
            .is_some_and(|suffix| !suffix.is_empty())
        || name
            .strip_prefix("cookie_")
            .is_some_and(|suffix| !suffix.is_empty())
}

fn validate_rewrite_variable_name(
    directive: &NginxDirective,
    name: &str,
) -> Result<(), NginxConfigError> {
    if name.is_empty()
        || name.len() > 128
        || name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "`{name}` is not a supported rewrite variable name; use letters, digits, or `_` and do not start with a digit"
            ),
        ));
    }
    Ok(())
}

fn paren_balance(value: &str) -> i64 {
    value.bytes().fold(0_i64, |balance, byte| match byte {
        b'(' => balance + 1,
        b')' => balance - 1,
        _ => balance,
    })
}

fn strip_condition_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn validate_rewrite_template(
    directive: &NginxDirective,
    template: &str,
    known: &[String],
) -> Result<(), NginxConfigError> {
    let variables = crate::config::template_variables(template)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    for variable in variables {
        if !rewrite_variable_is_known(&variable.name, known) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "rewrite-phase variable `${{{}}}` is not resolvable here; it must be a request variable, a declared `map` result, a regex capture, or an earlier `set`",
                    variable.name
                ),
            ));
        }
    }
    Ok(())
}

fn parse_set_operation(
    directive: &NginxDirective,
    known: &mut Vec<String>,
) -> Result<Value, NginxConfigError> {
    let [name, value] = directive.args.as_slice() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "`set` requires `$variable value`",
        ));
    };
    let Some(variable) = name.strip_prefix('$') else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("`set` target `{name}` must be a `$variable`"),
        ));
    };
    validate_rewrite_variable_name(directive, variable)?;
    validate_rewrite_template(directive, value, known)?;
    if !known.iter().any(|existing| existing == variable) {
        if known.len() >= crate::config::MAX_REWRITE_PHASE_VARIABLES {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "rewrite phase assigns more than {} variables",
                    crate::config::MAX_REWRITE_PHASE_VARIABLES
                ),
            ));
        }
        known.push(variable.to_owned());
    }
    Ok(json!({ "action": "set", "variable": variable, "value": value }))
}

/// Parse one `if (<condition>) { … }` directive into an executable rewrite
/// instruction, registering variables its body assigns so later instructions
/// may reference them.
fn parse_if_operation(
    directive: &NginxDirective,
    known: &mut Vec<String>,
    map_variables: &[&str],
    nested: bool,
) -> Result<Value, NginxConfigError> {
    if nested {
        return Err(NginxConfigError::unsupported(
            directive,
            "nested `if` blocks are not executable by the bounded rewrite engine",
        ));
    }
    // The tokenizer keeps an unquoted closing `)` inside the last token but
    // consumes a `)` that follows a quoted token as the nginx `need_space`
    // delimiter, so the assembled condition may lack its closing parenthesis.
    let joined = directive.args.join(" ");
    let trimmed = joined.trim();
    let Some(inner) = trimmed.strip_prefix('(') else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("`if` condition must be parenthesized, found `{trimmed}`"),
        ));
    };
    let inner = inner.trim_end();
    // Drop the condition's own closing parenthesis only when the assembled
    // text is unbalanced. A quoted/unquoted regex may legitimately end with a
    // group `)`; a naive suffix strip would truncate it.
    let mut inner = inner;
    while inner.ends_with(')') && paren_balance(inner) < 0 {
        inner = inner[..inner.len() - 1].trim_end();
    }
    let inner = inner;
    let mut tokens = inner.split_whitespace();
    let Some(left) = tokens.next() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "`if` condition is empty",
        ));
    };
    let negated_variable = left.starts_with('!');
    let left = left.strip_prefix('!').unwrap_or(left);
    let Some(variable) = left.strip_prefix('$') else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("`if` left-hand side `{left}` must be a `$variable`"),
        ));
    };
    validate_rewrite_variable_name(directive, variable)?;
    let rest: Vec<&str> = tokens.collect();
    let (operator, value) = if rest.is_empty() {
        (
            if negated_variable {
                "negated-truthy"
            } else {
                "truthy"
            },
            None,
        )
    } else {
        if negated_variable {
            return Err(NginxConfigError::unsupported(
                directive,
                "`!$variable` is only valid as a bare truthiness test",
            ));
        }
        let operator = match rest[0] {
            "=" => "equals",
            "!=" => "not-equals",
            "~" => "regex",
            "!~" => "not-regex",
            "~*" => "regex-ignore-case",
            "!~*" => "not-regex-ignore-case",
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`if` operator `{other}` is not executable; supported: `=`, `!=`, `~`, `~*`, `!~`, `!~*`, and bare truthiness"
                    ),
                ))
            }
        };
        let raw = rest[1..].join(" ");
        if raw.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("`if` operator `{}` requires a right-hand operand", rest[0]),
            ));
        }
        let raw = strip_condition_quotes(&raw).to_owned();
        (operator, Some(raw))
    };
    let mut condition = json!({ "variable": variable, "operator": operator });
    if let Some(raw) = value {
        if operator.contains("regex") {
            if raw.len() > 1_024 {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`if` regex exceeds the 1024-byte bound",
                ));
            }
            let pattern = if operator.ends_with("ignore-case") {
                format!("(?i){raw}")
            } else {
                raw.clone()
            };
            regex::Regex::new(&pattern).map_err(|error| {
                NginxConfigError::unsupported(
                    directive,
                    format!("`if` regex `{raw}` is invalid: {error}"),
                )
            })?;
        } else if raw.contains('$') {
            validate_rewrite_template(directive, &raw, known)?;
        }
        condition["value"] = Value::String(raw);
    }
    let mut body = Vec::new();
    for child in &directive.children {
        match child.name.as_str() {
            "set" => body.push(parse_set_operation(child, known)?),
            "add_header" => {
                let mut header = parse_add_header(child, map_variables)?;
                header["action"] = Value::String("add-header".to_owned());
                body.push(header);
            }
            "return" => body.push(parse_rewrite_phase_return(child)?),
            "rewrite" => {
                let mut rule = parse_rewrite_rule(child)?;
                rule["action"] = Value::String("rewrite".to_owned());
                body.push(rule);
            }
            "if" => {
                return Err(NginxConfigError::unsupported(
                    child,
                    "nested `if` blocks are not executable by the bounded rewrite engine",
                ))
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    child,
                    format!(
                        "`if` body directive `{other}` is not executable by the bounded rewrite engine; supported: set, add_header, return, rewrite"
                    ),
                ))
            }
        }
    }
    Ok(json!({ "action": "if", "condition": condition, "body": body }))
}

/// Validate a request-time `proxy_pass` template against the variables the
/// bounded rewrite engine can resolve in this context.
fn validate_dynamic_proxy_target(
    directive: &NginxDirective,
    target: &str,
    known: &[String],
) -> Result<(), NginxConfigError> {
    let variables = crate::config::template_variables(target)
        .map_err(|message| NginxConfigError::unsupported(directive, message))?;
    for variable in variables {
        if !rewrite_variable_is_known(&variable.name, known) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "variable `proxy_pass` target `{target}` uses `${{{}}}`, which is not resolvable here; use a request variable, a declared `map` result, a regex capture, or an earlier `set`",
                    variable.name
                ),
            ));
        }
    }
    Ok(())
}

/// Parse a rewrite-phase `return <code> [body|url]` / `return <url>`.
fn parse_rewrite_phase_return(directive: &NginxDirective) -> Result<Value, NginxConfigError> {
    let Some(first) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "`return` requires a status or URL",
        ));
    };
    let Ok(status) = first.parse::<u16>() else {
        if directive.args.len() != 1 {
            return Err(NginxConfigError::unsupported(
                directive,
                "the `return URL` form takes exactly one argument",
            ));
        }
        if first.contains('$') && !redirect_variables_ok(first) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("return URL `{first}` uses unsupported variables; supported: $host $request_uri $scheme"),
            ));
        }
        return Ok(json!({ "action": "return", "status": 302, "location": first }));
    };
    if status == 444 {
        return Err(NginxConfigError::unsupported(
            directive,
            "`return 444` is not supported by the runtime model",
        ));
    }
    if !(200..=599).contains(&status) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("`return {status}` is outside 200..=599"),
        ));
    }
    match (status, directive.args.get(1), directive.args.len()) {
        (301 | 302 | 303 | 307 | 308, Some(url), 2) => {
            if url.contains('$') && !redirect_variables_ok(url) {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("return URL `{url}` uses unsupported variables; supported: $host $request_uri $scheme"),
                ));
            }
            Ok(json!({ "action": "return", "status": status, "location": url }))
        }
        (301 | 302 | 303 | 307 | 308, _, _) => Err(NginxConfigError::unsupported(
            directive,
            "a redirect `return` requires exactly one URL argument",
        )),
        (_, None, _) => Ok(json!({ "action": "return", "status": status })),
        (_, Some(body), 2) => {
            if body.contains('$') {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`return` bodies in a rewrite phase must be literal text (variables are not expanded)",
                ));
            }
            Ok(json!({ "action": "return", "status": status, "body": body }))
        }
        _ => Err(NginxConfigError::unsupported(
            directive,
            "`return` takes a status and at most one body/URL argument",
        )),
    }
}

/// Parse `rewrite <pattern> <replacement> <flag>` into a runtime rule.
/// Unsupported flags fail closed with a precise diagnostic.
fn parse_rewrite_rule(directive: &NginxDirective) -> Result<Value, NginxConfigError> {
    if directive.args.len() != 3 {
        return Err(NginxConfigError::unsupported(
            directive,
            "rewrite requires a pattern, replacement, and flag",
        ));
    }
    let flag = match directive.args[2].as_str() {
        "break" => "break",
        "last" => "last",
        "redirect" => "redirect",
        "permanent" => "permanent",
        other => {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("unsupported rewrite flag `{other}`"),
            ))
        }
    };
    Ok(json!({
        "pattern": directive.args[0],
        "replacement": directive.args[1],
        "flag": flag,
    }))
}

/// Parse `allow` / `deny` into a runtime access rule.
/// Shared `sub_filter <from> <to>` application (http/server/location).
fn apply_sub_filter_entry(
    target: &mut Option<Value>,
    directive: &NginxDirective,
) -> Result<(), NginxConfigError> {
    let Some(from) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "sub_filter requires a pattern to replace",
        ));
    };
    if from.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "sub_filter pattern must not be empty",
        ));
    }
    let to = directive.args.get(1).cloned().unwrap_or_default();
    let entry = target.get_or_insert_with(
        || json!({ "rules": [], "once": true, "types": ["text/html"], "lastModified": false }),
    );
    entry["rules"]
        .as_array_mut()
        .expect("rules array")
        .push(json!({ "from": from, "to": to }));
    Ok(())
}

/// Shared `sub_filter_once on|off` application.
fn apply_sub_filter_once(
    target: &mut Option<Value>,
    directive: &NginxDirective,
) -> Result<(), NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "sub_filter_once requires on|off",
        ));
    };
    let enabled = match value.as_str() {
        "on" => true,
        "off" => false,
        other => {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("sub_filter_once accepts on|off, found `{other}`"),
            ))
        }
    };
    let entry = target.get_or_insert_with(
        || json!({ "rules": [], "once": true, "types": ["text/html"], "lastModified": false }),
    );
    entry["once"] = Value::Bool(enabled);
    Ok(())
}

/// Shared `sub_filter_types <mime…>` application.
fn apply_sub_filter_types(
    target: &mut Option<Value>,
    directive: &NginxDirective,
) -> Result<(), NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "sub_filter_types requires at least one MIME type",
        ));
    }
    let types: Vec<Value> = directive.args.iter().cloned().map(Value::String).collect();
    let entry = target.get_or_insert_with(
        || json!({ "rules": [], "once": true, "types": ["text/html"], "lastModified": false }),
    );
    entry["types"] = Value::Array(types);
    Ok(())
}

/// Shared `on|off` parser for simple switch directives.
/// Parse nginx `valid_referers` (server/location context; a declared
/// directive replaces the inherited list).
fn parse_valid_referers(directive: &NginxDirective) -> Result<RefererConfig, NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "valid_referers requires at least one matcher",
        ));
    }
    if directive.args.len() > 256 {
        return Err(NginxConfigError::unsupported(
            directive,
            "valid_referers supports at most 256 matchers",
        ));
    }
    let mut config = RefererConfig::default();
    for argument in &directive.args {
        let argument = argument.trim();
        if argument.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "invalid empty valid_referers argument",
            ));
        }
        match argument {
            "none" => {
                config.none = true;
                continue;
            }
            "blocked" => {
                config.blocked = true;
                continue;
            }
            "server_names" => {
                config.server_names = true;
                continue;
            }
            _ => {}
        }
        if let Some(pattern) = argument.strip_prefix('~') {
            if pattern.is_empty() || pattern.len() > 512 {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("invalid valid_referers regex `{argument}`"),
                ));
            }
            if crate::config::compile_referer_regex(pattern).is_none() {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("valid_referers regex `{argument}` does not compile"),
                ));
            }
            config.regexes.push(pattern.to_owned());
            continue;
        }
        // `host[/path]`: nginx splits at the first `/` into a host matcher and
        // an optional URI prefix.
        let (host, path) = match argument.split_once('/') {
            Some((host, path)) => (host, Some(format!("/{path}"))),
            None => (argument, None),
        };
        let host = host.to_ascii_lowercase();
        let wildcard_suffix = host.strip_prefix("*.").or_else(|| host.strip_prefix('.'));
        let valid_host = !host.is_empty()
            && host.len() <= 253
            && match wildcard_suffix {
                Some(suffix) => {
                    !suffix.is_empty() && !suffix.contains('*') && !suffix.starts_with('.')
                }
                None => !host.contains('*'),
            };
        if !valid_host {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "invalid valid_referers host `{argument}`; supported forms are an exact host, `*.suffix`, `.suffix`, `server_names`, `none`, `blocked`, and `~regex`"
                ),
            ));
        }
        config.hosts.push(RefererHostEntry { host, path });
    }
    // nginx rejects `none`/`blocked` when no positive matcher is declared.
    if (config.none || config.blocked)
        && config.hosts.is_empty()
        && config.regexes.is_empty()
        && !config.server_names
    {
        return Err(NginxConfigError::unsupported(
            directive,
            "valid_referers `none`/`blocked` requires at least one host, wildcard, `~regex`, or `server_names` matcher (nginx rejects the configuration)",
        ));
    }
    Ok(config)
}

/// Parse `http2_max_concurrent_streams <1..10000>` (nginx http/server
/// context; the runtime ingress model owns one value).
fn parse_http2_max_concurrent_streams(directive: &NginxDirective) -> Result<u32, NginxConfigError> {
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            "http2_max_concurrent_streams takes exactly one stream count",
        ));
    }
    let value = &directive.args[0];
    let Ok(streams) = value.parse::<u32>() else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid http2_max_concurrent_streams `{value}`; expected 1..10000"),
        ));
    };
    if !(1..=10_000).contains(&streams) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "http2_max_concurrent_streams `{value}` is outside the runtime's 1..10000 range"
            ),
        ));
    }
    Ok(streams)
}

/// Parse `proxy_method <method>` (an RFC 9110 method token).
fn parse_proxy_method(directive: &NginxDirective) -> Result<String, NginxConfigError> {
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            "proxy_method takes exactly one method",
        ));
    }
    let method = &directive.args[0];
    if method.is_empty()
        || method.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                ))
        })
    {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("proxy_method `{method}` is not a valid HTTP method token"),
        ));
    }
    Ok(method.clone())
}

/// nginx `memcached_*` settings accumulated over the http → server → location
/// inheritance chain (the module has no `*_param`; the request key comes from
/// `set $memcached_key <value>` in the location).
#[derive(Clone, Default)]
struct MemcachedSettings {
    bind_address: Option<Option<std::net::IpAddr>>,
    socket_keepalive: Option<bool>,
    buffering: Option<bool>,
    connect_timeout_ms: Option<u64>,
    send_timeout_ms: Option<u64>,
    read_timeout_ms: Option<u64>,
    next_upstream: Option<Vec<UpstreamRetryCondition>>,
    next_upstream_tries: Option<u8>,
    next_upstream_timeout_ms: Option<u64>,
}

impl MemcachedSettings {
    fn overlay(&self, inner: &Self) -> Self {
        Self {
            bind_address: inner.bind_address.or(self.bind_address),
            socket_keepalive: inner.socket_keepalive.or(self.socket_keepalive),
            buffering: inner.buffering.or(self.buffering),
            connect_timeout_ms: inner.connect_timeout_ms.or(self.connect_timeout_ms),
            send_timeout_ms: inner.send_timeout_ms.or(self.send_timeout_ms),
            read_timeout_ms: inner.read_timeout_ms.or(self.read_timeout_ms),
            next_upstream: inner
                .next_upstream
                .clone()
                .or_else(|| self.next_upstream.clone()),
            next_upstream_tries: inner.next_upstream_tries.or(self.next_upstream_tries),
            next_upstream_timeout_ms: inner
                .next_upstream_timeout_ms
                .or(self.next_upstream_timeout_ms),
        }
    }

    fn apply(&mut self, directive: &NginxDirective) -> Result<(), NginxConfigError> {
        let Some(suffix) = directive.name.strip_prefix("memcached_") else {
            return Err(NginxConfigError::unsupported(
                directive,
                "unsupported memcached directive",
            ));
        };
        match suffix {
            "buffering" => self.buffering = Some(parse_on_off(directive, &directive.name)?),
            "bind" => self.bind_address = Some(parse_upstream_bind(directive)?),
            "socket_keepalive" => {
                self.socket_keepalive = Some(parse_on_off(directive, &directive.name)?)
            }
            "connect_timeout" => self.connect_timeout_ms = Some(parse_fastcgi_timeout(directive)?),
            "send_timeout" => self.send_timeout_ms = Some(parse_fastcgi_timeout(directive)?),
            "read_timeout" => self.read_timeout_ms = Some(parse_fastcgi_timeout(directive)?),
            "next_upstream" => self.next_upstream = Some(parse_fastcgi_next_upstream(directive)?),
            "next_upstream_tries" => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "memcached_next_upstream_tries requires a count",
                    ));
                };
                self.next_upstream_tries = Some(value.parse::<u8>().map_err(|_| {
                    NginxConfigError::unsupported(
                        directive,
                        format!("invalid memcached_next_upstream_tries `{value}`"),
                    )
                })?);
            }
            "next_upstream_timeout" => {
                self.next_upstream_timeout_ms = Some(parse_fastcgi_timeout(directive)?)
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`memcached_{other}` is not executable by the bounded memcached adapter; supported: memcached_pass/memcached_bind/memcached_socket_keepalive/memcached_connect_timeout/memcached_send_timeout/memcached_read_timeout/memcached_buffering/memcached_next_upstream/_tries/_timeout (nginx's memcached module has no hide/pass/intercept_errors directives)"
                    ),
                ))
            }
        }
        Ok(())
    }
}

/// CGI-family protocol of one location handler (`uwsgi_pass` / `scgi_pass`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum CgiKind {
    Uwsgi,
    Scgi,
}

impl CgiKind {
    fn from_directive(name: &str) -> Option<Self> {
        if name.starts_with("uwsgi_") {
            Some(Self::Uwsgi)
        } else if name.starts_with("scgi_") {
            Some(Self::Scgi)
        } else {
            None
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Uwsgi => "uwsgi",
            Self::Scgi => "scgi",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Uwsgi => 0,
            Self::Scgi => 1,
        }
    }

    fn protocol(self) -> crate::config::CgiUpstreamProtocol {
        match self {
            Self::Uwsgi => crate::config::CgiUpstreamProtocol::Uwsgi,
            Self::Scgi => crate::config::CgiUpstreamProtocol::Scgi,
        }
    }
}

/// nginx `uwsgi_*` / `scgi_*` settings accumulated over the http → server →
/// location inheritance chain (the two modules share one shape; `uwsgi_modifier*`
/// and `uwsgi_string` are uWSGI-only).
#[derive(Clone, Default)]
struct CgiSettings {
    params: Option<Vec<FastCgiParamSetting>>,
    pass_request_headers: Option<bool>,
    bind_address: Option<Option<std::net::IpAddr>>,
    socket_keepalive: Option<bool>,
    buffering: Option<bool>,
    request_buffering: Option<bool>,
    connect_timeout_ms: Option<u64>,
    send_timeout_ms: Option<u64>,
    read_timeout_ms: Option<u64>,
    intercept_errors: Option<bool>,
    hide_headers: Option<Vec<String>>,
    pass_headers: Option<Vec<String>>,
    next_upstream: Option<Vec<UpstreamRetryCondition>>,
    next_upstream_tries: Option<u8>,
    next_upstream_timeout_ms: Option<u64>,
    modifier1: Option<u8>,
    modifier2: Option<u8>,
    extra_string: Option<String>,
}

impl CgiSettings {
    fn overlay(&self, inner: &Self) -> Self {
        Self {
            params: inner.params.clone().or_else(|| self.params.clone()),
            pass_request_headers: inner.pass_request_headers.or(self.pass_request_headers),
            bind_address: inner.bind_address.or(self.bind_address),
            socket_keepalive: inner.socket_keepalive.or(self.socket_keepalive),
            buffering: inner.buffering.or(self.buffering),
            request_buffering: inner.request_buffering.or(self.request_buffering),
            connect_timeout_ms: inner.connect_timeout_ms.or(self.connect_timeout_ms),
            send_timeout_ms: inner.send_timeout_ms.or(self.send_timeout_ms),
            read_timeout_ms: inner.read_timeout_ms.or(self.read_timeout_ms),
            intercept_errors: inner.intercept_errors.or(self.intercept_errors),
            hide_headers: inner
                .hide_headers
                .clone()
                .or_else(|| self.hide_headers.clone()),
            pass_headers: inner
                .pass_headers
                .clone()
                .or_else(|| self.pass_headers.clone()),
            next_upstream: inner
                .next_upstream
                .clone()
                .or_else(|| self.next_upstream.clone()),
            next_upstream_tries: inner.next_upstream_tries.or(self.next_upstream_tries),
            next_upstream_timeout_ms: inner
                .next_upstream_timeout_ms
                .or(self.next_upstream_timeout_ms),
            modifier1: inner.modifier1.or(self.modifier1),
            modifier2: inner.modifier2.or(self.modifier2),
            extra_string: inner
                .extra_string
                .clone()
                .or_else(|| self.extra_string.clone()),
        }
    }

    fn apply(
        &mut self,
        kind: CgiKind,
        directive: &NginxDirective,
        map_variables: &[&str],
    ) -> Result<(), NginxConfigError> {
        let prefix = kind.prefix();
        let Some(suffix) = directive
            .name
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('_'))
        else {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("`{}` is not a `{prefix}_*` directive", directive.name),
            ));
        };
        match suffix {
            "param" => {
                let (name, value, if_not_empty) = match directive.args.as_slice() {
                    [name, value] => (name, value, false),
                    [name, value, flag] if flag.eq_ignore_ascii_case("if_not_empty") => {
                        (name, value, true)
                    }
                    _ => {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            format!("{prefix}_param requires `NAME VALUE [if_not_empty]`"),
                        ))
                    }
                };
                crate::config::validate_param_name(name)
                    .map_err(|message| NginxConfigError::unsupported(directive, message))?;
                crate::config::validate_param_value(value, map_variables)
                    .map_err(|message| NginxConfigError::unsupported(directive, message))?;
                self.params
                    .get_or_insert_with(Vec::new)
                    .push(FastCgiParamSetting {
                        name: name.clone(),
                        value: value.clone(),
                        if_not_empty,
                    });
            }
            "pass_request_headers" => {
                self.pass_request_headers = Some(parse_on_off(directive, &directive.name)?)
            }
            "buffering" => self.buffering = Some(parse_on_off(directive, &directive.name)?),
            "bind" => self.bind_address = Some(parse_upstream_bind(directive)?),
            "socket_keepalive" => {
                self.socket_keepalive = Some(parse_on_off(directive, &directive.name)?)
            }
            "request_buffering" => {
                self.request_buffering = Some(parse_on_off(directive, &directive.name)?)
            }
            "connect_timeout" => self.connect_timeout_ms = Some(parse_cgi_timeout(directive)?),
            "send_timeout" => self.send_timeout_ms = Some(parse_cgi_timeout(directive)?),
            "read_timeout" => self.read_timeout_ms = Some(parse_cgi_timeout(directive)?),
            "intercept_errors" => {
                self.intercept_errors = Some(parse_on_off(directive, &directive.name)?)
            }
            "hide_header" | "pass_header" => {
                let Some(name) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("{} requires a header name", directive.name),
                    ));
                };
                if !crate::config::header_block_token(name) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("{} `{name}` is not a valid HTTP field name", directive.name),
                    ));
                }
                let list = if suffix == "hide_header" {
                    self.hide_headers.get_or_insert_with(Vec::new)
                } else {
                    self.pass_headers.get_or_insert_with(Vec::new)
                };
                list.push(name.clone());
            }
            "next_upstream" => self.next_upstream = Some(parse_fastcgi_next_upstream(directive)?),
            "next_upstream_tries" => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "next_upstream_tries requires a count",
                    ));
                };
                self.next_upstream_tries = Some(value.parse::<u8>().map_err(|_| {
                    NginxConfigError::unsupported(
                        directive,
                        format!("invalid {} `{value}`", directive.name),
                    )
                })?);
            }
            "next_upstream_timeout" => {
                self.next_upstream_timeout_ms = Some(parse_cgi_timeout(directive)?)
            }
            "modifier1" | "modifier2" if kind == CgiKind::Uwsgi => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("{} requires a number", directive.name),
                    ));
                };
                let parsed = value.parse::<u8>().map_err(|_| {
                    NginxConfigError::unsupported(
                        directive,
                        format!("{} must be a byte 0..=255", directive.name),
                    )
                })?;
                if suffix == "modifier1" {
                    self.modifier1 = Some(parsed);
                } else {
                    self.modifier2 = Some(parsed);
                }
            }
            "string" if kind == CgiKind::Uwsgi => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "uwsgi_string requires a value",
                    ));
                };
                if value.bytes().any(|byte| byte == 0 || byte == b'\r' || byte == b'\n') {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "uwsgi_string must not contain NUL or line breaks",
                    ));
                }
                self.extra_string = Some(value.clone());
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{prefix}_{other}` is not executable by the bounded {prefix} adapter; supported: {prefix}_pass/{prefix}_param/{prefix}_pass_request_headers/{prefix}_bind/{prefix}_socket_keepalive/{prefix}_buffering/{prefix}_request_buffering/{prefix}_connect_timeout/{prefix}_send_timeout/{prefix}_read_timeout/{prefix}_hide_header/{prefix}_pass_header/{prefix}_intercept_errors/{prefix}_next_upstream/_tries/_timeout{}",
                        if kind == CgiKind::Uwsgi {
                            "/uwsgi_modifier1/uwsgi_modifier2/uwsgi_string"
                        } else {
                            ""
                        }
                    ),
                ))
            }
        }
        Ok(())
    }
}

fn parse_cgi_timeout(directive: &NginxDirective) -> Result<u64, NginxConfigError> {
    parse_fastcgi_timeout(directive)
}

/// nginx proxy response-header directives (`proxy_hide_header`,
/// `proxy_pass_header`, `proxy_redirect`, `proxy_cookie_path`,
/// nginx `fastcgi_*` settings accumulated over the http → server → location
/// inheritance chain.
///
/// Each nginx directive family has its own inheritance rule: `fastcgi_param`
/// and the hide/pass header lists replace/extend at the level that declares
/// them, while the remaining single-value directives inherit independently.
#[derive(Clone, Default)]
struct FastCgiSettings {
    params: Option<Vec<FastCgiParamSetting>>,
    index: Option<String>,
    split_path_info: Option<String>,
    keep_conn: Option<bool>,
    bind_address: Option<Option<std::net::IpAddr>>,
    socket_keepalive: Option<bool>,
    buffering: Option<bool>,
    request_buffering: Option<bool>,
    pass_request_headers: Option<bool>,
    connect_timeout_ms: Option<u64>,
    send_timeout_ms: Option<u64>,
    read_timeout_ms: Option<u64>,
    intercept_errors: Option<bool>,
    hide_headers: Option<Vec<String>>,
    pass_headers: Option<Vec<String>>,
    next_upstream: Option<Vec<UpstreamRetryCondition>>,
    next_upstream_tries: Option<u8>,
    next_upstream_timeout_ms: Option<u64>,
}

#[derive(Clone)]
struct FastCgiParamSetting {
    name: String,
    value: String,
    if_not_empty: bool,
}

impl FastCgiSettings {
    /// Overlay the more specific `inner` level on this (inherited) one.
    fn overlay(&self, inner: &Self) -> Self {
        Self {
            params: inner.params.clone().or_else(|| self.params.clone()),
            index: inner.index.clone().or_else(|| self.index.clone()),
            split_path_info: inner
                .split_path_info
                .clone()
                .or_else(|| self.split_path_info.clone()),
            keep_conn: inner.keep_conn.or(self.keep_conn),
            bind_address: inner.bind_address.or(self.bind_address),
            socket_keepalive: inner.socket_keepalive.or(self.socket_keepalive),
            buffering: inner.buffering.or(self.buffering),
            request_buffering: inner.request_buffering.or(self.request_buffering),
            pass_request_headers: inner.pass_request_headers.or(self.pass_request_headers),
            connect_timeout_ms: inner.connect_timeout_ms.or(self.connect_timeout_ms),
            send_timeout_ms: inner.send_timeout_ms.or(self.send_timeout_ms),
            read_timeout_ms: inner.read_timeout_ms.or(self.read_timeout_ms),
            intercept_errors: inner.intercept_errors.or(self.intercept_errors),
            hide_headers: inner
                .hide_headers
                .clone()
                .or_else(|| self.hide_headers.clone()),
            pass_headers: inner
                .pass_headers
                .clone()
                .or_else(|| self.pass_headers.clone()),
            next_upstream: inner
                .next_upstream
                .clone()
                .or_else(|| self.next_upstream.clone()),
            next_upstream_tries: inner.next_upstream_tries.or(self.next_upstream_tries),
            next_upstream_timeout_ms: inner
                .next_upstream_timeout_ms
                .or(self.next_upstream_timeout_ms),
        }
    }

    /// Apply one `fastcgi_*` directive to this level's settings.
    ///
    /// `fastcgi_pass` is handled by the caller (it selects the handler), and
    /// directives outside the executable subset return a located diagnostic.
    fn apply(
        &mut self,
        directive: &NginxDirective,
        map_variables: &[&str],
    ) -> Result<(), NginxConfigError> {
        match directive.name.as_str() {
            "fastcgi_param" => {
                let (name, value, if_not_empty) = match directive.args.as_slice() {
                    [name, value] => (name, value, false),
                    [name, value, flag] if flag.eq_ignore_ascii_case("if_not_empty") => {
                        (name, value, true)
                    }
                    _ => {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            "fastcgi_param requires `NAME VALUE [if_not_empty]`",
                        ))
                    }
                };
                crate::config::validate_param_name(name)
                    .map_err(|message| NginxConfigError::unsupported(directive, message))?;
                crate::config::validate_param_value(value, map_variables)
                    .map_err(|message| NginxConfigError::unsupported(directive, message))?;
                self.params
                    .get_or_insert_with(Vec::new)
                    .push(FastCgiParamSetting {
                        name: name.clone(),
                        value: value.clone(),
                        if_not_empty,
                    });
            }
            "fastcgi_index" => {
                let Some(index) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "fastcgi_index requires a file name",
                    ));
                };
                if index.is_empty()
                    || index.starts_with('/')
                    || index
                        .bytes()
                        .any(|byte| byte == 0 || byte < 0x20 || byte == 0x7f)
                {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "fastcgi_index must be a relative file name",
                    ));
                }
                self.index = Some(index.clone());
            }
            "fastcgi_split_path_info" => {
                let Some(pattern) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "fastcgi_split_path_info requires a regular expression",
                    ));
                };
                let regex = regex::Regex::new(pattern).map_err(|_| {
                    NginxConfigError::unsupported(
                        directive,
                        "fastcgi_split_path_info is not a valid regular expression",
                    )
                })?;
                if regex.captures_len() != 3 {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "fastcgi_split_path_info must contain exactly two capture groups",
                    ));
                }
                self.split_path_info = Some(pattern.clone());
            }
            "fastcgi_keep_conn" => self.keep_conn = Some(parse_on_off(directive, "fastcgi_keep_conn")?),
            "fastcgi_bind" => self.bind_address = Some(parse_upstream_bind(directive)?),
            "fastcgi_socket_keepalive" => {
                self.socket_keepalive = Some(parse_on_off(directive, "fastcgi_socket_keepalive")?)
            }
            "fastcgi_buffering" => {
                self.buffering = Some(parse_on_off(directive, "fastcgi_buffering")?)
            }
            "fastcgi_request_buffering" => {
                self.request_buffering = Some(parse_on_off(directive, "fastcgi_request_buffering")?)
            }
            "fastcgi_pass_request_headers" => {
                self.pass_request_headers =
                    Some(parse_on_off(directive, "fastcgi_pass_request_headers")?)
            }
            "fastcgi_connect_timeout" => {
                self.connect_timeout_ms = Some(parse_fastcgi_timeout(directive)?)
            }
            "fastcgi_send_timeout" => self.send_timeout_ms = Some(parse_fastcgi_timeout(directive)?),
            "fastcgi_read_timeout" => self.read_timeout_ms = Some(parse_fastcgi_timeout(directive)?),
            "fastcgi_intercept_errors" => {
                self.intercept_errors = Some(parse_on_off(directive, "fastcgi_intercept_errors")?)
            }
            "fastcgi_hide_header" | "fastcgi_pass_header" => {
                let Some(name) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("{} requires a header name", directive.name),
                    ));
                };
                if !crate::config::header_block_token(name) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                            "{} `{name}` is not a valid HTTP field name",
                            directive.name
                        ),
                    ));
                }
                let list = if directive.name == "fastcgi_hide_header" {
                    self.hide_headers.get_or_insert_with(Vec::new)
                } else {
                    self.pass_headers.get_or_insert_with(Vec::new)
                };
                list.push(name.clone());
            }
            "fastcgi_next_upstream" => {
                self.next_upstream = Some(parse_fastcgi_next_upstream(directive)?);
            }
            "fastcgi_next_upstream_tries" => {
                let Some(value) = directive.args.first() else {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "fastcgi_next_upstream_tries requires a count",
                    ));
                };
                let tries = value.parse::<u8>().map_err(|_| {
                    NginxConfigError::unsupported(
                        directive,
                        format!("invalid fastcgi_next_upstream_tries `{value}`"),
                    )
                })?;
                self.next_upstream_tries = Some(tries);
            }
            "fastcgi_next_upstream_timeout" => {
                self.next_upstream_timeout_ms = Some(parse_fastcgi_timeout(directive)?)
            }
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{other}` is not executable by the bounded FastCGI adapter; supported: fastcgi_pass/fastcgi_param/fastcgi_index/fastcgi_split_path_info/fastcgi_keep_conn/fastcgi_bind/fastcgi_socket_keepalive/fastcgi_buffering/fastcgi_request_buffering/fastcgi_pass_request_headers/fastcgi_connect_timeout/fastcgi_send_timeout/fastcgi_read_timeout/fastcgi_hide_header/fastcgi_pass_header/fastcgi_intercept_errors/fastcgi_next_upstream/_tries/_timeout"
                    ),
                ))
            }
        }
        Ok(())
    }
}

/// Parse an nginx `*_bind` argument.
///
/// nginx accepts an IP literal (optionally followed by `transparent`) or
/// `off`. Only a literal address is executable by the bounded adapters:
/// hostnames need a resolver decision at bind time and `transparent` needs
/// privileged socket options, so both fail closed instead of silently relaxing
/// the source-address contract.
fn parse_upstream_bind(
    directive: &NginxDirective,
) -> Result<Option<std::net::IpAddr>, NginxConfigError> {
    match directive.args.as_slice() {
        [value] if value.eq_ignore_ascii_case("off") => Ok(None),
        [value] if !value.contains('$') => value.parse::<std::net::IpAddr>().map(Some).map_err(
            |_| {
                NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{}` requires an IP literal or `off`; hostnames, variables, and `transparent` binds are not executable by the bounded adapter",
                        directive.name
                    ),
                )
            },
        ),
        _ => Err(NginxConfigError::unsupported(
            directive,
            format!(
                "`{}` takes one argument: an IP literal or `off`",
                directive.name
            ),
        )),
    }
}

/// Parse one nginx timeout value into milliseconds (`ms`, `s`, `m`, `h`).
fn parse_fastcgi_timeout(directive: &NginxDirective) -> Result<u64, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires a time value", directive.name),
        ));
    };
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} takes exactly one time value", directive.name),
        ));
    }
    let (number, multiplier) = if let Some(rest) = value.strip_suffix("ms") {
        (rest, 1u64)
    } else if let Some(rest) = value.strip_suffix('s') {
        (rest, 1_000)
    } else if let Some(rest) = value.strip_suffix('m') {
        (rest, 60_000)
    } else if let Some(rest) = value.strip_suffix('h') {
        (rest, 3_600_000)
    } else {
        (value.as_str(), 1_000)
    };
    let number = number.trim().parse::<u64>().map_err(|_| {
        NginxConfigError::unsupported(directive, format!("invalid time value `{value}`"))
    })?;
    number
        .checked_mul(multiplier)
        .filter(|millis| (1..=86_400_000).contains(millis))
        .ok_or_else(|| {
            NginxConfigError::unsupported(
                directive,
                format!("time value `{value}` is outside 1ms..=24h"),
            )
        })
}

/// Parse `fastcgi_next_upstream` flags into the runtime retry conditions.
fn parse_fastcgi_next_upstream(
    directive: &NginxDirective,
) -> Result<Vec<UpstreamRetryCondition>, NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires at least one condition", directive.name),
        ));
    }
    let mut conditions = Vec::new();
    for flag in &directive.args {
        let condition = match flag.as_str() {
            "error" => UpstreamRetryCondition::TransportFailure,
            "timeout" => UpstreamRetryCondition::Timeout,
            "http_502" => UpstreamRetryCondition::Http502,
            "http_503" => UpstreamRetryCondition::Http503,
            "http_504" => UpstreamRetryCondition::Http504,
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "{} condition `{other}` is not executable by the bounded adapter; supported: error timeout http_502 http_503 http_504",
                        directive.name
                    ),
                ))
            }
        };
        if !conditions.contains(&condition) {
            conditions.push(condition);
        }
    }
    Ok(conditions)
}

/// nginx proxy response-header directives (`proxy_hide_header`,
/// `proxy_pass_header`, `proxy_redirect`, `proxy_cookie_path`,
/// `proxy_cookie_domain`) declared at one configuration level.
///
/// Each directive family in this group is inherited from the previous level
/// only when the current level declares none of it (nginx replace-on-declare),
/// so an undeclared field stays `None` through the http → server → location
/// merge chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ProxyResponseDirectives {
    hide_headers: Option<Vec<String>>,
    pass_headers: Option<Vec<String>>,
    redirect: Option<ProxyRedirectPolicyConfig>,
    cookie_path: Option<CookieRewritePolicyConfig>,
    cookie_domain: Option<CookieRewritePolicyConfig>,
}

impl ProxyResponseDirectives {
    /// Overlay `inner` (the more specific level) on `self`: a declared
    /// directive family replaces the inherited one; an undeclared one keeps it.
    fn overlay(self, inner: Self) -> Self {
        Self {
            hide_headers: inner.hide_headers.or(self.hide_headers),
            pass_headers: inner.pass_headers.or(self.pass_headers),
            redirect: inner.redirect.or(self.redirect),
            cookie_path: inner.cookie_path.or(self.cookie_path),
            cookie_domain: inner.cookie_domain.or(self.cookie_domain),
        }
    }

    /// Resolve the resource policy. `redirect` stays `None` when no level
    /// declared `proxy_redirect`; the caller decides between nginx's implicit
    /// `default` and the no-rule behavior of a variable `proxy_pass` target.
    fn resolve(&self) -> ProxyResponsePolicyConfig {
        ProxyResponsePolicyConfig {
            hide_headers: self.hide_headers.clone().unwrap_or_default(),
            pass_headers: self.pass_headers.clone().unwrap_or_default(),
            redirect: self.redirect.clone(),
            cookie_path: self.cookie_path.clone(),
            cookie_domain: self.cookie_domain.clone(),
        }
    }
}

/// HTTP token characters (RFC 9110 section 5.6.2) — the accepted header names
/// for `proxy_hide_header` / `proxy_pass_header`.
fn is_header_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn proxy_response_policy_json(
    policy: &ProxyResponsePolicyConfig,
) -> Result<Value, NginxConfigError> {
    serde_json::to_value(policy).map_err(|error| {
        NginxConfigError::ValidationFailed(format!("nginx materialization failed: {error}"))
    })
}

fn parse_proxy_header_name(directive: &NginxDirective) -> Result<String, NginxConfigError> {
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} takes exactly one header name", directive.name),
        ));
    }
    let name = &directive.args[0];
    if name.is_empty() || name.bytes().any(|byte| !is_header_token_byte(byte)) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "{} header name `{name}` is not a valid HTTP header token",
                directive.name
            ),
        ));
    }
    Ok(name.clone())
}

/// Shared validation for a `proxy_redirect` / `proxy_cookie_*` `<from> <to>`
/// pair. nginx compiles both sides as variable scripts and accepts a `~`
/// regular-expression `from`; the runtime executes literal rewriting only, so
/// any form it cannot execute fails closed instead of silently passing traffic
/// through.
fn validate_proxy_rewrite_rule(
    directive: &NginxDirective,
    from: &str,
    to: &str,
) -> Result<(), NginxConfigError> {
    if from.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires a non-empty <from> pattern", directive.name),
        ));
    }
    if from.starts_with('~') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "{} regular-expression form `{from}` is not supported; use literal `<from> <to>` rules",
                directive.name
            ),
        ));
    }
    if from.contains('$') || to.contains('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "{} variable expansion in `{from}` / `{to}` is not supported; nginx compiles both sides as variable scripts and the runtime executes literal rewriting only",
                directive.name
            ),
        ));
    }
    Ok(())
}

/// `proxy_redirect off|default|<from> <to>` at one configuration level.
fn apply_proxy_redirect_directive(
    state: &mut ProxyResponseDirectives,
    directive: &NginxDirective,
) -> Result<(), NginxConfigError> {
    match directive.args.as_slice() {
        [single] if single == "off" || single == "default" => {
            if state.redirect.is_some() {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`proxy_redirect {single}` must be the only proxy_redirect directive at its level"
                    ),
                ));
            }
            state.redirect = Some(if single == "off" {
                ProxyRedirectPolicyConfig::Off
            } else {
                ProxyRedirectPolicyConfig::Default
            });
            Ok(())
        }
        [from, to] => {
            validate_proxy_rewrite_rule(directive, from, to)?;
            let rule = ProxyRewriteRuleConfig {
                from: from.clone(),
                to: to.clone(),
            };
            match state.redirect.take() {
                None => state.redirect = Some(ProxyRedirectPolicyConfig::Rules(vec![rule])),
                Some(ProxyRedirectPolicyConfig::Rules(mut rules)) => {
                    rules.push(rule);
                    state.redirect = Some(ProxyRedirectPolicyConfig::Rules(rules));
                }
                Some(other) => {
                    state.redirect = Some(other);
                    return Err(NginxConfigError::unsupported(
                        directive,
                        "`proxy_redirect off|default` cannot be combined with explicit `proxy_redirect <from> <to>` rules at the same level",
                    ));
                }
            }
            Ok(())
        }
        _ => Err(NginxConfigError::unsupported(
            directive,
            "proxy_redirect requires `off`, `default`, or `<from> <to>`",
        )),
    }
}

/// `proxy_cookie_path` / `proxy_cookie_domain` `<from> <to>` or `off` at one
/// configuration level.
///
/// nginx strips one leading dot from a `proxy_cookie_domain` pattern and
/// replacement at configuration time and re-applies the dot the matched
/// attribute value carried (differential-confirmed against nginx 1.29.6), so
/// the stored rule is normalized the same way.
fn apply_cookie_rewrite_directive(
    directive: &NginxDirective,
    slot: &mut Option<CookieRewritePolicyConfig>,
    domain: bool,
) -> Result<(), NginxConfigError> {
    match directive.args.as_slice() {
        [single] if single == "off" => {
            if slot.is_some() {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{} off` must be the only {} directive at its level",
                        directive.name, directive.name
                    ),
                ));
            }
            *slot = Some(CookieRewritePolicyConfig::Off);
            Ok(())
        }
        [from, to] if from.starts_with('~') => {
            // `proxy_cookie_path ~[*]<pattern> <replacement>`: regex attribute
            // rewriting, evaluated with the same regex engine/limits as the
            // rest of the configuration.
            let (pattern, case_insensitive) = match from.strip_prefix("~*") {
                Some(pattern) => (pattern, true),
                None => (
                    from.strip_prefix('~').expect("guarded by starts_with('~')"),
                    false,
                ),
            };
            if pattern.is_empty() || pattern.len() > 1_024 {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`proxy_cookie_path` regex must be 1..=1024 bytes",
                ));
            }
            if domain {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`proxy_cookie_domain` accepts only the literal `<from> <to>` form",
                ));
            }
            let compiled = if case_insensitive {
                format!("(?i){pattern}")
            } else {
                pattern.to_owned()
            };
            regex::Regex::new(&compiled).map_err(|error| {
                NginxConfigError::unsupported(
                    directive,
                    format!("`proxy_cookie_path` regex `{from}` is invalid: {error}"),
                )
            })?;
            let rule = crate::config::ProxyRewriteRegexRuleConfig {
                pattern: pattern.to_owned(),
                replacement: to.clone(),
                case_insensitive,
            };
            match slot {
                Some(CookieRewritePolicyConfig::RegexRules(rules)) => rules.push(rule),
                Some(_) => return Err(NginxConfigError::unsupported(
                    directive,
                    "`proxy_cookie_path` literal and regex rules cannot be mixed at the same level",
                )),
                None => *slot = Some(CookieRewritePolicyConfig::RegexRules(vec![rule])),
            }
            Ok(())
        }
        [from, to] => {
            validate_proxy_rewrite_rule(directive, from, to)?;
            let rule = if domain {
                ProxyRewriteRuleConfig {
                    from: from.strip_prefix('.').unwrap_or(from).to_owned(),
                    to: to.strip_prefix('.').unwrap_or(to).to_owned(),
                }
            } else {
                ProxyRewriteRuleConfig {
                    from: from.clone(),
                    to: to.clone(),
                }
            };
            if slot.is_none() {
                *slot = Some(CookieRewritePolicyConfig::Rules(vec![rule]));
                Ok(())
            } else if let Some(CookieRewritePolicyConfig::Rules(rules)) = slot {
                rules.push(rule);
                Ok(())
            } else {
                Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "`{} off` cannot be combined with explicit rules at the same level",
                        directive.name
                    ),
                ))
            }
        }
        _ => Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires `off` or `<from> <to>`", directive.name),
        )),
    }
}

/// nginx autoindex family declarations (`autoindex`,
/// `autoindex_exact_size`, `autoindex_localtime`, `autoindex_format`) at one
/// configuration level. Every field inherits the previous level's value when
/// undeclared (nginx merges all four with `ngx_conf_merge_value`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AutoindexDeclarations {
    enabled: Option<bool>,
    exact_size: Option<bool>,
    localtime: Option<bool>,
    format: Option<AutoindexFormat>,
}

impl AutoindexDeclarations {
    fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.exact_size.is_none()
            && self.localtime.is_none()
            && self.format.is_none()
    }

    fn overlay(self, inner: Self) -> Self {
        Self {
            enabled: inner.enabled.or(self.enabled),
            exact_size: inner.exact_size.or(self.exact_size),
            localtime: inner.localtime.or(self.localtime),
            format: inner.format.or(self.format),
        }
    }

    /// Materialize the effective policy; `None` means no level declared the
    /// family (nginx default: listings off).
    fn resolve(&self) -> Option<AutoindexConfig> {
        if self.is_empty() {
            return None;
        }
        Some(AutoindexConfig {
            enabled: self.enabled.unwrap_or(false),
            exact_size: self.exact_size.unwrap_or(true),
            localtime: self.localtime.unwrap_or(false),
            format: self.format.unwrap_or_default(),
        })
    }
}

/// Parse `autoindex_format html|json|jsonp|xml`.
fn parse_autoindex_format(directive: &NginxDirective) -> Result<AutoindexFormat, NginxConfigError> {
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            "autoindex_format takes exactly one format",
        ));
    }
    match directive.args[0].as_str() {
        "html" => Ok(AutoindexFormat::Html),
        "json" => Ok(AutoindexFormat::Json),
        "jsonp" => Ok(AutoindexFormat::Jsonp),
        "xml" => Ok(AutoindexFormat::Xml),
        other => Err(NginxConfigError::unsupported(
            directive,
            format!("autoindex_format accepts html|json|jsonp|xml, found `{other}`"),
        )),
    }
}

/// nginx charset-family directives (`charset`, `charset_types`,
/// `source_charset`, `override_charset`) declared at one configuration level.
///
/// `charset`/`source_charset`/`override_charset` inherit from the previous
/// level when undeclared; `charset_types` replaces the inherited list when
/// declared. An undeclared field stays `None` through the http → server →
/// location merge chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CharsetDeclarations {
    charset: Option<CharsetValue>,
    types: Option<Vec<String>>,
    source_charset: Option<CharsetValue>,
    override_charset: Option<bool>,
}

/// `charset` / `source_charset` argument. `off` is a real declaration (it
/// disables the inherited value), distinct from "not declared".
#[derive(Debug, Clone, PartialEq, Eq)]
enum CharsetValue {
    Off,
    Name(String),
}

impl CharsetDeclarations {
    fn is_empty(&self) -> bool {
        self.charset.is_none()
            && self.types.is_none()
            && self.source_charset.is_none()
            && self.override_charset.is_none()
    }

    /// Overlay `inner` (the more specific level) on `self`.
    fn overlay(self, inner: Self) -> Self {
        Self {
            charset: inner.charset.or(self.charset),
            types: inner.types.or(self.types),
            source_charset: inner.source_charset.or(self.source_charset),
            override_charset: inner.override_charset.or(self.override_charset),
        }
    }

    /// Materialize the effective policy. `None` means no level declared the
    /// family, so the runtime performs no charset header rewrite.
    fn resolve(&self) -> Option<CharsetConfig> {
        if self.is_empty() {
            return None;
        }
        Some(CharsetConfig {
            charset: match &self.charset {
                Some(CharsetValue::Name(name)) => Some(name.clone()),
                Some(CharsetValue::Off) | None => None,
            },
            types: self.types.clone(),
            source_charset: match &self.source_charset {
                Some(CharsetValue::Name(name)) => Some(name.clone()),
                Some(CharsetValue::Off) | None => None,
            },
            override_charset: self.override_charset.unwrap_or(false),
        })
    }
}

/// Parse a `charset <name>|off` / `source_charset <name>` argument.
fn parse_charset_value(
    directive: &NginxDirective,
    allow_off: bool,
) -> Result<CharsetValue, NginxConfigError> {
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} takes exactly one charset name", directive.name),
        ));
    }
    let value = &directive.args[0];
    if allow_off && value.eq_ignore_ascii_case("off") {
        return Ok(CharsetValue::Off);
    }
    if value.starts_with('$') {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "{} variable `{value}` is not supported; declare a literal charset name",
                directive.name
            ),
        ));
    }
    if value.is_empty()
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'+'))
        })
    {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "{} charset name `{value}` is not a valid charset token",
                directive.name
            ),
        ));
    }
    Ok(CharsetValue::Name(value.clone()))
}

/// Parse `charset_types <mime…>`.
///
/// nginx seeds a declared list with the first built-in default type
/// (`text/html`) and then appends the arguments, so `charset_types text/css`
/// matches both `text/css` and `text/html` (differential-confirmed against
/// nginx 1.29.6). `*` makes the filter match every media type.
fn parse_charset_types(directive: &NginxDirective) -> Result<Vec<String>, NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "charset_types requires at least one MIME type",
        ));
    }
    if directive.args.iter().any(|arg| arg == "*") {
        return Ok(vec!["*".to_owned()]);
    }
    let mut types = vec!["text/html".to_owned()];
    for arg in &directive.args {
        validate_media_type(directive, arg)?;
        if !types
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(arg))
        {
            types.push(arg.clone());
        }
    }
    Ok(types)
}

/// nginx gzip refinement directives (`gzip_comp_level`, `gzip_disable`,
/// `gzip_proxied`) declared at one configuration level.
///
/// Each directive inherits independently: `gzip_comp_level` and the
/// `gzip_proxied` bitmask inherit the previous level's value when undeclared,
/// the `gzip_disable` regex array is replaced when declared, and the predefined
/// `msie6` selector is a separate flag that keeps inheriting unless a level
/// names it (nginx merges the flag and the array through two chains).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GzipRefinementDeclarations {
    disable: Option<Vec<String>>,
    disable_msie6: Option<bool>,
    proxied: Option<GzipProxiedConfig>,
}

impl GzipRefinementDeclarations {
    fn is_empty(&self) -> bool {
        self.disable.is_none() && self.disable_msie6.is_none() && self.proxied.is_none()
    }

    fn overlay(self, inner: Self) -> Self {
        Self {
            disable: inner.disable.or(self.disable),
            disable_msie6: inner.disable_msie6.or(self.disable_msie6),
            proxied: inner.proxied.or(self.proxied),
        }
    }

    fn resolve(&self) -> GzipRefinements {
        GzipRefinements {
            // nginx `gzip_comp_level` stays accepted-unenforced: the runtime's
            // tower-http compression layer owns its deflate level, so the
            // declared level has no observable effect on negotiation or bytes.
            comp_level: 1,
            disable: self.disable.clone().unwrap_or_default(),
            disable_msie6: self.disable_msie6.unwrap_or(false),
            proxied: self.proxied.clone(),
        }
    }
}

/// Effective values of the gzip refinement family for one level.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GzipRefinements {
    comp_level: u8,
    disable: Vec<String>,
    disable_msie6: bool,
    proxied: Option<GzipProxiedConfig>,
}

/// Attach the refinement fields to a `compression` JSON object.
fn set_gzip_refinement_fields(
    object: &mut Value,
    refinements: &GzipRefinements,
) -> Result<(), NginxConfigError> {
    object["compLevel"] = json!(refinements.comp_level);
    if !refinements.disable.is_empty() {
        object["disable"] = json!(refinements.disable);
    }
    if refinements.disable_msie6 {
        object["disableMsie6"] = json!(true);
    }
    if let Some(proxied) = &refinements.proxied {
        object["proxied"] = serde_json::to_value(proxied).map_err(|error| {
            NginxConfigError::ValidationFailed(format!("nginx materialization failed: {error}"))
        })?;
    }
    Ok(())
}

/// Parse `gzip_disable <regex…>` / `gzip_disable "msie6"`.
///
/// nginx compiles every non-`msie6` argument as a case-insensitive PCRE and
/// disables gzip when any pattern matches the request `User-Agent`. The
/// runtime compiles the same patterns with the bounded Rust regex engine; a
/// pattern that engine rejects fails closed here instead of silently changing
/// the negotiation.
fn parse_gzip_disable(
    directive: &NginxDirective,
) -> Result<(Option<Vec<String>>, Option<bool>), NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "gzip_disable requires at least one pattern",
        ));
    }
    let mut patterns = Vec::new();
    let mut msie6 = None;
    for argument in &directive.args {
        if argument == "msie6" {
            msie6 = Some(true);
            continue;
        }
        if argument.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "gzip_disable pattern must not be empty",
            ));
        }
        if regex::RegexBuilder::new(argument)
            .case_insensitive(true)
            .build()
            .is_err()
        {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "gzip_disable pattern `{argument}` is not supported by the runtime regex engine; use a portable pattern or remove it"
                ),
            ));
        }
        patterns.push(argument.clone());
    }
    Ok((Some(patterns), msie6))
}

/// Parse `gzip_proxied <off|expired|no-cache|no-store|private|no_last_modified|no_etag|auth|any…>`.
fn parse_gzip_proxied(directive: &NginxDirective) -> Result<GzipProxiedConfig, NginxConfigError> {
    if directive.args.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "gzip_proxied requires at least one condition",
        ));
    }
    let mut config = GzipProxiedConfig::default();
    for argument in &directive.args {
        match argument.as_str() {
            "off" => config.off = true,
            "any" => config.any = true,
            "expired" => config.expired = true,
            "no-cache" => config.no_cache = true,
            "no-store" => config.no_store = true,
            "private" => config.private = true,
            "no_last_modified" => config.no_last_modified = true,
            "no_etag" => config.no_etag = true,
            "auth" => config.auth = true,
            other => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!(
                        "gzip_proxied accepts off|expired|no-cache|no-store|private|no_last_modified|no_etag|auth|any, found `{other}`"
                    ),
                ));
            }
        }
    }
    Ok(config)
}

fn parse_gzip_static(directive: &NginxDirective) -> Result<GzipStaticMode, NginxConfigError> {
    match directive.args.first().map(String::as_str) {
        Some("on") => Ok(GzipStaticMode::On),
        Some("off") => Ok(GzipStaticMode::Off),
        Some("always") => Ok(GzipStaticMode::Always),
        _ => Err(NginxConfigError::unsupported(
            directive,
            "gzip_static requires on, off, or always",
        )),
    }
}

fn parse_on_off(directive: &NginxDirective, name: &str) -> Result<bool, NginxConfigError> {
    match directive.args.first().map(String::as_str) {
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        other => Err(NginxConfigError::unsupported(
            directive,
            format!("{name} requires on or off, found `{other:?}`"),
        )),
    }
}

/// nginx `limit_req_status` / `limit_conn_status`: one status code in
/// nginx's `ngx_http_limit_*_status_bounds` window 400-599 (default 503).
fn parse_limit_status(directive: &NginxDirective) -> Result<u16, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires one status code", directive.name),
        ));
    };
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} takes exactly one status code", directive.name),
        ));
    }
    let status = value.parse::<u16>().map_err(|_| {
        NginxConfigError::unsupported(
            directive,
            format!(
                "{} requires a 400-599 status code, found `{value}`",
                directive.name
            ),
        )
    })?;
    if !(400..=599).contains(&status) {
        return Err(NginxConfigError::unsupported(
            directive,
            format!(
                "{} requires a status code in 400-599 (nginx bound), found `{status}`",
                directive.name
            ),
        ));
    }
    Ok(status)
}

/// nginx `max_ranges <n>`: an unsigned range-count ceiling; `0` disables
/// byte-range processing entirely (`ngx_http_range_header_filter`).
fn parse_max_ranges(directive: &NginxDirective) -> Result<u64, NginxConfigError> {
    let Some(value) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "max_ranges requires a non-negative range count",
        ));
    };
    if directive.args.len() != 1 {
        return Err(NginxConfigError::unsupported(
            directive,
            "max_ranges takes exactly one range count",
        ));
    }
    value.parse::<u64>().map_err(|_| {
        NginxConfigError::unsupported(
            directive,
            format!("max_ranges requires a non-negative integer, found `{value}`"),
        )
    })
}

/// One nginx `error_page <codes...> [=[response]] uri;` mapping.
fn parse_error_page(directive: &NginxDirective) -> Result<Value, NginxConfigError> {
    let mut codes = Vec::new();
    let mut response_code: Option<u16> = None;
    // nginx `error_page <code> = <uri>;` (bare `=`, no status) uses the
    // internally redirected target's own status; `=code` replaces it.
    let mut from_target = false;
    let mut uri: Option<String> = None;
    for argument in &directive.args {
        if let Some(code) = argument.strip_prefix('=') {
            if response_code.is_some() || from_target {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "error_page accepts at most one `=` or `=code` response override",
                ));
            }
            if code.is_empty() {
                from_target = true;
                continue;
            }
            let Some(parsed) = code.parse::<u16>().ok() else {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "`=code` requires a response status (200-599)",
                ));
            };
            if !(200..=599).contains(&parsed) {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("error_page response `{parsed}` must be 200-599"),
                ));
            }
            response_code = Some(parsed);
        } else if argument.starts_with('/')
            || argument.starts_with('@')
            || argument.starts_with('$')
        {
            if uri.is_some() {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "error_page accepts exactly one target URI",
                ));
            }
            if argument.starts_with('$') {
                // nginx allows a variable error_page target
                // (`error_page 405 =200 $uri;`). The runtime resolves the
                // template with the request/rewrite variables before selecting
                // the target.
                let variables = crate::config::template_variables(argument)
                    .map_err(|message| NginxConfigError::unsupported(directive, message))?;
                for variable in variables {
                    if !rewrite_variable_is_known(&variable.name, &[]) {
                        return Err(NginxConfigError::unsupported(
                            directive,
                            format!(
                                "error_page target `{argument}` uses `${{{}}}`, which the runtime cannot resolve",
                                variable.name
                            ),
                        ));
                    }
                }
            }
            uri = Some(argument.clone());
        } else if let Ok(code) = argument.parse::<u16>() {
            if !(300..=599).contains(&code) {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("error_page code `{code}` must be 300-599"),
                ));
            }
            codes.push(code);
        } else {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "unsupported error_page argument `{argument}`; expected codes, an optional `=code`, and one `/path` or `@named` target"
                ),
            ));
        }
    }
    if codes.is_empty() {
        return Err(NginxConfigError::unsupported(
            directive,
            "error_page requires at least one status code",
        ));
    }
    let Some(uri) = uri else {
        return Err(NginxConfigError::unsupported(
            directive,
            "error_page requires a `/path` or `@named` target",
        ));
    };
    Ok(json!({
        "codes": codes,
        "uri": uri,
        "responseCode": response_code,
        "fromTarget": from_target,
    }))
}

/// nginx server-level `return`: every request in the host is answered by the
/// rewrite phase before location selection. Materialized as one catch-all
/// route plus its respond/redirect resource.
fn materialize_server_return(
    directive: &NginxDirective,
    server_index: usize,
    args: &[String],
    map_variables: &[&str],
) -> Result<(Value, Value), NginxConfigError> {
    let resource_id = format!("loc-{server_index}-return");
    let Some(first) = args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            "return requires a status or URL",
        ));
    };
    let resource = if let Ok(status) = first.parse::<u16>() {
        if status == 444 {
            return Err(NginxConfigError::unsupported(
                directive,
                "return 444 (close the connection without a response) is not supported by the runtime model",
            ));
        }
        match (status, args.get(1)) {
            (301 | 302 | 303 | 307 | 308, Some(url)) => {
                if url.contains('$') && !redirect_variables_ok_with_maps(url, map_variables) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!("return URL `{url}` uses unsupported variables; supported: $host $request_uri $scheme or a declared map result"),
                    ));
                }
                json!({
                    "id": resource_id,
                    "type": "redirect",
                    "status": status,
                    "location": url,
                })
            }
            (200..=599, None) => json!({
                "id": resource_id,
                "type": "respond",
                "status": status,
                "contentType": "text/plain; charset=utf-8",
                "contentTypeFromUri": true,
                "body": "",
            }),
            (200..=599, Some(body)) => {
                if body.contains('$') && !return_body_variables_ok(body, map_variables) {
                    return Err(NginxConfigError::unsupported(
                        directive,
                        format!(
                            "return body `{body}` uses variables the runtime does not expand; supported: the declared map results (write `$$` for a literal dollar sign)"
                        ),
                    ));
                }
                json!({
                    "id": resource_id,
                    "type": "respond",
                    "status": status,
                    "contentType": "text/plain; charset=utf-8",
                    "contentTypeFromUri": true,
                    "body": body,
                })
            }
            _ => {
                return Err(NginxConfigError::unsupported(
                    directive,
                    "server-level `return` supports 301/302/303/307/308 with a URL or 200-599 with a body",
                ));
            }
        }
    } else {
        if args.len() != 1 {
            return Err(NginxConfigError::unsupported(
                directive,
                "the `return URL` form takes exactly one argument",
            ));
        }
        if first.contains('$') && !redirect_variables_ok_with_maps(first, map_variables) {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("return URL `{first}` uses unsupported variables; supported: $host $request_uri $scheme or a declared map result"),
            ));
        }
        json!({
            "id": resource_id,
            "type": "redirect",
            "status": 302,
            "location": first,
        })
    };
    let route = json!({
        "id": format!("route-{server_index}-return"),
        "match": {"pathType": "prefix", "path": "/"},
        "resourceRef": resource_id,
    });
    Ok((route, resource))
}

fn parse_access_rule(directive: &NginxDirective) -> Result<Value, NginxConfigError> {
    let Some(network) = directive.args.first() else {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("{} requires a network (CIDR, IP, or all)", directive.name),
        ));
    };
    let action = match directive.name.as_str() {
        "allow" => "allow",
        "deny" => "deny",
        other => {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("unsupported access directive `{other}`"),
            ))
        }
    };
    let trimmed = network.trim();
    if !(trimmed.eq_ignore_ascii_case("all")
        || trimmed.parse::<IpAddr>().is_ok()
        || trimmed.parse::<ipnet::IpNet>().is_ok())
    {
        return Err(NginxConfigError::unsupported(
            directive,
            format!("invalid access network `{network}`"),
        ));
    }
    Ok(json!({
        "action": action,
        "network": trimmed,
    }))
}

/// Parse nginx time suffixes (`10s`, `5m`, `1h`, `1d`) into whole seconds.
fn parse_nginx_time_seconds(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (number, multiplier) = match trimmed.as_bytes().last().copied() {
        Some(b's' | b'S') => (&trimmed[..trimmed.len() - 1], 1_u64),
        Some(b'm' | b'M') => (&trimmed[..trimmed.len() - 1], 60),
        Some(b'h' | b'H') => (&trimmed[..trimmed.len() - 1], 3_600),
        Some(b'd' | b'D') => (&trimmed[..trimmed.len() - 1], 86_400),
        Some(b'0'..=b'9') => (trimmed, 1_u64),
        _ => return None,
    };
    let amount = number.trim().parse::<u64>().ok()?;
    amount.checked_mul(multiplier)
}

fn sanitize_id(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' {
            output.push(ch);
        } else {
            output.push('-');
        }
    }
    if output.is_empty() || output.chars().all(|ch| ch == '-') {
        "any".to_owned()
    } else {
        output
    }
}

fn certificate_name_counter() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ResourceConfig, RewritePhaseOpConfig};
    use crate::nginx::parser::parse_nginx_config;

    fn materialize(text: &str) -> Result<WebServerAppConfig, NginxConfigError> {
        let parsed = parse_nginx_config(text, std::path::Path::new("site.conf")).expect("parse");
        materialize_nginx_app_default(
            &parsed,
            std::path::Path::new("/etc/nginx/sites-enabled"),
            "test",
        )
    }

    #[test]
    fn materializes_redirect_policy_at_every_level() {
        let config = materialize(
            r#"
absolute_redirect off;
port_in_redirect off;

server {
    listen 8080;
    server_name redirect.example.test;
    server_name_in_redirect on;

    location /host/ {
        root /srv/www;
    }

    location /route/ {
        absolute_redirect on;
        port_in_redirect on;
        server_name_in_redirect off;
        root /srv/www;
    }
}
"#,
        )
        .expect("materialize");
        let host = &config.virtual_hosts[0];
        let host_policy = host.redirect_policy.expect("host policy");
        // http-level declarations are inherited, server-level overlays them.
        assert!(!host_policy.absolute_redirect);
        assert!(!host_policy.port_in_redirect);
        assert!(host_policy.server_name_in_redirect);
        let host_route = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/host/")
            .expect("host route");
        assert!(host_route.redirect_policy.is_none(), "inherits the host");
        let route = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/route/")
            .expect("route");
        let route_policy = route.redirect_policy.expect("route policy");
        assert!(route_policy.absolute_redirect);
        assert!(route_policy.port_in_redirect);
        assert!(!route_policy.server_name_in_redirect);
        // The directive is executed, not accepted-unenforced.
        assert!(
            ignored_directive_diagnostics(
                &parse_nginx_config(
                    "absolute_redirect off;\nserver { listen 8080; server_name a.test; location / { root /srv; } }",
                    std::path::Path::new("site.conf"),
                )
                .expect("parse")
            )
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("absolute_redirect")),
            "absolute_redirect must not be reported accepted-unenforced"
        );
    }

    #[test]
    fn materializes_limit_statuses_and_max_ranges_with_inheritance() {
        let config = materialize(
            r#"
limit_req_status 429;
limit_conn_status 429;
max_ranges 2;

server {
    listen 8080;
    server_name limits.example.test;

    location /host/ {
        root /srv/www;
    }

    location /route/ {
        limit_req_status 400;
        limit_conn_status 599;
        max_ranges 0;
        root /srv/www;
    }
}
"#,
        )
        .expect("materialize");
        let host = &config.virtual_hosts[0];
        assert_eq!(host.limit_req_status, Some(429));
        assert_eq!(host.limit_conn_status, Some(429));
        assert_eq!(host.max_ranges, Some(2));
        let host_route = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/host/")
            .expect("host route");
        assert!(host_route.limit_req_status.is_none());
        assert!(host_route.max_ranges.is_none());
        let route = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/route/")
            .expect("route");
        assert_eq!(route.limit_req_status, Some(400));
        assert_eq!(route.limit_conn_status, Some(599));
        assert_eq!(route.max_ranges, Some(0));
    }

    #[test]
    fn limit_status_and_max_ranges_forms_fail_closed() {
        for text in [
            "limit_req_status 200;\nserver { listen 8080; server_name a.test; location / { root /srv; } }",
            "limit_conn_status abc;\nserver { listen 8080; server_name a.test; location / { root /srv; } }",
            "max_ranges -1;\nserver { listen 8080; server_name a.test; location / { root /srv; } }",
        ] {
            let error = materialize(text).expect_err("invalid form must fail closed");
            assert!(
                error.to_string().contains("limit_") || error.to_string().contains("max_ranges"),
                "unexpected diagnostic: {error}"
            );
        }
    }

    #[test]
    fn materializes_proxy_server_with_upstream_and_ssl() {
        let config = materialize(
            r#"
upstream backend {
    server 127.0.0.1:8080 weight=2 max_fails=3 fail_timeout=10s;
    keepalive 16;
}

server {
    listen 80;
    listen [::]:80;
    server_name example.com www.example.com;

    location /api/ {
        proxy_pass http://backend;
        proxy_set_header Host $host;
        proxy_http_version 1.1;
        proxy_buffering off;
    }
    location = /healthz {
        return 200 "ok";
    }
}
"#,
        )
        .expect("materialize");
        assert_eq!(config.virtual_hosts.len(), 1);
        assert_eq!(config.upstreams.len(), 1);
        let upstream = &config.upstreams[0];
        assert_eq!(upstream.id, "backend");
        assert_eq!(upstream.targets[0].url, "http://127.0.0.1:8080");
        assert_eq!(upstream.targets[0].weight, 2);
        assert_eq!(config.resources.len(), 2);
        let kinds = config
            .resources
            .iter()
            .map(|resource| match resource {
                crate::config::ResourceConfig::Proxy { .. } => "proxy",
                crate::config::ResourceConfig::Respond { .. } => "respond",
                other => panic!("unexpected {other:?}"),
            })
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"proxy"));
        assert!(kinds.contains(&"respond"));
        let proxy = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Proxy {
                    request_set_headers,
                    ..
                } => Some(request_set_headers.clone()),
                _ => None,
            })
            .expect("proxy resource");
        assert_eq!(proxy, vec!["Host $host".to_owned()]);
        let host = &config.virtual_hosts[0];
        assert_eq!(host.server_names, vec!["example.com", "www.example.com"]);
        assert_eq!(host.routes.len(), 2);
        assert!(host.routes.iter().any(|route| route.route_match.path_type
            == crate::config::RoutePathType::Exact
            && route.route_match.path == "/healthz"));
    }

    #[test]
    #[allow(clippy::useless_format)]
    fn materializes_ssl_listener_with_certificates() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("site.pem"), "cert").unwrap();
        std::fs::write(directory.path().join("site.key"), "key").unwrap();
        let parsed = parse_nginx_config(
            &format!(
                r#"
server {{
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name secure.example.com;

    ssl_certificate site.pem;
    ssl_certificate_key site.key;
    ssl_protocols TLSv1.2 TLSv1.3;

    location / {{
        proxy_pass http://127.0.0.1:9443;
    }}
}}
"#
            ),
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize");
        // `listen 443 ssl http2` + `listen [::]:443 ssl http2` materialize as
        // two listeners (one per address family) sharing the TLS policy.
        assert_eq!(config.listeners.len(), 2);
        let listener = config
            .listeners
            .iter()
            .find(|listener| listener.bind == "0.0.0.0")
            .expect("ipv4 listener");
        assert_eq!(listener.port, 443);
        assert!(listener.tls_policy_ref.is_some());
        assert_eq!(config.certificates.len(), 1);
        assert_eq!(config.tls_policies.len(), 1);
        let certificate = &config.certificates[0];
        let crate::config::CertificateSource::ProtectedFile {
            certificate_file,
            private_key_file,
            ..
        } = &certificate.source;
        assert!(certificate_file.ends_with("site.pem"));
        assert!(private_key_file.ends_with("site.key"));
        // Relative certificate paths resolve against the config directory.
        assert!(std::path::Path::new(certificate_file).is_absolute());
    }

    #[test]
    fn ssl_protocols_declaration_selects_the_tls_window() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("site.pem"), "cert").unwrap();
        std::fs::write(directory.path().join("site.key"), "key").unwrap();
        let materialize = |protocols: &str| {
            let parsed = parse_nginx_config(
                &format!(
                    r#"
server {{
    listen 443 ssl;
    server_name secure.example.com;

    ssl_certificate site.pem;
    ssl_certificate_key site.key;
    ssl_protocols {protocols};

    location / {{
        proxy_pass http://127.0.0.1:9443;
    }}
}}
"#
                ),
                std::path::Path::new("site.conf"),
            )
            .expect("parse");
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize")
        };
        // TLS 1.3 only is a genuine hardening declaration; it must reach the
        // TLS policy instead of being dropped and silently re-opening 1.2.
        let tls13 = materialize("TLSv1.3");
        assert_eq!(tls13.tls_policies.len(), 1);
        assert_eq!(tls13.tls_policies[0].minimum_version, TlsVersion::Tls13);
        assert_eq!(tls13.tls_policies[0].maximum_version, TlsVersion::Tls13);
        // TLS 1.2 only is the mirror case.
        let tls12 = materialize("TLSv1.2");
        assert_eq!(tls12.tls_policies[0].minimum_version, TlsVersion::Tls12);
        assert_eq!(tls12.tls_policies[0].maximum_version, TlsVersion::Tls12);
        // The legacy tokens nginx accepts are intersected with the rustls
        // window (never widening it, never failing the load).
        let mixed = materialize("TLSv1 TLSv1.1 TLSv1.2 TLSv1.3");
        assert_eq!(mixed.tls_policies[0].minimum_version, TlsVersion::Tls12);
        assert_eq!(mixed.tls_policies[0].maximum_version, TlsVersion::Tls13);
        // An undeclared `ssl_protocols` keeps the documented runtime default.
        assert_eq!(
            materialize("TLSv1.2 TLSv1.3").tls_policies[0].minimum_version,
            TlsVersion::Tls12
        );
    }

    #[test]
    fn ssl_protocols_inherits_http_level_and_server_overrides() {
        let directory = tempfile::tempdir().expect("temp dir");
        for name in ["a", "b"] {
            std::fs::write(directory.path().join(format!("{name}.pem")), "cert").unwrap();
            std::fs::write(directory.path().join(format!("{name}.key")), "key").unwrap();
        }
        let parsed = parse_nginx_config(
            r#"
http {
    ssl_protocols TLSv1.3;

    server {
        listen 443 ssl;
        server_name inherited.example.com;
        ssl_certificate a.pem;
        ssl_certificate_key a.key;
        location / { proxy_pass http://127.0.0.1:9443; }
    }

    server {
        listen 8443 ssl;
        server_name override.example.com;
        ssl_certificate b.pem;
        ssl_certificate_key b.key;
        ssl_protocols TLSv1.2;
        location / { proxy_pass http://127.0.0.1:9443; }
    }
}
"#,
            std::path::Path::new("nginx.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize");
        assert_eq!(config.tls_policies.len(), 2);
        let mut windows: Vec<(TlsVersion, TlsVersion)> = config
            .tls_policies
            .iter()
            .map(|policy| (policy.minimum_version, policy.maximum_version))
            .collect();
        windows.sort();
        assert_eq!(
            windows,
            vec![
                (TlsVersion::Tls12, TlsVersion::Tls12),
                (TlsVersion::Tls13, TlsVersion::Tls13),
            ],
            "the http-level window is inherited while an explicit server-level declaration replaces it"
        );
    }

    #[test]
    fn ssl_protocols_that_keeps_no_servable_protocol_is_refused() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("site.pem"), "cert").unwrap();
        std::fs::write(directory.path().join("site.key"), "key").unwrap();
        let parsed = parse_nginx_config(
            r#"
server {
    listen 443 ssl;
    server_name legacy.example.com;
    ssl_certificate site.pem;
    ssl_certificate_key site.key;
    ssl_protocols TLSv1 TLSv1.1;
    location / { proxy_pass http://127.0.0.1:9443; }
}
"#,
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let error = materialize_nginx_app_default(&parsed, directory.path(), "test")
            .expect_err("TLS 1.0/1.1 only must be refused, not silently defaulted");
        assert!(
            format!("{error}").contains("TLS 1.0/1.1"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn ssl_protocols_rejects_an_unknown_token() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("site.pem"), "cert").unwrap();
        std::fs::write(directory.path().join("site.key"), "key").unwrap();
        let parsed = parse_nginx_config(
            r#"
server {
    listen 443 ssl;
    server_name future.example.com;
    ssl_certificate site.pem;
    ssl_certificate_key site.key;
    ssl_protocols TLSv1.4;
    location / { proxy_pass http://127.0.0.1:9443; }
}
"#,
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let error = materialize_nginx_app_default(&parsed, directory.path(), "test")
            .expect_err("an unknown protocol token must be refused");
        assert!(
            format!("{error}").contains("TLSv1.4"),
            "unexpected error: {error}"
        );
    }

    /// The materialized JSON is the contract with the data plane, so the
    /// hand-built `cache_policy_json` must stay parseable as the model type it
    /// stands for — otherwise a mismatched key would only surface as a 500 at
    /// request time.
    #[test]
    fn cache_policy_json_matches_the_model_wire_form() {
        let directory = tempfile::tempdir().expect("temp dir");
        let parsed = parse_nginx_config(
            "server {\n    listen 80;\n    server_name cache.example.com;\n    expires modified 2h;\n    etag off;\n    if_modified_since before;\n    location / { return 200 \"ok\"; }\n}\n",
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "cache").expect("materialize");
        let policy = config.virtual_hosts[0]
            .cache_policy
            .as_ref()
            .expect("host cache policy");
        let json = serde_json::to_value(policy).expect("serialize");
        assert_eq!(json["expires"], "modified");
        assert_eq!(json["expiresSeconds"], 7_200);
        assert_eq!(json["etag"], false);
        assert_eq!(json["ifModifiedSince"], "before");
        // Round-trips through the model's own serde names, which is what the
        // data plane deserializes.
        let reparsed: CachePolicyConfig =
            serde_json::from_value(json.clone()).expect("deserialize");
        assert_eq!(&reparsed, policy);
    }

    #[test]
    fn expires_declaration_selects_the_mode_and_reaches_the_route() {
        let directory = tempfile::tempdir().expect("temp dir");
        let parsed = parse_nginx_config(
            "server {\n    listen 80;\n    server_name cache.example.com;\n    location /assets/ { expires 7d; root /srv/www; }\n    location /api/ { expires epoch; root /srv/www; }\n    location /max/ { expires max; root /srv/www; }\n    location /daily/ { expires @15h30m; root /srv/www; }\n    location /never/ { expires -1h; root /srv/www; }\n    location /plain/ { root /srv/www; }\n}\n",
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "cache").expect("materialize");
        let routes = &config.virtual_hosts[0].routes;
        let policy = |index: usize| {
            routes[index]
                .cache_policy
                .as_ref()
                .unwrap_or_else(|| panic!("route {index} must carry a cache policy"))
        };
        assert_eq!(policy(0).expires, ExpiresMode::Access);
        assert_eq!(policy(0).expires_seconds, 604_800);
        assert_eq!(policy(1).expires, ExpiresMode::Epoch);
        assert_eq!(policy(2).expires, ExpiresMode::Max);
        assert_eq!(policy(3).expires, ExpiresMode::Daily);
        assert_eq!(policy(3).expires_seconds, 55_800);
        assert_eq!(policy(4).expires, ExpiresMode::Access);
        assert_eq!(policy(4).expires_seconds, -3_600);
        // A location that declares nothing keeps no policy of its own, so it
        // inherits the host/app default rather than pinning `expires off`.
        assert!(
            routes[5].cache_policy.is_none(),
            "an undeclared location must not emit a policy"
        );
    }

    #[test]
    fn cache_directives_inherit_independently_http_server_location() {
        let directory = tempfile::tempdir().expect("temp dir");
        let parsed = parse_nginx_config(
            r#"
http {
    expires 1h;
    etag off;
    server {
        listen 80;
        server_name cache.example.com;
        if_modified_since before;
        location /override/ { expires modified 30m; root /srv/www; }
        location /inherit/ { root /srv/www; }
        location /etaon/ { etag on; root /srv/www; }
    }
}
"#,
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "cache").expect("materialize");
        let host = config.virtual_hosts[0]
            .cache_policy
            .as_ref()
            .expect("http-level declarations must reach the host");
        assert_eq!(host.expires, ExpiresMode::Access);
        assert_eq!(host.expires_seconds, 3_600);
        assert!(!host.etag);
        assert_eq!(host.if_modified_since, IfModifiedSinceMode::Before);

        let routes = &config.virtual_hosts[0].routes;
        // Location replaces `expires` only; the http `etag off` and the server
        // `if_modified_since before` survive.
        let override_policy = routes[0].cache_policy.as_ref().expect("override policy");
        assert_eq!(override_policy.expires, ExpiresMode::Modified);
        assert_eq!(override_policy.expires_seconds, 1_800);
        assert!(!override_policy.etag);
        assert_eq!(
            override_policy.if_modified_since,
            IfModifiedSinceMode::Before
        );
        // A location with no declaration emits nothing and inherits the host.
        assert!(routes[1].cache_policy.is_none());
        // `etag on` at the location re-enables what the http context disabled.
        let etag_on = routes[2].cache_policy.as_ref().expect("etag policy");
        assert!(etag_on.etag);
        assert_eq!(etag_on.expires, ExpiresMode::Access);
        assert_eq!(etag_on.expires_seconds, 3_600);
    }

    #[test]
    fn expires_off_at_the_location_reverts_an_inherited_policy() {
        let directory = tempfile::tempdir().expect("temp dir");
        let parsed = parse_nginx_config(
            "server {\n    listen 80;\n    server_name cache.example.com;\n    expires 1d;\n    location /fresh/ { root /srv/www; }\n    location /volatile/ { expires off; root /srv/www; }\n}\n",
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "cache").expect("materialize");
        let host = config.virtual_hosts[0]
            .cache_policy
            .as_ref()
            .expect("server-level policy");
        assert_eq!(host.expires, ExpiresMode::Access);
        assert_eq!(host.expires_seconds, 86_400);
        let routes = &config.virtual_hosts[0].routes;
        assert!(
            routes[0].cache_policy.is_none(),
            "a silent location inherits the host policy instead of repeating it"
        );
        let explicit_off = routes[1]
            .cache_policy
            .as_ref()
            .expect("`expires off` is an explicit declaration");
        assert_eq!(
            explicit_off.expires,
            ExpiresMode::Off,
            "`expires off` must be distinguishable from an omission, otherwise \
             it could not turn off an inherited policy"
        );
    }

    #[test]
    fn malformed_cache_directives_fail_closed() {
        let directory = tempfile::tempdir().expect("temp dir");
        let cases = [
            ("expires 1x;", "invalid expires value"),
            ("expires modified;", "invalid value"),
            ("expires modified epoch;", "invalid expires modified value"),
            ("expires @25h;", "less than 24 hours"),
            ("expires @-1h;", "invalid value"),
            ("expires beginning 1h;", "expires accepts"),
            ("etag maybe;", "etag requires on or off"),
            ("if_modified_since sometimes;", "if_modified_since accepts"),
        ];
        for (directive, expected) in cases {
            let text = format!(
                "server {{\n    listen 80;\n    server_name cache.example.com;\n    {directive}\n    location / {{ return 200 \"ok\"; }}\n}}\n"
            );
            let parsed = parse_nginx_config(&text, std::path::Path::new("site.conf"))
                .unwrap_or_else(|error| panic!("`{directive}` must parse: {error}"));
            let error = materialize_nginx_app_default(&parsed, directory.path(), "cache")
                .err()
                .unwrap_or_else(|| panic!("`{directive}` must be refused"));
            let message = format!("{error}");
            assert!(
                message.contains(expected),
                "`{directive}`: expected `{expected}` in `{message}`"
            );
        }
    }

    #[test]
    fn materializes_redirect_with_host_variables_and_static_root() {
        let directory = tempfile::tempdir().expect("temp dir");
        let parsed = parse_nginx_config(
            &format!(
                r#"
server {{
    listen 80;
    server_name redirect.example.com;

    location / {{
        return 301 https://$host$request_uri;
    }}
}}

server {{
    listen 80;
    server_name static.example.com;

    location / {{
        root {};
        try_files $uri $uri/ /index.html;
    }}
}}
"#,
                "/srv/sdkwork/static"
            ),
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize");
        assert_eq!(config.virtual_hosts.len(), 2);
        let redirect = config
            .resources
            .iter()
            .find(|resource| matches!(resource, crate::config::ResourceConfig::Redirect { .. }))
            .expect("redirect resource");
        let crate::config::ResourceConfig::Redirect {
            status, location, ..
        } = redirect
        else {
            unreachable!()
        };
        assert_eq!(*status, 301);
        assert_eq!(location, "https://$host$request_uri");
        let static_resource = config
            .resources
            .iter()
            .find(|resource| matches!(resource, crate::config::ResourceConfig::Static { .. }))
            .expect("static resource");
        let crate::config::ResourceConfig::Static {
            root, spa_fallback, ..
        } = static_resource
        else {
            unreachable!()
        };
        assert_eq!(root, "/srv/sdkwork/static");
        assert_eq!(spa_fallback.as_deref(), Some("index.html"));
    }

    #[test]
    fn rewrite_and_regex_locations_materialize_into_routes() {
        let parsed = parse_nginx_config(
            r#"
server {
    listen 80;
    server_name ws.example.com;

    location ^~ /im/ws {
        rewrite ^/im/ws/?(.*)$ /$1 break;
        proxy_pass http://127.0.0.1:15200;
    }

    location ~ ^/(app|api)(/|$) {
        proxy_pass http://127.0.0.1:8080;
    }

    location ~* \.(png|jpg)$ {
        proxy_pass http://127.0.0.1:8081;
    }
}
"#,
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config = materialize_nginx_app_default(
            &parsed,
            std::path::Path::new("/etc/nginx/sites-enabled"),
            "test",
        )
        .expect("materialize");
        let host = &config.virtual_hosts[0];
        assert_eq!(host.routes.len(), 3);
        let exclusive = host
            .routes
            .iter()
            .find(|route| {
                route.route_match.path_type == crate::config::RoutePathType::PrefixExclusive
            })
            .expect("^~ route");
        assert_eq!(exclusive.route_match.path, "/im/ws");
        assert_eq!(exclusive.rewrite.len(), 1);
        assert_eq!(exclusive.rewrite[0].pattern, "^/im/ws/?(.*)$");
        assert_eq!(exclusive.rewrite[0].replacement, "/$1");
        assert!(host
            .routes
            .iter()
            .any(|route| route.route_match.path_type == crate::config::RoutePathType::Regex));
        assert!(host.routes.iter().any(|route| {
            route.route_match.path_type == crate::config::RoutePathType::RegexIgnoreCase
        }));
    }

    #[test]
    fn server_level_root_and_try_files_are_inherited_by_locations() {
        let parsed = parse_nginx_config(
            r#"
server {
    listen 80;
    server_name static.example.com;

    root /srv/web/static;
    try_files $uri $uri/ /index.html;

    location / {
        # no own root: inherits the server defaults
    }
}
"#,
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config = materialize_nginx_app_default(
            &parsed,
            std::path::Path::new("/etc/nginx/sites-enabled"),
            "test",
        )
        .expect("materialize");
        let static_resource = config
            .resources
            .iter()
            .find(|resource| matches!(resource, crate::config::ResourceConfig::Static { .. }))
            .expect("static resource from inherited root");
        let crate::config::ResourceConfig::Static {
            root, spa_fallback, ..
        } = static_resource
        else {
            unreachable!()
        };
        assert_eq!(root, "/srv/web/static");
        assert_eq!(spa_fallback.as_deref(), Some("index.html"));
    }

    #[test]
    fn unsupported_rewrite_flags_fail_closed() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name rewrite.example.com;
    location / {
        rewrite ^/old(.*)$ /new$1 if_not_supported;
        proxy_pass http://127.0.0.1:8080;
    }
}
"#,
        )
        .expect_err("unknown rewrite flag must be rejected");
        assert!(error.to_string().contains("rewrite flag"), "{error}");
    }

    #[test]
    fn add_header_declarations_are_validated_and_ordered() {
        // Supported variables, `always`, duplicate names, and declaration order
        // survive materialization on both the host and location levels.
        let config = materialize(
            r#"
add_header X-Http-Level http;
server {
    listen 80;
    server_name headers.example.com;
    location / {
        add_header X-Dup one;
        add_header X-Dup two always;
        add_header X-Var "$host|$request_uri";
        return 200 "ok";
    }
}
"#,
        )
        .expect("materialize");
        let headers = &config.virtual_hosts[0]
            .security_headers
            .as_ref()
            .expect("http-level headers inherited")
            .custom_headers;
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "X-Http-Level");
        assert_eq!(headers[0].value, "http");
        assert!(!headers[0].always);
        let route_headers = &config.virtual_hosts[0].routes[0].response_headers;
        assert_eq!(
            route_headers
                .iter()
                .map(|header| (header.name.as_str(), header.value.as_str(), header.always))
                .collect::<Vec<_>>(),
            vec![
                ("X-Dup", "one", false),
                ("X-Dup", "two", true),
                ("X-Var", "$host|$request_uri", false),
            ]
        );
        // The location declares its own array, so the inherited host array is
        // replaced for that route (nginx `add_header` inheritance).
        assert_eq!(
            route_headers.first().map(|header| header.name.as_str()),
            Some("X-Dup")
        );
    }

    #[test]
    fn add_header_rejects_unknown_variables_and_bad_arity() {
        for (text, expected) in [
            (
                r#"server {
    listen 80;
    server_name bad.example.com;
    add_header X-Bad "$unsupported_variable";
    location / { return 200 "ok"; }
}
"#,
                "$unsupported_variable",
            ),
            (
                r#"server {
    listen 80;
    server_name bad.example.com;
    add_header X-Bad value sometimes;
    location / { return 200 "ok"; }
}
"#,
                "Name value [always]",
            ),
            (
                r#"server {
    listen 80;
    server_name bad.example.com;
    add_header "Bad Name" value;
    location / { return 200 "ok"; }
}
"#,
                "not a valid HTTP field name",
            ),
            (
                r#"server {
    listen 80;
    server_name bad.example.com;
    add_header X-Bad "café";
    location / { return 200 "ok"; }
}
"#,
                "printable ASCII",
            ),
        ] {
            let error = materialize(text).expect_err("must fail closed");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn materializes_gzip_limit_req_cache_and_stream() {
        let directory = tempfile::tempdir().expect("temp");
        let htpasswd = directory.path().join("htpasswd");
        // username `alice`, password `secret` as {SHA} hash
        std::fs::write(&htpasswd, "alice:{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ=\n").unwrap();
        let cache_root = directory.path().join("cache");
        std::fs::create_dir_all(&cache_root).unwrap();
        let text = format!(
            r#"
http {{
    gzip on;
    gzip_types text/plain application/json;
    gzip_min_length 1024;
    limit_req_zone $binary_remote_addr zone=one:10m rate=10r/s;
    proxy_cache_path {} levels=1:2 keys_zone=cache:10m inactive=5m;

    upstream api {{
        least_conn;
        server 127.0.0.1:9001;
        server 127.0.0.1:9002;
    }}

    server {{
        listen 80;
        server_name cached.example.com;
        location / {{
            limit_req zone=one burst=20 nodelay;
            auth_basic "Restricted";
            auth_basic_user_file {};
            proxy_pass http://api;
            proxy_cache cache;
            proxy_cache_valid 200 5m;
        }}
        location /admin {{
            allow 10.0.0.0/8;
            deny all;
            proxy_pass http://127.0.0.1:9003;
        }}
    }}
}}

stream {{
    upstream tcp_backend {{
        server 127.0.0.1:19000;
    }}
    server {{
        listen 12345;
        proxy_pass tcp_backend;
        proxy_timeout 30s;
        proxy_protocol on;
    }}
    server {{
        listen 12346;
        ssl_preread on;
        proxy_pass 127.0.0.1:19001;
    }}
}}
"#,
            cache_root.display().to_string().replace('\\', "/"),
            htpasswd.display().to_string().replace('\\', "/"),
        );
        let parsed = parse_nginx_config(&text, std::path::Path::new("nginx.conf")).expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize");
        assert!(config.gzip.enabled);
        assert!(config.gzip.types.contains(&"application/json".to_owned()));
        assert_eq!(config.gzip.min_length, 1024);
        assert_eq!(config.limit_req_zones.len(), 1);
        assert_eq!(config.limit_req_zones[0].name, "one");
        assert!(config.proxy_cache.enabled);
        assert!(config.proxy_cache.disk_path.is_some());
        assert_eq!(config.proxy_cache.default_ttl_seconds, 300);
        let api = config
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "api")
            .expect("api upstream");
        assert_eq!(
            api.load_balancing,
            crate::config::UpstreamLoadBalancingStrategy::LeastConnections
        );
        let host = &config.virtual_hosts[0];
        assert!(host.routes[0].auth_basic.is_some());
        assert_eq!(host.routes[0].limit_req.len(), 1);
        assert_eq!(host.routes[1].access.len(), 2);
        assert_eq!(config.streams.len(), 2);
        assert!(config.streams[0].proxy_protocol);
        assert_eq!(
            config.streams[1].tls,
            Some(crate::config::StreamTlsMode::Preread)
        );
    }

    #[test]
    fn materializes_ip_hash_and_consistent_hash() {
        let config = materialize(
            r#"
upstream sticky {
    ip_hash;
    server 127.0.0.1:8001;
    server 127.0.0.1:8002;
}

upstream by_uri {
    hash $request_uri consistent;
    server 127.0.0.1:8003;
}

server {
    listen 80;
    server_name lb.example.com;
    location /a/ { proxy_pass http://sticky; }
    location /b/ { proxy_pass http://by_uri; }
}
"#,
        )
        .expect("materialize");
        let sticky = config
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "sticky")
            .expect("sticky");
        assert_eq!(
            sticky.load_balancing,
            crate::config::UpstreamLoadBalancingStrategy::IpHash
        );
        let by_uri = config
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "by_uri")
            .expect("by_uri");
        assert_eq!(
            by_uri.load_balancing,
            crate::config::UpstreamLoadBalancingStrategy::Hash
        );
        let hash = by_uri.hash.as_ref().expect("hash config");
        assert!(hash.consistent);
        assert_eq!(hash.key, crate::config::UpstreamHashKeyVar::RequestUri);
    }

    #[test]
    fn listen_defaults_to_port_80_and_first_server_becomes_default() {
        let config = materialize(
            r#"
server {
    listen 127.0.0.1;
    server_name first.example.com;
    location / { return 200 "first"; }
}

server {
    listen 127.0.0.1;
    server_name second.example.com;
    location / { return 200 "second"; }
}
"#,
        )
        .expect("materialize");
        let listener = config
            .listeners
            .iter()
            .find(|listener| listener.port == 80)
            .expect("port-80 listener from bare address");
        assert_eq!(listener.bind, "127.0.0.1");
        let default_ref = listener
            .default_virtual_host_ref
            .as_deref()
            .expect("first server must be the default");
        assert_eq!(default_ref, "first-example-com-80");
    }

    #[test]
    fn explicit_default_server_overrides_and_duplicate_fails() {
        let config = materialize(
            r#"
server {
    listen 8080;
    server_name a.example.com;
    location / { return 200 "a"; }
}

server {
    listen 8080 default_server;
    server_name b.example.com;
    location / { return 200 "b"; }
}
"#,
        )
        .expect("materialize");
        let listener = config
            .listeners
            .iter()
            .find(|listener| listener.port == 8080)
            .expect("listener");
        assert_eq!(
            listener.default_virtual_host_ref.as_deref(),
            Some("b-example-com-8080"),
            "explicit default_server must win over the first server"
        );
        let error = materialize(
            r#"
server {
    listen 8080 default_server;
    server_name a.example.com;
    location / { return 200 "a"; }
}

server {
    listen 8080 default_server;
    server_name b.example.com;
    location / { return 200 "b"; }
}
"#,
        )
        .expect_err("duplicate default_server must be rejected");
        assert!(
            error.to_string().contains("duplicate `default_server`"),
            "{error}"
        );
    }

    #[test]
    #[allow(clippy::useless_format)]
    fn http2_on_directive_enables_h2_protocols() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("site.pem"), "cert").unwrap();
        std::fs::write(directory.path().join("site.key"), "key").unwrap();
        let parsed = parse_nginx_config(
            &format!(
                r#"
server {{
    listen 443 ssl;
    http2 on;
    server_name h2.example.com;
    ssl_certificate site.pem;
    ssl_certificate_key site.key;
    location / {{ return 200 "ok"; }}
}}
"#
            ),
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize");
        let listener = config
            .listeners
            .iter()
            .find(|listener| listener.port == 443)
            .expect("443 listener");
        assert!(
            listener
                .protocols
                .contains(&crate::config::ListenerProtocol::Http2),
            "http2 on; must enable h2: {:?}",
            listener.protocols
        );
    }

    #[test]
    fn proxy_pass_uri_parts_materialize_strip_and_target_uri() {
        let config = materialize(
            r#"
upstream api {
    server 127.0.0.1:9001;
}

server {
    listen 80;
    server_name uri.example.com;
    location /api/ {
        proxy_pass http://api/;
    }
    location /v1/ {
        proxy_pass http://127.0.0.1:9002/api;
    }
    location /legacy/ {
        proxy_pass http://127.0.0.1:9003;
    }
}
"#,
        )
        .expect("materialize");
        let proxy_resources = config
            .resources
            .iter()
            .filter_map(|resource| match resource {
                crate::config::ResourceConfig::Proxy {
                    upstream_ref,
                    target_uri,
                    ..
                } => Some((upstream_ref.clone(), target_uri.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(proxy_resources.contains(&("api".to_owned(), Some("/".to_owned()))));
        assert!(
            proxy_resources
                .contains(&("literal-127-0-0-1-9002".to_owned(), Some("/api".to_owned()))),
            "{proxy_resources:?}"
        );
        assert!(proxy_resources.contains(&("literal-127-0-0-1-9003".to_owned(), None)));
        // Literal upstream targets never carry the proxy_pass URI part.
        let literal = config
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "literal-127-0-0-1-9002")
            .expect("literal upstream");
        assert_eq!(literal.targets[0].url, "http://127.0.0.1:9002");
    }

    #[test]
    fn return_url_form_is_a_302_redirect_and_444_fails_closed() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name redirect.example.com;
    location / {
        return https://$host$request_uri;
    }
}
"#,
        )
        .expect("materialize");
        let redirect = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Redirect {
                    status, location, ..
                } => Some((*status, location.clone())),
                _ => None,
            })
            .expect("redirect resource");
        assert_eq!(redirect.0, 302);
        assert_eq!(redirect.1, "https://$host$request_uri");
        let error = materialize(
            r#"
server {
    listen 80;
    server_name close.example.com;
    location / { return 444; }
}
"#,
        )
        .expect_err("return 444 must fail closed");
        assert!(error.to_string().contains("return 444"), "{error}");
    }

    #[test]
    fn alias_without_trailing_slash_accepts_nginx_forms_and_rejects_gluing() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name alias.example.com;
    location /static {
        alias /srv/www/static;
    }
}
"#,
        )
        .expect("materialize");
        let static_resource = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Static {
                    root, strip_prefix, ..
                } => Some((root.clone(), *strip_prefix)),
                _ => None,
            })
            .expect("static resource");
        assert_eq!(static_resource.0, "/srv/www/static");
        assert!(static_resource.1, "alias must strip the route prefix");
        let error = materialize(
            r#"
server {
    listen 80;
    server_name glue.example.com;
    location /static/ {
        alias /srv/www/static;
    }
}
"#,
        )
        .expect_err("alias gluing footgun must fail closed");
        assert!(error.to_string().contains("trailing slash"), "{error}");
    }

    #[test]
    fn location_alias_wins_over_an_inherited_server_root() {
        // nginx consults `clcf->alias` before `clcf->root`, so a location
        // that declares `alias` while the server declares `root` must use
        // the alias. Falling back to the inherited root silently rewrote the
        // document root: working configurations 404, and an alias that
        // deliberately points outside the document root stops doing so.
        let config = materialize(
            r#"
server {
    listen 80;
    server_name alias-shadow.example.com;
    root /srv/www;
    location /docs/ {
        alias /srv/shared/docs/;
    }
}
"#,
        )
        .expect("materialize");
        let static_resource = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Static {
                    root, strip_prefix, ..
                } => Some((root.clone(), *strip_prefix)),
                _ => None,
            })
            .expect("static resource");
        assert_eq!(static_resource.0, "/srv/shared/docs/");
        assert!(
            static_resource.1,
            "a location alias must still replace the matched prefix"
        );
    }

    #[test]
    fn location_alias_survives_a_root_declared_on_another_location() {
        // Two locations on the same server: only one declares `root`, the
        // other declares `alias`. The alias must not pick up its sibling's
        // root, and the root location must keep the full request path.
        let config = materialize(
            r#"
server {
    listen 80;
    server_name siblings.example.com;
    location /a/ {
        root /srv/www;
    }
    location /b/ {
        alias /srv/external/b/;
    }
}
"#,
        )
        .expect("materialize");
        let static_resources: Vec<(String, bool)> = config
            .resources
            .iter()
            .filter_map(|resource| match resource {
                crate::config::ResourceConfig::Static {
                    root, strip_prefix, ..
                } => Some((root.clone(), *strip_prefix)),
                _ => None,
            })
            .collect();
        assert_eq!(static_resources.len(), 2, "{static_resources:?}");
        assert!(
            static_resources.contains(&("/srv/www".to_owned(), false)),
            "the root location keeps the full request path: {static_resources:?}"
        );
        assert!(
            static_resources.contains(&("/srv/external/b/".to_owned(), true)),
            "the alias location replaces the matched prefix: {static_resources:?}"
        );
    }

    #[test]
    fn try_files_only_location_still_inherits_the_server_root() {
        // The alias precedence fix must not break the other direction: a
        // location with only `try_files` still inherits the server root and
        // keeps the full request path (SPA fallback layout).
        let config = materialize(
            r#"
server {
    listen 80;
    server_name spa.example.com;
    root /srv/www;
    location / {
        try_files $uri /index.html;
    }
}
"#,
        )
        .expect("materialize");
        let static_resource = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Static {
                    root,
                    strip_prefix,
                    spa_fallback,
                    ..
                } => Some((root.clone(), *strip_prefix, spa_fallback.clone())),
                _ => None,
            })
            .expect("static resource");
        assert_eq!(static_resource.0, "/srv/www");
        assert!(
            !static_resource.1,
            "an inherited root keeps the full request path"
        );
        assert_eq!(static_resource.2.as_deref(), Some("index.html"));
    }

    #[test]
    fn root_materializes_without_prefix_stripping() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name root.example.com;
    location /assets/ {
        root /srv/www;
    }
}
"#,
        )
        .expect("materialize");
        let static_resource = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Static {
                    root, strip_prefix, ..
                } => Some((root.clone(), *strip_prefix)),
                _ => None,
            })
            .expect("static resource");
        assert_eq!(static_resource.0, "/srv/www");
        assert!(
            !static_resource.1,
            "nginx root must keep the full request path"
        );
    }

    #[test]
    fn acme_challenge_location_declares_the_listener_webroot() {
        let config = materialize(
            r#"
upstream gateway {
    server 127.0.0.1:3800;
}
server {
    listen 443 ssl;
    listen 80;
    server_name edge.example.com;
    ssl_certificate /etc/ssl/edge/fullchain.pem;
    ssl_certificate_key /etc/ssl/edge/privkey.pem;
    location ^~ /.well-known/acme-challenge/ {
        root /var/lib/sdkwork/webserver/acme-webroot;
    }
    location / {
        proxy_pass http://gateway;
    }
}
"#,
        )
        .expect("materialize");
        assert_eq!(
            config.listeners.len(),
            2,
            "`listen 443 ssl` and `listen 80` are one listener each"
        );
        for listener in &config.listeners {
            let acme = listener
                .acme_http_01
                .as_ref()
                .expect("every listener of the server serves the challenge namespace");
            assert_eq!(acme.webroot, "/var/lib/sdkwork/webserver/acme-webroot");
        }
        assert!(
            !config.resources.iter().any(|resource| matches!(
                resource,
                crate::config::ResourceConfig::Static { root, .. }
                    if root == "/var/lib/sdkwork/webserver/acme-webroot"
            )),
            "the challenge namespace is a listener endpoint, not a static route"
        );
    }

    #[test]
    fn acme_challenge_match_accepts_the_directory_without_a_trailing_slash() {
        let config = materialize(
            r#"
upstream gateway {
    server 127.0.0.1:3800;
}
server {
    listen 80;
    server_name edge.example.com;
    location ^~ /.well-known/acme-challenge {
        root /var/lib/acme;
    }
    location / {
        proxy_pass http://gateway;
    }
}
"#,
        )
        .expect("materialize");
        assert_eq!(
            config.listeners[0]
                .acme_http_01
                .as_ref()
                .map(|acme| acme.webroot.as_str()),
            Some("/var/lib/acme")
        );
    }

    #[test]
    fn acme_challenge_shared_across_domains_keeps_one_listener_webroot() {
        // One port-80 listener serves every domain of the edge, so the same
        // declaration repeated per server block must agree rather than conflict.
        let config = materialize(
            r#"
upstream gateway {
    server 127.0.0.1:3800;
}
server {
    listen 80;
    server_name a.example.com;
    location ^~ /.well-known/acme-challenge/ { root /var/lib/acme; }
    location / { proxy_pass http://gateway; }
}
server {
    listen 80;
    server_name b.example.com;
    location ^~ /.well-known/acme-challenge/ { root /var/lib/acme; }
    location / { proxy_pass http://gateway; }
}
"#,
        )
        .expect("materialize");
        assert_eq!(config.listeners.len(), 1);
        assert_eq!(
            config.listeners[0]
                .acme_http_01
                .as_ref()
                .map(|acme| acme.webroot.as_str()),
            Some("/var/lib/acme")
        );
    }

    #[test]
    fn conflicting_acme_webroots_on_one_listener_fail_closed() {
        let error = materialize(
            r#"
upstream gateway {
    server 127.0.0.1:3800;
}
server {
    listen 80;
    server_name a.example.com;
    location ^~ /.well-known/acme-challenge/ { root /var/lib/acme-a; }
    location / { proxy_pass http://gateway; }
}
server {
    listen 80;
    server_name b.example.com;
    location ^~ /.well-known/acme-challenge/ { root /var/lib/acme-b; }
    location / { proxy_pass http://gateway; }
}
"#,
        )
        .expect_err("one listener cannot serve two challenge webroots");
        let message = error.to_string();
        assert!(
            message.contains("one listener serves one webroot"),
            "{message}"
        );
    }

    #[test]
    fn a_proxied_acme_challenge_location_is_not_reinterpreted() {
        // Proxying the challenge path asks for a mechanism the narrow endpoint
        // cannot express. It must stay an ordinary route rather than being
        // silently rewritten into a filesystem webroot the operator never
        // provisioned.
        let config = materialize(
            r#"
upstream gateway {
    server 127.0.0.1:3800;
}
server {
    listen 80;
    server_name legacy.example.com;
    location ^~ /.well-known/acme-challenge/ {
        proxy_pass http://gateway;
    }
    location / {
        proxy_pass http://gateway;
    }
}
"#,
        )
        .expect("materialize");
        assert!(config.listeners[0].acme_http_01.is_none());
        assert_eq!(
            config.resources.len(),
            2,
            "both locations stay proxy routes"
        );
    }

    #[test]
    fn an_acme_challenge_location_without_a_root_fails_closed() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name edge.example.com;
    location ^~ /.well-known/acme-challenge/ {
        index index.html;
    }
}
"#,
        )
        .expect_err("a challenge location must name the webroot");
        let message = error.to_string();
        assert!(message.contains("requires `root <webroot>`"), "{message}");
    }

    #[test]
    fn try_files_accepts_nginx_standard_probes_and_codes() {
        // nginx-standard literal probe + `=code` fallback materialize as a
        // static resource without a SPA fallback (miss -> 404).
        let resources = materialize(
            r#"
server {
    listen 80;
    server_name tf.example.com;
    location / {
        root /srv/www;
        try_files /__sdkwork_probe__ $uri $uri/ =404;
    }
}
"#,
        )
        .expect("nginx-standard try_files must be accepted");
        let static_resource = resources
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Static {
                    root, spa_fallback, ..
                } => Some((root.clone(), spa_fallback.clone())),
                _ => None,
            })
            .expect("static resource");
        assert_eq!(static_resource.0, "/srv/www");
        assert!(
            static_resource.1.is_none(),
            "=code fallback must not become a SPA fallback"
        );
    }

    #[test]
    fn try_files_rejects_unsupported_forms() {
        for (suffix, expected) in [
            ("@named", "not a declared named location"),
            ("$1", "try_files fallback"),
        ] {
            let error = materialize(&format!(
                r#"
server {{
    listen 80;
    server_name tf.example.com;
    location / {{
        root /srv/www;
        try_files $uri $uri/ {suffix};
    }}
}}
"#
            ))
            .err()
            .unwrap_or_else(|| panic!("{suffix} must fail closed"));
            assert!(error.to_string().contains(expected), "{suffix}: {error}");
        }
        let resources = materialize(
            r#"
server {
    listen 80;
    server_name tf.example.com;
    location / {
        root /srv/www;
        try_files /fixed.html $uri /index.html;
    }
}
"#,
        )
        .expect("literal intermediate try_files entries are nginx-standard");
        let static_resource = resources
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Static {
                    root, spa_fallback, ..
                } => Some((root.clone(), spa_fallback.clone())),
                _ => None,
            })
            .expect("static resource");
        assert_eq!(static_resource.0, "/srv/www");
        assert_eq!(
            static_resource.1.as_deref(),
            Some("index.html"),
            "literal fallback becomes the SPA fallback"
        );
    }

    #[test]
    fn upstream_server_unknown_parameters_fail_closed() {
        let error = materialize(
            r#"
upstream api {
    server 127.0.0.1:9001 resolve;
}

server {
    listen 80;
    server_name up.example.com;
    location / { proxy_pass http://api; }
}
"#,
        )
        .expect_err("resolve parameter must fail closed");
        assert!(
            error
                .to_string()
                .contains("unsupported upstream server parameter"),
            "{error}"
        );
    }

    #[test]
    fn stream_multiple_listen_materializes_one_server_per_listen() {
        let config = materialize(
            r#"
stream {
    server {
        listen 5100;
        listen 5101;
        proxy_pass 127.0.0.1:15100;
    }
}
"#,
        )
        .expect("materialize");
        let ports = config
            .streams
            .iter()
            .map(|stream| stream.port)
            .collect::<Vec<_>>();
        assert_eq!(ports, vec![5100, 5101]);
    }

    #[test]
    fn stream_proxy_protocol_v2_fails_closed() {
        let error = materialize(
            r#"
stream {
    server {
        listen 5102;
        proxy_pass 127.0.0.1:15102;
        proxy_protocol v2;
    }
}
"#,
        )
        .expect_err("proxy_protocol v2 must fail closed");
        assert!(error.to_string().contains("proxy_protocol v2"), "{error}");
    }

    #[test]
    fn per_location_client_max_body_size_records_the_effective_route_limit() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name body.example.com;
    client_max_body_size 1m;
    location /a/ {
        client_max_body_size 10m;
        return 200 "a";
    }
    location /b/ { return 200 "b"; }
}
"#,
        )
        .expect("per-location body limits materialize");
        assert_eq!(
            config.virtual_hosts[0].routes[0].max_request_body_bytes,
            Some(10 * 1024 * 1024)
        );
        assert_eq!(
            config.virtual_hosts[0].routes[1].max_request_body_bytes,
            Some(1024 * 1024)
        );
        // The app-level bound is the largest declared value so a request is
        // never rejected before the location limit can be applied.
        assert_eq!(config.limits.max_request_body_bytes, 10 * 1024 * 1024);
    }

    #[test]
    fn every_nginx_server_name_form_materializes() {
        for name in [
            "exact.example.com",
            "*.lead.example.com",
            "www.example.*",
            ".dot.example.com",
            r"~^www\d+\.example\.com$",
        ] {
            let config = materialize(&format!(
                r#"
server {{
    listen 80;
    server_name {name};
    location / {{ return 200 "ok"; }}
}}
"#
            ))
            .unwrap_or_else(|error| panic!("{name:?} must materialize: {error}"));
            assert_eq!(
                config.virtual_hosts[0].server_names,
                vec![name.to_owned()],
                "authored server_name is preserved"
            );
        }
    }

    #[test]
    fn server_name_forms_the_runtime_cannot_execute_fail_closed() {
        for (name, expected) in [
            ("\"\"", "empty `server_name"),
            ("$hostname", "server_name $hostname"),
            ("*lead.example.com", "not a supported nginx form"),
            ("www.*.example.com", "not a supported nginx form"),
            ("~", "not a supported nginx form"),
        ] {
            let error = materialize(&format!(
                r#"
server {{
    listen 80;
    server_name {name};
    location / {{ return 200 "ok"; }}
}}
"#
            ))
            .err()
            .unwrap_or_else(|| panic!("{name:?} must fail closed"));
            assert!(error.to_string().contains(expected), "{name:?}: {error}");
        }
    }

    #[test]
    fn generic_named_locations_materialize_as_bounded_internal_redirect_targets() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name named.example.com;
    root /srv/www;
    error_page 404 = @fallback;
    location / {
        try_files $uri $uri/ @fallback;
    }
    location @fallback {
        root /srv/fallback;
    }
}
"#,
        )
        .expect("generic named location with root must materialize");
        let named_route = config.virtual_hosts[0]
            .routes
            .iter()
            .find(|route| route.route_match.path == "@fallback")
            .expect("named route materialized");
        assert_eq!(
            named_route.route_match.path_type,
            crate::config::RoutePathType::Exact
        );
        let main_route = config.virtual_hosts[0]
            .routes
            .iter()
            .find(|route| route.route_match.path == "/")
            .expect("main route");
        let crate::config::ResourceConfig::Static {
            root, spa_fallback, ..
        } = config
            .resources
            .iter()
            .find(|resource| matches!(resource, crate::config::ResourceConfig::Static { .. }))
            .expect("static resource")
        else {
            panic!("static resource");
        };
        assert_eq!(root, "/srv/www");
        assert_eq!(
            spa_fallback.as_deref(),
            Some("@fallback"),
            "try_files @named becomes the bounded named fallback"
        );
        let page = &config.virtual_hosts[0].error_pages[0];
        assert_eq!(page.uri, "@fallback");
        assert!(page.from_target, "bare `=` keeps the target status");
        let _ = main_route;
    }

    #[test]
    fn named_locations_support_return_proxy_and_try_files_serving_behaviors() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name named.example.com;
    root /srv/www;
    location /a { try_files $uri @ret; }
    location /b { try_files $uri @proxy; }
    location /c { try_files $uri @static; }
    location @ret { return 200 "named-return"; }
    location @proxy { proxy_pass http://127.0.0.1:9001; }
    location @static { try_files $uri /index.html; }
}
"#,
        )
        .expect("named locations with return/proxy/try_files must materialize");
        assert!(config.virtual_hosts[0]
            .routes
            .iter()
            .any(|route| route.route_match.path == "@ret"));
        assert!(config.virtual_hosts[0]
            .routes
            .iter()
            .any(|route| route.route_match.path == "@proxy"));
        assert!(config.virtual_hosts[0]
            .routes
            .iter()
            .any(|route| route.route_match.path == "@static"));
    }

    #[test]
    fn named_location_chains_and_unknown_targets_fail_closed() {
        // One internal redirect per selection: a named location must not
        // itself redirect to another named location.
        let error = materialize(
            r#"
server {
    listen 80;
    server_name chain.example.com;
    root /srv/www;
    location / { try_files $uri @a; }
    location @a { try_files $uri @b; }
    location @b { root /srv/b; }
}
"#,
        )
        .expect_err("named -> named chain must fail closed");
        assert!(
            error.to_string().contains("internal redirect depth bound"),
            "{error}"
        );

        let error = materialize(
            r#"
server {
    listen 80;
    server_name missing.example.com;
    location / { try_files $uri @missing; }
}
"#,
        )
        .expect_err("unknown named target must fail closed");
        assert!(
            error.to_string().contains("not a declared named location"),
            "{error}"
        );
    }

    #[test]
    fn ssl_servers_reject_trailing_wildcards_and_carry_regex_names() {
        let error = materialize(
            r#"
server {
    listen 443 ssl;
    server_name www.example.*;
    ssl_certificate /etc/ssl/cert.pem;
    ssl_certificate_key /etc/ssl/key.pem;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("trailing wildcard has no certificate-index representation");
        assert!(
            error.to_string().contains("SNI certificate index"),
            "{error}"
        );

        let config = materialize(
            r#"
server {
    listen 443 ssl;
    server_name ~^(admin[a-zA-Z0-9_-]+)\.example\.com$;
    ssl_certificate /etc/ssl/cert.pem;
    ssl_certificate_key /etc/ssl/key.pem;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect("regex server names are carried into the certificate SNI index");
        assert_eq!(
            config.certificates[0].server_name_regexes,
            vec!["^(admin[a-zA-Z0-9_-]+)\\.example\\.com$".to_owned()]
        );
    }

    #[test]
    fn dot_prefixed_server_name_expands_for_the_certificate_index() {
        let config = materialize(
            r#"
server {
    listen 443 ssl;
    server_name .example.com;
    ssl_certificate /etc/ssl/cert.pem;
    ssl_certificate_key /etc/ssl/key.pem;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect(".example.com expands to the exact name plus the wildcard");
        assert_eq!(
            config.certificates[0].server_names,
            vec!["example.com".to_owned(), "*.example.com".to_owned()]
        );
    }

    #[test]
    fn named_error_page_cycles_fail_closed_with_recursive_error_pages() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name cycle.example.com;
    root /srv/www;
    recursive_error_pages on;
    error_page 404 = @fallback;
    location / { try_files $uri @fallback; }
    location @fallback { root /srv/www; }
}
"#,
        )
        .expect_err("a recursive named error_page cycle must fail closed");
        assert!(error.to_string().contains("cycle"), "{error}");
    }

    #[test]
    fn adaptive_named_locations_keep_their_placeholder_semantics() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name adaptive.example.com;
    location @pc {
        root /opt/deploy/sdkwork-space/sdkwork-im/apps/sdkwork-im-pc/dist/dev;
        try_files $uri $uri/ /index.html;
    }
    location @h5 {
        root /opt/deploy/sdkwork-space/sdkwork-im/apps/sdkwork-im-h5/dist/dev;
        try_files $uri $uri/ /index.html;
    }
    location / {
        try_files $uri $uri/ @sdkwork_webserver_surface_final;
    }
}
"#,
        )
        .expect("adaptive dispatch with @pc/@h5 must materialize");
        assert_eq!(config.resources.len(), 1);
        let crate::config::ResourceConfig::Static { root, h5_root, .. } = &config.resources[0]
        else {
            panic!("expected static adaptive resource");
        };
        assert!(root.ends_with("sdkwork-im-pc/dist/dev"), "{root}");
        assert_eq!(
            h5_root.as_deref(),
            Some("/opt/deploy/sdkwork-space/sdkwork-im/apps/sdkwork-im-h5/dist/dev")
        );
    }

    #[test]
    fn http_level_proxy_set_headers_are_inherited_by_servers() {
        // nginx: `proxy_set_header` inherits from the previous level only
        // when the current level declares none (nginx docs, proxy_set_header).
        let config = materialize(
            r#"
http {
    proxy_set_header X-Http-Level $scheme;
    upstream api {
        server 127.0.0.1:9001;
    }
    server {
        listen 80;
        server_name headers.example.com;
        location /inherits {
            proxy_pass http://api;
        }
        location /replaces {
            proxy_set_header X-Location-Level $host;
            proxy_pass http://api;
        }
    }
}
"#,
        )
        .expect("materialize");
        // Proxy resources are emitted in location order (`loc-<server>-<n>`);
        // the first location inherits, the second declares its own set.
        let mut proxies: Vec<(String, Vec<String>)> = config
            .resources
            .iter()
            .filter_map(|resource| match resource {
                crate::config::ResourceConfig::Proxy {
                    id,
                    request_set_headers,
                    ..
                } => Some((id.clone(), request_set_headers.clone())),
                _ => None,
            })
            .collect();
        proxies.sort_by(|left, right| left.0.cmp(&right.0));
        let [(_, inherited), (_, replaced)] = proxies.as_slice() else {
            panic!("expected two proxy resources, got {proxies:?}");
        };
        // A location without its own directives inherits the http-level set.
        assert!(
            inherited
                .iter()
                .any(|entry| entry == "X-Http-Level $scheme"),
            "{inherited:?}"
        );
        // A location that declares any proxy_set_header replaces the whole
        // inherited array (nginx semantics).
        assert!(
            replaced
                .iter()
                .any(|entry| entry == "X-Location-Level $host"),
            "{replaced:?}"
        );
        assert!(
            !replaced.iter().any(|entry| entry == "X-Http-Level $scheme"),
            "{replaced:?}"
        );
    }

    #[test]
    fn proxy_cache_zone_references_are_validated() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name cache.example.com;
    location / {
        proxy_cache missing_zone;
        proxy_pass http://127.0.0.1:9001;
    }
}
"#,
        )
        .expect_err("undefined cache zone must fail closed");
        assert!(
            error.to_string().contains("undefined cache zone"),
            "{error}"
        );
        let config = materialize(
            r#"
proxy_cache_path /tmp/cache keys_zone=one:10m;
server {
    listen 80;
    server_name cache.example.com;
    location / {
        proxy_cache one;
        proxy_pass http://127.0.0.1:9001;
    }
}
"#,
        )
        .expect("declared cache zone materializes");
        assert!(config.proxy_cache.enabled);
    }

    #[test]
    fn autoindex_is_accepted_and_process_directives_are_accepted() {
        // The conformance corpus exercises `autoindex on` as part of the
        // accepted surface; directory listing stays an operator policy knob.
        let config = materialize(
            r#"
server {
    listen 80;
    server_name listing.example.com;
    autoindex on;
    location / { root /srv/www; }
}
"#,
        )
        .expect("autoindex on must stay accepted (corpus surface)");
        assert_eq!(config.virtual_hosts.len(), 1);
        let config = materialize(
            r#"
daemon off;
master_process on;
worker_priority -5;
env FOO;
pcre_jit on;
events {
    worker_connections 1024;
}
http {
    server {
        listen 80;
        server_name proc.example.com;
        location / { return 200 "ok"; }
    }
}
"#,
        )
        .expect("stock nginx process directives must be accepted");
        assert_eq!(config.virtual_hosts.len(), 1);
    }

    #[test]
    fn stock_nginx_conf_tuning_directives_are_accepted() {
        let config = materialize(
            r#"
server_names_hash_bucket_size 64;
types_hash_max_size 2048;
types_hash_bucket_size 64;
variables_hash_max_size 1024;
gzip_http_version 1.1;
gzip_buffers 16 8k;
charset_map koi8-r utf-8 { }
keepalive_disable msie6;
keepalive_time 1h;
lingering_timeout 5s;
connection_pool_size 320;
output_buffers 2 32k;
http {
    server {
        listen 80;
        server_name tuning.example.com;
        location / { return 200 "ok"; }
    }
}
"#,
        )
        .expect("stock tuning directives must be accepted");
        assert_eq!(config.virtual_hosts.len(), 1);
    }

    #[test]
    fn listen_os_socket_parameters_are_accepted() {
        let config = materialize(
            r#"
server {
    listen 80 backlog=1024 so_keepalive=on ipv6only=off bind deferred fastopen=10 rcvbuf=64k sndbuf=64k;
    server_name sockets.example.com;
    location / { return 200 "ok"; }
}
stream {
    server {
        listen 5100 backlog=1024 so_keepalive=30s::5 bind;
        proxy_pass 127.0.0.1:15100;
    }
}
"#,
        )
        .expect("OS socket listen parameters must be accepted");
        assert_eq!(config.listeners.len(), 1);
        assert_eq!(config.streams.len(), 1);
    }

    #[test]
    fn cgi_family_bind_and_socket_keepalive_materialize_with_inheritance() {
        let config = materialize(
            r#"
http {
    fastcgi_bind 127.0.0.2;
    fastcgi_socket_keepalive on;
    upstream fcgi { server 127.0.0.1:9001; }
    upstream uwsgi { server 127.0.0.1:9002; }
    upstream scgi { server 127.0.0.1:9003; }
    upstream cache { server 127.0.0.1:9004; }
    server {
        listen 80;
        server_name bind.example.com;
        location /inherits { fastcgi_pass fcgi; }
        location /unbound { fastcgi_bind off; fastcgi_pass fcgi; }
        location /uwsgi {
            uwsgi_bind 127.0.0.3;
            uwsgi_socket_keepalive on;
            uwsgi_pass uwsgi;
        }
        location /scgi {
            scgi_bind off;
            scgi_socket_keepalive off;
            scgi_pass scgi;
        }
        location /cache {
            set $memcached_key $uri;
            memcached_bind 127.0.0.4;
            memcached_socket_keepalive on;
            memcached_pass cache;
        }
    }
}
"#,
        )
        .expect("bind and socket_keepalive directives must materialize");
        // Resources are emitted in location order; this fixture has exactly
        // the five CGI-family locations below.
        assert_eq!(config.resources.len(), 5);
        let find = |index: usize| &config.resources[index];
        let bind = |value: &str| -> std::net::IpAddr { value.parse().expect("ip literal") };
        let crate::config::ResourceConfig::FastCgi {
            bind_address,
            socket_keepalive,
            ..
        } = find(0)
        else {
            panic!("fastcgi resource");
        };
        assert_eq!(*bind_address, Some(bind("127.0.0.2")));
        assert_eq!(*socket_keepalive, Some(true));
        let crate::config::ResourceConfig::FastCgi { bind_address, .. } = find(1) else {
            panic!("fastcgi resource");
        };
        assert_eq!(*bind_address, None, "`off` clears the inherited bind");
        let crate::config::ResourceConfig::CgiUpstream {
            bind_address,
            socket_keepalive,
            ..
        } = find(2)
        else {
            panic!("uwsgi resource");
        };
        assert_eq!(*bind_address, Some(bind("127.0.0.3")));
        assert_eq!(*socket_keepalive, Some(true));
        let crate::config::ResourceConfig::CgiUpstream {
            bind_address,
            socket_keepalive,
            ..
        } = find(3)
        else {
            panic!("scgi resource");
        };
        assert_eq!(*bind_address, None);
        assert_eq!(*socket_keepalive, Some(false));
        let crate::config::ResourceConfig::Memcached {
            bind_address,
            socket_keepalive,
            ..
        } = find(4)
        else {
            panic!("memcached resource");
        };
        assert_eq!(*bind_address, Some(bind("127.0.0.4")));
        assert_eq!(*socket_keepalive, Some(true));
    }

    #[test]
    fn cgi_family_bind_and_socket_keepalive_reject_inexecutable_forms() {
        for (directive, expected) in [
            ("fastcgi_bind example.com;", "IP literal"),
            ("fastcgi_bind 127.0.0.2 transparent;", "one argument"),
            ("uwsgi_bind $address;", "IP literal"),
            ("scgi_bind 127.0.0.1 transparent;", "one argument"),
            ("memcached_bind example.com;", "IP literal"),
            ("fastcgi_socket_keepalive maybe;", "requires on or off"),
            ("memcached_socket_keepalive maybe;", "requires on or off"),
        ] {
            let error = materialize(&format!(
                r#"
server {{
    listen 80;
    server_name bind.example.com;
    location / {{ {directive} fastcgi_pass 127.0.0.1:9001; }}
}}
"#
            ))
            .err()
            .unwrap_or_else(|| panic!("{directive:?} must fail closed"));
            assert!(
                error.to_string().contains(expected),
                "{directive:?}: {error}"
            );
        }
    }

    #[test]
    fn unix_pass_targets_point_at_the_typed_plane_confinement_root() {
        for (directive, expected) in [
            ("fastcgi_pass unix:/run/php.sock;", "unixSocketRoot"),
            ("uwsgi_pass unix:/run/uwsgi.sock;", "unixSocketRoot"),
            ("scgi_pass unix:/run/scgi.sock;", "unixSocketRoot"),
            (
                "set $memcached_key $uri; memcached_pass unix:/run/mc.sock;",
                "unixSocketRoot",
            ),
        ] {
            let error = materialize(&format!(
                r#"
server {{
    listen 80;
    server_name unix.example.com;
    location / {{ {directive} }}
}}
"#
            ))
            .err()
            .unwrap_or_else(|| panic!("{directive:?} must fail closed"));
            assert!(
                error.to_string().contains(expected),
                "{directive:?}: {error}"
            );
        }
    }

    #[test]
    fn upstream_keepalive_family_is_accepted() {
        let config = materialize(
            r#"
upstream api {
    server 127.0.0.1:9001;
    keepalive 32;
    keepalive_timeout 60s;
    keepalive_requests 1000;
    keepalive_time 1h;
}

server {
    listen 80;
    server_name keepalive.example.com;
    location / { proxy_pass http://api; }
}
"#,
        )
        .expect("upstream keepalive family must be accepted");
        assert_eq!(config.upstreams.len(), 1);
    }

    #[test]
    #[allow(clippy::useless_format)]
    fn proxy_ssl_settings_materialize_upstream_tls() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("ca.pem"), "ca").unwrap();
        std::fs::write(directory.path().join("client.pem"), "cert").unwrap();
        std::fs::write(directory.path().join("client.key"), "key").unwrap();
        let parsed = parse_nginx_config(
            &format!(
                r#"
upstream api {{
    server https://127.0.0.1:9001;
}}

server {{
    listen 80;
    server_name tls.example.com;
    location / {{
        proxy_ssl_verify on;
        proxy_ssl_trusted_certificate ca.pem;
        proxy_ssl_certificate client.pem;
        proxy_ssl_certificate_key client.key;
        proxy_ssl_server_name on;
        proxy_pass https://api;
    }}
}}
"#
            ),
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize");
        let upstream = config
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "api")
            .expect("api upstream");
        let tls = upstream.tls.as_ref().expect("upstream tls");
        assert_eq!(tls.trust_mode, crate::config::UpstreamTlsTrustMode::Custom);
        assert_eq!(tls.ca_certificate_files.len(), 1);
        assert!(tls.ca_certificate_files[0].ends_with("ca.pem"));
        assert!(tls
            .client_certificate_file
            .as_deref()
            .unwrap()
            .ends_with("client.pem"));
        assert!(tls
            .client_private_key_file
            .as_deref()
            .unwrap()
            .ends_with("client.key"));
    }

    #[test]
    fn proxy_ssl_unsupported_forms_fail_closed() {
        for (body, expected) in [
            (
                "proxy_ssl_verify off; proxy_pass https://127.0.0.1:9001;",
                "proxy_ssl_verify off",
            ),
            (
                "proxy_ssl_server_name off; proxy_pass https://127.0.0.1:9001;",
                "proxy_ssl_server_name off",
            ),
            (
                "proxy_ssl_trusted_certificate ca.pem; proxy_pass https://127.0.0.1:9001;",
                "requires `proxy_ssl_verify on`",
            ),
            (
                "proxy_ssl_certificate c.pem; proxy_pass https://127.0.0.1:9001;",
                "requires `proxy_ssl_certificate_key`",
            ),
            (
                "proxy_ssl_name api.internal; proxy_pass https://127.0.0.1:9001;",
                "custom upstream SNI name",
            ),
            (
                "proxy_ssl_verify on; proxy_pass http://127.0.0.1:9001;",
                "require an `https://`",
            ),
        ] {
            let error = materialize(&format!(
                r#"
server {{
    listen 80;
    server_name tlsfail.example.com;
    location / {{
        {body}
    }}
}}
"#
            ))
            .err()
            .unwrap_or_else(|| panic!("{body} must fail closed"));
            assert!(error.to_string().contains(expected), "{body}: {error}");
        }
    }

    #[test]
    fn conflicting_proxy_ssl_settings_for_one_upstream_fail_closed() {
        let error = materialize(
            r#"
upstream api {
    server https://127.0.0.1:9001;
}

server {
    listen 80;
    server_name conflict.example.com;
    location /a/ {
        proxy_ssl_verify on;
        proxy_ssl_trusted_certificate ca-a.pem;
        proxy_pass https://api;
    }
    location /b/ {
        proxy_ssl_verify on;
        proxy_ssl_trusted_certificate ca-b.pem;
        proxy_pass https://api;
    }
}
"#,
        )
        .expect_err("conflicting upstream TLS settings must fail closed");
        assert!(
            error
                .to_string()
                .contains("conflicting `proxy_ssl_*` settings"),
            "{error}"
        );
    }

    #[test]
    fn real_ip_directives_materialize_trusted_proxy() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name realip.example.com;
    set_real_ip_from 10.0.0.0/8;
    set_real_ip_from 192.168.1.1;
    real_ip_header X-Forwarded-For;
    real_ip_recursive on;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect("materialize");
        let listener = config
            .listeners
            .iter()
            .find(|listener| listener.port == 80)
            .expect("listener");
        let trusted = listener
            .trusted_proxy
            .as_ref()
            .expect("trusted proxy policy");
        assert_eq!(
            trusted.trusted_cidrs,
            vec![
                "10.0.0.0/8".parse::<ipnet::IpNet>().unwrap(),
                "192.168.1.1/32".parse::<ipnet::IpNet>().unwrap()
            ]
        );
        assert!(trusted.recursive);
        let error = materialize(
            r#"
server {
    listen 80;
    server_name realipfail.example.com;
    real_ip_header X-Real-IP;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("unsupported real_ip_header must fail closed");
        assert!(error.to_string().contains("X-Forwarded-For"), "{error}");
        let error = materialize(
            r#"
server {
    listen 80;
    server_name realipconflict.example.com;
    set_real_ip_from 10.0.0.0/8;
    location / { return 200 "ok"; }
}

server {
    listen 80;
    server_name other.example.com;
    set_real_ip_from 192.168.0.0/16;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("conflicting real_ip settings must fail closed");
        assert!(
            error.to_string().contains("conflicting `set_real_ip_from`"),
            "{error}"
        );
    }

    #[test]
    #[allow(clippy::useless_format)]
    fn http_level_http2_on_inherits_to_ssl_listeners() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("site.pem"), "cert").unwrap();
        std::fs::write(directory.path().join("site.key"), "key").unwrap();
        let parsed = parse_nginx_config(
            &format!(
                r#"
http {{
    http2 on;
    server {{
        listen 443 ssl;
        server_name h2http.example.com;
        ssl_certificate site.pem;
        ssl_certificate_key site.key;
        location / {{ return 200 "ok"; }}
    }}
}}
"#
            ),
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let config =
            materialize_nginx_app_default(&parsed, directory.path(), "test").expect("materialize");
        let listener = config
            .listeners
            .iter()
            .find(|listener| listener.port == 443)
            .expect("443 listener");
        assert!(
            listener
                .protocols
                .contains(&crate::config::ListenerProtocol::Http2),
            "http-level http2 on; must enable h2: {:?}",
            listener.protocols
        );
    }

    #[test]
    fn variable_return_bodies_fail_closed() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name bodyvar.example.com;
    location / { return 200 "$upstream_addr"; }
}
"#,
        )
        .expect_err("unsupported variable return bodies must fail closed");
        assert!(
            error
                .to_string()
                .contains("uses variables the runtime does not expand"),
            "{error}"
        );
    }

    fn proxy_resource_policy(config: &WebServerAppConfig) -> &ProxyResponsePolicyConfig {
        config
            .resources
            .iter()
            .find_map(|resource| match resource {
                ResourceConfig::Proxy {
                    proxy_response_policy,
                    ..
                } => proxy_response_policy.as_ref(),
                _ => None,
            })
            .expect("proxy resource with a response policy")
    }

    #[test]
    fn materializes_proxy_response_header_policy_with_nginx_inheritance() {
        let config = materialize(
            r#"
proxy_hide_header X-Http-Hide;

server {
    listen 80;
    server_name policy.example.test;
    root /srv/www;

    proxy_hide_header X-Server-Hide;
    proxy_pass_header X-Server-Pass;

    location /api/ {
        proxy_pass http://127.0.0.1:8080/v1/;
        # A location that declares a directive family replaces the inherited
        # one (nginx replace-on-declare); undeclared families keep inheriting.
        proxy_hide_header X-Location-Hide;
        proxy_cookie_path /upstream /client;
        proxy_cookie_domain .upstream.example.test .edge.example.test;
    }
}
"#,
        )
        .expect("materialize");
        let policy = proxy_resource_policy(&config);
        // `proxy_hide_header` declared at the location replaces the server +
        // http arrays; `proxy_pass_header` is inherited from the server.
        assert_eq!(policy.hide_headers, vec!["X-Location-Hide".to_owned()]);
        assert_eq!(policy.pass_headers, vec!["X-Server-Pass".to_owned()]);
        // `proxy_redirect default` is expanded from the raw `proxy_pass`
        // argument and the location name.
        assert_eq!(
            policy.redirect,
            Some(ProxyRedirectPolicyConfig::Rules(vec![
                ProxyRewriteRuleConfig {
                    from: "http://127.0.0.1:8080/v1/".to_owned(),
                    to: "/api/".to_owned(),
                }
            ]))
        );
        assert_eq!(
            policy.cookie_path,
            Some(CookieRewritePolicyConfig::Rules(vec![
                ProxyRewriteRuleConfig {
                    from: "/upstream".to_owned(),
                    to: "/client".to_owned(),
                }
            ]))
        );
        // One leading dot is stripped from the configured domain pattern and
        // replacement (nginx normalizes them at configuration time).
        assert_eq!(
            policy.cookie_domain,
            Some(CookieRewritePolicyConfig::Rules(vec![
                ProxyRewriteRuleConfig {
                    from: "upstream.example.test".to_owned(),
                    to: "edge.example.test".to_owned(),
                }
            ]))
        );
    }

    #[test]
    fn proxy_redirect_default_uses_the_upstream_base_without_a_uri_part() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name policy.example.test;
    location /nouri/ {
        proxy_pass http://127.0.0.1:8080;
        proxy_redirect default;
    }
}
"#,
        )
        .expect("materialize");
        assert_eq!(
            proxy_resource_policy(&config).redirect,
            Some(ProxyRedirectPolicyConfig::Rules(vec![
                ProxyRewriteRuleConfig {
                    from: "http://127.0.0.1:8080/".to_owned(),
                    to: "/".to_owned(),
                }
            ]))
        );
    }

    #[test]
    fn proxy_response_directives_fail_closed_on_unexecutable_forms() {
        for (label, location) in [
            (
                "regex redirect",
                r#"proxy_pass http://127.0.0.1:8080/; proxy_redirect ~^http://127.0.0.1:8080/(.*) /$1;"#,
            ),
            (
                "variable redirect",
                r#"proxy_pass http://127.0.0.1:8080/; proxy_redirect http://127.0.0.1:8080/ $scheme://client/;"#,
            ),
            (
                "default with variable proxy_pass",
                r#"proxy_pass http://$host; proxy_redirect default;"#,
            ),
        ] {
            let error = materialize(&format!(
                "server {{ listen 80; server_name x.example.test; location / {{ {location} }} }}"
            ))
            .err()
            .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            let message = error.to_string();
            assert!(
                message.contains("proxy_redirect") || message.contains("proxy_cookie"),
                "`{label}` diagnostic must name the directive: {message}"
            );
        }
    }

    #[test]
    fn materializes_internal_locations() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name internal.example.test;
    location /open/ { return 200 "open"; }
    location /hidden/ { internal; return 200 "hidden"; }
}
"#,
        )
        .expect("materialize");
        let routes = &config.virtual_hosts[0].routes;
        assert!(
            !routes
                .iter()
                .find(|route| route.route_match.path == "/open/")
                .expect("open route")
                .internal
        );
        assert!(
            routes
                .iter()
                .find(|route| route.route_match.path == "/hidden/")
                .expect("hidden route")
                .internal
        );
        // `internal` is a location-context directive in nginx.
        assert!(materialize(
            "server { listen 80; server_name x.example.test; internal; location / { return 200 \"x\"; } }"
        )
        .is_err());
    }

    #[test]
    fn materializes_request_boundary_policies_with_inheritance() {
        let config = materialize(
            r#"
merge_slashes off;
underscores_in_headers on;

server {
    listen 80;
    server_name boundary.example.test;
    location / { return 200 "ok"; }
}

server {
    listen 80;
    server_name override.example.test;
    merge_slashes on;
    underscores_in_headers off;
    ignore_invalid_headers off;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect("materialize");
        let first = &config.virtual_hosts[0];
        assert!(!first.merge_slashes, "http-level merge_slashes inherits");
        assert!(first.underscores_in_headers);
        assert!(first.ignore_invalid_headers, "nginx default is on");
        // A configuration that declares none of the three still materializes
        // nginx's defaults (merge_slashes on, underscore headers dropped,
        // ignore_invalid_headers on).
        let defaults = materialize(
            "server { listen 80; server_name defaults.example.test; location / { return 200 \"ok\"; } }",
        )
        .expect("materialize");
        let default_host = &defaults.virtual_hosts[0];
        assert!(default_host.merge_slashes);
        assert!(!default_host.underscores_in_headers);
        assert!(default_host.ignore_invalid_headers);
        let second = &config.virtual_hosts[1];
        assert!(second.merge_slashes, "server-level declaration wins");
        assert!(!second.underscores_in_headers);
        assert!(!second.ignore_invalid_headers);
    }

    #[test]
    fn materializes_charset_family_with_nginx_inheritance() {
        let config = materialize(
            r#"
charset utf-8;
charset_types text/css;

server {
    listen 80;
    server_name charset.example.test;
    root /srv/www;

    location /inherit/ { }

    location /off/ {
        charset off;
    }

    location /types/ {
        charset_types application/json;
    }
}
"#,
        )
        .expect("materialize");
        let host = &config.virtual_hosts[0];
        let host_charset = host.charset.as_ref().expect("host charset config");
        assert_eq!(host_charset.charset.as_deref(), Some("utf-8"));
        // A declared `charset_types` list is seeded with `text/html`, then the
        // declared types follow in order.
        assert_eq!(
            host_charset.types.as_deref(),
            Some(["text/html".to_owned(), "text/css".to_owned()].as_slice())
        );
        assert!(!host_charset.override_charset);
        // A location inherits the resolved policy when it declares nothing.
        let inherit = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/inherit/")
            .expect("inherit route");
        assert!(inherit.charset.is_none(), "inherits the host value");
        // `charset off` is a declaration that disables the inherited charset.
        let off = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/off/")
            .expect("off route");
        let off_charset = off.charset.as_ref().expect("route charset config");
        assert_eq!(off_charset.charset, None);
        assert_eq!(off_charset.types, host_charset.types);
        // The location restates only `charset_types`; the host `charset` is
        // resolved in, and the declared list replaces the host one.
        let types = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/types/")
            .expect("types route");
        let types_charset = types.charset.as_ref().expect("route charset config");
        assert_eq!(types_charset.charset.as_deref(), Some("utf-8"));
        assert_eq!(
            types_charset.types.as_deref(),
            Some(["text/html".to_owned(), "application/json".to_owned()].as_slice())
        );
    }

    #[test]
    fn charset_family_fails_closed_on_unexecutable_forms() {
        for (label, location, needle) in [
            (
                "variable charset",
                "charset $upstream_charset; return 200 \"ok\";",
                "variable",
            ),
            (
                "invalid charset token",
                "charset \"utf 8\"; return 200 \"ok\";",
                "charset",
            ),
            (
                "empty charset_types",
                "charset_types; return 200 \"ok\";",
                "charset_types",
            ),
            (
                "invalid charset type",
                "charset_types not-a-media-type; return 200 \"ok\";",
                "MIME",
            ),
            (
                "override_charset bad value",
                "override_charset maybe; return 200 \"ok\";",
                "on or off",
            ),
        ] {
            let error = materialize(&format!(
                "server {{ listen 80; server_name x.example.test; location / {{ {location} }} }}"
            ))
            .err()
            .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(
                error.to_string().contains(needle),
                "`{label}` diagnostic must mention `{needle}`: {error}"
            );
        }
        // `charset_map` is an http-context table; it stays accepted-unenforced.
        materialize(
            "charset_map utf-8 windows-1251 { 90 80; }\nserver { listen 80; server_name x.example.test; location / { return 200 \"ok\"; } }",
        )
        .expect("charset_map table is accepted-unenforced");
        // The accepted-unenforced list is context-insensitive today, so a
        // location-level `charset_map` is accepted too (nginx would reject the
        // context); the table still has no effect either way.
        materialize(
            "server { listen 80; server_name x.example.test; location / { charset_map utf-8 windows-1251 { } return 200 \"ok\"; } }",
        )
        .expect("charset_map stays accepted-unenforced");
    }

    #[test]
    fn materializes_gzip_refinements_with_nginx_inheritance() {
        let config = materialize(
            r#"
gzip_proxied any;

server {
    listen 80;
    server_name gzip.example.test;
    root /srv/www;
    gzip_disable "msie6" "googlebot";

    location /inherit/ { }

    location /loc/ {
        gzip_comp_level 9;
        gzip_proxied no-cache no-store auth;
        gzip_disable "curl";
    }
}
"#,
        )
        .expect("materialize");
        let host = &config.virtual_hosts[0];
        let host_compression = host.compression.as_ref().expect("host compression");
        // http-level `gzip_proxied any` is inherited by the server; the
        // `gzip_disable` regex list and the msie6 flag come from the server.
        assert_eq!(host_compression.proxied.as_ref().map(|p| p.any), Some(true));
        assert_eq!(host_compression.comp_level, 1, "nginx default");
        // `msie6` is the predefined flag, not a regex entry.
        assert_eq!(host_compression.disable, vec!["googlebot".to_owned()]);
        assert!(host_compression.disable_msie6, "msie6 is a separate flag");
        // A location that declares nothing inherits the host policy.
        let inherit = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/inherit/")
            .expect("inherit route");
        assert!(inherit.compression.is_none(), "inherits the host value");
        // A location that declares the family gets fully resolved values: the
        // declared regex list replaces the inherited one, the msie6 flag keeps
        // inheriting (nginx merges the two chains independently), and the
        // declared proxied bitmask replaces the inherited one.
        let location = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/loc/")
            .expect("loc route");
        let location_compression = location.compression.as_ref().expect("route compression");
        // `gzip_comp_level` is accepted-unenforced (runtime-owned level).
        assert_eq!(location_compression.comp_level, 1);
        assert_eq!(location_compression.disable, vec!["curl".to_owned()]);
        assert!(location_compression.disable_msie6);
        let proxied = location_compression
            .proxied
            .as_ref()
            .expect("route proxied policy");
        assert!(proxied.no_cache && proxied.no_store && proxied.auth);
        assert!(!proxied.any, "declared bitmask replaces the inherited one");
        assert!(!proxied.off);
    }

    #[test]
    fn gzip_refinements_fail_closed_on_unexecutable_forms() {
        for (label, location, needle) in [
            (
                "proxied unknown token",
                "gzip_proxied sometimes; return 200 \"ok\";",
                "gzip_proxied",
            ),
            (
                "proxied empty",
                "gzip_proxied; return 200 \"ok\";",
                "gzip_proxied",
            ),
            (
                "disable empty",
                "gzip_disable; return 200 \"ok\";",
                "gzip_disable",
            ),
            (
                "disable unsupported regex",
                "gzip_disable \"a(?=b)\"; return 200 \"ok\";",
                "regex engine",
            ),
        ] {
            let error = materialize(&format!(
                "server {{ listen 80; server_name x.example.test; location / {{ {location} }} }}"
            ))
            .err()
            .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(
                error.to_string().contains(needle),
                "`{label}` diagnostic must mention `{needle}`: {error}"
            );
        }
    }

    #[test]
    fn materializes_autoindex_empty_gif_and_stub_status() {
        let config = materialize(
            r#"
autoindex_exact_size off;

server {
    listen 80;
    server_name listing.example.test;
    root /srv/www;
    autoindex on;

    location /list/ { root /srv/www; }
    location /json/ { autoindex_format json; root /srv/www; }
    location = /pixel.gif { empty_gif; }
    location = /status { stub_status; }
}
"#,
        )
        .expect("materialize");
        let host = &config.virtual_hosts[0];
        let host_autoindex = host.autoindex.as_ref().expect("host autoindex policy");
        assert!(host_autoindex.enabled);
        assert!(
            !host_autoindex.exact_size,
            "http-level declaration inherits"
        );
        assert_eq!(
            host_autoindex.format,
            crate::config::AutoindexFormat::Html,
            "nginx default format"
        );
        // A location that declares nothing inherits the host policy.
        let inherit = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/list/")
            .expect("list route");
        assert!(inherit.autoindex.is_none());
        // A location that declares only the format gets the fully resolved
        // policy (inherited enabled/exact_size included).
        let json = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/json/")
            .expect("json route");
        let json_autoindex = json.autoindex.as_ref().expect("route autoindex");
        assert!(json_autoindex.enabled);
        assert!(!json_autoindex.exact_size);
        assert_eq!(json_autoindex.format, crate::config::AutoindexFormat::Json);
        // The two location handlers materialize as their dedicated resources.
        let resources: Vec<&ResourceConfig> = config.resources.iter().collect();
        assert!(resources
            .iter()
            .any(|resource| matches!(resource, ResourceConfig::EmptyGif { .. })));
        assert!(resources
            .iter()
            .any(|resource| matches!(resource, ResourceConfig::StubStatus { .. })));
    }

    #[test]
    fn autoindex_and_handler_forms_fail_closed() {
        for (label, location, needle) in [
            (
                "bad autoindex format",
                "autoindex_format yaml; return 200 \"ok\";",
                "autoindex_format",
            ),
            (
                "empty_gif with arguments",
                "empty_gif on; return 200 \"ok\";",
                "empty_gif",
            ),
            (
                "stub_status with arguments",
                "stub_status on; return 200 \"ok\";",
                "stub_status",
            ),
        ] {
            let error = materialize(&format!(
                "server {{ listen 80; server_name x.example.test; location / {{ {location} }} }}"
            ))
            .err()
            .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(
                error.to_string().contains(needle),
                "`{label}` diagnostic must mention `{needle}`: {error}"
            );
        }
    }

    #[test]
    fn materializes_http2_max_concurrent_streams_into_ingress_limits() {
        let config = materialize(
            r#"
http2_max_concurrent_streams 7;
server { listen 80; server_name h2.example.test; location / { return 200 "ok"; } }
"#,
        )
        .expect("materialize");
        assert_eq!(config.limits.http2_max_concurrent_streams, 7);
        // A server-level declaration of the same value is accepted (one global
        // ingress value); a conflicting value fails closed.
        materialize(
            r#"
http2_max_concurrent_streams 7;
server { listen 80; server_name h2.example.test; http2_max_concurrent_streams 7; location / { return 200 "ok"; } }
"#,
        )
        .expect("same value at both levels");
        let conflict = materialize(
            r#"
http2_max_concurrent_streams 7;
server { listen 80; server_name h2.example.test; http2_max_concurrent_streams 9; location / { return 200 "ok"; } }
"#,
        )
        .expect_err("conflicting values must fail closed");
        assert!(conflict.to_string().contains("conflicting"), "{conflict}");
        for value in ["0", "10001", "many"] {
            let error = materialize(&format!(
                "http2_max_concurrent_streams {value};\nserver {{ listen 80; server_name h2.example.test; location / {{ return 200 \"ok\"; }} }}"
            ))
            .err()
            .unwrap_or_else(|| panic!("`{value}` must fail closed"));
            assert!(
                error.to_string().contains("http2_max_concurrent_streams"),
                "{error}"
            );
        }
    }

    #[test]
    fn http2_tuning_diagnostics_are_truthful() {
        // The four obsolete directives are accepted (nginx 1.29.6 accepts them
        // as deprecated no-ops) and the diagnostic says so explicitly.
        let parsed = parse_nginx_config(
            r#"
http {
    http2_max_field_size 8k;
    http2_max_header_size 16k;
    http2_max_requests 1000;
    http2_idle_timeout 3m;
    open_file_cache max=1000 inactive=20s;
    ssl_stapling on;
    ssl_trusted_certificate /etc/ssl/chain.pem;
    server { listen 80; server_name h2.example.test; location / { return 200 "ok"; } }
}
"#,
            std::path::Path::new("site.conf"),
        )
        .expect("parse");
        let diagnostics = ignored_directive_diagnostics(&parsed);
        let message = |name: &str| {
            diagnostics
                .iter()
                .find(|diagnostic| diagnostic.message.contains(name))
                .unwrap_or_else(|| panic!("missing diagnostic for {name}: {diagnostics:?}"))
                .message
                .clone()
        };
        for name in [
            "http2_max_field_size",
            "http2_max_header_size",
            "http2_max_requests",
            "http2_idle_timeout",
        ] {
            let text = message(name);
            assert!(
                text.contains("obsolete in the pinned nginx 1.29.6"),
                "`{name}` must state nginx obsoletes it: {text}"
            );
        }
        assert!(
            message("ssl_stapling")
                .contains("dynamic OCSP fetching/verification is not implemented"),
            "ssl_stapling diagnostic must state the dynamic-fetch blocker and the static-file path"
        );
        assert!(
            message("ssl_trusted_certificate").contains("consumed only as an extra PEM chain"),
            "ssl_trusted_certificate diagnostic must state that it is chain-only"
        );
        assert!(
            message("open_file_cache").contains("descriptor/metadata cache"),
            "open_file_cache must state its precise runtime rationale"
        );
        // `http2_chunk_size` is active in nginx but not exposed by hyper/h2:
        // fail closed with a precise diagnostic instead of faking it.
        let error = materialize(
            r#"
http2_chunk_size 4k;
server { listen 80; server_name h2.example.test; location / { return 200 "ok"; } }
"#,
        )
        .expect_err("http2_chunk_size must fail closed");
        assert!(
            error.to_string().contains("hyper/h2 ingress"),
            "http2_chunk_size diagnostic must name the runtime blocker: {error}"
        );
    }

    #[test]
    fn materializes_proxy_method_inheritance() {
        let config = materialize(
            r#"
proxy_method POST;
server {
    listen 80;
    server_name method.example.test;
    location /inherit/ { proxy_pass http://127.0.0.1:8080; }
    location /server/ { proxy_method PUT; proxy_pass http://127.0.0.1:8080; }
    location /location/ { proxy_method PATCH; proxy_pass http://127.0.0.1:8080; }
}
"#,
        )
        .expect("materialize");
        let proxy_methods = config
            .resources
            .iter()
            .filter_map(|resource| match resource {
                ResourceConfig::Proxy { proxy_method, .. } => Some(proxy_method.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            proxy_methods,
            vec![
                Some("POST".to_owned()),
                Some("PUT".to_owned()),
                Some("PATCH".to_owned())
            ],
            "http → server → location proxy_method inheritance"
        );
        // An invalid method token fails closed.
        let error = materialize(
            "server { listen 80; server_name method.example.test; location / { proxy_method \"GET /x\"; proxy_pass http://127.0.0.1:8080; } }",
        )
        .expect_err("invalid method token must fail closed");
        assert!(error.to_string().contains("proxy_method"), "{error}");
    }

    #[test]
    fn materializes_stream_access_and_return() {
        let config = materialize(
            r#"
stream {
    allow 10.0.0.0/8;
    deny all;

    server {
        listen 127.0.0.1:9101;
        proxy_pass 127.0.0.1:9201;
    }

    server {
        listen 127.0.0.1:9102;
        allow 127.0.0.1;
        proxy_pass 127.0.0.1:9201;
    }

    server {
        listen 127.0.0.1:9103;
        return 403;
    }
}
"#,
        )
        .expect("materialize stream");
        let streams = config.streams;
        // The server with no rules inherits the stream-context rules.
        let inherited = streams
            .iter()
            .find(|stream| stream.id.contains("9101"))
            .expect("inheriting stream");
        assert_eq!(inherited.access.len(), 2);
        assert_eq!(inherited.access[0].network, "10.0.0.0/8");
        assert_eq!(inherited.access[1].network, "all");
        // A server declaration replaces the inherited list.
        let own = streams
            .iter()
            .find(|stream| stream.id.contains("9102"))
            .expect("own-access stream");
        assert_eq!(own.access.len(), 1);
        assert_eq!(own.access[0].network, "127.0.0.1");
        assert!(own.target.is_some());
        // `return` streams answer without a target.
        let returned = streams
            .iter()
            .find(|stream| stream.id.contains("9103"))
            .expect("return stream");
        assert_eq!(returned.return_value.as_deref(), Some("403"));
        assert!(returned.target.is_none());
        // Fail-closed forms.
        for (label, body, needle) in [
            (
                "return with an unknown variable",
                "listen 127.0.0.1:9104; return \"$nope\";",
                "does not know at accept/preread time",
            ),
            (
                "return with proxy_pass",
                "listen 127.0.0.1:9105; return 403; proxy_pass 127.0.0.1:9201;",
                "cannot combine `return` with `proxy_pass`",
            ),
            (
                "neither target nor return",
                "listen 127.0.0.1:9106;",
                "requires proxy_pass or return",
            ),
            (
                "invalid access network",
                "listen 127.0.0.1:9107; allow not-a-network; proxy_pass 127.0.0.1:9201;",
                "invalid access network",
            ),
        ] {
            let error = materialize(&format!("stream {{ server {{ {body} }} }}"))
                .err()
                .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(error.to_string().contains(needle), "`{label}`: {error}");
        }
    }

    #[test]
    fn materializes_valid_referers_policy() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name referer.example.test;
    valid_referers none blocked server_names *.example.com example.org/galleries ~\.google\.;
    location /inherit/ { return 200 "ok"; }
    location /own/ { valid_referers server_names; return 200 "ok"; }
}
"#,
        )
        .expect("materialize");
        let host = &config.virtual_hosts[0];
        let host_referer = host.referer.as_ref().expect("host referer policy");
        assert!(host_referer.none);
        assert!(host_referer.blocked);
        assert!(host_referer.server_names);
        assert_eq!(host_referer.hosts.len(), 2);
        assert_eq!(host_referer.hosts[0].host, "*.example.com");
        assert_eq!(host_referer.hosts[1].host, "example.org");
        assert_eq!(host_referer.hosts[1].path.as_deref(), Some("/galleries"));
        assert_eq!(host_referer.regexes.len(), 1);
        // A location without a declaration inherits the host policy (the
        // mapping emits it on the route so the data plane has one lookup).
        let inherit = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/inherit/")
            .expect("inherit route");
        assert_eq!(inherit.referer.as_ref(), Some(host_referer));
        // A location declaration replaces the inherited list.
        let own = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/own/")
            .expect("own route");
        let own_referer = own.referer.as_ref().expect("own referer policy");
        assert!(own_referer.server_names);
        assert!(!own_referer.none);
        assert!(own_referer.hosts.is_empty());
        // Fail-closed forms.
        for (label, body, needle) in [
            (
                "none without a positive matcher",
                "valid_referers none;",
                "requires at least one host",
            ),
            (
                "bad regex",
                "valid_referers ~(unclosed;",
                "does not compile",
            ),
            (
                "invalid wildcard host",
                "valid_referers a*b.example.com;",
                "invalid valid_referers host",
            ),
            (
                "empty directive",
                "valid_referers;",
                "requires at least one matcher",
            ),
        ] {
            let error = materialize(&format!(
                "server {{ listen 80; server_name referer.example.test; {body} location / {{ return 200 \"ok\"; }} }}"
            ))
            .err()
            .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(error.to_string().contains(needle), "`{label}`: {error}");
        }
    }

    #[test]
    fn materializes_geo_and_split_clients_blocks() {
        let config = materialize(
            r#"
geo $geo_zone {
    default "none";
    127.0.0.0/8 "loopback";
    127.0.0.1/32 "loopback-host";
}
geo $http_x_forwarded_for $xff_zone {
    ranges;
    default "no";
    10.0.0.5-10.0.0.10 "ten";
}
split_clients "$arg_key" $split_bucket {
    30% "a";
    30% "b";
    40% "c";
}
server { listen 80; server_name geo.example.test; location / { return 200 "ok"; } }
"#,
        )
        .expect("materialize");
        assert_eq!(config.geo.len(), 2);
        assert_eq!(config.geo[0].source, "$remote_addr");
        assert_eq!(config.geo[0].result, "$geo_zone");
        assert!(!config.geo[0].ranges);
        assert_eq!(config.geo[0].entries.len(), 3);
        assert_eq!(config.geo[0].entries[2].key, "127.0.0.1/32");
        assert!(config.geo[1].ranges);
        assert_eq!(config.geo[1].source, "$http_x_forwarded_for");
        assert_eq!(config.split_clients.len(), 1);
        let split = &config.split_clients[0];
        assert_eq!(split.source, "$arg_key");
        assert_eq!(split.result, "$split_bucket");
        assert_eq!(
            split
                .parts
                .iter()
                .map(|part| part.percent)
                .collect::<Vec<_>>(),
            vec![Some(3000), Some(3000), Some(4000)]
        );
        // Fail-closed forms.
        for (label, block, needle) in [
            (
                "geo CIDR in ranges mode",
                "geo $z { ranges; default \"no\"; 10.0.0.0/8 \"x\"; }",
                "must be an `start-end` address range",
            ),
            (
                "geo range without ranges",
                "geo $z { default \"no\"; 10.0.0.0-10.0.0.10 \"x\"; }",
                "must be a CIDR or IP address",
            ),
            (
                "geo variable value",
                "geo $z { default \"no\"; 10.0.0.0/8 \"$host\"; }",
                "must be literal strings",
            ),
            (
                "split bad percent",
                "split_clients \"$arg_k\" $b { 0% \"x\"; }",
                "invalid split_clients percentage",
            ),
            (
                "split over 100%",
                "split_clients \"$arg_k\" $b { 60% \"x\"; 60% \"y\"; }",
                "total more than 100%",
            ),
            (
                "duplicate variable block",
                "geo $z { default \"a\"; }\ngeo $z { default \"b\"; }",
                "already declared",
            ),
        ] {
            let error = materialize(&format!(
                "{block}\nserver {{ listen 80; server_name geo.example.test; location / {{ return 200 \"ok\"; }} }}"
            ))
            .err()
            .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(error.to_string().contains(needle), "`{label}`: {error}");
        }
    }

    #[test]
    fn materializes_stream_limit_conn_and_dry_run() {
        let config = materialize(
            r#"
stream {
    limit_conn_zone $binary_remote_addr zone=addr:1m;
    limit_conn_dry_run on;

    server {
        listen 127.0.0.1:9601;
        proxy_pass 127.0.0.1:9701;
    }

    server {
        listen 127.0.0.1:9602;
        limit_conn addr 2;
        limit_conn_dry_run off;
        proxy_pass 127.0.0.1:9701;
    }
}
"#,
        )
        .expect("materialize stream limit_conn");
        assert_eq!(config.stream_limit_conn_zones.len(), 1);
        let zone = &config.stream_limit_conn_zones[0];
        assert_eq!(zone.name, "addr");
        assert_eq!(zone.key, "$binary_remote_addr");
        assert!(zone.max_keys > 0);
        // A server without rules inherits only the dry-run flag.
        let inherited = config
            .streams
            .iter()
            .find(|stream| stream.id.contains("9601"))
            .expect("inheriting stream");
        assert!(inherited.limit_conn.is_empty());
        assert!(inherited.limit_conn_dry_run);
        // A server declaration replaces the flag and adds the rules.
        let own = config
            .streams
            .iter()
            .find(|stream| stream.id.contains("9602"))
            .expect("own rules stream");
        assert_eq!(own.limit_conn.len(), 1);
        assert_eq!(own.limit_conn[0].zone, "addr");
        assert_eq!(own.limit_conn[0].max_connections, 2);
        assert!(!own.limit_conn_dry_run);
        // Fail-closed forms.
        for (label, body, needle) in [
            (
                "undefined zone",
                "stream { server { listen 127.0.0.1:9603; limit_conn missing 1; proxy_pass 127.0.0.1:9701; } }",
                "undefined zone",
            ),
            (
                "duplicate zone",
                "stream { limit_conn_zone $remote_addr zone=addr:1m; limit_conn_zone $remote_addr zone=addr:1m; }",
                "duplicate stream limit_conn_zone",
            ),
            (
                "unsupported key",
                "stream { limit_conn_zone $host zone=addr:1m; }",
                "invalid stream limit_conn_zone",
            ),
            (
                "zero connections",
                "stream { limit_conn_zone $remote_addr zone=addr:1m; server { listen 127.0.0.1:9604; limit_conn addr 0; proxy_pass 127.0.0.1:9701; } }",
                "invalid stream limit_conn",
            ),
        ] {
            let error = materialize(body)
                .err()
                .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(error.to_string().contains(needle), "`{label}`: {error}");
        }
    }

    #[test]
    fn materializes_stream_set_map_and_variable_return() {
        let config = materialize(
            r#"
stream {
    map $remote_addr $who {
        default "other";
        127.0.0.1 "loopback";
    }
    map $ssl_preread_server_name $backend {
        default "none";
        a.example "backend-a";
    }

    server {
        listen 127.0.0.1:9611;
        set $tag "tagged";
        return $tag;
    }

    server {
        listen 127.0.0.1:9612;
        set $greeting "hello-$who";
        return $greeting;
    }

    server {
        listen 127.0.0.1:9613;
        ssl_preread on;
        return $backend;
    }
}
"#,
        )
        .expect("materialize stream variables");
        assert_eq!(config.stream_maps.len(), 2);
        assert_eq!(config.stream_maps[0].source, "$remote_addr");
        assert_eq!(config.stream_maps[1].source, "$ssl_preread_server_name");
        let by_port = |port: &str| {
            config
                .streams
                .iter()
                .find(|stream| stream.id.contains(port))
                .expect("stream listener")
        };
        let set_return = by_port("9611");
        assert_eq!(set_return.set_variables.len(), 1);
        assert_eq!(set_return.set_variables[0].name, "tag");
        assert_eq!(set_return.return_value.as_deref(), Some("$tag"));
        let map_return = by_port("9612");
        assert_eq!(map_return.return_value.as_deref(), Some("$greeting"));
        assert_eq!(map_return.set_variables[0].value, "hello-$who");
        let preread = by_port("9613");
        assert_eq!(preread.return_value.as_deref(), Some("$backend"));
        // Fail-closed forms.
        for (label, body, needle) in [
            (
                "unknown map source",
                "stream { map $host $m { default \"x\"; } }",
                "not known at accept/preread time",
            ),
            (
                "unknown set variable reference",
                "stream { server { listen 127.0.0.1:9614; set $x $missing; return 200; } }",
                "does not know at accept/preread time",
            ),
            (
                "unknown return variable",
                "stream { server { listen 127.0.0.1:9615; return $nope; } }",
                "does not know at accept/preread time",
            ),
            (
                "map variable value",
                "stream { map $remote_addr $m { default \"$host\"; } }",
                "must be literal strings",
            ),
        ] {
            let error = materialize(body)
                .err()
                .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(error.to_string().contains(needle), "`{label}`: {error}");
        }
    }

    #[test]
    fn materializes_stream_preread_timeout_and_buffer() {
        let config = materialize(
            r#"
stream {
    preread_timeout 5s;
    preread_buffer_size 8k;

    server {
        listen 127.0.0.1:9621;
        ssl_preread on;
        proxy_pass 127.0.0.1:9721;
    }

    server {
        listen 127.0.0.1:9622;
        ssl_preread on;
        preread_timeout 2s;
        preread_buffer_size 4k;
        proxy_pass 127.0.0.1:9721;
    }
}
"#,
        )
        .expect("materialize preread settings");
        let by_port = |port: &str| {
            config
                .streams
                .iter()
                .find(|stream| stream.id.contains(port))
                .expect("stream listener")
        };
        let inherited = by_port("9621");
        assert_eq!(inherited.preread_timeout_ms, 5_000);
        assert_eq!(inherited.preread_buffer_bytes, 8 * 1024);
        let own = by_port("9622");
        assert_eq!(own.preread_timeout_ms, 2_000);
        assert_eq!(own.preread_buffer_bytes, 4 * 1024);
        // The fixed defaults apply when nothing is declared.
        let default_config =
            materialize("stream { server { listen 127.0.0.1:9623; proxy_pass 127.0.0.1:9721; } }")
                .expect("materialize defaults");
        assert_eq!(default_config.streams[0].preread_timeout_ms, 30_000);
        assert_eq!(default_config.streams[0].preread_buffer_bytes, 16 * 1024);
        // Fail-closed forms: inbound PROXY protocol timing has no runtime path,
        // and the buffer bound is enforced.
        for (label, body, needle) in [
            (
                "proxy_protocol_timeout",
                "stream { server { listen 127.0.0.1:9624; proxy_protocol_timeout 3s; proxy_pass 127.0.0.1:9721; } }",
                "does not parse",
            ),
            (
                "buffer too small",
                "stream { preread_buffer_size 100; }",
                "between 512 bytes and 1 MiB",
            ),
            (
                "buffer too large",
                "stream { preread_buffer_size 2m; }",
                "between 512 bytes and 1 MiB",
            ),
        ] {
            let error = materialize(body)
                .err()
                .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(error.to_string().contains(needle), "`{label}`: {error}");
        }
    }

    #[test]
    fn materializes_static_ocsp_stapling_file() {
        let directory = std::env::temp_dir().join(format!(
            "sdkwork-nginx-stapling-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("temp dir");
        let ocsp_path = directory.join("ocsp.der");
        // A minimal ASN.1 SEQUENCE: real OCSP parsing is the TLS stack's job;
        // the runtime validates the DER envelope and serves the bytes verbatim.
        std::fs::write(&ocsp_path, [0x30_u8, 0x03, 0x0a, 0x01, 0x00]).expect("write ocsp");
        let certificate_path = directory.join("server.pem");
        let key_path = directory.join("server.key");
        std::fs::write(&certificate_path, "placeholder").expect("cert");
        std::fs::write(&key_path, "placeholder").expect("key");
        let config = materialize(&format!(
            r#"
server {{
    listen 443 ssl;
    server_name stapling.example.test;
    ssl_certificate {};
    ssl_certificate_key {};
    ssl_stapling on;
    ssl_stapling_file {};
    location / {{ return 200 "ok"; }}
}}
"#,
            certificate_path.display(),
            key_path.display(),
            ocsp_path.display()
        ))
        .expect("materialize stapling");
        let certificate = config.certificates.first().expect("certificate");
        let crate::config::CertificateSource::ProtectedFile {
            ocsp_stapling_file, ..
        } = &certificate.source;
        assert_eq!(
            ocsp_stapling_file.as_deref(),
            Some(ocsp_path.to_string_lossy().as_ref())
        );
        // Fail-closed forms: a missing file and a non-DER payload are located
        // configuration errors, and the http context is rejected with a
        // pointer to the server block.
        let invalid = directory.join("invalid.der");
        std::fs::write(&invalid, b"not-der").expect("invalid");
        for (label, body, needle) in [
            (
                "missing file",
                "ssl_stapling_file /nonexistent/ocsp.der;".to_owned(),
                "cannot be read",
            ),
            (
                "not DER",
                format!("ssl_stapling_file {};", invalid.display()),
                "missing SEQUENCE tag",
            ),
        ] {
            let error = materialize(&format!(
                r#"
server {{
    listen 443 ssl;
    server_name stapling.example.test;
    ssl_certificate {};
    ssl_certificate_key {};
    {body}
    location / {{ return 200 "ok"; }}
}}
"#,
                certificate_path.display(),
                key_path.display()
            ))
            .err()
            .unwrap_or_else(|| panic!("`{label}` must fail closed"));
            assert!(error.to_string().contains(needle), "`{label}`: {error}");
        }
        let error = materialize(
            r#"
ssl_stapling_file /tmp/ocsp.der;
server { listen 80; server_name stapling.example.test; location / { return 200 "ok"; } }
"#,
        )
        .expect_err("http-level stapling must fail closed");
        assert!(
            error
                .to_string()
                .contains("declare it in the `server` block"),
            "{error}"
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn every_accepted_ignored_directive_materializes() {
        // Block-syntax directives get a minimal body; everything else takes
        // a representative argument list. Each entry is placed at the http
        // level where the accepted-ignored check runs.
        let blocks = ["map", "types", "charset_map", "events"];
        let args: &[(&str, &str)] = &[
            ("user", "nginx"),
            ("worker_processes", "auto"),
            ("worker_rlimit_nofile", "65535"),
            ("worker_connections", "1024"),
            ("pid", "/run/nginx.pid"),
            ("error_log", "/var/log/nginx/error.log warn"),
            ("access_log", "/var/log/nginx/access.log"),
            ("sendfile", "on"),
            ("tcp_nopush", "on"),
            ("tcp_nodelay", "on"),
            ("keepalive_timeout", "75"),
            ("keepalive_requests", "1000"),
            ("server_tokens", "off"),
            ("log_format", "main '$remote_addr $request'"),
            ("default_type", "application/octet-stream"),
            ("charset", "utf-8"),
            ("so_keepalive", "on"),
            ("resolver", "127.0.0.1"),
            ("resolver_timeout", "5s"),
            ("client_body_timeout", "60s"),
            ("client_header_timeout", "60s"),
            ("client_header_buffer_size", "1k"),
            ("large_client_header_buffers", "4 8k"),
            ("reset_timedout_connection", "on"),
            ("server_names_hash_max_size", "512"),
            ("proxy_http_version", "1.1"),
            ("proxy_buffering", "off"),
            ("proxy_request_buffering", "off"),
            ("proxy_method", "POST"),
            ("proxy_intercept_errors", "on"),
            ("proxy_next_upstream", "error timeout"),
            ("proxy_hide_header", "X-Foo"),
            ("proxy_redirect", "default"),
            ("proxy_connect_timeout", "5s"),
            ("proxy_read_timeout", "60s"),
            ("proxy_send_timeout", "60s"),
            ("proxy_buffer_size", "4k"),
            ("proxy_buffers", "8 4k"),
            ("ssl_prefer_server_ciphers", "on"),
            ("ssl_session_cache", "shared:SSL:10m"),
            ("ssl_session_timeout", "10m"),
            ("ssl_session_tickets", "on"),
            ("ssl_stapling", "on"),
            ("ssl_stapling_verify", "on"),
            ("ssl_trusted_certificate", "/etc/ssl/chain.pem"),
            ("ssl_ciphers", "HIGH:!aNULL"),
            ("ssl_verify_depth", "3"),
            ("ssl_dhparam", "/etc/ssl/dhparam.pem"),
            ("ssl_ecdh_curve", "auto"),
            ("http2", "on"),
            ("http2_max_field_size", "8k"),
            ("http2_max_header_size", "16k"),
            ("http2_max_requests", "1000"),
            ("http2_idle_timeout", "3m"),
            ("http2_recv_timeout", "3m"),
            ("http2_max_concurrent_pushes", "10"),
            ("http2_push", "/pushed.css"),
            ("http2_push_preload", "on"),
            ("http2_recv_buffer_size", "256k"),
            ("http2_body_preread_size", "64k"),
            ("http2_pool_size", "4k"),
            ("http2_streams_index_size", "32"),
            ("keepalive", "16"),
            ("client_body_buffer_size", "16k"),
            ("send_timeout", "60s"),
            ("sendfile_max_chunk", "1m"),
            ("fastcgi_read_timeout", "60s"),
            ("merge_slashes", "on"),
            ("gzip_comp_level", "6"),
            ("gzip_vary", "on"),
            ("gzip_proxied", "any"),
            ("gzip_disable", "msie6"),
            ("gzip_static", "on"),
            ("open_file_cache", "max=1000 inactive=20s"),
            ("open_file_cache_valid", "30s"),
            ("open_file_cache_min_uses", "2"),
            ("limit_conn_status", "429"),
            ("limit_conn_log_level", "warn"),
            ("limit_req_status", "429"),
            ("limit_req_log_level", "warn"),
            ("log_not_found", "off"),
            ("underscores_in_headers", "on"),
            ("ignore_invalid_headers", "on"),
            ("absolute_redirect", "off"),
            ("port_in_redirect", "off"),
            ("server_name_in_redirect", "off"),
            ("server_names_hash_bucket_size", "64"),
            ("types_hash_max_size", "2048"),
            ("types_hash_bucket_size", "64"),
            ("variables_hash_max_size", "1024"),
            ("variables_hash_bucket_size", "64"),
            ("map_hash_max_size", "2048"),
            ("map_hash_bucket_size", "64"),
            ("proxy_headers_hash_max_size", "512"),
            ("proxy_headers_hash_bucket_size", "64"),
            ("gzip_http_version", "1.1"),
            ("gzip_buffers", "16 8k"),
            ("gzip_window", "512k"),
            ("source_charset", "utf-8"),
            ("keepalive_disable", "msie6"),
            ("keepalive_time", "1h"),
            ("lingering_time", "30s"),
            ("lingering_timeout", "5s"),
            ("lingering_close", "off"),
            ("connection_pool_size", "320"),
            ("request_pool_size", "4k"),
            ("output_buffers", "2 32k"),
            ("postpone_output", "1460"),
            ("read_ahead", "0"),
            ("send_lowat", "0"),
            ("directio", "4m"),
            ("directio_alignment", "512"),
            ("log_subrequest", "on"),
            ("msie_padding", "off"),
            ("msie_refresh", "off"),
            ("chunked_transfer_encoding", "on"),
            ("max_ranges", "1"),
            ("recursive_error_pages", "on"),
            ("proxy_temp_path", "/var/cache/nginx/temp"),
            ("proxy_max_temp_file_size", "1g"),
            ("proxy_temp_file_write_size", "8k"),
            ("proxy_send_lowat", "0"),
            ("ssl_conf_command", "Options PrioritizeChaCha"),
            ("proxy_pass_header", "X-Bar"),
            ("accept_mutex_delay", "500ms"),
            ("charset_types", "text/html text/xml"),
            ("ssl_buffer_size", "4k"),
            ("proxy_ssl_protocols", "TLSv1.2 TLSv1.3"),
            ("proxy_ssl_ciphers", "HIGH"),
            ("proxy_ssl_session_reuse", "on"),
            ("proxy_ssl_verify_depth", "2"),
            ("daemon", "off"),
            ("master_process", "on"),
            ("env", "FOO"),
            ("pcre_jit", "on"),
            ("ssl_engine", "dynamic"),
            ("timer_resolution", "100ms"),
            ("lock_file", "/var/lock/nginx.lock"),
            ("worker_priority", "-5"),
            ("worker_cpu_affinity", "auto"),
            ("worker_shutdown_timeout", "10s"),
            ("worker_aio_requests", "32"),
            ("worker_rlimit_core", "1g"),
            ("working_directory", "/var/cache/nginx"),
            ("epoll_events", "512"),
            // Response-behavior knobs the runtime owns via its own defaults.
            ("error_page", "500 502 503 504 /50x.html"),
            ("autoindex", "off"),
            ("use", "epoll"),
            ("accept_mutex", "off"),
            ("multi_accept", "on"),
            ("disable_symlinks", "off"),
        ];
        let mut seen: Vec<&str> = Vec::new();
        for directive in ACCEPTED_IGNORED {
            if seen.contains(directive) {
                continue;
            }
            seen.push(directive);
            let statement = if blocks.contains(directive) {
                match *directive {
                    "map" => "map $http_user_agent $x { default 0; }".to_owned(),
                    "types" => "types { text/html html; }".to_owned(),
                    "charset_map" => "charset_map utf-8 iso-8859-1 { }".to_owned(),
                    "events" => "events { worker_connections 1024; }".to_owned(),
                    _ => unreachable!(),
                }
            } else {
                let sample = args
                    .iter()
                    .find(|(name, _)| name == directive)
                    .unwrap_or_else(|| panic!("missing sample args for `{directive}`"))
                    .1;
                format!("{directive} {sample};")
            };
            let text = format!(
                "http {{\n    {statement}\n    server {{\n        listen 80;\n        server_name ignored-{}.example.com;\n        location / {{ return 200 \"ok\"; }}\n    }}\n}}\n",
                directive.replace('_', "-")
            );
            let config = materialize(&text)
                .unwrap_or_else(|error| panic!("`{directive}` must materialize: {error}"));
            assert_eq!(config.virtual_hosts.len(), 1, "`{directive}`");
        }
        // Every entry in the list was exercised.
        assert_eq!(seen.len(), ACCEPTED_IGNORED.len());
    }

    #[test]
    fn adversarial_configs_never_panic() {
        // Malformed but never crashing: every input must produce a
        // parse error or a materialize error, never a panic.
        let cases = [
            "",
            " ",
            "\n",
            "# comment only\n",
            "server",
            "server {",
            "server { }",
            "}",
            "{",
            "};",
            "server { listen ; }",
            "server { listen 80; server_name; }",
            "server { listen 80; server_name x; location { } }",
            "server { listen 80; server_name x; location / { proxy_pass; } }",
            "server { listen 80; server_name x; location / { return; } }",
            "server { listen 80; server_name x; location / { rewrite; } }",
            "server { listen 80; server_name x; location / { gzip }",
            "server { listen 80; server_name x; location / { sub_filter; } }",
            "server { listen 80; server_name x; location / { secure_link; } }",
            "upstream { }",
            "upstream u { server; }",
            "stream { server { } }",
            "stream { server { listen; } }",
            "stream { server { listen 1; } }",
            "http { http { } }",
            "server { listen 80; server_name x; location / { try_files; } }",
            "server { listen 80; server_name x; location / { limit_req zone=; } }",
            "server { listen 80; server_name x; add_header; location / { return 200 \"ok\"; } }",
            "server { listen 80; server_name x; location / { proxy_set_header; } }",
            "server { listen 80; server_name x; location / { proxy_pass http://; } }",
            "server { listen 80; server_name x; location / { proxy_pass http://[::1; } }",
            "server { listen 80; server_name x; location / { return 99999; } }",
            "server { listen 80; server_name x; location / { return 200 \"unterminated; } }",
            "server { listen 80; server_name x; location / { set $x \"a\\\"; } }",
            "include ;",
            "include /nonexistent/*.conf;",
            "\u{feff}server { }",
            "server { listen 80; server_name x; location / { proxy_cache_valid; } }",
            "server { listen 80; server_name x; location / { limit_conn; } }",
            "server { listen 80; server_name x; location / { auth_basic_user_file; } }",
        ];
        for (index, case) in cases.iter().enumerate() {
            if let Ok(directives) = parse_nginx_config(case, Path::new("adversarial.conf")) {
                // Materialization may fail; it must not panic.
                let _ = materialize_nginx_app_default(
                    &directives,
                    Path::new("/etc/nginx/sites-enabled"),
                    "adversarial",
                );
            }
            let _ = index;
        }
    }

    #[test]
    fn deeply_nested_blocks_fail_closed_instead_of_overflowing() {
        let depth = 600;
        let text = format!(
            "{}server {{ return 200 \"ok\"; }}{}",
            "http { ".repeat(depth),
            "}".repeat(depth)
        );
        let error = parse_nginx_config(&text, Path::new("deep.conf"))
            .expect_err("excessive nesting must fail closed");
        assert!(error.to_string().contains("nesting exceeds"), "{error}");
        // Reasonable nesting still parses.
        let shallow = format!("{}server {{ }}{}", "http { ".repeat(64), "}".repeat(64));
        assert!(parse_nginx_config(&shallow, Path::new("shallow.conf")).is_ok());
    }

    #[test]
    fn invalid_client_max_body_size_fails_closed() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name body.example.com;
    client_max_body_size huge;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("invalid client_max_body_size must fail closed");
        assert!(
            error.to_string().contains("invalid client_max_body_size"),
            "{error}"
        );
    }

    #[test]
    fn unix_proxy_pass_and_bare_listen_ip_fail_or_default() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name unix.example.com;
    location / { proxy_pass http://unix:/tmp/sock; }
}
"#,
        )
        .expect_err("unix proxy_pass must fail closed");
        assert!(error.to_string().contains("unix:"), "{error}");
        let config = materialize(
            r#"
server {
    listen 127.0.0.1;
    server_name bare.example.com;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect("bare listen address defaults to port 80");
        assert!(config.listeners.iter().any(|listener| listener.port == 80));
    }

    #[test]
    fn map_declarations_materialize_and_expand_in_header_and_return_values() {
        let config = materialize(
            r#"
map $http_x_tenant $tenant_zone {
    default shared;
    "" anonymous;
    acme acme-zone;
    ~*^beta-[0-9]+ beta-zone;
}
map $host $host_role {
    hostnames;
    default generic;
    *.example.com wild;
}
server {
    listen 80;
    server_name map.example.com;
    add_header X-Tenant "$tenant_zone";
    location = /ret {
        return 302 https://edge.example.com/$tenant_zone;
    }
    location = /body {
        return 200 "zone=$tenant_zone";
    }
    location / {
        return 200 "ok";
    }
}
"#,
        )
        .expect("map declarations materialize");

        assert_eq!(config.maps.len(), 2);
        assert_eq!(config.maps[0].source, "$http_x_tenant");
        assert_eq!(config.maps[0].result, "$tenant_zone");
        assert!(!config.maps[0].hostnames);
        assert_eq!(config.maps[0].default.as_deref(), Some("shared"));
        assert_eq!(config.maps[0].entries.len(), 3);
        assert!(config.maps[1].hostnames);

        let host = &config.virtual_hosts[0];
        assert_eq!(
            host.security_headers
                .as_ref()
                .expect("server add_header array")
                .custom_headers[0]
                .value,
            "$tenant_zone"
        );
    }

    #[test]
    fn map_references_in_proxy_values_are_accepted_and_unknown_variables_rejected() {
        materialize(
            r#"
map $host $tenant_zone { default shared; }
server {
    listen 80;
    server_name map.example.com;
    location / {
        proxy_set_header X-Zone $tenant_zone;
        proxy_pass http://$tenant_zone;
    }
}
"#,
        )
        .expect("map result is expandable in proxy_set_header and proxy_pass");

        let error = materialize(
            r#"
map $host $tenant_zone { default shared; }
server {
    listen 80;
    server_name map.example.com;
    add_header X-Zone "$not_declared";
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("an undeclared variable still fails closed");
        assert!(error.to_string().contains("not_declared"), "{error}");
    }

    #[test]
    fn types_and_default_type_materialize_with_nginx_inheritance() {
        let config = materialize(
            r#"
types { application/x-http-txt txt; text/html html; }
default_type application/x-http-default;
server {
    listen 80;
    server_name mime.example.com;
    types { application/x-host-txt txt; }
    location /host/ { return 200 "ok"; }
    location /loc/ {
        types { application/x-loc-txt txt; }
        default_type application/x-loc-default;
        return 200 "ok";
    }
}
"#,
        )
        .expect("types materialize");

        let app_types = config.mime_types.as_ref().expect("http-level types table");
        assert_eq!(app_types.len(), 2);
        assert_eq!(app_types[0].mime, "application/x-http-txt");
        assert_eq!(app_types[0].extensions, vec!["txt".to_owned()]);
        assert_eq!(
            config.default_type.as_deref(),
            Some("application/x-http-default")
        );

        let host = &config.virtual_hosts[0];
        let host_types = host.mime_types.as_ref().expect("server-level types");
        assert_eq!(host_types[0].mime, "application/x-host-txt");
        // `default_type` is not declared at the server level, so it inherits.
        assert_eq!(host.default_type, None);

        let host_route = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/host/")
            .expect("host route");
        assert_eq!(host_route.mime_types, None);
        assert_eq!(host_route.default_type, None);

        let location_route = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/loc/")
            .expect("location route");
        assert_eq!(
            location_route.mime_types.as_ref().expect("location types")[0].mime,
            "application/x-loc-txt"
        );
        assert_eq!(
            location_route.default_type.as_deref(),
            Some("application/x-loc-default")
        );
    }

    #[test]
    fn types_and_default_type_reject_malformed_declarations() {
        let bad_extension = r#"
types { application/x-txt .txt; }
server { listen 80; server_name ext.example.com; location / { return 200 "ok"; } }
"#;
        let error = materialize(bad_extension).expect_err("leading dot extension must fail closed");
        assert!(error.to_string().contains("invalid extension"), "{error}");

        let bad_mime = r#"
types { notamimetype txt; }
server { listen 80; server_name mime.example.com; location / { return 200 "ok"; } }
"#;
        let error = materialize(bad_mime).expect_err("invalid mime must fail closed");
        assert!(error.to_string().contains("MIME type"), "{error}");

        let no_extension = r#"
types { application/x-txt; }
server { listen 80; server_name noext.example.com; location / { return 200 "ok"; } }
"#;
        let error =
            materialize(no_extension).expect_err("types row without extension must fail closed");
        assert!(error.to_string().contains("file extension"), "{error}");
    }

    #[test]
    fn gzip_vary_and_gzip_static_materialize_with_inheritance() {
        let config = materialize(
            r#"
server {
    listen 80;
    server_name plain.example.com;
    location / { return 200 "ok"; }
}
server {
    listen 80;
    server_name gz.example.com;
    gzip on;
    gzip_types text/plain;
    gzip_static on;
    location /st/ { gzip_vary on; return 200 "ok"; }
    location /always/ {
        gzip_static always;
        gzip_vary off;
        return 200 "ok";
    }
}
"#,
        )
        .expect("gzip policy materializes");

        // Hosts that declare no gzip directive keep the app-level policy.
        let plain = &config.virtual_hosts[0];
        assert!(plain.compression.is_none());

        let gz = &config.virtual_hosts[1];
        let host = gz.compression.as_ref().expect("server gzip policy");
        assert!(host.enabled);
        assert!(!host.vary, "gzip_vary is off by default in the nginx plane");
        assert_eq!(
            host.static_files,
            crate::config::GzipStaticMode::On,
            "server-level gzip_static"
        );

        let vary_route = gz
            .routes
            .iter()
            .find(|route| route.route_match.path == "/st/")
            .expect("location gzip_vary route");
        let vary = vary_route.compression.as_ref().expect("route policy");
        assert!(vary.vary);
        assert_eq!(vary.static_files, crate::config::GzipStaticMode::On);

        let always_route = gz
            .routes
            .iter()
            .find(|route| route.route_match.path == "/always/")
            .expect("location gzip_static route");
        let always = always_route.compression.as_ref().expect("route policy");
        assert!(!always.vary);
        assert_eq!(always.static_files, crate::config::GzipStaticMode::Always);
    }

    #[test]
    fn gzip_static_rejects_unknown_modes() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name gz.example.com;
    gzip_static sometimes;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("unknown gzip_static mode must fail closed");
        assert!(error.to_string().contains("gzip_static"), "{error}");
    }

    #[test]
    fn stream_map_executes_with_bounded_sources_and_no_diagnostic() {
        let text = r#"
http {
    map $host $zone { default http-zone; }
    server {
        listen 80;
        server_name map.example.com;
        location / { return 200 "ok"; }
    }
}
stream {
    map $remote_addr $stream_zone { default stream-zone; }
    server {
        listen 5100;
        proxy_pass 127.0.0.1:15100;
    }
}
"#;
        let parsed =
            super::super::parser::parse_nginx_config(text, std::path::Path::new("site.conf"))
                .expect("parse");
        let app = materialize_nginx_app_default(
            &parsed,
            std::path::Path::new("/etc/nginx"),
            "stream-map",
        )
        .expect("both map planes are executable");
        assert_eq!(app.maps.len(), 1, "the http map uses the HTTP map engine");
        assert_eq!(app.maps[0].result, "$zone");
        assert_eq!(
            app.stream_maps.len(),
            1,
            "the stream map uses the bounded stream variable engine"
        );
        assert_eq!(app.stream_maps[0].result, "$stream_zone");

        let diagnostics = ignored_directive_diagnostics(&parsed);
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("`map`")),
            "an executed map must not carry an accepted-unenforced diagnostic: {diagnostics:?}"
        );
    }

    #[test]
    fn map_validation_failures_are_located_and_precise() {
        let duplicate = r#"
map $host $zone { default a; }
map $uri $zone { default b; }
server { listen 80; server_name dup.example.com; location / { return 200 "ok"; } }
"#;
        // nginx map variables are changeable: the later declaration replaces
        // the earlier one instead of failing the whole configuration.
        let config = materialize(duplicate).expect("duplicate map result is last-wins");
        assert_eq!(config.maps.len(), 1);
        assert_eq!(config.maps[0].default.as_deref(), Some("b"));

        let unsupported_source = r#"
map $upstream_addr $zone { default a; }
server { listen 80; server_name src.example.com; location / { return 200 "ok"; } }
"#;
        let error =
            materialize(unsupported_source).expect_err("unsupported map source must fail closed");
        assert!(error.to_string().contains("not supported"), "{error}");

        let volatile = r#"
map $host $zone { volatile; default a; }
server { listen 80; server_name vol.example.com; location / { return 200 "ok"; } }
"#;
        let error = materialize(volatile).expect_err("volatile map must fail closed");
        assert!(error.to_string().contains("volatile"), "{error}");

        let conflicting_key = r#"
map $host $zone { a A; A B; }
server { listen 80; server_name key.example.com; location / { return 200 "ok"; } }
"#;
        let error = materialize(conflicting_key)
            .expect_err("case-insensitively duplicate keys must fail closed");
        assert!(error.to_string().contains("duplicate key"), "{error}");
    }

    #[test]
    fn materializes_fastcgi_location_with_inherited_params_and_split() {
        let config = materialize(
            r#"
fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
fastcgi_param REQUEST_METHOD $request_method;
fastcgi_read_timeout 30s;
http {
    server {
        listen 80;
        server_name fcgi.example.com;
        root /srv/www;
        location /app/ {
            fastcgi_pass 127.0.0.1:9000;
            fastcgi_index index.php;
            fastcgi_split_path_info ^(/app/[^/]+\.php)(/.*)$;
            fastcgi_keep_conn on;
            fastcgi_hide_header X-Mock-Hide;
            fastcgi_pass_header X-Mock-Pass;
            fastcgi_intercept_errors on;
        }
    }
}
"#,
        );
        // The `fastcgi_param` directives must be inside `http {}`.
        assert!(config.is_err(), "params outside http must fail closed");

        let config = materialize(
            r#"
http {
    fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
    fastcgi_param REQUEST_METHOD $request_method;
    fastcgi_read_timeout 30s;
    server {
        listen 80;
        server_name fcgi.example.com;
        root /srv/www;
        location /app/ {
            fastcgi_pass 127.0.0.1:9000;
            fastcgi_index index.php;
            fastcgi_split_path_info ^(/app/[^/]+\.php)(/.*)$;
            fastcgi_keep_conn on;
            fastcgi_hide_header X-Mock-Hide;
            fastcgi_pass_header X-Mock-Pass;
            fastcgi_intercept_errors on;
        }
    }
}
"#,
        )
        .expect("materialize");
        let resource = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::FastCgi {
                    upstream_ref,
                    params,
                    index,
                    split_path_info,
                    document_root,
                    keep_conn,
                    read_timeout_ms,
                    intercept_errors,
                    response_policy,
                    retry,
                    ..
                } => Some((
                    upstream_ref.clone(),
                    params.clone(),
                    index.clone(),
                    split_path_info.clone(),
                    document_root.clone(),
                    *keep_conn,
                    *read_timeout_ms,
                    *intercept_errors,
                    response_policy.clone(),
                    retry.clone(),
                )),
                _ => None,
            })
            .expect("fastcgi resource");
        assert_eq!(resource.0, "literal-127-0-0-1-9000");
        assert_eq!(resource.1.len(), 2);
        assert_eq!(resource.1[0].name, "SCRIPT_FILENAME");
        assert_eq!(resource.1[0].value, "$document_root$fastcgi_script_name");
        assert_eq!(resource.2.as_deref(), Some("index.php"));
        assert_eq!(resource.3.as_deref(), Some(r"^(/app/[^/]+\.php)(/.*)$"));
        assert_eq!(resource.4.as_deref(), Some("/srv/www"));
        assert!(resource.5);
        assert_eq!(resource.6, 30_000);
        assert!(resource.7);
        let policy = resource.8.expect("response policy");
        assert_eq!(policy.hide_headers, ["X-Mock-Hide"]);
        assert_eq!(policy.pass_headers, ["X-Mock-Pass"]);
        let retry = resource.9.expect("retry policy");
        assert_eq!(retry.max_attempts, 1);
        assert_eq!(config.upstreams[0].targets[0].url, "http://127.0.0.1:9000");
    }

    #[test]
    fn fastcgi_unsupported_directives_fail_closed_with_located_diagnostics() {
        for (directive, needle) in [
            ("fastcgi_cache off;", "fastcgi_cache"),
            ("fastcgi_pass unix:/run/php.sock;", "unix:"),
            ("fastcgi_param BAD $upstream_addr;", "$upstream_addr"),
            ("fastcgi_next_upstream http_500;", "http_500"),
            (
                "fastcgi_split_path_info ^(/x[.]php)$;",
                "two capture groups",
            ),
        ] {
            let text = format!(
                "http {{ server {{ listen 80; server_name a.test; location / {{ {directive} }} }} }}"
            );
            let error = materialize(&text).expect_err("must fail closed");
            assert!(
                error.to_string().contains(needle),
                "expected `{needle}` in: {error}"
            );
        }
    }

    #[test]
    fn materializes_uwsgi_and_scgi_locations_with_shared_param_pipeline() {
        let config = materialize(
            r#"
http {
    uwsgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
    uwsgi_param REQUEST_METHOD $request_method;
    uwsgi_modifier1 0;
    uwsgi_string "extra=1";
    scgi_param REQUEST_METHOD $request_method;
    server {
        listen 80;
        server_name cgi.example.com;
        root /srv/www;
        location /uwsgi/ {
            uwsgi_pass 127.0.0.1:9100;
            uwsgi_hide_header X-Mock-Hide;
            uwsgi_intercept_errors on;
        }
        location /scgi/ {
            scgi_pass 127.0.0.1:9200;
            scgi_read_timeout 15s;
        }
    }
}
"#,
        )
        .expect("materialize");
        let resources: Vec<_> = config
            .resources
            .iter()
            .filter_map(|resource| match resource {
                crate::config::ResourceConfig::CgiUpstream {
                    protocol,
                    upstream_ref,
                    params,
                    read_timeout_ms,
                    intercept_errors,
                    modifier1,
                    extra_string,
                    ..
                } => Some((
                    *protocol,
                    upstream_ref.clone(),
                    params.clone(),
                    *read_timeout_ms,
                    *intercept_errors,
                    *modifier1,
                    extra_string.clone(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(resources.len(), 2);
        let uwsgi = resources
            .iter()
            .find(|resource| resource.0 == crate::config::CgiUpstreamProtocol::Uwsgi)
            .expect("uwsgi resource");
        assert_eq!(uwsgi.1, "literal-127-0-0-1-9100");
        assert_eq!(uwsgi.2.len(), 2);
        assert_eq!(uwsgi.3, 60_000);
        assert!(uwsgi.4);
        assert_eq!(uwsgi.5, Some(0));
        assert_eq!(uwsgi.6.as_deref(), Some("extra=1"));
        let scgi = resources
            .iter()
            .find(|resource| resource.0 == crate::config::CgiUpstreamProtocol::Scgi)
            .expect("scgi resource");
        assert_eq!(scgi.1, "literal-127-0-0-1-9200");
        assert_eq!(scgi.2.len(), 1);
        assert_eq!(scgi.3, 15_000);
    }

    #[test]
    fn uwsgi_and_scgi_unsupported_directives_fail_closed() {
        for (directive, needle) in [
            ("uwsgi_cache off;", "uwsgi_cache"),
            ("uwsgi_pass unix:/run/uwsgi.sock;", "unix:"),
            ("uwsgi_ssl_certificate /tmp/c.pem;", "uwsgi_ssl_certificate"),
            ("scgi_param BAD $upstream_addr;", "$upstream_addr"),
        ] {
            let text = format!(
                "http {{ server {{ listen 80; server_name a.test; location / {{ {directive} }} }} }}"
            );
            let error = materialize(&text).expect_err("must fail closed");
            assert!(
                error.to_string().contains(needle),
                "expected `{needle}` in: {error}"
            );
        }
    }

    #[test]
    fn materializes_memcached_location_with_set_key_template() {
        let config = materialize(
            r#"
http {
    server {
        listen 80;
        server_name mc.example.com;
        location /mc/ {
            set $memcached_key $uri;
            memcached_pass 127.0.0.1:11211;
            memcached_read_timeout 10s;
        }
    }
}
"#,
        )
        .expect("materialize");
        let resource = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Memcached {
                    upstream_ref,
                    key_template,
                    read_timeout_ms,
                    ..
                } => Some((upstream_ref.clone(), key_template.clone(), *read_timeout_ms)),
                _ => None,
            })
            .expect("memcached resource");
        assert_eq!(resource.0, "literal-127-0-0-1-11211");
        assert_eq!(resource.1, "$uri");
        assert_eq!(resource.2, 10_000);
    }

    #[test]
    fn memcached_set_and_unsupported_directives_fail_closed() {
        for (text, needle) in [
            (
                "http { server { listen 80; server_name a.test; location / { memcached_pass 127.0.0.1:11211; } } }",
                "set $memcached_key",
            ),
            (
                "http { server { listen 80; server_name a.test; location / { set $memcached_key $uri; memcached_pass unix:/run/mc.sock; } } }",
                "unix:",
            ),
            (
                "http { server { listen 80; server_name a.test; location / { set $memcached_key $uri; memcached_cache off; memcached_pass 127.0.0.1:11211; } } }",
                "memcached_cache",
            ),
        ] {
            let error = materialize(text).expect_err("must fail closed");
            assert!(
                error.to_string().contains(needle),
                "expected `{needle}` in: {error}"
            );
        }
    }

    fn materialize_with_allowlist(
        text: &str,
        allowlist: &[&str],
    ) -> Result<WebServerAppConfig, NginxConfigError> {
        let parsed = parse_nginx_config(text, std::path::Path::new("site.conf")).expect("parse");
        materialize_nginx_app(
            &parsed,
            std::path::Path::new("/etc/nginx/sites-enabled"),
            "test",
            &NginxCompatOptions {
                document_root_allowlist: allowlist
                    .iter()
                    .map(|entry| (*entry).to_owned())
                    .collect(),
            },
        )
    }

    #[test]
    fn rewrite_phase_set_and_if_materialize_in_order() {
        let config = materialize(
            r#"
server {
    listen 8080;
    server_name api.example.com;
    set $env prod;
    if ($host ~* "^([a-z]+)\\.example\\.com$") {
        set $env $1;
    }
    location / {
        set $target "http://api_servers_${env}";
        if ($env = prod) {
            add_header X-Env "prod";
        }
        proxy_pass $target$request_uri;
    }
}
"#,
        )
        .expect("rewrite phase materializes");
        let host = &config.virtual_hosts[0];
        assert_eq!(host.rewrite_phase.len(), 2);
        assert!(matches!(
            &host.rewrite_phase[0],
            RewritePhaseOpConfig::Set { variable, value }
                if variable == "env" && value == "prod"
        ));
        assert!(matches!(
            &host.rewrite_phase[1],
            RewritePhaseOpConfig::If { body, .. } if body.len() == 1
        ));
        let route = &host.routes[0];
        assert_eq!(route.rewrite_phase.len(), 2);
        assert!(matches!(
            &route.rewrite_phase[0],
            RewritePhaseOpConfig::Set { variable, .. } if variable == "target"
        ));
        let metadata = serde_json::to_value(&route.resource_ref);
        let _ = metadata;
        let proxy = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                ResourceConfig::Proxy { dynamic_target, .. } => dynamic_target.as_deref(),
                _ => None,
            })
            .expect("dynamic proxy target");
        assert_eq!(proxy, "$target$request_uri");
    }

    #[test]
    fn rewrite_phase_references_to_unknown_variables_fail_closed() {
        let error = materialize(
            r#"
server {
    listen 8080;
    server_name api.example.com;
    set $env $undefined_variable;
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("unknown variable must fail closed");
        assert!(error.to_string().contains("is not resolvable"), "{error}");
    }

    #[test]
    fn if_body_directives_outside_the_bounded_set_fail_closed() {
        let error = materialize(
            r#"
server {
    listen 8080;
    server_name api.example.com;
    if ($host = api.example.com) {
        proxy_pass http://127.0.0.1:9000;
    }
    location / { return 200 "ok"; }
}
"#,
        )
        .expect_err("if-body proxy_pass must fail closed");
        assert!(
            error
                .to_string()
                .contains("if` body directive `proxy_pass`"),
            "{error}"
        );
    }

    #[test]
    fn variable_document_root_requires_the_operator_allowlist() {
        let text = r#"
server {
    listen 8080;
    server_name api.example.com;
    location / {
        set $root_path /srv/www;
        root $root_path;
    }
}
"#;
        let error = materialize(text).expect_err("without allowlist root must fail closed");
        assert!(
            error.to_string().contains("documentRootAllowlist"),
            "{error}"
        );
        let config =
            materialize_with_allowlist(text, &["/srv/www"]).expect("allowlisted root materializes");
        let route = &config.virtual_hosts[0].routes[0];
        let resource = config
            .resources
            .iter()
            .find(|resource| resource.id() == route.resource_ref)
            .expect("resource");
        match resource {
            ResourceConfig::Static { dynamic_root, .. } => {
                assert_eq!(dynamic_root.as_deref(), Some("$root_path"));
            }
            other => panic!("expected static resource, got {other:?}"),
        }
    }

    #[test]
    fn rewrite_phase_limits_fail_closed() {
        let mut text =
            String::from("server {\n    listen 8080;\n    server_name api.example.com;\n");
        for _ in 0..=crate::config::MAX_REWRITE_PHASE_OPS {
            text.push_str("    set $var \"value\";\n");
        }
        text.push_str("    location / { return 200 \"ok\"; }\n}\n");
        let error = materialize(&text).expect_err("oversized rewrite phase must fail closed");
        assert!(
            error.to_string().contains("rewrite-phase instructions"),
            "{error}"
        );
    }
}
