//! Materialize parsed nginx `http`- and `stream`-context configuration into
//! the runtime `WebServerAppConfig` model.
//!
//! Supported directives (everything else fails closed with a diagnostic):
//!
//! - `upstream { server <addr> [weight=…] [max_fails=…] [fail_timeout=…]; keepalive …; }`
//! - `server { listen …; server_name …; ssl_certificate(_key) …; location …; }`
//! - `location <match> { proxy_pass http(s)://<upstream|host:port>[/uri]; … }`
//! - `location <match> { return <code> <url-with-$host/$request_uri/$scheme>; }`
//! - `location <match> { root <absolute>; try_files $uri $uri/ /index.html; }`
//! - `location <match> { alias <absolute-dir>; }` (nginx prefix replacement)
//! - `location` `rewrite`, `allow`/`deny`, `limit_req`, `auth_basic` +
//!   `auth_basic_user_file` (htpasswd loaded at materialize)
//! - http `limit_req_zone`, `gzip` / `gzip_types` / `gzip_min_length`
//! - http `proxy_cache_path` / `proxy_cache` / `proxy_cache_valid`
//! - http/server/location `proxy_set_header` (validated supported `$vars`)
//! - `stream { upstream …; server { listen; proxy_pass; ssl_preread; … } }`
//!
//! Safe nginx tuning directives are accepted and ignored (the runtime owns
//! timeouts, buffering, and TLS defaults). Directives the runtime cannot
//! enforce — `sub_filter`, `limit_conn`, variable `proxy_pass` — are rejected.

use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};
use thiserror::Error;

use crate::config::{
    format_proxy_set_header_entry, hostname_upstream_allowed_cidrs, merge_proxy_set_headers,
    parse_htpasswd, parse_limit_conn, parse_limit_conn_zone, parse_limit_req, parse_limit_req_zone,
    ConfigDiagnostic, StreamTargetConfig, StreamTlsMode, WebServerAppConfig, WebServerConfigError,
};

use super::parser::NginxDirective;

const ACCEPTED_IGNORED: &[&str] = &[
    // process / http tuning (gzip / limit_req_zone / proxy_cache* are handled explicitly)
    "user", "worker_processes", "worker_rlimit_nofile", "worker_connections", "pid",
    "error_log", "access_log", "sendfile", "tcp_nopush", "tcp_nodelay",
    "keepalive_timeout", "keepalive_requests", "server_tokens",
    "map", "log_format", "types", "default_type", "charset", "events", "so_keepalive",
    "resolver", "resolver_timeout", "client_body_timeout", "client_header_timeout",
    "client_header_buffer_size",
    "large_client_header_buffers", "reset_timedout_connection", "server_names_hash_max_size",
    "proxy_http_version", "proxy_buffering", "proxy_request_buffering", "proxy_method",
    "proxy_intercept_errors", "proxy_next_upstream", "proxy_hide_header", "proxy_redirect",
    "proxy_connect_timeout", "proxy_read_timeout", "proxy_send_timeout", "proxy_buffer_size",
    "proxy_buffers", "ssl_protocols", "ssl_prefer_server_ciphers", "ssl_session_cache",
    "ssl_session_timeout", "ssl_session_tickets", "ssl_stapling", "ssl_stapling_verify",
    "ssl_trusted_certificate", "ssl_ciphers", "ssl_verify_depth", "ssl_dhparam",
    "ssl_ecdh_curve", "http2", "keepalive",
    "client_body_buffer_size", "send_timeout", "sendfile_max_chunk", "fastcgi_read_timeout",
    "merge_slashes",
    "gzip_comp_level", "gzip_vary", "gzip_proxied", "gzip_disable", "gzip_static",
    "open_file_cache", "open_file_cache_valid", "open_file_cache_min_uses",
    "limit_conn_status", "limit_conn_log_level", "limit_req_status", "limit_req_log_level", "log_not_found",
    "underscores_in_headers", "ignore_invalid_headers", "absolute_redirect",
    "port_in_redirect", "server_name_in_redirect",
    // Stock nginx.conf / distro hash-table and buffering tuning; the runtime
    // owns its hash tables and I/O buffering.
    "server_names_hash_bucket_size", "types_hash_max_size", "types_hash_bucket_size",
    "variables_hash_max_size", "variables_hash_bucket_size",
    "map_hash_max_size", "map_hash_bucket_size",
    "proxy_headers_hash_max_size", "proxy_headers_hash_bucket_size",
    "gzip_http_version", "gzip_buffers", "gzip_window",
    "charset_map", "source_charset",
    "keepalive_disable", "keepalive_time",
    "lingering_time", "lingering_timeout", "lingering_close",
    "connection_pool_size", "request_pool_size",
    "output_buffers", "postpone_output", "read_ahead", "send_lowat",
    "directio", "directio_alignment",
    "log_subrequest", "msie_padding", "msie_refresh",
    "chunked_transfer_encoding", "max_ranges", "recursive_error_pages",
    "proxy_temp_path", "proxy_max_temp_file_size", "proxy_temp_file_write_size",
    "proxy_send_lowat", "ssl_conf_command", "proxy_pass_header",
    "accept_mutex_delay", "charset_types", "ssl_buffer_size",
    "proxy_ssl_protocols", "proxy_ssl_ciphers", "proxy_ssl_session_reuse",
    "proxy_ssl_verify_depth",
    // Main-context process/OS knobs nginx accepts at the top of nginx.conf;
    // the runtime owns process management, so they are safe to accept.
    "daemon", "master_process", "env", "pcre_jit", "ssl_engine", "timer_resolution",
    "lock_file", "worker_priority", "worker_cpu_affinity", "worker_shutdown_timeout",
    "worker_aio_requests", "worker_rlimit_core", "working_directory", "epoll_events",
    // Response-behavior knobs the runtime owns via its own defaults (error
    // page mapping, directory autoindex, cache/entity headers). Accepted and
    // ignored like the safe tuning directives above; the conformance corpus
    // (config-source-fixtures/nginx/full-nginx.conf) exercises `autoindex on`
    // as part of the accepted surface, and the TOML spec treats autoindex as
    // an operator policy knob (§11.2).
    "error_page", "autoindex", "expires", "etag", "if_modified_since",
    // events / OS tuning
    "use", "accept_mutex", "multi_accept", "disable_symlinks",
];

const UNSUPPORTED_SECURITY: &[&str] = &[];

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
pub fn materialize_nginx_app(
    directives: &[NginxDirective],
    base_dir: &Path,
    app_key: &str,
) -> Result<WebServerAppConfig, NginxConfigError> {
    let http_directives = extract_http_context(directives)?;
    let stream_directives = extract_stream_context(directives)?;
    let mut mapper = Mapper::new(app_key, base_dir);
    for (server_index, directive) in http_directives.iter().enumerate() {
        match directive.name.as_str() {
            "upstream" => mapper.materialize_upstream(directive)?,
            "server" => mapper.materialize_server(directive, server_index)?,
            "limit_req_zone" => mapper.materialize_limit_req_zone(directive)?,
            "limit_conn_zone" => mapper.materialize_limit_conn_zone(directive)?,
            "gzip" => mapper.materialize_gzip(directive)?,
            "gzip_types" => mapper.materialize_gzip_types(directive)?,
            "gzip_min_length" => mapper.materialize_gzip_min_length(directive)?,
            "proxy_cache_path" => mapper.materialize_proxy_cache_path(directive)?,
            "proxy_cache_valid" => mapper.apply_proxy_cache_valid(directive)?,
            "proxy_cache" => mapper.materialize_proxy_cache(directive)?,
            "proxy_set_header" => {
                let entry = format_proxy_set_header_entry(&directive.args).map_err(|message| {
                    NginxConfigError::unsupported(directive, message)
                })?;
                mapper.http_proxy_set_headers.push(entry);
            }
            "client_max_body_size" => {
                mapper.note_client_max_body_size(parse_body_size(directive)?);
            }
            "proxy_ssl_verify" | "proxy_ssl_trusted_certificate"
            | "proxy_ssl_certificate" | "proxy_ssl_certificate_key"
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
            "set_real_ip_from" | "real_ip_header" | "real_ip_recursive" => {
                parse_real_ip_directive(&mut mapper.http_real_ip, directive)?;
            }
            "stream" | "events" | "http" => {}
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
    if mapper.virtual_hosts.is_empty() && mapper.streams.is_empty() {
        return Err(NginxConfigError::unsupported(
            &NginxDirective {
                name: "server".to_owned(),
                args: Vec::new(),
                children: Vec::new(),
                line: 0,
                source: base_dir.to_path_buf(),
            },
            "no `server` or `stream` blocks were materialized",
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
    // runtime does not consume (mail, load_module, ...): silently dropping
    // them would hide configuration that is not executed.
    for directive in directives {
        if directive.name == "http" || directive.name == "stream" {
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
    /// http-level `proxy_set_header` entries inherited by every server
    /// (nginx http context inheritance).
    http_proxy_set_headers: Vec<String>,
    /// http-level `proxy_ssl_*` upstream TLS settings inherited by every
    /// server and location.
    http_proxy_ssl: ProxySslSettings,
    /// http-level `http2 on|off` inherited by every server's ssl listeners
    /// (nginx 1.25.1+ `http2` directive).
    http_http2: Option<bool>,
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
    limit_req_zones: Vec<Value>,
    limit_req_zone_names: Vec<String>,
    limit_conn_zones: Vec<Value>,
    limit_conn_zone_names: Vec<String>,
    /// Largest `client_max_body_size` seen while materializing nginx config.
    client_max_body_size: Option<u64>,
    /// Set when different `client_max_body_size` values are declared at
    /// different levels; the runtime model owns one global body limit.
    client_max_body_size_conflict: bool,
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
            http_proxy_set_headers: Vec::new(),
            http_proxy_ssl: ProxySslSettings::default(),
            http_http2: None,
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
            limit_req_zones: Vec::new(),
            limit_req_zone_names: Vec::new(),
            limit_conn_zones: Vec::new(),
            limit_conn_zone_names: Vec::new(),
            client_max_body_size: None,
            client_max_body_size_conflict: false,
        }
    }

    fn note_client_max_body_size(&mut self, bytes: Option<u64>) {
        if let Some(bytes) = bytes {
            match self.client_max_body_size {
                Some(existing) if existing != bytes => self.client_max_body_size_conflict = true,
                None => self.client_max_body_size = Some(bytes),
                _ => {}
            }
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
        let name = directive
            .args
            .first()
            .cloned()
            .ok_or_else(|| NginxConfigError::unsupported(directive, "upstream requires a name"))?;
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
                            "unix: upstream sockets are not supported by the runtime model",
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
                    let unsupported = child.args.iter().any(|argument| {
                        argument != "two" && argument != "least_conn"
                    });
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

        for child in &directive.children {
            match child.name.as_str() {
                "listen" => {
                    if child.args.first().is_none() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "listen requires an address",
                        ));
                    }
                    let spec = parse_listen_spec(&child.args.join(" ")).map_err(|message| {
                        NginxConfigError::unsupported(child, message)
                    })?;
                    listen_specs.push(spec);
                }
                "server_name" => {
                    server_names.extend(child.args.iter().cloned());
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
                                format!("ssl_verify_client accepts on|optional|off, found `{other}`"),
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
                    let Some(_root_path) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "root requires a directory",
                        ));
                    };
                    server_root = Some(self.resolve_path(child)?);
                }
                "try_files" => {
                    validate_try_files(child)?;
                    server_try_files = child.args.clone();
                }
                "index" => {
                    server_index_files = child.args.clone();
                }
                // `add_header Name value [always];` -> security headers when
                // the name matches a dedicated field, otherwise custom.
                "add_header" => {
                    if child.args.len() >= 2 {
                        add_headers.push(parse_add_header(&child.args[0], &child.args[1]));
                    }
                }
                "client_max_body_size" => {
                    client_max_body_size = parse_body_size(child)?;
                }
                "proxy_set_header" => {
                    let entry = format_proxy_set_header_entry(&child.args).map_err(|message| {
                        NginxConfigError::unsupported(child, message)
                    })?;
                    inherited_proxy_set_headers.push(entry);
                }
                "access_log" | "error_log" | "proxy_http_version"
                | "proxy_buffering" | "proxy_read_timeout" | "proxy_send_timeout"
                | "proxy_connect_timeout" | "ssl_protocols" | "ssl_prefer_server_ciphers"
                | "ssl_session_cache" | "ssl_trusted_certificate"
                | "client_body_timeout" | "client_header_timeout" => {}
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
                "proxy_ssl_verify" | "proxy_ssl_trusted_certificate"
                | "proxy_ssl_certificate" | "proxy_ssl_certificate_key"
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
                        format!("unsupported server directive `{name}`"),
                    ));
                }
            }
        }

        validate_server_names(directive, &server_names)?;
        let primary_name = server_names[0].clone();
        // nginx inheritance: server-level `http2` overrides the http level.
        let http2_on = server_http2.or(self.http_http2).unwrap_or(false);
        let first_port = listen_specs
            .first()
            .map(|spec| spec.port)
            .unwrap_or(0);
        // The virtual host id is the default-server target on every listener
        // this server owns (nginx: the first server for a listen address is
        // its default, overridden by an explicit `default_server`).
        let virtual_host_id = format!("{}-{first_port}", sanitize_id(&primary_name));

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
            self.certificates.push(json!({
                "id": certificate_name,
                "serverNames": server_names,
                "source": {
                    "type": "protected-file",
                    "certificateFile": certificate_file,
                    "privateKeyFile": certificate_key,
                },
            }));
            let mut tls_policy = json!({
                "id": format!("tls-{certificate_name}"),
                "certificateRefs": [certificate_name],
                "minimumVersion": "tls1.2",
                "maximumVersion": "tls1.3",
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

        // http-level `proxy_set_header` entries are inherited by every
        // server; server entries override http entries on the same name.
        let inherited_proxy_set_headers =
            merge_proxy_set_headers(&self.http_proxy_set_headers, &inherited_proxy_set_headers);
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
                        self.merge_certificate_into_shared_listener(
                            &existing,
                            cert_name,
                        )?;
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
                        self.explicit_defaults.insert(key.clone(), virtual_host_id.clone());
                        if let Some(listener) = self
                            .listeners
                            .iter_mut()
                            .find(|listener| listener.get("id").and_then(Value::as_str) == Some(existing.as_str()))
                        {
                            listener["defaultVirtualHostRef"] = Value::String(virtual_host_id.clone());
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
                    self.explicit_defaults.insert(key.clone(), virtual_host_id.clone());
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
            if !listener_refs.contains(&listener_id) {
                listener_refs.push(listener_id.clone());
            }
            self.apply_real_ip_to_listener(&listener_id);
        }

        let mut named_pc_root = None;
        let mut named_h5_root = None;
        for named in named_locations {
            let (name, root) = named_adaptive_location_root(named)?;
            match name.as_str() {
                "@pc" => named_pc_root = Some(root),
                "@h5" => named_h5_root = Some(root),
                _ => {
                    return Err(NginxConfigError::unsupported(
                        named,
                        format!(
                            "named location `{name}` is not supported; only Adaptive Web @pc and @h5 are accepted"
                        ),
                    ));
                }
            }
        }

        let mut location_extras = Vec::new();
        for (index, location) in locations.iter().enumerate() {
            let extras = self.materialize_location(
                location,
                &primary_name,
                server_index,
                index,
                server_root.as_deref(),
                &server_try_files,
                &server_index_files,
                &inherited_access,
                &inherited_limit_req,
                inherited_auth_realm.as_deref(),
                inherited_auth_file.as_deref(),
                inherited_auth_off,
                &inherited_proxy_set_headers,
                &effective_proxy_ssl,
                named_pc_root.as_deref(),
                named_h5_root.as_deref(),
            )?;
            location_extras.push(extras);
        }
        if locations.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "server requires at least one location",
            ));
        }

        let mut virtual_host = json!({
            "id": virtual_host_id,
            "listenerRefs": listener_refs,
            "serverNames": server_names,
            "routes": [],
        });
        if !add_headers.is_empty() {
            let mut security = serde_json::Map::new();
            let mut custom = Vec::new();
            for header in add_headers {
                let name = header.get("name").and_then(Value::as_str).unwrap_or_default();
                let value = header.get("value").and_then(Value::as_str).unwrap_or_default();
                let lower = name.to_ascii_lowercase();
                match lower.as_str() {
                    "x-frame-options" => {
                        let frame = if value.eq_ignore_ascii_case("SAMEORIGIN") {
                            "SAMEORIGIN"
                        } else {
                            "DENY"
                        };
                        security.insert("xFrameOptions".to_owned(), Value::String(frame.to_owned()));
                    }
                    "x-content-type-options" if value.eq_ignore_ascii_case("nosniff") => {
                        security.insert("xContentTypeOptions".to_owned(), Value::Bool(true));
                    }
                    "content-security-policy" => {
                        security.insert("contentSecurityPolicy".to_owned(), Value::String(value.to_owned()));
                    }
                    "referrer-policy" => {
                        security.insert("referrerPolicy".to_owned(), Value::String(value.to_owned()));
                    }
                    "strict-transport-security" => {
                        let max_age = value
                            .split(';')
                            .find_map(|directive| directive.trim().strip_prefix("max-age="))
                            .and_then(|value| value.parse::<u64>().ok())
                            .unwrap_or(31_536_000);
                        security.insert(
                            "strictTransportSecurity".to_owned(),
                            json!({ "maxAgeSeconds": max_age }),
                        );
                    }
                    _ => custom.push(header),
                }
            }
            if !custom.is_empty() {
                security.insert("customHeaders".to_owned(), Value::Array(custom));
            }
            virtual_host["securityHeaders"] = Value::Object(security);
        }
        // Rewrite route matches with their actual path types (exact/prefix).
        let mut route_entries = Vec::new();
        for (index, location) in locations.iter().enumerate() {
            let (path_type, path) = parse_location_match(location)?;
            let mut route = json!({
                "id": format!("route-{server_index}-{index}"),
                "match": {"pathType": path_type, "path": path},
                "resourceRef": format!("loc-{server_index}-{index}"),
            });
            if !location_extras[index].rewrite.is_empty() {
                route["rewrite"] = Value::Array(location_extras[index].rewrite.clone());
            }
            if !location_extras[index].access.is_empty() {
                route["access"] = Value::Array(location_extras[index].access.clone());
            }
            if !location_extras[index].limit_req.is_empty() {
                route["limitReq"] = Value::Array(location_extras[index].limit_req.clone());
            }
            if !location_extras[index].limit_conn.is_empty() {
                route["limitConn"] = Value::Array(location_extras[index].limit_conn.clone());
            }
            if let Some(auth_basic) = &location_extras[index].auth_basic {
                route["authBasic"] = auth_basic.clone();
            }
            if let Some(sub_filter) = &location_extras[index].sub_filter {
                route["subFilter"] = sub_filter.clone();
            }
            if let Some(secure_link) = &location_extras[index].secure_link {
                route["secureLink"] = secure_link.clone();
            }
            route_entries.push(route);
        }
        virtual_host["routes"] = Value::Array(route_entries);
        self.note_client_max_body_size(client_max_body_size);
        self.virtual_hosts.push(virtual_host);
        Ok(())
    }

    fn materialize_location(
        &mut self,
        location: &NginxDirective,
        server_name: &str,
        server_index: usize,
        index: usize,
        inherited_root: Option<&str>,
        inherited_try_files: &[String],
        inherited_index_files: &[String],
        inherited_access: &[Value],
        inherited_limit_req: &[Value],
        inherited_auth_realm: Option<&str>,
        inherited_auth_file: Option<&str>,
        inherited_auth_off: bool,
        inherited_proxy_set_headers: &[String],
        inherited_proxy_ssl: &ProxySslSettings,
        named_pc_root: Option<&str>,
        named_h5_root: Option<&str>,
    ) -> Result<LocationExtras, NginxConfigError> {
        let (path_type, location_path) = parse_location_match(location)?;
        let resource_id = format!("loc-{server_index}-{index}");
        let mut proxy_pass = None;
        let mut dynamic_target = None;
        let mut proxy_pass_request_headers = true;
        let mut return_directive = None;
        let mut root = None;
        let mut alias = None;
        let mut try_files = Vec::new();
        let mut index_files = Vec::new();
        let mut extras = LocationExtras::default();
        let mut client_max_body_size: Option<u64> = None;
        let mut location_access = Vec::new();
        let mut location_limit_req = Vec::new();
        let mut location_proxy_set_headers: Vec<String> = Vec::new();
        let mut auth_realm = inherited_auth_realm.map(str::to_owned);
        let mut auth_file = inherited_auth_file.map(str::to_owned);
        let mut auth_off = inherited_auth_off;
        let mut location_proxy_ssl = ProxySslSettings::default();
        for child in &location.children {
            match child.name.as_str() {
                "proxy_pass" => {
                    let Some(target) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "proxy_pass requires a target",
                        ));
                    };
                    if target.contains('$') {
                        crate::config::validate_proxy_pass_template(target).map_err(|message| {
                            NginxConfigError::unsupported(child, message)
                        })?;
                        dynamic_target = Some(target.clone());
                    } else {
                        proxy_pass = Some(target.clone());
                    }
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
                                format!("proxy_pass_request_headers accepts on|off, found `{other}`"),
                            ))
                        }
                    }
                }
                "return" => {
                    return_directive = Some(child.args.clone());
                }
                "root" => {
                    let Some(_root_path) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "root requires a directory",
                        ));
                    };
                    root = Some(self.resolve_path(child)?);
                }
                "alias" => {
                    let Some(_alias_path) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "alias requires a directory",
                        ));
                    };
                    alias = Some(self.resolve_path(child)?);
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
                    // Location-level response headers (e.g. `Vary`) are accepted
                    // for nginx compatibility with Adaptive Web snippets.
                    if child.args.len() < 2 {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "add_header requires a name and value",
                        ));
                    }
                }
                "proxy_set_header" => {
                    let entry = format_proxy_set_header_entry(&child.args).map_err(|message| {
                        NginxConfigError::unsupported(child, message)
                    })?;
                    location_proxy_set_headers.push(entry);
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
                    crate::config::validate_md5_template(template).map_err(|message| {
                        NginxConfigError::unsupported(child, message)
                    })?;
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
                            format!("secure_link_expires argument `{argument}` must be `$arg_<name>`"),
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
                "proxy_http_version" | "proxy_buffering"
                | "proxy_read_timeout" | "proxy_send_timeout" | "proxy_connect_timeout"
                | "proxy_redirect" | "proxy_request_buffering" => {}
                "proxy_cache" => {
                    self.materialize_proxy_cache(child)?;
                }
                "proxy_cache_key" => {
                    self.proxy_cache_enabled = true;
                }
                "proxy_ssl_verify" | "proxy_ssl_trusted_certificate"
                | "proxy_ssl_certificate" | "proxy_ssl_certificate_key"
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
            return_directive.is_some(),
            root.is_some(),
            alias.is_some(),
        ]
        .iter()
        .filter(|present| **present)
        .count();
        // A location with only `try_files` inherits the server-level root
        // (nginx SPA layout), which counts as static serving.
        let inherits_static = serving == 0 && inherited_root.is_some();
        // Adaptive Web dispatch: `try_files … @$surface;` jumps to named
        // locations (`@pc` / `@h5`) that own the document roots.
        let effective_try_files_for_check: &[String] = if try_files.is_empty() {
            inherited_try_files
        } else {
            try_files.as_slice()
        };
        let adaptive_named_dispatch = serving == 0
            && effective_try_files_for_check
                .iter()
                .any(|entry| entry.starts_with('@'));
        if serving > 1 || (serving == 0 && !inherits_static && !adaptive_named_dispatch) {
            return Err(NginxConfigError::unsupported(
                location,
                "a location must declare exactly one of proxy_pass | return | root | alias (or inherit the server root with try_files, or Adaptive Web try_files → @named)",
            ));
        }
        // nginx `try_files … @named` with a serving behavior would dispatch
        // internally to a named location; the runtime cannot execute that
        // jump, so it fails closed (Adaptive Web dispatch is only supported
        // on locations without another serving behavior).
        if serving > 0
            && effective_try_files_for_check
                .last()
                .is_some_and(|entry| entry.starts_with('@'))
        {
            return Err(NginxConfigError::unsupported(
                location,
                format!(
                    "try_files `@named` dispatch cannot be combined with proxy_pass | return | root | alias; the runtime only supports Adaptive Web dispatch on a location with no other serving behavior"
                ),
            ));
        }
        self.note_client_max_body_size(client_max_body_size);
        let _ = server_name;

        if let Some(target) = dynamic_target {
            if !effective_proxy_ssl.is_empty() {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`proxy_ssl_*` with variable `proxy_pass` targets is not supported; use a literal http(s):// target",
                ));
            }
            // Variable proxy_pass: the URL is evaluated per request; the
            // materialized model carries the template.
            let request_set_headers =
                merge_proxy_set_headers(inherited_proxy_set_headers, &location_proxy_set_headers);
            let mut proxy_resource = json!({
                "id": resource_id,
                "type": "proxy",
                "stripPrefix": false,
                "upstreamRef": "",
                "dynamicTarget": target,
                "proxyPassRequestHeaders": proxy_pass_request_headers,
            });
            if !request_set_headers.is_empty() {
                proxy_resource["requestSetHeaders"] = Value::Array(
                    request_set_headers
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                );
            }
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
                    format!("proxy_pass `{target}` must be http(s)://upstream or http(s)://host:port"),
                ));
            };
            if !effective_proxy_ssl.is_empty() && !upstream_target.starts_with("https://") {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`proxy_ssl_*` settings require an `https://` `proxy_pass` target (the runtime model attaches one TLS policy per upstream)",
                ));
            }
            let request_set_headers =
                merge_proxy_set_headers(inherited_proxy_set_headers, &location_proxy_set_headers);
            let mut proxy_resource = json!({
                "id": resource_id,
                "type": "proxy",
                "stripPrefix": false,
                "proxyPassRequestHeaders": proxy_pass_request_headers,
            });
            if let Some(uri) = &target_uri {
                proxy_resource["targetUri"] = Value::String(uri.clone());
            }
            if !request_set_headers.is_empty() {
                proxy_resource["requestSetHeaders"] = Value::Array(
                    request_set_headers
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                );
            }
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
        } else if let Some(return_args) = return_directive {
            let Some(first) = return_args.first() else {
                return Err(NginxConfigError::unsupported(location, "return requires a status or URL"));
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
                if first.contains('$') && !redirect_variables_ok(first) {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("return URL `{first}` uses unsupported variables; supported: $host $request_uri $scheme"),
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
                    if url.contains('$') && !redirect_variables_ok(url) {
                        return Err(NginxConfigError::unsupported(
                            location,
                            format!("return URL `{url}` uses unsupported variables; supported: $host $request_uri $scheme"),
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
                        "body": "",
                    }));
                }
                (200..=599, Some(body)) => {
                    if body.contains('$') {
                        return Err(NginxConfigError::unsupported(
                            location,
                            format!(
                                "return body `{body}` contains variables; the runtime responds with literal text only"
                            ),
                        ));
                    }
                    self.resources.push(json!({
                        "id": resource_id,
                        "type": "respond",
                        "status": status,
                        "contentType": "text/plain; charset=utf-8",
                        "body": body,
                    }));
                }
                _ => {
                    return Err(NginxConfigError::unsupported(
                        location,
                        format!("return {status} requires a URL for redirects or a body for responses"),
                    ));
                }
            }
        } else if let Some(root) = root.or_else(|| inherited_root.map(str::to_owned)) {
            // nginx `root` uses POSIX path semantics: a leading `/` is
            // absolute regardless of the host platform, and the full request
            // path is appended to the root (no prefix stripping).
            if !root.starts_with('/') {
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
            let spa_fallback = effective_try_files
                .iter()
                .filter(|entry| entry.starts_with('/') && !entry.starts_with('$'))
                .last()
                .map(|entry| entry.trim_start_matches('/').to_owned());
            let index_files: Vec<String> = if effective_index_files.is_empty() {
                vec!["index.html".to_owned()]
            } else {
                effective_index_files.to_vec()
            };
            self.resources.push(json!({
                "id": resource_id,
                "type": "static",
                "root": root,
                "indexFiles": index_files,
                "spaFallback": spa_fallback,
            }));
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
            if !alias.starts_with('/') {
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
            if !alias.ends_with('/') && location_path.ends_with('/') {
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
            self.resources.push(json!({
                "id": resource_id,
                "type": "static",
                "root": alias,
                "indexFiles": alias_index_files,
                "stripPrefix": true,
            }));
        }
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
        self.real_ip_by_listener.insert(key.clone(), settings.clone());
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

    fn materialize_gzip_types(&mut self, directive: &NginxDirective) -> Result<(), NginxConfigError> {
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
                    if !name.is_empty() && !self.proxy_cache_zone_names.contains(&name.to_owned())
                    {
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

    fn parse_limit_req_rule(
        &self,
        directive: &NginxDirective,
    ) -> Result<Value, NginxConfigError> {
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

    fn parse_limit_conn_rule(
        &self,
        directive: &NginxDirective,
    ) -> Result<Value, NginxConfigError> {
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
        let contents = std::fs::read_to_string(path).map_err(|error| {
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
        for child in &directive.children {
            match child.name.as_str() {
                "listen" => {
                    if child.args.first().is_none() {
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
                                format!("ssl_verify_client accepts on|optional|off, found `{other}`"),
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
                "ssl_protocols" | "ssl_ciphers" | "ssl_prefer_server_ciphers"
                | "proxy_connect_timeout" | "proxy_socket_keepalive" | "so_keepalive"
                | "access_log" | "error_log" => {}
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
        let Some(target_name) = proxy_pass else {
            return Err(NginxConfigError::unsupported(
                directive,
                "stream server requires proxy_pass",
            ));
        };
        let target = if self.upstream_names.contains(&target_name) {
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
        };
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
                "target": target,
                "proxyTimeoutMs": proxy_timeout_ms,
                "proxyProtocol": proxy_protocol,
            });
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
            let _: StreamTargetConfig = serde_json::from_value(target.clone()).map_err(|error| {
                NginxConfigError::unsupported(
                    directive,
                    format!("stream target materialization failed: {error}"),
                )
            })?;
            if stream.get("tls").is_some() {
                let _: StreamTlsMode = serde_json::from_value(stream["tls"].clone()).map_err(|error| {
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
        let instance = json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": self.app_key,
            "nginx": {
                "enabled": true,
                "profile": "http-core-v1",
                "unknownDirectivePolicy": "error",
            },
            "gzip": {
                "enabled": self.gzip_enabled,
                "types": self.gzip_types,
                "minLength": self.gzip_min_length,
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
            "proxyCache": proxy_cache,
            "metadata": { "source": "nginx configuration" },
        });
        let mut instance = instance;
        if self.client_max_body_size_conflict {
            return Err(NginxConfigError::unsupported(
                &NginxDirective {
                    name: "client_max_body_size".to_owned(),
                    args: Vec::new(),
                    children: Vec::new(),
                    line: 0,
                    source: self.base_dir.to_path_buf(),
                },
                "conflicting `client_max_body_size` values at different levels; the runtime model supports one global request body limit — declare a single value",
            ));
        }
        if let Some(bytes) = self.client_max_body_size {
            instance["limits"] = json!({ "maxRequestBodyBytes": bytes });
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
            other if other.starts_with("backlog=")
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
                return Err(format!("unsupported listen parameter `{other}` in `{spec}`"));
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
            trusted_certificate: self.trusted_certificate.clone().or_else(|| lower.trusted_certificate.clone()),
            certificate: self.certificate.clone().or_else(|| lower.certificate.clone()),
            certificate_key: self.certificate_key.clone().or_else(|| lower.certificate_key.clone()),
            server_name: self.server_name.or(lower.server_name),
        }
    }

    fn is_empty(&self) -> bool {
        self == &ProxySslSettings::default()
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

fn parse_stream_listen_spec(directive: &NginxDirective) -> Result<StreamListenSpec, NginxConfigError> {
    let mut parts = directive.args.iter();
    let address = parts
        .next()
        .ok_or_else(|| NginxConfigError::unsupported(directive, "stream listen requires an address"))?;
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
            other if other.starts_with("backlog=")
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
    let (bind, port) =
        parse_listen_address(address, &directive.args.join(" ")).map_err(|message| {
            NginxConfigError::unsupported(directive, message)
        })?;
    Ok(StreamListenSpec { bind, port, ssl, udp })
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

/// Validate `server_name` names against what the runtime can match.
///
/// nginx supports exact names, one-level leading wildcards (`*.example.com`
/// matches subdomains of any depth in nginx, which the runtime implements),
/// trailing wildcards, `.example.org`-style names, `~` regex names, and the
/// empty name. The runtime model only matches exact and leading-wildcard
/// names, so every other form fails closed instead of silently never
/// matching.
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
        if name.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "the empty `server_name \"\"` (requests without a Host header) is not supported by the runtime model",
            ));
        }
        if name.starts_with('~') {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("regex server name `{name}` is not supported by the runtime model; use an exact or leading-wildcard name"),
            ));
        }
        if name.starts_with('.') {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("`.example.org`-style server name `{name}` is not supported by the runtime model; use `{name}` plus `*{name}`"),
            ));
        }
        if name.contains('*') && !name.starts_with("*.") {
            return Err(NginxConfigError::unsupported(
                directive,
                format!("trailing/embedded wildcard server name `{name}` is not supported by the runtime model; use a leading `*.` wildcard"),
            ));
        }
        if let Some(suffix) = name.strip_prefix("*.") {
            if suffix.is_empty() || suffix.contains('*') {
                return Err(NginxConfigError::unsupported(
                    directive,
                    format!("invalid leading-wildcard server name `{name}`"),
                ));
            }
        }
    }
    Ok(())
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
        if entry != "$uri" && entry != "$uri/" {
            return Err(NginxConfigError::unsupported(
                directive,
                format!(
                    "try_files entry `{entry}` is not supported; the runtime executes `$uri` and `$uri/` probes with a literal fallback path"
                ),
            ));
        }
    }
    match entries.last().map(String::as_str) {
        Some(last) if last.starts_with('/') => Ok(()),
        // Adaptive Web dispatch target (`try_files … @$surface`).
        Some(last) if last.starts_with('@') => Ok(()),
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
        let value = directive
            .args
            .first()
            .expect("checked argument presence");
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
/// `/x` prefix, `~`/`~*` regex. Adaptive Web named locations (`@pc` / `@h5`)
/// are collected before this parser runs.
fn parse_location_match(
    location: &NginxDirective,
) -> Result<(&'static str, String), NginxConfigError> {
    let Some(match_value) = location.args.first() else {
        return Err(NginxConfigError::unsupported(
            location,
            "location requires a match path",
        ));
    };
    if match_value.starts_with('@') {
        return Err(NginxConfigError::unsupported(
            location,
            format!(
                "named location `{match_value}` is not supported; inline the location or fail closed"
            ),
        ));
    }
    match (match_value.as_str(), location.args.get(1)) {
        ("=", Some(path)) => Ok(("exact", path.clone())),
        ("^~", Some(path)) => Ok(("prefix-exclusive", path.clone())),
        ("~", Some(pattern)) => Ok(("regex", pattern.clone())),
        ("~*", Some(pattern)) => Ok(("regex-ignore-case", pattern.clone())),
        (path, _) => Ok(("prefix", path.to_owned())),
    }
}

/// Only the variable combinations the redirect data plane expands are
/// accepted in `return` URLs and TOML `returnLocation` values.
pub(crate) fn redirect_variables_ok(url: &str) -> bool {
    let mut remainder = url;
    while let Some(index) = remainder.find('$') {
        let rest = &remainder[index..];
        let variable = if rest.starts_with("$request_uri") {
            "$request_uri"
        } else if rest.starts_with("$host") {
            "$host"
        } else if rest.starts_with("$scheme") {
            "$scheme"
        } else {
            return false;
        };
        remainder = &rest[variable.len()..];
    }
    true
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
        NginxConfigError::unsupported(
            directive,
            format!("invalid client_max_body_size `{value}`"),
        )
    })
}

fn parse_size_bytes(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    let (number, multiplier) = if let Some(rest) = trimmed.strip_suffix('k').or_else(|| trimmed.strip_suffix('K')) {
        (rest, 1024u64)
    } else if let Some(rest) = trimmed.strip_suffix('m').or_else(|| trimmed.strip_suffix('M')) {
        (rest, 1024 * 1024)
    } else if let Some(rest) = trimmed.strip_suffix('g').or_else(|| trimmed.strip_suffix('G')) {
        (rest, 1024 * 1024 * 1024)
    } else {
        (trimmed, 1)
    };
    number.trim().parse::<u64>().ok()?.checked_mul(multiplier)
}

/// Sanitize a bind address, upstream target, or hostname into the runtime id
/// charset `[a-z0-9-]` (dots, colons, slashes become dashes).
/// Map one `add_header Name value` entry into a security/custom header
/// object; the dedicated security fields are recognized by name.
fn parse_add_header(name: &str, value: &str) -> Value {
    json!({ "name": name, "value": value })
}

/// Parse `rewrite <pattern> <replacement> <flag>` into a runtime rule.
/// Unsupported flags fail closed with a precise diagnostic.
fn parse_rewrite_rule(
    directive: &NginxDirective,
) -> Result<Value, NginxConfigError> {
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
    use crate::nginx::parser::parse_nginx_config;

    fn materialize(text: &str) -> Result<WebServerAppConfig, NginxConfigError> {
        let parsed = parse_nginx_config(text, std::path::Path::new("site.conf")).expect("parse");
        materialize_nginx_app(&parsed, std::path::Path::new("/etc/nginx/sites-enabled"), "test")
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
        assert!(host
            .routes
            .iter()
            .any(|route| route.route_match.path_type == crate::config::RoutePathType::Exact
                && route.route_match.path == "/healthz"));
    }

    #[test]
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
            materialize_nginx_app(&parsed, directory.path(), "test").expect("materialize");
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
        } = &certificate.source;
        assert!(certificate_file.ends_with("site.pem"));
        assert!(private_key_file.ends_with("site.key"));
        // Relative certificate paths resolve against the config directory.
        assert!(std::path::Path::new(certificate_file).is_absolute());
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
            materialize_nginx_app(&parsed, directory.path(), "test").expect("materialize");
        assert_eq!(config.virtual_hosts.len(), 2);
        let redirect = config
            .resources
            .iter()
            .find(|resource| matches!(resource, crate::config::ResourceConfig::Redirect { .. }))
            .expect("redirect resource");
        let crate::config::ResourceConfig::Redirect { status, location, .. } = redirect else {
            unreachable!()
        };
        assert_eq!(*status, 301);
        assert_eq!(location, "https://$host$request_uri");
        let static_resource = config
            .resources
            .iter()
            .find(|resource| matches!(resource, crate::config::ResourceConfig::Static { .. }))
            .expect("static resource");
        let crate::config::ResourceConfig::Static { root, spa_fallback, .. } = static_resource
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
        let config = materialize_nginx_app(
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
            .find(|route| route.route_match.path_type == crate::config::RoutePathType::PrefixExclusive)
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
        let config = materialize_nginx_app(
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
        let crate::config::ResourceConfig::Static { root, spa_fallback, .. } = static_resource
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
        .err()
        .expect("unknown rewrite flag must be rejected");
        assert!(error.to_string().contains("rewrite flag"), "{error}");
    }

    #[test]
    fn materializes_gzip_limit_req_cache_and_stream() {
        let directory = tempfile::tempdir().expect("temp");
        let htpasswd = directory.path().join("htpasswd");
        // username `alice`, password `secret` as {SHA} hash
        std::fs::write(
            &htpasswd,
            "alice:{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ=\n",
        )
        .unwrap();
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
            materialize_nginx_app(&parsed, directory.path(), "test").expect("materialize");
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
        .err()
        .expect("duplicate default_server must be rejected");
        assert!(error.to_string().contains("duplicate `default_server`"), "{error}");
    }

    #[test]
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
            materialize_nginx_app(&parsed, directory.path(), "test").expect("materialize");
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
            proxy_resources.contains(&("literal-127-0-0-1-9002".to_owned(), Some("/api".to_owned()))),
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
                crate::config::ResourceConfig::Redirect { status, location, .. } => {
                    Some((*status, location.clone()))
                }
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
        .err()
        .expect("return 444 must fail closed");
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
        .err()
        .expect("alias gluing footgun must fail closed");
        assert!(error.to_string().contains("trailing slash"), "{error}");
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
        assert!(!static_resource.1, "nginx root must keep the full request path");
    }

    #[test]
    fn try_files_rejects_unsupported_forms() {
        for (suffix, expected) in [
            ("=404", "try_files fallback"),
            ("@named", "try_files `@named` dispatch cannot be combined"),
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
        let error = materialize(
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
        .err()
        .expect("intermediate literal try_files entries must fail closed");
        assert!(
            error.to_string().contains("try_files entry `/fixed.html`"),
            "{error}"
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
        .err()
        .expect("resolve parameter must fail closed");
        assert!(error.to_string().contains("unsupported upstream server parameter"), "{error}");
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
        .err()
        .expect("proxy_protocol v2 must fail closed");
        assert!(error.to_string().contains("proxy_protocol v2"), "{error}");
    }

    #[test]
    fn conflicting_client_max_body_size_values_fail_closed() {
        let error = materialize(
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
        .err()
        .expect("conflicting body limits must fail closed");
        assert!(
            error.to_string().contains("conflicting `client_max_body_size`"),
            "{error}"
        );
    }

    #[test]
    fn unsupported_server_name_forms_fail_closed() {
        for (name, expected) in [
            ("~^www\\d+\\.example\\.com$", "regex server name"),
            ("www.example.*", "wildcard server name"),
            ("\"\"", "empty `server_name"),
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
    fn named_locations_and_named_try_files_fail_closed_unless_adaptive_dispatch() {
        let error = materialize(
            r#"
server {
    listen 80;
    server_name named.example.com;
    location @fallback {
        root /srv/www;
    }
    location / { return 200 "ok"; }
}
"#,
        )
        .err()
        .expect("unknown named locations must fail closed");
        assert!(error.to_string().contains("named location"), "{error}");
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
        let crate::config::ResourceConfig::Static {
            root,
            h5_root,
            ..
        } = &config.resources[0]
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
        location / {
            proxy_set_header X-Server-Level $host;
            proxy_pass http://api;
        }
    }
}
"#,
        )
        .expect("materialize");
        let proxy = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Proxy {
                    request_set_headers, ..
                } => Some(request_set_headers.clone()),
                _ => None,
            })
            .expect("proxy resource");
        assert!(
            proxy.iter().any(|entry| entry == "X-Http-Level $scheme"),
            "{proxy:?}"
        );
        assert!(
            proxy.iter().any(|entry| entry == "X-Server-Level $host"),
            "{proxy:?}"
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
        .err()
        .expect("undefined cache zone must fail closed");
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
            materialize_nginx_app(&parsed, directory.path(), "test").expect("materialize");
        let upstream = config
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "api")
            .expect("api upstream");
        let tls = upstream.tls.as_ref().expect("upstream tls");
        assert_eq!(
            tls.trust_mode,
            crate::config::UpstreamTlsTrustMode::Custom
        );
        assert_eq!(tls.ca_certificate_files.len(), 1);
        assert!(tls.ca_certificate_files[0].ends_with("ca.pem"));
        assert!(tls.client_certificate_file.as_deref().unwrap().ends_with("client.pem"));
        assert!(tls.client_private_key_file.as_deref().unwrap().ends_with("client.key"));
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
        .err()
        .expect("conflicting upstream TLS settings must fail closed");
        assert!(
            error.to_string().contains("conflicting `proxy_ssl_*` settings"),
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
        .err()
        .expect("unsupported real_ip_header must fail closed");
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
        .err()
        .expect("conflicting real_ip settings must fail closed");
        assert!(error.to_string().contains("conflicting `set_real_ip_from`"), "{error}");
    }

    #[test]
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
            materialize_nginx_app(&parsed, directory.path(), "test").expect("materialize");
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
    location / { return 200 "$host"; }
}
"#,
        )
        .err()
        .expect("variable return bodies must fail closed");
        assert!(
            error.to_string().contains("contains variables"),
            "{error}"
        );
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
            ("ssl_protocols", "TLSv1.2 TLSv1.3"),
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
            ("expires", "1d"),
            ("etag", "off"),
            ("if_modified_since", "before"),
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
            "", " ", "\n", "# comment only\n",
            "server", "server {", "server { }", "}", "{", "};",
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
            let parsed = parse_nginx_config(case, Path::new("adversarial.conf"));
            match parsed {
                Ok(directives) => {
                    // Materialization may fail; it must not panic.
                    let _ = materialize_nginx_app(
                        &directives,
                        Path::new("/etc/nginx/sites-enabled"),
                        "adversarial",
                    );
                }
                Err(_) => {}
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
            .err()
            .expect("excessive nesting must fail closed");
        assert!(
            error.to_string().contains("nesting exceeds"),
            "{error}"
        );
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
        .err()
        .expect("invalid client_max_body_size must fail closed");
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
        .err()
        .expect("unix proxy_pass must fail closed");
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
}
