//! Materialize parsed nginx `http`- and `stream`-context configuration into
//! the runtime `WebServerAppConfig` model.
//!
//! Supported directives (everything else fails closed with a diagnostic):
//!
//! - `upstream { server <addr> [weight=…] [max_fails=…] [fail_timeout=…]; keepalive …; }`
//! - `server { listen …; server_name …; ssl_certificate(_key) …; location …; }`
//! - `location <match> { proxy_pass http://<upstream|host:port>; … }`
//! - `location <match> { return <code> <url-with-$host/$request_uri/$scheme>; }`
//! - `location <match> { root <absolute>; try_files $uri $uri/ /index.html; }`
//! - `location <match> { alias <absolute-dir/>; }` (directory must end with `/`)
//! - `location` `rewrite`, `allow`/`deny`, `limit_req`, `auth_basic` +
//!   `auth_basic_user_file` (htpasswd loaded at materialize)
//! - http `limit_req_zone`, `gzip` / `gzip_types` / `gzip_min_length`
//! - http `proxy_cache_path` / `proxy_cache` / `proxy_cache_valid`
//! - location/server `proxy_set_header` (validated supported `$vars`)
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
    format_proxy_set_header_entry, merge_proxy_set_headers, parse_htpasswd, parse_limit_req,
    parse_limit_req_zone, ConfigDiagnostic, StreamTargetConfig, StreamTlsMode, WebServerAppConfig,
    WebServerConfigError,
};

use super::parser::NginxDirective;

const ACCEPTED_IGNORED: &[&str] = &[
    // process / http tuning (gzip / limit_req_zone / proxy_cache* are handled explicitly)
    "user", "worker_processes", "worker_connections", "pid", "error_log", "access_log",
    "sendfile", "tcp_nopush", "tcp_nodelay", "keepalive_timeout", "server_tokens",
    "map", "log_format", "types", "default_type", "charset", "events", "so_keepalive",
    "resolver", "client_body_timeout", "client_header_timeout", "client_header_buffer_size",
    "large_client_header_buffers", "reset_timedout_connection", "server_names_hash_max_size",
    "proxy_http_version", "proxy_buffering", "proxy_request_buffering",
    "proxy_intercept_errors", "proxy_next_upstream", "proxy_hide_header", "proxy_redirect",
    "proxy_connect_timeout", "proxy_read_timeout", "proxy_send_timeout", "proxy_buffer_size",
    "proxy_buffers", "ssl_protocols", "ssl_prefer_server_ciphers", "ssl_session_cache",
    "ssl_session_timeout", "ssl_session_tickets", "ssl_stapling", "ssl_stapling_verify",
    "ssl_trusted_certificate", "ssl_ciphers", "http2", "keepalive",
    "client_body_buffer_size", "send_timeout", "fastcgi_read_timeout", "merge_slashes",
    "gzip_comp_level", "gzip_vary", "gzip_proxied", "gzip_disable", "gzip_static",
    "open_file_cache", "open_file_cache_valid", "open_file_cache_min_uses",
    "underscores_in_headers", "ignore_invalid_headers", "absolute_redirect",
    "port_in_redirect", "server_name_in_redirect",
];

const UNSUPPORTED_SECURITY: &[&str] = &[
    "sub_filter", "limit_conn", "secure_link", "proxy_pass_request_headers",
];

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
                diagnostics: vec![ConfigDiagnostic::new(
                    format!("{}:{line}", path.display()),
                    message.clone(),
                )],
                path,
                line,
                message,
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
            "gzip" => mapper.materialize_gzip(directive)?,
            "gzip_types" => mapper.materialize_gzip_types(directive)?,
            "gzip_min_length" => mapper.materialize_gzip_min_length(directive)?,
            "proxy_cache_path" => mapper.materialize_proxy_cache_path(directive)?,
            "proxy_cache_valid" => mapper.apply_proxy_cache_valid(directive)?,
            "proxy_cache" => mapper.proxy_cache_enabled = true,
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
    auth_basic: Option<Value>,
}

struct Mapper<'a> {
    app_key: &'a str,
    base_dir: &'a Path,
    proxy_cache_enabled: bool,
    proxy_cache_disk_path: Option<String>,
    proxy_cache_ttl_seconds: u64,
    proxy_cache_stale_ttl_seconds: u64,
    proxy_cache_max_entries: usize,
    gzip_enabled: bool,
    gzip_types: Vec<String>,
    gzip_min_length: u64,
    listeners: Vec<Value>,
    listeners_by_port: HashMap<(String, u16), String>,
    certificates: Vec<Value>,
    tls_policies: Vec<Value>,
    resources: Vec<Value>,
    upstreams: Vec<Value>,
    upstream_names: Vec<String>,
    virtual_hosts: Vec<Value>,
    streams: Vec<Value>,
    limit_req_zones: Vec<Value>,
    limit_req_zone_names: Vec<String>,
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
            gzip_enabled: false,
            gzip_types: Vec::new(),
            gzip_min_length: 20,
            listeners: Vec::new(),
            listeners_by_port: HashMap::new(),
            certificates: Vec::new(),
            tls_policies: Vec::new(),
            resources: Vec::new(),
            upstreams: Vec::new(),
            upstream_names: Vec::new(),
            virtual_hosts: Vec::new(),
            streams: Vec::new(),
            limit_req_zones: Vec::new(),
            limit_req_zone_names: Vec::new(),
        }
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
                    for argument in child.args.iter().skip(1) {
                        if let Some(value) = argument.strip_prefix("weight=") {
                            weight = parse_u64(value).map_or(Value::Null, Value::from);
                        } else if argument == "backup" {
                            backup = true;
                        } else if argument.starts_with("max_fails=")
                            || argument.starts_with("fail_timeout=")
                        {
                            // Accepted for nginx compatibility; the runtime
                            // owns its own failure/ejection policy.
                        } else if argument == "down"
                            || argument.starts_with("max_conns=")
                            || argument.starts_with("slow_start=")
                        {
                            return Err(NginxConfigError::unsupported(
                                child,
                                format!("upstream server flag `{argument}` is not supported"),
                            ));
                        }
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
                "keepalive" => {}
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
        let authorized_literal_ips = targets
            .iter()
            .filter_map(|target| target.get("url").and_then(Value::as_str))
            .filter_map(|url| {
                let host = url
                    .strip_prefix("http://")
                    .or_else(|| url.strip_prefix("https://"))?
                    .rsplit_once(':')?
                    .0;
                host.parse::<std::net::IpAddr>().ok().map(|ip| match ip {
                    std::net::IpAddr::V4(ip) => format!("{ip}/32"),
                    std::net::IpAddr::V6(ip) => format!("{ip}/128"),
                })
            })
            .collect::<Vec<_>>();
        let mut upstream = json!({
            "id": name,
            "targets": targets,
            "loadBalancing": load_balancing,
        });
        if let Some(hash) = hash_config {
            upstream["hash"] = hash;
        }
        if !authorized_literal_ips.is_empty() {
            upstream["addressPolicy"] = json!({ "allowedCidrs": authorized_literal_ips });
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
        let mut listen_entries = Vec::new();
        let mut server_names: Vec<String> = Vec::new();
        let mut certificate_file: Option<String> = None;
        let mut certificate_key: Option<String> = None;
        let mut certificate_name: Option<String> = None;
        let mut locations = Vec::new();
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

        for child in &directive.children {
            match child.name.as_str() {
                "listen" => {
                    if child.args.first().is_none() {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "listen requires an address",
                        ));
                    }
                    let ssl = child.args.iter().any(|argument| argument == "ssl");
                    let http2 = child.args.iter().any(|argument| argument == "http2");
                    listen_entries.push((child.args.join(" "), ssl, http2));
                }
                "server_name" => {
                    server_names.extend(child.args.iter().cloned());
                }
                "ssl_certificate" => {
                    certificate_file = Some(self.resolve_path(child)?);
                    certificate_name = Some(format!("nginx-{}", certificate_name_counter()));
                }
                "ssl_certificate_key" => {
                    certificate_key = Some(self.resolve_path(child)?);
                }
                "location" => locations.push(child),
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
                    client_max_body_size = child
                        .args
                        .first()
                        .and_then(|value| parse_size_bytes(value));
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
                | "ssl_session_cache" | "ssl_trusted_certificate" | "http2"
                | "client_body_timeout" | "client_header_timeout" => {}
                "allow" | "deny" => {
                    inherited_access.push(parse_access_rule(child)?);
                }
                "limit_req" => {
                    inherited_limit_req.push(self.parse_limit_req_rule(child)?);
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

        if server_names.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "server requires server_name",
            ));
        }
        let primary_name = server_names[0].clone();
        let has_ssl = listen_entries.iter().any(|(_, ssl, _)| *ssl);
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
            self.certificates.push(json!({
                "id": certificate_name,
                "serverNames": server_names,
                "source": {
                    "type": "protected-file",
                    "certificateFile": certificate_file,
                    "privateKeyFile": certificate_key,
                },
            }));
            self.tls_policies.push(json!({
                "id": format!("tls-{certificate_name}"),
                "certificateRefs": [certificate_name],
                "minimumVersion": "tls1.2",
                "maximumVersion": "tls1.3",
                "alpn": ["h2", "http/1.1"],
            }));
        }

        let mut listener_refs = Vec::new();
        for (spec, ssl, http2) in &listen_entries {
            let (bind, port) = parse_listen_spec(spec).map_err(|message| {
                NginxConfigError::unsupported(directive, message)
            })?;
            let key = (bind.clone(), port);
            let listener_id = if let Some(existing) = self.listeners_by_port.get(&key) {
                existing.clone()
            } else {
                let id = format!("listener-{}-{port}", sanitize_id(&bind));
                let mut listener = json!({
                    "id": id,
                    "bind": bind,
                    "port": port,
                    "protocols": if *http2 && *ssl {
                        vec!["http1", "http2"]
                    } else {
                        vec!["http1"]
                    },
                });
                if !ssl {
                    listener["allowPlaintextHttp"] = Value::Bool(true);
                }
                if *ssl {
                    let policy = format!("tls-{}", certificate_name.as_deref().unwrap_or(""));
                    listener["tlsPolicyRef"] = Value::String(policy);
                }
                self.listeners.push(listener);
                self.listeners_by_port.insert(key, id.clone());
                id
            };
            if !listener_refs.contains(&listener_id) {
                listener_refs.push(listener_id);
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
            )?;
            location_extras.push(extras);
        }
        if locations.is_empty() {
            return Err(NginxConfigError::unsupported(
                directive,
                "server requires at least one location",
            ));
        }

        let first_port = listen_entries
            .first()
            .map(|(spec, _, _)| parse_listen_spec(spec).map(|(_, port)| port))
            .transpose()
            .map_err(|message| NginxConfigError::unsupported(directive, message))?
            .unwrap_or(0);
        let mut virtual_host = json!({
            "id": format!("{}-{first_port}", sanitize_id(&primary_name)),
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
            if let Some(auth_basic) = &location_extras[index].auth_basic {
                route["authBasic"] = auth_basic.clone();
            }
            route_entries.push(route);
        }
        virtual_host["routes"] = Value::Array(route_entries);
        if let Some(bytes) = client_max_body_size {
            let _ = bytes;
        }
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
    ) -> Result<LocationExtras, NginxConfigError> {
        let (path_type, _path) = parse_location_match(location)?;
        let resource_id = format!("loc-{server_index}-{index}");
        let mut proxy_pass = None;
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
        for child in &location.children {
            match child.name.as_str() {
                "proxy_pass" => {
                    let Some(target) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "proxy_pass requires a target",
                        ));
                    };
                    proxy_pass = Some(target.clone());
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
                    client_max_body_size = child
                        .args
                        .first()
                        .and_then(|value| parse_size_bytes(value));
                }
                "proxy_set_header" => {
                    let entry = format_proxy_set_header_entry(&child.args).map_err(|message| {
                        NginxConfigError::unsupported(child, message)
                    })?;
                    location_proxy_set_headers.push(entry);
                }
                "proxy_http_version" | "proxy_buffering"
                | "proxy_read_timeout" | "proxy_send_timeout" | "proxy_connect_timeout"
                | "proxy_redirect" | "proxy_request_buffering" => {}
                "proxy_cache" | "proxy_cache_key" => {
                    self.proxy_cache_enabled = true;
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
            proxy_pass.is_some(),
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
        if serving > 1 || (serving == 0 && !inherits_static) {
            return Err(NginxConfigError::unsupported(
                location,
                "a location must declare exactly one of proxy_pass | return | root | alias (or inherit the server root with try_files)",
            ));
        }
        let _ = (server_name, client_max_body_size);

        if let Some(target) = proxy_pass {
            let upstream_ref = if let Some(rest) = target
                .strip_prefix("http://")
                .or_else(|| target.strip_prefix("https://"))
            {
                rest.to_owned()
            } else {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!("proxy_pass `{target}` must be http(s)://upstream or http(s)://host:port"),
                ));
            };
            let request_set_headers =
                merge_proxy_set_headers(inherited_proxy_set_headers, &location_proxy_set_headers);
            let mut proxy_resource = json!({
                "id": resource_id,
                "type": "proxy",
                "stripPrefix": false,
            });
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
                    let address_policy = host
                        .parse::<std::net::IpAddr>()
                        .ok()
                        .map(|ip| match ip {
                            std::net::IpAddr::V4(ip) => format!("{ip}/32"),
                            std::net::IpAddr::V6(ip) => format!("{ip}/128"),
                        });
                    let mut literal_upstream = json!({
                        "id": literal_id,
                        "targets": [{"url": target}],
                        "loadBalancing": "round-robin",
                    });
                    if let Some(cidr) = address_policy {
                        literal_upstream["addressPolicy"] = json!({ "allowedCidrs": [cidr] });
                    }
                    self.upstreams.push(literal_upstream);
                    self.upstream_names.push(literal_id.clone());
                }
                proxy_resource["upstreamRef"] = Value::String(literal_id);
                self.resources.push(proxy_resource);
            } else if self.upstream_names.contains(&upstream_ref) {
                proxy_resource["upstreamRef"] = Value::String(upstream_ref);
                self.resources.push(proxy_resource);
            } else {
                return Err(NginxConfigError::unsupported(
                    location,
                    format!("proxy_pass references undefined upstream `{upstream_ref}`"),
                ));
            }
        } else if let Some(return_args) = return_directive {
            let status = return_args[0].parse::<u16>().map_err(|_| {
                NginxConfigError::unsupported(location, format!("invalid return status `{}`", return_args[0]))
            })?;
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
            // absolute regardless of the host platform.
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
        } else if let Some(alias) = alias {
            if !alias.starts_with('/') {
                return Err(NginxConfigError::unsupported(
                    location,
                    "alias must be an absolute directory for nginx compatibility",
                ));
            }
            if !alias.ends_with('/') {
                return Err(NginxConfigError::unsupported(
                    location,
                    "directory aliases must end with `/`",
                ));
            }
            if matches!(path_type, "regex" | "regex-ignore-case") {
                return Err(NginxConfigError::unsupported(
                    location,
                    "`alias` with regex location match is not supported; use a prefix/`^~` location",
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
            }));
        }
        let _ = (server_name, client_max_body_size);
        Ok(extras)
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
                if let Some((_name, size)) = rest.split_once(':') {
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
        let mut listen_spec = None;
        let mut proxy_pass = None;
        let mut proxy_timeout_ms = 60_000_u64;
        let mut proxy_protocol = false;
        let mut ssl = false;
        let mut ssl_preread = false;
        let mut certificate_file = None;
        let mut certificate_key = None;
        for child in &directive.children {
            match child.name.as_str() {
                "listen" => {
                    let Some(spec) = child.args.first() else {
                        return Err(NginxConfigError::unsupported(
                            child,
                            "stream listen requires an address",
                        ));
                    };
                    listen_spec = Some(spec.clone());
                    ssl = child.args.iter().any(|argument| argument == "ssl");
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
                    let flag = child.args.first().map(String::as_str).unwrap_or("on");
                    proxy_protocol = flag == "on";
                }
                "ssl_preread" => {
                    let flag = child.args.first().map(String::as_str).unwrap_or("on");
                    ssl_preread = flag == "on";
                }
                "ssl_certificate" => {
                    certificate_file = Some(self.resolve_path(child)?);
                }
                "ssl_certificate_key" => {
                    certificate_key = Some(self.resolve_path(child)?);
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
        let Some(listen) = listen_spec.as_deref() else {
            return Err(NginxConfigError::unsupported(
                directive,
                "stream server requires listen",
            ));
        };
        let (bind, port) = parse_listen_spec(listen)
            .map_err(|message| NginxConfigError::unsupported(directive, message))?;
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
        let mut stream = json!({
            "id": format!("stream-{index}-{port}"),
            "bind": bind,
            "port": port,
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
            let certificate_name = format!("stream-cert-{index}");
            self.certificates.push(json!({
                "id": certificate_name,
                "serverNames": ["stream.local"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": certificate_file,
                    "privateKeyFile": certificate_key,
                },
            }));
            stream["tls"] = json!({
                "mode": "terminate",
                "certificateRef": certificate_name,
            });
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

/// Parse `listen` spec into (bind, port). Supports `80`, `443 ssl http2`,
/// `127.0.0.1:8080`, `[::]:80`, `[::]:443 ssl http2`, and `default_server`.
fn parse_listen_spec(spec: &str) -> Result<(String, u16), String> {
    let mut parts = spec.split_whitespace();
    let address = parts
        .next()
        .ok_or_else(|| "listen entry is empty".to_owned())?;
    if parts.any(|part| part != "ssl" && part != "http2" && part != "default_server" && part != "reuseport") {
        return Err(format!("unsupported listen flag in `{spec}`"));
    }
    let (bind, port) = if let Some((host, port_text)) = address.rsplit_once(':') {
        let port = port_text.parse::<u16>().map_err(|_| format!("invalid listen port in `{spec}`"))?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let normalized = match host {
            "" | "0.0.0.0" | "*" => "0.0.0.0".to_owned(),
            "::" => "::".to_owned(),
            other => other.to_owned(),
        };
        (normalized, port)
    } else {
        let port = address.parse::<u16>().map_err(|_| format!("invalid listen value `{spec}`"))?;
        ("0.0.0.0".to_owned(), port)
    };
    Ok((bind, port))
}

/// Parse a location match into (path_type, path). `= /x` exact, `^~ /x` and
/// `/x` prefix; regex locations are rejected by the caller.
/// Parse a location match into (path_type, path). nginx tokenizes
/// `location = /x` as two arguments; `~`/`~*` regex and `^~` exclusive
/// prefixes map to the runtime route types.
fn parse_location_match(
    location: &NginxDirective,
) -> Result<(&'static str, String), NginxConfigError> {
    let Some(match_value) = location.args.first() else {
        return Err(NginxConfigError::unsupported(
            location,
            "location requires a match path",
        ));
    };
    match (match_value.as_str(), location.args.get(1)) {
        ("=", Some(path)) => Ok(("exact", path.clone())),
        ("^~", Some(path)) => Ok(("prefix-exclusive", path.clone())),
        ("~", Some(pattern)) => Ok(("regex", pattern.clone())),
        ("~*", Some(pattern)) => Ok(("regex-ignore-case", pattern.clone())),
        (path, _) => Ok(("prefix", path.to_owned())),
    }
}

/// Only the variable combinations the redirect data plane expands are
/// accepted in `return` URLs.
fn redirect_variables_ok(url: &str) -> bool {
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
}
