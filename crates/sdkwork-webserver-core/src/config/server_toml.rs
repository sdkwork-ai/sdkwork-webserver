//! Layout v2 `server.toml` loader per `SDKWORK_WEBSERVER_SPEC.md`.
//!
//! Loads `deployments/webserver/server.common.toml` plus the profile file
//! (`server.standalone.toml` / `server.cloud.toml`), applies the standard
//! inheritance merge (scalar override, leaf-array replacement, identity-key
//! upsert for object arrays, wholesale upstream target replacement), and
//! materializes the effective configuration into the runtime
//! `WebServerAppConfig` model. The materialization matrix follows the spec
//! section 13.3; directives that the runtime model cannot express fail closed
//! instead of silently diverging from the declared intent.

use std::{collections::BTreeMap, path::Path};

use serde_json::{json, Map, Value};

use super::{
    error::WebServerConfigError,
    model::WebServerAppConfig,
    proxy_headers::merge_proxy_set_headers,
    validate_webserver_config,
};

/// Identity keys for object-array merge (spec section 2.3 rule 4).
fn identity_key(path: &str) -> Option<&'static str> {
    match path {
        "http.server" => Some("serverName"),
        "http.upstream" => Some("name"),
        "http.server.location" => Some("match"),
        "stream.server" => Some("listen"),
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
            let merged = merge_value(
                out.get(key).unwrap_or(&Value::Null),
                value,
                &child_path,
            );
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

/// Effective configuration for one profile (spec section 2.2).
pub fn merge_common_profile(
    common: &Value,
    profile: &Value,
) -> Result<Value, WebServerConfigError> {
    let mut overlay = profile.clone();
    if let Some(profile_value) = overlay.get("profile") {
        if !profile_value.is_null() {
            overlay
                .as_object_mut()
                .expect("object")
                .remove("profile");
        }
    }
    Ok(merge_value(common, &overlay, ""))
}

fn parse_toml_file(path: &Path) -> Result<Value, WebServerConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| WebServerConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let value: toml::Value = toml::from_str(&text).map_err(|source| WebServerConfigError::Toml {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::to_value(&value).map_err(|source| WebServerConfigError::Materialize(format!(
        "cannot convert TOML from {}: {source}",
        path.display()
    )))
}

fn materialize_error(path: &str, message: impl std::fmt::Display) -> WebServerConfigError {
    WebServerConfigError::Materialize(format!("{path}: {message}"))
}

fn materialize_static_resource(
    resource_id: &str,
    configured_root: &str,
    location: &Map<String, Value>,
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
    });
    if let Some(try_files) = location.get("tryFiles").and_then(Value::as_array) {
        if let Some(last) = try_files.iter().filter_map(Value::as_str).last() {
            if last.starts_with('/') {
                resource["spaFallback"] =
                    Value::String(last.trim_start_matches('/').to_owned());
            }
        }
    }
    resource
}

fn as_object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, WebServerConfigError> {
    value
        .as_object()
        .ok_or_else(|| materialize_error(path, "expected a TOML table"))
}

fn as_str<'a>(value: &'a Map<String, Value>, path: &str, key: &str) -> Result<&'a str, WebServerConfigError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| materialize_error(path, format!("`{key}` must be a string")))
}

fn as_array<'a>(value: &'a Map<String, Value>, path: &str, key: &str) -> Result<&'a Vec<Value>, WebServerConfigError> {
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
    "user", "workerProcesses", "workerRlimitNofile", "pid", "errorLog", "include", "raw",
    "workerConnections", "use", "acceptMutex", "multiAccept", "sendfile", "tcpNopush",
    "tcpNodelay", "keepaliveTimeout", "keepaliveRequests", "clientBodyTimeout",
    "clientHeaderTimeout", "clientBodyBufferSize", "clientHeaderBufferSize",
    "largeClientHeaderBuffers", "resetTimedoutConnection", "sendTimeout",
    "serverNamesHashMaxSize", "serverTokens", "defaultType", "logFormat", "accessLog",
    "map", "keepalive",
    // proxy knobs accepted at non-location contexts; location `proxySetHeader`
    // and websocket/buffering flags are listed in location supported keys and execute
    "proxyHttpVersion", "proxyBuffering", "proxyWebsocketUpgrade",
    "proxyInterceptErrors", "proxyNextUpstream", "proxyHideHeader",
    "proxyRequestBuffering", "proxyMethod",
    // upstream server flags the runtime owns via its own failure/ejection
    // policy (nginx `max_fails=` / `fail_timeout=` are accepted no-ops)
    "maxFails", "failTimeout",
    // `resolve` marks DNS-named targets for deploy-time cluster resolution;
    // the runtime always resolves hostnames at connection time.
    "resolve",
    // static file details
    "autoindex", "expires", "etag", "disableSymlinks", "logNotFound", "sendfileMaxChunk",
    "charset", "errorPage",
    // TLS tuning (the runtime enforces TLSv1.2/1.3 defaults)
    "ocspStapling", "preferServerCiphers", "sessionCache", "sessionTimeout",
    "sessionTickets", "stapling", "staplingVerify", "verifyDepth", "dhparam",
    "ecdhCurve", "ciphers",
];

/// Directives that change security semantics; declaring them in server.toml
/// fails closed because the runtime cannot enforce them.
const UNSUPPORTED_SECURITY: &[&str] = &[];

const SERVER_TLS_KEYS: &[&str] = &[
    "cert",
    "certFile",
    "certKeyFile",
    "chainFile",
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
fn parse_listen(entry: &str, path: &str) -> Result<(String, u16, bool, bool), WebServerConfigError> {
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
    let (bind, port) = if let Some((host, port_text)) = address.rsplit_once(':') {
        let port: u16 = port_text
            .parse()
            .map_err(|_| materialize_error(path, format!("invalid listen port in `{entry}`")))?;
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
            !v6.is_loopback()
                && !v6.is_unique_local()
                && !v6.is_unspecified()
                && !v6.is_multicast()
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
            return Err(materialize_error(path, "exact location match must start with `/`"));
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
            materialize_error(path, format!("invalid case-insensitive regex location: {error}"))
        })?;
        return Ok(("regex-ignore-case", rest));
    }
    if let Some(rest) = match_value.strip_prefix("~ ") {
        regex::Regex::new(rest).map_err(|error| {
            materialize_error(path, format!("invalid regex location: {error}"))
        })?;
        return Ok(("regex", rest));
    }
    if !match_value.starts_with('/') {
        return Err(materialize_error(path, "prefix location match must start with `/`"));
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
    let key = parts.next().ok_or_else(|| {
        materialize_error(path, "`hashKey` must declare a key variable")
    })?;
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
            crate::config::validate_md5_template(template).map_err(|message| {
                materialize_error(&format!("{path}.secureLinkMd5"), message)
            })?;
            for (key, value) in [
                ("secureLink", argument),
                ("secureLinkExpires", expires),
            ] {
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
            let contents = std::fs::read_to_string(file).map_err(|error| {
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
    resources: Vec<Value>,
    upstreams: Vec<Value>,
    upstream_names: Vec<String>,
    virtual_hosts: Vec<Value>,
    streams: Vec<Value>,
    limit_req_zones: Vec<Value>,
    limit_req_zone_names: Vec<String>,
    limit_conn_zones: Vec<Value>,
    limit_conn_zone_names: Vec<String>,
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
            resources: Vec::new(),
            upstreams: Vec::new(),
            upstream_names: Vec::new(),
            virtual_hosts: Vec::new(),
            streams: Vec::new(),
            limit_req_zones: Vec::new(),
            limit_req_zone_names: Vec::new(),
            limit_conn_zones: Vec::new(),
            limit_conn_zone_names: Vec::new(),
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
            if existing_ssl != &ssl {
                return Err(materialize_error(
                    path,
                    format!("port {port} is declared both with and without ssl; split listeners or unify TLS"),
                ));
            }
            if let Some(tls_policy) = tls_policy_ref {
                // Multiple servers may share one listener only when they agree
                // on the TLS policy (SNI selection is not modeled).
                if let Some(listener) = self.listeners.iter_mut().find(|l| l["id"] == *existing_id) {
                    let current = listener.get("tlsPolicyRef").and_then(Value::as_str);
                    if current.is_some() && current != Some(tls_policy) {
                        return Err(materialize_error(
                            path,
                            format!("port {port} is shared by servers with different TLS policies; use one certificate per listener port"),
                        ));
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
            }
        }
        self.listeners.push(listener);
        self.listeners_by_port.insert(key, (id.clone(), ssl));
        Ok(id)
    }

    fn ensure_certificate(
        &mut self,
        name: &str,
        cert: &Map<String, Value>,
        server_names: &[String],
        path: &str,
        client_auth: Option<&Value>,
    ) -> Result<(), WebServerConfigError> {
        if let Some(index) = self.certificate_names.iter().position(|existing| existing == name) {
            if let Some(entry) = self.certificates.get_mut(index) {
                if let Some(names) = entry.get_mut("serverNames").and_then(Value::as_array_mut) {
                    for server_name in server_names {
                        if !names.iter().any(|value| value.as_str() == Some(server_name.as_str())) {
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
        let (certificate_file, private_key_file) = match (acme, cert_file, cert_key_file) {
            (Some(acme_name), _, _) => (
                format!("/opt/certs/letsencrypt/live/{acme_name}/fullchain.pem"),
                format!("/opt/certs/letsencrypt/live/{acme_name}/privkey.pem"),
            ),
            (_, Some(cert_file), Some(cert_key_file)) => (cert_file.to_owned(), cert_key_file.to_owned()),
            _ => {
                return Err(materialize_error(
                    path,
                    format!("certificate `{name}` requires certFile + certKeyFile or acme"),
                ))
            }
        };
        self.certificates.push(json!({
            "id": name,
            "serverNames": server_names,
            "source": {
                "type": "protected-file",
                "certificateFile": certificate_file,
                "privateKeyFile": private_key_file,
            },
        }));
        let mut policy = json!({
            "id": format!("tls-{name}"),
            "certificateRefs": [name],
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["h2", "http/1.1"],
        });
        if let Some(client_auth) = client_auth {
            policy["clientAuth"] = client_auth.clone();
        }
        self.tls_policies.push(policy);
        self.certificate_names.push(name.to_owned());
        Ok(())
    }

    fn ensure_upstream(&mut self, upstream: &Map<String, Value>, path: &str) -> Result<String, WebServerConfigError> {
        check_supported_keys(upstream, path, &[
            "name", "target", "loadBalancing", "hashKey",
        ])?;
        let name = as_str(upstream, path, "name")?.to_owned();
        if self.upstream_names.contains(&name) {
            return Ok(name);
        }
        let targets = upstream
            .get("target")
            .and_then(Value::as_array)
            .ok_or_else(|| materialize_error(path, format!("upstream `{name}` must declare a target array")))?;
        let mut target_values = Vec::new();
        let mut authorized_literal_ips: Vec<String> = Vec::new();
        for (index, target) in targets.iter().enumerate() {
            let target_path = format!("{path}.target[{index}]");
            let target = target
                .as_object()
                .ok_or_else(|| materialize_error(&target_path, "target entries must be tables"))?;
            check_supported_keys(target, &target_path, &[
                "address", "weight", "backup", "down", "maxConnections",
                "slowStartMs",
            ])?;
            let address = as_str(target, &target_path, "address")?;
            let down = target.get("down").and_then(Value::as_bool).unwrap_or(false);
            if down {
                continue;
            }
            let url = if address.starts_with("unix:") {
                return Err(materialize_error(&target_path, "unix: upstream sockets are not supported by the runtime model"));
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
            let host = address.rsplit_once(':').map(|(host, _)| host).unwrap_or(&address);
            if host.parse::<std::net::IpAddr>().is_ok() {
                authorized_literal_ips.push(format!("{host}/32"));
            }
        }
        if target_values.is_empty() {
            return Err(materialize_error(path, format!("upstream `{name}` has no live targets after `down` filtering")));
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
                        materialize_error(path, "`hashKey` is required when loadBalancing = \"hash\"")
                    })?;
                let hash = parse_upstream_hash_key(hash_key, path)?;
                ("hash", Some(hash))
            }
            Some(other) => {
                return Err(materialize_error(path, format!("unknown loadBalancing `{other}`")))
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

    fn materialize_location(
        &mut self,
        server_index: usize,
        location_index: usize,
        location: &Map<String, Value>,
        server_name: &str,
        inherited_proxy_set_headers: &[String],
        inherited_root: Option<&str>,
    ) -> Result<String, WebServerConfigError> {
        let path = format!("http.server[{server_index}].location[{location_index}]");
        check_supported_keys(location, &path, &[
            "match", "proxyPass", "root", "alias", "index", "tryFiles", "returnStatus",
            "returnBody", "returnLocation", "proxyConnectTimeout", "proxyReadTimeout",
            "proxySendTimeout", "proxyBufferSize", "proxyRedirect", "proxySetHeader",
            "proxyWebsocketUpgrade", "proxyHttpVersion", "proxyBuffering", "proxyPassRequestHeaders",
            "allow", "deny",
            "limitReq", "limitConn", "rewrite", "authBasic", "authBasicUserFile", "subFilter",
            "subFilterOnce", "subFilterTypes", "subFilterLastModified", "secureLinkSecret",
            "secureLink", "secureLinkMd5", "secureLinkExpires",
        ])?;
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
        let inherits_static = serving == 0
            && location.contains_key("tryFiles")
            && inherited_root.is_some();
        if serving > 1 || (serving == 0 && !inherits_static) {
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

        if let Some(proxy_pass) = location.get("proxyPass").and_then(Value::as_str) {
            let proxy_pass_request_headers = location
                .get("proxyPassRequestHeaders")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if proxy_pass.contains('$') {
                crate::config::validate_proxy_pass_template(proxy_pass).map_err(|message| {
                    materialize_error(&path, message)
                })?;
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
                for entry in request_set_headers.iter().chain(inherited_proxy_set_headers) {
                    validate_proxy_set_header_entry(&path, entry)?;
                }
                let merged_headers =
                    merge_proxy_set_headers(inherited_proxy_set_headers, &request_set_headers);
                self.resources.push(json!({
                    "id": resource_id,
                    "type": "proxy",
                    "stripPrefix": false,
                    "requestSetHeaders": merged_headers,
                    "upstreamRef": "",
                    "dynamicTarget": proxy_pass,
                    "proxyPassRequestHeaders": proxy_pass_request_headers,
                }));
            } else {
            let upstream_ref = if let Some(rest) = proxy_pass.strip_prefix("http://").or_else(|| proxy_pass.strip_prefix("https://")) {
                if rest.contains(':') {
                    // Literal host:port target: synthesize a dedicated upstream.
                    let literal_id = format!("literal-{}", rest.replace([':', '/', '.'], "-"));
                    if !self.upstream_names.contains(&literal_id) {
                        let host = rest.rsplit_once(':').map_or("", |(host, _)| host);
                        let address_policy = host.parse::<std::net::IpAddr>().ok().map(|ip| {
                            match ip {
                                std::net::IpAddr::V4(ip) => format!("{ip}/32"),
                                std::net::IpAddr::V6(ip) => format!("{ip}/128"),
                            }
                        });
                        let mut literal_upstream = json!({
                            "id": literal_id,
                            "targets": [{ "url": proxy_pass }],
                            "loadBalancing": "round-robin",
                        });
                        if let Some(cidr) = address_policy {
                            literal_upstream["addressPolicy"] =
                                json!({ "allowedCidrs": [cidr] });
                        }
                        self.upstreams.push(literal_upstream);
                        self.upstream_names.push(literal_id.clone());
                    }
                    literal_id
                } else {
                    rest.to_owned()
                }
            } else {
                return Err(materialize_error(&path, format!("proxyPass `{proxy_pass}` must be http(s)://upstream or http(s)://host:port")));
            };
            if !self.upstream_names.contains(&upstream_ref) {
                return Err(materialize_error(&path, format!("proxyPass references undefined upstream `{upstream_ref}`")));
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
            for entry in request_set_headers.iter().chain(inherited_proxy_set_headers) {
                validate_proxy_set_header_entry(&path, entry)?;
            }
            let merged_headers = merge_proxy_set_headers(inherited_proxy_set_headers, &request_set_headers);
            self.resources.push(json!({
                "id": resource_id,
                "type": "proxy",
                "upstreamRef": upstream_ref,
                "stripPrefix": false,
                "requestSetHeaders": merged_headers,
                "proxyPassRequestHeaders": proxy_pass_request_headers,
            }));
            }
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
            ));
        } else if let Some(alias) = location.get("alias").and_then(Value::as_str) {
            // Runtime static serving strips the matched location prefix before
            // joining the configured directory (nginx `alias` semantics).
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
            ));
        } else if let Some(status) = location.get("returnStatus").and_then(Value::as_u64) {
            let status: u16 = u16::try_from(status)
                .map_err(|_| materialize_error(&path, format!("invalid returnStatus `{status}`")))?;
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
        check_supported_keys(server, &path, &[
            "listen", "serverName", "http2", "root", "index", "tryFiles",
            "tls", "proxySetHeader", "addHeader", "location",
        ])?;
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
            .ok_or_else(|| materialize_error(&path, "serverName must be a non-empty array of hostnames"))?
            .to_owned();
        let server_name_list: Vec<String> = server_names
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let http2 = server.get("http2").and_then(Value::as_bool).unwrap_or(false);
        let tls_table = server.get("tls").and_then(Value::as_object);
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
                    "minimumVersion": "tls1.2",
                    "maximumVersion": "tls1.3",
                    "alpn": ["h2", "http/1.1"],
                });
                if let Some(client_auth) = &client_auth {
                    policy["clientAuth"] = client_auth.clone();
                }
                self.tls_policies.push(policy);
                self.certificate_names.push(inline_name.clone());
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
                    return Err(materialize_error(&path, format!("listen `{entry}` requires [http.server.tls] with a certificate")));
                };
                self.ensure_certificate(
                    certificate,
                    &Map::new(),
                    &server_name_list,
                    &path,
                    client_auth.as_ref(),
                )?;
            }
            let tls_policy_ref = ssl.then(|| format!("tls-{}", certificate_name.as_deref().expect("checked ssl")));
            let listener_id = self.ensure_listener(
                &bind,
                port,
                ssl,
                http2,
                tls_policy_ref.as_deref(),
                &path,
            )?;
            if !listener_refs.contains(&listener_id) {
                listener_refs.push(listener_id);
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
            let resource_id = self.materialize_location(
                server_index,
                location_index,
                location,
                &primary_name,
                &server_proxy_set_headers,
                server_root,
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
            route_entries.push(route);
        }
        virtual_host["routes"] = Value::Array(route_entries);
        self.virtual_hosts.push(virtual_host);
        Ok(())
    }

    fn materialize_upstreams(&mut self, http: &Map<String, Value>) -> Result<(), WebServerConfigError> {
        if let Some(upstreams) = http.get("upstream").and_then(Value::as_array) {
            for (index, upstream) in upstreams.iter().enumerate() {
                let upstream = upstream
                    .as_object()
                    .ok_or_else(|| materialize_error("http.upstream", "upstream entries must be tables"))?;
                self.ensure_upstream(upstream, &format!("http.upstream[{index}]"))?;
            }
        }
        Ok(())
    }

    fn materialize_certificates(&mut self, http: &Map<String, Value>) -> Result<(), WebServerConfigError> {
        if let Some(certificates) = http.get("certificates").and_then(Value::as_object) {
            for (name, cert) in certificates {
                let cert = cert
                    .as_object()
                    .ok_or_else(|| materialize_error("http.certificates", "certificate entries must be tables"))?;
                self.ensure_certificate(name, cert, &[], &format!("http.certificates.{name}"), None)?;
            }
        }
        Ok(())
    }

    fn materialize_streams(&mut self, root: &Map<String, Value>) -> Result<(), WebServerConfigError> {
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
            check_supported_keys(server, &path, &[
                "listen", "proxyPass", "proxyTimeout", "proxyProtocol",
                "sslPreread", "certificate", "protocol", "clientCertificate",
                "clientCertificateCA",
            ])?;
            let listen = as_array(server, &path, "listen")?;
            let first = listen
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| materialize_error(&path, "listen must be a non-empty array of bindings"))?;
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
                if !self.certificate_names.iter().any(|name| name == certificate_ref) {
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
                materialize_error(&path, format!("stream listen host `{bind}` must be an IP address"))
            })?;
            if !bind_ip.is_unspecified() && is_public_ip(bind_ip) {
                return Err(materialize_error(
                    &path,
                    "stream listeners must bind a loopback or private address unless approved (SDKWORK_WEBSERVER_SPEC section 12)",
                ));
            }
            let proxy_pass = as_str(server, &path, "proxyPass")?;
            let target = if let Some((host, port_text)) = proxy_pass.rsplit_once(':') {
                if host.is_empty() || !port_text.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(materialize_error(
                        &path,
                        format!("proxyPass `{proxy_pass}` must be host:port or a declared upstream name"),
                    ));
                }
                let port: u16 = port_text
                    .parse()
                    .map_err(|_| materialize_error(&path, format!("invalid proxyPass port in `{proxy_pass}`")))?;
                crate::config::model::StreamTargetConfig::Literal {
                    host: host.to_owned(),
                    port,
                }
            } else if self.upstream_names.contains(&proxy_pass.to_owned()) {
                crate::config::model::StreamTargetConfig::Upstream {
                    name: proxy_pass.to_owned(),
                }
            } else {
                return Err(materialize_error(
                    &path,
                    format!("proxyPass references undefined upstream `{proxy_pass}`"),
                ));
            };
            let proxy_timeout_ms = match server.get("proxyTimeout").and_then(Value::as_str) {
                Some(value) => materialize_duration_ms(value)
                    .ok_or_else(|| materialize_error(&path, format!("invalid proxyTimeout `{value}`")))?,
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

    fn finish(
        self,
        limits: Value,
        nginx: Value,
        gzip: Value,
        proxy_cache: Value,
    ) -> Result<WebServerAppConfig, WebServerConfigError> {
        let instance = json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": self.app_key,
            "nginx": nginx,
            "gzip": gzip,
            "limitReqZones": self.limit_req_zones,
            "limitConnZones": self.limit_conn_zones,
            "limits": limits,
            "listeners": self.listeners,
            "certificates": self.certificates,
            "tlsPolicies": self.tls_policies,
            "resources": self.resources,
            "upstreams": self.upstreams,
            "virtualHosts": self.virtual_hosts,
            "streams": self.streams,
            "proxyCache": proxy_cache,
            "metadata": { "source": "deployments/webserver/server.toml" },
        });
        let config: WebServerAppConfig = serde_json::from_value(instance).map_err(|source| {
            WebServerConfigError::Materialize(format!("runtime model deserialization failed: {source}"))
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
            let zone = crate::config::parse_limit_conn_zone(text).map_err(|error| {
                materialize_error(&path, error.to_string())
            })?;
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
            let zone = crate::config::parse_limit_req_zone(text).map_err(|error| {
                materialize_error(&path, error.to_string())
            })?;
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
/// The document uses the same typed surface as the layout v2 effective
/// configuration (`[main]`, `[http]`, `[[http.server]]`, `[[http.upstream]]`,
/// `[[stream.server]]`, `[http.certificates.*]`, `[nginx]`, `proxyCache`).
/// A root `profile` key is layout-merge metadata and is ignored here.
pub fn load_server_toml_file(
    path: impl AsRef<Path>,
    app_key: &str,
) -> Result<WebServerAppConfig, WebServerConfigError> {
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
    materialize_app(&effective, app_key)
}

/// Load and materialize a layout v2 `server.toml` directory for one profile.
///
/// `dir` must contain `server.common.toml` and `server.<profile>.toml`;
/// `profile` is `"standalone"` or `"cloud"`.
pub fn load_server_toml_app(
    dir: impl AsRef<Path>,
    profile: &str,
    app_key: &str,
) -> Result<WebServerAppConfig, WebServerConfigError> {
    let dir = dir.as_ref();
    let common_path = dir.join("server.common.toml");
    let profile_path = dir.join(format!("server.{profile}.toml"));
    if !common_path.exists() || !profile_path.exists() {
        return Err(WebServerConfigError::Materialize(format!(
            "layout v2 requires {} and {} in {}",
            common_path.display(),
            profile_path.display(),
            dir.display()
        )));
    }
    let common = parse_toml_file(&common_path)?;
    let overlay = parse_toml_file(&profile_path)?;
    let declared = overlay.get("profile").and_then(Value::as_str);
    if declared != Some(profile) {
        return Err(WebServerConfigError::Materialize(format!(
            "{} must declare profile = \"{profile}\" (found {:?})",
            profile_path.display(),
            declared
        )));
    }
    let effective = merge_common_profile(&common, &overlay)?;
    materialize_app(&effective, app_key)
}

/// Materialize an already-merged effective TOML document.
pub fn materialize_app(
    effective: &Value,
    app_key: &str,
) -> Result<WebServerAppConfig, WebServerConfigError> {
    let root = as_object(effective, "server.toml")?;
    if root.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Err(WebServerConfigError::Materialize(
            "effective configuration has enabled = false; no runtime app can be materialized".to_owned(),
        ));
    }
    check_supported_keys(root, "server.toml", &[
        "specVersion", "kind", "id", "enabled", "description", "nginx", "main", "http",
        "stream", "proxyCache",
    ])?;
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
    let nginx = match root.get("nginx").and_then(Value::as_object) {
        Some(nginx) => {
            if nginx.get("nginxProfile").is_some() {
                return Err(materialize_error(
                    "server.toml.nginx.nginxProfile",
                    RETIRED_NGINX_PROFILE,
                ));
            }
            check_supported_keys(nginx, "server.toml.nginx", &[
                "enabled", "profile", "unknownDirectivePolicy", "exceptionRef",
                "strict", "confFile",
            ])?;
            json!({
                "enabled": nginx.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                "profile": nginx.get("profile").and_then(Value::as_str).unwrap_or("http-core-v1"),
                "unknownDirectivePolicy": nginx.get("unknownDirectivePolicy").and_then(Value::as_str).unwrap_or("error"),
            })
        }
        None => json!({ "enabled": true, "profile": "http-core-v1", "unknownDirectivePolicy": "error" }),
    };
    let mut limits = json!({});
    let mut gzip = json!({ "enabled": false, "types": [], "minLength": 20 });
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
        check_supported_keys(cache, "server.toml.proxyCache", &[
            "enabled", "maxEntries", "maxObjectBytes", "defaultTtlSeconds",
            "staleTtlSeconds", "diskPath",
        ])?;
    }
    let mut materializer = Materializer::new(app_key);
    let http = root
        .get("http")
        .and_then(Value::as_object)
        .ok_or_else(|| materialize_error("server.toml", "http table is required for an enabled configuration"))?;
    check_supported_keys(http, "server.toml.http", &[
        "certificates", "clientMaxBodySize", "gzip", "gzipTypes", "gzipMinLength",
        "keepaliveTimeout", "limitConnZone", "limitReqZone", "server", "upstream",
    ])?;
    materializer.materialize_limit_req_zones(http)?;
    materializer.materialize_limit_conn_zones(http)?;
    materializer.materialize_certificates(http)?;
    materializer.materialize_upstreams(http)?;
    if let Some(servers) = http.get("server").and_then(Value::as_array) {
        for (index, server) in servers.iter().enumerate() {
            let server = server
                .as_object()
                .ok_or_else(|| materialize_error("http.server", "server entries must be tables"))?;
            materializer.materialize_server(index, server)?;
        }
    }
    // Stream servers resolve upstream references against the materialized
    // upstream set, so they are materialized after http.upstream.
    materializer.materialize_streams(root)?;
    if materializer.virtual_hosts.is_empty() {
        return Err(WebServerConfigError::Materialize(
            "no [[http.server]] virtual hosts are declared in the effective configuration".to_owned(),
        ));
    }
    materializer.finish(limits, nginx, gzip, proxy_cache)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    }

    #[test]
    fn loads_cloud_profile_from_example_layout() {
        let config = load_server_toml_app(examples_dir(), "cloud", "sdkwork-example")
            .expect("cloud example must load");
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.app_key, "sdkwork-example");
        // Upstream target replaced wholesale by the cloud delta.
        let upstream = config.upstreams.iter().find(|u| u.id == "api_backend").expect("upstream");
        assert_eq!(upstream.targets.len(), 1);
        assert_eq!(upstream.targets[0].url, "http://10.0.4.12:3900");
        // Virtual hosts inherited from common (2 hosts).
        assert_eq!(config.virtual_hosts.len(), 2);
        let main = config.virtual_hosts.iter().find(|v| v.server_names.contains(&"im.sdkwork.com".to_owned())).expect("main host");
        assert!(main.listener_refs.iter().any(|id| id.contains("443")));
        // Resources: proxy, static, respond present.
        let kinds: Vec<&str> = config
            .resources
            .iter()
            .map(|r| match r {
                super::super::model::ResourceConfig::Proxy { .. } => "proxy",
                super::super::model::ResourceConfig::Static { .. } => "static",
                super::super::model::ResourceConfig::Respond { .. } => "respond",
                super::super::model::ResourceConfig::Redirect { .. } => "redirect",
                super::super::model::ResourceConfig::Drive { .. } => "drive",
                super::super::model::ResourceConfig::Knowledgebase { .. } => "knowledgebase",
            })
            .collect();
        assert!(kinds.contains(&"proxy"), "expected proxy resource");
        assert!(kinds.contains(&"static"), "expected static resource");
        assert!(kinds.contains(&"respond"), "expected respond resource");
    }

    #[test]
    fn loads_standalone_profile_with_dual_targets() {
        let config = load_server_toml_app(examples_dir(), "standalone", "sdkwork-example")
            .expect("standalone example must load");
        let upstream = config.upstreams.iter().find(|u| u.id == "api_backend").expect("upstream");
        assert_eq!(upstream.targets.len(), 2);
        assert_eq!(upstream.targets[0].url, "http://127.0.0.1:3900");
        assert!(upstream.targets[1].backup);
    }

    /// The product's own shipped layout must materialize completely for both
    /// deployment profiles: every virtual host resolves its routes to declared
    /// resources, every proxy references a declared upstream, and TLS listeners
    /// carry the shared certificate policy.
    #[test]
    fn loads_the_product_webserver_layout_for_both_profiles() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates")
            .parent()
            .expect("sdkwork-webserver")
            .join("deployments")
            .join("webserver");

        let standalone =
            load_server_toml_app(&repo, "standalone", "sdkwork-webserver").expect("standalone layout must load");
        let cloud = load_server_toml_app(&repo, "cloud", "sdkwork-webserver")
            .expect("cloud layout must load");
        for config in [&standalone, &cloud] {
            assert_eq!(config.virtual_hosts.len(), 13, "one server block per registered host");
            assert_eq!(config.listeners.len(), 3, "443, 80, and the loopback operations listener");
            let certificate = config
                .certificates
                .iter()
                .find(|cert| cert.id == "sdkwork")
                .expect("shared wildcard certificate");
            let super::super::model::CertificateSource::ProtectedFile {
                certificate_file, ..
            } = &certificate.source;
            assert_eq!(
                certificate_file,
                "/opt/certs/letsencrypt/live/sdkwork.com/fullchain.pem"
            );
            let tls_listener = config
                .listeners
                .iter()
                .find(|listener| listener.port == 443)
                .expect("443 listener");
            assert_eq!(
                tls_listener.tls_policy_ref.as_deref(),
                Some("tls-sdkwork"),
                "443 must use the shared TLS policy"
            );
            assert_eq!(tls_listener.protocols.len(), 2, "http1 + http2");
            let plaintext = config
                .listeners
                .iter()
                .find(|listener| listener.port == 80)
                .expect("80 listener");
            assert!(plaintext.allow_plaintext_http, "declared plaintext listen must be honored");

            let resource_ids = config
                .resources
                .iter()
                .map(|resource| match resource {
                    super::super::model::ResourceConfig::Proxy { id, .. } => id.as_str(),
                    super::super::model::ResourceConfig::Respond { id, .. } => id.as_str(),
                    other => panic!("unexpected resource kind {other:?}"),
                })
                .collect::<std::collections::HashSet<_>>();
            for host in &config.virtual_hosts {
                for route in &host.routes {
                    assert!(
                        resource_ids.contains(route.resource_ref.as_str()),
                        "route {} on {} references a missing resource",
                        route.id,
                        host.server_names.join(",")
                    );
                }
            }
            let upstream_ids = config
                .upstreams
                .iter()
                .map(|upstream| upstream.id.as_str())
                .collect::<std::collections::HashSet<_>>();
            for resource in &config.resources {
                if let super::super::model::ResourceConfig::Proxy { upstream_ref, .. } = resource {
                    assert!(
                        upstream_ids.contains(upstream_ref.as_str()),
                        "proxy {} references a missing upstream {upstream_ref}",
                        resource.id()
                    );
                }
            }
        }

        let gateway = standalone
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "gateway")
            .expect("gateway upstream");
        assert_eq!(
            gateway.targets.iter().map(|target| target.url.as_str()).collect::<Vec<_>>(),
            vec!["http://127.0.0.1:3800"]
        );
        let gateway = cloud
            .upstreams
            .iter()
            .find(|upstream| upstream.id == "gateway")
            .expect("gateway upstream");
        assert_eq!(
            gateway.targets.iter().map(|target| target.url.as_str()).collect::<Vec<_>>(),
            vec!["http://sdkwork-api-cloud-gateway:80"]
        );
        let exact_healthz = standalone
            .virtual_hosts
            .iter()
            .find(|host| host.server_names.contains(&"server.sdkwork.com".to_owned()))
            .expect("main public host")
            .routes
            .iter()
            .find(|route| route.route_match.path == "/healthz")
            .expect("healthz route");
        assert_eq!(
            exact_healthz.route_match.path_type,
            super::super::model::RoutePathType::Exact,
            "= /healthz must materialize as an exact match"
        );
    }

    #[test]
    fn rejects_wrong_profile_file() {
        let dir = std::env::temp_dir().join(format!("sdkwork-server-toml-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let examples = examples_dir();
        let common = std::fs::read_to_string(examples.join("server.common.toml")).expect("common text");
        let mut standalone =
            std::fs::read_to_string(examples.join("server.standalone.toml")).expect("standalone text");
        standalone = standalone.replacen("profile = \"standalone\"", "profile = \"cloud\"", 1);
        std::fs::write(dir.join("server.common.toml"), common).expect("write common");
        std::fs::write(dir.join("server.standalone.toml"), standalone).expect("write standalone");
        std::fs::write(dir.join("server.cloud.toml"), "profile = \"cloud\"\n").expect("write cloud");
        let error = load_server_toml_app(&dir, "standalone", "sdkwork-example")
            .err()
            .expect("profile mismatch must be rejected");
        let message = error.to_string();
        assert!(
            message.contains("must declare profile"),
            "unexpected error: {message}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn merge_replaces_targets_wholesale() {
        let common: Value = serde_json::from_str(r#"{
            "http": {
                "upstream": [{
                    "name": "gateway",
                    "keepalive": 32,
                    "target": [{ "address": "127.0.0.1:3900", "weight": 1 }]
                }]
            }
        }"#)
        .expect("fixture");
        let overlay: Value = serde_json::from_str(r#"{
            "http": {
                "upstream": [{
                    "name": "gateway",
                    "target": [{ "address": "10.0.4.12:3900", "weight": 3 }]
                }]
            }
        }"#)
        .expect("fixture");
        let merged = merge_common_profile(&common, &overlay).expect("merge");
        let targets = merged["http"]["upstream"][0]["target"].as_array().expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0]["address"], "10.0.4.12:3900");
        assert_eq!(merged["http"]["upstream"][0]["keepalive"], 32);
    }

    #[test]
    fn merge_upserts_locations_by_match() {
        let common: Value = serde_json::from_str(r#"{
            "http": {
                "server": [{
                    "serverName": ["im.sdkwork.com"],
                    "location": [
                        { "match": "/api/", "proxyPass": "http://gateway" },
                        { "match": "/", "root": "/srv" }
                    ]
                }]
            }
        }"#)
        .expect("fixture");
        let overlay: Value = serde_json::from_str(r#"{
            "http": {
                "server": [{
                    "serverName": ["im.sdkwork.com"],
                    "location": [
                        { "match": "/api/", "proxyReadTimeout": "120s" },
                        { "match": "/ws/", "proxyPass": "http://gateway" }
                    ]
                }]
            }
        }"#)
        .expect("fixture");
        let merged = merge_common_profile(&common, &overlay).expect("merge");
        let locations = merged["http"]["server"][0]["location"].as_array().expect("locations");
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
        std::fs::write(
            dir.join("server.common.toml"),
            r#"
specVersion = 1
kind = "sdkwork.webserver.server"
id = "stream-test"

[[http.server]]
listen = ["80"]
serverName = ["stream-test.local"]
[[http.server.location]]
match = "/"
returnStatus = 404

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
        )
        .unwrap();
        std::fs::write(
            dir.join("server.standalone.toml"),
            "profile = \"standalone\"\n",
        )
        .unwrap();
        let config = load_server_toml_app(&dir, "standalone", "stream-test").expect("layout must load");
        assert_eq!(config.streams.len(), 2);
        let literal = &config.streams[0];
        assert_eq!(literal.id, "stream-0");
        assert_eq!(literal.bind, "0.0.0.0");
        assert_eq!(literal.port, 3306);
        assert_eq!(
            literal.target,
            super::super::model::StreamTargetConfig::Literal {
                host: "127.0.0.1".to_owned(),
                port: 3306,
            }
        );
        assert_eq!(literal.proxy_timeout_ms, 30_000);
        assert!(!literal.proxy_protocol);
        let upstream = &config.streams[1];
        assert_eq!(upstream.bind, "127.0.0.1");
        assert_eq!(upstream.port, 5433);
        assert_eq!(
            upstream.target,
            super::super::model::StreamTargetConfig::Upstream {
                name: "db".to_owned(),
            }
        );
        assert_eq!(upstream.proxy_timeout_ms, 60_000);
        assert!(upstream.proxy_protocol);
        assert!(upstream.tls.is_none());
        assert!(literal.tls.is_none());
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
        let config = load_server_toml_app(&dir, "standalone", "stream-tls-test").expect("layout must load");
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
        let error = load_server_toml_app(&dir, "standalone", "stream-tls-reject")
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
        let config =
            load_server_toml_app(&dir, "standalone", "stream-preread-test").expect("layout must load");
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
        let config = load_server_toml_app(&dir, "standalone", "gzip-test").expect("layout must load");
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
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-stream-reject-test-{}",
            std::process::id()
        ));
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
        let error = load_server_toml_app(&dir, "standalone", "stream-reject")
            .err()
            .expect("unknown upstream must be rejected");
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
        let error = load_server_toml_app(&dir, "standalone", "stream-reject")
            .err()
            .expect("public stream bind must be rejected");
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
        let config =
            load_server_toml_app(&dir, "standalone", "regex-rewrite-test").expect("layout must load");
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
        let config = load_server_toml_app(&dir, "standalone", "alias-test").expect("layout must load");
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
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-alias-reject-test-{}",
            std::process::id()
        ));
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
        let error = load_server_toml_app(&dir, "standalone", "alias-reject")
            .err()
            .expect("alias without trailing slash must fail");
        assert!(
            error.to_string().contains("directory aliases must end with"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_alias_on_regex_location() {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-alias-regex-reject-{}",
            std::process::id()
        ));
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
        let error = load_server_toml_app(&dir, "standalone", "alias-regex-reject")
            .err()
            .expect("regex alias must fail");
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
        let htpasswd_path = htpasswd
            .to_str()
            .expect("utf8 path")
            .replace('\\', "/");
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
        let config = load_server_toml_app(&dir, "standalone", "auth-basic-test")
            .expect("layout must load");
        let route = &config.virtual_hosts[0].routes[0];
        let auth = route.auth_basic.as_ref().expect("auth_basic");
        assert_eq!(auth.realm, "Restricted");
        assert_eq!(auth.users.len(), 1);
        assert_eq!(auth.users[0].username, "alice");
        assert_eq!(auth.users[0].password_hash, hash);
        std::fs::remove_dir_all(&dir).ok();
    }
}
