use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr},
    path::{Component, Path},
};

use ipnet::IpNet;
use url::Url;

use super::{
    is_supported_upstream_allowed_cidr, upstream_ip_is_allowed, AppDomainFallbackLookup,
    CertificateSource, ConfigDiagnostic, ListenerProtocol, ResourceConfig, RouteConfig,
    RoutePathType, SecurityHeadersConfig, StreamTargetConfig, TlsVersion, UpstreamConfig,
    UpstreamLoadBalancingStrategy, UpstreamTlsTrustMode, UsageMeteringChannel, WebServerAppConfig,
    WebServerConfigError, WebServerLimits,
};

const MAX_DIAGNOSTICS: usize = 128;
const MAX_TOTAL_ROUTES: usize = 10_000;
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
}

impl SemanticValidator {
    fn validate(&mut self, config: &WebServerAppConfig) {
        if config.schema_version != 1 {
            self.push("/schemaVersion", "only schemaVersion 1 is supported");
        }
        if config.kind != "sdkwork.webserver.app" {
            self.push("/kind", "kind must be sdkwork.webserver.app");
        }
        if config.listeners.is_empty() && config.streams.is_empty() {
            self.push(
                "",
                "at least one HTTP listener or one stream server is required",
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
        if config.nginx.unknown_directive_policy != "error" {
            self.push(
                "/nginx/unknownDirectivePolicy",
                "Rust activation requires unknownDirectivePolicy=error",
            );
        }

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
            if listener.tls_policy_ref.is_some() && listener.tls_runtime.is_some() {
                self.push(
                    format!("{path}/tlsRuntime"),
                    "tlsRuntime cannot be combined with tlsPolicyRef",
                );
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

        for (index, certificate) in config.certificates.iter().enumerate() {
            let path = format!("/certificates/{index}");
            let mut normalized_names = HashSet::new();
            for (name_index, server_name) in certificate.server_names.iter().enumerate() {
                match normalize_server_name(server_name) {
                    Some(normalized) => {
                        if !normalized_names.insert(normalized) {
                            self.push(
                                format!("{path}/serverNames/{name_index}"),
                                "duplicate certificate server name after DNS normalization",
                            );
                        }
                    }
                    None => self.push(
                        format!("{path}/serverNames/{name_index}"),
                        "invalid exact or leading-wildcard DNS server name",
                    ),
                }
            }
            let CertificateSource::ProtectedFile {
                certificate_file,
                private_key_file,
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
            if policy.minimum_version < TlsVersion::Tls12 {
                self.push(&path, "TLS versions below 1.2 are forbidden");
            }
            if policy.minimum_version != TlsVersion::Tls12
                || policy.maximum_version != TlsVersion::Tls13
            {
                self.push(
                    &path,
                    "REQ-2026-0003 currently supports the TLS 1.2 through TLS 1.3 policy only",
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
            match resource {
                ResourceConfig::Static {
                    root,
                    index_files,
                    spa_fallback,
                    follow_symlinks,
                    ..
                } => {
                    // Absolute roots (nginx `root` compatibility) are used
                    // directly; relative roots stay chrooted.
                    if !root.starts_with('/') {
                        validate_relative_path(self, &format!("{path}/root"), root, true);
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
                        validate_relative_path(self, &format!("{path}/spaFallback"), file, false);
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
                        validate_relative_path(self, &format!("{path}/spaFallback"), file, false);
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
                        let normalized = url.as_str().trim_end_matches('/').to_ascii_lowercase();
                        if !target_urls.insert(normalized) {
                            self.push(&target_path, "duplicate upstream target URL");
                        }
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
                        if let Some(ip) = url.host_str().and_then(|host| host.parse::<IpAddr>().ok())
                        {
                            if !upstream_ip_is_allowed(
                                ip,
                                &upstream.address_policy.allowed_cidrs,
                            ) {
                                self.push(
                                    &target_path,
                                    "literal upstream IP is forbidden unless its restricted range is explicitly authorized",
                                );
                            }
                        }
                    }
                    _ => self.push(
                        &target_path,
                        "upstream URL must be an http/https origin without credentials, query, or fragment",
                    ),
                }
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
                    validate_relative_path(
                        self,
                        &format!("{path}/tls/caCertificateFiles/{file_index}"),
                        file,
                        true,
                    );
                }
                match (
                    tls.client_certificate_file.as_deref(),
                    tls.client_private_key_file.as_deref(),
                ) {
                    (Some(certificate), Some(private_key)) => {
                        validate_relative_path(
                            self,
                            &format!("{path}/tls/clientCertificateFile"),
                            certificate,
                            true,
                        );
                        validate_relative_path(
                            self,
                            &format!("{path}/tls/clientPrivateKeyFile"),
                            private_key,
                            true,
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

        let mut host_owners: HashMap<(String, String), String> = HashMap::new();
        let mut total_routes = 0usize;
        for (index, virtual_host) in config.virtual_hosts.iter().enumerate() {
            let path = format!("/virtualHosts/{index}");
            for (listener_index, listener_ref) in virtual_host.listener_refs.iter().enumerate() {
                if !listeners.contains_key(listener_ref.as_str()) {
                    self.push(
                        format!("{path}/listenerRefs/{listener_index}"),
                        format!("unknown listener {listener_ref}"),
                    );
                    continue;
                }
                for (name_index, server_name) in virtual_host.server_names.iter().enumerate() {
                    let Some(normalized) = normalize_server_name(server_name) else {
                        self.push(
                            format!("{path}/serverNames/{name_index}"),
                            "invalid exact or leading-wildcard DNS server name",
                        );
                        continue;
                    };
                    let key = (listener_ref.clone(), normalized.clone());
                    if let Some(owner) = host_owners.insert(key, virtual_host.id.clone()) {
                        if owner != virtual_host.id {
                            self.push(
                                format!("{path}/serverNames/{name_index}"),
                                format!(
                                    "server name {normalized} on listener {listener_ref} is already owned by {owner}"
                                ),
                            );
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
            );
            validate_security_headers(self, &path, virtual_host.security_headers.as_ref());
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
                .collect::<Vec<_>>();
            for virtual_host in config.virtual_hosts.iter().filter(|virtual_host| {
                virtual_host
                    .listener_refs
                    .iter()
                    .any(|listener_ref| listener_ref == &listener.id)
            }) {
                for server_name in &virtual_host.server_names {
                    if normalize_server_name(server_name)
                        .is_some_and(|name| name.parse::<IpAddr>().is_ok())
                    {
                        self.push(
                            format!("/listeners/{listener_index}/tlsPolicyRef"),
                            format!(
                                "strict SNI certificate selection requires a DNS server name, not IP address {server_name}"
                            ),
                        );
                        continue;
                    }
                    if !policy_certificates.iter().any(|certificate| {
                        certificate.server_names.iter().any(|certificate_name| {
                            server_name_covers(certificate_name, server_name)
                        })
                    }) {
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
        }
        match &fallback.lookup {
            AppDomainFallbackLookup::Embedded => {}
            AppDomainFallbackLookup::Http { endpoint, .. } => {
                if endpoint.trim().is_empty()
                    || !endpoint.starts_with("https://") && !endpoint.starts_with("http://")
                {
                    self.push(
                        "/appDomainFallback/lookup/endpoint",
                        "http lookup requires an absolute http(s) endpoint URL",
                    );
                }
            }
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
            match &stream.target {
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

fn validate_routes(
    validator: &mut SemanticValidator,
    host_path: &str,
    routes: &[RouteConfig],
    resources: &HashMap<&str, &ResourceConfig>,
    limits: &WebServerLimits,
    limit_req_zones: &[crate::config::LimitReqZoneConfig],
) {
    let zone_names = limit_req_zones
        .iter()
        .map(|zone| zone.name.as_str())
        .collect::<HashSet<_>>();
    for (index, route) in routes.iter().enumerate() {
        let path = format!("{host_path}/routes/{index}");
        let configured_path = &route.route_match.path;
        let is_regex = matches!(
            route.route_match.path_type,
            RoutePathType::Regex | RoutePathType::RegexIgnoreCase
        );
        if is_regex {
            let pattern = if matches!(route.route_match.path_type, RoutePathType::RegexIgnoreCase) {
                format!("(?i){configured_path}")
            } else {
                configured_path.clone()
            };
            if let Err(error) = regex::Regex::new(&pattern) {
                validator.push(
                    format!("{path}/match/path"),
                    format!("invalid regex location pattern: {error}"),
                );
            }
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
                            "route path is not a valid canonical Path"
                        }
                        super::uri::UriPathNormalizationError::TooLong => {
                            "route path exceeds canonical Path budgets"
                        }
                    },
                ),
            }
        }
        for (rule_index, rule) in route.rewrite.iter().enumerate() {
            let pattern = &rule.pattern;
            if let Err(error) = regex::Regex::new(pattern) {
                validator.push(
                    format!("{path}/rewrite/{rule_index}/pattern"),
                    format!("invalid rewrite pattern: {error}"),
                );
            }
            if rule.replacement.is_empty() {
                validator.push(
                    format!("{path}/rewrite/{rule_index}/replacement"),
                    "rewrite replacement must not be empty",
                );
            }
        }
        if !resources.contains_key(route.resource_ref.as_str()) {
            validator.push(
                format!("{path}/resourceRef"),
                format!("unknown resource {}", route.resource_ref),
            );
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

        for (other_index, other) in routes.iter().take(index).enumerate() {
            if route.route_match.path_type == other.route_match.path_type
                && route.route_match.path == other.route_match.path
                && methods_overlap(
                    route.route_match.methods.as_deref(),
                    other.route_match.methods.as_deref(),
                )
            {
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
    }
}

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
    "strict-transport-security",
    "x-frame-options",
    "content-security-policy",
    "referrer-policy",
    "x-content-type-options",
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
    server_name
        .strip_suffix(suffix)
        .is_some_and(|prefix| prefix.ends_with('.') && !prefix[..prefix.len() - 1].contains('.'))
}
