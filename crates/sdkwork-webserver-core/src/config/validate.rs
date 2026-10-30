use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr},
    path::{Component, Path},
};

use ipnet::IpNet;
use url::Url;

use super::{
    is_capture_variable, is_supported_upstream_allowed_cidr, parse_server_name,
    server_name_is_regex, template_variables, upstream_ip_is_allowed, AppDomainFallbackLookup,
    CachePolicyConfig, CertificateSource, ConfigDiagnostic, ErrorPageConfig, ExpiresMode,
    IfConditionConfig, IfOperatorConfig, ListenerProtocol, ResourceConfig, RewritePhaseOpConfig,
    RouteConfig, RoutePathType, SecurityHeadersConfig, ServerNameForm, StreamTargetConfig,
    TlsCertificateResolution, TlsVersion, UpstreamConfig, UpstreamLoadBalancingStrategy,
    UpstreamTlsTrustMode, UsageMeteringChannel, WebServerAppConfig, WebServerConfigError,
    WebServerLimits, MAX_REWRITE_PHASE_OPS, MAX_REWRITE_PHASE_VARIABLES,
};

const MAX_DIAGNOSTICS: usize = 128;
const MAX_TOTAL_ROUTES: usize = 10_000;
/// Regex source bound (PRD section 8.3 regex-complexity quota): the engine
/// itself is linear-time, so the remaining unbounded cost is compiling an
/// adversarially large pattern during activation. 1 KiB covers every real
/// location/rewrite pattern.
const MAX_REGEX_PATTERN_BYTES: usize = 1_024;

/// Bounds and confinement checks for the Wave 7D CGI-family response
/// buffering/store/rate directives. The nginx materializer applies the same
/// ranges while parsing; this pass also covers typed TOML/JSON resources that
/// do not pass through the nginx parser.
fn validate_upstream_buffering_store(
    validator: &mut SemanticValidator,
    path: &str,
    resource: &ResourceConfig,
    store_root: Option<&str>,
) {
    let (buffering, store, limit_rate, ignore_headers) = match resource {
        ResourceConfig::FastCgi {
            buffer_policy,
            store,
            limit_rate_bytes_per_second,
            ignore_headers,
            ..
        }
        | ResourceConfig::CgiUpstream {
            buffer_policy,
            store,
            limit_rate_bytes_per_second,
            ignore_headers,
            ..
        } => (
            *buffer_policy,
            store.as_ref(),
            *limit_rate_bytes_per_second,
            ignore_headers.as_slice(),
        ),
        ResourceConfig::Grpc { ignore_headers, .. } => {
            (None, None, None, ignore_headers.as_slice())
        }
        ResourceConfig::Memcached {
            buffer_bytes,
            gzip_flag,
            ..
        } => {
            if let Some(bytes) = buffer_bytes {
                if !(512..=1024 * 1024).contains(bytes) {
                    validator.push(
                        format!("{path}/bufferBytes"),
                        "memcached bufferBytes must be in 512..=1048576 bytes",
                    );
                }
            }
            if let Some(flag) = gzip_flag {
                if *flag == 0 {
                    validator.push(
                        format!("{path}/gzipFlag"),
                        "memcached gzipFlag 0 can never match a flag bit",
                    );
                }
            }
            return;
        }
        _ => return,
    };

    if let Some(policy) = buffering {
        if !(512..=1024 * 1024).contains(&policy.header_bytes) {
            validator.push(
                format!("{path}/bufferPolicy/headerBytes"),
                "buffer headerBytes must be in 512..=1048576 bytes",
            );
        }
        if !(512..=1024 * 1024).contains(&policy.chunk_bytes) {
            validator.push(
                format!("{path}/bufferPolicy/chunkBytes"),
                "buffer chunkBytes must be in 512..=1048576 bytes",
            );
        }
        if !(1..=64).contains(&policy.chunks) {
            validator.push(
                format!("{path}/bufferPolicy/chunks"),
                "buffer chunks must be in 1..=64",
            );
        }
        let total = policy.chunk_bytes.saturating_mul(u64::from(policy.chunks));
        if total > 16 * 1024 * 1024 {
            validator.push(
                format!("{path}/bufferPolicy/chunkBytes"),
                "buffer bank (chunks * chunkBytes) must not exceed 16 MiB",
            );
        }
        if !(512..=8 * 1024 * 1024).contains(&policy.busy_bytes) {
            validator.push(
                format!("{path}/bufferPolicy/busyBytes"),
                "busyBytes must be in 512..=8388608 bytes",
            );
        } else if policy.busy_bytes > total.saturating_mul(2) {
            // nginx allows a `busy_buffers_size` smaller than one body buffer
            // (the 1k `buffer_size` + default 4k `buffers` combination is
            // accepted by nginx 1.29.6), so only the upper relationship is a
            // validation error.
            validator.push(
                format!("{path}/bufferPolicy/busyBytes"),
                "busyBytes must not exceed two buffer banks (2 * chunks * chunkBytes)",
            );
        }
    }

    if let Some(store) = store {
        if !store.path.starts_with('/') {
            validator.push(
                format!("{path}/store/path"),
                "store path template must be absolute",
            );
        }
        if store
            .path
            .bytes()
            .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
        {
            validator.push(
                format!("{path}/store/path"),
                "store path template must not contain NUL or line breaks",
            );
        } else if store.path.split('/').any(|component| component == "..") {
            validator.push(
                format!("{path}/store/path"),
                "store path template must not contain `..` segments",
            );
        }
        if let Some(root) = store_root {
            if !root.starts_with('/') {
                validator.push("/nginx/storeRoot", "storeRoot must be an absolute path");
            } else {
                // The template's leading literal prefix must stay inside the
                // configured root; the per-request expansion re-checks the
                // final path at write time.
                let literal_prefix = store
                    .path
                    .split('$')
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('/');
                let root = root.trim_end_matches('/');
                if !(literal_prefix == root || literal_prefix.starts_with(&format!("{root}/"))) {
                    validator.push(
                        format!("{path}/store/path"),
                        format!(
                            "store path template must be a descendant of nginx.storeRoot `{root}`"
                        ),
                    );
                }
            }
        }
        if store.access_mode == 0 || store.access_mode > 0o7777 {
            validator.push(
                format!("{path}/store/accessMode"),
                "store accessMode must be a unix permission mode in 1..=4095",
            );
        }
    }

    if let Some(rate) = limit_rate {
        if rate > 1024 * 1024 * 1024 {
            validator.push(
                format!("{path}/limitRateBytesPerSecond"),
                "limit rate must not exceed 1 GiB/s",
            );
        }
    }

    for (index, field) in ignore_headers.iter().enumerate() {
        let recognized = [
            "x-accel-redirect",
            "x-accel-expires",
            "x-accel-limit-rate",
            "x-accel-buffering",
            "x-accel-charset",
            "expires",
            "cache-control",
            "set-cookie",
            "vary",
        ];
        if !recognized
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(field))
        {
            validator.push(
                format!("{path}/ignoreHeaders/{index}"),
                format!(
                    "`{field}` is not a header nginx's *_ignore_headers directive recognises (X-Accel-Redirect, X-Accel-Expires, X-Accel-Limit-Rate, X-Accel-Buffering, X-Accel-Charset, Expires, Cache-Control, Set-Cookie, Vary)"
                ),
            );
        }
    }
}

fn validate_regex_pattern(validator: &mut SemanticValidator, path: String, pattern: &str) {
    if pattern.len() > MAX_REGEX_PATTERN_BYTES {
        validator.push(
            path,
            format!(
                "regex pattern is {} bytes; maximum is {MAX_REGEX_PATTERN_BYTES}",
                pattern.len()
            ),
        );
        return;
    }
    if let Err(error) = regex::Regex::new(pattern) {
        validator.push(path, format!("invalid regex pattern: {error}"));
    }
}
const MAX_TOTAL_UPSTREAM_TARGETS: usize = 10_000;

pub(crate) fn validate_webserver_config(
    config: &WebServerAppConfig,
) -> Result<(), WebServerConfigError> {
    let mut validator = SemanticValidator::default();
    validator.validate(config);
    validator.finish()
}

#[derive(Default)]
struct SemanticValidator {
    diagnostics: Vec<ConfigDiagnostic>,
    ids: HashMap<String, String>,
    /// nginx `map` result variables (without `$`) declared by this config, so
    /// a response-header value may reference one exactly like nginx does.
    map_variables: Vec<String>,
}

impl SemanticValidator {
    fn validate(&mut self, config: &WebServerAppConfig) {
        if config.schema_version != 1 {
            self.push("/schemaVersion", "only schemaVersion 1 is supported");
        }
        if config.kind != "sdkwork.webserver.app" {
            self.push("/kind", "kind must be sdkwork.webserver.app");
        }
        if config.listeners.is_empty() && config.streams.is_empty() && config.mail.is_empty() {
            self.push(
                "",
                "at least one HTTP listener, one stream server, or one mail server is required",
            );
        }
        if !config.listeners.is_empty()
            && (config.resources.is_empty() || config.virtual_hosts.is_empty())
        {
            self.push(
                "",
                "HTTP listeners require at least one resource and one virtual host",
            );
        }
        if config.nginx.profile != "http-core-v1" {
            self.push(
                "/nginx/profile",
                "only the http-core-v1 compatibility profile is supported",
            );
        }
        if let Some(unix_socket_root) = &config.nginx.unix_socket_root {
            if !valid_unix_socket_root(unix_socket_root) {
                self.push(
                    "/nginx/unixSocketRoot",
                    "unixSocketRoot must be an absolute directory path without parent traversal, backslashes, NUL, or control characters",
                );
            }
        }
        for (index, prefix) in config.nginx.document_root_allowlist.iter().enumerate() {
            if !valid_document_root_allowlist_entry(prefix) {
                self.push(
                    format!("/nginx/documentRootAllowlist/{index}"),
                    "documentRootAllowlist entries must be absolute directory prefixes without `..`, a trailing slash, backslashes, NUL, or control characters",
                );
            }
        }
        if config.nginx.unknown_directive_policy != "error" {
            self.push(
                "/nginx/unknownDirectivePolicy",
                "Rust activation requires unknownDirectivePolicy=error",
            );
        }

        self.validate_maps(config);
        validate_mime_policy(
            self,
            "/mimeTypes",
            config.mime_types.as_deref(),
            config.default_type.as_deref(),
        );
        self.validate_limits(config);
        self.validate_resource_pressure(config);
        self.validate_limit_req_zones(config);
        self.validate_app_domain_fallback(config);
        self.validate_usage_metering(config);
        self.register_ids(config);

        let listeners = config
            .listeners
            .iter()
            .map(|listener| (listener.id.as_str(), listener))
            .collect::<HashMap<_, _>>();
        let certificates = config
            .certificates
            .iter()
            .map(|certificate| (certificate.id.as_str(), certificate))
            .collect::<HashMap<_, _>>();
        let tls_policies = config
            .tls_policies
            .iter()
            .map(|policy| (policy.id.as_str(), policy))
            .collect::<HashMap<_, _>>();
        let resources = config
            .resources
            .iter()
            .map(|resource| (resource.id(), resource))
            .collect::<HashMap<_, _>>();
        let upstreams = config
            .upstreams
            .iter()
            .map(|upstream| (upstream.id.as_str(), upstream))
            .collect::<HashMap<_, _>>();
        let resolvers = config
            .resolvers
            .iter()
            .map(|resolver| (resolver.id.as_str(), resolver))
            .collect::<HashMap<_, _>>();
        let virtual_hosts = config
            .virtual_hosts
            .iter()
            .map(|virtual_host| (virtual_host.id.as_str(), virtual_host))
            .collect::<HashMap<_, _>>();

        let mut sockets = HashSet::new();
        for (index, listener) in config.listeners.iter().enumerate() {
            let path = format!("/listeners/{index}");
            if listener.bind.parse::<IpAddr>().is_err() {
                self.push(
                    format!("{path}/bind"),
                    "bind must be an explicit IPv4 or IPv6 address",
                );
            }
            let socket_key = format!("{}:{}", listener.bind.to_ascii_lowercase(), listener.port);
            if !sockets.insert(socket_key.clone()) {
                self.push(&path, "another listener already owns this bind and port");
            }
            // Raw TCP stream listeners share the same bind:port namespace and
            // must not collide with HTTP listeners (SDKWORK_WEBSERVER_SPEC §12).
            if config
                .streams
                .iter()
                .any(|stream| stream.socket_key() == socket_key)
            {
                self.push(
                    &path,
                    "an http stream server already owns this bind and port",
                );
            }
            // The mail proxy plane binds real TCP sockets on the same host, so
            // its listeners share the HTTP/stream bind:port namespace.
            if config
                .mail
                .servers
                .iter()
                .any(|mail| mail.socket_key() == socket_key)
            {
                self.push(&path, "a mail server already owns this bind and port");
            }
            if listener.protocols.contains(&ListenerProtocol::Http2)
                && listener.tls_policy_ref.is_none()
                && listener.tls_runtime.is_none()
            {
                self.push(
                    format!("{path}/protocols"),
                    "HTTP/2 requires TLS in the foundation profile; public h2c is unsupported",
                );
            }
            if let Ok(bind) = listener.bind.parse::<IpAddr>() {
                let has_tls = listener.tls_policy_ref.is_some() || listener.tls_runtime.is_some();
                if !bind.is_loopback()
                    && !has_tls
                    && listener.acme_http_01.is_none()
                    && !listener.allow_plaintext_http
                {
                    self.push(
                        format!("{path}/bind"),
                        "public non-loopback listener without TLS requires allowPlaintextHttp, acmeHttp01, or a TLS policy; plaintext HTTP is fail-closed by default",
                    );
                }
            }
            if listener.tls_policy_ref.is_some()
                && listener.tls_runtime.is_some()
                && listener.tls_certificate_resolution
                    != Some(TlsCertificateResolution::PolicyFirst)
            {
                self.push(
                    format!("{path}/tlsRuntime"),
                    "tlsRuntime cannot be combined with tlsPolicyRef",
                );
            }
            if let Some(resolution) = listener.tls_certificate_resolution {
                if !resolution.serves_policy_files() && listener.tls_policy_ref.is_some() {
                    self.push(
                        format!("{path}/tlsCertificateResolution"),
                        "declared resolution does not serve tlsPolicyRef certificate files",
                    );
                }
                if !resolution.serves_assignments() && listener.tls_runtime.is_some() {
                    self.push(
                        format!("{path}/tlsCertificateResolution"),
                        "declared resolution does not serve tlsRuntime assignments",
                    );
                }
                if listener.tls_policy_ref.is_none() && listener.tls_runtime.is_none() {
                    self.push(
                        format!("{path}/tlsCertificateResolution"),
                        "tlsCertificateResolution requires tlsPolicyRef or tlsRuntime",
                    );
                }
                if resolution == TlsCertificateResolution::PolicyFirst
                    && (listener.tls_policy_ref.is_none() || listener.tls_runtime.is_none())
                {
                    self.push(
                        format!("{path}/tlsCertificateResolution"),
                        "policy-first requires both tlsPolicyRef and tlsRuntime",
                    );
                }
            }
            if let Some(policy_ref) = &listener.tls_policy_ref {
                if !tls_policies.contains_key(policy_ref.as_str()) {
                    self.push(
                        format!("{path}/tlsPolicyRef"),
                        format!("unknown TLS policy {policy_ref}"),
                    );
                } else if let Some(policy) = tls_policies.get(policy_ref.as_str()) {
                    let expected_alpn = if listener.protocols == [ListenerProtocol::Http1] {
                        ["http/1.1"].as_slice()
                    } else if listener.protocols == [ListenerProtocol::Http2] {
                        ["h2"].as_slice()
                    } else {
                        ["h2", "http/1.1"].as_slice()
                    };
                    if policy.alpn.iter().map(String::as_str).collect::<Vec<_>>() != expected_alpn {
                        self.push(
                            format!("{path}/tlsPolicyRef"),
                            "TLS policy ALPN must exactly match the listener protocols",
                        );
                    }
                }
            }
            if listener
                .max_connections
                .is_some_and(|maximum| maximum > config.limits.max_connections)
            {
                self.push(
                    format!("{path}/maxConnections"),
                    "listener maxConnections cannot exceed the application maximum",
                );
            }
            if let Some(policy) = &listener.trusted_proxy {
                if policy.trusted_cidrs.is_empty() {
                    self.push(
                        format!("{path}/trustedProxy/trustedCidrs"),
                        "trustedCidrs must contain at least one trusted proxy network",
                    );
                }
                if policy.trusted_cidrs.len() > 64 {
                    self.push(
                        format!("{path}/trustedProxy/trustedCidrs"),
                        "trustedCidrs cannot contain more than 64 networks",
                    );
                }
                let mut trusted_cidrs = HashSet::new();
                for (cidr_index, cidr) in policy.trusted_cidrs.iter().enumerate() {
                    if !trusted_cidrs.insert(cidr) {
                        self.push(
                            format!("{path}/trustedProxy/trustedCidrs/{cidr_index}"),
                            "trusted proxy CIDR is duplicated",
                        );
                    }
                    if trusted_proxy_network_is_overbroad(cidr) {
                        self.push(
                            format!("{path}/trustedProxy/trustedCidrs/{cidr_index}"),
                            "trusted proxy CIDR is broader than the minimum IPv4 /8 or IPv6 /16 boundary",
                        );
                    }
                }
                if !(1..=64).contains(&policy.max_hops) {
                    self.push(
                        format!("{path}/trustedProxy/maxHops"),
                        "maxHops must be between 1 and 64",
                    );
                }
                if !(64..=65_536).contains(&policy.max_header_bytes) {
                    self.push(
                        format!("{path}/trustedProxy/maxHeaderBytes"),
                        "maxHeaderBytes must be between 64 bytes and 64 KiB",
                    );
                }
                if policy.max_header_bytes > config.limits.max_header_value_bytes {
                    self.push(
                        format!("{path}/trustedProxy/maxHeaderBytes"),
                        "maxHeaderBytes cannot exceed limits.maxHeaderValueBytes",
                    );
                }
            }
            if let Some(policy) = &listener.proxy_protocol {
                if listener.trusted_proxy.is_some() {
                    self.push(
                        format!("{path}/proxyProtocol"),
                        "proxyProtocol and trustedProxy are mutually exclusive identity authorities",
                    );
                }
                if policy.trusted_source_cidrs.is_empty() {
                    self.push(
                        format!("{path}/proxyProtocol/trustedSourceCidrs"),
                        "trustedSourceCidrs must contain at least one proxy network",
                    );
                }
                if policy.trusted_source_cidrs.len() > 64 {
                    self.push(
                        format!("{path}/proxyProtocol/trustedSourceCidrs"),
                        "trustedSourceCidrs cannot contain more than 64 networks",
                    );
                }
                let mut trusted_sources = HashSet::new();
                for (cidr_index, cidr) in policy.trusted_source_cidrs.iter().enumerate() {
                    if !trusted_sources.insert(cidr) {
                        self.push(
                            format!("{path}/proxyProtocol/trustedSourceCidrs/{cidr_index}"),
                            "PROXY protocol trusted source CIDR is duplicated",
                        );
                    }
                    if trusted_proxy_network_is_overbroad(cidr) {
                        self.push(
                            format!("{path}/proxyProtocol/trustedSourceCidrs/{cidr_index}"),
                            "PROXY protocol trusted source CIDR is broader than the minimum IPv4 /8 or IPv6 /16 boundary",
                        );
                    }
                }
                if policy.versions.is_empty() || policy.versions.len() > 2 {
                    self.push(
                        format!("{path}/proxyProtocol/versions"),
                        "versions must contain one or both supported versions",
                    );
                }
                let mut versions = HashSet::new();
                for (version_index, version) in policy.versions.iter().enumerate() {
                    if !versions.insert(version) {
                        self.push(
                            format!("{path}/proxyProtocol/versions/{version_index}"),
                            "PROXY protocol version is duplicated",
                        );
                    }
                }
                if !(100..=10_000).contains(&policy.timeout_ms) {
                    self.push(
                        format!("{path}/proxyProtocol/timeoutMs"),
                        "timeoutMs must be between 100 and 10,000 milliseconds",
                    );
                }
                if !(107..=4_096).contains(&policy.max_header_bytes) {
                    self.push(
                        format!("{path}/proxyProtocol/maxHeaderBytes"),
                        "maxHeaderBytes must be between 107 and 4,096 bytes",
                    );
                }
                if policy.crc32c_policy != super::ProxyProtocolCrc32cPolicy::Ignore
                    && !policy.versions.contains(&super::ProxyProtocolVersion::V2)
                {
                    self.push(
                        format!("{path}/proxyProtocol/crc32cPolicy"),
                        "crc32cPolicy requires v2 to be enabled",
                    );
                }
            }
            if let Some(default_ref) = &listener.default_virtual_host_ref {
                match virtual_hosts.get(default_ref.as_str()) {
                    Some(virtual_host)
                        if virtual_host
                            .listener_refs
                            .iter()
                            .any(|listener_ref| listener_ref == &listener.id) => {}
                    Some(_) => self.push(
                        format!("{path}/defaultVirtualHostRef"),
                        "default virtual host does not reference this listener",
                    ),
                    None => self.push(
                        format!("{path}/defaultVirtualHostRef"),
                        format!("unknown virtual host {default_ref}"),
                    ),
                }
            }
            if let Some(acme) = &listener.acme_http_01 {
                let webroot_path = format!("{path}/acmeHttp01/webroot");
                if acme.webroot.is_empty() || acme.webroot.len() > 4_096 {
                    self.push(&webroot_path, "webroot must contain 1..4096 bytes");
                } else if acme
                    .webroot
                    .bytes()
                    .any(|byte| byte == 0 || byte.is_ascii_control())
                {
                    self.push(
                        &webroot_path,
                        "webroot must not contain NUL or control bytes",
                    );
                }
            }
        }

        self.validate_streams(config, &upstreams);
        self.validate_mail(config);

        for (index, certificate) in config.certificates.iter().enumerate() {
            let path = format!("/certificates/{index}");
            let mut normalized_names = HashSet::new();
            for (name_index, server_name) in certificate.server_names.iter().enumerate() {
                let Some(normalized) = normalize_tls_server_name(server_name) else {
                    // A name `normalize_server_name` accepts but this rejects can
                    // only be an IP literal; saying so is more useful than calling
                    // it malformed.
                    let message = if normalize_server_name(server_name).is_some() {
                        format!(
                            "certificate server name {server_name} must be a DNS name, not IP address"
                        )
                    } else {
                        "invalid exact or leading-wildcard DNS server name".to_owned()
                    };
                    self.push(format!("{path}/serverNames/{name_index}"), message);
                    continue;
                };
                if !normalized_names.insert(normalized) {
                    self.push(
                        format!("{path}/serverNames/{name_index}"),
                        "duplicate certificate server name after DNS normalization",
                    );
                }
            }
            for (regex_index, pattern) in certificate.server_name_regexes.iter().enumerate() {
                let compiled = if pattern.bytes().any(|byte| byte.is_ascii_uppercase()) {
                    format!("(?i){pattern}")
                } else {
                    pattern.clone()
                };
                validate_regex_pattern(
                    self,
                    format!("{path}/serverNameRegexes/{regex_index}"),
                    &compiled,
                );
            }
            let CertificateSource::ProtectedFile {
                certificate_file,
                private_key_file,
                ..
            } = &certificate.source;
            if certificate_file == private_key_file {
                self.push(
                    format!("{path}/source"),
                    "certificateFile and privateKeyFile must be different files",
                );
            }
        }

        for (index, policy) in config.tls_policies.iter().enumerate() {
            let path = format!("/tlsPolicies/{index}");
            if policy.certificate_ref.is_some() != policy.certificate_refs.is_empty() {
                self.push(
                    &path,
                    "exactly one of certificateRef or certificateRefs must be configured",
                );
            }
            let mut referenced_certificates = HashSet::new();
            let mut server_name_owners = HashMap::new();
            for (reference_path, certificate_ref) in policy
                .certificate_ref
                .iter()
                .map(|certificate_ref| (format!("{path}/certificateRef"), certificate_ref))
                .chain(policy.certificate_refs.iter().enumerate().map(
                    |(reference_index, certificate_ref)| {
                        (
                            format!("{path}/certificateRefs/{reference_index}"),
                            certificate_ref,
                        )
                    },
                ))
            {
                if !referenced_certificates.insert(certificate_ref.as_str()) {
                    self.push(reference_path, "duplicate certificate reference");
                    continue;
                }
                let Some(certificate) = certificates.get(certificate_ref.as_str()) else {
                    self.push(
                        reference_path,
                        format!("unknown certificate {certificate_ref}"),
                    );
                    continue;
                };
                for server_name in &certificate.server_names {
                    let Some(normalized) = normalize_server_name(server_name) else {
                        continue;
                    };
                    if let Some(previous_certificate) =
                        server_name_owners.insert(normalized.clone(), certificate.id.as_str())
                    {
                        if previous_certificate != certificate.id {
                            self.push(
                                &path,
                                format!(
                                    "certificates {previous_certificate} and {} both declare server name {normalized}",
                                    certificate.id
                                ),
                            );
                        }
                    }
                }
            }
            if policy.minimum_version > policy.maximum_version {
                self.push(
                    &path,
                    "minimumVersion must not be greater than maximumVersion",
                );
            }
            if policy.minimum_version < TlsVersion::PLATFORM_FLOOR {
                self.push(&path, "TLS versions below 1.2 are forbidden");
            }
            // `PRD-https-and-certificates.md` §2: "An application may select an
            // approved policy or a stricter compatible policy, but it cannot
            // silently weaken the platform minimum." §3 requires every policy to
            // carry both a minimum and a maximum version. So any window that
            // stays inside [floor, ceiling] with minimum <= maximum is legal —
            // including the singletons `TLS1.2..TLS1.2` and `TLS1.3..TLS1.3`
            // that a narrower operator policy produces. `TlsVersion` cannot
            // currently express anything outside that span, so this guard only
            // fires if a future variant widens the enum past the platform floor
            // or ceiling without updating them.
            if policy.minimum_version < TlsVersion::PLATFORM_FLOOR
                || policy.maximum_version > TlsVersion::PLATFORM_CEILING
            {
                self.push(
                    &path,
                    "TLS versions outside the supported TLS 1.2 through TLS 1.3 span are forbidden",
                );
            }
        }

        for (index, resolver) in config.resolvers.iter().enumerate() {
            if !resolver.servers.is_empty() {
                self.push(
                    format!("/resolvers/{index}/servers"),
                    "custom DNS server transport is not implemented; omit servers to use the bounded system resolver",
                );
            }
        }

        for (index, resource) in config.resources.iter().enumerate() {
            let path = format!("/resources/{index}");
            // Unix-socket upstream targets are executable only by the CGI
            // family/memcached adapters, and a source-address bind has no
            // meaning on a unix socket.
            let referenced_upstream = match resource {
                ResourceConfig::Proxy {
                    upstream_ref,
                    dynamic_target,
                    ..
                } if dynamic_target.is_none() => Some(upstream_ref.as_str()),
                ResourceConfig::FastCgi { upstream_ref, .. }
                | ResourceConfig::CgiUpstream { upstream_ref, .. }
                | ResourceConfig::Memcached { upstream_ref, .. }
                | ResourceConfig::Grpc { upstream_ref, .. } => Some(upstream_ref.as_str()),
                _ => None,
            };
            if let Some(upstream) = referenced_upstream.and_then(|id| upstreams.get(id)) {
                let unix = upstream_has_unix_target(upstream);
                let cgi_family = matches!(
                    resource,
                    ResourceConfig::FastCgi { .. }
                        | ResourceConfig::CgiUpstream { .. }
                        | ResourceConfig::Memcached { .. }
                );
                if unix && !cgi_family {
                    self.push(
                        format!("{path}/upstreamRef"),
                        "upstream has unix-socket targets; only fastcgi/uwsgi/scgi/memcached resources can execute them",
                    );
                }
                if unix {
                    let bind_address = match resource {
                        ResourceConfig::FastCgi { bind_address, .. }
                        | ResourceConfig::CgiUpstream { bind_address, .. }
                        | ResourceConfig::Memcached { bind_address, .. } => *bind_address,
                        _ => None,
                    };
                    if bind_address.is_some() {
                        self.push(
                            format!("{path}/bindAddress"),
                            "a source-address bind does not apply to a unix-socket upstream target",
                        );
                    }
                }
            }
            validate_upstream_buffering_store(
                self,
                &path,
                resource,
                config.nginx.store_root.as_deref(),
            );
            match resource {
                ResourceConfig::Static {
                    root,
                    index_files,
                    spa_fallback,
                    follow_symlinks,
                    dynamic_root,
                    ..
                } => {
                    // Absolute roots (nginx `root` compatibility) are used
                    // directly; relative roots stay chrooted. A request-time
                    // `root $variable` placeholder is validated per referencing
                    // route, where the rewrite-phase variable scope is known.
                    if dynamic_root.is_none() && !root.starts_with('/') {
                        validate_relative_path(self, &format!("{path}/root"), root, true);
                    }
                    if let Some(template) = dynamic_root {
                        if let Err(message) = template_variables(template) {
                            self.push(format!("{path}/dynamicRoot"), message);
                        }
                    }
                    for (file_index, file) in index_files.iter().enumerate() {
                        validate_relative_path(
                            self,
                            &format!("{path}/indexFiles/{file_index}"),
                            file,
                            false,
                        );
                    }
                    if index_files.is_empty() {
                        self.push(
                            format!("{path}/indexFiles"),
                            "indexFiles must declare at least one file name",
                        );
                    }
                    if let Some(file) = spa_fallback {
                        // nginx `try_files` fallbacks are internal redirect
                        // targets and may be nested (`/app/index.html`); the
                        // data plane re-sanitizes every component under the
                        // chrooted root, so nested fallbacks are safe.
                        validate_relative_path(self, &format!("{path}/spaFallback"), file, true);
                    }
                    if *follow_symlinks {
                        self.push(
                            format!("{path}/followSymlinks"),
                            "following symlinks is forbidden by the foundation profile",
                        );
                    }
                }
                ResourceConfig::Proxy {
                    upstream_ref,
                    dynamic_target,
                    ..
                } => {
                    // Variable `proxy_pass` resolves its target per request
                    // and does not reference a declared upstream.
                    if dynamic_target.is_none() && !upstreams.contains_key(upstream_ref.as_str()) {
                        self.push(
                            format!("{path}/upstreamRef"),
                            format!("unknown upstream {upstream_ref}"),
                        );
                    }
                }
                ResourceConfig::FastCgi {
                    upstream_ref,
                    params,
                    index,
                    split_path_info,
                    document_root,
                    connect_timeout_ms,
                    send_timeout_ms,
                    read_timeout_ms,
                    response_policy,
                    retry,
                    cache,
                    ..
                } => {
                    if !upstreams.contains_key(upstream_ref.as_str()) {
                        self.push(
                            format!("{path}/upstreamRef"),
                            format!("unknown upstream {upstream_ref}"),
                        );
                    }
                    if let Some(cache) = cache {
                        validate_cgi_cache_config(
                            self,
                            &format!("{path}/cache"),
                            cache,
                            config.proxy_cache.enabled,
                            &config.cache_zone_names,
                        );
                    }
                    if let Some(root) = document_root {
                        if !root.starts_with('/') {
                            self.push(
                                format!("{path}/documentRoot"),
                                "FastCGI document_root must be an absolute path",
                            );
                        }
                    }
                    if let Some(index) = index {
                        validate_relative_path(self, &format!("{path}/index"), index, false);
                    }
                    if let Some(pattern) = split_path_info {
                        match regex::Regex::new(pattern) {
                            Ok(regex) if regex.captures_len() == 3 => {}
                            Ok(_) => self.push(
                                format!("{path}/splitPathInfo"),
                                "fastcgi_split_path_info must contain exactly two capture groups",
                            ),
                            Err(_) => self.push(
                                format!("{path}/splitPathInfo"),
                                "fastcgi_split_path_info is not a valid regular expression",
                            ),
                        }
                    }
                    for (param_index, param) in params.iter().enumerate() {
                        let param_path = format!("{path}/params/{param_index}");
                        if let Err(message) =
                            super::upstream_params::validate_param_name(&param.name)
                        {
                            self.push(format!("{param_path}/name"), message);
                        }
                        let map_variables: Vec<&str> =
                            self.map_variables.iter().map(String::as_str).collect();
                        if let Err(message) = super::upstream_params::validate_param_value(
                            &param.value,
                            &map_variables,
                        ) {
                            self.push(format!("{param_path}/value"), message);
                        }
                    }
                    for (field, value) in [
                        ("connectTimeoutMs", connect_timeout_ms),
                        ("sendTimeoutMs", send_timeout_ms),
                        ("readTimeoutMs", read_timeout_ms),
                    ] {
                        if *value == 0 || *value > 86_400_000 {
                            self.push(
                                format!("{path}/{field}"),
                                "FastCGI timeout must be in 1..=86400000 ms",
                            );
                        }
                    }
                    if let Some(policy) = response_policy {
                        for (name_index, name) in policy.hide_headers.iter().enumerate() {
                            if !super::header_block_token(name) {
                                self.push(
                                    format!("{path}/responsePolicy/hideHeaders/{name_index}"),
                                    "hideHeaders entry is not a valid HTTP field name",
                                );
                            }
                        }
                        for (name_index, name) in policy.pass_headers.iter().enumerate() {
                            if !super::header_block_token(name) {
                                self.push(
                                    format!("{path}/responsePolicy/passHeaders/{name_index}"),
                                    "passHeaders entry is not a valid HTTP field name",
                                );
                            }
                        }
                    }
                    if let Some(retry) = retry {
                        if retry.max_attempts == 0 {
                            self.push(
                                format!("{path}/retry/maxAttempts"),
                                "retry maxAttempts must be at least 1",
                            );
                        }
                        if retry.timeout_ms == 0 || retry.timeout_ms > 86_400_000 {
                            self.push(
                                format!("{path}/retry/timeoutMs"),
                                "retry timeoutMs must be in 1..=86400000 ms",
                            );
                        }
                    }
                }
                ResourceConfig::Grpc {
                    upstream_ref,
                    request_set_headers,
                    connect_timeout_ms,
                    send_timeout_ms,
                    read_timeout_ms,
                    buffer_size,
                    response_policy,
                    retry,
                    ..
                } => {
                    if !upstreams.contains_key(upstream_ref.as_str()) {
                        self.push(
                            format!("{path}/upstreamRef"),
                            format!("unknown upstream {upstream_ref}"),
                        );
                    }
                    let map_variable_names = self.map_variables.clone();
                    let map_variables: Vec<&str> =
                        map_variable_names.iter().map(String::as_str).collect();
                    for (entry_index, entry) in request_set_headers.iter().enumerate() {
                        if let Err(message) =
                            super::proxy_headers::validate_proxy_set_header_entry_with_maps(
                                entry,
                                &map_variables,
                            )
                        {
                            self.push(format!("{path}/requestSetHeaders/{entry_index}"), message);
                        }
                    }
                    for (field, value) in [
                        ("connectTimeoutMs", connect_timeout_ms),
                        ("sendTimeoutMs", send_timeout_ms),
                        ("readTimeoutMs", read_timeout_ms),
                    ] {
                        if *value == 0 || *value > 86_400_000 {
                            self.push(
                                format!("{path}/{field}"),
                                "gRPC timeout must be in 1..=86400000 ms",
                            );
                        }
                    }
                    if *buffer_size == 0 || *buffer_size > 1_048_576 {
                        self.push(
                            format!("{path}/bufferSize"),
                            "gRPC bufferSize must be in 1..=1048576 bytes",
                        );
                    }
                    if let Some(policy) = response_policy {
                        for (name_index, name) in policy.hide_headers.iter().enumerate() {
                            if !super::header_block_token(name) {
                                self.push(
                                    format!("{path}/responsePolicy/hideHeaders/{name_index}"),
                                    "hideHeaders entry is not a valid HTTP field name",
                                );
                            }
                        }
                        for (name_index, name) in policy.pass_headers.iter().enumerate() {
                            if !super::header_block_token(name) {
                                self.push(
                                    format!("{path}/responsePolicy/passHeaders/{name_index}"),
                                    "passHeaders entry is not a valid HTTP field name",
                                );
                            }
                        }
                    }
                    if let Some(retry) = retry {
                        if retry.max_attempts == 0 {
                            self.push(
                                format!("{path}/retry/maxAttempts"),
                                "retry maxAttempts must be at least 1",
                            );
                        }
                        if retry.timeout_ms == 0 || retry.timeout_ms > 86_400_000 {
                            self.push(
                                format!("{path}/retry/timeoutMs"),
                                "retry timeoutMs must be in 1..=86400000 ms",
                            );
                        }
                    }
                }
                ResourceConfig::CgiUpstream {
                    upstream_ref,
                    params,
                    document_root,
                    connect_timeout_ms,
                    send_timeout_ms,
                    read_timeout_ms,
                    response_policy,
                    retry,
                    extra_string,
                    cache,
                    ..
                } => {
                    if !upstreams.contains_key(upstream_ref.as_str()) {
                        self.push(
                            format!("{path}/upstreamRef"),
                            format!("unknown upstream {upstream_ref}"),
                        );
                    }
                    if let Some(cache) = cache {
                        validate_cgi_cache_config(
                            self,
                            &format!("{path}/cache"),
                            cache,
                            config.proxy_cache.enabled,
                            &config.cache_zone_names,
                        );
                    }
                    if let Some(root) = document_root {
                        if !root.starts_with('/') {
                            self.push(
                                format!("{path}/documentRoot"),
                                "CGI-family document_root must be an absolute path",
                            );
                        }
                    }
                    for (param_index, param) in params.iter().enumerate() {
                        let param_path = format!("{path}/params/{param_index}");
                        if let Err(message) =
                            super::upstream_params::validate_param_name(&param.name)
                        {
                            self.push(format!("{param_path}/name"), message);
                        }
                        let map_variables: Vec<&str> =
                            self.map_variables.iter().map(String::as_str).collect();
                        if let Err(message) = super::upstream_params::validate_param_value(
                            &param.value,
                            &map_variables,
                        ) {
                            self.push(format!("{param_path}/value"), message);
                        }
                    }
                    for (field, value) in [
                        ("connectTimeoutMs", connect_timeout_ms),
                        ("sendTimeoutMs", send_timeout_ms),
                        ("readTimeoutMs", read_timeout_ms),
                    ] {
                        if *value == 0 || *value > 86_400_000 {
                            self.push(
                                format!("{path}/{field}"),
                                "CGI-family timeout must be in 1..=86400000 ms",
                            );
                        }
                    }
                    if let Some(policy) = response_policy {
                        for (name_index, name) in policy.hide_headers.iter().enumerate() {
                            if !super::header_block_token(name) {
                                self.push(
                                    format!("{path}/responsePolicy/hideHeaders/{name_index}"),
                                    "hideHeaders entry is not a valid HTTP field name",
                                );
                            }
                        }
                        for (name_index, name) in policy.pass_headers.iter().enumerate() {
                            if !super::header_block_token(name) {
                                self.push(
                                    format!("{path}/responsePolicy/passHeaders/{name_index}"),
                                    "passHeaders entry is not a valid HTTP field name",
                                );
                            }
                        }
                    }
                    if let Some(extra) = extra_string {
                        if extra
                            .bytes()
                            .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
                        {
                            self.push(
                                format!("{path}/extraString"),
                                "uwsgi_string must not contain NUL or line breaks",
                            );
                        }
                    }
                    if let Some(retry) = retry {
                        if retry.max_attempts == 0 {
                            self.push(
                                format!("{path}/retry/maxAttempts"),
                                "retry maxAttempts must be at least 1",
                            );
                        }
                        if retry.timeout_ms == 0 || retry.timeout_ms > 86_400_000 {
                            self.push(
                                format!("{path}/retry/timeoutMs"),
                                "retry timeoutMs must be in 1..=86400000 ms",
                            );
                        }
                    }
                }
                ResourceConfig::Memcached {
                    upstream_ref,
                    key_template,
                    connect_timeout_ms,
                    send_timeout_ms,
                    read_timeout_ms,
                    retry,
                    ..
                } => {
                    if !upstreams.contains_key(upstream_ref.as_str()) {
                        self.push(
                            format!("{path}/upstreamRef"),
                            format!("unknown upstream {upstream_ref}"),
                        );
                    }
                    let map_variables: Vec<&str> =
                        self.map_variables.iter().map(String::as_str).collect();
                    if let Err(message) =
                        super::upstream_params::validate_param_value(key_template, &map_variables)
                    {
                        self.push(format!("{path}/keyTemplate"), message);
                    }
                    for (field, value) in [
                        ("connectTimeoutMs", connect_timeout_ms),
                        ("sendTimeoutMs", send_timeout_ms),
                        ("readTimeoutMs", read_timeout_ms),
                    ] {
                        if *value == 0 || *value > 86_400_000 {
                            self.push(
                                format!("{path}/{field}"),
                                "memcached timeout must be in 1..=86400000 ms",
                            );
                        }
                    }
                    if let Some(retry) = retry {
                        if retry.max_attempts == 0 {
                            self.push(
                                format!("{path}/retry/maxAttempts"),
                                "retry maxAttempts must be at least 1",
                            );
                        }
                        if retry.timeout_ms == 0 || retry.timeout_ms > 86_400_000 {
                            self.push(
                                format!("{path}/retry/timeoutMs"),
                                "retry timeoutMs must be in 1..=86400000 ms",
                            );
                        }
                    }
                }
                ResourceConfig::EmptyGif { .. } | ResourceConfig::StubStatus { .. } => {}
                ResourceConfig::Redirect {
                    status, location, ..
                } => {
                    if !matches!(status, 301 | 302 | 307 | 308) {
                        self.push(format!("{path}/status"), "unsupported redirect status");
                    }
                    validate_redirect_location(self, &format!("{path}/location"), location);
                }
                ResourceConfig::Respond { status, body, .. } => {
                    if !(200..=599).contains(status) {
                        self.push(format!("{path}/status"), "invalid HTTP response status");
                    }
                    if matches!(status, 204 | 205 | 304) && !body.is_empty() {
                        self.push(
                            format!("{path}/body"),
                            "HTTP 204, 205, and 304 fixed responses cannot contain a body",
                        );
                    }
                }
                ResourceConfig::Drive {
                    provider_resource_uuid,
                    resource_subpath,
                    index_files,
                    spa_fallback,
                    ..
                } => {
                    validate_provider_resource_uuid(
                        self,
                        &format!("{path}/providerResourceUuid"),
                        provider_resource_uuid,
                    );
                    if let Some(subpath) = resource_subpath {
                        validate_provider_subpath(
                            self,
                            &format!("{path}/resourceSubpath"),
                            subpath,
                        );
                    }
                    for (file_index, file) in index_files.iter().enumerate() {
                        validate_relative_path(
                            self,
                            &format!("{path}/indexFiles/{file_index}"),
                            file,
                            false,
                        );
                    }
                    if index_files.as_slice() != ["index.html"] {
                        self.push(
                            format!("{path}/indexFiles"),
                            "REQ-2026-0003 currently supports indexFiles=[\"index.html\"] only",
                        );
                    }
                    if let Some(file) = spa_fallback {
                        // nginx `try_files` fallbacks are internal redirect
                        // targets and may be nested (`/app/index.html`); the
                        // data plane re-sanitizes every component under the
                        // chrooted root, so nested fallbacks are safe.
                        validate_relative_path(self, &format!("{path}/spaFallback"), file, true);
                    }
                }
                ResourceConfig::Knowledgebase {
                    provider_resource_uuid,
                    locale,
                    ..
                } => {
                    validate_provider_resource_uuid(
                        self,
                        &format!("{path}/providerResourceUuid"),
                        provider_resource_uuid,
                    );
                    if let Some(locale) = locale {
                        if locale.is_empty()
                            || locale.len() > 35
                            || !locale.chars().all(|character| {
                                character.is_ascii_alphanumeric() || character == '-'
                            })
                        {
                            self.push(
                                format!("{path}/locale"),
                                "locale must be 1..=35 ASCII letters, digits, or '-'",
                            );
                        }
                    }
                }
            }
        }

        let mut total_targets = 0usize;
        // Upstream ids that own at least one `unix:` target; only the CGI
        // family and memcached adapters can execute those.
        let mut unix_upstreams: HashSet<&str> = HashSet::new();
        for (index, upstream) in config.upstreams.iter().enumerate() {
            let path = format!("/upstreams/{index}");
            if !upstream.targets.iter().any(|target| !target.backup) {
                self.push(
                    format!("{path}/targets"),
                    "upstream must contain at least one non-backup primary target",
                );
            }
            if upstream.load_balancing == UpstreamLoadBalancingStrategy::IpHash
                && upstream
                    .targets
                    .iter()
                    .any(|target| target.slow_start_ms.is_some())
            {
                self.push(
                    format!("{path}/loadBalancing"),
                    "ip-hash cannot be combined with target slowStartMs",
                );
            }
            if upstream.max_idle_connections > upstream.max_connections {
                self.push(
                    format!("{path}/maxIdleConnections"),
                    "maxIdleConnections must not exceed maxConnections",
                );
            }
            if let Some(retry) = &upstream.retry {
                if usize::from(retry.max_attempts) > upstream.targets.len() {
                    self.push(
                        format!("{path}/retry/maxAttempts"),
                        "retry maxAttempts must not exceed the number of upstream targets",
                    );
                }
                let maximum_useful_timeout = upstream
                    .request_timeout_ms
                    .saturating_mul(u64::from(retry.max_attempts));
                if retry.timeout_ms > maximum_useful_timeout {
                    self.push(
                        format!("{path}/retry/timeoutMs"),
                        "retry timeoutMs must not exceed requestTimeoutMs multiplied by maxAttempts",
                    );
                }
            }
            if let Some(resolver_ref) = &upstream.resolver_ref {
                if !resolvers.contains_key(resolver_ref.as_str()) {
                    self.push(
                        format!("{path}/resolverRef"),
                        format!("unknown resolver {resolver_ref}"),
                    );
                }
            }
            for (cidr_index, cidr) in upstream.address_policy.allowed_cidrs.iter().enumerate() {
                if !is_supported_upstream_allowed_cidr(cidr) {
                    self.push(
                        format!("{path}/addressPolicy/allowedCidrs/{cidr_index}"),
                        "allowed CIDR must be wholly contained in an approved loopback, private, shared, link-local, or ULA range",
                    );
                }
            }
            total_targets = total_targets.saturating_add(upstream.targets.len());
            let mut target_urls = HashSet::new();
            let mut target_authorities = HashSet::new();
            let has_target_connection_limits = upstream
                .targets
                .iter()
                .any(|target| target.max_connections.is_some());
            let mut has_plaintext_target = false;
            let mut has_unix_target = false;
            for (target_index, target) in upstream.targets.iter().enumerate() {
                let target_path = format!("{path}/targets/{target_index}/url");
                if target
                    .max_connections
                    .is_some_and(|maximum| maximum > upstream.max_connections)
                {
                    self.push(
                        format!("{path}/targets/{target_index}/maxConnections"),
                        "target maxConnections cannot exceed upstream maxConnections",
                    );
                }
                if let Some(unix_path) = unix_target_path(&target.url) {
                    // Unix-socket targets are typed-plane only and confined to
                    // the operator-configured `nginx.unixSocketRoot`.
                    has_unix_target = true;
                    match &config.nginx.unix_socket_root {
                        None => self.push(
                            &target_path,
                            "unix upstream sockets require `nginx.unixSocketRoot` in the typed configuration (nginx-conf `*_pass unix:` cannot state the confinement root and fails closed)",
                        ),
                        Some(root) => {
                            if !unix_path_is_confined(root, &unix_path) {
                                self.push(
                                    &target_path,
                                    format!(
                                        "unix upstream socket `{unix_path}` must be a strict descendant of nginx.unixSocketRoot `{root}` without parent traversal"
                                    ),
                                );
                            }
                        }
                    }
                    let normalized = format!("unix:{unix_path}");
                    if !target_urls.insert(normalized) {
                        self.push(&target_path, "duplicate upstream target URL");
                    }
                    if upstream.tls.is_some() {
                        self.push(
                            &target_path,
                            "unix upstream sockets cannot use an upstream TLS policy",
                        );
                    }
                    continue;
                }
                match Url::parse(&target.url) {
                    Ok(url)
                        if matches!(url.scheme(), "http" | "https")
                            && url.host_str().is_some()
                            && url.username().is_empty()
                            && url.password().is_none()
                            && url.query().is_none()
                            && url.fragment().is_none() =>
                    {
                        has_plaintext_target |= url.scheme() == "http";
                        // nginx allows the same server address to appear more
                        // than once (it weights the peer); each entry keeps its
                        // own target index, so duplicates are not an error.
                        let normalized = url.as_str().trim_end_matches('/').to_ascii_lowercase();
                        target_urls.insert(normalized);
                        if has_target_connection_limits {
                            let authority = (
                                url.scheme().to_owned(),
                                url.host_str()
                                    .expect("validated upstream URL has a host")
                                    .to_ascii_lowercase(),
                                url.port_or_known_default()
                                    .expect("http/https upstream URL has an effective port"),
                            );
                            if !target_authorities.insert(authority) {
                                self.push(
                                    &target_path,
                                    "target authority must be unique when per-target maxConnections is configured",
                                );
                            }
                        }
                        if let Some(ip) =
                            url.host_str().and_then(|host| host.parse::<IpAddr>().ok())
                        {
                            if !upstream_ip_is_allowed(ip, &upstream.address_policy.allowed_cidrs) {
                                self.push(
                                    &target_path,
                                    "literal upstream IP is forbidden unless its restricted range is explicitly authorized",
                                );
                            }
                        }
                    }
                    _ => {
                        if target.url.starts_with("unix:") {
                            self.push(
                                &target_path,
                                "invalid unix upstream target: expected `unix:/absolute/socket/path` without query or fragment",
                            );
                        } else {
                            self.push(
                                &target_path,
                                "upstream URL must be an http/https origin without credentials, query, or fragment",
                            );
                        }
                    }
                }
            }
            if has_unix_target {
                if upstream.tls.is_some() {
                    self.push(
                        format!("{path}/tls"),
                        "unix upstream sockets cannot use an upstream TLS policy",
                    );
                }
                unix_upstreams.insert(upstream.id.as_str());
            }
            if let Some(tls) = &upstream.tls {
                if has_plaintext_target {
                    self.push(
                        format!("{path}/tls"),
                        "upstream TLS policy requires every target to use https",
                    );
                }
                match tls.trust_mode {
                    UpstreamTlsTrustMode::System if !tls.ca_certificate_files.is_empty() => {
                        self.push(
                            format!("{path}/tls/caCertificateFiles"),
                            "system trust mode does not accept custom CA certificate files",
                        );
                    }
                    UpstreamTlsTrustMode::Custom | UpstreamTlsTrustMode::SystemAndCustom
                        if tls.ca_certificate_files.is_empty() =>
                    {
                        self.push(
                            format!("{path}/tls/caCertificateFiles"),
                            "custom trust modes require at least one CA certificate file",
                        );
                    }
                    _ => {}
                }
                for (file_index, file) in tls.ca_certificate_files.iter().enumerate() {
                    validate_upstream_tls_material_path(
                        self,
                        &format!("{path}/tls/caCertificateFiles/{file_index}"),
                        file,
                    );
                }
                match (
                    tls.client_certificate_file.as_deref(),
                    tls.client_private_key_file.as_deref(),
                ) {
                    (Some(certificate), Some(private_key)) => {
                        validate_upstream_tls_material_path(
                            self,
                            &format!("{path}/tls/clientCertificateFile"),
                            certificate,
                        );
                        validate_upstream_tls_material_path(
                            self,
                            &format!("{path}/tls/clientPrivateKeyFile"),
                            private_key,
                        );
                    }
                    (None, None) => {}
                    _ => self.push(
                        format!("{path}/tls"),
                        "clientCertificateFile and clientPrivateKeyFile must be configured together",
                    ),
                }
                if tls.minimum_version > tls.maximum_version {
                    self.push(
                        format!("{path}/tls/maximumVersion"),
                        "maximum TLS version must not be lower than minimumVersion",
                    );
                }
                if tls.dangerous_accept_any_verify
                    && (tls.trust_mode != UpstreamTlsTrustMode::System
                        || !tls.ca_certificate_files.is_empty())
                {
                    self.push(
                        format!("{path}/tls"),
                        "dangerousAcceptAnyVerify disables peer-chain verification and cannot be combined with custom trust material (use system trust mode with no caCertificateFiles)",
                    );
                }
                if let Some(server_name) = &tls.server_name {
                    if server_name.is_empty()
                        || server_name.len() > 253
                        || server_name
                            .bytes()
                            .any(|byte| byte.is_ascii_control() || byte == b' ' || byte == b'/')
                    {
                        self.push(
                            format!("{path}/tls/serverName"),
                            "upstream TLS serverName must be a host name without controls, spaces or `/`",
                        );
                    }
                }
            }
            if let Some(active_health) = &upstream.active_health {
                if !valid_active_health_uri(&active_health.uri) {
                    self.push(
                        format!("{path}/activeHealth/uri"),
                        "active health URI must be an origin-form path, may include a query, and must not contain an authority, fragment, backslash, or control character",
                    );
                }
                if active_health.timeout_ms > active_health.interval_ms {
                    self.push(
                        format!("{path}/activeHealth/timeoutMs"),
                        "active health timeout must not exceed intervalMs",
                    );
                }
                if active_health.success_status_min > active_health.success_status_max {
                    self.push(
                        format!("{path}/activeHealth/successStatusMax"),
                        "active health maximum success status must not be lower than successStatusMin",
                    );
                }
            }
        }
        if total_targets > MAX_TOTAL_UPSTREAM_TARGETS {
            self.push(
                "/upstreams",
                format!(
                    "configuration has {total_targets} upstream targets; maximum is {MAX_TOTAL_UPSTREAM_TARGETS}"
                ),
            );
        }

        // nginx server-name ownership is tracked per listener and per name
        // table (`exact` / `leading-wildcard` / `trailing-wildcard`). A
        // dot-prefixed name claims both the exact and the leading-wildcard
        // entry. nginx only warns ("conflicting server name ... ignored") and
        // keeps the first owner; silently dropping a name changes host
        // selection, so the compiler rejects the ambiguity instead (PRD §5).
        let mut host_owners: HashMap<(String, String), String> = HashMap::new();
        let mut total_routes = 0usize;
        for (index, virtual_host) in config.virtual_hosts.iter().enumerate() {
            let path = format!("/virtualHosts/{index}");
            let named_targets = named_location_targets(self, &path, virtual_host);
            let map_variable_names: Vec<String> = config
                .maps
                .iter()
                .filter_map(|map| map.result.strip_prefix('$').map(str::to_owned))
                .collect();
            let mut server_variables = map_variable_names.clone();
            validate_rewrite_phase(
                self,
                &format!("{path}/rewritePhase"),
                &virtual_host.rewrite_phase,
                &mut server_variables,
            );
            for (listener_index, listener_ref) in virtual_host.listener_refs.iter().enumerate() {
                if !listeners.contains_key(listener_ref.as_str()) {
                    self.push(
                        format!("{path}/listenerRefs/{listener_index}"),
                        format!("unknown listener {listener_ref}"),
                    );
                    continue;
                }
                for (name_index, server_name) in virtual_host.server_names.iter().enumerate() {
                    let name_path = format!("{path}/serverNames/{name_index}");
                    if server_name_is_regex(server_name) {
                        let Some(ServerNameForm::Regex(regex)) = parse_server_name(server_name)
                        else {
                            self.push(&name_path, "invalid `~` server_name regular expression");
                            continue;
                        };
                        validate_regex_pattern(self, name_path, &regex.compiled_pattern());
                        continue;
                    }
                    let Some(form) = parse_server_name(server_name) else {
                        self.push(
                            &name_path,
                            "invalid server name; supported forms are an exact name, `*.suffix`, `prefix.*`, `.suffix`, and `~regex`",
                        );
                        continue;
                    };
                    for claim in form.claims() {
                        let (table, value) = match claim {
                            super::ServerNameClaim::Exact(value) => ("exact", value),
                            super::ServerNameClaim::LeadingWildcard(value) => {
                                ("leading-wildcard", value)
                            }
                            super::ServerNameClaim::TrailingWildcard(value) => {
                                ("trailing-wildcard", value)
                            }
                        };
                        let key = (listener_ref.clone(), format!("{table}:{value}"));
                        if let Some(owner) = host_owners.insert(key, virtual_host.id.clone()) {
                            if owner != virtual_host.id {
                                self.push(
                                    &name_path,
                                    format!(
                                        "server name {server_name} on listener {listener_ref} is already owned by {owner}"
                                    ),
                                );
                            }
                        }
                    }
                }
            }

            total_routes = total_routes.saturating_add(virtual_host.routes.len());
            validate_routes(
                self,
                &path,
                &virtual_host.routes,
                &resources,
                &config.limits,
                &config.limit_req_zones,
                &named_targets,
                &virtual_host.error_pages,
                virtual_host.recursive_error_pages,
                &server_variables,
                !config.nginx.document_root_allowlist.is_empty(),
            );
            for (page_index, page) in virtual_host.error_pages.iter().enumerate() {
                validate_error_page_target(
                    self,
                    &format!("{path}/errorPages/{page_index}"),
                    page,
                    &named_targets,
                    &resources,
                    false,
                );
            }
            validate_security_headers(self, &path, virtual_host.security_headers.as_ref());
            validate_mime_policy(
                self,
                &format!("{path}/mimeTypes"),
                virtual_host.mime_types.as_deref(),
                virtual_host.default_type.as_deref(),
            );
            validate_cache_policy(
                self,
                &format!("{path}/cachePolicy"),
                virtual_host.cache_policy.as_ref(),
            );
        }
        if total_routes > MAX_TOTAL_ROUTES {
            self.push(
                "/virtualHosts",
                format!("configuration has {total_routes} routes; maximum is {MAX_TOTAL_ROUTES}"),
            );
        }

        for (listener_index, listener) in config.listeners.iter().enumerate() {
            if !config.virtual_hosts.iter().any(|virtual_host| {
                virtual_host
                    .listener_refs
                    .iter()
                    .any(|listener_ref| listener_ref == &listener.id)
            }) {
                self.push(
                    format!("/listeners/{listener_index}"),
                    "listener has no virtual hosts",
                );
            }

            let Some(policy_ref) = &listener.tls_policy_ref else {
                continue;
            };
            let Some(policy) = tls_policies.get(policy_ref.as_str()) else {
                continue;
            };
            let policy_certificates = policy
                .certificate_refs()
                .filter_map(|certificate_ref| certificates.get(certificate_ref))
                .copied()
                .collect::<Vec<_>>();
            // A listener that also serves the assigned certificate set is
            // expected to leave server names to it. `policy-first` exists so a
            // configured file wins for the names it covers while the assigned set
            // covers every other name, which makes an uncovered name here a
            // supported configuration rather than a gap. Only a listener with no
            // lower layer has to be complete.
            let assignments_cover_unlisted_names = TlsCertificateResolution::effective(
                listener.tls_certificate_resolution,
                listener.tls_policy_ref.is_some(),
                listener.tls_runtime.is_some(),
            )
            .is_some_and(TlsCertificateResolution::serves_assignments);
            for virtual_host in config.virtual_hosts.iter().filter(|virtual_host| {
                virtual_host
                    .listener_refs
                    .iter()
                    .any(|listener_ref| listener_ref == &listener.id)
            }) {
                for server_name in &virtual_host.server_names {
                    // SNI selection supports exact names, one-level leading
                    // wildcards, and authored-order regex names (evaluated
                    // after the exact/wildcard tables, exactly like nginx). A
                    // trailing wildcard still has no certificate-index
                    // representation and fails closed.
                    if server_name_is_regex(server_name) {
                        if !assignments_cover_unlisted_names
                            && !policy_certificates
                                .iter()
                                .any(|certificate| !certificate.server_name_regexes.is_empty())
                        {
                            self.push(
                                format!("/listeners/{listener_index}/tlsPolicyRef"),
                                format!(
                                    "TLS policy {policy_ref} has no certificate declaring regex server name {server_name}"
                                ),
                            );
                        }
                        continue;
                    }
                    let Some(form) = parse_server_name(server_name) else {
                        // Already reported at the virtual host.
                        continue;
                    };
                    let coverage_name = match &form {
                        ServerNameForm::Exact(name) => {
                            if name.parse::<IpAddr>().is_ok() {
                                self.push(
                                    format!("/listeners/{listener_index}/tlsPolicyRef"),
                                    format!(
                                        "strict SNI certificate selection requires a DNS server name, not IP address {server_name}"
                                    ),
                                );
                                continue;
                            }
                            name.clone()
                        }
                        ServerNameForm::LeadingWildcard(suffix) => format!("*.{suffix}"),
                        ServerNameForm::DotPrefix(suffix) => {
                            // `.example.com` selects one certificate for the
                            // bare name and any subdomain: both claims have to
                            // be covered.
                            if !assignments_cover_unlisted_names
                                && !certificate_covers_any(
                                    &policy_certificates,
                                    &suffix.to_string(),
                                )
                            {
                                self.push(
                                    format!("/listeners/{listener_index}/tlsPolicyRef"),
                                    format!(
                                        "TLS policy {policy_ref} has no certificate covering server name {server_name}"
                                    ),
                                );
                                continue;
                            }
                            format!("*.{suffix}")
                        }
                        ServerNameForm::TrailingWildcard(_) => {
                            self.push(
                                format!("/listeners/{listener_index}/tlsPolicyRef"),
                                format!(
                                    "trailing-wildcard server name {server_name} is not supported on a TLS listener: SNI certificate selection is an exact/leading-wildcard index"
                                ),
                            );
                            continue;
                        }
                        ServerNameForm::Regex(_) => continue,
                    };
                    if assignments_cover_unlisted_names {
                        continue;
                    }
                    if !certificate_covers_any(&policy_certificates, &coverage_name) {
                        self.push(
                            format!("/listeners/{listener_index}/tlsPolicyRef"),
                            format!(
                                "TLS policy {policy_ref} has no certificate covering server name {server_name}"
                            ),
                        );
                    }
                }
            }
        }
    }

    fn validate_limit_req_zones(&mut self, config: &WebServerAppConfig) {
        let mut names = HashSet::new();
        for (index, zone) in config.limit_req_zones.iter().enumerate() {
            let path = format!("/limitReqZones/{index}");
            if zone.name.is_empty() {
                self.push(
                    format!("{path}/name"),
                    "limitReqZone name must not be empty",
                );
            } else if !names.insert(zone.name.as_str()) {
                self.push(
                    format!("{path}/name"),
                    format!("duplicate limitReqZone name {}", zone.name),
                );
            }
            if zone.key != "$binary_remote_addr" && zone.key != "$remote_addr" {
                self.push(
                    format!("{path}/key"),
                    "only $binary_remote_addr and $remote_addr are executable",
                );
            }
            if zone.max_keys == 0 {
                self.push(format!("{path}/maxKeys"), "maxKeys must be at least 1");
            }
            if !(zone.rate_per_second.is_finite() && zone.rate_per_second > 0.0) {
                self.push(
                    format!("{path}/ratePerSecond"),
                    "ratePerSecond must be a positive finite number",
                );
            }
        }
    }

    fn validate_app_domain_fallback(&mut self, config: &WebServerAppConfig) {
        let Some(fallback) = config.app_domain_fallback.as_ref() else {
            return;
        };
        if fallback.suffixes.is_empty() || fallback.suffixes.len() > 64 {
            self.push(
                "/appDomainFallback/suffixes",
                "suffixes must contain between 1 and 64 platform app-domain suffixes",
            );
        }
        let mut seen = std::collections::HashSet::new();
        for (index, suffix) in fallback.suffixes.iter().enumerate() {
            let path = format!("/appDomainFallback/suffixes/{index}");
            let suffix = suffix.trim().to_ascii_lowercase();
            if suffix.is_empty()
                || suffix.starts_with('.')
                || suffix.ends_with('.')
                || suffix.starts_with("*.")
                || suffix.contains('/')
                || suffix.len() > 253
                || !suffix.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'.'
                        || byte == b'-'
                })
            {
                self.push(
                    &path,
                    "suffix must be a normalized lowercase ASCII domain without a leading dot or wildcard",
                );
            }
            if !seen.insert(suffix.clone()) {
                self.push(&path, format!("duplicate app-domain suffix {suffix}"));
            }
            // The Deploy control plane only ever provisions
            // `<appLabel>.app[-<env>].<suffix>` under its own catalog, so a
            // suffix outside it can never resolve: rejecting it here turns a
            // permanently-404 configuration into a startup error instead of a
            // misleading "default app domain" classification.
            if !sdkwork_deploy_core::PLATFORM_APP_DOMAIN_SUFFIXES.contains(&suffix.as_str()) {
                self.push(
                    &path,
                    format!(
                        "suffix {suffix} is not part of the Deploy platform app-domain catalog \
                         ({}); the control plane never provisions hostnames under it",
                        sdkwork_deploy_core::PLATFORM_APP_DOMAIN_SUFFIXES.join(", ")
                    ),
                );
            }
        }
        match &fallback.lookup {
            AppDomainFallbackLookup::Embedded => {}
        }
        if !(100..=30_000).contains(&fallback.timeout_ms) {
            self.push(
                "/appDomainFallback/timeoutMs",
                "timeoutMs must be between 100 and 30000",
            );
        }
        if fallback.cache_ttl_ms > 86_400_000 {
            self.push(
                "/appDomainFallback/cacheTtlMs",
                "cacheTtlMs must not exceed 24 hours",
            );
        }
        if fallback.negative_cache_ttl_ms > 86_400_000 {
            self.push(
                "/appDomainFallback/negativeCacheTtlMs",
                "negativeCacheTtlMs must not exceed 24 hours",
            );
        }
    }

    fn validate_usage_metering(&mut self, config: &WebServerAppConfig) {
        let Some(metering) = config.usage_metering.as_ref() else {
            return;
        };
        if !(1..=86_400).contains(&metering.window_seconds) {
            self.push(
                "/usageMetering/windowSeconds",
                "windowSeconds must be between 1 and 86400",
            );
        }
        if !(1_000..=3_600_000).contains(&metering.flush_interval_ms) {
            self.push(
                "/usageMetering/flushIntervalMs",
                "flushIntervalMs must be between 1000 and 3600000",
            );
        }
        if let UsageMeteringChannel::Http { endpoint, .. } = &metering.channel {
            if endpoint.trim().is_empty()
                || !endpoint.starts_with("https://") && !endpoint.starts_with("http://")
            {
                self.push(
                    "/usageMetering/channel/endpoint",
                    "http channel requires an absolute http(s) endpoint URL",
                );
            }
        }
    }

    /// Validate the executable `map` subset and record each result variable so
    /// response-header declarations that reference one are accepted.
    fn validate_maps(&mut self, config: &WebServerAppConfig) {
        self.map_variables = config
            .maps
            .iter()
            .filter_map(super::map_variables::map_result_name)
            .map(str::to_owned)
            .collect();
        for (index, geo) in config.geo.iter().enumerate() {
            if let Err(message) = super::validate_geo_config(geo) {
                self.push(format!("/geo/{index}"), message);
            }
            if let Some(result) = geo.result.strip_prefix('$') {
                self.note_variable_block_result(result, &format!("/geo/{index}/result"));
            }
        }
        for (index, split) in config.split_clients.iter().enumerate() {
            if let Err(message) = super::validate_split_clients_config(split) {
                self.push(format!("/splitClients/{index}"), message);
            }
            if let Some(result) = split.result.strip_prefix('$') {
                self.note_variable_block_result(result, &format!("/splitClients/{index}/result"));
            }
        }
        if config.maps.is_empty() {
            return;
        }
        if let Err(message) = super::map_variables::validate_map_definitions(&config.maps) {
            self.push("/maps", message);
        }
    }

    /// One result-variable namespace: `map`, `geo` and `split_clients` all
    /// define request variables, so a duplicate is a configuration error (the
    /// runtime refuses silently shadowed variables).
    fn note_variable_block_result(&mut self, result: &str, path: &str) {
        if self.map_variables.iter().any(|name| name == result) {
            self.push(
                path,
                format!(
                    "result variable `${result}` is already declared by another variable block; the runtime rejects duplicate variable definitions"
                ),
            );
            return;
        }
        self.map_variables.push(result.to_owned());
    }

    fn validate_limits(&mut self, config: &WebServerAppConfig) {
        let limits = &config.limits;
        if limits.max_request_body_bytes > 2_147_483_648 {
            self.push(
                "/limits/maxRequestBodyBytes",
                "maximum request body limit is 2 GiB",
            );
        }
        if !(100..=3_600_000).contains(&limits.request_timeout_ms) {
            self.push(
                "/limits/requestTimeoutMs",
                "request timeout must be between 100 ms and 1 hour",
            );
        }
        if !(100..=3_600_000).contains(&limits.request_body_start_timeout_ms) {
            self.push(
                "/limits/requestBodyStartTimeoutMs",
                "request Body start timeout must be between 100 ms and 1 hour",
            );
        }
        if !(100..=3_600_000).contains(&limits.request_body_idle_timeout_ms) {
            self.push(
                "/limits/requestBodyIdleTimeoutMs",
                "request Body idle timeout must be between 100 ms and 1 hour",
            );
        }
        if !(100..=3_600_000).contains(&limits.response_body_idle_timeout_ms) {
            self.push(
                "/limits/responseBodyIdleTimeoutMs",
                "response Body idle timeout must be between 100 ms and 1 hour",
            );
        }
        if !(100..=3_600_000).contains(&limits.connection_write_timeout_ms) {
            self.push(
                "/limits/connectionWriteTimeoutMs",
                "connection write timeout must be between 100 ms and 1 hour",
            );
        }
        if !(100..=3_600_000).contains(&limits.tls_handshake_timeout_ms) {
            self.push(
                "/limits/tlsHandshakeTimeoutMs",
                "TLS handshake timeout must be between 100 ms and 1 hour",
            );
        }
        if !(100..=3_600_000).contains(&limits.http1_keep_alive_idle_timeout_ms) {
            self.push(
                "/limits/http1KeepAliveIdleTimeoutMs",
                "HTTP/1 Keep-Alive idle timeout must be between 100 ms and 1 hour",
            );
        }
        if !(1..=1_024).contains(&limits.http1_max_pipeline_depth) {
            self.push(
                "/limits/http1MaxPipelineDepth",
                "HTTP/1 Pipeline depth must be between 1 and 1,024",
            );
        }
        if !(100..=600_000).contains(&limits.drain_timeout_ms) {
            self.push(
                "/limits/drainTimeoutMs",
                "drain timeout must be between 100 ms and 10 minutes",
            );
        }
        if !(1..=1_000_000).contains(&limits.max_connections) {
            self.push(
                "/limits/maxConnections",
                "maxConnections must be between 1 and 1,000,000",
            );
        }
        if !(1..=100_000).contains(&limits.max_concurrent_requests) {
            self.push(
                "/limits/maxConcurrentRequests",
                "maxConcurrentRequests must be between 1 and 100,000",
            );
        }
        if !(8_192..=1_048_576).contains(&limits.max_request_header_bytes) {
            self.push(
                "/limits/maxRequestHeaderBytes",
                "maxRequestHeaderBytes must be between 8 KiB and 1 MiB",
            );
        }
        if !(16..=65_536).contains(&limits.max_request_line_bytes) {
            self.push(
                "/limits/maxRequestLineBytes",
                "maxRequestLineBytes must be between 16 bytes and 64 KiB",
            );
        }
        if !(1..=256).contains(&limits.max_request_method_bytes) {
            self.push(
                "/limits/maxRequestMethodBytes",
                "maxRequestMethodBytes must be between 1 and 256 bytes",
            );
        }
        if !(1..=65_536).contains(&limits.max_request_target_bytes) {
            self.push(
                "/limits/maxRequestTargetBytes",
                "maxRequestTargetBytes must be between 1 byte and 64 KiB",
            );
        }
        if !(1..=65_536).contains(&limits.max_uri_path_bytes) {
            self.push(
                "/limits/maxUriPathBytes",
                "maxUriPathBytes must be between 1 and 64 KiB",
            );
        }
        if !(1..=65_536).contains(&limits.max_decoded_path_bytes) {
            self.push(
                "/limits/maxDecodedPathBytes",
                "maxDecodedPathBytes must be between 1 and 64 KiB",
            );
        }
        if limits.max_decoded_path_bytes > limits.max_uri_path_bytes {
            self.push(
                "/limits/maxDecodedPathBytes",
                "maxDecodedPathBytes must not exceed maxUriPathBytes",
            );
        }
        if !(1..=4_096).contains(&limits.max_path_segments) {
            self.push(
                "/limits/maxPathSegments",
                "maxPathSegments must be between 1 and 4,096",
            );
        }
        if limits.max_query_string_bytes > 65_536 {
            self.push(
                "/limits/maxQueryStringBytes",
                "maxQueryStringBytes must not exceed 64 KiB",
            );
        }
        if limits.max_query_parameters > 4_096 {
            self.push(
                "/limits/maxQueryParameters",
                "maxQueryParameters must not exceed 4,096",
            );
        }
        if limits.max_query_component_bytes > 65_536 {
            self.push(
                "/limits/maxQueryComponentBytes",
                "maxQueryComponentBytes must not exceed 64 KiB",
            );
        }
        let query_disabled = limits.max_query_string_bytes == 0
            && limits.max_query_parameters == 0
            && limits.max_query_component_bytes == 0;
        let query_enabled = limits.max_query_string_bytes > 0
            && limits.max_query_parameters > 0
            && limits.max_query_component_bytes > 0;
        if !query_disabled && !query_enabled {
            self.push(
                "/limits",
                "query string, parameter, and component budgets must all be zero or all be positive",
            );
        }
        if query_enabled && limits.max_query_component_bytes > limits.max_query_string_bytes {
            self.push(
                "/limits/maxQueryComponentBytes",
                "maxQueryComponentBytes must not exceed maxQueryStringBytes",
            );
        }
        if !(1..=8_192).contains(&limits.max_header_name_bytes) {
            self.push(
                "/limits/maxHeaderNameBytes",
                "maxHeaderNameBytes must be between 1 byte and 8 KiB",
            );
        }
        if !(1..=1_048_576).contains(&limits.max_header_value_bytes) {
            self.push(
                "/limits/maxHeaderValueBytes",
                "maxHeaderValueBytes must be between 1 byte and 1 MiB",
            );
        }
        if limits.max_request_line_bytes > limits.max_request_header_bytes {
            self.push(
                "/limits/maxRequestLineBytes",
                "maxRequestLineBytes must not exceed maxRequestHeaderBytes",
            );
        }
        if limits.max_request_method_bytes > limits.max_request_line_bytes {
            self.push(
                "/limits/maxRequestMethodBytes",
                "maxRequestMethodBytes must not exceed maxRequestLineBytes",
            );
        }
        if limits.max_request_target_bytes > limits.max_request_line_bytes {
            self.push(
                "/limits/maxRequestTargetBytes",
                "maxRequestTargetBytes must not exceed maxRequestLineBytes",
            );
        }
        if limits.max_header_name_bytes > limits.max_request_header_bytes {
            self.push(
                "/limits/maxHeaderNameBytes",
                "maxHeaderNameBytes must not exceed maxRequestHeaderBytes",
            );
        }
        if limits.max_header_value_bytes > limits.max_request_header_bytes {
            self.push(
                "/limits/maxHeaderValueBytes",
                "maxHeaderValueBytes must not exceed maxRequestHeaderBytes",
            );
        }
        if !(1..=1_024).contains(&limits.max_request_headers) {
            self.push(
                "/limits/maxRequestHeaders",
                "maxRequestHeaders must be between 1 and 1,024",
            );
        }
        if !(100..=60_000).contains(&limits.request_header_timeout_ms) {
            self.push(
                "/limits/requestHeaderTimeoutMs",
                "requestHeaderTimeoutMs must be between 100 ms and 1 minute",
            );
        }
        if !(16..=8_192).contains(&limits.max_chunk_line_bytes) {
            self.push(
                "/limits/maxChunkLineBytes",
                "maxChunkLineBytes must be between 16 bytes and 8 KiB",
            );
        }
        if limits.max_trailer_bytes > 1_048_576 {
            self.push(
                "/limits/maxTrailerBytes",
                "maxTrailerBytes must not exceed 1 MiB",
            );
        }
        if limits.max_trailers > 1_024 {
            self.push("/limits/maxTrailers", "maxTrailers must not exceed 1,024");
        }
        if (limits.max_trailer_bytes == 0) != (limits.max_trailers == 0) {
            self.push(
                "/limits",
                "maxTrailerBytes and maxTrailers must both be zero or both be positive",
            );
        }
        if !(1..=10_000).contains(&limits.http2_max_concurrent_streams) {
            self.push(
                "/limits/http2MaxConcurrentStreams",
                "http2MaxConcurrentStreams must be between 1 and 10,000",
            );
        }
        if !(100..=86_400_000).contains(&limits.max_connection_age_ms) {
            self.push(
                "/limits/maxConnectionAgeMs",
                "maxConnectionAgeMs must be between 100 ms and 24 hours",
            );
        }
        if !(1_000..=3_600_000).contains(&limits.http2_keep_alive_interval_ms) {
            self.push(
                "/limits/http2KeepAliveIntervalMs",
                "HTTP/2 Keep-Alive interval must be between 1 second and 1 hour",
            );
        }
        if !(100..=60_000).contains(&limits.http2_keep_alive_timeout_ms) {
            self.push(
                "/limits/http2KeepAliveTimeoutMs",
                "HTTP/2 Keep-Alive ACK timeout must be between 100 ms and 1 minute",
            );
        }
        if limits.http2_keep_alive_timeout_ms > limits.http2_keep_alive_interval_ms {
            self.push(
                "/limits/http2KeepAliveTimeoutMs",
                "HTTP/2 Keep-Alive ACK timeout must not exceed its interval",
            );
        }
        if !(1..=1_024).contains(&limits.http2_max_pending_accept_reset_streams) {
            self.push(
                "/limits/http2MaxPendingAcceptResetStreams",
                "http2MaxPendingAcceptResetStreams must be between 1 and 1,024",
            );
        }
        if !(1..=4_096).contains(&limits.http2_max_local_error_reset_streams) {
            self.push(
                "/limits/http2MaxLocalErrorResetStreams",
                "http2MaxLocalErrorResetStreams must be between 1 and 4,096",
            );
        }
        if !(1_024..=16_777_216).contains(&limits.http2_max_send_buffer_bytes) {
            self.push(
                "/limits/http2MaxSendBufferBytes",
                "http2MaxSendBufferBytes must be between 1 KiB and 16 MiB",
            );
        }
        if !(1_024..=1_048_576).contains(&limits.http2_max_header_list_bytes) {
            self.push(
                "/limits/http2MaxHeaderListBytes",
                "http2MaxHeaderListBytes must be between 1 KiB and 1 MiB",
            );
        }
        if !(16_384..=16_777_215).contains(&limits.http2_max_frame_bytes) {
            self.push(
                "/limits/http2MaxFrameBytes",
                "http2MaxFrameBytes must be between 16 KiB and 16,777,215 bytes",
            );
        }
        if !(100..=60_000).contains(&limits.http2_abuse_window_ms) {
            self.push(
                "/limits/http2AbuseWindowMs",
                "http2AbuseWindowMs must be between 100 ms and 1 minute",
            );
        }
        if !(100..=1_000_000).contains(&limits.http2_max_frames_per_window) {
            self.push(
                "/limits/http2MaxFramesPerWindow",
                "http2MaxFramesPerWindow must be between 100 and 1,000,000",
            );
        }
        if !(1..=100_000).contains(&limits.http2_max_new_streams_per_window) {
            self.push(
                "/limits/http2MaxNewStreamsPerWindow",
                "http2MaxNewStreamsPerWindow must be between 1 and 100,000",
            );
        }
        if !(1..=100_000).contains(&limits.http2_max_reset_frames_per_window) {
            self.push(
                "/limits/http2MaxResetFramesPerWindow",
                "http2MaxResetFramesPerWindow must be between 1 and 100,000",
            );
        }
        if limits.http2_max_continuation_frames > 1_024 {
            self.push(
                "/limits/http2MaxContinuationFrames",
                "http2MaxContinuationFrames must not exceed 1,024",
            );
        }
        if !(1_024..=1_048_576).contains(&limits.http2_max_encoded_header_block_bytes) {
            self.push(
                "/limits/http2MaxEncodedHeaderBlockBytes",
                "http2MaxEncodedHeaderBlockBytes must be between 1 KiB and 1 MiB",
            );
        }
        if limits.http2_max_new_streams_per_window > limits.http2_max_frames_per_window {
            self.push(
                "/limits/http2MaxNewStreamsPerWindow",
                "http2MaxNewStreamsPerWindow must not exceed http2MaxFramesPerWindow",
            );
        }
        if limits.http2_max_reset_frames_per_window > limits.http2_max_frames_per_window {
            self.push(
                "/limits/http2MaxResetFramesPerWindow",
                "http2MaxResetFramesPerWindow must not exceed http2MaxFramesPerWindow",
            );
        }
        let concurrent_streams = u64::from(limits.http2_max_concurrent_streams);
        if concurrent_streams.saturating_mul(limits.http2_max_send_buffer_bytes as u64)
            > 64 * 1024 * 1024
        {
            self.push(
                "/limits",
                "HTTP/2 concurrent-stream send-buffer budget must not exceed 64 MiB per connection",
            );
        }
        if concurrent_streams.saturating_mul(u64::from(limits.http2_max_header_list_bytes))
            > 64 * 1024 * 1024
        {
            self.push(
                "/limits",
                "HTTP/2 concurrent-stream header-list budget must not exceed 64 MiB per connection",
            );
        }
        if concurrent_streams.saturating_mul(limits.http2_max_encoded_header_block_bytes as u64)
            > 64 * 1024 * 1024
        {
            self.push(
                "/limits",
                "HTTP/2 concurrent-stream encoded-header budget must not exceed 64 MiB per connection",
            );
        }
        if (limits.max_connections as u64).saturating_mul(limits.max_request_header_bytes as u64)
            > 1024 * 1024 * 1024
        {
            self.push(
                "/limits",
                "global HTTP/1 connection header-window budget must not exceed 1 GiB",
            );
        }
        let active_requests = limits.max_concurrent_requests as u64;
        if active_requests.saturating_mul(u64::from(limits.http2_max_header_list_bytes))
            > 1024 * 1024 * 1024
        {
            self.push(
                "/limits",
                "global active HTTP/2 header-list budget must not exceed 1 GiB",
            );
        }
        if active_requests.saturating_mul(limits.http2_max_send_buffer_bytes as u64)
            > 1024 * 1024 * 1024
        {
            self.push(
                "/limits",
                "global active HTTP/2 send-buffer budget must not exceed 1 GiB",
            );
        }
        if (limits.max_connections as u64)
            .saturating_mul(limits.http2_max_encoded_header_block_bytes as u64)
            > 1024 * 1024 * 1024
        {
            self.push(
                "/limits",
                "global HTTP/2 encoded-header connection budget must not exceed 1 GiB",
            );
        }
        if !(100..=60_000).contains(&config.deployment.reload.poll_interval_ms) {
            self.push(
                "/deployment/reload/pollIntervalMs",
                "reload poll interval must be between 100 ms and 1 minute",
            );
        }
    }

    fn validate_resource_pressure(&mut self, config: &WebServerAppConfig) {
        let Some(policy) = &config.deployment.resource_pressure else {
            return;
        };
        let path = "/deployment/resourcePressure";
        if policy.memory_reserve_bytes >= policy.maximum_process_memory_bytes {
            self.push(
                format!("{path}/memoryReserveBytes"),
                "memory reserve must be lower than maximumProcessMemoryBytes",
            );
        }
        if policy.memory_recovery_percent >= policy.memory_admission_percent {
            self.push(
                format!("{path}/memoryRecoveryPercent"),
                "memory recovery percent must be lower than memoryAdmissionPercent",
            );
        }
        if effective_pressure_threshold(
            policy.maximum_process_memory_bytes,
            policy.memory_reserve_bytes,
            policy.memory_recovery_percent,
        ) >= effective_pressure_threshold(
            policy.maximum_process_memory_bytes,
            policy.memory_reserve_bytes,
            policy.memory_admission_percent,
        ) {
            self.push(
                format!("{path}/memoryReserveBytes"),
                "effective memory recovery threshold must be lower than the admission threshold",
            );
        }
        if policy.open_handle_reserve >= policy.maximum_open_handles {
            self.push(
                format!("{path}/openHandleReserve"),
                "open handle reserve must be lower than maximumOpenHandles",
            );
        }
        if policy.open_handle_recovery_percent >= policy.open_handle_admission_percent {
            self.push(
                format!("{path}/openHandleRecoveryPercent"),
                "open handle recovery percent must be lower than openHandleAdmissionPercent",
            );
        }
        if effective_pressure_threshold(
            policy.maximum_open_handles,
            policy.open_handle_reserve,
            policy.open_handle_recovery_percent,
        ) >= effective_pressure_threshold(
            policy.maximum_open_handles,
            policy.open_handle_reserve,
            policy.open_handle_admission_percent,
        ) {
            self.push(
                format!("{path}/openHandleReserve"),
                "effective open-handle recovery threshold must be lower than the admission threshold",
            );
        }
        if policy.event_loop_lag_recovery_ms >= policy.event_loop_lag_admission_ms {
            self.push(
                format!("{path}/eventLoopLagRecoveryMs"),
                "event-loop lag recovery must be lower than eventLoopLagAdmissionMs",
            );
        }
        if policy.operations_reserve_requests >= config.limits.max_concurrent_requests {
            self.push(
                format!("{path}/operationsReserveRequests"),
                "operations request reserve must be lower than maxConcurrentRequests",
            );
        }
    }

    fn validate_streams(
        &mut self,
        config: &WebServerAppConfig,
        upstreams: &HashMap<&str, &UpstreamConfig>,
    ) {
        // The stream module owns a separate `limit_conn_zone` table; validate
        // it before any server references it.
        // Stream `map` blocks and `set`/`return` templates use the bounded
        // accept/preread variable engine.
        let mut stream_map_results: Vec<String> = Vec::new();
        for (index, map) in config.stream_maps.iter().enumerate() {
            let path = format!("/streamMaps/{index}");
            let Some(source) = map.source.strip_prefix('$') else {
                self.push(
                    format!("{path}/source"),
                    "stream map source must be a `$variable`",
                );
                continue;
            };
            if !crate::config::STREAM_MAP_SOURCES.contains(&source) {
                self.push(
                    format!("{path}/source"),
                    format!("stream map source `${source}` is not known at accept/preread time"),
                );
            }
            if !map.result.starts_with('$') {
                self.push(
                    format!("{path}/result"),
                    "stream map result must be a `$variable`",
                );
                continue;
            }
            let result = map.result.trim_start_matches('$').to_owned();
            if stream_map_results.iter().any(|name| name == &result) {
                self.push(
                    format!("{path}/result"),
                    format!("duplicate stream map result `${result}`"),
                );
            }
            if map.entries.iter().any(|entry| entry.value.contains('$'))
                || map
                    .default
                    .as_deref()
                    .is_some_and(|value| value.contains('$'))
            {
                self.push(
                    &path,
                    "stream map values must be literal strings; variable values are not supported",
                );
            }
            if map.hostnames {
                self.push(&path, "stream map `hostnames` is not supported");
            }
            stream_map_results.push(result);
        }
        let mut stream_zone_names: HashSet<&str> = HashSet::new();
        for (index, zone) in config.stream_limit_conn_zones.iter().enumerate() {
            let path = format!("/streamLimitConnZones/{index}");
            if !stream_zone_names.insert(zone.name.as_str()) {
                self.push(
                    &path,
                    format!("duplicate stream limit_conn_zone `{}`", zone.name),
                );
            }
            if zone.key != "$binary_remote_addr" && zone.key != "$remote_addr" {
                self.push(
                    format!("{path}/key"),
                    "stream limit_conn_zone supports $binary_remote_addr or $remote_addr",
                );
            }
            if zone.max_keys == 0 {
                self.push(
                    format!("{path}/maxKeys"),
                    "zone must track at least one key",
                );
            }
        }
        let mut sockets = HashSet::new();
        for (index, stream) in config.streams.iter().enumerate() {
            let path = format!("/streams/{index}");
            if stream.bind.parse::<IpAddr>().is_err() {
                self.push(
                    format!("{path}/bind"),
                    "bind must be an explicit IPv4 or IPv6 address",
                );
            }
            let socket_key = stream.socket_key();
            if !sockets.insert(socket_key.clone()) {
                self.push(
                    &path,
                    "another stream server already owns this bind and port",
                );
            }
            if config.listeners.iter().any(|listener| {
                format!("{}:{}", listener.bind.to_ascii_lowercase(), listener.port) == socket_key
            }) {
                self.push(&path, "an http listener already owns this bind and port");
            }
            if config
                .mail
                .servers
                .iter()
                .any(|mail| mail.socket_key() == socket_key)
            {
                self.push(&path, "a mail server already owns this bind and port");
            }
            if !(1..=3_600_000).contains(&stream.preread_timeout_ms) {
                self.push(
                    format!("{path}/prereadTimeoutMs"),
                    "stream preread_timeout must be between 1 ms and 1 hour",
                );
            }
            if !(512..=1024 * 1024).contains(&stream.preread_buffer_bytes) {
                self.push(
                    format!("{path}/prereadBufferBytes"),
                    "stream preread_buffer_size must be between 512 bytes and 1 MiB",
                );
            }
            for (set_index, set) in stream.set_variables.iter().enumerate() {
                let set_path = format!("{path}/setVariables/{set_index}");
                let mut available = stream_map_results.clone();
                for prior in &stream.set_variables[..set_index] {
                    available.push(prior.name.clone());
                }
                if let Err(message) =
                    crate::config::validate_stream_set(&set.name, &set.value, &available)
                {
                    self.push(&set_path, message);
                }
            }
            if let Some(return_value) = &stream.return_value {
                let mut available = stream_map_results.clone();
                for set in &stream.set_variables {
                    available.push(set.name.clone());
                }
                if let Err(message) =
                    crate::config::validate_stream_return(return_value, &available)
                {
                    self.push(format!("{path}/returnValue"), message);
                }
            }
            for (rule_index, rule) in stream.limit_conn.iter().enumerate() {
                let rule_path = format!("{path}/limitConn/{rule_index}");
                if !stream_zone_names.contains(rule.zone.as_str()) {
                    self.push(
                        &rule_path,
                        format!("unknown stream limit_conn_zone `{}`", rule.zone),
                    );
                }
                if rule.max_connections == 0 {
                    self.push(
                        format!("{rule_path}/maxConnections"),
                        "must allow at least one connection",
                    );
                }
            }
            // nginx stream `return` servers answer without an upstream; every
            // other stream server needs exactly one target.
            match (stream.return_value.as_deref(), &stream.target) {
                (Some(value), Some(_)) => {
                    self.push(
                        &path,
                        "a stream server cannot combine `return` with a proxy target",
                    );
                    if value.is_empty() {
                        self.push(
                            format!("{path}/returnValue"),
                            "return value must not be empty",
                        );
                    }
                }
                (Some(value), None) => {
                    if value.is_empty() {
                        self.push(
                            format!("{path}/returnValue"),
                            "return value must not be empty",
                        );
                    }
                    if value.len() > 4096 {
                        self.push(
                            format!("{path}/returnValue"),
                            "return value must not exceed 4 KiB",
                        );
                    }
                }
                (None, None) => self.push(
                    &path,
                    "stream server requires a `target` or a `returnValue`",
                ),
                (None, Some(target)) => match target {
                    StreamTargetConfig::Upstream { name } => {
                        if !upstreams.contains_key(name.as_str()) {
                            self.push(format!("{path}/target"), format!("unknown upstream {name}"));
                        }
                    }
                    StreamTargetConfig::Literal { host, port } => {
                        if host.is_empty() || *port == 0 {
                            self.push(
                                format!("{path}/target"),
                                "literal target must be a non-empty host and port",
                            );
                        }
                    }
                },
            }
            for (rule_index, rule) in stream.access.iter().enumerate() {
                let rule_path = format!("{path}/access/{rule_index}/network");
                let trimmed = rule.network.trim();
                if !(trimmed.eq_ignore_ascii_case("all")
                    || trimmed.parse::<std::net::IpAddr>().is_ok()
                    || trimmed.parse::<ipnet::IpNet>().is_ok())
                {
                    self.push(rule_path, "access network must be `all`, an IP, or a CIDR");
                }
            }
            match &stream.tls {
                Some(crate::config::model::StreamTlsMode::Terminate {
                    certificate_ref, ..
                }) => {
                    if certificate_ref.is_empty() {
                        self.push(
                            format!("{path}/tls/certificateRef"),
                            "stream TLS terminate requires a non-empty certificateRef",
                        );
                    } else if !config
                        .certificates
                        .iter()
                        .any(|certificate| certificate.id == *certificate_ref)
                    {
                        self.push(
                            format!("{path}/tls/certificateRef"),
                            format!("unknown certificate {certificate_ref}"),
                        );
                    }
                }
                Some(crate::config::model::StreamTlsMode::Preread) | None => {}
            }
            if !(1_000..=3_600_000).contains(&stream.proxy_timeout_ms) {
                self.push(
                    format!("{path}/proxyTimeoutMs"),
                    "proxyTimeoutMs must be between 1000 and 3,600,000 milliseconds",
                );
            }
        }
    }

    /// Validate the nginx `mail {}` listener plane (Wave 9A): a real TCP
    /// listener on the shared bind:port namespace, bounded session/buffer
    /// knobs, and the bounded plaintext auth-mechanism vocabulary.
    fn validate_mail(&mut self, config: &WebServerAppConfig) {
        let mut sockets = HashSet::new();
        for (index, server) in config.mail.servers.iter().enumerate() {
            let path = format!("/mail/servers/{index}");
            if server.id.is_empty() || server.id.len() > 128 {
                self.push(format!("{path}/id"), "mail server id must be 1..=128 bytes");
            }
            if server.bind.parse::<IpAddr>().is_err() {
                self.push(
                    format!("{path}/bind"),
                    "bind must be an explicit IPv4 or IPv6 address",
                );
            }
            if server.port == 0 {
                self.push(format!("{path}/port"), "port must be 1..=65535");
            }
            let socket_key = server.socket_key();
            if !sockets.insert(socket_key.clone()) {
                self.push(&path, "another mail server already owns this bind and port");
            }
            if config.listeners.iter().any(|listener| {
                format!("{}:{}", listener.bind.to_ascii_lowercase(), listener.port) == socket_key
            }) {
                self.push(&path, "an http listener already owns this bind and port");
            }
            if config
                .streams
                .iter()
                .any(|stream| stream.socket_key() == socket_key)
            {
                self.push(
                    &path,
                    "an http stream server already owns this bind and port",
                );
            }
            if !(1..=86_400_000).contains(&server.timeout_ms) {
                self.push(
                    format!("{path}/timeoutMs"),
                    "mail timeout must be between 1 ms and 24 hours",
                );
            }
            for (field, value) in [
                ("smtpClientBuffer", server.smtp_client_buffer),
                ("imapClientBuffer", server.imap_client_buffer),
            ] {
                if !(512..=1024 * 1024).contains(&value) {
                    self.push(
                        format!("{path}/{field}"),
                        "mail client buffer must be between 512 bytes and 1 MiB",
                    );
                }
            }
            if server.smtp_greeting_delay_ms > 60_000 {
                self.push(
                    format!("{path}/smtpGreetingDelayMs"),
                    "smtp_greeting_delay must not exceed 60 seconds",
                );
            }
            if let Some(server_name) = &server.server_name {
                let valid = !server_name.is_empty()
                    && server_name.len() <= 253
                    && server_name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                    });
                if !valid {
                    self.push(
                        format!("{path}/serverName"),
                        "mail server_name must be a 1..=253 byte host name of letters, digits, `.`, `-`, or `_`",
                    );
                }
            }
            for (field, mechanisms) in [
                ("pop3Auth", &server.pop3_auth),
                ("imapAuth", &server.imap_auth),
                ("smtpAuth", &server.smtp_auth),
            ] {
                if mechanisms.is_empty() {
                    self.push(
                        format!("{path}/{field}"),
                        "at least one auth mechanism must be enabled",
                    );
                }
                let mut seen = HashSet::new();
                for (mechanism_index, mechanism) in mechanisms.iter().enumerate() {
                    if !seen.insert(*mechanism) {
                        self.push(
                            format!("{path}/{field}/{mechanism_index}"),
                            format!("duplicate auth mechanism `{}`", mechanism.as_token()),
                        );
                    }
                }
            }
            self.validate_mail_auth_http(&path, server);
            self.validate_mail_tls(&path, server);
            self.validate_mail_proxy(&path, server);
            if !(1..=100).contains(&server.max_errors) {
                self.push(
                    format!("{path}/maxErrors"),
                    "mail max_errors must be between 1 and 100",
                );
            }
        }
    }

    /// Validate the mail `auth_http` service declaration (Wave 9B).
    fn validate_mail_auth_http(&mut self, path: &str, server: &crate::config::MailServerConfig) {
        let Some(auth) = &server.auth_http else {
            return;
        };
        let endpoint = auth.endpoint();
        let valid_uri = endpoint.is_some_and(|endpoint| {
            let (host, rest) = match endpoint.split_once('/') {
                Some((host, rest)) => (host, Some(rest)),
                None => (endpoint.as_str(), None),
            };
            let (host, port) = match host.rsplit_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (host, None),
            };
            !host.is_empty()
                && !host.contains(char::is_whitespace)
                && port.is_none_or(|port| {
                    !port.is_empty()
                        && port.bytes().all(|byte| byte.is_ascii_digit())
                        && port.parse::<u16>().is_ok_and(|port| port != 0)
                })
                && rest.is_none_or(|rest| !rest.contains(char::is_whitespace))
        });
        if !valid_uri {
            self.push(
                format!("{path}/authHttp/uri"),
                "auth_http must be `host[:port][/path]` with an optional `http://` prefix",
            );
        }
        if !(1..=600_000).contains(&auth.timeout_ms) {
            self.push(
                format!("{path}/authHttp/timeoutMs"),
                "auth_http_timeout must be between 1 ms and 10 minutes",
            );
        }
        if let Some(host_header) = &auth.host_header {
            if host_header.is_empty()
                || host_header.len() > 255
                || host_header.contains(char::is_whitespace)
                || host_header.contains(char::is_control)
            {
                self.push(
                    format!("{path}/authHttp/hostHeader"),
                    "auth_http Host header must be a 1..=255 byte token",
                );
            }
        }
        for (header_index, header) in auth.headers.iter().enumerate() {
            let valid = header.split_once(':').is_some_and(|(name, value)| {
                !name.trim().is_empty()
                    && name
                        .trim()
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                    && !value.contains(['\r', '\n'])
                    && header.len() <= 4096
            });
            if !valid {
                self.push(
                    format!("{path}/authHttp/headers/{header_index}"),
                    "auth_http_header must be `Name: value` on one line",
                );
            }
        }
        if auth.pass_client_cert {
            // Forwarding the client certificate requires the mail plane to
            // model downstream client-certificate verification, which it does
            // not (mail `ssl_verify_client` fails closed). Accepting the flag
            // would silently drop it, so it fails closed instead.
            self.push(
                format!("{path}/authHttp/passClientCert"),
                "auth_http_pass_client_cert requires downstream client-certificate verification, which the mail plane does not model; remove the directive",
            );
        }
    }

    /// Validate the mail frontend TLS material (Wave 9B).
    fn validate_mail_tls(&mut self, path: &str, server: &crate::config::MailServerConfig) {
        let Some(tls) = &server.tls else {
            return;
        };
        if tls.certificate_file.trim().is_empty() {
            self.push(
                format!("{path}/tls/certificateFile"),
                "ssl_certificate path must not be empty",
            );
        }
        if tls.private_key_file.trim().is_empty() {
            self.push(
                format!("{path}/tls/privateKeyFile"),
                "ssl_certificate_key path must not be empty",
            );
        }
        if tls.protocols.is_empty() {
            self.push(
                format!("{path}/tls/protocols"),
                "ssl_protocols must declare TLSv1.2 and/or TLSv1.3",
            );
        }
        if matches!(tls.starttls, crate::config::MailStarttlsMode::Off) && !tls.implicit {
            self.push(
                format!("{path}/tls"),
                "mail TLS is declared but neither `listen … ssl` nor `starttls on|only` enables it",
            );
        }
        if tls.implicit && matches!(tls.starttls, crate::config::MailStarttlsMode::Only) {
            self.push(
                format!("{path}/tls/starttls"),
                "implicit TLS and `starttls only` are mutually exclusive",
            );
        }
    }

    /// Validate the mail backend relay policy (Wave 9B).
    fn validate_mail_proxy(&mut self, path: &str, server: &crate::config::MailServerConfig) {
        let proxy = &server.proxy;
        if !(512..=1024 * 1024).contains(&proxy.buffer_bytes) {
            self.push(
                format!("{path}/proxy/bufferBytes"),
                "proxy_buffer must be between 512 bytes and 1 MiB",
            );
        }
        if !(1..=86_400_000).contains(&proxy.timeout_ms) {
            self.push(
                format!("{path}/proxy/timeoutMs"),
                "proxy_timeout must be between 1 ms and 24 hours",
            );
        }
        if let Some(tls) = &proxy.tls {
            if tls.verify && tls.trusted_certificate_file.is_none() {
                // webpki roots are the default trust store, so an explicit
                // bundle is optional; nothing to reject here.
            }
            if tls.certificate_file.is_some() != tls.certificate_key_file.is_some() {
                self.push(
                    format!("{path}/proxy/tls"),
                    "mail backend TLS client identity requires both certificateFile and certificateKeyFile",
                );
            }
            if let Some(server_name) = &tls.server_name {
                if server_name.is_empty()
                    || server_name.len() > 253
                    || server_name.contains(|character: char| {
                        character.is_whitespace() || character.is_control()
                    })
                {
                    self.push(
                        format!("{path}/proxy/tls/serverName"),
                        "mail backend TLS server name must be a 1..=253 byte host name",
                    );
                }
            }
        }
        if proxy.proxy_protocol
            && !matches!(
                server.protocol,
                crate::config::MailServerProtocol::Smtp
                    | crate::config::MailServerProtocol::Pop3
                    | crate::config::MailServerProtocol::Imap
            )
        {
            // Every mail protocol can carry a backend PROXY header.
        }
    }

    fn register_ids(&mut self, config: &WebServerAppConfig) {
        for (index, listener) in config.listeners.iter().enumerate() {
            self.register_id(&listener.id, format!("/listeners/{index}/id"));
        }
        for (index, certificate) in config.certificates.iter().enumerate() {
            self.register_id(&certificate.id, format!("/certificates/{index}/id"));
        }
        for (index, policy) in config.tls_policies.iter().enumerate() {
            self.register_id(&policy.id, format!("/tlsPolicies/{index}/id"));
        }
        for (index, resolver) in config.resolvers.iter().enumerate() {
            self.register_id(&resolver.id, format!("/resolvers/{index}/id"));
        }
        for (index, resource) in config.resources.iter().enumerate() {
            self.register_id(resource.id(), format!("/resources/{index}/id"));
        }
        for (index, upstream) in config.upstreams.iter().enumerate() {
            self.register_id(&upstream.id, format!("/upstreams/{index}/id"));
        }
        for (host_index, virtual_host) in config.virtual_hosts.iter().enumerate() {
            self.register_id(&virtual_host.id, format!("/virtualHosts/{host_index}/id"));
            if let Some(referer) = &virtual_host.referer {
                validate_referer_policy(
                    self,
                    &format!("/virtualHosts/{host_index}/referer"),
                    referer,
                );
            }
            for (route_index, route) in virtual_host.routes.iter().enumerate() {
                if let Some(referer) = &route.referer {
                    validate_referer_policy(
                        self,
                        &format!("/virtualHosts/{host_index}/routes/{route_index}/referer"),
                        referer,
                    );
                }
            }
            for (route_index, route) in virtual_host.routes.iter().enumerate() {
                self.register_id(
                    &route.id,
                    format!("/virtualHosts/{host_index}/routes/{route_index}/id"),
                );
            }
        }
        for (index, stream) in config.streams.iter().enumerate() {
            self.register_id(&stream.id, format!("/streams/{index}/id"));
        }
        for (index, server) in config.mail.servers.iter().enumerate() {
            self.register_id(&server.id, format!("/mail/servers/{index}/id"));
        }
    }

    fn register_id(&mut self, id: &str, path: String) {
        if let Some(previous_path) = self.ids.insert(id.to_owned(), path.clone()) {
            self.push(
                path,
                format!("duplicate id {id}; first declared at {previous_path}"),
            );
        }
    }

    fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(ConfigDiagnostic::new(path, message));
        }
    }

    fn finish(self) -> Result<(), WebServerConfigError> {
        if self.diagnostics.is_empty() {
            Ok(())
        } else {
            Err(WebServerConfigError::Validation {
                diagnostics: self.diagnostics,
            })
        }
    }
}

/// Validate an nginx `valid_referers` policy: nginx rejects `none`/`blocked`
/// without at least one positive matcher, and every host/wildcard/regex must be
/// well formed (the runtime executes them, so a bad pattern fails at load).
/// nginx CGI-family `*_cache*` policy validation for the JSON/typed planes
/// (the nginx materializer validates the same rules at parse time).
fn validate_cgi_cache_config(
    validator: &mut SemanticValidator,
    path: &str,
    cache: &super::CgiCacheConfig,
    proxy_cache_enabled: bool,
    zone_names: &[String],
) {
    const USE_STALE_CONDITIONS: &[&str] = &[
        "error",
        "timeout",
        "invalid_header",
        "updating",
        "http_403",
        "http_404",
        "http_500",
        "http_502",
        "http_503",
        "http_504",
    ];
    if cache.zone.trim().is_empty() {
        validator.push(format!("{path}/zone"), "cache zone name must not be empty");
    } else if !zone_names.iter().any(|name| name == &cache.zone) {
        validator.push(
            format!("{path}/zone"),
            format!(
                "cache zone `{}` is not declared in cacheZoneNames",
                cache.zone
            ),
        );
    }
    if !proxy_cache_enabled {
        validator.push(
            path,
            "CGI response caching requires a cache store: declare a `<module>_cache_path`/`proxy_cache_path` (proxyCache.enabled in JSON)",
        );
    }
    if cache.key.is_empty() || cache.key.len() > 4096 {
        validator.push(
            format!("{path}/key"),
            "cache key template must be 1..4096 bytes",
        );
    }
    for (index, rule) in cache.valid.iter().enumerate() {
        if rule.seconds > 31_536_000 {
            validator.push(
                format!("{path}/valid/{index}/seconds"),
                "cache freshness must not exceed one year",
            );
        }
        for status in &rule.statuses {
            if !(100..=599).contains(status) {
                validator.push(
                    format!("{path}/valid/{index}/statuses"),
                    format!("invalid status code {status}"),
                );
            }
        }
    }
    if cache.methods.is_empty() {
        validator.push(format!("{path}/methods"), "cache methods must not be empty");
    }
    for method in &cache.methods {
        if method.is_empty()
            || !method
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
        {
            validator.push(
                format!("{path}/methods"),
                format!("invalid cache method `{method}`"),
            );
        }
    }
    if cache.min_uses == 0 {
        validator.push(format!("{path}/minUses"), "minUses must be at least 1");
    }
    if cache.lock_timeout_ms > 86_400_000 || cache.lock_age_ms > 86_400_000 {
        validator.push(path, "cache lock timeouts must not exceed 24 hours");
    }
    for condition in &cache.use_stale {
        if !USE_STALE_CONDITIONS.contains(&condition.as_str()) {
            validator.push(
                format!("{path}/useStale"),
                format!("unsupported use_stale condition `{condition}`"),
            );
        }
    }
    for (field, entries) in [("bypass", &cache.bypass), ("noCache", &cache.no_cache)] {
        for (index, entry) in entries.iter().enumerate() {
            let Some(name) = entry.strip_prefix('$') else {
                validator.push(
                    format!("{path}/{field}/{index}"),
                    format!("`{entry}` must be a `$variable`"),
                );
                continue;
            };
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                validator.push(
                    format!("{path}/{field}/{index}"),
                    format!("`{entry}` is not a valid variable reference"),
                );
            }
        }
    }
}

fn validate_referer_policy(
    validator: &mut SemanticValidator,
    path: &str,
    policy: &crate::config::model::RefererConfig,
) {
    if (policy.none || policy.blocked)
        && policy.hosts.is_empty()
        && policy.regexes.is_empty()
        && !policy.server_names
    {
        validator.push(
            path,
            "`valid_referers none`/`blocked` requires at least one host, wildcard, `~regex`, or `server_names` matcher (nginx rejects the configuration)",
        );
    }
    if policy.hosts.len() > 64 {
        validator.push(
            format!("{path}/hosts"),
            "at most 64 host entries are supported",
        );
    }
    if policy.regexes.len() > 16 {
        validator.push(
            format!("{path}/regexes"),
            "at most 16 `~regex` entries are supported",
        );
    }
    for (index, entry) in policy.hosts.iter().enumerate() {
        let entry_path = format!("{path}/hosts/{index}");
        let host = entry.host.as_str();
        let suffix = host.strip_prefix("*.").or_else(|| host.strip_prefix('.'));
        let valid_host = !host.is_empty()
            && host.len() <= 253
            && !host.chars().any(char::is_whitespace)
            && match suffix {
                Some(suffix) => {
                    !suffix.is_empty() && !suffix.contains('*') && !suffix.starts_with('.')
                }
                None => !host.contains('*'),
            };
        if !valid_host {
            validator.push(
                &entry_path,
                "invalid `valid_referers` host; supported forms are an exact host, `*.suffix`, `.suffix`, and a host with a `/path` prefix",
            );
        }
        if entry
            .path
            .as_deref()
            .is_some_and(|path| !path.starts_with('/'))
        {
            validator.push(&entry_path, "a referer URI prefix must start with `/`");
        }
    }
    for (index, pattern) in policy.regexes.iter().enumerate() {
        if crate::config::compile_referer_regex(pattern).is_none() {
            validator.push(
                format!("{path}/regexes/{index}"),
                "invalid `~regex` referer pattern (must compile with the runtime regex engine)",
            );
        }
    }
}

fn trusted_proxy_network_is_overbroad(network: &IpNet) -> bool {
    match network {
        IpNet::V4(network) => network.prefix_len() < 8,
        IpNet::V6(network) => {
            network.prefix_len() < 16
                || (network.contains(&Ipv4Addr::UNSPECIFIED.to_ipv6_mapped())
                    && network.contains(&Ipv4Addr::BROADCAST.to_ipv6_mapped()))
        }
    }
}

fn effective_pressure_threshold(limit: u64, reserve: u64, percent: u8) -> u64 {
    let percentage = ((limit as u128 * percent as u128) / 100).min(u64::MAX as u128) as u64;
    percentage.min(limit.saturating_sub(reserve))
}

fn valid_active_health_uri(uri: &str) -> bool {
    if !uri.starts_with('/')
        || uri.starts_with("//")
        || uri.contains('\\')
        || uri.contains('#')
        || uri.chars().any(char::is_control)
    {
        return false;
    }
    let Ok(base) = Url::parse("http://sdkwork-health.invalid/") else {
        return false;
    };
    base.join(uri).is_ok_and(|parsed| {
        parsed.scheme() == "http"
            && parsed.host_str() == Some("sdkwork-health.invalid")
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.fragment().is_none()
    })
}

/// Index the named locations of one virtual host (`location @name { … }`).
///
/// A named location is carried as an ordinary route whose exact match path is
/// `@name`: it can never be selected by a request path (request paths always
/// start with `/`), only by a bounded internal redirect from `try_files` or
/// `error_page`. The name namespace is per virtual host, matching nginx.
fn named_location_targets<'a>(
    validator: &mut SemanticValidator,
    host_path: &str,
    virtual_host: &'a super::VirtualHostConfig,
) -> HashMap<&'a str, &'a RouteConfig> {
    let mut named = HashMap::new();
    for (index, route) in virtual_host.routes.iter().enumerate() {
        let route_path = route.route_match.path.as_str();
        if !route_path.starts_with('@') {
            continue;
        }
        let path = format!("{host_path}/routes/{index}");
        if !valid_named_location_name(route_path) {
            validator.push(
                format!("{path}/match/path"),
                "named location name must be `@` followed by 1 to 256 letters, digits, `_`, `-`, `.`, or `~`",
            );
        }
        if route.route_match.path_type != RoutePathType::Exact {
            validator.push(
                format!("{path}/match/pathType"),
                "named location must use the exact match type (it is reached by name, not by request path)",
            );
        }
        if route.route_match.methods.is_some() {
            validator.push(
                format!("{path}/match/methods"),
                "named location cannot declare HTTP method restrictions",
            );
        }
        if named.insert(route_path, route).is_some() {
            validator.push(
                format!("{path}/match/path"),
                format!("duplicate named location {route_path}"),
            );
        }
        if route.resource_ref.is_empty() {
            validator.push(
                format!("{path}/resourceRef"),
                "named location requires a serving behavior",
            );
        }
    }
    named
}

fn valid_named_location_name(value: &str) -> bool {
    let Some(name) = value.strip_prefix('@') else {
        return false;
    };
    !name.is_empty()
        && value.len() <= 257
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'~'))
}

/// Validate one `error_page` target: a literal URI, or a named-location
/// reference that must resolve to a named location in the same virtual host.
fn validate_error_page_target(
    validator: &mut SemanticValidator,
    path: &str,
    page: &ErrorPageConfig,
    named_targets: &HashMap<&str, &RouteConfig>,
    resources: &HashMap<&str, &ResourceConfig>,
    from_named_location: bool,
) {
    if !page.uri.starts_with('@') {
        if page.uri.starts_with('$') {
            return;
        }
        if !page.uri.starts_with('/') {
            validator.push(
                format!("{path}/uri"),
                format!(
                    "error_page target `{}` must be an absolute `/path`, an `@named` location, or a variable template",
                    page.uri
                ),
            );
        }
        return;
    }
    if !valid_named_location_name(&page.uri) {
        validator.push(
            format!("{path}/uri"),
            "named error_page target must be `@` followed by 1 to 256 letters, digits, `_`, `-`, `.`, or `~`",
        );
        return;
    }
    let Some(target) = named_targets.get(page.uri.as_str()) else {
        validator.push(
            format!("{path}/uri"),
            format!(
                "error_page target {} is not a declared named location",
                page.uri
            ),
        );
        return;
    };
    if from_named_location {
        // One internal redirect per selection: a named location must resolve
        // directly instead of chaining into another one (this also makes a
        // cycle impossible to express). PRD §6 bounds the internal-redirect
        // state machine; depth is one.
        validator.push(
            format!("{path}/uri"),
            format!(
                "named location must not internally redirect to {}; the internal redirect depth bound is one",
                page.uri
            ),
        );
        return;
    }
    if resources
        .get(target.resource_ref.as_str())
        .is_some_and(|resource| matches!(resource, ResourceConfig::Proxy { .. }))
    {
        validator.push(
            format!("{path}/uri"),
            format!(
                "error_page cannot target the named proxy location {}: the request body is consumed before the error page runs; use `try_files ... {}` instead",
                page.uri, page.uri
            ),
        );
    }
}

/// The absolute path of a `unix:/absolute/path` upstream target URL.
pub fn unix_target_path(value: &str) -> Option<String> {
    let path = value.strip_prefix("unix:")?;
    if !path.starts_with('/')
        || path.contains('?')
        || path.contains('#')
        || path.contains('\\')
        || path.contains('\0')
        || path.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        || Path::new(path)
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return None;
    }
    Some(path.to_owned())
}

/// The request-variable vocabulary the bounded rewrite engine resolves.
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

fn rewrite_variable_known(name: &str, known: &[String]) -> bool {
    is_capture_variable(name)
        || REWRITE_REQUEST_VARIABLES.contains(&name)
        || known.iter().any(|variable| variable == name)
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

fn valid_rewrite_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn validate_rewrite_template_reference(
    validator: &mut SemanticValidator,
    path: String,
    template: &str,
    known: &[String],
) {
    let variables = match template_variables(template) {
        Ok(variables) => variables,
        Err(message) => {
            validator.push(path, message);
            return;
        }
    };
    for variable in variables {
        if !rewrite_variable_known(&variable.name, known) {
            validator.push(
                path.clone(),
                format!(
                    "rewrite-phase variable `${}` is not resolvable here; it must be a request variable, a declared `map` result, a regex capture, or an earlier `set`",
                    variable.name
                ),
            );
        }
    }
}

fn validate_rewrite_phase(
    validator: &mut SemanticValidator,
    path: &str,
    operations: &[RewritePhaseOpConfig],
    known: &mut Vec<String>,
) {
    if operations.len() > MAX_REWRITE_PHASE_OPS {
        validator.push(
            path,
            format!(
                "rewrite phase has {} instructions; maximum is {MAX_REWRITE_PHASE_OPS}",
                operations.len()
            ),
        );
        return;
    }
    for (index, operation) in operations.iter().enumerate() {
        let operation_path = format!("{path}/{index}");
        match operation {
            RewritePhaseOpConfig::Set { variable, value } => {
                let variable_path = format!("{operation_path}/variable");
                if !valid_rewrite_variable_name(variable) {
                    validator.push(
                        &variable_path,
                        "rewrite variable name must be letters, digits, or `_` and must not start with a digit",
                    );
                    continue;
                }
                validate_rewrite_template_reference(
                    validator,
                    format!("{operation_path}/value"),
                    value,
                    known,
                );
                if !known.iter().any(|existing| existing == variable) {
                    if known.len() >= MAX_REWRITE_PHASE_VARIABLES {
                        validator.push(
                            &variable_path,
                            format!("rewrite phase assigns more than {MAX_REWRITE_PHASE_VARIABLES} variables"),
                        );
                        continue;
                    }
                    known.push(variable.clone());
                }
            }
            RewritePhaseOpConfig::If { condition, body } => {
                validate_if_condition(validator, &operation_path, condition, known);
                validate_rewrite_phase(validator, &format!("{operation_path}/body"), body, known);
            }
            RewritePhaseOpConfig::Return {
                status,
                body,
                location,
            } => {
                if *status == 444 || !(200..=599).contains(status) {
                    validator.push(
                        format!("{operation_path}/status"),
                        "rewrite-phase return status must be 200..=599 and not 444",
                    );
                }
                if let Some(location) = location {
                    validate_redirect_location(
                        validator,
                        &format!("{operation_path}/location"),
                        location,
                    );
                }
                if let Some(body) = body {
                    if body.contains('$')
                        || !body
                            .bytes()
                            .all(|byte| byte == b'\t' || (0x20..=0x7e).contains(&byte))
                    {
                        validator.push(
                            format!("{operation_path}/body"),
                            "rewrite-phase return bodies must be printable literal text without variables",
                        );
                    }
                }
            }
            RewritePhaseOpConfig::Rewrite {
                pattern,
                replacement: _,
                flag: _,
            } => validate_regex_pattern(validator, format!("{operation_path}/pattern"), pattern),
            RewritePhaseOpConfig::AddHeader { name, value, .. } => {
                if let Err(message) =
                    super::model::CustomHeaderConfig::validate_declaration(name, value)
                {
                    validator.push(&operation_path, message);
                }
            }
        }
    }
}

fn validate_if_condition(
    validator: &mut SemanticValidator,
    path: &str,
    condition: &IfConditionConfig,
    known: &[String],
) {
    let variable = &condition.variable;
    if !valid_rewrite_variable_name(variable) || is_capture_variable(variable) {
        validator.push(
            format!("{path}/condition/variable"),
            "`if` left-hand side must be a named `$variable`",
        );
    } else if !rewrite_variable_known(variable, known) {
        validator.push(
            format!("{path}/condition/variable"),
            format!("`if` variable `${variable}` is not resolvable here"),
        );
    }
    let requires_value = !matches!(
        condition.operator,
        IfOperatorConfig::Truthy | IfOperatorConfig::NegatedTruthy
    );
    match (&condition.value, requires_value) {
        (None, true) => validator.push(
            format!("{path}/condition/value"),
            "this `if` operator requires a right-hand operand",
        ),
        (Some(_), false) => validator.push(
            format!("{path}/condition/value"),
            "truthiness `if` conditions take no right-hand operand",
        ),
        (Some(value), true) => match condition.operator {
            IfOperatorConfig::Regex
            | IfOperatorConfig::NotRegex
            | IfOperatorConfig::RegexIgnoreCase
            | IfOperatorConfig::NotRegexIgnoreCase => {
                let pattern = match condition.operator {
                    IfOperatorConfig::RegexIgnoreCase | IfOperatorConfig::NotRegexIgnoreCase => {
                        format!("(?i){value}")
                    }
                    _ => value.clone(),
                };
                validate_regex_pattern(validator, format!("{path}/condition/value"), &pattern);
            }
            _ => validate_rewrite_template_reference(
                validator,
                format!("{path}/condition/value"),
                value,
                known,
            ),
        },
        (None, false) => {}
    }
}

fn valid_document_root_allowlist_entry(prefix: &str) -> bool {
    !prefix.is_empty()
        && prefix.starts_with('/')
        && prefix != "/"
        && !prefix.ends_with('/')
        && !prefix.contains('\\')
        && !prefix.contains('\0')
        && !prefix.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        && !Path::new(prefix)
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
}

/// Whether `root` is a usable `nginx.unixSocketRoot` value.
fn valid_unix_socket_root(root: &str) -> bool {
    !root.is_empty()
        && root.starts_with('/')
        && root != "/"
        && !root.ends_with('/')
        && !root.contains('\\')
        && !root.contains('\0')
        && !root.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        && !Path::new(root)
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
}

/// Lexical confinement: the socket path must be a strict descendant of `root`.
pub fn unix_path_is_confined(root: &str, path: &str) -> bool {
    let Some(suffix) = path.strip_prefix(root) else {
        return false;
    };
    let Some(suffix) = suffix.strip_prefix('/') else {
        return false;
    };
    !suffix.is_empty()
        && !suffix.ends_with('/')
        && !suffix.contains("..")
        && !suffix.contains('\\')
        && !suffix.contains('\0')
}

fn upstream_has_unix_target(upstream: &UpstreamConfig) -> bool {
    upstream
        .targets
        .iter()
        .any(|target| unix_target_path(&target.url).is_some())
}

fn certificate_covers_any(
    policy_certificates: &[&super::CertificateConfig],
    server_name: &str,
) -> bool {
    policy_certificates.iter().any(|certificate| {
        certificate
            .server_names
            .iter()
            .any(|certificate_name| server_name_covers(certificate_name, server_name))
    })
}

// The route validator is the one place that needs the complete host context
// (host path, routes, resources, limits, rate-limit zones, named-location index,
// host error pages, recursion policy) and threading a context struct through it
// would only move the same nine values behind one more indirection; grouping them
// into a struct would still be a nine-field parameter bag. Keep the explicit
// arguments.
#[allow(clippy::too_many_arguments)]
/// Validate the subrequest family (`addition`, `auth_request`, `mirror`) on
/// one route: bounded URIs, MIME lists, auth-set variable vocabulary and the
/// mirror body restriction.
fn validate_subrequest_policy(validator: &mut SemanticValidator, path: &str, route: &RouteConfig) {
    const SUBREQUEST_URI_MAX: usize = 2048;
    let check_uri = |validator: &mut SemanticValidator, field: &str, uri: &str| {
        if !uri.starts_with('/') || uri.len() > SUBREQUEST_URI_MAX || uri.contains('$') {
            validator.push(
                field.to_owned(),
                format!(
                    "subrequest URI `{uri}` must be an absolute internal path without variables"
                ),
            );
        }
    };
    if let Some(addition) = &route.addition {
        if let Some(uri) = &addition.before {
            check_uri(validator, &format!("{path}/addition/before"), uri);
        }
        if let Some(uri) = &addition.after {
            check_uri(validator, &format!("{path}/addition/after"), uri);
        }
        if addition.types.is_empty() {
            validator.push(
                format!("{path}/addition/types"),
                "addition_types must declare at least one MIME type",
            );
        }
        for (index, media_type) in addition.types.iter().enumerate() {
            if media_type.is_empty()
                || !media_type
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"/.!#$&^_-+".contains(&byte))
            {
                validator.push(
                    format!("{path}/addition/types/{index}"),
                    format!("invalid addition_types entry `{media_type}`"),
                );
            }
        }
    }
    if let Some(auth_request) = &route.auth_request {
        check_uri(
            validator,
            &format!("{path}/authRequest/uri"),
            &auth_request.uri,
        );
        for (index, assignment) in auth_request.set.iter().enumerate() {
            let assignment_path = format!("{path}/authRequest/set/{index}");
            if assignment.variable.is_empty()
                || !assignment
                    .variable
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                validator.push(
                    format!("{assignment_path}/variable"),
                    format!(
                        "invalid auth_request_set variable `{}`",
                        assignment.variable
                    ),
                );
            }
            let mut remainder = assignment.value.as_str();
            while let Some(position) = remainder.find('$') {
                let after = &remainder[position + 1..];
                if let Some(rest) = after.strip_prefix('$') {
                    remainder = rest;
                    continue;
                }
                let end = after
                    .find(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_')
                    })
                    .unwrap_or(after.len());
                if end == 0 {
                    validator.push(
                        format!("{assignment_path}/value"),
                        "auth_request_set contains a lone `$`",
                    );
                    break;
                }
                let name = &after[..end];
                if name != "upstream_status"
                    && !(name.starts_with("upstream_http_") && name.len() > 14)
                {
                    validator.push(
                        format!("{assignment_path}/value"),
                        format!("auth_request_set value variable `${name}` is not executable"),
                    );
                }
                remainder = &after[end..];
            }
        }
    }
    if let Some(mirror) = &route.mirror {
        if mirror.targets.is_empty() {
            validator.push(
                format!("{path}/mirror/targets"),
                "mirror must declare at least one target",
            );
        }
        if mirror.targets.len() > 8 {
            validator.push(
                format!("{path}/mirror/targets"),
                "at most 8 mirror targets are executable",
            );
        }
        for (index, target) in mirror.targets.iter().enumerate() {
            check_uri(validator, &format!("{path}/mirror/targets/{index}"), target);
        }
        if mirror.request_body {
            validator.push(
                format!("{path}/mirror/requestBody"),
                "mirror_request_body on is not executable: the runtime does not replay request bodies into subrequests",
            );
        }
    }
}

// The route validator threads the host-level inherited policy (zones, named
// targets, error pages, rewrite variables, root allowlist) into every route;
// bundling them into a struct would hide that each is a distinct contract.
#[allow(clippy::too_many_arguments)]
fn validate_routes(
    validator: &mut SemanticValidator,
    host_path: &str,
    routes: &[RouteConfig],
    resources: &HashMap<&str, &ResourceConfig>,
    limits: &WebServerLimits,
    limit_req_zones: &[crate::config::LimitReqZoneConfig],
    named_targets: &HashMap<&str, &RouteConfig>,
    host_error_pages: &[ErrorPageConfig],
    recursive_error_pages: bool,
    server_variables: &[String],
    document_root_allowlist_configured: bool,
) {
    let zone_names = limit_req_zones
        .iter()
        .map(|zone| zone.name.as_str())
        .collect::<HashSet<_>>();
    // Overlap detection is keyed by `(path_type, path)` so a host with N routes
    // does not compare every pair. Only routes that share a matcher can overlap,
    // and their count is expected to be tiny (a duplicate is a configuration
    // error); the previous O(N²) scan was the 10k-route compile bottleneck.
    let mut routes_by_matcher: HashMap<(RoutePathType, &str), Vec<usize>> = HashMap::new();
    for (index, route) in routes.iter().enumerate() {
        let path = format!("{host_path}/routes/{index}");
        if route.max_request_body_bytes == Some(0) {
            validator.push(
                format!("{path}/maxRequestBodyBytes"),
                "maxRequestBodyBytes must be greater than zero",
            );
        }
        let mut route_variables = server_variables.to_vec();
        validate_rewrite_phase(
            validator,
            &format!("{path}/rewritePhase"),
            &route.rewrite_phase,
            &mut route_variables,
        );
        let configured_path = &route.route_match.path;
        let is_regex = matches!(
            route.route_match.path_type,
            RoutePathType::Regex | RoutePathType::RegexIgnoreCase
        );
        let is_named = configured_path.starts_with('@');
        if is_named {
            // Named locations are reached by name only; the `@name` string is
            // not a request path and must not be canonicalized as one.
        } else if is_regex {
            let pattern = if matches!(route.route_match.path_type, RoutePathType::RegexIgnoreCase) {
                format!("(?i){configured_path}")
            } else {
                configured_path.clone()
            };
            validate_regex_pattern(validator, format!("{path}/match/path"), &pattern);
        } else if let Err(error) = super::uri::validate_canonical_uri_path(
            configured_path,
            limits.max_decoded_path_bytes,
            limits.max_path_segments,
        ) {
            match super::uri::normalize_uri_path(
                configured_path,
                limits.max_decoded_path_bytes,
                limits.max_path_segments,
            ) {
                Ok(normalized) if normalized != *configured_path => validator.push(
                    format!("{path}/match/path"),
                    format!("route path must be canonical; use {normalized}"),
                ),
                _ => validator.push(
                    format!("{path}/match/path"),
                    match error {
                        super::uri::UriPathNormalizationError::Invalid => {
                            format!("route path {configured_path:?} is not a valid canonical Path")
                        }
                        super::uri::UriPathNormalizationError::TooLong => {
                            "route path exceeds canonical Path budgets".to_owned()
                        }
                    },
                ),
            }
        }
        for (rule_index, rule) in route.rewrite.iter().enumerate() {
            let pattern = &rule.pattern;
            validate_regex_pattern(
                validator,
                format!("{path}/rewrite/{rule_index}/pattern"),
                pattern,
            );
            if rule.replacement.is_empty() {
                validator.push(
                    format!("{path}/rewrite/{rule_index}/replacement"),
                    "rewrite replacement must not be empty",
                );
            }
        }
        let resource = resources.get(route.resource_ref.as_str()).copied();
        if resource.is_none() {
            validator.push(
                format!("{path}/resourceRef"),
                format!("unknown resource {}", route.resource_ref),
            );
        }
        validate_subrequest_policy(validator, &path, route);
        if let Some(ResourceConfig::Static {
            dynamic_root: Some(template),
            ..
        }) = resource
        {
            if !document_root_allowlist_configured {
                validator.push(
                    format!("{path}/resourceRef"),
                    "a request-time `root`/`alias $variable` requires `nginx.documentRootAllowlist` in the typed configuration; without it the evaluated path cannot be confined",
                );
            } else {
                validate_rewrite_template_reference(
                    validator,
                    format!("{path}/resourceRef"),
                    template,
                    &route_variables,
                );
            }
        }
        // `try_files $uri @fallback;` materializes on a static resource as a
        // named fallback target. It must resolve inside this virtual host and
        // a named location must never chain into another named location.
        if let Some(ResourceConfig::Static {
            spa_fallback: Some(target),
            ..
        }) = resource
        {
            if target.starts_with('@') {
                if !valid_named_location_name(target) {
                    validator.push(
                        format!("{path}/resourceRef"),
                        "named try_files fallback must be `@` followed by 1 to 256 letters, digits, `_`, `-`, `.`, or `~`",
                    );
                } else if is_named {
                    validator.push(
                        &path,
                        format!(
                            "named location must not internally redirect to {target}; the internal redirect depth bound is one"
                        ),
                    );
                } else if !named_targets.contains_key(target.as_str()) {
                    validator.push(
                        format!("{path}/resourceRef"),
                        format!(
                            "try_files named fallback {target} is not a declared named location"
                        ),
                    );
                }
            }
        }
        if is_named {
            match resource {
                None => {}
                Some(
                    ResourceConfig::Static { .. }
                    | ResourceConfig::Proxy { .. }
                    | ResourceConfig::Respond { .. }
                    | ResourceConfig::Redirect { .. },
                ) => {}
                Some(_) => validator.push(
                    format!("{path}/resourceRef"),
                    "named location serving behavior must be static, proxy, return, or try_files",
                ),
            }
        }
        for (page_index, page) in route.error_pages.iter().enumerate() {
            validate_error_page_target(
                validator,
                &format!("{path}/errorPages/{page_index}"),
                page,
                named_targets,
                resources,
                is_named,
            );
        }
        if is_named && recursive_error_pages {
            // `recursive_error_pages on` lets a failure while serving the named
            // target re-enter error-page mapping. Rejecting named -> named
            // error-page edges (including a self-edge) makes a cycle or an
            // unbounded chain impossible to express; without recursion the
            // runtime already stops after one hop, so the same edge is a
            // bounded, faithful mapping.
            let effective_pages: &[ErrorPageConfig] = if route.error_pages.is_empty() {
                host_error_pages
            } else {
                &route.error_pages
            };
            for (page_index, page) in effective_pages.iter().enumerate() {
                if page.uri.starts_with('@') {
                    validator.push(
                        format!("{path}/errorPages/{page_index}"),
                        format!(
                            "named location would internally redirect to {} while `recursiveErrorPages` is enabled; this can form a cycle and the internal redirect depth bound is one",
                            page.uri
                        ),
                    );
                }
            }
        }
        validate_mime_policy(
            validator,
            &format!("{path}/mimeTypes"),
            route.mime_types.as_deref(),
            route.default_type.as_deref(),
        );
        for (header_index, header) in route.response_headers.iter().enumerate() {
            let header_path = format!("{path}/responseHeaders/{header_index}");
            if HOP_BY_HOP_HEADERS.contains(&header.name.to_ascii_lowercase().as_str()) {
                validator.push(
                    format!("{header_path}/name"),
                    format!(
                        "hop-by-hop header {} is forbidden in responseHeaders",
                        header.name
                    ),
                );
            }
            // `auth_request_set` variables declared on this route are request
            // variables for its response headers.
            let auth_variables: Vec<String> = route
                .auth_request
                .as_ref()
                .map(|auth_request| {
                    auth_request
                        .set
                        .iter()
                        .map(|assignment| assignment.variable.clone())
                        .collect()
                })
                .unwrap_or_default();
            validate_custom_header_declaration(validator, &header_path, header, &auth_variables);
        }
        for (rule_index, rule) in route.limit_req.iter().enumerate() {
            if !zone_names.contains(rule.zone.as_str()) {
                validator.push(
                    format!("{path}/limitReq/{rule_index}/zone"),
                    format!("unknown limitReq zone {}", rule.zone),
                );
            }
        }
        for (rule_index, rule) in route.access.iter().enumerate() {
            let network = rule.network.trim();
            if network.is_empty()
                || (!network.eq_ignore_ascii_case("all")
                    && network.parse::<IpAddr>().is_err()
                    && network.parse::<IpNet>().is_err())
            {
                validator.push(
                    format!("{path}/access/{rule_index}/network"),
                    "access network must be `all`, an IP, or a CIDR",
                );
            }
        }
        validate_cache_policy(
            validator,
            &format!("{path}/cachePolicy"),
            route.cache_policy.as_ref(),
        );

        if is_named {
            continue;
        }
        let matcher_key = (route.route_match.path_type, route.route_match.path.as_str());
        if let Some(earlier) = routes_by_matcher.get(&matcher_key) {
            for &other_index in earlier {
                let other = &routes[other_index];
                if methods_overlap(
                    route.route_match.methods.as_deref(),
                    other.route_match.methods.as_deref(),
                ) {
                    validator.push(
                        &path,
                        format!(
                            "route overlaps route {} at {host_path}/routes/{other_index}",
                            other.id
                        ),
                    );
                }
            }
        }
        routes_by_matcher
            .entry(matcher_key)
            .or_default()
            .push(index);
    }
}

fn methods_overlap(left: Option<&[String]>, right: Option<&[String]>) -> bool {
    match (left, right) {
        (None, _) | (_, None) => true,
        (Some(left), Some(right)) => left.iter().any(|method| right.contains(method)),
    }
}

fn validate_relative_path(
    validator: &mut SemanticValidator,
    path: &str,
    value: &str,
    allow_nested: bool,
) {
    let candidate = Path::new(value);
    let has_unsafe_component = candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if value.contains('\\')
        || value.contains('\0')
        || has_unsafe_component
        || (!allow_nested && candidate.components().count() != 1)
    {
        validator.push(
            path,
            "path must be a safe relative path without parent, root, prefix, backslash, or NUL",
        );
    }
}

/// Validate one upstream TLS material file reference.
///
/// JSON/TOML policies name conf-directory-relative files; nginx materialization
/// resolves them against the loaded nginx configuration directory first and
/// hands the compiled model absolute paths (the `NginxConf` compile base is
/// `/`). Both spellings are legal here; `CompiledWebServerApp::compile` still
/// enforces that the canonical file stays inside the format's base directory.
fn validate_upstream_tls_material_path(validator: &mut SemanticValidator, path: &str, value: &str) {
    let candidate = Path::new(value);
    let has_parent = candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir));
    if value.is_empty()
        || value.contains('\\')
        || value.contains('\0')
        || has_parent
        || (candidate.is_absolute() && value.starts_with("//"))
    {
        validator.push(
            path,
            "TLS material path must not be empty, contain parent traversal, a backslash, or NUL",
        );
    }
}

fn validate_redirect_location(validator: &mut SemanticValidator, path: &str, location: &str) {
    if location.starts_with('/')
        && !location.starts_with("//")
        && location.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return;
    }
    // nginx-compatible `return` templates expand at request time
    // (`$host`/`$request_uri`/`$scheme`).
    if redirect_template_is_well_formed(location) {
        return;
    }
    match Url::parse(location) {
        Ok(url)
            if matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none() => {}
        _ => validator.push(
            path,
            "redirect location must be a safe absolute path or http/https URL without credentials",
        ),
    }
}

/// Accepts nginx `return` URL templates built from the supported variable
/// subset, optionally prefixed by a literal scheme/host.
fn redirect_template_is_well_formed(location: &str) -> bool {
    let mut remainder = location;
    let mut saw_variable = false;
    while let Some(index) = remainder.find('$') {
        saw_variable = true;
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
    saw_variable && remainder.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

/// Validate a resolved `expires` / `etag` / `if_modified_since` policy.
///
/// The parsers already reject malformed nginx syntax, but the JSON and typed
/// TOML planes can carry a hand-written policy straight into the model, so the
/// runtime invariants are enforced here as well: a daily (`@`) offset must stay
/// inside one day (nginx refuses anything above 24h), and every other offset
/// must stay small enough that the computed `Expires` date remains a formatable
/// HTTP date.
fn validate_cache_policy(
    validator: &mut SemanticValidator,
    path: &str,
    policy: Option<&CachePolicyConfig>,
) {
    let Some(policy) = policy else {
        return;
    };
    if policy.expires == ExpiresMode::Daily {
        if !(0..=86_400).contains(&policy.expires_seconds) {
            validator.push(
                path,
                "a daily `expires @<time>` offset must be between 0 and 24 hours",
            );
        }
        return;
    }
    // 253402300799 == 9999-12-31T23:59:59Z, the last date `httpdate` formats.
    if policy.expires_seconds.unsigned_abs() > 253_402_300_799 {
        validator.push(
            path,
            "expires offset is outside the range an HTTP date can represent",
        );
    }
}

fn validate_security_headers(
    validator: &mut SemanticValidator,
    host_path: &str,
    security_headers: Option<&SecurityHeadersConfig>,
) {
    let Some(security_headers) = security_headers else {
        return;
    };
    for (index, header) in security_headers.custom_headers.iter().enumerate() {
        let name = header.name.to_ascii_lowercase();
        if HOP_BY_HOP_HEADERS.contains(&name.as_str()) {
            validator.push(
                format!("{host_path}/securityHeaders/customHeaders/{index}/name"),
                format!("hop-by-hop header {name} is forbidden in customHeaders"),
            );
        }
        validate_custom_header_declaration(
            validator,
            &format!("{host_path}/securityHeaders/customHeaders/{index}"),
            header,
            &[],
        );
    }
}

/// Validate one resolved nginx `types` / `default_type` pair: a bounded,
/// well-formed MIME table and a `type/subtype` default.
fn validate_mime_policy(
    validator: &mut SemanticValidator,
    path: &str,
    mime_types: Option<&[super::MimeTypeConfig]>,
    default_type: Option<&str>,
) {
    if let Some(mime_types) = mime_types {
        for (index, entry) in mime_types.iter().enumerate() {
            if !is_valid_media_type(&entry.mime) {
                validator.push(
                    format!("{path}/{index}/mime"),
                    format!("MIME type `{}` is not a `type/subtype` token", entry.mime),
                );
            }
            if entry.extensions.is_empty() {
                validator.push(
                    format!("{path}/{index}/extensions"),
                    "a types row requires at least one extension",
                );
            }
            for (extension_index, extension) in entry.extensions.iter().enumerate() {
                if extension.is_empty()
                    || extension.starts_with('.')
                    || extension.len() > 64
                    || !extension.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'+' | b'.')
                    })
                {
                    validator.push(
                        format!("{path}/{index}/extensions/{extension_index}"),
                        format!(
                            "invalid file extension `{extension}`; use the bare extension without a leading dot"
                        ),
                    );
                }
            }
        }
    }
    if let Some(default_type) = default_type {
        if !is_valid_media_type(default_type) {
            validator.push(
                path,
                format!("default_type `{default_type}` is not a `type/subtype` token"),
            );
        }
    }
}

fn is_valid_media_type(value: &str) -> bool {
    let Some((media_type, subtype)) = value.split_once('/') else {
        return false;
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
    token_is_valid(media_type) && token_is_valid(subtype)
}

/// Validate one `customHeaders` / `responseHeaders` declaration: HTTP field
/// name shape, bounded value, and only the `$variables` the response-header
/// layer expands. The nginx materializer validates the same rules at parse
/// time; this covers the JSON and TOML configuration planes as well.
fn validate_custom_header_declaration(
    validator: &mut SemanticValidator,
    path: &str,
    header: &super::CustomHeaderConfig,
    extra_variables: &[String],
) {
    let mut map_variables: Vec<&str> = validator.map_variables.iter().map(String::as_str).collect();
    map_variables.extend(extra_variables.iter().map(String::as_str));
    if let Err(message) = super::CustomHeaderConfig::validate_declaration_with_variables(
        &header.name,
        &header.value,
        &map_variables,
    ) {
        validator.push(path, message);
    }
}

/// Headers the runtime owns (framing, hop-by-hop fields, the representation
/// metadata it derives) and refuses to let a configuration duplicate.
const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
    "content-encoding",
    "content-type",
];

fn validate_provider_resource_uuid(validator: &mut SemanticValidator, path: &str, value: &str) {
    if value.is_empty()
        || value.len() > 128
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        validator.push(
            path,
            "providerResourceUuid must be 1..=128 characters without whitespace or control characters",
        );
    }
}

fn validate_provider_subpath(validator: &mut SemanticValidator, path: &str, value: &str) {
    let candidate = Path::new(value);
    let has_unsafe_component = candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)));
    if !value.starts_with('/')
        || value.starts_with("//")
        || value.contains('\\')
        || value.contains('\0')
        || has_unsafe_component
    {
        validator.push(
            path,
            "resourceSubpath must be a safe absolute provider path without parent, double-slash, backslash, or NUL",
        );
    }
}

pub fn normalize_server_name(value: &str) -> Option<String> {
    let normalized = value.trim_end_matches('.').to_ascii_lowercase();
    let dns_name = normalized.strip_prefix("*.").unwrap_or(&normalized);
    if dns_name.is_empty() || dns_name.len() > 253 || dns_name.contains('*') {
        return None;
    }
    if dns_name.parse::<IpAddr>().is_ok() {
        return (!normalized.starts_with("*.")).then_some(normalized);
    }
    if dns_name.split('.').all(valid_dns_label) {
        Some(normalized)
    } else {
        None
    }
}

fn valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// The single definition of a server name a certificate can be selected for.
///
/// [`normalize_server_name`] also accepts an IP literal, because an HTTP virtual
/// host may legitimately be addressed by one. A certificate cannot be: it is
/// chosen through SNI, and RFC 6066 confines that extension to DNS names.
/// Configuration validation, assignment snapshot validation and the certificate
/// index all call this function, so a name that validates is a name the data
/// plane can index - which is what keeps a snapshot that passed validation from
/// failing to install.
pub fn normalize_tls_server_name(value: &str) -> Option<String> {
    normalize_server_name(value).filter(|name| name.parse::<IpAddr>().is_err())
}

/// Whether `*.suffix` covers exactly one additional label of `server_name`.
///
/// `*.example.com` covers `www.example.com` but neither `example.com` nor
/// `a.b.example.com`. A wildcard pins the number of labels below its suffix, so
/// two wildcards can never cover one name and no lookup order between them is
/// observable. Certificate SAN coverage, SNI selection and website host
/// selection all answer this same question, so they share this one rule rather
/// than each deciding it for itself; this is its only implementation.
pub fn wildcard_server_name_covers(suffix: &str, server_name: &str) -> bool {
    server_name.strip_suffix(suffix).is_some_and(|prefix| {
        prefix.ends_with('.') && prefix.len() > 1 && !prefix[..prefix.len() - 1].contains('.')
    })
}

pub fn server_name_covers(certificate_name: &str, server_name: &str) -> bool {
    let Some(certificate_name) = normalize_server_name(certificate_name) else {
        return false;
    };
    let Some(server_name) = normalize_server_name(server_name) else {
        return false;
    };
    if certificate_name == server_name {
        return true;
    }
    let Some(suffix) = certificate_name.strip_prefix("*.") else {
        return false;
    };
    wildcard_server_name_covers(suffix, &server_name)
}
