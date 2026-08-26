use std::collections::HashMap;

use crate::normalize_uri_path;

use super::{
    normalize_website_hostname, ProviderResourceReference, WebsiteBinding, WebsiteBindingAction,
    WebsiteClientClass, WebsiteMount, WebsiteMountMode, WebsiteRedirectScheme, WebsiteResource,
    WebsiteRouteSelectionError, WebsiteRuntimeDescriptor, WebsiteVariant,
    WebsiteVariantRuleMatcher,
};

#[derive(Debug)]
pub struct CompiledWebsiteRuntimeDescriptor {
    descriptor: WebsiteRuntimeDescriptor,
    descriptor_sha256: String,
    exact_bindings: HashMap<String, PrefixIndex>,
    /// Wildcard bindings keyed by their suffix (without the leading `*.`):
    /// `*.suffix` matches a request host exactly when the host is one label
    /// plus that suffix (constant-time map lookup, no per-request scan).
    wildcard_bindings: HashMap<String, PrefixIndex>,
    variants: HashMap<String, usize>,
    resources: HashMap<String, usize>,
    mounts_by_variant: HashMap<String, PrefixIndex>,
    path_variant_rules: Vec<usize>,
    client_variant_rules: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebsiteClientClassificationSource {
    ClientHint,
    UserAgent,
    Bot,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WebsiteRequestRoutingContext<'a> {
    pub verified_preferred_variant_uuid: Option<&'a str>,
    pub client_class: Option<WebsiteClientClass>,
    pub client_classification_source: Option<WebsiteClientClassificationSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebsiteVariantSelectionReason {
    Forced,
    Preference,
    PathRule,
    ClientHint,
    UserAgent,
    Bot,
    BindingDefault,
    SiteDefault,
}

#[derive(Debug)]
pub struct SelectedWebsiteRoute<'a> {
    pub revision_uuid: &'a str,
    pub app_uuid: &'a str,
    pub tenant_scope_hash: &'a str,
    pub normalized_request_hostname: String,
    pub binding: &'a WebsiteBinding,
    pub variant: &'a WebsiteVariant,
    pub mount: &'a WebsiteMount,
    pub resource: &'a WebsiteResource,
    pub provider: &'a ProviderResourceReference,
    pub normalized_request_path: String,
    pub binding_relative_path: String,
    pub provider_relative_path: String,
    pub provider_timeout_ms: u64,
    pub metadata_cache_ttl_seconds: u32,
    pub negative_cache_ttl_seconds: u32,
    pub stale_while_revalidate_seconds: u32,
    pub maximum_object_bytes: u64,
    pub force_https: bool,
    pub variant_reason: WebsiteVariantSelectionReason,
}

#[derive(Debug)]
pub struct SelectedWebsiteRedirect<'a> {
    pub binding: &'a WebsiteBinding,
    pub status_code: u16,
    pub scheme: WebsiteRedirectScheme,
    pub hostname: &'a str,
    pub path: String,
    pub preserve_query: bool,
}

#[derive(Debug)]
pub enum WebsiteRouteSelection<'a> {
    Serve(SelectedWebsiteRoute<'a>),
    Redirect(SelectedWebsiteRedirect<'a>),
}

#[derive(Debug, Default)]
pub(super) struct PrefixIndex {
    nodes: Vec<PrefixNode>,
}

#[derive(Debug, Default)]
struct PrefixNode {
    children: HashMap<u8, usize>,
    value: Option<usize>,
}

impl CompiledWebsiteRuntimeDescriptor {
    pub(crate) fn compile(descriptor: WebsiteRuntimeDescriptor, descriptor_sha256: String) -> Self {
        let mut exact_bindings: HashMap<String, PrefixIndex> = HashMap::new();
        let mut wildcard_bindings: HashMap<String, PrefixIndex> = HashMap::new();
        for (index, binding) in descriptor.bindings.iter().enumerate() {
            if let Some(suffix) = binding.hostname.strip_prefix("*.") {
                wildcard_bindings
                    .entry(suffix.to_owned())
                    .or_default()
                    .insert(&binding.path_prefix, index);
            } else {
                exact_bindings
                    .entry(binding.hostname.clone())
                    .or_default()
                    .insert(&binding.path_prefix, index);
            }
        }
        let variants = descriptor
            .variants
            .iter()
            .enumerate()
            .map(|(index, variant)| (variant.variant_uuid.clone(), index))
            .collect::<HashMap<_, _>>();
        let resources = descriptor
            .resources
            .iter()
            .enumerate()
            .map(|(index, resource)| (resource.resource_uuid.clone(), index))
            .collect::<HashMap<_, _>>();
        let mut mounts_by_variant: HashMap<String, PrefixIndex> = HashMap::new();
        for (index, mount) in descriptor.mounts.iter().enumerate() {
            mounts_by_variant
                .entry(mount.variant_uuid.clone())
                .or_default()
                .insert(&mount.path_prefix, index);
        }

        let mut path_variant_rules = descriptor
            .variant_rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                matches!(rule.matcher, WebsiteVariantRuleMatcher::PathPrefix { .. })
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        path_variant_rules.sort_unstable_by(|left, right| {
            let left_rule = &descriptor.variant_rules[*left];
            let right_rule = &descriptor.variant_rules[*right];
            right_rule
                .priority
                .cmp(&left_rule.priority)
                .then_with(|| {
                    matcher_path_length(&right_rule.matcher)
                        .cmp(&matcher_path_length(&left_rule.matcher))
                })
                .then_with(|| left_rule.rule_uuid.cmp(&right_rule.rule_uuid))
        });
        let mut client_variant_rules = descriptor
            .variant_rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                matches!(rule.matcher, WebsiteVariantRuleMatcher::ClientClass { .. })
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        client_variant_rules.sort_unstable_by(|left, right| {
            let left_rule = &descriptor.variant_rules[*left];
            let right_rule = &descriptor.variant_rules[*right];
            right_rule
                .priority
                .cmp(&left_rule.priority)
                .then_with(|| left_rule.rule_uuid.cmp(&right_rule.rule_uuid))
        });

        Self {
            descriptor,
            descriptor_sha256,
            exact_bindings,
            wildcard_bindings,
            variants,
            resources,
            mounts_by_variant,
            path_variant_rules,
            client_variant_rules,
        }
    }

    pub fn descriptor(&self) -> &WebsiteRuntimeDescriptor {
        &self.descriptor
    }

    pub fn descriptor_sha256(&self) -> &str {
        &self.descriptor_sha256
    }

    pub fn select_route<'a>(
        &'a self,
        host: &str,
        path: &str,
        context: WebsiteRequestRoutingContext<'_>,
    ) -> Result<Option<WebsiteRouteSelection<'a>>, WebsiteRouteSelectionError> {
        let host =
            normalize_request_hostname(host).ok_or(WebsiteRouteSelectionError::InvalidHost)?;
        if host.starts_with("*.") {
            return Err(WebsiteRouteSelectionError::InvalidHost);
        }
        let normalized_path = normalize_uri_path(
            path,
            self.descriptor.limits.maximum_path_bytes,
            self.descriptor.limits.maximum_path_segments,
        )
        .map_err(|_| WebsiteRouteSelectionError::InvalidPath)?;
        if self.path_is_denied(&normalized_path) {
            return Err(WebsiteRouteSelectionError::DeniedPath);
        }
        let Some(binding_index) = self.select_binding(&host, &normalized_path) else {
            return Ok(None);
        };
        let binding = &self.descriptor.bindings[binding_index];
        let binding_relative_path = strip_segment_prefix(&normalized_path, &binding.path_prefix)
            .expect("compiled Binding prefix was selected from the same canonical path");

        match &binding.action {
            WebsiteBindingAction::Redirect {
                status_code,
                scheme,
                hostname,
                path_prefix,
                preserve_path,
                preserve_query,
            } => {
                let path = if *preserve_path {
                    join_canonical_paths(path_prefix, &binding_relative_path)
                } else {
                    path_prefix.clone()
                };
                Ok(Some(WebsiteRouteSelection::Redirect(
                    SelectedWebsiteRedirect {
                        binding,
                        status_code: *status_code,
                        scheme: *scheme,
                        hostname,
                        path,
                        preserve_query: *preserve_query,
                    },
                )))
            }
            WebsiteBindingAction::Serve {
                default_variant_uuid,
                forced_variant_uuid,
            } => {
                let (variant_index, variant_reason) = self.select_variant(
                    &binding_relative_path,
                    default_variant_uuid.as_deref(),
                    forced_variant_uuid.as_deref(),
                    context,
                );
                let variant = &self.descriptor.variants[variant_index];
                let mount_index = self
                    .mounts_by_variant
                    .get(&variant.variant_uuid)
                    .and_then(|index| index.select(&binding_relative_path));
                let Some(mount_index) = mount_index else {
                    return Ok(None);
                };
                let mount = &self.descriptor.mounts[mount_index];
                let resource_index = self.resources[&mount.resource_uuid];
                let resource = &self.descriptor.resources[resource_index];
                let provider_relative_path = translate_mount_path(mount, &binding_relative_path);
                Ok(Some(WebsiteRouteSelection::Serve(SelectedWebsiteRoute {
                    revision_uuid: &self.descriptor.revision_uuid,
                    app_uuid: &self.descriptor.app_uuid,
                    tenant_scope_hash: &self.descriptor.tenant_scope_hash,
                    normalized_request_hostname: host,
                    binding,
                    variant,
                    mount,
                    resource,
                    provider: &resource.provider,
                    normalized_request_path: normalized_path,
                    binding_relative_path,
                    provider_relative_path,
                    provider_timeout_ms: self.descriptor.delivery_policy.provider_timeout_ms,
                    metadata_cache_ttl_seconds: self
                        .descriptor
                        .delivery_policy
                        .metadata_cache_ttl_seconds,
                    negative_cache_ttl_seconds: self
                        .descriptor
                        .delivery_policy
                        .negative_cache_ttl_seconds,
                    stale_while_revalidate_seconds: self
                        .descriptor
                        .delivery_policy
                        .stale_while_revalidate_seconds,
                    maximum_object_bytes: self.descriptor.delivery_policy.maximum_object_bytes,
                    force_https: self.descriptor.security_policy.force_https,
                    variant_reason,
                })))
            }
        }
    }

    fn select_binding(&self, host: &str, path: &str) -> Option<usize> {
        if let Some(index) = self.exact_bindings.get(host) {
            return index.select(path);
        }
        // One-level wildcard (`*.suffix`) semantics: the host must be exactly
        // one label plus the configured suffix (constant-time map lookup).
        host.split_once('.')
            .and_then(|(label, suffix)| (!label.is_empty()).then_some(suffix))
            .and_then(|suffix| self.wildcard_bindings.get(suffix))
            .and_then(|index| index.select(path))
    }

    fn path_is_denied(&self, path: &str) -> bool {
        if self.descriptor.security_policy.deny_dot_files
            && path
                .split('/')
                .any(|segment| segment.starts_with('.') && segment.len() > 1)
        {
            return true;
        }
        self.descriptor
            .security_policy
            .denied_path_prefixes
            .iter()
            .any(|prefix| segment_prefix_matches(prefix, path))
    }

    fn select_variant(
        &self,
        path: &str,
        binding_default: Option<&str>,
        forced: Option<&str>,
        context: WebsiteRequestRoutingContext<'_>,
    ) -> (usize, WebsiteVariantSelectionReason) {
        if let Some(variant_uuid) = forced {
            return (
                self.variants[variant_uuid],
                WebsiteVariantSelectionReason::Forced,
            );
        }
        if let Some(variant_index) = context
            .verified_preferred_variant_uuid
            .and_then(|variant_uuid| self.variants.get(variant_uuid).copied())
        {
            return (variant_index, WebsiteVariantSelectionReason::Preference);
        }
        for rule_index in &self.path_variant_rules {
            let rule = &self.descriptor.variant_rules[*rule_index];
            let WebsiteVariantRuleMatcher::PathPrefix { path_prefix } = &rule.matcher else {
                continue;
            };
            if segment_prefix_matches(path_prefix, path) {
                return (
                    self.variants[&rule.variant_uuid],
                    WebsiteVariantSelectionReason::PathRule,
                );
            }
        }
        if let Some(client_class) = context.client_class {
            for rule_index in &self.client_variant_rules {
                let rule = &self.descriptor.variant_rules[*rule_index];
                let WebsiteVariantRuleMatcher::ClientClass {
                    client_class: expected,
                } = rule.matcher
                else {
                    continue;
                };
                if expected == client_class {
                    let reason = match context.client_classification_source {
                        Some(WebsiteClientClassificationSource::ClientHint) => {
                            WebsiteVariantSelectionReason::ClientHint
                        }
                        Some(WebsiteClientClassificationSource::Bot) => {
                            WebsiteVariantSelectionReason::Bot
                        }
                        Some(WebsiteClientClassificationSource::UserAgent) | None => {
                            WebsiteVariantSelectionReason::UserAgent
                        }
                    };
                    return (self.variants[&rule.variant_uuid], reason);
                }
            }
        }
        if let Some(variant_uuid) = binding_default {
            return (
                self.variants[variant_uuid],
                WebsiteVariantSelectionReason::BindingDefault,
            );
        }
        (
            self.variants[&self.descriptor.app_default_variant_uuid],
            WebsiteVariantSelectionReason::SiteDefault,
        )
    }
}

impl PrefixIndex {
    pub(super) fn insert(&mut self, prefix: &str, value: usize) {
        if self.nodes.is_empty() {
            self.nodes.push(PrefixNode::default());
        }
        let mut node_index = 0usize;
        for byte in prefix.bytes() {
            let next = if let Some(index) = self.nodes[node_index].children.get(&byte) {
                *index
            } else {
                let index = self.nodes.len();
                self.nodes.push(PrefixNode::default());
                self.nodes[node_index].children.insert(byte, index);
                index
            };
            node_index = next;
        }
        self.nodes[node_index].value = Some(value);
    }

    pub(super) fn select(&self, path: &str) -> Option<usize> {
        let bytes = path.as_bytes();
        let mut node_index = 0usize;
        let mut selected = None;
        for (position, byte) in bytes.iter().enumerate() {
            let Some(next) = self.nodes[node_index].children.get(byte).copied() else {
                break;
            };
            node_index = next;
            if let Some(value) = self.nodes[node_index].value {
                let root_prefix = position == 0;
                let segment_boundary = position + 1 == bytes.len() || bytes[position + 1] == b'/';
                if root_prefix || segment_boundary {
                    selected = Some(value);
                }
            }
        }
        selected
    }
}

fn matcher_path_length(matcher: &WebsiteVariantRuleMatcher) -> usize {
    match matcher {
        WebsiteVariantRuleMatcher::PathPrefix { path_prefix } => path_prefix.len(),
        WebsiteVariantRuleMatcher::ClientClass { .. } => 0,
    }
}

pub(super) fn normalize_request_hostname(authority: &str) -> Option<String> {
    let authority = authority.trim();
    let hostname = match authority.rsplit_once(':') {
        Some((hostname, port))
            if !hostname.contains(':')
                && !port.is_empty()
                && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            hostname
        }
        _ if !authority.contains(':') => authority,
        _ => return None,
    };
    normalize_website_hostname(hostname)
}

fn segment_prefix_matches(prefix: &str, path: &str) -> bool {
    prefix == "/"
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn strip_segment_prefix(path: &str, prefix: &str) -> Option<String> {
    if prefix == "/" {
        return Some(path.to_owned());
    }
    if path == prefix {
        return Some("/".to_owned());
    }
    path.strip_prefix(prefix)
        .filter(|remainder| remainder.starts_with('/'))
        .map(str::to_owned)
}

fn translate_mount_path(mount: &WebsiteMount, request_path: &str) -> String {
    let translated = match mount.translation.mode {
        WebsiteMountMode::Root => request_path.to_owned(),
        WebsiteMountMode::Alias => strip_segment_prefix(request_path, &mount.path_prefix)
            .expect("compiled Mount prefix was selected from the same canonical path"),
    };
    join_canonical_paths(&mount.translation.resource_subpath, &translated)
}

fn join_canonical_paths(prefix: &str, suffix: &str) -> String {
    match (prefix, suffix) {
        ("/", suffix) => suffix.to_owned(),
        (prefix, "/") => prefix.to_owned(),
        (prefix, suffix) => format!("{prefix}{suffix}"),
    }
}
