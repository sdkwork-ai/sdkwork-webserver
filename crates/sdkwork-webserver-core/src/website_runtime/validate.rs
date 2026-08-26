use std::collections::{HashMap, HashSet};

use url::Host;

use crate::{normalize_uri_path, ConfigDiagnostic};

use super::{
    WebsiteBindingAction, WebsiteHandler, WebsiteProviderType, WebsiteRuntimeDescriptor,
    WebsiteRuntimeDescriptorError, WebsiteRuntimeEnvironment, WebsiteVariantRuleMatcher,
    WEBSITE_RUNTIME_DESCRIPTOR_KIND, WEBSITE_RUNTIME_SCHEMA_VERSION,
};

const MAX_DIAGNOSTICS: usize = 128;
const HARD_MAX_BINDINGS: usize = 1_024;
const HARD_MAX_VARIANTS: usize = 64;
const HARD_MAX_VARIANT_RULES: usize = 1_024;
const HARD_MAX_RESOURCES: usize = 512;
const HARD_MAX_MOUNTS: usize = 2_048;
const HARD_MAX_INDEX_FILES_PER_MOUNT: usize = 16;
const HARD_MAX_PATH_BYTES: usize = 4_096;
const HARD_MAX_PATH_SEGMENTS: usize = 128;
const HARD_MAX_PROVIDER_TIMEOUT_MS: u64 = 60_000;
const HARD_MAX_CACHE_TTL_SECONDS: u32 = 604_800;
const HARD_MAX_OBJECT_BYTES: u64 = 1_099_511_627_776;

pub(crate) fn validate_website_runtime_descriptor(
    descriptor: &WebsiteRuntimeDescriptor,
) -> Result<(), WebsiteRuntimeDescriptorError> {
    let mut validator = WebsiteRuntimeValidator::default();
    validator.validate(descriptor);
    validator.finish()
}

pub fn normalize_website_hostname(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('.');
    if value.is_empty() || value.contains(['/', '\\', ':']) {
        return None;
    }
    let (wildcard, dns_name) = match value.strip_prefix("*.") {
        Some(suffix) => (true, suffix),
        None => (false, value),
    };
    if dns_name.is_empty() || dns_name.contains('*') {
        return None;
    }
    let Host::Domain(domain) = Host::parse(dns_name).ok()? else {
        return None;
    };
    if domain.len() > 253 || domain.split('.').any(|label| label.len() > 63) {
        return None;
    }
    Some(if wildcard {
        format!("*.{}", domain.to_ascii_lowercase())
    } else {
        domain.to_ascii_lowercase()
    })
}

#[derive(Default)]
struct WebsiteRuntimeValidator {
    diagnostics: Vec<ConfigDiagnostic>,
}

impl WebsiteRuntimeValidator {
    fn validate(&mut self, descriptor: &WebsiteRuntimeDescriptor) {
        self.validate_envelope(descriptor);
        self.validate_limits(descriptor);

        let variants = self.index_unique(
            "/variants",
            descriptor
                .variants
                .iter()
                .map(|variant| variant.variant_uuid.as_str()),
        );
        let resources = self.index_unique(
            "/resources",
            descriptor
                .resources
                .iter()
                .map(|resource| resource.resource_uuid.as_str()),
        );
        self.index_unique(
            "/bindings",
            descriptor
                .bindings
                .iter()
                .map(|binding| binding.binding_uuid.as_str()),
        );
        self.index_unique(
            "/variantRules",
            descriptor
                .variant_rules
                .iter()
                .map(|rule| rule.rule_uuid.as_str()),
        );
        self.index_unique(
            "/mounts",
            descriptor
                .mounts
                .iter()
                .map(|mount| mount.mount_uuid.as_str()),
        );

        self.require_canonical_order(
            "/bindings",
            descriptor
                .bindings
                .iter()
                .map(|binding| binding.binding_uuid.as_str()),
        );
        self.require_canonical_order(
            "/variants",
            descriptor
                .variants
                .iter()
                .map(|variant| variant.variant_uuid.as_str()),
        );
        self.require_canonical_order(
            "/variantRules",
            descriptor
                .variant_rules
                .iter()
                .map(|rule| rule.rule_uuid.as_str()),
        );
        self.require_canonical_order(
            "/resources",
            descriptor
                .resources
                .iter()
                .map(|resource| resource.resource_uuid.as_str()),
        );
        self.require_canonical_order(
            "/mounts",
            descriptor
                .mounts
                .iter()
                .map(|mount| mount.mount_uuid.as_str()),
        );

        if !variants.contains_key(descriptor.app_default_variant_uuid.as_str()) {
            self.push(
                "/appDefaultVariantUuid",
                "site default Variant does not exist",
            );
        }
        self.validate_bindings(descriptor, &variants);
        self.validate_variant_rules(descriptor, &variants);
        self.validate_resources(descriptor);
        self.validate_mounts(descriptor, &variants, &resources);
        self.validate_security_policy(descriptor);
    }

    fn validate_envelope(&mut self, descriptor: &WebsiteRuntimeDescriptor) {
        if descriptor.schema_version != WEBSITE_RUNTIME_SCHEMA_VERSION {
            self.push(
                "/schemaVersion",
                format!("only {WEBSITE_RUNTIME_SCHEMA_VERSION} is supported"),
            );
        }
        if descriptor.kind != WEBSITE_RUNTIME_DESCRIPTOR_KIND {
            self.push(
                "/kind",
                format!("kind must be {WEBSITE_RUNTIME_DESCRIPTOR_KIND}"),
            );
        }
        for (path, value) in [
            ("/revisionUuid", descriptor.revision_uuid.as_str()),
            ("/appUuid", descriptor.app_uuid.as_str()),
        ] {
            if !valid_opaque_id(value) {
                self.push(path, "must be a bounded opaque identifier");
            }
        }
        if !is_lower_hex(&descriptor.tenant_scope_hash, 64) {
            self.push(
                "/tenantScopeHash",
                "must contain exactly 64 lowercase hexadecimal characters",
            );
        }
        if !is_lower_hex(&descriptor.descriptor_sha256, 64) {
            self.push(
                "/descriptorSha256",
                "must contain exactly 64 lowercase hexadecimal characters",
            );
        }
        if !valid_canonical_timestamp(&descriptor.generated_at) {
            self.push(
                "/generatedAt",
                "must use canonical UTC RFC 3339 seconds format",
            );
        }
        if descriptor.compiler_version.is_empty()
            || descriptor.compiler_version.len() > 128
            || descriptor
                .compiler_version
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            self.push(
                "/compilerVersion",
                "must be a non-empty bounded version token",
            );
        }
    }

    fn validate_limits(&mut self, descriptor: &WebsiteRuntimeDescriptor) {
        let limits = &descriptor.limits;
        self.bounded_limit(
            "/limits/maximumBindings",
            limits.maximum_bindings,
            HARD_MAX_BINDINGS,
        );
        self.bounded_limit(
            "/limits/maximumVariants",
            limits.maximum_variants,
            HARD_MAX_VARIANTS,
        );
        self.bounded_limit(
            "/limits/maximumVariantRules",
            limits.maximum_variant_rules,
            HARD_MAX_VARIANT_RULES,
        );
        self.bounded_limit(
            "/limits/maximumResources",
            limits.maximum_resources,
            HARD_MAX_RESOURCES,
        );
        self.bounded_limit(
            "/limits/maximumMounts",
            limits.maximum_mounts,
            HARD_MAX_MOUNTS,
        );
        self.bounded_limit(
            "/limits/maximumIndexFilesPerMount",
            limits.maximum_index_files_per_mount,
            HARD_MAX_INDEX_FILES_PER_MOUNT,
        );
        self.bounded_limit(
            "/limits/maximumPathBytes",
            limits.maximum_path_bytes,
            HARD_MAX_PATH_BYTES,
        );
        self.bounded_limit(
            "/limits/maximumPathSegments",
            limits.maximum_path_segments,
            HARD_MAX_PATH_SEGMENTS,
        );

        for (path, actual, configured) in [
            (
                "/bindings",
                descriptor.bindings.len(),
                limits.maximum_bindings,
            ),
            (
                "/variants",
                descriptor.variants.len(),
                limits.maximum_variants,
            ),
            (
                "/variantRules",
                descriptor.variant_rules.len(),
                limits.maximum_variant_rules,
            ),
            (
                "/resources",
                descriptor.resources.len(),
                limits.maximum_resources,
            ),
            ("/mounts", descriptor.mounts.len(), limits.maximum_mounts),
        ] {
            if actual > configured {
                self.push(
                    path,
                    format!("contains {actual} entries; descriptor limit is {configured}"),
                );
            }
        }
        if descriptor.delivery_policy.provider_timeout_ms == 0
            || descriptor.delivery_policy.provider_timeout_ms > HARD_MAX_PROVIDER_TIMEOUT_MS
        {
            self.push(
                "/deliveryPolicy/providerTimeoutMs",
                format!("must be between 1 and {HARD_MAX_PROVIDER_TIMEOUT_MS}"),
            );
        }
        for (path, value) in [
            (
                "/deliveryPolicy/metadataCacheTtlSeconds",
                descriptor.delivery_policy.metadata_cache_ttl_seconds,
            ),
            (
                "/deliveryPolicy/negativeCacheTtlSeconds",
                descriptor.delivery_policy.negative_cache_ttl_seconds,
            ),
            (
                "/deliveryPolicy/staleWhileRevalidateSeconds",
                descriptor.delivery_policy.stale_while_revalidate_seconds,
            ),
        ] {
            if value > HARD_MAX_CACHE_TTL_SECONDS {
                self.push(
                    path,
                    format!("must not exceed {HARD_MAX_CACHE_TTL_SECONDS}"),
                );
            }
        }
        if descriptor.delivery_policy.negative_cache_ttl_seconds
            > descriptor.delivery_policy.metadata_cache_ttl_seconds
        {
            self.push(
                "/deliveryPolicy/negativeCacheTtlSeconds",
                "must not exceed metadataCacheTtlSeconds",
            );
        }
        if descriptor.delivery_policy.maximum_object_bytes == 0
            || descriptor.delivery_policy.maximum_object_bytes > HARD_MAX_OBJECT_BYTES
        {
            self.push(
                "/deliveryPolicy/maximumObjectBytes",
                format!("must be between 1 and {HARD_MAX_OBJECT_BYTES}"),
            );
        }
        if descriptor.observability_policy.trace_sample_rate_per_mille > 1_000 {
            self.push(
                "/observabilityPolicy/traceSampleRatePerMille",
                "must not exceed 1000",
            );
        }
    }

    fn validate_bindings(
        &mut self,
        descriptor: &WebsiteRuntimeDescriptor,
        variants: &HashMap<&str, usize>,
    ) {
        let mut route_keys = HashSet::new();
        for (index, binding) in descriptor.bindings.iter().enumerate() {
            let path = format!("/bindings/{index}");
            self.validate_id(&format!("{path}/bindingUuid"), &binding.binding_uuid);
            match normalize_website_hostname(&binding.hostname) {
                Some(normalized) if normalized == binding.hostname => {}
                Some(normalized) => self.push(
                    format!("{path}/hostname"),
                    format!("must use canonical lowercase ASCII hostname {normalized}"),
                ),
                None => self.push(
                    format!("{path}/hostname"),
                    "must be an exact or leading-wildcard DNS hostname",
                ),
            }
            self.validate_prefix_path(
                &format!("{path}/pathPrefix"),
                &binding.path_prefix,
                descriptor,
            );
            if !route_keys.insert((binding.hostname.as_str(), binding.path_prefix.as_str())) {
                self.push(&path, "hostname and pathPrefix are already owned");
            }
            match &binding.action {
                WebsiteBindingAction::Serve {
                    default_variant_uuid,
                    forced_variant_uuid,
                } => {
                    for (field, value) in [
                        ("defaultVariantUuid", default_variant_uuid.as_deref()),
                        ("forcedVariantUuid", forced_variant_uuid.as_deref()),
                    ] {
                        if let Some(value) = value {
                            if !variants.contains_key(value) {
                                self.push(
                                    format!("{path}/action/{field}"),
                                    "references an unknown Variant",
                                );
                            }
                        }
                    }
                }
                WebsiteBindingAction::Redirect {
                    status_code,
                    scheme,
                    hostname,
                    path_prefix,
                    ..
                } => {
                    if !matches!(status_code, 301 | 302 | 307 | 308) {
                        self.push(
                            format!("{path}/action/statusCode"),
                            "must be 301, 302, 307, or 308",
                        );
                    }
                    if descriptor.environment == WebsiteRuntimeEnvironment::Production
                        && *scheme == super::WebsiteRedirectScheme::Http
                    {
                        self.push(
                            format!("{path}/action/scheme"),
                            "production redirects must use https",
                        );
                    }
                    match normalize_website_hostname(hostname) {
                        Some(normalized)
                            if normalized == *hostname && !hostname.starts_with("*.") => {}
                        _ => self.push(
                            format!("{path}/action/hostname"),
                            "redirect hostname must be one canonical exact DNS hostname",
                        ),
                    }
                    self.validate_prefix_path(
                        &format!("{path}/action/pathPrefix"),
                        path_prefix,
                        descriptor,
                    );
                }
            }
        }
        self.validate_redirect_cycles(descriptor);
    }

    fn validate_redirect_cycles(&mut self, descriptor: &WebsiteRuntimeDescriptor) {
        let edges = descriptor
            .bindings
            .iter()
            .map(|binding| match &binding.action {
                WebsiteBindingAction::Redirect {
                    hostname,
                    path_prefix,
                    ..
                } => select_binding_for_redirect(&descriptor.bindings, hostname, path_prefix),
                WebsiteBindingAction::Serve { .. } => None,
            })
            .collect::<Vec<_>>();
        let mut states = vec![0_u8; descriptor.bindings.len()];
        for start in 0..descriptor.bindings.len() {
            let mut current = start;
            let mut stack = Vec::new();
            loop {
                match states[current] {
                    1 => {
                        self.push(
                            format!("/bindings/{current}/action"),
                            "redirect graph contains a cycle",
                        );
                        break;
                    }
                    2 => break,
                    _ => {
                        states[current] = 1;
                        stack.push(current);
                        let Some(next) = edges[current] else {
                            break;
                        };
                        current = next;
                    }
                }
            }
            for index in stack {
                states[index] = 2;
            }
        }
    }

    fn validate_variant_rules(
        &mut self,
        descriptor: &WebsiteRuntimeDescriptor,
        variants: &HashMap<&str, usize>,
    ) {
        let mut match_keys = HashSet::new();
        for (index, rule) in descriptor.variant_rules.iter().enumerate() {
            let path = format!("/variantRules/{index}");
            self.validate_id(&format!("{path}/ruleUuid"), &rule.rule_uuid);
            if !variants.contains_key(rule.variant_uuid.as_str()) {
                self.push(
                    format!("{path}/variantUuid"),
                    "references an unknown Variant",
                );
            }
            let key = match &rule.matcher {
                WebsiteVariantRuleMatcher::PathPrefix { path_prefix } => {
                    self.validate_prefix_path(
                        &format!("{path}/match/pathPrefix"),
                        path_prefix,
                        descriptor,
                    );
                    format!("path:{path_prefix}:{}", rule.priority)
                }
                WebsiteVariantRuleMatcher::ClientClass { client_class } => {
                    format!("client:{client_class:?}:{}", rule.priority)
                }
            };
            if !match_keys.insert(key) {
                self.push(
                    format!("{path}/match"),
                    "another rule has the same matcher and priority",
                );
            }
        }
    }

    fn validate_resources(&mut self, descriptor: &WebsiteRuntimeDescriptor) {
        for (index, resource) in descriptor.resources.iter().enumerate() {
            let path = format!("/resources/{index}");
            self.validate_id(&format!("{path}/resourceUuid"), &resource.resource_uuid);
            self.validate_id(
                &format!("{path}/provider/providerResourceUuid"),
                &resource.provider.provider_resource_uuid,
            );
            if resource.provider.provider_contract_version.is_empty()
                || resource.provider.provider_contract_version.len() > 64
                || resource
                    .provider
                    .provider_contract_version
                    .bytes()
                    .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
            {
                self.push(
                    format!("{path}/provider/providerContractVersion"),
                    "must be a bounded non-empty contract token",
                );
            }
            let capabilities = &resource.capabilities;
            if !capabilities.static_content
                && !capabilities.wiki_routes
                && !capabilities.wiki_search
            {
                self.push(
                    format!("{path}/capabilities"),
                    "at least one public provider capability is required",
                );
            }
            if resource.provider.provider_type == WebsiteProviderType::Drive
                && (capabilities.wiki_routes || capabilities.wiki_search)
            {
                self.push(
                    format!("{path}/capabilities"),
                    "Drive WebsiteRoot resources cannot declare Wiki capabilities",
                );
            }
            if capabilities.wiki_search && !capabilities.wiki_routes {
                self.push(
                    format!("{path}/capabilities/wikiSearch"),
                    "Wiki search requires wikiRoutes",
                );
            }
        }
    }

    fn validate_mounts(
        &mut self,
        descriptor: &WebsiteRuntimeDescriptor,
        variants: &HashMap<&str, usize>,
        resources: &HashMap<&str, usize>,
    ) {
        let mut mount_keys = HashSet::new();
        let mut mounted_variants = HashSet::new();
        let mut mounted_resources = HashSet::new();
        for (index, mount) in descriptor.mounts.iter().enumerate() {
            let path = format!("/mounts/{index}");
            self.validate_id(&format!("{path}/mountUuid"), &mount.mount_uuid);
            if !variants.contains_key(mount.variant_uuid.as_str()) {
                self.push(
                    format!("{path}/variantUuid"),
                    "references an unknown Variant",
                );
            } else {
                mounted_variants.insert(mount.variant_uuid.as_str());
            }
            let resource = resources
                .get(mount.resource_uuid.as_str())
                .and_then(|resource_index| descriptor.resources.get(*resource_index));
            if resource.is_none() {
                self.push(
                    format!("{path}/resourceUuid"),
                    "references an unknown Resource",
                );
            } else {
                mounted_resources.insert(mount.resource_uuid.as_str());
            }
            self.validate_prefix_path(
                &format!("{path}/pathPrefix"),
                &mount.path_prefix,
                descriptor,
            );
            self.validate_prefix_path(
                &format!("{path}/translation/resourceSubpath"),
                &mount.translation.resource_subpath,
                descriptor,
            );
            if !mount_keys.insert((mount.variant_uuid.as_str(), mount.path_prefix.as_str())) {
                self.push(
                    &path,
                    "Variant and pathPrefix are already owned by another Mount",
                );
            }
            if mount.index_files.len() > descriptor.limits.maximum_index_files_per_mount {
                self.push(
                    format!("{path}/indexFiles"),
                    "exceeds maximumIndexFilesPerMount",
                );
            }
            let mut index_files = HashSet::new();
            for (file_index, file) in mount.index_files.iter().enumerate() {
                if !valid_file_name(file) {
                    self.push(
                        format!("{path}/indexFiles/{file_index}"),
                        "must be a safe bounded file name without path separators",
                    );
                }
                if !index_files.insert(file.as_str()) {
                    self.push(
                        format!("{path}/indexFiles/{file_index}"),
                        "index file is duplicated",
                    );
                }
            }
            match mount.handler {
                WebsiteHandler::Static => {
                    if mount.spa_fallback.is_some() {
                        self.push(
                            format!("{path}/spaFallback"),
                            "STATIC Mounts cannot declare spaFallback",
                        );
                    }
                    self.require_static_capability(&path, resource);
                }
                WebsiteHandler::Spa => {
                    self.require_static_capability(&path, resource);
                    match mount.spa_fallback.as_deref() {
                        Some(fallback) => self.validate_prefix_path(
                            &format!("{path}/spaFallback"),
                            fallback,
                            descriptor,
                        ),
                        None => self.push(
                            format!("{path}/spaFallback"),
                            "SPA Mounts require a fallback path",
                        ),
                    }
                }
                WebsiteHandler::Wiki => {
                    if mount.spa_fallback.is_some() {
                        self.push(
                            format!("{path}/spaFallback"),
                            "WIKI Mounts cannot declare spaFallback",
                        );
                    }
                    if !mount.index_files.is_empty() {
                        self.push(
                            format!("{path}/indexFiles"),
                            "WIKI Mounts delegate route and index behavior to Knowledgebase",
                        );
                    }
                    if resource.is_some_and(|resource| !resource.capabilities.wiki_routes) {
                        self.push(
                            format!("{path}/handler"),
                            "WIKI requires the Resource wikiRoutes capability",
                        );
                    }
                }
            }
        }

        for (index, variant) in descriptor.variants.iter().enumerate() {
            if !mounted_variants.contains(variant.variant_uuid.as_str()) {
                self.push(
                    format!("/variants/{index}/variantUuid"),
                    "Variant has no Mount",
                );
            }
        }
        for (index, resource) in descriptor.resources.iter().enumerate() {
            if !mounted_resources.contains(resource.resource_uuid.as_str()) {
                self.push(
                    format!("/resources/{index}/resourceUuid"),
                    "Resource is not referenced by a Mount",
                );
            }
        }
    }

    fn validate_security_policy(&mut self, descriptor: &WebsiteRuntimeDescriptor) {
        let prefixes = &descriptor.security_policy.denied_path_prefixes;
        if prefixes.len() > 128 {
            self.push(
                "/securityPolicy/deniedPathPrefixes",
                "must not contain more than 128 prefixes",
            );
        }
        self.require_canonical_order(
            "/securityPolicy/deniedPathPrefixes",
            prefixes.iter().map(String::as_str),
        );
        let mut unique = HashSet::new();
        for (index, prefix) in prefixes.iter().enumerate() {
            self.validate_prefix_path(
                &format!("/securityPolicy/deniedPathPrefixes/{index}"),
                prefix,
                descriptor,
            );
            if !unique.insert(prefix.as_str()) {
                self.push(
                    format!("/securityPolicy/deniedPathPrefixes/{index}"),
                    "prefix is duplicated",
                );
            }
        }
    }

    fn require_static_capability(
        &mut self,
        mount_path: &str,
        resource: Option<&super::WebsiteResource>,
    ) {
        if resource.is_some_and(|resource| !resource.capabilities.static_content) {
            self.push(
                format!("{mount_path}/handler"),
                "STATIC and SPA require the Resource staticContent capability",
            );
        }
    }

    fn validate_prefix_path(
        &mut self,
        path: &str,
        value: &str,
        descriptor: &WebsiteRuntimeDescriptor,
    ) {
        let normalized = normalize_uri_path(
            value,
            descriptor.limits.maximum_path_bytes,
            descriptor.limits.maximum_path_segments,
        );
        if normalized.as_deref() != Ok(value) || (value.len() > 1 && value.ends_with('/')) {
            self.push(
                path,
                "must be a canonical absolute path without a trailing slash",
            );
        }
    }

    fn validate_id(&mut self, path: &str, value: &str) {
        if !valid_opaque_id(value) {
            self.push(path, "must be a bounded opaque identifier");
        }
    }

    fn index_unique<'a>(
        &mut self,
        path: &str,
        values: impl Iterator<Item = &'a str>,
    ) -> HashMap<&'a str, usize> {
        let mut indexed = HashMap::new();
        for (index, value) in values.enumerate() {
            if indexed.insert(value, index).is_some() {
                self.push(
                    format!("{path}/{index}"),
                    format!("identifier {value} is duplicated"),
                );
            }
        }
        indexed
    }

    fn require_canonical_order<'a>(&mut self, path: &str, values: impl Iterator<Item = &'a str>) {
        let values = values.collect::<Vec<_>>();
        if values.windows(2).any(|pair| pair[0] > pair[1]) {
            self.push(path, "entries must be ordered by stable identifier");
        }
    }

    fn bounded_limit(&mut self, path: &str, value: usize, hard_maximum: usize) {
        if value == 0 || value > hard_maximum {
            self.push(
                path,
                format!("must be between 1 and runtime ceiling {hard_maximum}"),
            );
        }
    }

    fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(ConfigDiagnostic::new(path, message));
        }
    }

    fn finish(self) -> Result<(), WebsiteRuntimeDescriptorError> {
        if self.diagnostics.is_empty() {
            Ok(())
        } else {
            Err(WebsiteRuntimeDescriptorError::Validation {
                diagnostics: self.diagnostics,
            })
        }
    }
}

fn valid_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_file_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && !value.starts_with('.')
        && !value.contains(['/', '\\'])
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_canonical_timestamp(value: &str) -> bool {
    value.len() == 20
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value.as_bytes()[10] == b'T'
        && value.as_bytes()[13] == b':'
        && value.as_bytes()[16] == b':'
        && value.as_bytes()[19] == b'Z'
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        })
}

fn select_binding_for_redirect(
    bindings: &[super::WebsiteBinding],
    hostname: &str,
    path: &str,
) -> Option<usize> {
    let exact_host_exists = bindings.iter().any(|binding| binding.hostname == hostname);
    if exact_host_exists {
        return bindings
            .iter()
            .enumerate()
            .filter(|(_, binding)| binding.hostname == hostname)
            .filter(|(_, binding)| segment_prefix_matches(&binding.path_prefix, path))
            .max_by_key(|(_, binding)| binding.path_prefix.len())
            .map(|(index, _)| index);
    }
    bindings
        .iter()
        .enumerate()
        .filter(|(_, binding)| {
            binding
                .hostname
                .strip_prefix("*.")
                .is_some_and(|suffix| wildcard_matches(suffix, hostname))
        })
        .filter(|(_, binding)| segment_prefix_matches(&binding.path_prefix, path))
        .max_by_key(|(_, binding)| (binding.hostname.len(), binding.path_prefix.len()))
        .map(|(index, _)| index)
}

fn segment_prefix_matches(prefix: &str, path: &str) -> bool {
    prefix == "/"
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn wildcard_matches(suffix: &str, hostname: &str) -> bool {
    hostname.strip_suffix(suffix).is_some_and(|prefix| {
        prefix.ends_with('.') && prefix.len() > 1 && !prefix[..prefix.len() - 1].contains('.')
    })
}
