//! Layout v3 `server.toml` loader per `SDKWORK_WEBSERVER_SPEC.md`.
//!
//! Loads `deployments/webserver/server.common.toml`, the lifecycle file
//! (`server.<environment>.toml`), and the profile file
//! (`server.standalone.toml` / `server.cloud.toml`), applies the standard
//! three-layer inheritance merge, and materializes the effective configuration
//! into the runtime `WebServerAppConfig` model.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

use super::{
    error::ConfigDiagnostic,
    error::WebServerConfigError,
    map_variables::{validate_map_definitions, validate_map_source},
    model::CachePolicyConfig,
    model::MapConfig,
    model::MapEntryConfig,
    model::TlsVersion,
    model::WebServerAppConfig,
    network::hostname_upstream_allowed_cidrs,
    proxy_headers::merge_proxy_set_headers,
    validate_webserver_config,
};
use crate::nginx::{parse_nginx_config, NginxDirective};

/// Identity keys for object-array merge (spec section 2.3 rule 4).
fn identity_key(path: &str) -> Option<&'static str> {
    match path {
        "http.server" => Some("serverName"),
        "http.upstream" => Some("name"),
        "http.server.location" => Some("match"),
        "stream.server" => Some("listen"),
        "mail.server" => Some("listen"),
        _ => None,
    }
}

fn identity_of(element: &Value, key: &str) -> Option<String> {
    match key {
        "serverName" | "listen" => element
            .get(key)
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => element.get(key).and_then(Value::as_str).map(str::to_owned),
    }
}

fn is_plain_object(value: &Value) -> bool {
    value.is_object()
}

fn merge_value(base: &Value, overlay: &Value, path: &str) -> Value {
    if overlay.is_null() {
        return base.clone();
    }
    if base.is_null() {
        return overlay.clone();
    }
    if is_plain_object(base) && is_plain_object(overlay) {
        let mut out = base.as_object().cloned().unwrap_or_default();
        for (key, value) in overlay.as_object().expect("checked object") {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            let merged = merge_value(out.get(key).unwrap_or(&Value::Null), value, &child_path);
            out.insert(key.clone(), merged);
        }
        return Value::Object(out);
    }
    if base.is_array() && overlay.is_array() {
        return merge_arrays(
            base.as_array().expect("checked array"),
            overlay.as_array().expect("checked array"),
            path,
        );
    }
    // Scalars and type mismatches: the overlay wins (spec rule 1).
    overlay.clone()
}

fn merge_arrays(base: &[Value], overlay: &[Value], path: &str) -> Value {
    let Some(identity) = identity_key(path) else {
        // Leaf arrays (listen, serverName, protocols, ...) are replaced
        // wholesale; [[http.upstream.target]] has no identity entry and is
        // therefore also replaced (spec rule 5).
        return Value::Array(overlay.to_vec());
    };
    let mut out: Vec<Value> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for item in base {
        if is_plain_object(item) {
            if let Some(id) = identity_of(item, identity) {
                if !ids.contains(&id) {
                    ids.push(id.clone());
                    out.push(item.clone());
                }
            } else {
                out.push(item.clone());
            }
        } else {
            out.push(item.clone());
        }
    }
    for item in overlay {
        if is_plain_object(item) {
            if let Some(id) = identity_of(item, identity) {
                if let Some(index) = ids.iter().position(|existing| existing == &id) {
                    out[index] = merge_value(&out[index], item, path);
                } else {
                    ids.push(id.clone());
                    out.push(item.clone());
                }
            } else {
                out.push(item.clone());
            }
        } else {
            out.push(item.clone());
        }
    }
    Value::Array(out)
}

/// Effective configuration for one profile overlay (spec section 2.2).
pub fn merge_common_profile(
    common: &Value,
    profile: &Value,
) -> Result<Value, WebServerConfigError> {
    let mut overlay = profile.clone();
    if let Some(obj) = overlay.as_object_mut() {
        obj.remove("profile");
        obj.remove("environment");
    }
    Ok(merge_value(common, &overlay, ""))
}

/// effective(profile, environment) = merge(common, environment, profile)
pub fn merge_effective(
    common: &Value,
    environment: &Value,
    profile: &Value,
) -> Result<Value, WebServerConfigError> {
    let after_environment = merge_common_profile(common, environment)?;
    merge_common_profile(&after_environment, profile)
}

/// Merge one effective document over another with the standard layout v3
/// merge semantics (scalar override, leaf-array replacement, identity-key
/// upsert for object arrays, wholesale upstream target replacement). Used
/// to assemble the merged module-imports data plane.
pub fn merge_overlay(base: &Value, overlay: &Value) -> Result<Value, WebServerConfigError> {
    Ok(merge_value(base, overlay, ""))
}

fn parse_toml_file(path: &Path) -> Result<Value, WebServerConfigError> {
    let text = crate::fs_bounds::read_text_capped(path, crate::fs_bounds::MAX_IMPORT_SOURCE_BYTES)
        .map_err(|source| WebServerConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|source| WebServerConfigError::Toml {
            path: path.to_path_buf(),
            source,
        })?;
    serde_json::to_value(&value).map_err(|source| {
        WebServerConfigError::Materialize(format!(
            "cannot convert TOML from {}: {source}",
            path.display()
        ))
    })
}

fn materialize_error(path: &str, message: impl std::fmt::Display) -> WebServerConfigError {
    WebServerConfigError::Materialize(format!("{path}: {message}"))
}

fn materialize_static_resource(
    resource_id: &str,
    configured_root: &str,
    location: &Map<String, Value>,
    strip_prefix: bool,
) -> Value {
    let root = configured_root.trim_start_matches('/');
    let index_files: Vec<Value> = location
        .get("index")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|s| Value::String(s.to_owned()))
                .collect()
        })
        .unwrap_or_else(|| vec![Value::String("index.html".to_owned())]);
    let mut resource = json!({
        "id": resource_id,
        "type": "static",
        "root": root,
        "indexFiles": index_files,
        // nginx semantics: `root` appends the full request path, `alias`
        // replaces the matched location prefix with the configured value.
        "stripPrefix": strip_prefix,
    });
    if let Some(try_files) = location.get("tryFiles").and_then(Value::as_array) {
        if let Some(last) = try_files.iter().filter_map(Value::as_str).next_back() {
            if last.starts_with('/') {
                resource["spaFallback"] = Value::String(last.trim_start_matches('/').to_owned());
            }
        }
    }
    resource
}

fn as_object<'a>(
    value: &'a Value,
    path: &str,
) -> Result<&'a Map<String, Value>, WebServerConfigError> {
    value
        .as_object()
        .ok_or_else(|| materialize_error(path, "expected a TOML table"))
}

fn as_str<'a>(
    value: &'a Map<String, Value>,
    path: &str,
    key: &str,
) -> Result<&'a str, WebServerConfigError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| materialize_error(path, format!("`{key}` must be a string")))
}

fn as_array<'a>(
    value: &'a Map<String, Value>,
    path: &str,
    key: &str,
) -> Result<&'a Vec<Value>, WebServerConfigError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| materialize_error(path, format!("`{key}` must be an array")))
}

/// Directives accepted in typed TOML whose knobs are not separately executed
/// by the SDKWork data plane (safe defaults / process-owned behavior apply).
/// Location-level keys that *do* execute (for example `proxySetHeader`,
/// `rewrite`, `allow`/`deny`, `limitReq`) must appear in that context's
/// supported-key list instead of relying on this ignore set.
const ACCEPTED_IGNORED: &[&str] = &[
    // process / http tuning
    "user",
    "workerProcesses",
    "workerRlimitNofile",
    "pid",
    "errorLog",
    "include",
    "raw",
    "workerConnections",
    "use",
    "acceptMutex",
    "multiAccept",
    "sendfile",
    "tcpNopush",
    "tcpNodelay",
    "keepaliveTimeout",
    "keepaliveRequests",
    "clientBodyTimeout",
    "clientHeaderTimeout",
    "clientBodyBufferSize",
    "clientHeaderBufferSize",
    "largeClientHeaderBuffers",
    "resetTimedoutConnection",
    "sendTimeout",
    "serverNamesHashMaxSize",
    "serverTokens",
    "logFormat",
    "accessLog",
    "map",
    "keepalive",
    // proxy knobs accepted at non-location contexts; location `proxySetHeader`
    // and websocket/buffering flags are listed in location supported keys and execute
    "proxyHttpVersion",
    "proxyBuffering",
    "proxyWebsocketUpgrade",
    "proxyInterceptErrors",
    "proxyNextUpstream",
    "proxyHideHeader",
    "proxyRequestBuffering",
    // upstream server flags the runtime owns via its own failure/ejection
    // policy (nginx `max_fails=` / `fail_timeout=` are accepted no-ops)
    "maxFails",
    "failTimeout",
    // `resolve` marks DNS-named targets for deploy-time cluster resolution;
    // the runtime always resolves hostnames at connection time.
    "resolve",
    // static file details
    "autoindex",
    "disableSymlinks",
    "logNotFound",
    "sendfileMaxChunk",
    "charset",
    "errorPage",
    // TLS tuning (the runtime enforces TLSv1.2/1.3 defaults)
    "ocspStapling",
    "preferServerCiphers",
    "sessionCache",
    "sessionTimeout",
    "sessionTickets",
    "stapling",
    "staplingVerify",
    "verifyDepth",
    "dhparam",
    "ecdhCurve",
    "ciphers",
];

/// Directives that change security semantics; declaring them in server.toml
/// fails closed because the runtime cannot enforce them.
const UNSUPPORTED_SECURITY: &[&str] = &[];

/// Typed-TOML keys that are accepted for nginx-parallel configuration but that
/// the runtime cannot enforce *and* whose ignored value changes client-visible
/// behaviour.
///
/// PRD-nginx-compatibility §11 forbids silent acceptance: a key whose value an
/// operator expects to take effect must surface a diagnostic instead of being
/// dropped. The nginx-conf import plane already reports its accepted-ignored
/// directives (`mapping::ignored_directive_diagnostics`); this table is the
/// equivalent for the typed TOML plane, and covers exactly the keys where
/// "accepted" would otherwise read as "enforced" — TLS cipher/session policy,
/// OCSP stapling, response-header filtering, retry/health policy, buffering,
/// MIME/charset, symlink policy, directory listing and captured `map` variables.
///
/// Pure process/OS tuning (`workerProcesses`, `sendfile`, `tcpNopush`,
/// `clientBodyTimeout`, hash-table sizes, log formats, …) is deliberately absent:
/// the runtime owns those knobs by design and flagging them on every load would
/// bury the behaviour-affecting warnings in noise.
///
/// The second element is the operator-facing consequence, not a restatement of
/// the key name.
const UNENFORCED_BEHAVIOR_TOML_KEYS: &[(&str, &str)] = &[
    (
        "map",
        "the legacy string-array `map` key is not executed; declare `[[http.maps]]` entries (source/result/entries) for an executable map",
    ),
    (
        "errorPage",
        "error-page mapping is not materialized from this key; declare typed errorPages entries",
    ),
    (
        "proxyInterceptErrors",
        "upstream error interception is not materialized from this key; the upstream status is relayed",
    ),
    (
        "autoindex",
        "directory listings are not generated by the data plane (404 for a directory without an index file)",
    ),
    (
        "disableSymlinks",
        "symlink policy is enforced by the configured static root, not by this key",
    ),
    (
        "logNotFound",
        "missing-file logging follows the runtime log policy",
    ),
    (
        "sendfileMaxChunk",
        "static files are streamed by the runtime in bounded chunks",
    ),
    (
        "charset",
        "response charset is not rewritten from this key",
    ),
    (
        "proxyBuffering",
        "proxy responses are always streamed; buffering cannot be enabled by this key",
    ),
    (
        "proxyConnectTimeout",
        "upstream connect timeouts follow the resolved upstream policy, not this location key",
    ),
    (
        "proxyReadTimeout",
        "upstream response timeouts follow the resolved upstream policy, not this location key",
    ),
    (
        "proxySendTimeout",
        "upstream request timeouts follow the resolved upstream policy, not this location key",
    ),
    (
        "proxyRequestBuffering",
        "request bodies are streamed to the upstream, never buffered by this key",
    ),
    (
        "proxyNextUpstream",
        "retry eligibility follows the runtime bounded retry policy, not this key",
    ),
    (
        "proxyHideHeader",
        "upstream response headers are filtered by the runtime hop-by-hop policy, not this key",
    ),
    (
        "proxyWebsocketUpgrade",
        "WebSocket upgrades follow the runtime upgrade policy, not this key",
    ),
    (
        "proxyHttpVersion",
        "the upstream protocol is negotiated by the runtime, not this key",
    ),
    (
        "maxFails",
        "upstream ejection follows the runtime passive/active health policy, not this key",
    ),
    (
        "failTimeout",
        "upstream ejection follows the runtime passive/active health policy, not this key",
    ),
    (
        "resolve",
        "upstream hostnames are resolved at connection time; this key adds no effect",
    ),
    (
        "serverTokens",
        "the runtime never emits an nginx version banner, so the declared value has no effect",
    ),
    (
        "ocspStapling",
        "OCSP stapling is not implemented: no OCSP response is stapled to the certificate",
    ),
    (
        "stapling",
        "OCSP stapling is not implemented: no OCSP response is stapled to the certificate",
    ),
    (
        "staplingVerify",
        "OCSP stapling is not implemented, so response verification has no effect",
    ),
    (
        "ciphers",
        "cipher suites follow the runtime policy-safe TLS defaults, not this key",
    ),
    (
        "preferServerCiphers",
        "server cipher preference is owned by the runtime TLS policy, not this key",
    ),
    (
        "sessionCache",
        "TLS session resumption is owned by the runtime TLS policy, not this key",
    ),
    (
        "sessionTimeout",
        "TLS session resumption is owned by the runtime TLS policy, not this key",
    ),
    (
        "sessionTickets",
        "TLS session tickets are owned by the runtime TLS policy, not this key",
    ),
    (
        "verifyDepth",
        "client-certificate chain depth follows the runtime verifier policy, not this key",
    ),
    (
        "dhparam",
        "the runtime TLS stack uses its own key-exchange groups, not this key",
    ),
    (
        "ecdhCurve",
        "the runtime TLS stack uses its own key-exchange groups, not this key",
    ),
];

/// Collect one diagnostic per accepted-but-unenforced behaviour key found in an
/// effective layout-v3 TOML document. Paths are JSON-pointed at the authored
/// key (`server.toml.http.certificates.<name>.ocspStapling`, …) so the operator
/// can locate the value in the source files.
pub fn accepted_unenforced_toml_diagnostics(effective: &Value) -> Vec<ConfigDiagnostic> {
    let mut diagnostics = Vec::new();
    collect_unenforced_toml_diagnostics(effective, "server.toml", &mut diagnostics);
    diagnostics
}

fn collect_unenforced_toml_diagnostics(
    value: &Value,
    path: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    match value {
        Value::Object(table) => {
            for (key, child) in table {
                let child_path = format!("{path}.{key}");
                if let Some((_, consequence)) = UNENFORCED_BEHAVIOR_TOML_KEYS
                    .iter()
                    .find(|(name, _)| name == key)
                {
                    diagnostics.push(ConfigDiagnostic::new(
                        child_path.clone(),
                        format!(
                            "`{key}` is accepted for nginx compatibility but is not enforced by \
                             the SDKWork runtime: {consequence}"
                        ),
                    ));
                }
                collect_unenforced_toml_diagnostics(child, &child_path, diagnostics);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_unenforced_toml_diagnostics(item, &format!("{path}[{index}]"), diagnostics);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

const SERVER_TLS_KEYS: &[&str] = &[
    "cert",
    "certFile",
    "certKeyFile",
    "chainFile",
    "ocspStaplingFile",
    "protocols",
    "ciphers",
    "preferServerCiphers",
    "sessionCache",
    "sessionTimeout",
    "sessionTickets",
    "stapling",
    "staplingVerify",
    "clientCertificate",
    "clientCertificateCA",
    "verifyDepth",
    "dhparam",
    "ecdhCurve",
    "raw",
];

/// Resolve `[http.server.tls] protocols` (or `[http.defaults.tls] protocols`)
/// into the TLS policy's inclusive version window.
///
/// `SDKWORK_WEBSERVER_SPEC.md` §10 maps the key onto nginx `ssl_protocols`
/// with the default `["TLSv1.2", "TLSv1.3"]`; the config schema restricts the
/// tokens to exactly those two. `PRD-https-and-certificates.md` §2 lets an
/// application pick "a stricter compatible policy", so a singleton list is
/// legal (`["TLSv1.3"]` = TLS 1.3 only). The platform floor is not
/// negotiable: no token below TLS 1.2 exists, so a declaration can never
/// lower it. An absent key keeps the platform default window.
fn tls_window_from_protocols(
    tls: Option<&Map<String, Value>>,
    path: &str,
) -> Result<(TlsVersion, TlsVersion), WebServerConfigError> {
    let Some(protocols) = tls.and_then(|tls| tls.get("protocols")) else {
        return Ok(TlsVersion::PLATFORM_DEFAULT_WINDOW);
    };
    let Some(entries) = protocols.as_array() else {
        return Err(materialize_error(
            path,
            "`protocols` must be an array of `TLSv1.2` / `TLSv1.3`",
        ));
    };
    let mut minimum: Option<TlsVersion> = None;
    let mut maximum: Option<TlsVersion> = None;
    for entry in entries {
        let Some(token) = entry.as_str() else {
            return Err(materialize_error(
                path,
                "`protocols` entries must be `TLSv1.2` or `TLSv1.3` strings",
            ));
        };
        let version = match token {
            "TLSv1.2" => TlsVersion::Tls12,
            "TLSv1.3" => TlsVersion::Tls13,
            other => {
                return Err(materialize_error(
                    path,
                    format!("`protocols` accepts TLSv1.2|TLSv1.3, found `{other}`"),
                ))
            }
        };
        minimum = Some(minimum.map_or(version, |current| current.min(version)));
        maximum = Some(maximum.map_or(version, |current| current.max(version)));
    }
    let (Some(minimum), Some(maximum)) = (minimum, maximum) else {
        return Err(materialize_error(
            path,
            "`protocols` must list at least one of TLSv1.2|TLSv1.3",
        ));
    };
    Ok((minimum, maximum))
}

/// Human-readable rendering of a resolved window for conflict diagnostics.
fn describe_tls_window(window: (TlsVersion, TlsVersion)) -> String {
    let (minimum, maximum) = window;
    if minimum == maximum {
        minimum.wire().to_owned()
    } else {
        format!("{}..{}", minimum.wire(), maximum.wire())
    }
}

fn check_supported_keys(
    table: &Map<String, Value>,
    path: &str,
    supported: &[&str],
) -> Result<(), WebServerConfigError> {
    for key in table.keys() {
        if supported.contains(&key.as_str()) {
            continue;
        }
        if ACCEPTED_IGNORED.contains(&key.as_str()) {
            continue;
        }
        if UNSUPPORTED_SECURITY.contains(&key.as_str()) {
            return Err(materialize_error(
                path,
                format!(
                    "`{key}` cannot be enforced by the SDKWork runtime; remove it (see specs/nginx-gap.catalog.json)"
                ),
            ));
        }
        return Err(materialize_error(
            path,
            format!("unknown or unsupported key `{key}` for the runtime model"),
        ));
    }
    Ok(())
}

/// Parse a listen entry like `"443 ssl"`, `"80"`, or `"127.0.0.1:8088"`.
fn parse_listen(
    entry: &str,
    path: &str,
) -> Result<(String, u16, bool, bool), WebServerConfigError> {
    let mut parts = entry.split_whitespace();
    let address = parts
        .next()
        .ok_or_else(|| materialize_error(path, "empty listen entry"))?;
    let mut ssl = false;
    let mut http2 = false;
    for flag in parts {
        match flag {
            "ssl" => ssl = true,
            "http2" => http2 = true,
            // OS/selection tuning owned by the runtime: `default_server`
            // (the runtime's default-host selection covers it) and
            // `reuseport` (socket tuning) are accepted and ignored.
            "default_server" | "reuseport" => {}
            other => {
                return Err(materialize_error(
                    path,
                    format!("unsupported listen flag `{other}` in `{entry}`"),
                ))
            }
        }
    }
    let (bind, port) = if let Some((host, _)) = address.rsplit_once(':') {
        // The numeric port parse is shared so this cannot drift from the other
        // listeners; the nginx-specific bare-port fallback and IPv6 bracket
        // stripping below stay local to this directive grammar.
        let port: u16 = sdkwork_utils_rust::service_base_url::bind_port(address)
            .ok_or_else(|| materialize_error(path, format!("invalid listen port in `{entry}`")))?;
        if host.starts_with('[') && host.ends_with(']') {
            (host[1..host.len() - 1].to_owned(), port)
        } else {
            (host.to_owned(), port)
        }
    } else {
        let port: u16 = address
            .parse()
            .map_err(|_| materialize_error(path, format!("invalid listen value `{entry}`")))?;
        ("0.0.0.0".to_owned(), port)
    };
    Ok((bind, port, ssl, http2))
}

fn listener_id(bind: &str, port: u16) -> String {
    format!("listener-{bind}-{port}")
}

/// Parse an nginx-style duration (`60s`, `5m`, `1h`, `500ms`) into
/// milliseconds. Returns `None` for malformed or zero values.
/// True when the address is a routable public IP (not loopback, private,
/// link-local, shared CGNAT, IPv6 ULA, documentation, or multicast). Used to
/// fail closed on stream listeners that would expose a public interface
/// without approval.
fn is_public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            !v4.is_loopback()
                && !v4.is_private()
                && !v4.is_link_local()
                && !v4.is_unspecified()
                && !v4.is_broadcast()
                && !v4.is_documentation()
        }
        std::net::IpAddr::V6(v6) => {
            !v6.is_loopback() && !v6.is_unique_local() && !v6.is_unspecified() && !v6.is_multicast()
        }
    }
}

fn materialize_duration_ms(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    let (number, multiplier) = if let Some(rest) = trimmed.strip_suffix("ms") {
        (rest, 1u64)
    } else if let Some(rest) = trimmed.strip_suffix('s') {
        (rest, 1_000)
    } else if let Some(rest) = trimmed.strip_suffix('m') {
        (rest, 60_000)
    } else if let Some(rest) = trimmed.strip_suffix('h') {
        (rest, 3_600_000)
    } else {
        (trimmed, 1_000)
    };
    let parsed = number.trim().parse::<u64>().ok()?;
    if parsed == 0 {
        return None;
    }
    parsed.checked_mul(multiplier)
}

/// Validate one `proxySetHeader = "Name value"` entry for runtime execution.
fn validate_proxy_set_header_entry(path: &str, entry: &str) -> Result<(), WebServerConfigError> {
    crate::config::proxy_headers::validate_proxy_set_header_entry(entry)
        .map_err(|message| materialize_error(path, message))
}

/// Parse a mail client buffer size (`"4k"`, `"8k"`, or a byte count).
fn materialize_mail_buffer(value: &Value, path: &str) -> Result<usize, WebServerConfigError> {
    let bytes = match value {
        Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| materialize_error(path, "buffer size must be a positive byte count"))?,
        Value::String(text) => materialize_size_bytes(text),
        _ => {
            return Err(materialize_error(
                path,
                "buffer size must be a size string (e.g. \"4k\") or a byte count",
            ))
        }
    };
    if !(512..=1024 * 1024).contains(&bytes) {
        return Err(materialize_error(
            path,
            "buffer size must be between 512 bytes and 1 MiB",
        ));
    }
    Ok(bytes as usize)
}

/// Parse a typed-TOML `[mail.server.authHttp]` table into the runtime model.
fn materialize_mail_auth_http(value: &Value, path: &str) -> Result<Value, WebServerConfigError> {
    let table = value
        .as_object()
        .ok_or_else(|| materialize_error(path, "authHttp must be a table"))?;
    check_supported_keys(
        table,
        path,
        &[
            "uri",
            "hostHeader",
            "headers",
            "timeoutMs",
            "passClientCert",
        ],
    )?;
    let uri = table
        .get("uri")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|uri| !uri.is_empty())
        .ok_or_else(|| materialize_error(&format!("{path}.uri"), "`uri` is required"))?;
    let uri = uri.strip_prefix("http://").unwrap_or(uri);
    let mut out = json!({ "uri": uri });
    if let Some(host_header) = table.get("hostHeader") {
        out["hostHeader"] = host_header.clone();
    }
    if let Some(headers) = table.get("headers") {
        let headers = headers
            .as_array()
            .ok_or_else(|| materialize_error(&format!("{path}.headers"), "must be an array"))?;
        let mut entries = Vec::with_capacity(headers.len());
        for (index, header) in headers.iter().enumerate() {
            let header = header.as_str().ok_or_else(|| {
                materialize_error(
                    &format!("{path}.headers[{index}]"),
                    "auth_http_header entries must be `Name: value` strings",
                )
            })?;
            let valid = header.split_once(':').is_some_and(|(name, value)| {
                !name.trim().is_empty() && !value.contains(['\r', '\n'])
            });
            if !valid {
                return Err(materialize_error(
                    &format!("{path}.headers[{index}]"),
                    "auth_http_header entries must be `Name: value` on one line",
                ));
            }
            entries.push(Value::String(header.to_owned()));
        }
        out["headers"] = Value::Array(entries);
    }
    if let Some(timeout) = table.get("timeoutMs") {
        let millis = timeout.as_u64().ok_or_else(|| {
            materialize_error(&format!("{path}.timeoutMs"), "must be a millisecond count")
        })?;
        if !(1..=600_000).contains(&millis) {
            return Err(materialize_error(
                &format!("{path}.timeoutMs"),
                "auth_http_timeout must be between 1 ms and 10 minutes",
            ));
        }
        out["timeoutMs"] = Value::from(millis);
    }
    if let Some(pass_client_cert) = table.get("passClientCert") {
        out["passClientCert"] = Value::Bool(pass_client_cert.as_bool().unwrap_or(false));
    }
    Ok(out)
}

/// Parse a typed-TOML `[mail.server.tls]` table.
fn materialize_mail_tls(value: &Value, path: &str) -> Result<Value, WebServerConfigError> {
    let table = value
        .as_object()
        .ok_or_else(|| materialize_error(path, "tls must be a table"))?;
    check_supported_keys(
        table,
        path,
        &[
            "certificateFile",
            "privateKeyFile",
            "protocols",
            "starttls",
            "implicit",
        ],
    )?;
    let certificate_file = table
        .get("certificateFile")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|file| !file.is_empty())
        .ok_or_else(|| {
            materialize_error(
                &format!("{path}.certificateFile"),
                "`certificateFile` is required",
            )
        })?;
    let private_key_file = table
        .get("privateKeyFile")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|file| !file.is_empty())
        .ok_or_else(|| {
            materialize_error(
                &format!("{path}.privateKeyFile"),
                "`privateKeyFile` is required",
            )
        })?;
    let mut out = json!({
        "certificateFile": certificate_file,
        "privateKeyFile": private_key_file,
    });
    if let Some(protocols) = table.get("protocols") {
        let protocols = protocols
            .as_array()
            .ok_or_else(|| materialize_error(&format!("{path}.protocols"), "must be an array"))?;
        let mut tokens = Vec::with_capacity(protocols.len());
        for (index, token) in protocols.iter().enumerate() {
            let token = token.as_str().ok_or_else(|| {
                materialize_error(&format!("{path}.protocols[{index}]"), "must be a string")
            })?;
            if !matches!(token, "TLSv1.2" | "TLSv1.3") {
                return Err(materialize_error(
                    &format!("{path}.protocols[{index}]"),
                    format!("protocol `{token}` is not executable; TLSv1.2/TLSv1.3 only"),
                ));
            }
            tokens.push(Value::String(token.to_owned()));
        }
        if tokens.is_empty() {
            return Err(materialize_error(
                &format!("{path}.protocols"),
                "at least one TLS protocol is required",
            ));
        }
        out["protocols"] = Value::Array(tokens);
    }
    if let Some(starttls) = table.get("starttls") {
        let mode = starttls
            .as_str()
            .ok_or_else(|| materialize_error(&format!("{path}.starttls"), "must be a string"))?;
        if !matches!(mode, "off" | "on" | "only") {
            return Err(materialize_error(
                &format!("{path}.starttls"),
                "`starttls` accepts off|on|only",
            ));
        }
        out["starttls"] = Value::String(mode.to_owned());
    }
    Ok(out)
}

/// Parse a typed-TOML `[mail.server.proxy]` table (backend relay policy).
fn materialize_mail_proxy(value: &Value, path: &str) -> Result<Value, WebServerConfigError> {
    let table = value
        .as_object()
        .ok_or_else(|| materialize_error(path, "proxy must be a table"))?;
    check_supported_keys(
        table,
        path,
        &[
            "enabled",
            "xclient",
            "smtpAuth",
            "proxyProtocol",
            "passErrorMessage",
            "bufferBytes",
            "timeoutMs",
            "tls",
        ],
    )?;
    let mut out = json!({});
    for (key, field) in [
        ("enabled", "enabled"),
        ("xclient", "xclient"),
        ("smtpAuth", "smtpAuth"),
        ("proxyProtocol", "proxyProtocol"),
        ("passErrorMessage", "passErrorMessage"),
    ] {
        if let Some(flag) = table.get(key) {
            let flag = flag
                .as_bool()
                .ok_or_else(|| materialize_error(&format!("{path}.{key}"), "must be a boolean"))?;
            out[field] = Value::Bool(flag);
        }
    }
    if let Some(buffer) = table.get("bufferBytes") {
        let bytes = buffer.as_u64().ok_or_else(|| {
            materialize_error(&format!("{path}.bufferBytes"), "must be a byte count")
        })?;
        if !(512..=1024 * 1024).contains(&bytes) {
            return Err(materialize_error(
                &format!("{path}.bufferBytes"),
                "proxy_buffer must be between 512 bytes and 1 MiB",
            ));
        }
        out["bufferBytes"] = Value::from(bytes);
    }
    if let Some(timeout) = table.get("timeoutMs") {
        let millis = timeout.as_u64().ok_or_else(|| {
            materialize_error(&format!("{path}.timeoutMs"), "must be a millisecond count")
        })?;
        if !(1..=86_400_000).contains(&millis) {
            return Err(materialize_error(
                &format!("{path}.timeoutMs"),
                "proxy_timeout must be between 1 ms and 24 hours",
            ));
        }
        out["timeoutMs"] = Value::from(millis);
    }
    if let Some(tls) = table.get("tls") {
        let tls_table = tls
            .as_object()
            .ok_or_else(|| materialize_error(&format!("{path}.tls"), "must be a table"))?;
        check_supported_keys(
            tls_table,
            &format!("{path}.tls"),
            &[
                "verify",
                "trustedCertificateFile",
                "certificateFile",
                "certificateKeyFile",
                "serverName",
            ],
        )?;
        let mut proxy_tls = json!({});
        if let Some(verify) = tls_table.get("verify") {
            proxy_tls["verify"] = Value::Bool(verify.as_bool().unwrap_or(true));
        }
        for (key, field) in [
            ("trustedCertificateFile", "trustedCertificateFile"),
            ("certificateFile", "certificateFile"),
            ("certificateKeyFile", "certificateKeyFile"),
            ("serverName", "serverName"),
        ] {
            if let Some(file) = tls_table.get(key) {
                let file = file
                    .as_str()
                    .map(str::trim)
                    .filter(|file| !file.is_empty())
                    .ok_or_else(|| {
                        materialize_error(&format!("{path}.tls.{key}"), "must be a non-empty path")
                    })?;
                proxy_tls[field] = Value::String(file.to_owned());
            }
        }
        out["tls"] = proxy_tls;
    }
    Ok(out)
}

/// Parse a `pop3Auth` / `imapAuth` / `smtpAuth` mechanism list.
///
/// Wave 9A parses PLAIN and LOGIN only. nginx also accepts `apop`,
/// `cram-md5`, and `external`; those need the `auth_http` credential exchange
/// (Wave 9B) and are refused with a located diagnostic instead of a silent
/// downgrade.
fn materialize_mail_auth(
    server: &Map<String, Value>,
    path: &str,
    key: &str,
) -> Result<Option<Vec<crate::config::model::MailAuthMechanism>>, WebServerConfigError> {
    let Some(list) = server.get(key) else {
        return Ok(None);
    };
    let path = format!("{path}.{key}");
    let entries = list
        .as_array()
        .ok_or_else(|| materialize_error(&path, "auth mechanisms must be an array"))?;
    if entries.is_empty() {
        return Err(materialize_error(
            &path,
            "at least one auth mechanism must be enabled",
        ));
    }
    let mut mechanisms = Vec::with_capacity(entries.len());
    for entry in entries {
        let token = entry
            .as_str()
            .ok_or_else(|| materialize_error(&path, "auth mechanisms must be strings"))?;
        let mechanism = match token.to_ascii_lowercase().as_str() {
            "plain" => crate::config::model::MailAuthMechanism::Plain,
            "login" => crate::config::model::MailAuthMechanism::Login,
            "apop" | "cram-md5" | "external" => {
                return Err(materialize_error(
                    &path,
                    format!(
                        "auth mechanism `{token}` requires the auth_http credential exchange (Wave 9B); Wave 9A parses PLAIN and LOGIN only"
                    ),
                ))
            }
            other => {
                return Err(materialize_error(
                    &path,
                    format!("unknown auth mechanism `{other}`; expected plain|login"),
                ))
            }
        };
        mechanisms.push(mechanism);
    }
    Ok(Some(mechanisms))
}

fn materialize_size_bytes(value: &str) -> u64 {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let (number, multiplier) = match trimmed.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&trimmed[..trimmed.len() - 1], 1024_u64),
        Some(b'm' | b'M') => (&trimmed[..trimmed.len() - 1], 1024_u64 * 1024),
        Some(b'g' | b'G') => (&trimmed[..trimmed.len() - 1], 1024_u64 * 1024 * 1024),
        _ => (trimmed, 1_u64),
    };
    number
        .parse::<u64>()
        .unwrap_or(0)
        .saturating_mul(multiplier)
}

fn parse_location_match<'a>(
    path: &str,
    match_value: &'a str,
) -> Result<(&'static str, &'a str), WebServerConfigError> {
    if let Some(rest) = match_value.strip_prefix("= ") {
        if !rest.starts_with('/') {
            return Err(materialize_error(
                path,
                "exact location match must start with `/`",
            ));
        }
        return Ok(("exact", rest));
    }
    if let Some(rest) = match_value.strip_prefix("^~ ") {
        if !rest.starts_with('/') {
            return Err(materialize_error(
                path,
                "prefix-exclusive (`^~`) location match must start with `/`",
            ));
        }
        return Ok(("prefix-exclusive", rest));
    }
    if let Some(rest) = match_value.strip_prefix("~* ") {
        regex::Regex::new(&format!("(?i){rest}")).map_err(|error| {
            materialize_error(
                path,
                format!("invalid case-insensitive regex location: {error}"),
            )
        })?;
        return Ok(("regex-ignore-case", rest));
    }
    if let Some(rest) = match_value.strip_prefix("~ ") {
        regex::Regex::new(rest)
            .map_err(|error| materialize_error(path, format!("invalid regex location: {error}")))?;
        return Ok(("regex", rest));
    }
    if !match_value.starts_with('/') {
        return Err(materialize_error(
            path,
            "prefix location match must start with `/`",
        ));
    }
    Ok(("prefix", match_value))
}

fn materialize_rewrite_rules(
    location: &Map<String, Value>,
    path: &str,
) -> Result<Vec<Value>, WebServerConfigError> {
    let Some(entries) = location.get("rewrite").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut rules = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let text = entry.as_str().ok_or_else(|| {
            materialize_error(
                &format!("{path}.rewrite[{index}]"),
                "rewrite entries must be strings",
            )
        })?;
        let rule = crate::config::parse_rewrite(text).map_err(|error| {
            materialize_error(&format!("{path}.rewrite[{index}]"), error.to_string())
        })?;
        rules.push(json!({
            "pattern": rule.pattern,
            "replacement": rule.replacement,
            "flag": match rule.flag {
                crate::config::RewriteFlag::Last => "last",
                crate::config::RewriteFlag::Break => "break",
                crate::config::RewriteFlag::Redirect => "redirect",
                crate::config::RewriteFlag::Permanent => "permanent",
            },
        }));
    }
    Ok(rules)
}

fn parse_client_auth(
    tls: &Map<String, Value>,
    path: &str,
) -> Result<Option<Value>, WebServerConfigError> {
    let mode_value = tls.get("clientCertificate");
    let ca = tls.get("clientCertificateCA").and_then(Value::as_str);
    let Some(mode_value) = mode_value else {
        if ca.is_some() {
            return Err(materialize_error(
                &format!("{path}.clientCertificateCA"),
                "`clientCertificateCA` requires `clientCertificate`",
            ));
        }
        return Ok(None);
    };
    let mode = match mode_value {
        Value::Bool(false) => "off",
        Value::Bool(true) => "on",
        Value::String(text) => text.trim(),
        _ => {
            return Err(materialize_error(
                &format!("{path}.clientCertificate"),
                "`clientCertificate` must be a string (`on`/`off`/`optional`) or boolean",
            ));
        }
    };
    let mode = match mode {
        "off" | "false" => return Ok(None),
        "on" | "true" => "required",
        "optional" => "optional",
        other => {
            return Err(materialize_error(
                &format!("{path}.clientCertificate"),
                format!("unsupported clientCertificate `{other}`; use on/off/optional"),
            ));
        }
    };
    let Some(ca) = ca.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(materialize_error(
            &format!("{path}.clientCertificate"),
            "`clientCertificate` on/optional requires `clientCertificateCA`",
        ));
    };
    if !(ca.starts_with('/')
        || (ca.len() >= 3
            && ca.as_bytes()[1] == b':'
            && (ca.as_bytes()[2] == b'\\' || ca.as_bytes()[2] == b'/'))
        || ca.starts_with("secret://"))
    {
        return Err(materialize_error(
            &format!("{path}.clientCertificateCA"),
            "`clientCertificateCA` must be an absolute path or secret:// reference",
        ));
    }
    if ca.starts_with("secret://") {
        return Err(materialize_error(
            &format!("{path}.clientCertificateCA"),
            "`secret://` clientCertificateCA resolution is not implemented; use an absolute PEM path",
        ));
    }
    Ok(Some(json!({
        "mode": mode,
        "caCertificateFiles": [ca],
    })))
}

fn parse_upstream_hash_key(hash_key: &str, path: &str) -> Result<Value, WebServerConfigError> {
    let mut parts = hash_key.split_whitespace();
    let key = parts
        .next()
        .ok_or_else(|| materialize_error(path, "`hashKey` must declare a key variable"))?;
    let consistent = match parts.next() {
        None => false,
        Some("consistent") => true,
        Some(other) => {
            return Err(materialize_error(
                path,
                format!("unsupported hashKey flag `{other}`; only `consistent` is allowed"),
            ));
        }
    };
    if parts.next().is_some() {
        return Err(materialize_error(
            path,
            "`hashKey` accepts at most one key and optional `consistent`",
        ));
    }
    let key = match key {
        "$request_uri" => "$request_uri",
        "$uri" => "$uri",
        "$remote_addr" => "$remote_addr",
        "$host" => "$host",
        other => {
            return Err(materialize_error(
                path,
                format!(
                    "unsupported hashKey `{other}`; executable keys are $request_uri, $uri, $remote_addr, $host"
                ),
            ));
        }
    };
    Ok(json!({
        "key": key,
        "consistent": consistent,
    }))
}

fn materialize_access_rules(
    location: &Map<String, Value>,
    path: &str,
) -> Result<Vec<Value>, WebServerConfigError> {
    let mut rules = Vec::new();
    for (action, key) in [("allow", "allow"), ("deny", "deny")] {
        let Some(entries) = location.get(key).and_then(Value::as_array) else {
            continue;
        };
        for (index, entry) in entries.iter().enumerate() {
            let network = entry.as_str().ok_or_else(|| {
                materialize_error(
                    &format!("{path}.{key}[{index}]"),
                    format!("`{key}` entries must be strings"),
                )
            })?;
            let trimmed = network.trim();
            if trimmed.is_empty() {
                return Err(materialize_error(
                    &format!("{path}.{key}[{index}]"),
                    format!("`{key}` entry must not be empty"),
                ));
            }
            if !trimmed.eq_ignore_ascii_case("all")
                && trimmed.parse::<std::net::IpAddr>().is_err()
                && trimmed.parse::<ipnet::IpNet>().is_err()
            {
                return Err(materialize_error(
                    &format!("{path}.{key}[{index}]"),
                    format!("`{key}` entry `{trimmed}` must be `all`, an IP, or a CIDR"),
                ));
            }
            rules.push(json!({ "action": action, "network": trimmed }));
        }
    }
    Ok(rules)
}

fn materialize_limit_conn_rules(
    location: &Map<String, Value>,
    path: &str,
    zone_names: &[String],
) -> Result<Vec<Value>, WebServerConfigError> {
    let Some(entries) = location.get("limitConn").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut rules = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let entry_path = format!("{path}.limitConn[{index}]");
        let text = entry
            .as_str()
            .ok_or_else(|| materialize_error(&entry_path, "limitConn entries must be strings"))?;
        let rule = crate::config::parse_limit_conn(text)
            .map_err(|error| materialize_error(&entry_path, error.to_string()))?;
        if !zone_names.contains(&rule.zone) {
            return Err(materialize_error(
                &entry_path,
                format!(
                    "limitConn zone `{}` is not declared in http.limitConnZone",
                    rule.zone
                ),
            ));
        }
        rules.push(json!({
            "zone": rule.zone,
            "maxConnections": rule.max_connections,
        }));
    }
    Ok(rules)
}

fn materialize_limit_req_rules(
    location: &Map<String, Value>,
    path: &str,
    zone_names: &[String],
) -> Result<Vec<Value>, WebServerConfigError> {
    let Some(entries) = location.get("limitReq").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut rules = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let entry_path = format!("{path}.limitReq[{index}]");
        let text = entry
            .as_str()
            .ok_or_else(|| materialize_error(&entry_path, "limitReq entries must be strings"))?;
        let rule = crate::config::parse_limit_req(text)
            .map_err(|error| materialize_error(&entry_path, error.to_string()))?;
        if !zone_names.contains(&rule.zone) {
            return Err(materialize_error(
                &entry_path,
                format!(
                    "limitReq zone `{}` is not declared in http.limitReqZone",
                    rule.zone
                ),
            ));
        }
        rules.push(json!({
            "zone": rule.zone,
            "burst": rule.burst,
            "nodelay": rule.nodelay,
        }));
    }
    Ok(rules)
}

/// Materialize the location `secureLink*` family (the TOML mirror of
/// nginx `secure_link_secret` / `secure_link` + `secure_link_md5` +
/// `secure_link_expires`).
fn materialize_secure_link(
    location: &Map<String, Value>,
    path: &str,
) -> Result<Option<Value>, WebServerConfigError> {
    let secret = location.get("secureLinkSecret").and_then(Value::as_str);
    let argument = location.get("secureLink").and_then(Value::as_str);
    let template = location.get("secureLinkMd5").and_then(Value::as_str);
    let expires = location.get("secureLinkExpires").and_then(Value::as_str);
    match (secret, argument, template, expires) {
        (Some(secret), None, None, None) => {
            if secret.is_empty() {
                return Err(materialize_error(
                    &format!("{path}.secureLinkSecret"),
                    "`secureLinkSecret` must not be empty",
                ));
            }
            Ok(Some(json!({ "mode": "secret", "secret": secret })))
        }
        (None, argument, template, expires) => {
            let Some(template) = template else {
                if argument.is_some() || expires.is_some() {
                    return Err(materialize_error(
                        path,
                        "`secureLink`/`secureLinkExpires` require `secureLinkMd5`",
                    ));
                }
                return Ok(None);
            };
            crate::config::validate_md5_template(template)
                .map_err(|message| materialize_error(&format!("{path}.secureLinkMd5"), message))?;
            for (key, value) in [("secureLink", argument), ("secureLinkExpires", expires)] {
                if let Some(value) = value {
                    if value.starts_with("$arg_") {
                        return Err(materialize_error(
                            &format!("{path}.{key}"),
                            format!("`{key}` takes the bare query argument name (nginx writes `$arg_<name>`; write `{value}` without the `$arg_` prefix)"),
                        ));
                    }
                    if value.trim().is_empty() {
                        return Err(materialize_error(
                            &format!("{path}.{key}"),
                            format!("`{key}` must not be empty"),
                        ));
                    }
                }
            }
            let mut mode = json!({
                "mode": "md5",
                "argument": argument.unwrap_or("st"),
                "template": template,
            });
            if let Some(expires) = expires {
                mode["expiresArgument"] = Value::String(expires.to_owned());
            }
            Ok(Some(mode))
        }
        (Some(_), _, _, _) => Err(materialize_error(
            path,
            "`secureLinkSecret` conflicts with `secureLink`/`secureLinkMd5`/`secureLinkExpires`",
        )),
    }
}

/// Materialize the location `subFilter` family into the model shape
/// (`subFilter` rule entries `"from to"`, `subFilterOnce`, `subFilterTypes`,
/// `subFilterLastModified` — the TOML mirror of nginx `sub_filter*`).
/// Typed-TOML counterpart of the nginx `expires` / `etag` directives.
///
/// `expires = "1d"` uses the same grammar as the nginx plane (the parser is
/// shared, so the two surfaces cannot drift); `etag = false` suppresses the
/// static-file entity tag. Both are location-scope keys per
/// SDKWORK_WEBSERVER_SPEC.md §11.1/§11.2.
fn materialize_cache_policy(
    location: &Map<String, Value>,
    path: &str,
) -> Result<Option<CachePolicyConfig>, WebServerConfigError> {
    let expires = location.get("expires");
    let etag = location.get("etag");
    if expires.is_none() && etag.is_none() {
        return Ok(None);
    }
    let mut policy = CachePolicyConfig::default();
    if let Some(value) = expires {
        let Some(text) = value.as_str() else {
            return Err(materialize_error(
                &format!("{path}.expires"),
                "`expires` must be a string such as \"1d\", \"max\", \"off\" or \"modified 1h\"",
            ));
        };
        // The TOML value is one string, so a `modified <time>` declaration
        // arrives space-separated; split it back into the nginx argument list.
        let arguments: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
        let (mode, seconds) = crate::config::cache_policy::parse_expires_arguments(&arguments)
            .map_err(|detail| {
                materialize_error(
                    &format!("{path}.expires"),
                    format!("invalid `expires`: {detail}"),
                )
            })?;
        policy.expires = mode;
        policy.expires_seconds = seconds;
    }
    if let Some(value) = etag {
        let Some(enabled) = value.as_bool() else {
            return Err(materialize_error(
                &format!("{path}.etag"),
                "`etag` must be a boolean",
            ));
        };
        policy.etag = enabled;
    }
    Ok(Some(policy))
}

/// `CachePolicyConfig` as it appears in the materialized JSON plane; kept in
/// step with the model's serde names by the nginx-plane round-trip test.
fn cache_policy_json(policy: &CachePolicyConfig) -> Value {
    json!({
        "expires": policy.expires.wire(),
        "expiresSeconds": policy.expires_seconds,
        "etag": policy.etag,
        "ifModifiedSince": policy.if_modified_since.wire(),
    })
}

fn materialize_sub_filter(
    location: &Map<String, Value>,
    path: &str,
) -> Result<Option<Value>, WebServerConfigError> {
    let Some(entries) = location.get("subFilter").and_then(Value::as_array) else {
        if location.contains_key("subFilterOnce")
            || location.contains_key("subFilterTypes")
            || location.contains_key("subFilterLastModified")
        {
            return Err(materialize_error(
                path,
                "`subFilterOnce`/`subFilterTypes`/`subFilterLastModified` require `subFilter`",
            ));
        }
        return Ok(None);
    };
    let mut rules = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let entry = entry.as_str().ok_or_else(|| {
            materialize_error(
                &format!("{path}.subFilter[{index}]"),
                "subFilter entries must be strings `\"from to\"`",
            )
        })?;
        let (from, to) = entry
            .split_once(char::is_whitespace)
            .map(|(from, to)| (from.to_owned(), to.to_owned()))
            .ok_or_else(|| {
                materialize_error(
                    &format!("{path}.subFilter[{index}]"),
                    "subFilter entry must be `\"from to\"` with a non-empty pattern",
                )
            })?;
        if from.is_empty() {
            return Err(materialize_error(
                &format!("{path}.subFilter[{index}]"),
                "subFilter pattern must not be empty",
            ));
        }
        rules.push(json!({ "from": from, "to": to }));
    }
    let types = location
        .get("subFilterTypes")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["text/html".to_owned()]);
    if types.is_empty() {
        return Err(materialize_error(
            &format!("{path}.subFilterTypes"),
            "`subFilterTypes` must not be empty",
        ));
    }
    Ok(Some(json!({
        "rules": rules,
        "once": location.get("subFilterOnce").and_then(Value::as_bool).unwrap_or(true),
        "types": types,
        "lastModified": location.get("subFilterLastModified").and_then(Value::as_bool).unwrap_or(false),
    })))
}

fn materialize_auth_basic(
    location: &Map<String, Value>,
    path: &str,
) -> Result<Option<Value>, WebServerConfigError> {
    let auth_basic = location.get("authBasic").and_then(Value::as_str);
    let user_file = location.get("authBasicUserFile").and_then(Value::as_str);
    match (auth_basic, user_file) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(materialize_error(
            &format!("{path}.authBasicUserFile"),
            "`authBasicUserFile` requires `authBasic`",
        )),
        (Some(realm), file) => {
            let realm = realm.trim();
            if realm.is_empty() {
                return Err(materialize_error(
                    &format!("{path}.authBasic"),
                    "`authBasic` must not be empty",
                ));
            }
            if realm.eq_ignore_ascii_case("off") {
                return Ok(None);
            }
            let Some(file) = file.map(str::trim).filter(|value| !value.is_empty()) else {
                return Err(materialize_error(
                    &format!("{path}.authBasic"),
                    "`authBasic` requires `authBasicUserFile`",
                ));
            };
            if !(file.starts_with('/')
                || (file.len() >= 3
                    && file.as_bytes()[1] == b':'
                    && (file.as_bytes()[2] == b'\\' || file.as_bytes()[2] == b'/'))
                || file.starts_with("secret://"))
            {
                return Err(materialize_error(
                    &format!("{path}.authBasicUserFile"),
                    "`authBasicUserFile` must be an absolute path or secret:// reference",
                ));
            }
            if file.starts_with("secret://") {
                return Err(materialize_error(
                    &format!("{path}.authBasicUserFile"),
                    "`secret://` authBasicUserFile resolution is not implemented; use an absolute htpasswd path",
                ));
            }
            let contents = crate::fs_bounds::read_text_capped(
                Path::new(file),
                crate::fs_bounds::MAX_IMPORT_SOURCE_BYTES,
            )
            .map_err(|error| {
                materialize_error(
                    &format!("{path}.authBasicUserFile"),
                    format!("failed to read htpasswd file `{file}`: {error}"),
                )
            })?;
            let users = crate::config::parse_htpasswd(&contents).map_err(|error| {
                materialize_error(&format!("{path}.authBasicUserFile"), error.to_string())
            })?;
            Ok(Some(json!({
                "realm": realm,
                "users": users.into_iter().map(|user| json!({
                    "username": user.username,
                    "passwordHash": user.password_hash,
                })).collect::<Vec<_>>(),
            })))
        }
    }
}

struct Materializer<'a> {
    app_key: &'a str,
    listeners: Vec<Value>,
    listeners_by_port: BTreeMap<(String, u16), (String, bool)>,
    certificates: Vec<Value>,
    certificate_names: Vec<String>,
    tls_policies: Vec<Value>,
    /// Version window each certificate's TLS policy was explicitly narrowed to
    /// by a server declaration. Certificates declared up front in
    /// `[http.certificates]` start on the platform default and are refined by
    /// the first server that names them; two servers that narrow the same
    /// certificate differently is a configuration error, not a silent
    /// last-wins (`SDKWORK_WEBSERVER_SPEC.md` §10, one policy per listener
    /// port).
    certificate_tls_windows: BTreeMap<String, (TlsVersion, TlsVersion)>,
    resources: Vec<Value>,
    upstreams: Vec<Value>,
    upstream_names: Vec<String>,
    virtual_hosts: Vec<Value>,
    streams: Vec<Value>,
    /// `[[mail.server]]` listeners materialized into the mail proxy plane.
    mail_servers: Vec<Value>,
    limit_req_zones: Vec<Value>,
    limit_req_zone_names: Vec<String>,
    limit_conn_zones: Vec<Value>,
    limit_conn_zone_names: Vec<String>,
    /// http-level `[[http.maps]]` declarations materialized into the executable
    /// `map` model (`WebServerAppConfig.maps`). The legacy string-array `map`
    /// key stays accepted-but-unenforced with its diagnostic.
    maps: Vec<Value>,
}

impl<'a> Materializer<'a> {
    fn new(app_key: &'a str) -> Self {
        Self {
            app_key,
            listeners: Vec::new(),
            listeners_by_port: BTreeMap::new(),
            certificates: Vec::new(),
            certificate_names: Vec::new(),
            tls_policies: Vec::new(),
            certificate_tls_windows: BTreeMap::new(),
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
            maps: Vec::new(),
        }
    }

    fn ensure_listener(
        &mut self,
        bind: &str,
        port: u16,
        ssl: bool,
        http2: bool,
        tls_policy_ref: Option<&str>,
        path: &str,
    ) -> Result<String, WebServerConfigError> {
        let key = (bind.to_owned(), port);
        let id = listener_id(bind, port);
        if let Some((existing_id, existing_ssl)) = self.listeners_by_port.get(&key) {
            let existing_id = existing_id.clone();
            if existing_ssl != &ssl {
                return Err(materialize_error(
                    path,
                    format!("port {port} is declared both with and without ssl; split listeners or unify TLS"),
                ));
            }
            if let Some(tls_policy) = tls_policy_ref {
                // Servers may share one TLS listener with different named
                // certificates (per-base-domain TLS split): the listener
                // policy collects every certificate for the port and the data
                // plane selects by SNI. Version/ALPN or client-auth policy
                // differences still fail closed.
                let current = self
                    .listeners
                    .iter()
                    .find(|l| l["id"] == *existing_id)
                    .and_then(|l| l.get("tlsPolicyRef"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(current) = current {
                    if current != tls_policy {
                        self.merge_listener_tls_policies(&current, tls_policy, port, http2, path)?;
                    }
                }
            }
            return Ok(existing_id.clone());
        }
        let mut protocols = vec!["http1".to_owned()];
        if http2 && ssl {
            protocols.push("http2".to_owned());
        }
        let mut listener = json!({
            "id": id,
            "bind": bind,
            "port": port,
            "protocols": protocols,
        });
        if !ssl {
            // An explicit plaintext listen in server.toml is the operator's
            // declared intent (nginx semantics); the runtime defaults to
            // fail-closed plaintext otherwise.
            listener["allowPlaintextHttp"] = Value::Bool(true);
        }
        if ssl {
            if let Some(policy) = tls_policy_ref {
                listener["tlsPolicyRef"] = Value::String(policy.to_owned());
                // The listener protocols are the operator's declared intent;
                // the policy ALPN must exactly match them (validate.rs).
                let alpn = if http2 {
                    json!(["h2", "http/1.1"])
                } else {
                    json!(["http/1.1"])
                };
                if let Some(existing) = self
                    .tls_policies
                    .iter_mut()
                    .find(|candidate| candidate.get("id").and_then(Value::as_str) == Some(policy))
                {
                    existing["alpn"] = alpn;
                }
            }
        }
        self.listeners.push(listener);
        self.listeners_by_port.insert(key, (id.clone(), ssl));
        Ok(id)
    }

    /// Merge the incoming certificate policy into the listener's existing
    /// policy so one SNI-terminated listener serves every certificate on the
    /// port (`SDKWORK_WEBSERVER_SPEC.md` §2.4 per-base-domain TLS split).
    /// HTTP/2 intent, TLS version, and client-auth policies must agree; they
    /// fail closed otherwise.
    fn merge_listener_tls_policies(
        &mut self,
        existing_id: &str,
        incoming_id: &str,
        port: u16,
        http2: bool,
        path: &str,
    ) -> Result<(), WebServerConfigError> {
        let listener_http2 = self
            .listeners
            .iter()
            .find(|listener| listener["id"] == existing_id)
            .and_then(|listener| listener.get("protocols"))
            .and_then(Value::as_array)
            .map(|protocols| {
                protocols
                    .iter()
                    .any(|protocol| protocol.as_str() == Some("http2"))
            })
            .unwrap_or(false);
        if listener_http2 != http2 {
            return Err(materialize_error(
                path,
                format!(
                    "port {port} is shared by servers with different TLS version/ALPN policies; use one certificate policy per listener port"
                ),
            ));
        }
        let existing = self
            .tls_policies
            .iter()
            .find(|policy| policy.get("id").and_then(Value::as_str) == Some(existing_id))
            .ok_or_else(|| {
                materialize_error(path, format!("TLS policy `{existing_id}` is missing"))
            })?;
        let incoming = self
            .tls_policies
            .iter()
            .find(|policy| policy.get("id").and_then(Value::as_str) == Some(incoming_id))
            .ok_or_else(|| {
                materialize_error(path, format!("TLS policy `{incoming_id}` is missing"))
            })?;
        for key in ["minimumVersion", "maximumVersion"] {
            if existing.get(key) != incoming.get(key) {
                return Err(materialize_error(
                    path,
                    format!(
                        "port {port} is shared by servers with different TLS version/ALPN policies; use one certificate policy per listener port"
                    ),
                ));
            }
        }
        if existing.get("clientAuth") != incoming.get("clientAuth") {
            return Err(materialize_error(
                path,
                format!(
                    "port {port} is shared by servers with different clientCertificate policies; use one client auth policy per listener port"
                ),
            ));
        }
        let incoming_refs = incoming
            .get("certificateRefs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if incoming_refs.is_empty() {
            return Ok(());
        }
        if let Some(existing) = self
            .tls_policies
            .iter_mut()
            .find(|policy| policy.get("id").and_then(Value::as_str) == Some(existing_id))
        {
            let refs = existing
                .get_mut("certificateRefs")
                .and_then(Value::as_array_mut)
                .expect("materialized TLS policy carries certificateRefs");
            for certificate_ref in incoming_refs {
                if !refs.iter().any(|value| value == &certificate_ref) {
                    refs.push(certificate_ref);
                }
            }
        }
        Ok(())
    }

    fn ensure_certificate(
        &mut self,
        name: &str,
        cert: &Map<String, Value>,
        server_names: &[String],
        path: &str,
        client_auth: Option<&Value>,
        tls_window: (TlsVersion, TlsVersion),
    ) -> Result<(), WebServerConfigError> {
        if let Some(index) = self
            .certificate_names
            .iter()
            .position(|existing| existing == name)
        {
            // The policy is per certificate, so every server naming the same
            // certificate must agree on the version window. A second server
            // that narrows it differently would make the listener's accepted
            // protocol set depend on which server block happened to match, so
            // that is refused rather than resolved last-wins.
            match self.certificate_tls_windows.get(name) {
                Some(existing) if *existing != tls_window => {
                    return Err(materialize_error(
                        path,
                        format!(
                            "certificate `{name}` is shared by servers with different TLS version policies ({} vs {}); use one `protocols` declaration per certificate",
                            describe_tls_window(*existing),
                            describe_tls_window(tls_window)
                        ),
                    ));
                }
                Some(_) => {}
                None => {
                    self.certificate_tls_windows
                        .insert(name.to_owned(), tls_window);
                    let policy_id = format!("tls-{name}");
                    if let Some(policy) = self.tls_policies.iter_mut().find(|policy| {
                        policy.get("id").and_then(Value::as_str) == Some(policy_id.as_str())
                    }) {
                        policy["minimumVersion"] = Value::String(tls_window.0.wire().to_owned());
                        policy["maximumVersion"] = Value::String(tls_window.1.wire().to_owned());
                    }
                }
            }
            if let Some(entry) = self.certificates.get_mut(index) {
                if let Some(names) = entry.get_mut("serverNames").and_then(Value::as_array_mut) {
                    for server_name in server_names {
                        if !names
                            .iter()
                            .any(|value| value.as_str() == Some(server_name.as_str()))
                        {
                            names.push(Value::String(server_name.clone()));
                        }
                    }
                }
            }
            let policy_id = format!("tls-{name}");
            if let Some(policy) = self
                .tls_policies
                .iter()
                .find(|policy| policy.get("id").and_then(Value::as_str) == Some(policy_id.as_str()))
            {
                let existing = policy.get("clientAuth");
                if existing != client_auth {
                    return Err(materialize_error(
                        path,
                        format!(
                            "certificate `{name}` is shared by servers with different clientCertificate policies; use one client auth policy per certificate"
                        ),
                    ));
                }
            }
            return Ok(());
        }
        let acme = cert.get("acme").and_then(Value::as_str);
        let cert_file = cert.get("certFile").and_then(Value::as_str);
        let cert_key_file = cert.get("certKeyFile").and_then(Value::as_str);
        // nginx `ssl_trusted_certificate` equivalent: a declared chain file is
        // part of the certificate material (the downstream client-auth trust
        // store), never a silently dropped key. A malformed declaration is a
        // located configuration error.
        let chain_file = match cert.get("chainFile") {
            None => None,
            Some(value) => {
                let chain_file = value.as_str().map(str::trim).unwrap_or_default();
                if chain_file.is_empty() {
                    return Err(materialize_error(
                        &format!("{path}.chainFile"),
                        "chainFile must be a non-empty PEM path",
                    ));
                }
                Some(chain_file.to_owned())
            }
        };
        let (certificate_file, private_key_file) = match (acme, cert_file, cert_key_file) {
            (Some(acme_name), _, _) => {
                // `acme <name>` resolves against the ACME live root, which is
                // platform-specific (`/etc/sdkwork/certs/letsencrypt` on Linux,
                // `%ProgramData%\sdkwork\certs\letsencrypt` on Windows). Deriving
                // it from a literal here pointed the compiled config at a tree
                // the certificate worker never writes to on Windows.
                let live_root = crate::config_paths::canonical_acme_live_root()
                    .map_err(|error| materialize_error(path, error))?;
                let directory = live_root.join(acme_name);
                (
                    directory
                        .join("fullchain.pem")
                        .to_string_lossy()
                        .into_owned(),
                    directory.join("privkey.pem").to_string_lossy().into_owned(),
                )
            }
            (_, Some(cert_file), Some(cert_key_file)) => {
                (cert_file.to_owned(), cert_key_file.to_owned())
            }
            _ => {
                return Err(materialize_error(
                    path,
                    format!("certificate `{name}` requires certFile + certKeyFile or acme"),
                ))
            }
        };
        let ocsp_stapling_file = match cert.get("ocspStaplingFile") {
            None => None,
            Some(value) => {
                let path = value.as_str().map(str::trim).unwrap_or_default();
                if path.is_empty() {
                    return Err(materialize_error(
                        &format!("{path}.ocspStaplingFile"),
                        "ocspStaplingFile must be a non-empty DER path",
                    ));
                }
                Some(path.to_owned())
            }
        };
        let mut source = json!({
            "type": "protected-file",
            "certificateFile": certificate_file,
            "privateKeyFile": private_key_file,
        });
        if let Some(chain_file) = chain_file {
            source["chainFile"] = Value::String(chain_file);
        }
        if let Some(ocsp_stapling_file) = ocsp_stapling_file {
            source["ocspStaplingFile"] = Value::String(ocsp_stapling_file);
        }
        self.certificates.push(json!({
            "id": name,
            "serverNames": server_names,
            "source": source,
        }));
        let mut policy = json!({
            "id": format!("tls-{name}"),
            "certificateRefs": [name],
            "minimumVersion": tls_window.0.wire(),
            "maximumVersion": tls_window.1.wire(),
            "alpn": ["h2", "http/1.1"],
        });
        if let Some(client_auth) = client_auth {
            policy["clientAuth"] = client_auth.clone();
        }
        self.tls_policies.push(policy);
        self.certificate_names.push(name.to_owned());
        self.certificate_tls_windows
            .insert(name.to_owned(), tls_window);
        Ok(())
    }

    fn ensure_upstream(
        &mut self,
        upstream: &Map<String, Value>,
        path: &str,
    ) -> Result<String, WebServerConfigError> {
        check_supported_keys(
            upstream,
            path,
            &["name", "target", "loadBalancing", "hashKey"],
        )?;
        let name = as_str(upstream, path, "name")?.to_owned();
        if self.upstream_names.contains(&name) {
            return Ok(name);
        }
        let targets = upstream
            .get("target")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                materialize_error(
                    path,
                    format!("upstream `{name}` must declare a target array"),
                )
            })?;
        let mut target_values = Vec::new();
        let mut authorized_literal_ips: Vec<String> = Vec::new();
        let mut has_hostname_target = false;
        for (index, target) in targets.iter().enumerate() {
            let target_path = format!("{path}.target[{index}]");
            let target = target
                .as_object()
                .ok_or_else(|| materialize_error(&target_path, "target entries must be tables"))?;
            check_supported_keys(
                target,
                &target_path,
                &[
                    "address",
                    "weight",
                    "backup",
                    "down",
                    "maxConnections",
                    "slowStartMs",
                ],
            )?;
            let address = as_str(target, &target_path, "address")?;
            let down = target.get("down").and_then(Value::as_bool).unwrap_or(false);
            if down {
                continue;
            }
            let url = if address.starts_with("unix:") {
                return Err(materialize_error(
                    &target_path,
                    "unix: upstream sockets are not supported by the runtime model",
                ));
            } else if address.contains("://") {
                address.to_owned()
            } else {
                format!("http://{address}")
            };
            let mut entry = json!({ "url": url });
            if let Some(weight) = target.get("weight").and_then(Value::as_u64) {
                entry["weight"] = Value::from(weight);
            }
            if let Some(backup) = target.get("backup").and_then(Value::as_bool) {
                entry["backup"] = Value::Bool(backup);
            }
            if let Some(max_connections) = target.get("maxConnections").and_then(Value::as_u64) {
                entry["maxConnections"] = Value::from(max_connections);
            }
            if let Some(slow_start_ms) = target.get("slowStartMs").and_then(Value::as_u64) {
                entry["slowStartMs"] = Value::from(slow_start_ms);
            }
            target_values.push(entry);
            // Literal IP targets are operator-declared in server.toml; the
            // runtime SSRF guard must be told the target is authorized.
            let host = address
                .rsplit_once(':')
                .map(|(host, _)| host)
                .unwrap_or(address);
            if host.parse::<std::net::IpAddr>().is_ok() {
                match host.parse::<std::net::IpAddr>() {
                    Ok(std::net::IpAddr::V4(ip)) => {
                        authorized_literal_ips.push(format!("{ip}/32"));
                    }
                    Ok(std::net::IpAddr::V6(ip)) => {
                        authorized_literal_ips.push(format!("{ip}/128"));
                    }
                    Err(_) => {}
                }
            } else {
                // Hostname targets (Docker Compose DNS, WSL service names) are
                // also operator-declared; authorize the standard private /
                // loopback ranges so resolved Docker bridge addresses work.
                // Hard-forbidden ranges stay blocked by upstream_ip_is_allowed.
                has_hostname_target = true;
            }
        }
        if target_values.is_empty() {
            return Err(materialize_error(
                path,
                format!("upstream `{name}` has no live targets after `down` filtering"),
            ));
        }
        if has_hostname_target {
            for network in hostname_upstream_allowed_cidrs() {
                authorized_literal_ips.push(network.to_string());
            }
        }
        let (load_balancing, hash) = match upstream.get("loadBalancing").and_then(Value::as_str) {
            None | Some("round-robin") => {
                if upstream.get("hashKey").is_some() {
                    return Err(materialize_error(
                        path,
                        "`hashKey` is allowed only when loadBalancing = \"hash\"",
                    ));
                }
                ("round-robin", None)
            }
            Some("least-connections") => {
                if upstream.get("hashKey").is_some() {
                    return Err(materialize_error(
                        path,
                        "`hashKey` is allowed only when loadBalancing = \"hash\"",
                    ));
                }
                ("least-connections", None)
            }
            Some("ip-hash") => {
                if upstream.get("hashKey").is_some() {
                    return Err(materialize_error(
                        path,
                        "`hashKey` is allowed only when loadBalancing = \"hash\"",
                    ));
                }
                ("ip-hash", None)
            }
            Some("random") => {
                if upstream.get("hashKey").is_some() {
                    return Err(materialize_error(
                        path,
                        "`hashKey` is allowed only when loadBalancing = \"hash\"",
                    ));
                }
                ("random-two-least-connections", None)
            }
            Some("hash") => {
                let hash_key = upstream
                    .get("hashKey")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        materialize_error(
                            path,
                            "`hashKey` is required when loadBalancing = \"hash\"",
                        )
                    })?;
                let hash = parse_upstream_hash_key(hash_key, path)?;
                ("hash", Some(hash))
            }
            Some(other) => {
                return Err(materialize_error(
                    path,
                    format!("unknown loadBalancing `{other}`"),
                ))
            }
        };
        let mut upstream = json!({
            "id": name,
            "targets": target_values,
            "loadBalancing": load_balancing,
        });
        if let Some(hash) = hash {
            upstream["hash"] = hash;
        }
        if !authorized_literal_ips.is_empty() {
            upstream["addressPolicy"] = json!({ "allowedCidrs": authorized_literal_ips });
        }
        self.upstreams.push(upstream);
        self.upstream_names.push(name.clone());
        Ok(name)
    }

    #[allow(clippy::too_many_arguments)]
    fn materialize_location(
        &mut self,
        server_index: usize,
        location_index: usize,
        location: &Map<String, Value>,
        server_name: &str,
        inherited_proxy_set_headers: &[String],
        inherited_root: Option<&str>,
        named_pc_root: Option<&str>,
        named_h5_root: Option<&str>,
    ) -> Result<String, WebServerConfigError> {
        let path = format!("http.server[{server_index}].location[{location_index}]");
        check_supported_keys(
            location,
            &path,
            &[
                "match",
                "proxyPass",
                "root",
                "alias",
                "index",
                "tryFiles",
                "returnStatus",
                "returnBody",
                "returnLocation",
                "proxyConnectTimeout",
                "proxyReadTimeout",
                "proxySendTimeout",
                "proxyBufferSize",
                "proxyRedirect",
                "proxySetHeader",
                "proxyWebsocketUpgrade",
                "proxyHttpVersion",
                "proxyBuffering",
                "proxyPassRequestHeaders",
                "proxyMethod",
                "allow",
                "deny",
                "limitReq",
                "limitConn",
                "rewrite",
                "authBasic",
                "authBasicUserFile",
                "subFilter",
                "subFilterOnce",
                "subFilterTypes",
                "subFilterLastModified",
                "secureLinkSecret",
                "secureLink",
                "secureLinkMd5",
                "secureLinkExpires",
                "expires",
                "etag",
            ],
        )?;
        let match_value = as_str(location, &path, "match")?;
        // Validate match syntax early so serving-behavior errors stay attributed
        // to the location; route pathType is attached in materialize_server.
        let _ = parse_location_match(&path, match_value)?;
        let resource_id = format!("loc-{server_index}-{location_index}");
        let serving = [
            location.contains_key("proxyPass"),
            location.contains_key("root"),
            location.contains_key("alias"),
            location.contains_key("returnStatus"),
        ]
        .iter()
        .filter(|present| **present)
        .count();
        // A location with only `tryFiles` inherits the server-level root
        // (nginx SPA layout), which counts as static serving.
        let inherits_static =
            serving == 0 && location.contains_key("tryFiles") && inherited_root.is_some();
        // Adaptive Web dispatch: `try_files … @$surface;` jumps to the
        // `@pc` / `@h5` named locations that own the document roots
        // (SDKWORK_DEPLOY_SPEC.md §8.1); no server-level root is required.
        let adaptive_dispatch = serving == 0
            && location
                .get("tryFiles")
                .and_then(Value::as_array)
                .is_some_and(|entries| {
                    entries
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|entry| entry.starts_with('@'))
                });
        if serving > 1 || (serving == 0 && !inherits_static && !adaptive_dispatch) {
            return Err(materialize_error(
                &path,
                "a location must declare exactly one serving behavior (proxyPass | root | alias | returnStatus, or tryFiles with a server-level root)",
            ));
        }
        if location.contains_key("returnLocation") && !location.contains_key("returnStatus") {
            return Err(materialize_error(
                &path,
                "`returnLocation` requires `returnStatus` (a 3xx redirect status)",
            ));
        }

        // nginx `proxy_method`: forwarded method replaces the client method.
        let proxy_method = match location.get("proxyMethod") {
            None => None,
            Some(value) => {
                let Some(method) = value.as_str() else {
                    return Err(materialize_error(
                        &format!("{path}.proxyMethod"),
                        "`proxyMethod` must be a string HTTP method",
                    ));
                };
                if method.is_empty()
                    || !method.bytes().all(|byte| {
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
                    })
                {
                    return Err(materialize_error(
                        &format!("{path}.proxyMethod"),
                        format!("`proxyMethod` value `{method}` is not a valid HTTP method token"),
                    ));
                }
                Some(method.to_owned())
            }
        };
        if let Some(proxy_pass) = location.get("proxyPass").and_then(Value::as_str) {
            let proxy_pass_request_headers = location
                .get("proxyPassRequestHeaders")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if proxy_pass.contains('$') {
                crate::config::validate_proxy_pass_template(proxy_pass)
                    .map_err(|message| materialize_error(&path, message))?;
                let request_set_headers = location
                    .get("proxySetHeader")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::trim)
                            .filter(|entry| !entry.is_empty())
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for entry in request_set_headers
                    .iter()
                    .chain(inherited_proxy_set_headers)
                {
                    validate_proxy_set_header_entry(&path, entry)?;
                }
                let merged_headers =
                    merge_proxy_set_headers(inherited_proxy_set_headers, &request_set_headers);
                let mut proxy_resource = json!({
                    "id": resource_id,
                    "type": "proxy",
                    "stripPrefix": false,
                    "requestSetHeaders": merged_headers,
                    "upstreamRef": "",
                    "dynamicTarget": proxy_pass,
                    "proxyPassRequestHeaders": proxy_pass_request_headers,
                });
                if let Some(method) = &proxy_method {
                    proxy_resource["proxyMethod"] = Value::String(method.clone());
                }
                self.resources.push(proxy_resource);
            } else {
                // Split the proxyPass URI part (nginx `proxy_pass` replacement
                // semantics): a URI part replaces the matched location prefix.
                let (authority, target_uri) = if let Some(rest) = proxy_pass
                    .strip_prefix("http://")
                    .or_else(|| proxy_pass.strip_prefix("https://"))
                {
                    if rest.contains("unix:") {
                        return Err(materialize_error(
                            &path,
                            "unix: proxyPass sockets are not supported by the runtime model",
                        ));
                    }
                    match rest.split_once('/') {
                        Some((authority, uri_path)) => {
                            let uri = format!("/{uri_path}");
                            if uri.contains('?') {
                                return Err(materialize_error(
                                &path,
                                "query strings in the proxyPass URI part are not supported by the runtime model",
                            ));
                            }
                            (authority.to_owned(), Some(uri))
                        }
                        None => (rest.to_owned(), None),
                    }
                } else {
                    return Err(materialize_error(&path, format!("proxyPass `{proxy_pass}` must be http(s)://upstream or http(s)://host:port")));
                };
                if authority.is_empty() {
                    return Err(materialize_error(
                        &path,
                        format!("proxyPass `{proxy_pass}` has an empty upstream authority"),
                    ));
                }
                let upstream_ref = if authority.contains(':') {
                    // Literal host:port target: synthesize a dedicated upstream;
                    // the target URL never carries the proxy_pass URI part.
                    let literal_id = format!("literal-{}", authority.replace([':', '/', '.'], "-"));
                    if !self.upstream_names.contains(&literal_id) {
                        let host = authority.rsplit_once(':').map_or("", |(host, _)| host);
                        let address_policy =
                            host.parse::<std::net::IpAddr>().ok().map(|ip| match ip {
                                std::net::IpAddr::V4(ip) => format!("{ip}/32"),
                                std::net::IpAddr::V6(ip) => format!("{ip}/128"),
                            });
                        let mut literal_upstream = json!({
                            "id": literal_id,
                            "targets": [{ "url": format!("http://{authority}") }],
                            "loadBalancing": "round-robin",
                        });
                        if let Some(cidr) = address_policy {
                            literal_upstream["addressPolicy"] = json!({ "allowedCidrs": [cidr] });
                        }
                        self.upstreams.push(literal_upstream);
                        self.upstream_names.push(literal_id.clone());
                    }
                    literal_id
                } else {
                    authority
                };
                if !self.upstream_names.contains(&upstream_ref) {
                    return Err(materialize_error(
                        &path,
                        format!("proxyPass references undefined upstream `{upstream_ref}`"),
                    ));
                }
                let request_set_headers = location
                    .get("proxySetHeader")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::trim)
                            .filter(|entry| !entry.is_empty())
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for entry in request_set_headers
                    .iter()
                    .chain(inherited_proxy_set_headers)
                {
                    validate_proxy_set_header_entry(&path, entry)?;
                }
                let merged_headers =
                    merge_proxy_set_headers(inherited_proxy_set_headers, &request_set_headers);
                let mut proxy_resource = json!({
                    "id": resource_id,
                    "type": "proxy",
                    "upstreamRef": upstream_ref,
                    "stripPrefix": false,
                    "requestSetHeaders": merged_headers,
                    "proxyPassRequestHeaders": proxy_pass_request_headers,
                });
                if let Some(method) = &proxy_method {
                    proxy_resource["proxyMethod"] = Value::String(method.clone());
                }
                if let Some(uri) = target_uri {
                    proxy_resource["targetUri"] = Value::String(uri);
                }
                self.resources.push(proxy_resource);
            }
        } else if adaptive_dispatch {
            // Adaptive Web dispatch (SDKWORK_DEPLOY_SPEC.md §8.1):
            // `try_files … @$sdkwork_webserver_surface_final;` with `@pc` /
            // `@h5` named locations owning the document roots. The runtime
            // model carries both roots so the data plane selects the surface
            // per request (mobile → H5, desktop → PC).
            let pc_root = named_pc_root
                .or(named_h5_root)
                .unwrap_or("/usr/share/sdkwork/web/pc");
            let mut resource = json!({
                "id": resource_id,
                "type": "static",
                "root": pc_root.trim_start_matches('/'),
                "indexFiles": ["index.html"],
                "spaFallback": "index.html",
            });
            if let Some(h5_root) = named_h5_root {
                resource["h5Root"] = Value::String(h5_root.trim_start_matches('/').to_owned());
            }
            self.resources.push(resource);
        } else if let Some(root) = if location.contains_key("root") {
            location.get("root").and_then(Value::as_str)
        } else if inherits_static {
            inherited_root
        } else {
            None
        } {
            self.resources.push(materialize_static_resource(
                &resource_id,
                root,
                location,
                false,
            ));
        } else if let Some(alias) = location.get("alias").and_then(Value::as_str) {
            // nginx `alias` semantics: the matched location prefix is
            // replaced by the configured directory before joining.
            if !alias.ends_with('/') {
                return Err(materialize_error(
                    &path,
                    "directory aliases must end with `/` (SDKWORK_WEBSERVER_SPEC.md §11.2)",
                ));
            }
            if matches!(match_value.chars().next(), Some('~')) {
                return Err(materialize_error(
                    &path,
                    "`alias` with regex location match is not supported; use a prefix/`^~` location",
                ));
            }
            self.resources.push(materialize_static_resource(
                &resource_id,
                alias,
                location,
                true,
            ));
        } else if let Some(status) = location.get("returnStatus").and_then(Value::as_u64) {
            let status: u16 = u16::try_from(status).map_err(|_| {
                materialize_error(&path, format!("invalid returnStatus `{status}`"))
            })?;
            if let Some(return_location) = location.get("returnLocation").and_then(Value::as_str) {
                // nginx `return <3xx> <url>`: the redirect data plane expands
                // `$host` / `$request_uri` / `$scheme` (same subset as nginx.conf).
                if !matches!(status, 301 | 302 | 303 | 307 | 308) {
                    return Err(materialize_error(
                        &path,
                        format!("`returnLocation` requires a redirect status (301|302|303|307|308), found {status}"),
                    ));
                }
                if return_location.contains('$')
                    && !crate::nginx::redirect_variables_ok(return_location)
                {
                    return Err(materialize_error(
                        &path,
                        format!("returnLocation `{return_location}` uses unsupported variables; supported: $host $request_uri $scheme"),
                    ));
                }
                self.resources.push(json!({
                    "id": resource_id,
                    "type": "redirect",
                    "status": status,
                    "location": return_location,
                }));
            } else {
                let mut resource = json!({
                    "id": resource_id,
                    "type": "respond",
                    "status": status,
                    "contentType": "text/plain; charset=utf-8",
                });
                if let Some(body) = location.get("returnBody").and_then(Value::as_str) {
                    resource["body"] = Value::String(body.to_owned());
                }
                self.resources.push(resource);
            }
        } else {
            return Err(materialize_error(
                &path,
                "location has no supported serving behavior",
            ));
        }

        let _ = server_name;
        Ok(resource_id)
    }

    fn materialize_server(
        &mut self,
        server_index: usize,
        server: &Map<String, Value>,
    ) -> Result<(), WebServerConfigError> {
        let path = format!("http.server[{server_index}]");
        check_supported_keys(
            server,
            &path,
            &[
                "listen",
                "serverName",
                "http2",
                "root",
                "index",
                "tryFiles",
                "tls",
                "proxySetHeader",
                "addHeader",
                "location",
            ],
        )?;
        // Server-level `proxySetHeader` entries are inherited by every proxy
        // location (nginx `proxy_set_header` inheritance semantics); the
        // merge lets location-level entries override by header name.
        let server_root = server.get("root").and_then(Value::as_str);
        let server_proxy_set_headers: Vec<String> = server
            .get("proxySetHeader")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for entry in &server_proxy_set_headers {
            validate_proxy_set_header_entry(&path, entry)?;
        }
        let listen = as_array(server, &path, "listen")?;
        let server_names = as_array(server, &path, "serverName")?;
        let primary_name = server_names
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| {
                materialize_error(&path, "serverName must be a non-empty array of hostnames")
            })?
            .to_owned();
        let server_name_list: Vec<String> = server_names
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let http2 = server
            .get("http2")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let tls_table = server.get("tls").and_then(Value::as_object);
        let tls_window = tls_window_from_protocols(tls_table, &format!("{path}.tls"))?;
        let client_auth = if let Some(tls) = tls_table {
            check_supported_keys(tls, &format!("{path}.tls"), SERVER_TLS_KEYS)?;
            parse_client_auth(tls, &format!("{path}.tls"))?
        } else {
            None
        };
        let certificate_name = if let Some(tls) = tls_table {
            if let Some(cert) = tls.get("cert").and_then(Value::as_str) {
                Some(cert.to_owned())
            } else if let (Some(cert_file), Some(cert_key_file)) = (
                tls.get("certFile").and_then(Value::as_str),
                tls.get("certKeyFile").and_then(Value::as_str),
            ) {
                let inline_name = format!("inline-{server_index}");
                self.certificates.push(json!({
                    "id": inline_name,
                    "serverNames": [primary_name],
                    "source": {
                        "type": "protected-file",
                        "certificateFile": cert_file,
                        "privateKeyFile": cert_key_file,
                    },
                }));
                let mut policy = json!({
                    "id": format!("tls-{inline_name}"),
                    "certificateRefs": [inline_name],
                    "minimumVersion": tls_window.0.wire(),
                    "maximumVersion": tls_window.1.wire(),
                    "alpn": ["h2", "http/1.1"],
                });
                if let Some(client_auth) = &client_auth {
                    policy["clientAuth"] = client_auth.clone();
                }
                self.tls_policies.push(policy);
                self.certificate_names.push(inline_name.clone());
                self.certificate_tls_windows
                    .insert(inline_name.clone(), tls_window);
                Some(inline_name)
            } else {
                None
            }
        } else {
            None
        };

        let mut listener_refs = Vec::new();
        for entry in listen {
            let entry = entry
                .as_str()
                .ok_or_else(|| materialize_error(&path, "listen entries must be strings"))?;
            let (bind, port, ssl, listen_http2) = parse_listen(entry, &path)?;
            let http2 = http2 || listen_http2;
            if ssl {
                let Some(certificate) = certificate_name.as_deref() else {
                    return Err(materialize_error(
                        &path,
                        format!("listen `{entry}` requires [http.server.tls] with a certificate"),
                    ));
                };
                self.ensure_certificate(
                    certificate,
                    &Map::new(),
                    &server_name_list,
                    &path,
                    client_auth.as_ref(),
                    tls_window,
                )?;
            }
            let tls_policy_ref =
                ssl.then(|| format!("tls-{}", certificate_name.as_deref().expect("checked ssl")));
            let listener_id =
                self.ensure_listener(&bind, port, ssl, http2, tls_policy_ref.as_deref(), &path)?;
            if !listener_refs.contains(&listener_id) {
                listener_refs.push(listener_id);
            }
        }

        // Adaptive Web named locations (`@pc` / `@h5`) are jump targets, not
        // routes: collect their document roots and skip them in the
        // materialization loops (mirrors `nginx::mapping`).
        let mut named_pc_root: Option<&str> = None;
        let mut named_h5_root: Option<&str> = None;
        for location in server
            .get("location")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
        {
            match location.get("match").and_then(Value::as_str) {
                Some("@pc") => named_pc_root = location.get("root").and_then(Value::as_str),
                Some("@h5") => named_h5_root = location.get("root").and_then(Value::as_str),
                _ => {}
            }
        }

        let mut routes = Vec::new();
        for (location_index, location) in server
            .get("location")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let location = location
                .as_object()
                .ok_or_else(|| materialize_error(&path, "location entries must be tables"))?;
            if location
                .get("match")
                .and_then(Value::as_str)
                .is_some_and(|match_value| match_value.starts_with('@'))
            {
                continue;
            }
            let resource_id = self.materialize_location(
                server_index,
                location_index,
                location,
                &primary_name,
                &server_proxy_set_headers,
                server_root,
                named_pc_root,
                named_h5_root,
            )?;
            routes.push(resource_id);
        }

        let mut virtual_host = json!({
            "id": primary_name.clone(),
            "listenerRefs": listener_refs,
            "serverNames": Value::Array(server_names.clone()),
            "routes": [],
        });
        if let Some(add_header) = server.get("addHeader").and_then(Value::as_array) {
            let custom_headers: Vec<Value> = add_header
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|entry| entry.split_once(' '))
                .map(|(name, value)| json!({ "name": name, "value": value }))
                .collect();
            if !custom_headers.is_empty() {
                virtual_host["securityHeaders"] = json!({ "customHeaders": custom_headers });
            }
        }
        // Build routes with their match metadata (the materialize_location
        // helper already pushed resources; attach route objects here).
        let mut route_entries = Vec::new();
        let _ = &mut routes;
        for (location_index, location) in server
            .get("location")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let location = location
                .as_object()
                .ok_or_else(|| materialize_error(&path, "location entries must be tables"))?;
            if location
                .get("match")
                .and_then(Value::as_str)
                .is_some_and(|match_value| match_value.starts_with('@'))
            {
                // Adaptive Web named locations are internal jump targets,
                // not routes (SDKWORK_DEPLOY_SPEC.md §8.1).
                continue;
            }
            let match_value = as_str(location, &path, "match")?;
            let (path_type, route_path) = parse_location_match(&path, match_value)?;
            let mut route = json!({
                "id": format!("route-{server_index}-{location_index}"),
                "match": { "pathType": path_type, "path": route_path },
                "resourceRef": format!("loc-{server_index}-{location_index}"),
                "access": materialize_access_rules(location, &path)?,
                "limitReq": materialize_limit_req_rules(location, &path, &self.limit_req_zone_names)?,
                "limitConn": materialize_limit_conn_rules(location, &path, &self.limit_conn_zone_names)?,
                "rewrite": materialize_rewrite_rules(location, &path)?,
            });
            if let Some(auth_basic) = materialize_auth_basic(location, &path)? {
                route["authBasic"] = auth_basic;
            }
            if let Some(sub_filter) = materialize_sub_filter(location, &path)? {
                route["subFilter"] = sub_filter;
            }
            if let Some(secure_link) = materialize_secure_link(location, &path)? {
                route["secureLink"] = secure_link;
            }
            if let Some(cache_policy) = materialize_cache_policy(location, &path)? {
                route["cachePolicy"] = cache_policy_json(&cache_policy);
            }
            route_entries.push(route);
        }
        virtual_host["routes"] = Value::Array(route_entries);
        self.virtual_hosts.push(virtual_host);
        Ok(())
    }

    fn materialize_upstreams(
        &mut self,
        http: &Map<String, Value>,
    ) -> Result<(), WebServerConfigError> {
        if let Some(upstreams) = http.get("upstream").and_then(Value::as_array) {
            for (index, upstream) in upstreams.iter().enumerate() {
                let upstream = upstream.as_object().ok_or_else(|| {
                    materialize_error("http.upstream", "upstream entries must be tables")
                })?;
                self.ensure_upstream(upstream, &format!("http.upstream[{index}]"))?;
            }
        }
        Ok(())
    }

    /// Materialize typed-TOML `[[http.maps]]` into the executable
    /// `WebServerAppConfig.maps` model.
    ///
    /// This is the typed plane's equivalent of the nginx `map` block Wave 1
    /// made executable: the same model, the same bounded validation
    /// ([`crate::config::map_variables::validate_map_definitions`]), and the
    /// same runtime expansion (`$result` in `addHeader`, `proxySetHeader`,
    /// `return`/redirect targets, `rewrite` replacements). The legacy
    /// string-array `map` key remains ACCEPTED_IGNORED with its diagnostic.
    fn materialize_maps(&mut self, http: &Map<String, Value>) -> Result<(), WebServerConfigError> {
        let Some(entries) = http.get("maps") else {
            return Ok(());
        };
        let Some(entries) = entries.as_array() else {
            return Err(materialize_error(
                "server.toml.http.maps",
                "`maps` must be an array of tables (`[[http.maps]]`)",
            ));
        };
        let mut maps: Vec<MapConfig> = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let path = format!("server.toml.http.maps[{index}]");
            let entry = entry
                .as_object()
                .ok_or_else(|| materialize_error(&path, "expected a TOML table"))?;
            check_supported_keys(
                entry,
                &path,
                &["source", "result", "hostnames", "default", "entries"],
            )?;
            let source = entry
                .get("source")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    materialize_error(&format!("{path}.source"), "`source` is required")
                })?
                .to_owned();
            validate_map_source(&source)
                .map_err(|message| materialize_error(&format!("{path}.source"), message))?;
            let result = entry
                .get("result")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    materialize_error(&format!("{path}.result"), "`result` is required")
                })?
                .to_owned();
            if let Err(message) = crate::config::map_variables::parse_variable_reference(&result) {
                return Err(materialize_error(
                    &format!("{path}.result"),
                    format!("map result `{result}` is invalid: {message}"),
                ));
            }
            let hostnames = entry
                .get("hostnames")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let default = match entry.get("default") {
                None => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .ok_or_else(|| {
                            materialize_error(
                                &format!("{path}.default"),
                                "`default` must be a string",
                            )
                        })?
                        .to_owned(),
                ),
            };
            let mut map_entries: Vec<MapEntryConfig> = Vec::new();
            if let Some(rows) = entry.get("entries") {
                let Some(rows) = rows.as_array() else {
                    return Err(materialize_error(
                        &format!("{path}.entries"),
                        "`entries` must be an array of `{ key = …, value = … }` tables",
                    ));
                };
                for (row_index, row) in rows.iter().enumerate() {
                    let row_path = format!("{path}.entries[{row_index}]");
                    let row = row
                        .as_object()
                        .ok_or_else(|| materialize_error(&row_path, "expected a TOML table"))?;
                    check_supported_keys(row, &row_path, &["key", "value"])?;
                    let key = row.get("key").and_then(Value::as_str).ok_or_else(|| {
                        materialize_error(&format!("{row_path}.key"), "`key` is required")
                    })?;
                    let value = row.get("value").and_then(Value::as_str).ok_or_else(|| {
                        materialize_error(&format!("{row_path}.value"), "`value` is required")
                    })?;
                    map_entries.push(MapEntryConfig {
                        key: key.to_owned(),
                        value: value.to_owned(),
                    });
                }
            }
            maps.push(MapConfig {
                source,
                result,
                hostnames,
                default,
                entries: map_entries,
            });
        }
        // Same bounded validation as the nginx-conf plane: source/result
        // vocabulary, value templates, duplicate result variables, hostnames
        // key form. A diagnostic is located at the authored array level (the
        // per-map detail is in the message).
        validate_map_definitions(&maps).map_err(|message| {
            materialize_error("server.toml.http.maps", format!("invalid `map`: {message}"))
        })?;
        self.maps = maps
            .into_iter()
            .map(|map| serde_json::to_value(map).unwrap_or(Value::Null))
            .collect();
        Ok(())
    }

    fn materialize_certificates(
        &mut self,
        http: &Map<String, Value>,
    ) -> Result<(), WebServerConfigError> {
        if let Some(certificates) = http.get("certificates").and_then(Value::as_object) {
            for (name, cert) in certificates {
                let cert = cert.as_object().ok_or_else(|| {
                    materialize_error("http.certificates", "certificate entries must be tables")
                })?;
                // A certificate declared up front carries no server TLS table,
                // so its policy starts on the platform default window. The
                // first server that names it refines the window from
                // `[http.server.tls] protocols` (see `ensure_certificate`).
                self.ensure_certificate(
                    name,
                    cert,
                    &[],
                    &format!("http.certificates.{name}"),
                    None,
                    TlsVersion::PLATFORM_DEFAULT_WINDOW,
                )?;
            }
        }
        Ok(())
    }

    fn materialize_streams(
        &mut self,
        root: &Map<String, Value>,
    ) -> Result<(), WebServerConfigError> {
        let Some(stream) = root.get("stream").and_then(Value::as_object) else {
            return Ok(());
        };
        let Some(servers) = stream.get("server").and_then(Value::as_array) else {
            return Ok(());
        };
        let mut stream_servers = Vec::new();
        for (index, server) in servers.iter().enumerate() {
            let path = format!("stream.server[{index}]");
            let server = server
                .as_object()
                .ok_or_else(|| materialize_error(&path, "stream server entries must be tables"))?;
            check_supported_keys(
                server,
                &path,
                &[
                    "listen",
                    "proxyPass",
                    "proxyTimeout",
                    "proxyProtocol",
                    "sslPreread",
                    "certificate",
                    "protocol",
                    "clientCertificate",
                    "clientCertificateCA",
                    // nginx stream access phase and `return` handler.
                    "access",
                    "return",
                ],
            )?;
            let listen = as_array(server, &path, "listen")?;
            let first = listen.first().and_then(Value::as_str).ok_or_else(|| {
                materialize_error(&path, "listen must be a non-empty array of bindings")
            })?;
            let (bind, port, ssl, _) = parse_listen(first, &path)?;
            let ssl_preread = server
                .get("sslPreread")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if ssl && ssl_preread {
                return Err(materialize_error(
                    &path,
                    "listen … ssl (TLS terminate) and sslPreread are mutually exclusive",
                ));
            }
            let udp = match server.get("protocol").and_then(Value::as_str) {
                None | Some("tcp") => false,
                Some("udp") => true,
                Some(other) => {
                    return Err(materialize_error(
                        &format!("{path}.protocol"),
                        format!("`protocol` accepts tcp|udp, found `{other}`"),
                    ))
                }
            };
            if udp && (ssl || ssl_preread) {
                return Err(materialize_error(
                    &path,
                    "UDP stream listeners cannot combine `protocol = \"udp\"` with ssl/sslPreread",
                ));
            }
            let tls = if ssl {
                let certificate_ref = as_str(server, &path, "certificate")?;
                if !self
                    .certificate_names
                    .iter()
                    .any(|name| name == certificate_ref)
                {
                    return Err(materialize_error(
                        &path,
                        format!(
                            "certificate `{certificate_ref}` is not declared in [http.certificates]"
                        ),
                    ));
                }
                let client_auth = parse_client_auth(server, &path)?;
                let client_auth = client_auth.map(|value| {
                    serde_json::from_value(value)
                        .expect("client auth materialization is type-checked")
                });
                Some(crate::config::model::StreamTlsMode::Terminate {
                    certificate_ref: certificate_ref.to_owned(),
                    client_auth,
                })
            } else if ssl_preread {
                Some(crate::config::model::StreamTlsMode::Preread)
            } else {
                None
            };
            // SDKWORK_WEBSERVER_SPEC §12: stream listeners bind loopback or
            // declared private addresses by default; nginx wildcard `"<port>"`
            // entries are honored, but an explicit public IP requires
            // documented approval and fails closed here.
            let bind_ip: std::net::IpAddr = bind.parse().map_err(|_| {
                materialize_error(
                    &path,
                    format!("stream listen host `{bind}` must be an IP address"),
                )
            })?;
            if !bind_ip.is_unspecified() && is_public_ip(bind_ip) {
                return Err(materialize_error(
                    &path,
                    "stream listeners must bind a loopback or private address unless approved (SDKWORK_WEBSERVER_SPEC section 12)",
                ));
            }
            // nginx stream `return <value>`: write the literal value and close,
            // mutually exclusive with a proxy target.
            let return_value = match server.get("return") {
                None => None,
                Some(value) => {
                    let Some(value) = value.as_str() else {
                        return Err(materialize_error(
                            &format!("{path}.return"),
                            "`return` must be a string value",
                        ));
                    };
                    if value.is_empty() || value.len() > 4096 {
                        return Err(materialize_error(
                            &format!("{path}.return"),
                            "`return` value must be 1..4096 bytes",
                        ));
                    }
                    if value.contains('$') {
                        return Err(materialize_error(
                            &format!("{path}.return"),
                            "`return` variables are not supported; use a literal value",
                        ));
                    }
                    Some(value.to_owned())
                }
            };
            // Ordered nginx stream allow/deny rules.
            let access = match server.get("access") {
                None => Vec::new(),
                Some(value) => {
                    let Some(rules) = value.as_array() else {
                        return Err(materialize_error(
                            &format!("{path}.access"),
                            "`access` must be an array of { action, network } tables",
                        ));
                    };
                    let mut parsed = Vec::with_capacity(rules.len());
                    for (rule_index, rule) in rules.iter().enumerate() {
                        let rule_path = format!("{path}.access[{rule_index}]");
                        let rule: crate::config::model::AccessRuleConfig =
                            serde_json::from_value(rule.clone())
                                .map_err(|source| materialize_error(&rule_path, source))?;
                        let network = rule.network.trim();
                        if !(network.eq_ignore_ascii_case("all")
                            || network.parse::<std::net::IpAddr>().is_ok()
                            || network.parse::<ipnet::IpNet>().is_ok())
                        {
                            return Err(materialize_error(
                                &format!("{rule_path}.network"),
                                "access network must be `all`, an IP, or a CIDR",
                            ));
                        }
                        parsed.push(rule);
                    }
                    parsed
                }
            };
            if return_value.is_some() && server.get("proxyPass").is_some() {
                return Err(materialize_error(
                    &path,
                    "a stream server cannot combine `return` with `proxyPass`",
                ));
            }
            let proxy_pass = match (server.get("proxyPass"), return_value.is_some()) {
                (Some(_), true) => unreachable!("combination rejected above"),
                (Some(_), false) => as_str(server, &path, "proxyPass")?,
                (None, true) => "",
                (None, false) => {
                    return Err(materialize_error(
                        &path,
                        "stream server requires `proxyPass` or `return`",
                    ))
                }
            };
            let target = if return_value.is_some() {
                None
            } else if let Some((host, _)) = proxy_pass.rsplit_once(':') {
                // The host/port validation is shared with every other listener
                // in the workspace; a local copy is what lets one grammar
                // accept a value another rejects.
                let port: u16 = sdkwork_utils_rust::service_base_url::bind_port(proxy_pass)
                    .filter(|_| !host.is_empty())
                    .ok_or_else(|| {
                        materialize_error(
                            &path,
                            format!("proxyPass `{proxy_pass}` must be host:port or a declared upstream name"),
                        )
                    })?;
                Some(crate::config::model::StreamTargetConfig::Literal {
                    host: host.to_owned(),
                    port,
                })
            } else if self.upstream_names.contains(&proxy_pass.to_owned()) {
                Some(crate::config::model::StreamTargetConfig::Upstream {
                    name: proxy_pass.to_owned(),
                })
            } else {
                return Err(materialize_error(
                    &path,
                    format!("proxyPass references undefined upstream `{proxy_pass}`"),
                ));
            };
            let proxy_timeout_ms = match server.get("proxyTimeout").and_then(Value::as_str) {
                Some(value) => materialize_duration_ms(value).ok_or_else(|| {
                    materialize_error(&path, format!("invalid proxyTimeout `{value}`"))
                })?,
                None => 60_000,
            };
            let proxy_protocol = server
                .get("proxyProtocol")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            stream_servers.push(crate::config::model::StreamServerConfig {
                id: format!("stream-{index}"),
                bind: bind.to_owned(),
                port,
                protocol: if udp {
                    crate::config::model::StreamProtocol::Udp
                } else {
                    crate::config::model::StreamProtocol::Tcp
                },
                target,
                access,
                return_value,
                limit_conn: Vec::new(),
                limit_conn_dry_run: false,
                set_variables: Vec::new(),
                preread_timeout_ms: 30_000,
                preread_buffer_bytes: 16 * 1024,
                proxy_timeout_ms,
                proxy_protocol,
                tls,
            });
        }
        self.streams = stream_servers
            .into_iter()
            .map(|server| {
                serde_json::to_value(server)
                    .map_err(|source| materialize_error("stream.server", source))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }

    /// Materialize the optional `[mail]` section: `[[mail.server]]` entries
    /// into the mail proxy plane (`MailConfig.servers`).
    ///
    /// Wave 9B keys: the Wave 9A listener/dialog knobs plus `authHttp`, `tls`
    /// (frontend TLS + STARTTLS), `proxy` (backend relay, XCLIENT, PROXY to the
    /// backend and the typed-plane backend TLS extension), and `maxErrors`.
    #[allow(clippy::too_many_lines)]
    fn materialize_mail(&mut self, root: &Map<String, Value>) -> Result<(), WebServerConfigError> {
        let Some(mail) = root.get("mail").and_then(Value::as_object) else {
            return Ok(());
        };
        check_supported_keys(mail, "server.toml.mail", &["server"])?;
        let Some(servers) = mail.get("server").and_then(Value::as_array) else {
            return Ok(());
        };
        let mut mail_servers = Vec::new();
        for (index, server) in servers.iter().enumerate() {
            let path = format!("mail.server[{index}]");
            let server = server
                .as_object()
                .ok_or_else(|| materialize_error(&path, "mail server entries must be tables"))?;
            check_supported_keys(
                server,
                &path,
                &[
                    "listen",
                    "protocol",
                    "serverName",
                    "timeout",
                    "soKeepalive",
                    "proxyProtocol",
                    "smtpClientBuffer",
                    "imapClientBuffer",
                    "pop3Auth",
                    "imapAuth",
                    "smtpAuth",
                    "smtpGreetingDelay",
                    "authHttp",
                    "tls",
                    "proxy",
                    "maxErrors",
                ],
            )?;
            let listen = as_array(server, &path, "listen")?;
            let first = listen.first().and_then(Value::as_str).ok_or_else(|| {
                materialize_error(&path, "listen must be a non-empty array of bindings")
            })?;
            if listen.len() != 1 {
                return Err(materialize_error(
                    &path,
                    "each mail server declares exactly one listen binding",
                ));
            }
            let (bind, port, implicit_tls, _) = parse_listen(first, &path)?;
            let protocol = match server.get("protocol").and_then(Value::as_str) {
                None | Some("imap") => crate::config::model::MailServerProtocol::Imap,
                Some("pop3") => crate::config::model::MailServerProtocol::Pop3,
                Some("smtp") => crate::config::model::MailServerProtocol::Smtp,
                Some(other) => {
                    return Err(materialize_error(
                        &format!("{path}.protocol"),
                        format!("`protocol` accepts pop3|imap|smtp, found `{other}`"),
                    ))
                }
            };
            let server_name = match server.get("serverName") {
                Some(value) => Some(
                    value
                        .as_str()
                        .ok_or_else(|| {
                            materialize_error(&format!("{path}.serverName"), "must be a string")
                        })?
                        .to_owned(),
                ),
                None => None,
            };
            let timeout_ms = match server.get("timeout").and_then(Value::as_str) {
                Some(value) => materialize_duration_ms(value)
                    .filter(|millis| (1..=86_400_000).contains(millis))
                    .ok_or_else(|| {
                        materialize_error(
                            &format!("{path}.timeout"),
                            format!("invalid timeout `{value}` (expected e.g. 60s, 10m, 1h)"),
                        )
                    })?,
                None => 60_000,
            };
            let smtp_greeting_delay_ms =
                match server.get("smtpGreetingDelay").and_then(Value::as_str) {
                    Some(value) => materialize_duration_ms(value)
                        .filter(|millis| *millis <= 60_000)
                        .ok_or_else(|| {
                            materialize_error(
                                &format!("{path}.smtpGreetingDelay"),
                                format!("invalid smtpGreetingDelay `{value}` (maximum 60s)"),
                            )
                        })?,
                    None => 0,
                };
            let so_keepalive = server
                .get("soKeepalive")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let proxy_protocol = server
                .get("proxyProtocol")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let smtp_client_buffer = match server.get("smtpClientBuffer") {
                Some(value) => materialize_mail_buffer(value, &format!("{path}.smtpClientBuffer"))?,
                None => 4 * 1024,
            };
            let imap_client_buffer = match server.get("imapClientBuffer") {
                Some(value) => materialize_mail_buffer(value, &format!("{path}.imapClientBuffer"))?,
                None => 4 * 1024,
            };
            let mut entry = json!({
                "id": format!("mail-{index}-{port}"),
                "bind": bind,
                "port": port,
                "protocol": protocol.as_str(),
                "timeoutMs": timeout_ms,
                "soKeepalive": so_keepalive,
                "proxyProtocol": proxy_protocol,
                "smtpClientBuffer": smtp_client_buffer,
                "imapClientBuffer": imap_client_buffer,
                "smtpGreetingDelayMs": smtp_greeting_delay_ms,
            });
            if let Some(server_name) = server_name {
                entry["serverName"] = Value::String(server_name);
            }
            for (key, mechanisms) in [
                (
                    "pop3Auth",
                    materialize_mail_auth(server, &path, "pop3Auth")?,
                ),
                (
                    "imapAuth",
                    materialize_mail_auth(server, &path, "imapAuth")?,
                ),
                (
                    "smtpAuth",
                    materialize_mail_auth(server, &path, "smtpAuth")?,
                ),
            ] {
                if let Some(mechanisms) = mechanisms {
                    entry[key] = serde_json::to_value(mechanisms)
                        .map_err(|source| materialize_error("mail.server", source))?;
                }
            }
            if let Some(auth_http) = server.get("authHttp") {
                entry["authHttp"] =
                    materialize_mail_auth_http(auth_http, &format!("{path}.authHttp"))?;
            }
            if let Some(tls) = server.get("tls") {
                let mut tls = materialize_mail_tls(tls, &format!("{path}.tls"))?;
                tls["implicit"] = Value::Bool(implicit_tls);
                if implicit_tls {
                    let starttls = tls.get("starttls").and_then(Value::as_str).unwrap_or("off");
                    if starttls == "only" {
                        return Err(materialize_error(
                            &format!("{path}.tls.starttls"),
                            "implicit TLS (`listen … ssl`) and `starttls only` are mutually exclusive",
                        ));
                    }
                }
                entry["tls"] = tls;
            } else if implicit_tls {
                return Err(materialize_error(
                    &path,
                    "mail `listen … ssl` requires a `[mail.server.tls]` table with certificateFile and privateKeyFile",
                ));
            }
            if let Some(proxy) = server.get("proxy") {
                entry["proxy"] = materialize_mail_proxy(proxy, &format!("{path}.proxy"))?;
            }
            if let Some(max_errors) = server.get("maxErrors") {
                let max_errors = max_errors.as_u64().ok_or_else(|| {
                    materialize_error(
                        &format!("{path}.maxErrors"),
                        "`maxErrors` must be an integer between 1 and 100",
                    )
                })?;
                if !(1..=100).contains(&max_errors) {
                    return Err(materialize_error(
                        &format!("{path}.maxErrors"),
                        "`maxErrors` must be an integer between 1 and 100",
                    ));
                }
                entry["maxErrors"] = Value::from(max_errors);
            }
            mail_servers.push(entry);
        }
        self.mail_servers = mail_servers;
        Ok(())
    }

    /// Materializes the optional `[tunnel]` section (SDKWORK WebServer
    /// Tunnel PRD SS38/SS39). Accepts snake_case aliases for the network
    /// fields so the TOML surface reads naturally, and emits the camelCase
    /// runtime model.
    #[allow(clippy::too_many_lines)]
    fn materialize_tunnel(effective: &Value) -> Result<Option<Value>, WebServerConfigError> {
        let Some(tunnel) = effective.get("tunnel").and_then(Value::as_object) else {
            return Ok(None);
        };
        check_supported_keys(
            tunnel,
            "server.toml.tunnel",
            &[
                "enabled", "gateway", "agent", "network", "timeout", "limits", "routes",
            ],
        )?;
        let mut value = json!({
            "enabled": tunnel.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        });
        if let Some(gateway) = tunnel.get("gateway").and_then(Value::as_object) {
            check_supported_keys(
                gateway,
                "server.toml.tunnel.gateway",
                &[
                    "listen",
                    "domainSuffixes",
                    "domain_suffixes",
                    "agentTokenEnv",
                    "agent_token_env",
                    "tlsCertPemEnv",
                    "tls_cert_pem_env",
                    "tlsKeyPemEnv",
                    "tls_key_pem_env",
                    "routePortMin",
                    "route_port_min",
                    "routePortMax",
                    "route_port_max",
                    "tcpmuxPort",
                    "tcpmux_port",
                    "authScopes",
                    "auth_scopes",
                    "proxyBindAddr",
                    "proxy_bind_addr",
                    "users",
                ],
            )?;
            let mut mapped = json!({
                "listen": gateway.get("listen").and_then(Value::as_str)
                    .ok_or_else(|| materialize_error("server.toml.tunnel.gateway.listen", "listen is required"))?,
            });
            for (source, target) in [
                ("domainSuffixes", "domainSuffixes"),
                ("domain_suffixes", "domainSuffixes"),
                ("agentTokenEnv", "agentTokenEnv"),
                ("agent_token_env", "agentTokenEnv"),
                ("tlsCertPemEnv", "tlsCertPemEnv"),
                ("tls_cert_pem_env", "tlsCertPemEnv"),
                ("tlsKeyPemEnv", "tlsKeyPemEnv"),
                ("tls_key_pem_env", "tlsKeyPemEnv"),
            ] {
                if let Some(entry) = gateway.get(source) {
                    mapped[target] = entry.clone();
                }
            }
            // The registrable public port range for `tcp`/`udp` routes is a
            // gateway registration policy: an operator must be able to narrow
            // it where the tunnel is configured (docs/tunnel.md §gateway).
            // Ports are u16, so an out-of-range value is a configuration error
            // rather than a silent truncation.
            for (camel, snake, target) in [
                ("routePortMin", "route_port_min", "routePortMin"),
                ("routePortMax", "route_port_max", "routePortMax"),
                // The multiplexed ingress port (FRP `tcpmux` parity): one port
                // that reaches every route through HTTP CONNECT.
                ("tcpmuxPort", "tcpmux_port", "tcpmuxPort"),
            ] {
                if let Some(entry) = gateway.get(camel).or_else(|| gateway.get(snake)) {
                    let port = entry
                        .as_i64()
                        .or_else(|| entry.as_u64().and_then(|value| i64::try_from(value).ok()))
                        .ok_or_else(|| {
                            materialize_error(
                                &format!("server.toml.tunnel.gateway.{snake}"),
                                "must be an integer port number",
                            )
                        })?;
                    if !(0..=i64::from(u16::MAX)).contains(&port) {
                        return Err(materialize_error(
                            &format!("server.toml.tunnel.gateway.{snake}"),
                            format!("port {port} is outside 0..=65535"),
                        ));
                    }
                    mapped[target] = json!(port);
                }
            }
            // FRP `auth.additionalScopes`: parsed through the core model so the
            // TOML surface accepts exactly the spellings the runtime can
            // honour, and `newWorkConns` is refused with the explanation that
            // it has no equivalent here rather than silently doing nothing.
            if let Some(entry) = gateway
                .get("authScopes")
                .or_else(|| gateway.get("auth_scopes"))
            {
                let entries = entry.as_array().ok_or_else(|| {
                    materialize_error(
                        "server.toml.tunnel.gateway.authScopes",
                        "must be an array of scope labels",
                    )
                })?;
                let mut labels = Vec::with_capacity(entries.len());
                for value in entries {
                    let label = value.as_str().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.gateway.authScopes",
                            "every entry must be a scope label string",
                        )
                    })?;
                    let scope = sdkwork_webserver_tunnel_core::AuthScope::parse(label).map_err(
                        |error| {
                            materialize_error(
                                "server.toml.tunnel.gateway.authScopes",
                                error.to_string(),
                            )
                        },
                    )?;
                    labels.push(Value::String(scope.wire_label().to_owned()));
                }
                mapped["authScopes"] = Value::Array(labels);
            }
            // FRP `proxyBindAddr`: the address per-route listeners bind.
            // Validated through the core model so a hostname or a `host:port`
            // form is a configuration error rather than a silently ignored
            // value that binds somewhere the operator did not ask for.
            if let Some(entry) = gateway
                .get("proxyBindAddr")
                .or_else(|| gateway.get("proxy_bind_addr"))
            {
                let raw = entry.as_str().ok_or_else(|| {
                    materialize_error(
                        "server.toml.tunnel.gateway.proxyBindAddr",
                        "must be an IP address string",
                    )
                })?;
                let parsed: sdkwork_webserver_tunnel_core::TunnelGatewayConfig =
                    serde_json::from_value(json!({
                        "listen": "0.0.0.0:0",
                        "proxyBindAddr": raw,
                    }))
                    .map_err(|error| {
                        materialize_error(
                            "server.toml.tunnel.gateway.proxyBindAddr",
                            error.to_string(),
                        )
                    })?;
                parsed.proxy_bind_addr().map_err(|error| {
                    materialize_error("server.toml.tunnel.gateway.proxyBindAddr", error)
                })?;
                mapped["proxyBindAddr"] = json!(raw.trim());
            }
            // Per-user credentials and quotas (FRP `user`, made enforceable):
            // `[[tunnel.gateway.users]]` entries with name, tokenEnv and
            // optional ceilings. Every entry is validated through the core
            // model, so the file cannot describe a user the runtime would
            // refuse to enforce.
            if let Some(entry) = gateway.get("users") {
                let entries = entry.as_array().ok_or_else(|| {
                    materialize_error(
                        "server.toml.tunnel.gateway.users",
                        "must be an array of user tables",
                    )
                })?;
                let mut users = Vec::with_capacity(entries.len());
                for (index, value) in entries.iter().enumerate() {
                    let scope = format!("server.toml.tunnel.gateway.users[{index}]");
                    let table = value.as_object().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.gateway.users",
                            "every entry must be a table",
                        )
                    })?;
                    check_supported_keys(
                        table,
                        &scope,
                        &[
                            "name",
                            "tokenEnv",
                            "token_env",
                            "maxRoutes",
                            "max_routes",
                            "maxPorts",
                            "max_ports",
                            "maxDevices",
                            "max_devices",
                        ],
                    )?;
                    let name = table.get("name").and_then(Value::as_str).ok_or_else(|| {
                        materialize_error(&format!("{scope}.name"), "a user needs a name")
                    })?;
                    let token_env = table
                        .get("tokenEnv")
                        .or_else(|| table.get("token_env"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            materialize_error(
                                &format!("{scope}.tokenEnv"),
                                "a user needs the environment variable holding its token",
                            )
                        })?;
                    let mut user = json!({
                        "name": name.trim(),
                        "tokenEnv": token_env.trim(),
                    });
                    for (camel, snake, target) in [
                        ("maxRoutes", "max_routes", "maxRoutes"),
                        ("maxPorts", "max_ports", "maxPorts"),
                        ("maxDevices", "max_devices", "maxDevices"),
                    ] {
                        if let Some(quota) = table.get(camel).or_else(|| table.get(snake)) {
                            let value = quota.as_u64().ok_or_else(|| {
                                materialize_error(
                                    &format!("{scope}.{camel}"),
                                    "must be a positive integer",
                                )
                            })?;
                            user[target] = json!(value);
                        }
                    }
                    let parsed: sdkwork_webserver_tunnel_core::TunnelUserConfig =
                        serde_json::from_value(user)
                            .map_err(|error| materialize_error(&scope, error.to_string()))?;
                    parsed
                        .validate()
                        .map_err(|error| materialize_error(&scope, error.to_string()))?;
                    users.push(json!({
                        "name": name.trim(),
                        "tokenEnv": token_env.trim(),
                        "maxRoutes": parsed.max_routes,
                        "maxPorts": parsed.max_ports,
                        "maxDevices": parsed.max_devices,
                    }));
                }
                mapped["users"] = Value::Array(users);
            }
            value["gateway"] = mapped;
        }
        if let Some(agent) = tunnel.get("agent").and_then(Value::as_object) {
            check_supported_keys(
                agent,
                "server.toml.tunnel.agent",
                &[
                    "endpoint",
                    "endpoints",
                    "tokenEnv",
                    "token_env",
                    "deviceIdEnv",
                    "device_id_env",
                    "deviceName",
                    "device_name",
                    "domainSuffix",
                    "domain_suffix",
                    "user",
                    "loginFailExit",
                    "login_fail_exit",
                    "tls",
                ],
            )?;
            let mut mapped = json!({});
            if let Some(entry) = agent.get("endpoint") {
                mapped["endpoint"] = entry.clone();
            }
            // FRP `loginFailExit`: stop instead of retrying when the gateway
            // rejects our credentials. A non-boolean is refused rather than
            // coerced, because "true" spelled as a string would silently keep
            // the retry behaviour it was meant to switch off.
            if let Some(entry) = agent
                .get("loginFailExit")
                .or_else(|| agent.get("login_fail_exit"))
            {
                let value = entry.as_bool().ok_or_else(|| {
                    materialize_error(
                        "server.toml.tunnel.agent.loginFailExit",
                        "must be true or false",
                    )
                })?;
                mapped["loginFailExit"] = json!(value);
            }
            // The machine's user claim (FRP `user`). Validated here so a typo
            // is a configuration error on the agent, not a registration the
            // gateway refuses with "the token does not speak for that user".
            if let Some(entry) = agent.get("user") {
                let raw = entry.as_str().ok_or_else(|| {
                    materialize_error("server.toml.tunnel.agent.user", "must be a user name")
                })?;
                if !raw.trim().is_empty() {
                    sdkwork_webserver_tunnel_core::validate_user_name(raw).map_err(|error| {
                        materialize_error("server.toml.tunnel.agent.user", error.to_string())
                    })?;
                    mapped["user"] = json!(raw.trim());
                }
            }
            // Ordered failover addresses (FRP `serverAddr` list). Each entry is
            // validated here so a typo is a configuration error naming the key,
            // instead of a fallback that silently never works.
            if let Some(entry) = agent.get("endpoints") {
                let entries = entry.as_array().ok_or_else(|| {
                    materialize_error(
                        "server.toml.tunnel.agent.endpoints",
                        "must be an array of host:port addresses",
                    )
                })?;
                let mut addresses = Vec::with_capacity(entries.len());
                for value in entries {
                    let address = value.as_str().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.agent.endpoints",
                            "every entry must be a host:port string",
                        )
                    })?;
                    sdkwork_webserver_tunnel_core::validate_endpoint_authority(address).map_err(
                        |error| {
                            materialize_error(
                                "server.toml.tunnel.agent.endpoints",
                                error.to_string(),
                            )
                        },
                    )?;
                    addresses.push(Value::String(address.trim().to_owned()));
                }
                mapped["endpoints"] = Value::Array(addresses);
            }
            for (source, target) in [
                ("tokenEnv", "tokenEnv"),
                ("token_env", "tokenEnv"),
                ("deviceIdEnv", "deviceIdEnv"),
                ("device_id_env", "deviceIdEnv"),
                ("deviceName", "deviceName"),
                ("device_name", "deviceName"),
                ("domainSuffix", "domainSuffix"),
                ("domain_suffix", "domainSuffix"),
            ] {
                if let Some(entry) = agent.get(source) {
                    mapped[target] = entry.clone();
                }
            }
            if let Some(tls) = agent.get("tls") {
                check_supported_keys(
                    tls.as_object().ok_or_else(|| {
                        materialize_error("server.toml.tunnel.agent.tls", "tls must be a table")
                    })?,
                    "server.toml.tunnel.agent.tls",
                    &[
                        "caPemPath",
                        "ca_pem_path",
                        "pinnedServerSha256",
                        "pinned_server_sha256",
                        "insecureSkipVerify",
                        "insecure_skip_verify",
                    ],
                )?;
                mapped["tls"] = json!({
                    "caPemPath": tls.get("caPemPath").or_else(|| tls.get("ca_pem_path")).cloned(),
                    "pinnedServerSha256": tls.get("pinnedServerSha256").or_else(|| tls.get("pinned_server_sha256")).cloned(),
                    "insecureSkipVerify": tls.get("insecureSkipVerify").or_else(|| tls.get("insecure_skip_verify")).and_then(Value::as_bool).unwrap_or(false),
                });
            }
            value["agent"] = mapped;
        }
        if let Some(network) = tunnel.get("network").and_then(Value::as_object) {
            check_supported_keys(
                network,
                "server.toml.tunnel.network",
                &[
                    "heartbeatIntervalSecs",
                    "heartbeat_interval",
                    "reconnect",
                    "initialBackoffSecs",
                    "initial_backoff",
                    "maxBackoffSecs",
                    "max_backoff",
                    "keepAliveIntervalSecs",
                    "keep_alive_interval",
                    "heartbeatTimeoutSecs",
                    "heartbeat_timeout",
                    "udpPacketSize",
                    "udp_packet_size",
                    "maxConcurrentStreams",
                    "max_concurrent_streams",
                    "dialServerLocalIP",
                    "dial_server_local_ip",
                    "dialLocalAddr",
                    "dial_local_addr",
                    "dnsServer",
                    "dns_server",
                ],
            )?;
            let mut mapped = json!({
                "heartbeatIntervalSecs": network.get("heartbeatIntervalSecs").or_else(|| network.get("heartbeat_interval")).and_then(Value::as_u64).unwrap_or(10),
                "reconnect": network.get("reconnect").and_then(Value::as_bool).unwrap_or(true),
                "initialBackoffSecs": network.get("initialBackoffSecs").or_else(|| network.get("initial_backoff")).and_then(Value::as_u64).unwrap_or(1),
                "maxBackoffSecs": network.get("maxBackoffSecs").or_else(|| network.get("max_backoff")).and_then(Value::as_u64).unwrap_or(60),
            });
            // Transport tunables (FRP `quic.keepalivePeriod`,
            // `transport.heartbeatTimeout`, `transport.udpPacketSize`,
            // `quic.maxIncomingStreams`). Absent keys stay absent so the core
            // defaults (derived, not hard-coded) apply; a present key is
            // validated here with the config path in the message.
            for (camel, snake, target) in [
                (
                    "keepAliveIntervalSecs",
                    "keep_alive_interval",
                    "keepAliveIntervalSecs",
                ),
                (
                    "heartbeatTimeoutSecs",
                    "heartbeat_timeout",
                    "heartbeatTimeoutSecs",
                ),
                ("udpPacketSize", "udp_packet_size", "udpPacketSize"),
                (
                    "maxConcurrentStreams",
                    "max_concurrent_streams",
                    "maxConcurrentStreams",
                ),
            ] {
                let Some(entry) = network.get(camel).or_else(|| network.get(snake)) else {
                    continue;
                };
                let value = entry.as_u64().ok_or_else(|| {
                    materialize_error(
                        &format!("server.toml.tunnel.network.{camel}"),
                        "must be a non-negative integer",
                    )
                })?;
                mapped[target] = json!(value);
            }
            // FRP `transport.dialServerLocalIP`: the source address for target
            // dials. Parsed as an IP so a hostname is a configuration error
            // rather than an address the kernel would refuse at the first
            // relayed connection.
            if let Some(entry) = network
                .get("dialServerLocalIP")
                .or_else(|| network.get("dial_server_local_ip"))
                .or_else(|| network.get("dialLocalAddr"))
                .or_else(|| network.get("dial_local_addr"))
            {
                let raw = entry.as_str().ok_or_else(|| {
                    materialize_error(
                        "server.toml.tunnel.network.dialServerLocalIP",
                        "must be an IP address string",
                    )
                })?;
                let parsed: std::net::IpAddr = raw.trim().parse().map_err(|_| {
                    materialize_error(
                        "server.toml.tunnel.network.dialServerLocalIP",
                        format!("`{raw}` is not a literal IP address"),
                    )
                })?;
                mapped["dialLocalAddr"] = json!(parsed.to_string());
            }
            // FRP `dnsServer`: the resolver the agent uses for the gateway
            // host. Validated through the core model, so a hostname — which
            // would put the system resolver back into the path the operator
            // just removed — is a configuration error.
            if let Some(entry) = network
                .get("dnsServer")
                .or_else(|| network.get("dns_server"))
            {
                let raw = entry.as_str().ok_or_else(|| {
                    materialize_error(
                        "server.toml.tunnel.network.dnsServer",
                        "must be an IP address string, optionally with a port",
                    )
                })?;
                sdkwork_webserver_tunnel_core::parse_dns_server(raw).map_err(|error| {
                    materialize_error("server.toml.tunnel.network.dnsServer", error)
                })?;
                mapped["dnsServer"] = json!(raw.trim());
            }
            value["network"] = mapped;
        }
        if let Some(timeout) = tunnel.get("timeout").and_then(Value::as_object) {
            check_supported_keys(
                timeout,
                "server.toml.tunnel.timeout",
                &["connect", "handshake", "streamOpen", "stream_open", "idle"],
            )?;
            value["timeout"] = json!({
                "connect": timeout.get("connect").and_then(Value::as_u64).unwrap_or(10),
                "handshake": timeout.get("handshake").and_then(Value::as_u64).unwrap_or(10),
                "streamOpen": timeout.get("streamOpen").or_else(|| timeout.get("stream_open")).and_then(Value::as_u64).unwrap_or(10),
                "idle": timeout.get("idle").and_then(Value::as_u64).unwrap_or(300),
            });
        }
        if let Some(limits) = tunnel.get("limits").and_then(Value::as_object) {
            check_supported_keys(
                limits,
                "server.toml.tunnel.limits",
                &[
                    "maxDevices",
                    "max_devices",
                    "maxSessions",
                    "max_sessions",
                    "maxStreamsPerSession",
                    "max_streams_per_session",
                    "maxRoutes",
                    "max_routes",
                    "maxControlMessageBytes",
                    "max_control_message_bytes",
                    "maxConnections",
                    "max_connections",
                ],
            )?;
            // Only the ceilings the operator wrote are emitted. Emitting
            // `null` for the rest made every *partial* `[tunnel.limits]` table
            // fail deserialization ("invalid type: null, expected u32") even
            // though every key is optional by contract.
            let mut mapped: Map<String, Value> = Map::new();
            for (camel, snake) in [
                ("maxDevices", "max_devices"),
                ("maxSessions", "max_sessions"),
                ("maxStreamsPerSession", "max_streams_per_session"),
                ("maxRoutes", "max_routes"),
                ("maxControlMessageBytes", "max_control_message_bytes"),
                // The gateway-wide connection ceiling was in the runtime model
                // but not on the TOML surface, so the value an operator wrote
                // here was rejected while the default silently applied.
                ("maxConnections", "max_connections"),
            ] {
                if let Some(entry) = limits.get(camel).or_else(|| limits.get(snake)) {
                    mapped.insert(camel.to_owned(), entry.clone());
                }
            }
            value["limits"] = Value::Object(mapped);
        }
        if let Some(routes) = tunnel.get("routes").and_then(Value::as_array) {
            let mut mapped_routes = Vec::with_capacity(routes.len());
            for route in routes {
                let route = route.as_object().ok_or_else(|| {
                    materialize_error("server.toml.tunnel.routes", "route entries must be tables")
                })?;
                check_supported_keys(
                    route,
                    "server.toml.tunnel.routes[]",
                    &[
                        "name",
                        "protocol",
                        "domain",
                        "port",
                        "target",
                        "policy",
                        "proxyProtocol",
                        "proxy_protocol",
                        "locations",
                        "bandwidthLimit",
                        "bandwidth_limit",
                        "loadBalancerGroup",
                        "load_balancer_group",
                        "healthCheck",
                        "health_check",
                        "secretKeyEnv",
                        "secret_key_env",
                        "hostHeaderRewrite",
                        "host_header_rewrite",
                        "requestHeaders",
                        "request_headers",
                        "responseHeaders",
                        "response_headers",
                        "annotations",
                    ],
                )?;
                let mut mapped = json!({
                    "name": route.get("name").and_then(Value::as_str)
                        .ok_or_else(|| materialize_error("server.toml.tunnel.routes[].name", "name is required"))?,
                    "protocol": route.get("protocol").and_then(Value::as_str)
                        .ok_or_else(|| materialize_error("server.toml.tunnel.routes[].protocol", "protocol is required"))?,
                    "target": route.get("target").and_then(Value::as_str)
                        .ok_or_else(|| materialize_error("server.toml.tunnel.routes[].target", "target is required"))?,
                });
                if let Some(domain) = route.get("domain") {
                    mapped["domain"] = domain.clone();
                }
                if let Some(port) = route.get("port") {
                    mapped["port"] = port.clone();
                }
                // PROXY protocol toward the agent's local target (FRP
                // `transport.proxyProtocolVersion` parity): a route-level
                // transport choice, validated here so a typo cannot silently
                // relay un-prefixed traffic to a service that requires it.
                if let Some(version) = route
                    .get("proxyProtocol")
                    .or_else(|| route.get("proxy_protocol"))
                {
                    let label = version.as_str().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.routes[].proxyProtocol",
                            "must be a string (v1 or v2)",
                        )
                    })?;
                    if sdkwork_webserver_tunnel_core::ProxyProtocolVersion::parse(label).is_none() {
                        return Err(materialize_error(
                            "server.toml.tunnel.routes[].proxyProtocol",
                            format!(
                                "`{label}` is not a PROXY protocol version (expected v1 or v2)"
                            ),
                        ));
                    }
                    mapped["proxyProtocol"] = json!(label);
                }
                // Path claims (FRP `locations` parity) are validated here so a
                // typo cannot reach the agent as a route that owns a host but
                // answers no path.
                if let Some(locations) = route.get("locations") {
                    let entries = locations.as_array().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.routes[].locations",
                            "must be an array of path prefixes",
                        )
                    })?;
                    let mut normalized = Vec::with_capacity(entries.len());
                    for entry in entries {
                        let raw = entry.as_str().ok_or_else(|| {
                            materialize_error(
                                "server.toml.tunnel.routes[].locations",
                                "entries must be strings",
                            )
                        })?;
                        let location = sdkwork_webserver_tunnel_core::validate_location(raw)
                            .map_err(|error| {
                                materialize_error(
                                    "server.toml.tunnel.routes[].locations",
                                    error.to_string(),
                                )
                            })?;
                        if !normalized.contains(&location) {
                            normalized.push(location);
                        }
                    }
                    mapped["locations"] = json!(normalized);
                }
                // Bandwidth ceiling (FRP `transport.bandwidthLimit` parity).
                // Written the way operators write it — `"2MB"`, `"512KB"` —
                // or as a plain byte-per-second integer; both normalize to
                // bytes per second in the runtime model.
                if let Some(limit) = route
                    .get("bandwidthLimit")
                    .or_else(|| route.get("bandwidth_limit"))
                {
                    let bytes = match limit {
                        Value::String(raw) => sdkwork_webserver_tunnel_core::parse_bandwidth_limit(
                            raw,
                        )
                        .map_err(|error| {
                            materialize_error(
                                "server.toml.tunnel.routes[].bandwidthLimit",
                                error.to_string(),
                            )
                        })?,
                        Value::Number(number) => {
                            number.as_u64().filter(|value| *value > 0).ok_or_else(
                                || {
                                    materialize_error(
                                        "server.toml.tunnel.routes[].bandwidthLimit",
                                        "must be a positive byte-per-second number or a size like \"2MB\"",
                                    )
                                },
                            )?
                        }
                        _ => {
                            return Err(materialize_error(
                                "server.toml.tunnel.routes[].bandwidthLimit",
                                "must be a size string (\"2MB\") or a byte-per-second number",
                            ))
                        }
                    };
                    mapped["bandwidthLimitBytesPerSecond"] = json!(bytes);
                }
                // Secret route key source (FRP `sk` parity): the name of an
                // environment variable, never the key itself.
                if let Some(env_name) = route
                    .get("secretKeyEnv")
                    .or_else(|| route.get("secret_key_env"))
                {
                    let raw = env_name.as_str().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.routes[].secretKeyEnv",
                            "must be an environment variable name",
                        )
                    })?;
                    if raw.trim().is_empty() {
                        return Err(materialize_error(
                            "server.toml.tunnel.routes[].secretKeyEnv",
                            "must not be empty",
                        ));
                    }
                    mapped["secretKeyEnv"] = json!(raw.trim());
                }
                // Load-balancer group (FRP `loadBalancer.group` parity).
                if let Some(group) = route
                    .get("loadBalancerGroup")
                    .or_else(|| route.get("load_balancer_group"))
                {
                    let raw = group.as_str().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.routes[].loadBalancerGroup",
                            "must be a group name string",
                        )
                    })?;
                    let group = sdkwork_webserver_tunnel_core::validate_load_balancer_group(raw)
                        .map_err(|error| {
                            materialize_error(
                                "server.toml.tunnel.routes[].loadBalancerGroup",
                                error.to_string(),
                            )
                        })?;
                    mapped["loadBalancerGroup"] = json!(group);
                }
                // Health probe (FRP `healthCheck` parity).
                if let Some(health) = route
                    .get("healthCheck")
                    .or_else(|| route.get("health_check"))
                {
                    let table = health.as_object().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.routes[].healthCheck",
                            "must be a table",
                        )
                    })?;
                    check_supported_keys(
                        table,
                        "server.toml.tunnel.routes[].healthCheck",
                        &[
                            "kind",
                            "type",
                            "intervalSeconds",
                            "interval_seconds",
                            "interval",
                            "timeoutSeconds",
                            "timeout_seconds",
                            "timeout",
                            "maxFailed",
                            "max_failed",
                            "path",
                        ],
                    )?;
                    let pick = |camel: &str, snake: &str| {
                        table
                            .get(camel)
                            .or_else(|| table.get(snake))
                            .and_then(Value::as_i64)
                    };
                    let kind_raw = table
                        .get("kind")
                        .or_else(|| table.get("type"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            materialize_error(
                                "server.toml.tunnel.routes[].healthCheck.kind",
                                "kind is required (tcp or http)",
                            )
                        })?;
                    let kind = sdkwork_webserver_tunnel_core::HealthCheckKind::parse(kind_raw)
                        .ok_or_else(|| {
                            materialize_error(
                                "server.toml.tunnel.routes[].healthCheck.kind",
                                format!("`{kind_raw}` must be tcp or http"),
                            )
                        })?;
                    mapped["healthCheck"] = json!({
                        "kind": match kind {
                            sdkwork_webserver_tunnel_core::HealthCheckKind::Tcp => "tcp",
                            sdkwork_webserver_tunnel_core::HealthCheckKind::Http => "http",
                        },
                        "intervalSeconds": pick("intervalSeconds", "interval_seconds")
                            .or_else(|| pick("interval", "interval"))
                            .unwrap_or(10),
                        "timeoutSeconds": pick("timeoutSeconds", "timeout_seconds")
                            .or_else(|| pick("timeout", "timeout"))
                            .unwrap_or(3),
                        "maxFailed": pick("maxFailed", "max_failed").unwrap_or(1),
                        "path": table.get("path").cloned().unwrap_or(Value::Null),
                    });
                }
                // HTTP header rewriting (FRP `hostHeaderRewrite` +
                // `requestHeaders.set` / `responseHeaders.set`). Parsed into the
                // core model here so a bad name or value is a *configuration*
                // error with a config path, not a runtime failure on live
                // traffic.
                if let Some(host) = route
                    .get("hostHeaderRewrite")
                    .or_else(|| route.get("host_header_rewrite"))
                {
                    let raw = host.as_str().ok_or_else(|| {
                        materialize_error(
                            "server.toml.tunnel.routes[].hostHeaderRewrite",
                            "must be a host or host:port string",
                        )
                    })?;
                    mapped["hostHeaderRewrite"] = json!(raw.trim());
                }
                for (camel, snake, target, direction) in [
                    (
                        "requestHeaders",
                        "request_headers",
                        "requestHeaders",
                        sdkwork_webserver_tunnel_core::HeaderDirection::Request,
                    ),
                    (
                        "responseHeaders",
                        "response_headers",
                        "responseHeaders",
                        sdkwork_webserver_tunnel_core::HeaderDirection::Response,
                    ),
                ] {
                    let Some(headers) = route.get(camel).or_else(|| route.get(snake)) else {
                        continue;
                    };
                    let scope = format!("server.toml.tunnel.routes[].{camel}");
                    let set_scope = format!("{scope}.set");
                    let table = headers.as_object().ok_or_else(|| {
                        materialize_error(&scope, "must be a table with a `set` map")
                    })?;
                    check_supported_keys(table, &scope, &["set"])?;
                    let mut set = std::collections::BTreeMap::new();
                    if let Some(entries) = table.get("set") {
                        let entries = entries.as_object().ok_or_else(|| {
                            materialize_error(&set_scope, "must be a table of header name → value")
                        })?;
                        for (name, value) in entries {
                            let value = value.as_str().ok_or_else(|| {
                                materialize_error(
                                    &set_scope,
                                    format!("header `{name}` must have a string value"),
                                )
                            })?;
                            // Validate each pair through the core model, so the
                            // TOML surface cannot accept something the gateway
                            // would later refuse to apply.
                            let pair = sdkwork_webserver_tunnel_core::HeaderPair::parse(
                                name, value, direction,
                            )
                            .map_err(|error| materialize_error(&set_scope, error.to_string()))?;
                            set.insert(pair.name.clone(), pair.value.clone());
                        }
                    }
                    mapped[target] = json!({ "set": set });
                }
                // Operator metadata (FRP `annotations`): `[tunnel.routes.annotations]`
                // as a flat string table, validated by the route model so the
                // file cannot describe metadata the agent would refuse to send.
                if let Some(annotations) = route.get("annotations") {
                    let scope = "server.toml.tunnel.routes[].annotations";
                    let table = annotations.as_object().ok_or_else(|| {
                        materialize_error(scope, "must be a table of key → value")
                    })?;
                    let entries: std::collections::BTreeMap<String, String> = table
                        .iter()
                        .map(|(key, value)| {
                            value
                                .as_str()
                                .map(|value| (key.clone(), value.to_owned()))
                                .ok_or_else(|| {
                                    materialize_error(
                                        scope,
                                        format!("annotation `{key}` must have a string value"),
                                    )
                                })
                        })
                        .collect::<std::result::Result<_, _>>()?;
                    sdkwork_webserver_tunnel_core::route::validate_annotations(&entries)
                        .map_err(|error| materialize_error(scope, error.to_string()))?;
                    mapped["annotations"] = serde_json::to_value(entries)
                        .map_err(|error| materialize_error(scope, error.to_string()))?;
                }
                if let Some(policy) = route.get("policy").and_then(Value::as_object) {
                    check_supported_keys(
                        policy,
                        "server.toml.tunnel.routes[].policy",
                        &[
                            "allowPublic",
                            "allow_public",
                            "allowedIps",
                            "allowed_ips",
                            "auth",
                            "visitorTokens",
                            "visitor_tokens",
                        ],
                    )?;
                    mapped["policy"] = json!({
                        "allowPublic": policy.get("allowPublic").or_else(|| policy.get("allow_public")).and_then(Value::as_bool).unwrap_or(false),
                        "allowedIps": policy.get("allowedIps").or_else(|| policy.get("allowed_ips")).cloned().unwrap_or(Value::Array(Vec::new())),
                        "auth": policy.get("auth").cloned().unwrap_or(Value::String("none".to_owned())),
                        "visitorTokens": policy.get("visitorTokens").or_else(|| policy.get("visitor_tokens")).cloned().unwrap_or(Value::Array(Vec::new())),
                    });
                }
                mapped_routes.push(mapped);
            }
            value["routes"] = Value::Array(mapped_routes);
        }
        Ok(Some(value))
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        self,
        limits: Value,
        nginx: Value,
        gzip: Value,
        proxy_cache: Value,
        tunnel: Option<Value>,
        mime_types: Option<Value>,
        default_type: Option<Value>,
    ) -> Result<WebServerAppConfig, WebServerConfigError> {
        let instance = json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": self.app_key,
            "nginx": nginx,
            "mimeTypes": mime_types,
            "defaultType": default_type,
            "gzip": gzip,
            "limitReqZones": self.limit_req_zones,
            "limitConnZones": self.limit_conn_zones,
            "maps": self.maps,
            "limits": limits,
            "listeners": self.listeners,
            "certificates": self.certificates,
            "tlsPolicies": self.tls_policies,
            "resources": self.resources,
            "upstreams": self.upstreams,
            "virtualHosts": self.virtual_hosts,
            "streams": self.streams,
            "mail": { "servers": self.mail_servers },
            "proxyCache": proxy_cache,
            "tunnel": tunnel,
            "metadata": { "source": "deployments/webserver/server.toml" },
        });
        let config: WebServerAppConfig = serde_json::from_value(instance).map_err(|source| {
            WebServerConfigError::Materialize(format!(
                "runtime model deserialization failed: {source}"
            ))
        })?;
        validate_webserver_config(&config)?;
        Ok(config)
    }

    fn materialize_limit_conn_zones(
        &mut self,
        http: &Map<String, Value>,
    ) -> Result<(), WebServerConfigError> {
        let Some(entries) = http.get("limitConnZone").and_then(Value::as_array) else {
            return Ok(());
        };
        for (index, entry) in entries.iter().enumerate() {
            let path = format!("http.limitConnZone[{index}]");
            let text = entry
                .as_str()
                .ok_or_else(|| materialize_error(&path, "limitConnZone entries must be strings"))?;
            let zone = crate::config::parse_limit_conn_zone(text)
                .map_err(|error| materialize_error(&path, error.to_string()))?;
            if self.limit_conn_zone_names.contains(&zone.name) {
                return Err(materialize_error(
                    &path,
                    format!("duplicate limitConnZone name `{}`", zone.name),
                ));
            }
            self.limit_conn_zone_names.push(zone.name.clone());
            self.limit_conn_zones.push(json!({
                "name": zone.name,
                "key": zone.key,
                "maxKeys": zone.max_keys,
            }));
        }
        Ok(())
    }

    fn materialize_limit_req_zones(
        &mut self,
        http: &Map<String, Value>,
    ) -> Result<(), WebServerConfigError> {
        let Some(entries) = http.get("limitReqZone").and_then(Value::as_array) else {
            return Ok(());
        };
        for (index, entry) in entries.iter().enumerate() {
            let path = format!("http.limitReqZone[{index}]");
            let text = entry
                .as_str()
                .ok_or_else(|| materialize_error(&path, "limitReqZone entries must be strings"))?;
            let zone = crate::config::parse_limit_req_zone(text)
                .map_err(|error| materialize_error(&path, error.to_string()))?;
            if self.limit_req_zone_names.contains(&zone.name) {
                return Err(materialize_error(
                    &path,
                    format!("duplicate limitReqZone name `{}`", zone.name),
                ));
            }
            self.limit_req_zone_names.push(zone.name.clone());
            self.limit_req_zones.push(json!({
                "name": zone.name,
                "key": zone.key,
                "maxKeys": zone.max_keys,
                "ratePerSecond": zone.rate_per_second,
            }));
        }
        Ok(())
    }
}

/// Load and materialize a single `server.toml` file as a complete app
/// configuration (the TOML equivalent of a standalone `nginx.conf`).
///
/// The document uses the same typed surface as the layout v3 effective
/// configuration (`[main]`, `[http]`, `[[http.server]]`, `[[http.upstream]]`,
/// `[[stream.server]]`, `[http.certificates.*]`, `[nginx]`, `proxyCache`).
/// A root `profile` key is layout-merge metadata and is ignored here.
pub fn load_server_toml_file(
    path: impl AsRef<Path>,
    app_key: &str,
) -> Result<WebServerAppConfig, WebServerConfigError> {
    load_server_toml_file_with_compatibility(path, app_key).map(|(app, _)| app)
}

/// Load one `server.toml` file and return the accepted-but-unenforced key
/// diagnostics alongside the materialized app.
pub fn load_server_toml_file_with_compatibility(
    path: impl AsRef<Path>,
    app_key: &str,
) -> Result<(WebServerAppConfig, Vec<ConfigDiagnostic>), WebServerConfigError> {
    let path = path.as_ref();
    let bytes = super::loader::read_bounded_config(path)?;
    let text = std::str::from_utf8(&bytes).map_err(|source| {
        WebServerConfigError::Materialize(format!(
            "server.toml file {} is not valid UTF-8: {source}",
            path.display()
        ))
    })?;
    let value: toml::Value = toml::from_str(text).map_err(|source| WebServerConfigError::Toml {
        path: path.to_path_buf(),
        source,
    })?;
    let mut effective: Value = serde_json::to_value(&value).map_err(|source| {
        WebServerConfigError::Materialize(format!(
            "cannot convert TOML from {}: {source}",
            path.display()
        ))
    })?;
    if let Some(root) = effective.as_object_mut() {
        root.remove("profile");
    }
    let app = materialize_app(&effective, app_key)?;
    Ok((app, accepted_unenforced_toml_diagnostics(&effective)))
}

/// Load the three-layer effective TOML document of a layout v3 directory
/// (common + `server.<environment>.toml` + `server.<profile>.toml`) without
/// materializing it. Used by `load_server_toml_app` and by the merged
/// module-imports assembly.
pub fn load_server_toml_app_effective(
    dir: impl AsRef<Path>,
    profile: &str,
    environment: &str,
) -> Result<Value, WebServerConfigError> {
    let dir = dir.as_ref();
    let common_path = dir.join("server.common.toml");
    let environment_path = dir.join(format!("server.{environment}.toml"));
    let profile_path = dir.join(format!("server.{profile}.toml"));
    if !common_path.exists() || !profile_path.exists() {
        return Err(WebServerConfigError::Materialize(format!(
            "layout v3 requires {} and {} in {}",
            common_path.display(),
            profile_path.display(),
            dir.display()
        )));
    }
    let common = parse_toml_file(&common_path)?;
    let profile_doc = parse_toml_file(&profile_path)?;
    let declared_profile = profile_doc.get("profile").and_then(Value::as_str);
    if declared_profile != Some(profile) {
        return Err(WebServerConfigError::Materialize(format!(
            "{} must declare profile = \"{profile}\" (found {declared_profile:?})",
            profile_path.display()
        )));
    }
    if environment_path.exists() {
        let environment_doc = parse_toml_file(&environment_path)?;
        let declared_environment = environment_doc.get("environment").and_then(Value::as_str);
        if declared_environment != Some(environment) {
            return Err(WebServerConfigError::Materialize(format!(
                "{} must declare environment = \"{environment}\" (found {declared_environment:?})",
                environment_path.display()
            )));
        }
        let mut effective = merge_effective(&common, &environment_doc, &profile_doc)?;
        expand_snippet_includes(&mut effective, dir)?;
        Ok(effective)
    } else {
        let mut effective = merge_common_profile(&common, &profile_doc)?;
        expand_snippet_includes(&mut effective, dir)?;
        Ok(effective)
    }
}

/// Load and materialize a layout v3 `server.toml` directory.
///
/// `dir` must contain `server.common.toml`, `server.<environment>.toml`, and
/// `server.<profile>.toml`; `profile` is `"standalone"` or `"cloud"`.
pub fn load_server_toml_app(
    dir: impl AsRef<Path>,
    profile: &str,
    environment: &str,
    app_key: &str,
) -> Result<WebServerAppConfig, WebServerConfigError> {
    load_server_toml_app_with_compatibility(dir, profile, environment, app_key).map(|(app, _)| app)
}

/// Load a layout v3 directory and return the accepted-but-unenforced key
/// diagnostics alongside the materialized app.
pub fn load_server_toml_app_with_compatibility(
    dir: impl AsRef<Path>,
    profile: &str,
    environment: &str,
    app_key: &str,
) -> Result<(WebServerAppConfig, Vec<ConfigDiagnostic>), WebServerConfigError> {
    let effective = load_server_toml_app_effective(dir, profile, environment)?;
    let app = materialize_app(&effective, app_key)?;
    Ok((app, accepted_unenforced_toml_diagnostics(&effective)))
}

/// Resolve `[http.defaults.tls]` shared TLS declarations.
///
/// SDKWORK_WEBSERVER_SPEC.md: `defaults` is a typed key under `[http]` and
/// `[http.defaults.tls]` carries shared `protocols`, `preferServerCiphers`,
/// `sessionCache`, ... entries that are merged into each `[[http.server]].tls`
/// at effective-config time. Only the `tls` sub-table is supported today; its
/// keys use the same server TLS key set.
fn materialize_http_defaults(
    http: &Map<String, Value>,
) -> Result<Option<Map<String, Value>>, WebServerConfigError> {
    let Some(defaults) = http.get("defaults").and_then(Value::as_object) else {
        return Ok(None);
    };
    check_supported_keys(defaults, "server.toml.http.defaults", &["tls"])?;
    let Some(tls) = defaults.get("tls").and_then(Value::as_object) else {
        return Ok(None);
    };
    check_supported_keys(tls, "server.toml.http.defaults.tls", SERVER_TLS_KEYS)?;
    Ok(Some(tls.clone()))
}

/// Overlay `[http.defaults.tls]` onto one `[[http.server]]` entry.
///
/// Server-level `tls` values win over the shared defaults (nginx override
/// semantics: the closest declaration to the server block applies). When the
/// server declares no `tls` of its own, the defaults table is applied as-is.
fn apply_server_tls_defaults(
    server: &Map<String, Value>,
    defaults_tls: Option<&Map<String, Value>>,
) -> Result<Map<String, Value>, WebServerConfigError> {
    let Some(defaults_tls) = defaults_tls else {
        return Ok(server.clone());
    };
    let mut effective = server.clone();
    let mut tls = defaults_tls.clone();
    if let Some(server_tls) = server.get("tls").and_then(Value::as_object) {
        for (key, value) in server_tls {
            tls.insert(key.clone(), value.clone());
        }
    }
    effective.insert("tls".to_owned(), Value::Object(tls));
    Ok(effective)
}

/// Expand layout v3 `include` snippet declarations into the typed TOML model
/// (SDKWORK_WEBSERVER_SPEC.md §11.2/§11.3): server-level `include` pulls in
/// `location` blocks (including Adaptive Web `@pc` / `@h5` named locations),
/// and location-level `include` merges snippet directives into the including
/// location. Snippets are nginx-conf fragments with paths relative to
/// `deployments/webserver/`; the module directory itself is accepted as the
/// checkout-layout fallback.
fn expand_snippet_includes(
    root: &mut Value,
    module_dir: &Path,
) -> Result<(), WebServerConfigError> {
    let Some(servers) = root
        .get_mut("http")
        .and_then(|http| http.get_mut("server"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(());
    };
    for (server_index, server) in servers.iter_mut().enumerate() {
        let Some(server) = server.as_object_mut() else {
            continue;
        };
        let path = format!("http.server[{server_index}]");
        if let Some(patterns) = server
            .remove("include")
            .and_then(|value| value.as_array().cloned())
        {
            for pattern in patterns {
                let pattern = pattern.as_str().ok_or_else(|| {
                    materialize_error(&path, "server `include` entries must be strings")
                })?;
                let locations = load_snippet_locations(module_dir, pattern, &path)?;
                let entry = server
                    .entry("location")
                    .or_insert_with(|| Value::Array(Vec::new()));
                let Value::Array(entry) = entry else {
                    return Err(materialize_error(&path, "`location` must be an array"));
                };
                for location in locations {
                    upsert_location_by_match(entry, location);
                }
            }
        }
        let Some(locations) = server.get_mut("location").and_then(Value::as_array_mut) else {
            continue;
        };
        for location in locations.iter_mut() {
            let Some(location) = location.as_object_mut() else {
                continue;
            };
            let Some(patterns) = location
                .remove("include")
                .and_then(|value| value.as_array().cloned())
            else {
                continue;
            };
            for pattern in patterns {
                let pattern = pattern.as_str().ok_or_else(|| {
                    materialize_error(&path, "location `include` entries must be strings")
                })?;
                merge_snippet_into_location(module_dir, pattern, location, &path)?;
            }
        }
    }
    Ok(())
}

/// Insert a snippet-derived location, replacing an existing declaration with
/// the same `match` (TOML merge identity for `http.server.location`).
fn upsert_location_by_match(locations: &mut Vec<Value>, location: Map<String, Value>) {
    let match_value = location.get("match").and_then(Value::as_str);
    if let Some(existing) = locations
        .iter_mut()
        .find(|existing| existing.get("match").and_then(Value::as_str) == match_value)
    {
        *existing = Value::Object(location);
        return;
    }
    locations.push(Value::Object(location));
}

/// Resolve one snippet pattern against the module directory candidates and
/// parse it into typed location tables (server-level include).
fn load_snippet_locations(
    module_dir: &Path,
    pattern: &str,
    path: &str,
) -> Result<Vec<Map<String, Value>>, WebServerConfigError> {
    let directives = load_snippet_directives(module_dir, pattern, path)?;
    let mut locations = Vec::new();
    for directive in &directives {
        if directive.name != "location" {
            return Err(materialize_error(
                path,
                format!(
                    "snippet `{pattern}`: server-level include accepts only `location` blocks, found `{}`",
                    directive.name
                ),
            ));
        }
        locations.push(snippet_location_to_toml(directive, path, pattern)?);
    }
    Ok(locations)
}

/// Merge a location-level snippet: its top-level directives become keys of
/// the including location. Nested `location` blocks fail closed — declare
/// them in a server-level snippet instead.
fn merge_snippet_into_location(
    module_dir: &Path,
    pattern: &str,
    location: &mut Map<String, Value>,
    path: &str,
) -> Result<(), WebServerConfigError> {
    let directives = load_snippet_directives(module_dir, pattern, path)?;
    for directive in &directives {
        if directive.name == "location" {
            return Err(materialize_error(
                path,
                format!(
                    "snippet `{pattern}`: nested `location` blocks inside a location-level include are not supported; declare them in a server-level include"
                ),
            ));
        }
        apply_snippet_directive(directive, location, path, pattern)?;
    }
    Ok(())
}

fn load_snippet_directives(
    module_dir: &Path,
    pattern: &str,
    path: &str,
) -> Result<Vec<NginxDirective>, WebServerConfigError> {
    let snippet = resolve_snippet_path(module_dir, pattern)?;
    let text =
        crate::fs_bounds::read_text_capped(&snippet, crate::fs_bounds::MAX_IMPORT_SOURCE_BYTES)
            .map_err(|error| {
                materialize_error(path, format!("read snippet `{pattern}`: {error}"))
            })?;
    parse_nginx_config(&text, &snippet)
        .map_err(|error| materialize_error(path, format!("parse snippet `{pattern}`: {error}")))
}

/// Snippet paths resolve against `deployments/webserver/` (install layout)
/// with the module directory itself as the checkout-layout fallback.
fn resolve_snippet_path(module_dir: &Path, pattern: &str) -> Result<PathBuf, WebServerConfigError> {
    if Path::new(pattern).is_absolute() {
        return Ok(PathBuf::from(pattern));
    }
    let candidates = [
        module_dir.to_path_buf(),
        module_dir.join("deployments").join("webserver"),
    ];
    for base in &candidates {
        let candidate = base.join(pattern);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(materialize_error(
        "server.toml.http.server.include",
        format!(
            "snippet `{pattern}` not found under {} (or {})",
            candidates[0].display(),
            candidates[1].display()
        ),
    ))
}

/// Convert one snippet `location <match> { … }` block into the typed TOML
/// location model. Mirrors `nginx::mapping` named-location semantics:
/// `@pc` / `@h5` carry only `root` / `index` / `try_files`.
fn snippet_location_to_toml(
    directive: &NginxDirective,
    path: &str,
    pattern: &str,
) -> Result<Map<String, Value>, WebServerConfigError> {
    if directive.args.is_empty() {
        return Err(materialize_error(
            path,
            format!("snippet `{pattern}`: `location` requires a match argument"),
        ));
    }
    // nginx tokenizes the match modifier as its own argument
    // (`location = /healthz` → `["=", "/healthz"]`); typed TOML stores the
    // combined form (`= /healthz`).
    let match_value = match directive.args.as_slice() {
        [modifier, rest] if matches!(modifier.as_str(), "=" | "^~" | "~" | "~*") => {
            format!("{modifier} {rest}")
        }
        [match_value, ..] => match_value.clone(),
        [] => unreachable!("args checked non-empty above"),
    };
    let mut table = Map::new();
    table.insert("match".to_owned(), Value::String(match_value));
    for child in &directive.children {
        apply_snippet_directive(child, &mut table, path, pattern)?;
    }
    Ok(table)
}

/// Map one nginx directive from a snippet onto the typed location keys. The
/// mapped set matches what `nginx::mapping` executes; anything else fails
/// closed instead of silently dropping behavior.
fn apply_snippet_directive(
    directive: &NginxDirective,
    location: &mut Map<String, Value>,
    path: &str,
    pattern: &str,
) -> Result<(), WebServerConfigError> {
    let unsupported = |detail: String| {
        materialize_error(
            path,
            format!(
                "snippet `{pattern}`: {detail} (directive `{}`)",
                directive.name
            ),
        )
    };
    let first_arg = || directive.args.first().cloned();
    match directive.name.as_str() {
        "try_files" => {
            if directive.args.is_empty() {
                return Err(unsupported(
                    "`try_files` requires at least one argument".to_owned(),
                ));
            }
            let entries = directive
                .args
                .iter()
                .map(|value| Value::String(value.clone()))
                .collect();
            location.insert("tryFiles".to_owned(), Value::Array(entries));
        }
        "proxy_pass" => {
            let Some(target) = first_arg() else {
                return Err(unsupported("`proxy_pass` requires a target".to_owned()));
            };
            location.insert("proxyPass".to_owned(), Value::String(target));
        }
        "proxy_http_version" => {
            let Some(version) = first_arg() else {
                return Err(unsupported(
                    "`proxy_http_version` requires a version".to_owned(),
                ));
            };
            location.insert("proxyHttpVersion".to_owned(), Value::String(version));
        }
        "proxy_set_header" => {
            let [name, value] = directive.args.as_slice() else {
                return Err(unsupported(
                    "`proxy_set_header` requires a header name and a value".to_owned(),
                ));
            };
            let entry = Value::String(format!("{name} {value}"));
            let headers = location
                .entry("proxySetHeader")
                .or_insert_with(|| Value::Array(Vec::new()));
            let Value::Array(headers) = headers else {
                return Err(unsupported("`proxySetHeader` must be an array".to_owned()));
            };
            headers.push(entry);
        }
        "proxy_buffering" => {
            let Some(state) = first_arg() else {
                return Err(unsupported("`proxy_buffering` requires on|off".to_owned()));
            };
            let enabled = match state.as_str() {
                "on" => true,
                "off" => false,
                other => {
                    return Err(unsupported(format!(
                        "`proxy_buffering` expects on|off, found `{other}`"
                    )))
                }
            };
            location.insert("proxyBuffering".to_owned(), Value::Bool(enabled));
        }
        "proxy_read_timeout" => {
            let Some(timeout) = first_arg() else {
                return Err(unsupported(
                    "`proxy_read_timeout` requires a duration".to_owned(),
                ));
            };
            location.insert("proxyReadTimeout".to_owned(), Value::String(timeout));
        }
        "root" => {
            let Some(root) = first_arg() else {
                return Err(unsupported("`root` requires a path".to_owned()));
            };
            if !root.starts_with('/') {
                return Err(unsupported(
                    "`root` must be an absolute directory for nginx compatibility".to_owned(),
                ));
            }
            location.insert("root".to_owned(), Value::String(root));
        }
        "alias" => {
            let Some(alias) = first_arg() else {
                return Err(unsupported("`alias` requires a path".to_owned()));
            };
            location.insert("alias".to_owned(), Value::String(alias));
        }
        "index" => {
            if directive.args.is_empty() {
                return Err(unsupported("`index` requires at least one file".to_owned()));
            }
            let files = directive
                .args
                .iter()
                .map(|value| Value::String(value.clone()))
                .collect();
            location.insert("index".to_owned(), Value::Array(files));
        }
        "return" => {
            let Some(status) = directive
                .args
                .first()
                .and_then(|value| value.parse::<u64>().ok())
            else {
                return Err(unsupported("`return` requires a numeric status".to_owned()));
            };
            location.insert("returnStatus".to_owned(), Value::from(status));
            if let Some(body) = directive.args.get(1) {
                location.insert("returnBody".to_owned(), Value::String(body.clone()));
            }
        }
        // Accepted like nginx named-location mapping: response headers on
        // dispatch/named locations are declarative only (the data plane owns
        // the Adaptive Web `Vary` header).
        "add_header" => {}
        other => {
            return Err(unsupported(format!(
                "directive `{other}` is not supported in snippets; materialize it as a typed TOML key"
            )));
        }
    }
    Ok(())
}

/// Materialize an already-merged effective TOML document.
pub fn materialize_app(
    effective: &Value,
    app_key: &str,
) -> Result<WebServerAppConfig, WebServerConfigError> {
    let root = as_object(effective, "server.toml")?;
    if root.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Err(WebServerConfigError::Materialize(
            "effective configuration has enabled = false; no runtime app can be materialized"
                .to_owned(),
        ));
    }
    check_supported_keys(
        root,
        "server.toml",
        &[
            "specVersion",
            "kind",
            "id",
            "enabled",
            "description",
            "nginx",
            "main",
            "http",
            "stream",
            "mail",
            "proxyCache",
            "tunnel",
        ],
    )?;
    // Keep wording aligned with sdkwork-specs/tools/webserver/retired-nginx.mjs.
    const RETIRED_COMPATIBILITY: &str =
        "retired; migrate to [nginx] (nginx.enabled, nginx.profile) per SDKWORK_WEBSERVER_SPEC.md §4.1";
    const RETIRED_NGINX_PROFILE: &str = "retired; rename to nginx.profile";
    if root.get("compatibility").is_some() {
        return Err(materialize_error(
            "server.toml.compatibility",
            RETIRED_COMPATIBILITY,
        ));
    }
    let mut nginx = match root.get("nginx").and_then(Value::as_object) {
        Some(nginx) => {
            if nginx.get("nginxProfile").is_some() {
                return Err(materialize_error(
                    "server.toml.nginx.nginxProfile",
                    RETIRED_NGINX_PROFILE,
                ));
            }
            check_supported_keys(
                nginx,
                "server.toml.nginx",
                &[
                    "enabled",
                    "profile",
                    "unknownDirectivePolicy",
                    "exceptionRef",
                    "strict",
                    "confFile",
                ],
            )?;
            json!({
                "enabled": nginx.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                "profile": nginx.get("profile").and_then(Value::as_str).unwrap_or("http-core-v1"),
                "unknownDirectivePolicy": nginx.get("unknownDirectivePolicy").and_then(Value::as_str).unwrap_or("error"),
            })
        }
        None => {
            json!({ "enabled": true, "profile": "http-core-v1", "unknownDirectivePolicy": "error" })
        }
    };
    let mut limits = json!({});
    let mut gzip = json!({ "enabled": false, "types": [], "minLength": 20 });
    let mut mime_types_json: Option<Value> = None;
    let mut default_type_json: Option<Value> = None;
    if let Some(http) = root.get("http").and_then(Value::as_object) {
        if let Some(body_size) = http.get("clientMaxBodySize").and_then(Value::as_str) {
            let bytes = materialize_size_bytes(body_size);
            if bytes > 0 {
                limits["maxRequestBodyBytes"] = Value::from(bytes);
            }
        }
        if let Some(keepalive) = http.get("keepaliveTimeout").and_then(Value::as_u64) {
            limits["requestTimeoutMs"] = Value::from(keepalive.saturating_mul(1000));
        }
        if let Some(enabled) = http.get("gzip").and_then(Value::as_bool) {
            gzip["enabled"] = Value::Bool(enabled);
        }
        if let Some(types) = http.get("gzipTypes").and_then(Value::as_array) {
            gzip["types"] = Value::Array(
                types
                    .iter()
                    .filter_map(|entry| entry.as_str().map(|value| Value::String(value.to_owned())))
                    .collect(),
            );
        }
        if let Some(min_length) = http
            .get("gzipMinLength")
            .and_then(|value| value.as_u64().or_else(|| value.as_i64().map(|n| n as u64)))
        {
            gzip["minLength"] = Value::from(min_length);
        }
        if let Some(default_type) = http.get("defaultType").and_then(Value::as_str) {
            if default_type.trim().is_empty() {
                return Err(materialize_error(
                    "server.toml.http.defaultType",
                    "`defaultType` must not be empty",
                ));
            }
            default_type_json = Some(Value::String(default_type.to_owned()));
        }
        if let Some(rows) = http.get("mimeTypes").and_then(Value::as_array) {
            let mut entries = Vec::with_capacity(rows.len());
            for (index, row) in rows.iter().enumerate() {
                let path = format!("server.toml.http.mimeTypes[{index}]");
                let row = row
                    .as_object()
                    .ok_or_else(|| materialize_error(&path, "expected a TOML table"))?;
                for key in row.keys() {
                    if !matches!(key.as_str(), "mime" | "extensions") {
                        return Err(materialize_error(
                            &path,
                            format!("unknown key `{key}`; expected `mime` and `extensions`"),
                        ));
                    }
                }
                let mime = as_str(row, &path, "mime")?;
                let extensions = row
                    .get("extensions")
                    .and_then(Value::as_array)
                    .ok_or_else(|| materialize_error(&path, "`extensions` must be an array"))?;
                let extensions = extensions
                    .iter()
                    .map(|extension| {
                        extension.as_str().map(str::to_owned).ok_or_else(|| {
                            materialize_error(&path, "`extensions` entries must be strings")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                entries.push(json!({ "mime": mime, "extensions": extensions }));
            }
            mime_types_json = Some(Value::Array(entries));
        }
    }

    let mut proxy_cache = json!({
        "enabled": false,
        "maxEntries": 4096,
        "maxObjectBytes": 1048576,
        "defaultTtlSeconds": 60,
        "staleTtlSeconds": 60,
    });
    if let Some(cache) = root.get("proxyCache").and_then(Value::as_object) {
        if let Some(enabled) = cache.get("enabled").and_then(Value::as_bool) {
            proxy_cache["enabled"] = Value::Bool(enabled);
        }
        if let Some(max_entries) = cache.get("maxEntries").and_then(Value::as_u64) {
            proxy_cache["maxEntries"] = Value::from(max_entries);
        }
        if let Some(max_object_bytes) = cache.get("maxObjectBytes").and_then(Value::as_u64) {
            proxy_cache["maxObjectBytes"] = Value::from(max_object_bytes);
        }
        if let Some(ttl) = cache.get("defaultTtlSeconds").and_then(Value::as_u64) {
            proxy_cache["defaultTtlSeconds"] = Value::from(ttl);
        }
        if let Some(stale) = cache.get("staleTtlSeconds").and_then(Value::as_u64) {
            proxy_cache["staleTtlSeconds"] = Value::from(stale);
        }
        if let Some(disk_path) = cache.get("diskPath").and_then(Value::as_str) {
            proxy_cache["diskPath"] = Value::String(disk_path.to_owned());
        }
    }
    if let Some(main) = root.get("main").and_then(Value::as_object) {
        check_supported_keys(main, "server.toml.main", &["events"])?;
        if let Some(events) = main.get("events").and_then(Value::as_object) {
            check_supported_keys(events, "server.toml.main.events", &["workerConnections"])?;
        }
    }
    if let Some(stream) = root.get("stream").and_then(Value::as_object) {
        check_supported_keys(stream, "server.toml.stream", &["server"])?;
    }
    if let Some(cache) = root.get("proxyCache").and_then(Value::as_object) {
        check_supported_keys(
            cache,
            "server.toml.proxyCache",
            &[
                "enabled",
                "maxEntries",
                "maxObjectBytes",
                "defaultTtlSeconds",
                "staleTtlSeconds",
                "diskPath",
            ],
        )?;
    }
    if let Some(mail) = root.get("mail").and_then(Value::as_object) {
        check_supported_keys(mail, "server.toml.mail", &["server"])?;
    }
    let mut materializer = Materializer::new(app_key);
    // A mail-only application (Wave 9A mail listener plane) has no HTTP
    // surface at all, so `[http]` is required only when `[mail]` is absent.
    let empty_http = Map::new();
    let http = match root.get("http").and_then(Value::as_object) {
        Some(http) => http,
        None if root.get("mail").and_then(Value::as_object).is_some() => &empty_http,
        None => {
            return Err(materialize_error(
                "server.toml",
                "http table is required for an enabled configuration",
            ))
        }
    };
    check_supported_keys(
        http,
        "server.toml.http",
        &[
            "certificates",
            "clientMaxBodySize",
            "defaults",
            "defaultType",
            "mimeTypes",
            "gzip",
            "gzipTypes",
            "gzipMinLength",
            "keepaliveTimeout",
            "limitConnZone",
            "limitReqZone",
            "maps",
            "server",
            "unixSocketRoot",
            "upstream",
        ],
    )?;
    if let Some(unix_socket_root) = http.get("unixSocketRoot").and_then(Value::as_str) {
        nginx["unixSocketRoot"] = Value::String(unix_socket_root.to_owned());
    }
    let defaults_tls = materialize_http_defaults(http)?;
    materializer.materialize_limit_req_zones(http)?;
    materializer.materialize_limit_conn_zones(http)?;
    materializer.materialize_maps(http)?;
    materializer.materialize_certificates(http)?;
    materializer.materialize_upstreams(http)?;
    if let Some(servers) = http.get("server").and_then(Value::as_array) {
        for (index, server) in servers.iter().enumerate() {
            let server = server
                .as_object()
                .ok_or_else(|| materialize_error("http.server", "server entries must be tables"))?;
            let effective_server = apply_server_tls_defaults(server, defaults_tls.as_ref())?;
            materializer.materialize_server(index, &effective_server)?;
        }
    }
    // Stream servers resolve upstream references against the materialized
    // upstream set, so they are materialized after http.upstream.
    materializer.materialize_streams(root)?;
    // Mail listeners are independent of the HTTP surface.
    materializer.materialize_mail(root)?;
    if materializer.virtual_hosts.is_empty() && materializer.mail_servers.is_empty() {
        return Err(WebServerConfigError::Materialize(
            "no [[http.server]] virtual hosts and no [[mail.server]] listeners are declared in the effective configuration"
                .to_owned(),
        ));
    }
    let tunnel = Materializer::materialize_tunnel(effective)?;
    materializer.finish(
        limits,
        nginx,
        gzip,
        proxy_cache,
        tunnel,
        mime_types_json,
        default_type_json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExpiresMode, IfModifiedSinceMode};
    use std::path::PathBuf;

    fn examples_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .expect("crates")
            .parent()
            .expect("sdkwork-webserver")
            .join("..")
            .join("sdkwork-specs")
            .join("examples")
            .join("webserver")
            .join("modules")
            .join("sdkwork-im")
            .join("deployments")
            .join("webserver")
    }

    fn write_layout_v3_dir(dir: &Path, common: &str, production: &str, standalone: &str) {
        std::fs::write(dir.join("server.common.toml"), common).expect("write common");
        std::fs::write(
            dir.join("server.production.toml"),
            format!("environment = \"production\"\n{production}"),
        )
        .expect("write production");
        for env in ["development", "test", "staging"] {
            std::fs::write(
                dir.join(format!("server.{env}.toml")),
                format!("environment = \"{env}\"\nenabled = false\n"),
            )
            .expect("write env stub");
        }
        std::fs::write(
            dir.join("server.standalone.toml"),
            if standalone.trim_start().starts_with("profile =") {
                standalone.to_owned()
            } else {
                format!("profile = \"standalone\"\n{standalone}")
            },
        )
        .expect("write standalone");
        std::fs::write(dir.join("server.cloud.toml"), "profile = \"cloud\"\n")
            .expect("write cloud");
    }

    const MINIMAL_PRODUCTION: &str = r#"
[http]
[http.certificates."sdkwork.com"]
certFile = "/etc/sdkwork/certs/letsencrypt/sdkwork.com/fullchain.pem"
certKeyFile = "/etc/sdkwork/certs/letsencrypt/sdkwork.com/privkey.pem"

[[http.server]]
listen = ["443 ssl", "80"]
serverName = ["im.sdkwork.com"]
http2 = true
[http.server.tls]
cert = "sdkwork.com"
protocols = ["TLSv1.2", "TLSv1.3"]

[[http.server.location]]
match = "/api/"
proxyPass = "http://gateway"

[[http.server.location]]
match = "/"
proxyPass = "http://gateway"
"#;

    /// Builds a layout v3 example directory for one test.
    ///
    /// The path is keyed by `label`, not only by process id: Rust runs a test
    /// binary's cases on multiple threads, and a single shared path let two
    /// tests `remove_dir_all` each other's tree mid-load, which surfaced as an
    /// intermittent "standalone example must load" failure with no source
    /// change behind it.
    fn minimal_examples_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-example-layout-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let common =
            std::fs::read_to_string(examples_dir().join("server.common.toml")).expect("common");
        let standalone = std::fs::read_to_string(examples_dir().join("server.standalone.toml"))
            .expect("standalone");
        let cloud =
            std::fs::read_to_string(examples_dir().join("server.cloud.toml")).expect("cloud");
        write_layout_v3_dir(&dir, &common, MINIMAL_PRODUCTION, &standalone);
        std::fs::write(dir.join("server.cloud.toml"), cloud).expect("cloud");
        dir
    }

    #[test]
    fn loads_cloud_profile_from_example_layout() {
        let dir = minimal_examples_dir("cloud-profile");
        let config = load_server_toml_app(&dir, "cloud", "production", "sdkwork-example")
            .expect("cloud example must load");
        assert_eq!(config.app_key, "sdkwork-example");
        let upstream = config
            .upstreams
            .iter()
            .find(|u| u.id == "gateway")
            .expect("upstream");
        assert_eq!(
            upstream.targets[0].url,
            "http://sdkwork-api-cloud-gateway:8080"
        );
        assert_eq!(config.virtual_hosts.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn materializes_tunnel_section_from_server_toml() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "127.0.0.1:8443"
domain_suffixes = ["sdkwork.link"]
[tunnel.network]
heartbeat_interval = 5
[[tunnel.routes]]
name = "web"
protocol = "http"
domain = "demo.sdkwork.link"
target = "127.0.0.1:3000"
[tunnel.routes.policy]
allow_public = true
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert!(config.enabled);
        assert_eq!(config.gateway_or_default().listen, "127.0.0.1:8443");
        assert_eq!(
            config.gateway_or_default().domain_suffixes,
            vec!["sdkwork.link".to_owned()]
        );
        assert_eq!(config.network_or_default().heartbeat_interval_secs, 5);
        let routes = config.routes_or_empty();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].domain.as_deref(), Some("demo.sdkwork.link"));
        assert!(routes[0].policy_or_default().allow_public);
    }

    #[test]
    fn materializes_tunnel_gateway_route_port_range_and_connection_ceiling() {
        // The route-port range and the gateway connection ceiling are
        // registration policies an operator must be able to set where the
        // tunnel is configured; both were rejected by the TOML surface while
        // `docs/tunnel.md` documented them (and the CLI/JSON model accepted
        // them), so the default silently applied instead.
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "127.0.0.1:8443"
route_port_min = 2048
routePortMax = 4096
[tunnel.limits]
maxRoutes = 12
max_connections = 64
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let gateway = config.gateway_or_default();
        assert_eq!(gateway.route_port_min, 2048);
        assert_eq!(gateway.route_port_max, 4096);
        let limits = config.limits_or_default();
        assert_eq!(limits.max_routes, 12);
        assert_eq!(limits.max_connections, 64);
        // Untouched keys keep their documented defaults.
        assert_eq!(limits.max_devices, 10_000);
        assert_eq!(limits.max_streams_per_session, 256);
    }

    /// The FRP-shaped header rewriting surface: `hostHeaderRewrite` plus
    /// `[tunnel.routes.requestHeaders]` / `[tunnel.routes.responseHeaders]`
    /// with a `set` map.
    #[test]
    fn materializes_tunnel_route_header_rewriting() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "web"
protocol = "http"
domain = "demo.sdkwork.link"
target = "127.0.0.1:3000"
hostHeaderRewrite = "backend.internal:8080"
[tunnel.routes.requestHeaders]
set = { "X-From" = "edge", "X-Tenant" = "acme" }
[tunnel.routes.responseHeaders]
set = { "X-Served-By" = "sdkwork" }
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let template = &config.routes.as_ref().expect("routes")[0];
        let rewrite = template.header_rewrite().expect("rewrite validates");
        assert_eq!(rewrite.host(), Some("backend.internal:8080"));
        assert_eq!(rewrite.request_set().len(), 2);
        assert_eq!(rewrite.response_set().len(), 1);
        // Names are canonicalized to lower case; values are preserved.
        assert!(rewrite
            .request_set()
            .iter()
            .any(|pair| pair.name == "x-from" && pair.value == "edge"));
        // The route the agent would register carries the same policy.
        let route = template
            .to_route(sdkwork_webserver_tunnel_core::RouteId::parse("route_web").expect("id"))
            .expect("route builds");
        assert_eq!(route.headers, rewrite);
    }

    /// A framing header inside a `set` map is a configuration error, not
    /// something the gateway silently refuses to apply at request time.
    #[test]
    fn a_framing_header_in_a_set_map_is_a_configuration_error() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "web"
protocol = "http"
domain = "demo.sdkwork.link"
target = "127.0.0.1:3000"
[tunnel.routes.requestHeaders]
set = { "Content-Length" = "0" }
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("a framing header cannot be rewritten");
        assert!(
            format!("{error}").contains("framing or hop-by-hop"),
            "unexpected error: {error}"
        );
    }

    /// Header rewriting is an HTTP-plane feature: declaring it on a TCP route
    /// would advertise something that can never take effect.
    #[test]
    fn header_rewriting_on_a_tcp_route_is_a_configuration_error() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "db"
protocol = "tcp"
port = 20001
target = "127.0.0.1:5432"
hostHeaderRewrite = "backend.internal"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let template = &config.routes.as_ref().expect("routes")[0];
        let error = template
            .header_rewrite()
            .expect_err("a tcp route cannot rewrite headers");
        assert!(error.to_string().contains("HTTP-only"), "{error}");
    }

    /// The ordered gateway address list (FRP `serverAddr` list parity).
    #[test]
    fn materializes_the_tunnel_agent_gateway_address_list() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"
endpoints = ["gw2.sdkwork.link:8443", "gw3.sdkwork.link:8443"]
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let agent = config.agent_or_default();
        assert_eq!(
            agent.gateway_endpoints(),
            vec![
                "gw1.sdkwork.link:8443".to_owned(),
                "gw2.sdkwork.link:8443".to_owned(),
                "gw3.sdkwork.link:8443".to_owned(),
            ]
        );
        config.validate().expect("the list validates");
    }

    /// A fallback that cannot be dialled is worse than none: a malformed
    /// address is a configuration error naming the key.
    #[test]
    fn a_malformed_gateway_address_in_the_list_is_a_configuration_error() {
        for address in ["gw2.sdkwork.link", "gw2.sdkwork.link:0", "https://gw2:8443"] {
            let toml_text = format!(
                r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"
endpoints = ["{address}"]
"#
            );
            let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
            let effective: Value = serde_json::to_value(&value).expect("converts");
            let error = Materializer::materialize_tunnel(&effective)
                .expect_err("a malformed address cannot be materialized");
            assert!(
                format!("{error}").contains("server.toml.tunnel.agent.endpoints"),
                "unexpected error: {error}"
            );
        }
    }

    /// Transport tunables on the `[tunnel.network]` surface.
    #[test]
    fn materializes_tunnel_transport_tunables() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.network]
heartbeat_interval = 10
keepAliveIntervalSecs = 2
heartbeatTimeoutSecs = 45
udp_packet_size = 1200
maxConcurrentStreams = 64
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let network = config.network_or_default();
        assert_eq!(network.keep_alive_interval_secs, Some(2));
        assert_eq!(network.heartbeat_timeout_secs, Some(45));
        assert_eq!(network.udp_packet_size, Some(1_200));
        assert_eq!(network.max_concurrent_streams, Some(64));
        config.validate().expect("the tunables validate");

        // Absent keys stay absent: the derived defaults are what applies, and
        // the model must not bake the derivation into the file.
        let defaults = sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default();
        assert_eq!(defaults.keep_alive_interval_secs, None);
        assert_eq!(defaults.heartbeat_timeout_secs, None);
        assert_eq!(defaults.udp_packet_size, None);
        assert_eq!(defaults.max_concurrent_streams, None);
    }

    /// An unusable tunable is a configuration error, not a silent clamp.
    #[test]
    fn an_unusable_transport_tunable_is_a_configuration_error() {
        for (key, value_text) in [
            ("udpPacketSize", "128"),
            ("maxConcurrentStreams", "4"),
            ("heartbeatTimeoutSecs", "3"),
        ] {
            let toml_text = format!(
                r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.network]
heartbeat_interval = 10
{key} = {value_text}
"#
            );
            let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
            let effective: Value = serde_json::to_value(&value).expect("converts");
            let tunnel = Materializer::materialize_tunnel(&effective)
                .expect("materializes")
                .expect("tunnel section present");
            let config: sdkwork_webserver_tunnel_core::TunnelConfig =
                serde_json::from_value(tunnel).expect("tunnel model parses");
            assert!(
                config.validate().is_err(),
                "{key} = {value_text} must be refused"
            );
        }
    }

    /// FRP `auth.additionalScopes` on the `[tunnel.gateway]` surface.
    #[test]
    fn materializes_gateway_auth_scopes() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "127.0.0.1:8443"
authScopes = ["HeartBeats"]
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert_eq!(
            config.gateway_or_default().auth_scopes,
            vec![sdkwork_webserver_tunnel_core::AuthScope::Heartbeats]
        );
        // Absent stays absent: the hardening is opt-in.
        assert!(
            sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default()
                .auth_scopes
                .is_empty()
        );
    }

    /// `newWorkConns` has no equivalent in this design (streams are opened
    /// inside the authenticated connection), so configuring it is refused with
    /// the explanation instead of being accepted as a no-op.
    #[test]
    fn new_work_conns_is_a_configuration_error_not_a_silent_no_op() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "127.0.0.1:8443"
authScopes = ["newWorkConns"]
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("a scope without an equivalent must be refused");
        assert!(
            format!("{error}").contains("no equivalent"),
            "unexpected error: {error}"
        );
    }

    /// Per-user credentials and quotas (FRP `user`, made enforceable).
    #[test]
    fn materializes_gateway_users_with_their_quotas() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "127.0.0.1:8443"
[[tunnel.gateway.users]]
name = "team-a"
tokenEnv = "TEAM_A_TOKEN"
maxRoutes = 20
max_ports = 5
maxDevices = 3
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let users = &config.gateway_or_default().users;
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "team-a");
        assert_eq!(users[0].token_env, "TEAM_A_TOKEN");
        assert_eq!(users[0].max_routes, Some(20));
        assert_eq!(users[0].max_ports, Some(5));
        assert_eq!(users[0].max_devices, Some(3));
        config.validate().expect("the user table validates");

        // Absent stays absent: no users means no per-user quota, which is the
        // pre-user behaviour.
        assert!(
            sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default()
                .users
                .is_empty()
        );
    }

    /// A user entry that cannot be enforced is a configuration error.
    #[test]
    fn an_unenforceable_user_entry_is_a_configuration_error() {
        for (entry, expected) in [
            (
                r#"[[tunnel.gateway.users]]
name = "team a"
tokenEnv = "TEAM_A_TOKEN""#,
                "may only contain",
            ),
            (
                r#"[[tunnel.gateway.users]]
name = "team-a"
tokenEnv = """#,
                "environment variable",
            ),
            (
                r#"[[tunnel.gateway.users]]
name = "team-a"
tokenEnv = "TEAM_A_TOKEN"
maxRoutes = 0"#,
                "at least 1",
            ),
        ] {
            let toml_text = format!(
                r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "127.0.0.1:8443"
{entry}
"#
            );
            let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
            let effective: Value = serde_json::to_value(&value).expect("converts");
            let error = Materializer::materialize_tunnel(&effective)
                .expect_err("an unenforceable user must be refused");
            assert!(
                format!("{error}").contains(expected),
                "expected `{expected}` in: {error}"
            );
        }
    }

    /// The machine's user claim is validated where it is written.
    #[test]
    fn materializes_the_agent_user_claim() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"
user = "team-a"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert_eq!(config.agent_or_default().user, "team-a");
    }

    /// FRP `loginFailExit` on the `[tunnel.agent]` surface, refused when it is
    /// not a boolean: a quoted "true" would silently switch nothing off.
    #[test]
    fn materializes_login_fail_exit_and_refuses_a_non_boolean() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"
loginFailExit = true
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert!(config.agent_or_default().login_fail_exit);
        // The default stays FRP's opposite, on purpose (see the field docs).
        assert!(!sdkwork_webserver_tunnel_core::TunnelAgentConfig::default().login_fail_exit);

        let bad = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"
loginFailExit = "true"
"#;
        let value: toml::Value = toml::from_str(bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("a quoted boolean must be refused");
        assert!(
            format!("{error}").contains("must be true or false"),
            "unexpected error: {error}"
        );
    }

    /// Operator metadata (FRP `annotations`) on a route, bounded at load time.
    #[test]
    fn materializes_route_annotations_and_refuses_unusable_ones() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "web"
protocol = "http"
domain = "docs.sdkwork.link"
target = "127.0.0.1:8080"
[tunnel.routes.annotations]
owner = "team-a"
ticket = "OPS-1"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let route = &config.routes_or_empty()[0];
        assert_eq!(
            route.annotations.get("owner").map(String::as_str),
            Some("team-a")
        );
        assert_eq!(
            route.annotations.get("ticket").map(String::as_str),
            Some("OPS-1")
        );
        // The template validates into a route that carries them.
        route
            .to_route(sdkwork_webserver_tunnel_core::RouteId::parse("route_web").expect("id"))
            .expect("route materializes")
            .annotations
            .get("owner")
            .expect("owner survives materialization");

        // An unusable key is a configuration error with the config path, not a
        // runtime surprise at the gateway.
        let bad = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "web"
protocol = "http"
domain = "docs.sdkwork.link"
target = "127.0.0.1:8080"
[tunnel.routes.annotations]
"has space" = "value"
"#;
        let value: toml::Value = toml::from_str(bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("an unusable annotation key must be refused");
        assert!(
            format!("{error}").contains("may only contain"),
            "unexpected error: {error}"
        );
    }

    /// FRP `proxyBindAddr`: a literal IP, and nothing else.
    #[test]
    fn materializes_proxy_bind_addr_and_refuses_anything_but_an_ip() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "0.0.0.0:8443"
proxyBindAddr = "127.0.0.1"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let gateway = config.gateway_or_default();
        assert_eq!(gateway.proxy_bind_addr, "127.0.0.1");
        assert_eq!(
            gateway.proxy_bind_addr().expect("parses"),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );
        config.validate().expect("the address validates");
        // The default is every interface, i.e. the previous behaviour.
        assert_eq!(
            sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default().proxy_bind_addr,
            "0.0.0.0"
        );

        // A hostname or a `host:port` form would bind somewhere the operator
        // did not ask for (a port here is overwritten by each route's own), so
        // both are configuration errors.
        for bad in ["gateway.sdkwork.link", "127.0.0.1:8080", ""] {
            let toml_text = format!(
                r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "0.0.0.0:8443"
proxyBindAddr = "{bad}"
"#
            );
            let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
            let effective: Value = serde_json::to_value(&value).expect("converts");
            let error = Materializer::materialize_tunnel(&effective)
                .expect_err("only a literal IP is accepted");
            assert!(
                format!("{error}").contains("proxyBindAddr"),
                "the error names the field for `{bad}`: {error}"
            );
        }
    }

    /// FRP `transport.dialServerLocalIP`: a literal IP, and nothing else.
    #[test]
    fn materializes_dial_server_local_ip_and_refuses_a_hostname() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.network]
dialServerLocalIP = "10.0.0.5"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert_eq!(
            config.network_or_default().dial_local_addr,
            Some("10.0.0.5".parse().expect("ip"))
        );
        // Absent means "let the kernel choose", which is the previous
        // behaviour and the right default for most machines.
        assert_eq!(
            sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default().dial_local_addr,
            None
        );

        let bad = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.network]
dialServerLocalIP = "eth0"
"#;
        let value: toml::Value = toml::from_str(bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("a hostname is not a source address");
        assert!(
            format!("{error}").contains("not a literal IP address"),
            "unexpected error: {error}"
        );
    }

    /// FRP `dnsServer`: an IP with an optional port, never a name.
    #[test]
    fn materializes_dns_server_and_refuses_a_name() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.network]
dnsServer = "10.0.0.53:5353"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert_eq!(
            config.network_or_default().dns_server.as_deref(),
            Some("10.0.0.53:5353")
        );
        config.validate().expect("the DNS server validates");
        // Absent means the system resolver, i.e. the previous behaviour.
        assert_eq!(
            sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default().dns_server,
            None
        );

        let bad = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.network]
dnsServer = "resolver.internal"
"#;
        let value: toml::Value = toml::from_str(bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("resolving the resolver is a dependency loop");
        assert!(
            format!("{error}").contains("dnsServer"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn an_out_of_range_tunnel_route_port_is_a_configuration_error() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "127.0.0.1:8443"
routePortMin = 70000
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("a port above 65535 cannot be materialized");
        assert!(
            format!("{error}").contains("65535"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_partial_tunnel_limits_table_keeps_the_other_defaults() {
        // Every `[tunnel.limits]` key is optional: a table that sets one
        // ceiling must not null out the rest and fail deserialization.
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.limits]
max_sessions = 7
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let encoded = tunnel.to_string();
        assert!(
            !encoded.contains("null"),
            "absent ceilings must be omitted, not nulled: {encoded}"
        );
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert_eq!(config.limits_or_default().max_sessions, 7);
        assert_eq!(config.limits_or_default().max_connections, 8_192);
    }

    #[test]
    fn materializes_a_proxy_protocol_route_and_refuses_an_unknown_version() {
        // FRP parity: `transport.proxyProtocolVersion` is a per-route choice,
        // so it belongs on the route template where the target lives.
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "audited"
protocol = "tcp"
port = 7022
target = "127.0.0.1:22"
proxy_protocol = "v1"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let routes = config.routes_or_empty();
        assert_eq!(routes.len(), 1);
        assert_eq!(
            routes[0].proxy_protocol,
            Some(sdkwork_webserver_tunnel_core::ProxyProtocolVersion::V1)
        );
        // The template must still validate with the new field set.
        routes[0].validate().expect("template validates");

        let bad = toml_text.replace("proxy_protocol = \"v1\"", "proxyProtocol = \"v3\"");
        let value: toml::Value = toml::from_str(&bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("an unknown PROXY protocol version must be refused");
        assert!(format!("{error}").contains("v1 or v2"), "got: {error}");
    }

    #[test]
    fn materializes_location_path_claims_and_refuses_an_unusable_prefix() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "api"
protocol = "http"
domain = "split.sdkwork.link"
target = "127.0.0.1:3000"
locations = ["/api/", "/api/v2"]
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let routes = config.routes_or_empty();
        // `/api/` and `/api/v2` normalize and stay a two-entry claim.
        assert_eq!(
            routes[0].locations,
            vec!["/api".to_owned(), "/api/v2".to_owned()]
        );
        let route = routes[0]
            .to_route(sdkwork_webserver_tunnel_core::RouteId::parse("route_api").expect("id"))
            .expect("materializes");
        assert_eq!(route.matches_path("/api/users"), Some(4));
        assert_eq!(route.matches_path("/api/v2/users"), Some(7));
        assert_eq!(route.matches_path("/other"), None);

        let bad = toml_text.replace(
            r#"locations = ["/api/", "/api/v2"]"#,
            r#"locations = ["api"]"#,
        );
        let value: toml::Value = toml::from_str(&bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("a location without a leading slash must be refused");
        assert!(
            format!("{error}").contains("must start with `/`"),
            "got: {error}"
        );
    }

    #[test]
    fn materializes_a_bandwidth_ceiling_from_a_size_string_or_a_number() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "slow"
protocol = "http"
domain = "slow.sdkwork.link"
target = "127.0.0.1:3000"
bandwidthLimit = "2MB"
[[tunnel.routes]]
name = "slower"
protocol = "tcp"
port = 7022
target = "127.0.0.1:22"
bandwidth_limit = 8192
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let routes = config.routes_or_empty();
        assert_eq!(
            routes[0].bandwidth_limit_bytes_per_second,
            Some(2 * 1024 * 1024)
        );
        assert_eq!(routes[1].bandwidth_limit_bytes_per_second, Some(8_192));
        for route in routes {
            route.validate().expect("templates with a ceiling validate");
        }

        // A zero ceiling is refused: "unlimited" and "no traffic" are opposite
        // readings of the same number.
        let bad = toml_text.replace("bandwidthLimit = \"2MB\"", "bandwidthLimit = \"0\"");
        let value: toml::Value = toml::from_str(&bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("a zero ceiling must be refused");
        assert!(
            format!("{error}").contains("greater than zero"),
            "got: {error}"
        );
    }

    #[test]
    fn materializes_a_load_balancer_group_with_a_health_probe() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[[tunnel.routes]]
name = "web"
protocol = "http"
domain = "lb.sdkwork.link"
target = "127.0.0.1:3000"
load_balancer_group = "web"
[tunnel.routes.health_check]
kind = "http"
path = "/health"
interval_seconds = 5
timeout = 2
max_failed = 3
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        let routes = config.routes_or_empty();
        assert_eq!(routes[0].load_balancer_group.as_deref(), Some("web"));
        let probe = routes[0].health_check.clone().expect("probe materialized");
        assert_eq!(
            probe.kind,
            sdkwork_webserver_tunnel_core::HealthCheckKind::Http
        );
        assert_eq!(probe.path.as_deref(), Some("/health"));
        assert_eq!(probe.interval_seconds, 5);
        assert_eq!(probe.timeout_seconds, 2);
        assert_eq!(probe.max_failed, 3);
        routes[0]
            .validate()
            .expect("the grouped template validates");

        // The probe itself is validated at load time: a path on a TCP probe
        // would silently check the socket while the operator believes an HTTP
        // probe is running.
        let bad = toml_text.replace("kind = \"http\"", "kind = \"tcp\"");
        let value: toml::Value = toml::from_str(&bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert!(
            config.routes_or_empty()[0].validate().is_err(),
            "a tcp probe with a path must not validate"
        );

        // An unknown kind is refused where it is written.
        let bad_kind = toml_text.replace("kind = \"http\"", "kind = \"icmp\"");
        let value: toml::Value = toml::from_str(&bad_kind).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("an unknown probe kind must be refused");
        assert!(format!("{error}").contains("tcp or http"), "got: {error}");
    }

    #[test]
    fn materializes_the_multiplexed_ingress_port() {
        let toml_text = r#"
specVersion = 1
[tunnel]
enabled = true
[tunnel.gateway]
listen = "0.0.0.0:8443"
tcpmux_port = 8444
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert_eq!(
            config.gateway_or_default().tcpmux_port,
            Some(8444),
            "the multiplexed ingress port reaches the gateway model"
        );

        // An out-of-range port is a configuration error, not a truncation.
        let bad = toml_text.replace("tcpmux_port = 8444", "tcpmux_port = 70000");
        let value: toml::Value = toml::from_str(&bad).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = Materializer::materialize_tunnel(&effective)
            .expect_err("an out-of-range multiplexer port must be refused");
        assert!(format!("{error}").contains("65535"), "got: {error}");

        // The camelCase spelling works too, and the default is "no ingress".
        let camel = toml_text.replace("tcpmux_port", "tcpmuxPort");
        let value: toml::Value = toml::from_str(&camel).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective)
            .expect("materializes")
            .expect("tunnel section present");
        let config: sdkwork_webserver_tunnel_core::TunnelConfig =
            serde_json::from_value(tunnel).expect("tunnel model parses");
        assert_eq!(config.gateway_or_default().tcpmux_port, Some(8444));
        assert_eq!(
            sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default().tcpmux_port,
            None
        );
    }

    #[test]
    fn typed_http_unix_socket_root_materializes_and_validates() {
        let toml_text = r#"
[http]
unixSocketRoot = "/var/run/sdkwork"

[[http.server]]
listen = ["8080"]
serverName = ["unix.example.test"]

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective = serde_json::to_value(&value).expect("converts");
        let config = materialize_app(&effective, "test").expect("materialize");
        assert_eq!(
            config.nginx.unix_socket_root.as_deref(),
            Some("/var/run/sdkwork"),
            "`[http] unixSocketRoot` must reach the runtime model"
        );

        // A relative root is not a confinement root: the shared semantic
        // validation rejects it with a located diagnostic.
        let invalid = r#"
[http]
unixSocketRoot = "relative/sockets"

[[http.server]]
listen = ["8080"]
serverName = ["unix.example.test"]

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#;
        let value: toml::Value = toml::from_str(invalid).expect("toml parses");
        let effective = serde_json::to_value(&value).expect("converts");
        let error = materialize_app(&effective, "test").expect_err("relative root fails");
        let diagnostics: Vec<String> = error
            .diagnostics()
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.path, diagnostic.message))
            .collect();
        assert!(
            diagnostics
                .iter()
                .any(|message| message.contains("unixSocketRoot")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn materializes_typed_maps_into_the_executable_model() {
        // Item 12: `[[http.maps]]` is the typed plane's executable map form;
        // the migration importer can render it without creating a silent
        // no-op. The same bounded validation as the nginx-conf plane applies.
        let toml_text = r#"
[http]

[[http.maps]]
source = "$host"
result = "$tenant_zone"
default = "anonymous"

[[http.maps.entries]]
key = "a.example.test"
value = "zone-a"

[[http.maps.entries]]
key = "b.example.test"
value = "zone-b"

[[http.maps]]
source = "$http_x_role"
result = "$role_tag"
hostnames = false
default = "none"

[[http.server]]
listen = ["8080"]
serverName = ["maps.example.test"]

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective = serde_json::to_value(&value).expect("converts");
        let config = materialize_app(&effective, "test").expect("materialize");
        assert_eq!(config.maps.len(), 2);
        let zones = &config.maps[0];
        assert_eq!(zones.source, "$host");
        assert_eq!(zones.result, "$tenant_zone");
        assert_eq!(zones.default.as_deref(), Some("anonymous"));
        assert_eq!(zones.entries.len(), 2);
        assert_eq!(zones.entries[0].key, "a.example.test");
        assert_eq!(zones.entries[0].value, "zone-a");
        // The declared result variable evaluates through the shared map
        // evaluator, exactly like the nginx-conf plane.
        struct Facts(&'static str);
        impl crate::config::map_variables::MapVariables for Facts {
            fn map_variable(&self, name: &str) -> Option<String> {
                (name == "host").then(|| self.0.to_owned())
            }
        }
        let resolved = crate::config::evaluate_map(zones, &Facts("a.example.test"));
        assert_eq!(resolved, "zone-a");
        let fallback = crate::config::evaluate_map(zones, &Facts("c.example.test"));
        assert_eq!(fallback, "anonymous");
    }

    #[test]
    fn typed_maps_invalid_forms_fail_closed_with_a_located_diagnostic() {
        let build = |maps: &str| {
            let toml_text = format!(
                r#"
[http]
{maps}
[[http.server]]
listen = ["8080"]
serverName = ["maps.example.test"]

[[http.server.location]]
match = "/"
proxyPass = "http://127.0.0.1:9001"
"#
            );
            let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
            serde_json::to_value(&value).expect("converts")
        };
        let unsupported_source = build(
            r#"
[[http.maps]]
source = "$server_protocol"
result = "$tag"
"#,
        );
        let error = materialize_app(&unsupported_source, "test").expect_err("unsupported source");
        assert!(
            error.to_string().contains("server.toml.http.maps"),
            "diagnostic must be located: {error}"
        );
        let unknown_key = build(
            r#"
[[http.maps]]
source = "$host"
result = "$tag"
unknown = "x"
"#,
        );
        let error = materialize_app(&unknown_key, "test").expect_err("unknown key");
        assert!(
            error.to_string().contains("server.toml.http.maps[0]"),
            "diagnostic must be located: {error}"
        );
    }

    #[test]
    fn materializes_chain_file_into_the_certificate_source() {
        // Item 10: `chainFile` is in SERVER_TLS_KEYS, so it must reach the
        // runtime material instead of being silently dropped. It is carried on
        // the certificate source and consumed by the TLS materializer
        // (downstream client-auth trust chain / served chain completion).
        let toml_text = r#"
[http]
[http.certificates."tls-site"]
certFile = "/etc/sdkwork/certs/tls.example.com/fullchain.pem"
certKeyFile = "/etc/sdkwork/certs/tls.example.com/privkey.pem"
chainFile = "/etc/sdkwork/certs/tls.example.com/chain.pem"

[[http.server]]
listen = ["443 ssl"]
serverName = ["tls.example.com"]

[http.server.tls]
cert = "tls-site"

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective = serde_json::to_value(&value).expect("converts");
        let config = materialize_app(&effective, "test").expect("materialize");
        let certificate = config
            .certificates
            .iter()
            .find(|certificate| certificate.id == "tls-site")
            .expect("certificate");
        match &certificate.source {
            crate::config::CertificateSource::ProtectedFile { chain_file, .. } => assert_eq!(
                chain_file.as_deref(),
                Some("/etc/sdkwork/certs/tls.example.com/chain.pem")
            ),
        }
    }

    /// Minimal single-server typed-TOML document with an inline certificate,
    /// used by the `protocols` version-window tests.
    fn tls_protocols_document(tls_block: &str) -> Value {
        let toml_text = format!(
            r#"
[http]

[[http.server]]
listen = ["443 ssl"]
serverName = ["tls.example.com"]

[http.server.tls]
certFile = "/etc/sdkwork/certs/tls.example.com/fullchain.pem"
certKeyFile = "/etc/sdkwork/certs/tls.example.com/privkey.pem"
{tls_block}

[[http.server.location]]
match = "/"
proxyPass = "http://127.0.0.1:9001"
"#
        );
        let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
        serde_json::to_value(&value).expect("converts")
    }

    #[test]
    fn toml_protocols_select_the_tls_window() {
        // `PRD-https-and-certificates.md` §2 grants "a stricter compatible
        // policy", so a singleton window must reach the policy instead of being
        // accepted and dropped.
        let tls13 = materialize_app(
            &tls_protocols_document(r#"protocols = ["TLSv1.3"]"#),
            "test",
        )
        .expect("TLS 1.3 only must materialize");
        assert_eq!(tls13.tls_policies.len(), 1);
        assert_eq!(tls13.tls_policies[0].minimum_version, TlsVersion::Tls13);
        assert_eq!(tls13.tls_policies[0].maximum_version, TlsVersion::Tls13);

        // An absent `protocols` keeps the documented platform default.
        let default = materialize_app(&tls_protocols_document(""), "test")
            .expect("absent protocols keeps the platform default");
        assert_eq!(default.tls_policies[0].minimum_version, TlsVersion::Tls12);
        assert_eq!(default.tls_policies[0].maximum_version, TlsVersion::Tls13);
    }

    #[test]
    fn toml_defaults_tls_protocols_apply_to_every_server() {
        let toml_text = r#"
[http]

[http.defaults.tls]
protocols = ["TLSv1.3"]

[[http.server]]
listen = ["443 ssl"]
serverName = ["defaults.example.com"]

[http.server.tls]
certFile = "/etc/sdkwork/certs/a/fullchain.pem"
certKeyFile = "/etc/sdkwork/certs/a/privkey.pem"

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let config = materialize_app(&effective, "test").expect("materialize");
        assert_eq!(config.tls_policies[0].minimum_version, TlsVersion::Tls13);
        assert_eq!(config.tls_policies[0].maximum_version, TlsVersion::Tls13);
    }

    #[test]
    fn toml_protocols_reject_unsupported_tokens() {
        let error = materialize_app(
            &tls_protocols_document(r#"protocols = ["TLSv1.1"]"#),
            "test",
        )
        .expect_err("TLS 1.1 must never be accepted");
        assert!(
            format!("{error}").contains("TLSv1.2|TLSv1.3"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn toml_shared_certificate_requires_one_tls_window() {
        let toml_text = r#"
[http]

[http.certificates."shared"]
certFile = "/etc/sdkwork/certs/shared/fullchain.pem"
certKeyFile = "/etc/sdkwork/certs/shared/privkey.pem"

[[http.server]]
listen = ["443 ssl"]
serverName = ["a.example.com"]

[http.server.tls]
cert = "shared"
protocols = ["TLSv1.2", "TLSv1.3"]

[[http.server.location]]
match = "/"
proxyPass = "http://127.0.0.1:9001"

[[http.server]]
listen = ["8443 ssl"]
serverName = ["b.example.com"]

[http.server.tls]
cert = "shared"
protocols = ["TLSv1.3"]

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let error = materialize_app(&effective, "test")
            .expect_err("two windows on one shared certificate must fail closed");
        assert!(
            format!("{error}").contains("different TLS version policies"),
            "unexpected error: {error}"
        );
    }

    /// Minimal typed-TOML document with one proxied location, used by the
    /// `expires` / `etag` location-key tests.
    fn cache_policy_document(location_block: &str) -> Value {
        let toml_text = format!(
            r#"
[http]

[[http.server]]
listen = ["80"]
serverName = ["cache.example.com"]

[[http.server.location]]
match = "/assets/"
proxyPass = "http://127.0.0.1:9001"
{location_block}
"#
        );
        let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
        serde_json::to_value(&value).expect("converts")
    }

    #[test]
    fn toml_expires_and_etag_reach_the_route() {
        // The typed plane must map the same nginx grammar the conf plane maps;
        // `expires`/`etag` used to be accepted-and-ignored here, so the
        // declaration had no effect on the served response.
        let config = materialize_app(
            &cache_policy_document(
                "expires = \"7d\"
etag = false",
            ),
            "test",
        )
        .expect("materializes");
        let policy = config.virtual_hosts[0].routes[0]
            .cache_policy
            .as_ref()
            .expect("route cache policy");
        assert_eq!(policy.expires, ExpiresMode::Access);
        assert_eq!(policy.expires_seconds, 604_800);
        assert!(!policy.etag);
        // The nginx default, since the typed plane declares no equivalent key.
        assert_eq!(policy.if_modified_since, IfModifiedSinceMode::Exact);

        // `modified <time>` is one string in TOML and an argument list in nginx.
        let config = materialize_app(&cache_policy_document("expires = \"modified 30m\""), "test")
            .expect("materializes");
        let policy = config.virtual_hosts[0].routes[0]
            .cache_policy
            .as_ref()
            .expect("route cache policy");
        assert_eq!(policy.expires, ExpiresMode::Modified);
        assert_eq!(policy.expires_seconds, 1_800);

        // An undeclared location keeps no policy of its own.
        let config = materialize_app(&cache_policy_document(""), "test").expect("materializes");
        assert!(config.virtual_hosts[0].routes[0].cache_policy.is_none());
    }

    #[test]
    fn toml_expires_rejects_malformed_values() {
        for (declaration, expected) in [
            ("expires = \"1x\"", "invalid `expires`"),
            ("expires = \"@25h\"", "less than 24 hours"),
            ("expires = 60", "must be a string"),
            (
                "expires = \"1d\"
etag = \"off\"",
                "must be a boolean",
            ),
        ] {
            let error = materialize_app(&cache_policy_document(declaration), "test")
                .err()
                .unwrap_or_else(|| panic!("`{declaration}` must fail closed"));
            let message = format!("{error}");
            assert!(
                message.contains(expected),
                "`{declaration}`: expected `{expected}` in `{message}`"
            );
        }
    }

    #[test]
    fn materializes_tunnel_absent_section_as_none() {
        let toml_text = "specVersion = 1
";
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective: Value = serde_json::to_value(&value).expect("converts");
        let tunnel = Materializer::materialize_tunnel(&effective).expect("materializes");
        assert!(tunnel.is_none());
    }

    #[test]
    fn loads_standalone_profile_with_gateway_target() {
        let dir = minimal_examples_dir("standalone-profile");
        let config = load_server_toml_app(&dir, "standalone", "production", "sdkwork-example")
            .expect("standalone example must load");
        let upstream = config
            .upstreams
            .iter()
            .find(|u| u.id == "gateway")
            .expect("upstream");
        assert_eq!(upstream.targets[0].url, "http://127.0.0.1:18079");
        assert_eq!(config.virtual_hosts.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn merges_im_example_production_hosts_for_all_base_domains() {
        let repo = examples_dir();
        let common = parse_toml_file(&repo.join("server.common.toml")).expect("common");
        let production = parse_toml_file(&repo.join("server.production.toml")).expect("production");
        let standalone = parse_toml_file(&repo.join("server.standalone.toml")).expect("standalone");
        let effective = merge_effective(&common, &production, &standalone).expect("merge");
        let servers = effective["http"]["server"].as_array().expect("servers");
        // Every base domain in the production example must survive the merge
        // (currently sdkwork/birdcoder/dtupay/noaper/skubc/zowalk/offer86/
        // 86offer across .com and .cn). Deriving the expectation from the
        // source document keeps this test honest across example updates
        // while still proving nothing was dropped.
        let expected: Vec<String> = production["http"]["server"]
            .as_array()
            .expect("production servers")
            .iter()
            .filter_map(|server| {
                server["serverName"]
                    .as_array()
                    .and_then(|names| names[0].as_str())
                    .map(str::to_owned)
            })
            .collect();
        let served: Vec<String> = servers
            .iter()
            .filter_map(|server| {
                server["serverName"]
                    .as_array()
                    .and_then(|names| names[0].as_str())
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(served, expected);
    }

    #[test]
    fn merges_the_product_webserver_layout_for_both_profiles() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates")
            .parent()
            .expect("sdkwork-webserver")
            .join("deployments")
            .join("webserver");
        let common = parse_toml_file(&repo.join("server.common.toml")).expect("common");
        let production = parse_toml_file(&repo.join("server.production.toml")).expect("production");
        let standalone = parse_toml_file(&repo.join("server.standalone.toml")).expect("standalone");
        let cloud = parse_toml_file(&repo.join("server.cloud.toml")).expect("cloud");
        assert_eq!(
            merge_effective(&common, &production, &standalone).expect("merge")["http"]["server"]
                .as_array()
                .expect("servers")
                .len(),
            14
        );
        let cloud_effective = merge_effective(&common, &production, &cloud).expect("merge cloud");
        assert_eq!(
            cloud_effective["http"]["server"]
                .as_array()
                .expect("servers")
                .len(),
            14
        );
        assert_eq!(
            cloud_effective["http"]["upstream"][0]["target"][0]["address"],
            "sdkwork-api-cloud-gateway:8080"
        );
    }

    #[test]
    fn rejects_wrong_profile_file() {
        let dir =
            std::env::temp_dir().join(format!("sdkwork-server-toml-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let examples = examples_dir();
        let common =
            std::fs::read_to_string(examples.join("server.common.toml")).expect("common text");
        let mut standalone = std::fs::read_to_string(examples.join("server.standalone.toml"))
            .expect("standalone text");
        standalone = standalone.replacen("profile = \"standalone\"", "profile = \"cloud\"", 1);
        write_layout_v3_dir(&dir, &common, MINIMAL_PRODUCTION, &standalone);
        let error = load_server_toml_app(&dir, "standalone", "production", "sdkwork-example")
            .expect_err("profile mismatch must be rejected");
        let message = error.to_string();
        assert!(
            message.contains("must declare profile"),
            "unexpected error: {message}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn merge_replaces_targets_wholesale() {
        let common: Value = serde_json::from_str(
            r#"{
            "http": {
                "upstream": [{
                    "name": "gateway",
                    "keepalive": 32,
                    "target": [{ "address": "127.0.0.1:3900", "weight": 1 }]
                }]
            }
        }"#,
        )
        .expect("fixture");
        let overlay: Value = serde_json::from_str(
            r#"{
            "http": {
                "upstream": [{
                    "name": "gateway",
                    "target": [{ "address": "10.0.4.12:3900", "weight": 3 }]
                }]
            }
        }"#,
        )
        .expect("fixture");
        let merged = merge_common_profile(&common, &overlay).expect("merge");
        let targets = merged["http"]["upstream"][0]["target"]
            .as_array()
            .expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0]["address"], "10.0.4.12:3900");
        assert_eq!(merged["http"]["upstream"][0]["keepalive"], 32);
    }

    #[test]
    fn merge_upserts_locations_by_match() {
        let common: Value = serde_json::from_str(
            r#"{
            "http": {
                "server": [{
                    "serverName": ["im.sdkwork.com"],
                    "location": [
                        { "match": "/api/", "proxyPass": "http://gateway" },
                        { "match": "/", "root": "/srv" }
                    ]
                }]
            }
        }"#,
        )
        .expect("fixture");
        let overlay: Value = serde_json::from_str(
            r#"{
            "http": {
                "server": [{
                    "serverName": ["im.sdkwork.com"],
                    "location": [
                        { "match": "/api/", "proxyReadTimeout": "120s" },
                        { "match": "/ws/", "proxyPass": "http://gateway" }
                    ]
                }]
            }
        }"#,
        )
        .expect("fixture");
        let merged = merge_common_profile(&common, &overlay).expect("merge");
        let locations = merged["http"]["server"][0]["location"]
            .as_array()
            .expect("locations");
        assert_eq!(locations.len(), 3);
        assert_eq!(locations[0]["proxyPass"], "http://gateway");
        assert_eq!(locations[0]["proxyReadTimeout"], "120s");
        assert_eq!(locations[2]["match"], "/ws/");
    }

    #[test]
    fn materializes_stream_servers_with_literal_and_upstream_targets() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-stream-materialize-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        write_layout_v3_dir(
            &dir,
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-test"

[[http.upstream]]
name = "db"
[[http.upstream.target]]
address = "127.0.0.1:5432"
weight = 1

[stream]
[[stream.server]]
listen = ["3306"]
proxyPass = "127.0.0.1:3306"
proxyTimeout = "30s"

[[stream.server]]
listen = ["127.0.0.1:5433"]
proxyPass = "db"
proxyProtocol = true
"#,
            r#"
[[http.server]]
listen = ["80"]
serverName = ["stream-test.local"]
[[http.server.location]]
match = "/"
returnStatus = 404
"#,
            "",
        );
        let config = load_server_toml_app(&dir, "standalone", "production", "stream-test")
            .expect("layout must load");
        assert_eq!(config.streams.len(), 2);
        let literal = &config.streams[0];
        assert_eq!(literal.id, "stream-0");
        assert_eq!(literal.bind, "0.0.0.0");
        assert_eq!(literal.port, 3306);
        assert_eq!(
            literal.target,
            Some(super::super::model::StreamTargetConfig::Literal {
                host: "127.0.0.1".to_owned(),
                port: 3306,
            })
        );
        assert_eq!(literal.proxy_timeout_ms, 30_000);
        assert!(!literal.proxy_protocol);
        let upstream = &config.streams[1];
        assert_eq!(upstream.bind, "127.0.0.1");
        assert_eq!(upstream.port, 5433);
        assert_eq!(
            upstream.target,
            Some(super::super::model::StreamTargetConfig::Upstream {
                name: "db".to_owned(),
            })
        );
        assert_eq!(upstream.proxy_timeout_ms, 60_000);
        assert!(upstream.proxy_protocol);
        assert!(upstream.tls.is_none());
        assert!(literal.tls.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn typed_proxy_method_reaches_the_proxy_resource() {
        let materialize = |method_line: &str| {
            let toml_text = format!(
                r#"
[[http.server]]
listen = ["8080"]
serverName = ["method.example.test"]
[[http.server.location]]
match = "/"
proxyPass = "http://127.0.0.1:9001"
{method_line}
"#
            );
            let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
            let effective = serde_json::to_value(&value).expect("converts");
            materialize_app(&effective, "test")
        };
        let config = materialize("proxyMethod = \"POST\"").expect("materialize");
        let proxy_method = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                crate::config::ResourceConfig::Proxy { proxy_method, .. } => {
                    Some(proxy_method.clone())
                }
                _ => None,
            })
            .expect("proxy resource");
        assert_eq!(proxy_method.as_deref(), Some("POST"));
        let error = materialize("proxyMethod = \"GET /x\"")
            .expect_err("invalid method token must fail closed");
        assert!(
            error.to_string().contains("not a valid HTTP method token"),
            "{error}"
        );
    }

    #[test]
    fn materializes_stream_return_and_access_in_typed_config() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-stream-access-materialize-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        write_layout_v3_dir(
            &dir,
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-access-test"

[stream]
[[stream.server]]
listen = ["127.0.0.1:3500"]
access = [
  { action = "allow", network = "127.0.0.1" },
  { action = "deny", network = "all" },
]
proxyPass = "127.0.0.1:3501"

[[stream.server]]
listen = ["127.0.0.1:3502"]
return = "403"
"#,
            r#"
[[http.server]]
listen = ["80"]
serverName = ["stream-access-test.local"]
[[http.server.location]]
match = "/"
returnStatus = 404
"#,
            "",
        );
        let config = load_server_toml_app(&dir, "standalone", "production", "stream-access-test")
            .expect("layout must load");
        assert_eq!(config.streams.len(), 2);
        let proxied = &config.streams[0];
        assert_eq!(proxied.access.len(), 2);
        assert_eq!(proxied.access[0].network, "127.0.0.1");
        assert_eq!(proxied.access[1].network, "all");
        assert!(proxied.target.is_some());
        let returned = &config.streams[1];
        assert_eq!(returned.return_value.as_deref(), Some("403"));
        assert!(returned.target.is_none());
        std::fs::remove_dir_all(&dir).ok();

        // `return` and `proxyPass` are mutually exclusive in the typed plane.
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-stream-access-conflict-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        write_layout_v3_dir(
            &dir,
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-access-test"

[stream]
[[stream.server]]
listen = ["127.0.0.1:3503"]
return = "403"
proxyPass = "127.0.0.1:3504"
"#,
            r#"
[[http.server]]
listen = ["80"]
serverName = ["stream-access-test.local"]
[[http.server.location]]
match = "/"
returnStatus = 404
"#,
            "",
        );
        let error = load_server_toml_app(&dir, "standalone", "production", "stream-access-test")
            .expect_err("return + proxyPass must fail closed");
        assert!(
            error
                .to_string()
                .contains("cannot combine `return` with `proxyPass`"),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn materializes_stream_tls_terminate_and_rejects_ssl_with_preread() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-stream-tls-materialize-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-tls-test"

[[http.server]]
listen = ["80"]
serverName = ["stream-tls-test.local"]
[[http.server.location]]
match = "/"
returnStatus = 404

[http.certificates.site]
certFile = "/tmp/site.pem"
certKeyFile = "/tmp/site.key"

[stream]
[[stream.server]]
listen = ["127.0.0.1:8443 ssl"]
certificate = "site"
proxyPass = "127.0.0.1:9443"
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let config = load_server_toml_app(&dir, "standalone", "production", "stream-tls-test")
            .expect("layout must load");
        assert_eq!(config.streams.len(), 1);
        assert_eq!(
            config.streams[0].tls,
            Some(super::super::model::StreamTlsMode::Terminate {
                certificate_ref: "site".to_owned(),
                client_auth: None,
            })
        );

        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-tls-reject"

[[http.server]]
listen = ["80"]
serverName = ["stream-tls-reject.local"]
[[http.server.location]]
match = "/"
returnStatus = 404

[http.certificates.site]
certFile = "/tmp/site.pem"
certKeyFile = "/tmp/site.key"

[stream]
[[stream.server]]
listen = ["127.0.0.1:8443 ssl"]
certificate = "site"
sslPreread = true
proxyPass = "127.0.0.1:9443"
"#,
        )
        .unwrap();
        let error = load_server_toml_app(&dir, "standalone", "production", "stream-tls-reject")
            .expect_err("ssl + sslPreread must fail closed");
        assert!(
            error.to_string().contains("mutually exclusive"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn materializes_stream_ssl_preread() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-stream-preread-materialize-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-preread-test"

[[http.server]]
listen = ["80"]
serverName = ["stream-preread-test.local"]
[[http.server.location]]
match = "/"
returnStatus = 404

[stream]
[[stream.server]]
listen = ["127.0.0.1:8443"]
sslPreread = true
proxyPass = "127.0.0.1:9443"
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let config = load_server_toml_app(&dir, "standalone", "production", "stream-preread-test")
            .expect("layout must load");
        assert_eq!(
            config.streams[0].tls,
            Some(super::super::model::StreamTlsMode::Preread)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn materializes_http_gzip_settings() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-gzip-materialize-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "gzip-test"

[http]
gzip = true
gzipTypes = ["application/json", "text/css"]
gzipMinLength = 64

[[http.server]]
listen = ["80"]
serverName = ["gzip-test.local"]
[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let config = load_server_toml_app(&dir, "standalone", "production", "gzip-test")
            .expect("layout must load");
        assert!(config.gzip.enabled);
        assert_eq!(config.gzip.min_length, 64);
        assert_eq!(
            config.gzip.types,
            vec!["application/json".to_owned(), "text/css".to_owned()]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_public_stream_binds_and_unknown_upstreams() {
        let dir =
            std::env::temp_dir().join(format!("sdkwork-stream-reject-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let common = r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-reject"

[[http.server]]
listen = ["80"]
serverName = ["stream-reject.local"]
[[http.server.location]]
match = "/"
returnStatus = 404
"#;
        std::fs::write(dir.join("server.common.toml"), common).expect("write common");
        std::fs::write(
            dir.join("server.standalone.toml"),
            r#"
profile = "standalone"

[stream]
[[stream.server]]
listen = ["3306"]
proxyPass = "missing-upstream"
"#,
        )
        .unwrap();
        let error = load_server_toml_app(&dir, "standalone", "production", "stream-reject")
            .expect_err("unknown upstream must be rejected");
        assert!(
            error.to_string().contains("undefined upstream"),
            "unexpected error: {error}"
        );

        std::fs::write(
            dir.join("server.standalone.toml"),
            r#"
profile = "standalone"

[stream]
[[stream.server]]
listen = ["3306"]
proxyPass = "127.0.0.1:3306"
"#,
        )
        .unwrap();
        // An explicit public IP bind fails closed per SDKWORK_WEBSERVER_SPEC §12.
        std::fs::write(
            dir.join("server.common.toml"),
            common.replacen(
                "[[http.server]]",
                "[stream]\n[[stream.server]]\nlisten = [\"8.8.8.8:3306\"]\nproxyPass = \"127.0.0.1:3306\"\n\n[[http.server]]",
                1,
            ),
        )
        .expect("write common");
        let error = load_server_toml_app(&dir, "standalone", "production", "stream-reject")
            .expect_err("public stream bind must be rejected");
        assert!(
            error.to_string().contains("loopback or private address"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn materializes_regex_prefix_exclusive_and_rewrite() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-regex-rewrite-materialize-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "regex-rewrite-test"

[[http.server]]
listen = ["80"]
serverName = ["regex-rewrite-test.local"]

[[http.server.location]]
match = "^~ /assets/"
root = "/var/www/assets"

[[http.server.location]]
match = '~ \.php$'
returnStatus = 403

[[http.server.location]]
match = "/"
rewrite = ['^/old/(.*)$ /new/$1 last']
returnStatus = 200
returnBody = "ok"
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let config = load_server_toml_app(&dir, "standalone", "production", "regex-rewrite-test")
            .expect("layout must load");
        let host = &config.virtual_hosts[0];
        let exclusive = host
            .routes
            .iter()
            .find(|route| route.route_match.path == "/assets/")
            .expect("exclusive route");
        assert_eq!(
            exclusive.route_match.path_type,
            super::super::model::RoutePathType::PrefixExclusive
        );
        let regex = host
            .routes
            .iter()
            .find(|route| route.route_match.path_type == super::super::model::RoutePathType::Regex)
            .expect("regex route");
        assert_eq!(regex.route_match.path, r"\.php$");
        let rewrite_route = host
            .routes
            .iter()
            .find(|route| !route.rewrite.is_empty())
            .expect("rewrite route");
        assert_eq!(rewrite_route.rewrite.len(), 1);
        assert_eq!(
            rewrite_route.rewrite[0].flag,
            super::super::model::RewriteFlag::Last
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn materializes_alias_static_with_try_files_spa_fallback() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-alias-materialize-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "alias-test"

[[http.server]]
listen = ["80"]
serverName = ["alias-test.local"]

[[http.server.location]]
match = "/assets/"
alias = "/var/www/static-assets/"
index = ["index.html"]
tryFiles = ["$uri", "$uri/", "/index.html"]

[[http.server.location]]
match = "/"
returnStatus = 404
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let config = load_server_toml_app(&dir, "standalone", "production", "alias-test")
            .expect("layout must load");
        let static_resource = config
            .resources
            .iter()
            .find_map(|resource| match resource {
                super::super::model::ResourceConfig::Static {
                    id,
                    root,
                    spa_fallback,
                    ..
                } if id.starts_with("loc-") => Some((root.as_str(), spa_fallback.as_deref())),
                _ => None,
            })
            .expect("alias static resource");
        assert_eq!(static_resource.0, "var/www/static-assets/");
        assert_eq!(static_resource.1, Some("index.html"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_alias_without_trailing_slash() {
        let dir =
            std::env::temp_dir().join(format!("sdkwork-alias-reject-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "alias-reject"

[[http.server]]
listen = ["80"]
serverName = ["alias-reject.local"]

[[http.server.location]]
match = "/assets/"
alias = "/var/www/static-assets"

[[http.server.location]]
match = "/"
returnStatus = 404
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let error = load_server_toml_app(&dir, "standalone", "production", "alias-reject")
            .expect_err("alias without trailing slash must fail");
        assert!(
            error
                .to_string()
                .contains("directory aliases must end with"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_alias_on_regex_location() {
        let dir =
            std::env::temp_dir().join(format!("sdkwork-alias-regex-reject-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "alias-regex-reject"

[[http.server]]
listen = ["80"]
serverName = ["alias-regex-reject.local"]

[[http.server.location]]
match = '~ ^/assets/(.*)$'
alias = "/var/www/static-assets/"

[[http.server.location]]
match = "/"
returnStatus = 404
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let error = load_server_toml_app(&dir, "standalone", "production", "alias-regex-reject")
            .expect_err("regex alias must fail");
        assert!(
            error.to_string().contains("regex location"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn materializes_auth_basic_from_htpasswd_file() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-auth-basic-materialize-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let htpasswd = dir.join("users.htpasswd");
        let hash = crate::config::apr1_hash("secret", "matsalt1");
        std::fs::write(&htpasswd, format!("alice:{hash}\n")).unwrap();
        let htpasswd_path = htpasswd.to_str().expect("utf8 path").replace('\\', "/");
        std::fs::write(
            dir.join("server.common.toml"),
            format!(
                r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "auth-basic-test"

[[http.server]]
listen = ["80"]
serverName = ["auth-basic-test.local"]

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
authBasic = "Restricted"
authBasicUserFile = "{htpasswd_path}"
"#
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let config = load_server_toml_app(&dir, "standalone", "production", "auth-basic-test")
            .expect("layout must load");
        let route = &config.virtual_hosts[0].routes[0];
        let auth = route.auth_basic.as_ref().expect("auth_basic");
        assert_eq!(auth.realm, "Restricted");
        assert_eq!(auth.users.len(), 1);
        assert_eq!(auth.users[0].username, "alice");
        assert_eq!(auth.users[0].password_hash, hash);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unenforced_behaviour_keys_surface_typed_toml_diagnostics() {
        let effective = serde_json::json!({
            "http": {
                "map": ["$http_host $x default"],
                "autoindex": true,
                "server": [{
                    "serverName": ["example.com"],
                    "location": [{
                        "match": "/",
                        "proxyPass": "http://127.0.0.1:8080",
                        "proxyBuffering": false,
                        "proxyInterceptErrors": true
                    }],
                    "tls": {
                        "cert": "example.com",
                        "preferServerCiphers": true,
                        "sessionTickets": false,
                        "verifyDepth": 2
                    }
                }],
                "certificates": {
                    "example.com": {
                        "certFile": "/etc/sdkwork/certs/example.com/fullchain.pem",
                        "certKeyFile": "/etc/sdkwork/certs/example.com/privkey.pem",
                        "ocspStapling": true
                    }
                }
            }
        });
        let diagnostics = accepted_unenforced_toml_diagnostics(&effective);
        let paths = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.path.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "server.toml.http.map",
            "server.toml.http.autoindex",
            "server.toml.http.server[0].location[0].proxyBuffering",
            "server.toml.http.server[0].location[0].proxyInterceptErrors",
            "server.toml.http.server[0].tls.preferServerCiphers",
            "server.toml.http.server[0].tls.sessionTickets",
            "server.toml.http.server[0].tls.verifyDepth",
            "server.toml.http.certificates.example.com.ocspStapling",
        ] {
            assert!(
                paths.contains(&expected),
                "missing diagnostic for {expected}: {paths:?}"
            );
        }
        // Every diagnostic must say the value has no effect, not merely name the key.
        for diagnostic in &diagnostics {
            assert!(
                diagnostic.message.contains("not enforced"),
                "{}: {}",
                diagnostic.path,
                diagnostic.message
            );
        }
    }

    #[test]
    fn enforced_and_process_tuning_keys_produce_no_diagnostics() {
        let effective = serde_json::json!({
            "main": {
                "workerProcesses": "auto",
                "workerConnections": 1024,
                "sendfile": true,
                "tcpNopush": true,
                "keepaliveTimeout": 75,
                "clientBodyTimeout": "60s",
                "logFormat": ["main '$request'"],
            },
            "http": {
                "gzip": true,
                "gzipTypes": ["text/css"],
                "clientMaxBodySize": "1100m",
                "server": [{
                    "serverName": ["example.com"],
                    "root": "/srv/www",
                    "location": [{
                        "match": "/",
                        "proxyPass": "http://127.0.0.1:8080",
                        "proxySetHeader": ["Host $host"],
                        "expires": "1h"
                    }]
                }]
            }
        });
        let diagnostics = accepted_unenforced_toml_diagnostics(&effective);
        assert!(
            diagnostics.is_empty(),
            "process tuning and enforced keys must not produce diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.path.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn typed_mail_only_application_materializes() {
        let toml_text = r#"
[mail]

[[mail.server]]
listen = ["127.0.0.1:2525"]
protocol = "pop3"
serverName = "pop.example.com"
timeout = "30s"
soKeepalive = true
proxyProtocol = true
pop3Auth = ["plain"]
smtpClientBuffer = "8k"
imapClientBuffer = "8k"
smtpGreetingDelay = "500ms"

[[mail.server]]
listen = ["127.0.0.1:2993"]
protocol = "smtp"
smtpAuth = ["plain", "login"]
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective = serde_json::to_value(&value).expect("converts");
        let config = materialize_app(&effective, "mail-only").expect("materialize");
        assert!(
            config.listeners.is_empty() && config.virtual_hosts.is_empty(),
            "a mail-only application has no HTTP surface"
        );
        assert_eq!(config.mail.servers.len(), 2);
        let pop3 = &config.mail.servers[0];
        assert_eq!(pop3.protocol, crate::config::MailServerProtocol::Pop3);
        assert_eq!(pop3.bind, "127.0.0.1");
        assert_eq!(pop3.port, 2525);
        assert_eq!(pop3.server_name.as_deref(), Some("pop.example.com"));
        assert_eq!(pop3.timeout_ms, 30_000);
        assert!(pop3.so_keepalive);
        assert!(pop3.proxy_protocol);
        assert_eq!(
            pop3.pop3_auth,
            vec![crate::config::MailAuthMechanism::Plain]
        );
        assert_eq!(pop3.smtp_client_buffer, 8 * 1024);
        assert_eq!(pop3.imap_client_buffer, 8 * 1024);
        assert_eq!(pop3.smtp_greeting_delay_ms, 500);
        let smtp = &config.mail.servers[1];
        assert_eq!(smtp.protocol, crate::config::MailServerProtocol::Smtp);
        assert_eq!(smtp.timeout_ms, 60_000);
        assert_eq!(
            smtp.smtp_auth,
            vec![
                crate::config::MailAuthMechanism::Plain,
                crate::config::MailAuthMechanism::Login
            ]
        );
    }

    #[test]
    fn typed_mail_9b_section_materializes_auth_http_tls_proxy_and_max_errors() {
        let toml_text = r#"
[mail]

[[mail.server]]
listen = ["127.0.0.1:2525"]
protocol = "smtp"
maxErrors = 3

[mail.server.authHttp]
uri = "127.0.0.1:9000/auth"
timeoutMs = 3000
headers = ["X-Tenant: acme"]

[mail.server.tls]
certificateFile = "/etc/sdkwork/certs/mail.pem"
privateKeyFile = "/etc/sdkwork/certs/mail.key"
starttls = "on"
protocols = ["TLSv1.2", "TLSv1.3"]

[mail.server.proxy]
xclient = false
proxyProtocol = true
bufferBytes = 8192
timeoutMs = 30000

[mail.server.proxy.tls]
verify = false
serverName = "backend.example.com"
"#;
        let value: toml::Value = toml::from_str(toml_text).expect("toml parses");
        let effective = serde_json::to_value(&value).expect("converts");
        let config = materialize_app(&effective, "mail-only").expect("materialize");
        let server = &config.mail.servers[0];
        let auth = server.auth_http.as_ref().expect("auth_http");
        assert_eq!(auth.uri, "127.0.0.1:9000/auth");
        assert_eq!(auth.timeout_ms, 3000);
        assert_eq!(auth.headers, vec!["X-Tenant: acme".to_owned()]);
        let tls = server.tls.as_ref().expect("mail tls");
        assert!(!tls.implicit);
        assert_eq!(tls.starttls, crate::config::MailStarttlsMode::On);
        assert_eq!(tls.protocols.len(), 2);
        assert!(!server.proxy.xclient);
        assert!(server.proxy.proxy_protocol);
        assert_eq!(server.proxy.buffer_bytes, 8192);
        let proxy_tls = server.proxy.tls.as_ref().expect("backend tls extension");
        assert!(!proxy_tls.verify);
        assert_eq!(
            proxy_tls.server_name.as_deref(),
            Some("backend.example.com")
        );
        assert_eq!(server.max_errors, 3);
    }

    #[test]
    fn typed_mail_section_rejects_wave_9b_mechanisms_and_unbounded_values() {
        for (mail_section, expected) in [
            (
                "[[mail.server]]\nlisten = [\"127.0.0.1:2525\"]\nprotocol = \"pop3\"\npop3Auth = [\"apop\"]\n",
                "Wave 9B",
            ),
            (
                "[[mail.server]]\nlisten = [\"127.0.0.1:2525\"]\nprotocol = \"imap\"\nimapAuth = []\n",
                "at least one auth mechanism",
            ),
            (
                "[[mail.server]]\nlisten = [\"127.0.0.1:2525\"]\nprotocol = \"smtp\"\nsmtpClientBuffer = \"128\"\n",
                "between 512 bytes and 1 MiB",
            ),
            (
                "[[mail.server]]\nlisten = [\"127.0.0.1:2525 ssl\"]\nprotocol = \"imap\"\n",
                "requires a `[mail.server.tls]`",
            ),
            (
                "[[mail.server]]\nlisten = [\"127.0.0.1:2525\"]\nprotocol = \"imap\"\nunknownKey = 1\n",
                "unknown or unsupported key",
            ),
        ] {
            let toml_text = format!("[mail]\n{mail_section}");
            let value: toml::Value = toml::from_str(&toml_text).expect("toml parses");
            let effective = serde_json::to_value(&value).expect("converts");
            let error = materialize_app(&effective, "mail-only").expect_err("must fail closed");
            let message = error.to_string();
            assert!(
                message.contains(expected),
                "expected `{expected}` in diagnostic, got: {message}"
            );
        }
    }
}
