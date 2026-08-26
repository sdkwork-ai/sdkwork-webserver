use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

use arc_swap::ArcSwapOption;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    canonical_json::canonical_sha256_excluding_field, normalize_uri_path, ConfigDiagnostic,
};

use super::{
    compile_website_runtime_descriptor,
    compiled::{normalize_request_hostname, PrefixIndex},
    CompiledWebsiteRuntimeDescriptor, WebsiteRequestRoutingContext, WebsiteRouteSelection,
    WebsiteRouteSelectionError, WebsiteRuntimeDescriptor, WebsiteRuntimeDescriptorError,
    WebsiteRuntimeEnvironment,
};

pub const WEBSITE_RUNTIME_SET_SCHEMA_VERSION: &str = "sdkwork.website-runtime-set.v1";
pub const WEBSITE_RUNTIME_SET_KIND: &str = "sdkwork.website-runtime-set.snapshot";
pub const MAX_WEBSITE_RUNTIME_SET_BYTES: usize = 64 * 1024 * 1024;
const MAX_SCHEMA_DIAGNOSTICS: usize = 64;
const HARD_MAXIMUM_SITES: usize = 10_000;
const HARD_MAXIMUM_GENERATION: u64 = 9_007_199_254_740_991;
const HARD_MAXIMUM_PATH_BYTES: usize = 4_096;
const HARD_MAXIMUM_PATH_SEGMENTS: usize = 128;
const SCHEMA: &str =
    include_str!("../../../../specs/sdkwork.website-runtime-set.snapshot.schema.json");
static SCHEMA_VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA).map_err(|error| error.to_string())?;
    jsonschema::draft202012::new(&schema).map_err(|error| error.to_string())
});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteRuntimeSetSnapshot {
    pub schema_version: String,
    pub kind: String,
    pub snapshot_uuid: String,
    pub node_uuid: String,
    pub environment: WebsiteRuntimeEnvironment,
    pub generation: u64,
    pub generated_at: String,
    pub compiler_version: String,
    pub snapshot_sha256: String,
    pub maximum_sites: usize,
    pub descriptors: Vec<WebsiteRuntimeDescriptor>,
}

#[derive(Debug)]
pub struct CompiledWebsiteRuntimeSet {
    snapshot_uuid: String,
    node_uuid: String,
    environment: WebsiteRuntimeEnvironment,
    generation: u64,
    snapshot_sha256: String,
    descriptors: Vec<CompiledWebsiteRuntimeDescriptor>,
    tenant_scope_hashes: HashSet<String>,
    provider_types: HashSet<super::WebsiteProviderType>,
    exact_hosts: HashMap<String, PrefixIndex>,
    /// Wildcard bindings keyed by their suffix (without the leading `*.`):
    /// `*.suffix` matches a request host exactly when the host is one label
    /// plus that suffix, resolved as a constant-time map lookup instead of a
    /// per-request scan over every wildcard.
    wildcard_hosts: HashMap<String, PrefixIndex>,
}

#[derive(Debug, Error)]
pub enum WebsiteRuntimeSetError {
    #[error("website runtime set is {actual_bytes} bytes; maximum is {maximum_bytes}")]
    TooLarge {
        actual_bytes: usize,
        maximum_bytes: usize,
    },
    #[error("website runtime set is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("embedded website runtime set JSON Schema is invalid: {0}")]
    InvalidSchema(String),
    #[error("website runtime set hash mismatch: expected {expected}, calculated {calculated}")]
    HashMismatch {
        expected: String,
        calculated: String,
    },
    #[error("website runtime set failed validation")]
    Validation { diagnostics: Vec<ConfigDiagnostic> },
    #[error("website runtime descriptor at index {index} is invalid: {source}")]
    Descriptor {
        index: usize,
        #[source]
        source: WebsiteRuntimeDescriptorError,
    },
    #[error(
        "website runtime route {hostname}{path_prefix} is owned by both Site {first_app_uuid} and Site {second_app_uuid}"
    )]
    RouteConflict {
        hostname: String,
        path_prefix: String,
        first_app_uuid: String,
        second_app_uuid: String,
    },
    #[error(
        "website runtime set scope mismatch: expected node {expected_node_uuid} in {expected_environment:?}, received node {actual_node_uuid} in {actual_environment:?}"
    )]
    ScopeMismatch {
        expected_node_uuid: String,
        expected_environment: WebsiteRuntimeEnvironment,
        actual_node_uuid: String,
        actual_environment: WebsiteRuntimeEnvironment,
    },
    #[error(
        "website runtime set generation {candidate_generation} is stale; generation must be greater than {highest_generation}"
    )]
    StaleGeneration {
        candidate_generation: u64,
        highest_generation: u64,
    },
    #[error(
        "website runtime set generation {generation} has conflicting hashes {current_sha256} and {candidate_sha256}"
    )]
    GenerationConflict {
        generation: u64,
        current_sha256: String,
        candidate_sha256: String,
    },
}

impl WebsiteRuntimeSetError {
    pub fn diagnostics(&self) -> &[ConfigDiagnostic] {
        match self {
            Self::Validation { diagnostics } => diagnostics,
            _ => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebsiteRuntimeActivationReport {
    pub changed: bool,
    pub generation: u64,
    pub snapshot_sha256: String,
    pub previous_generation: Option<u64>,
    pub previous_snapshot_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebsiteRuntimeRollbackReport {
    pub generation: u64,
    pub snapshot_sha256: String,
    pub rolled_back_from_generation: u64,
    pub rolled_back_from_sha256: String,
}

#[derive(Debug, Default)]
struct WebsiteRuntimeActivationState {
    highest_generation: u64,
}

#[derive(Debug)]
pub struct WebsiteRuntimeRegistry {
    node_uuid: String,
    environment: WebsiteRuntimeEnvironment,
    current: ArcSwapOption<CompiledWebsiteRuntimeSet>,
    previous: ArcSwapOption<CompiledWebsiteRuntimeSet>,
    activation_state: Mutex<WebsiteRuntimeActivationState>,
}

pub fn website_runtime_set_snapshot_sha256(
    snapshot: &WebsiteRuntimeSetSnapshot,
) -> Result<String, WebsiteRuntimeSetError> {
    canonical_sha256_excluding_field(snapshot, "snapshotSha256")
        .map_err(WebsiteRuntimeSetError::Json)
}

pub fn compile_website_runtime_set_snapshot(
    bytes: &[u8],
) -> Result<CompiledWebsiteRuntimeSet, WebsiteRuntimeSetError> {
    if bytes.len() > MAX_WEBSITE_RUNTIME_SET_BYTES {
        return Err(WebsiteRuntimeSetError::TooLarge {
            actual_bytes: bytes.len(),
            maximum_bytes: MAX_WEBSITE_RUNTIME_SET_BYTES,
        });
    }
    let instance: Value = serde_json::from_slice(bytes)?;
    validate_schema(&instance)?;
    let snapshot: WebsiteRuntimeSetSnapshot = serde_json::from_value(instance)?;
    let calculated = website_runtime_set_snapshot_sha256(&snapshot)?;
    if snapshot.snapshot_sha256 != calculated {
        return Err(WebsiteRuntimeSetError::HashMismatch {
            expected: snapshot.snapshot_sha256.clone(),
            calculated,
        });
    }
    validate_snapshot(&snapshot)?;

    let mut descriptors = Vec::with_capacity(snapshot.descriptors.len());
    for (index, descriptor) in snapshot.descriptors.iter().enumerate() {
        let bytes = serde_json::to_vec(descriptor)?;
        let compiled = compile_website_runtime_descriptor(&bytes)
            .map_err(|source| WebsiteRuntimeSetError::Descriptor { index, source })?;
        descriptors.push(compiled);
    }
    CompiledWebsiteRuntimeSet::build(snapshot, calculated, descriptors)
}

impl CompiledWebsiteRuntimeSet {
    fn build(
        snapshot: WebsiteRuntimeSetSnapshot,
        snapshot_sha256: String,
        descriptors: Vec<CompiledWebsiteRuntimeDescriptor>,
    ) -> Result<Self, WebsiteRuntimeSetError> {
        let mut owners: HashMap<(String, String), String> = HashMap::new();
        let mut exact_hosts: HashMap<String, PrefixIndex> = HashMap::new();
        let mut wildcard_hosts: HashMap<String, PrefixIndex> = HashMap::new();
        for (descriptor_index, descriptor) in descriptors.iter().enumerate() {
            let app_uuid = &descriptor.descriptor().app_uuid;
            for binding in &descriptor.descriptor().bindings {
                let key = (binding.hostname.clone(), binding.path_prefix.clone());
                if let Some(first_app_uuid) = owners.insert(key, app_uuid.clone()) {
                    return Err(WebsiteRuntimeSetError::RouteConflict {
                        hostname: binding.hostname.clone(),
                        path_prefix: binding.path_prefix.clone(),
                        first_app_uuid,
                        second_app_uuid: app_uuid.clone(),
                    });
                }
                if let Some(suffix) = binding.hostname.strip_prefix("*.") {
                    wildcard_hosts
                        .entry(suffix.to_owned())
                        .or_default()
                        .insert(&binding.path_prefix, descriptor_index);
                } else {
                    exact_hosts
                        .entry(binding.hostname.clone())
                        .or_default()
                        .insert(&binding.path_prefix, descriptor_index);
                }
            }
        }
        let tenant_scope_hashes = descriptors
            .iter()
            .map(|descriptor| descriptor.descriptor().tenant_scope_hash.clone())
            .collect();
        let provider_types = descriptors
            .iter()
            .flat_map(|descriptor| descriptor.descriptor().resources.iter())
            .map(|resource| resource.provider.provider_type)
            .collect();
        Ok(Self {
            snapshot_uuid: snapshot.snapshot_uuid,
            node_uuid: snapshot.node_uuid,
            environment: snapshot.environment,
            generation: snapshot.generation,
            snapshot_sha256,
            descriptors,
            tenant_scope_hashes,
            provider_types,
            exact_hosts,
            wildcard_hosts,
        })
    }

    pub fn snapshot_uuid(&self) -> &str {
        &self.snapshot_uuid
    }

    pub fn node_uuid(&self) -> &str {
        &self.node_uuid
    }

    pub fn environment(&self) -> WebsiteRuntimeEnvironment {
        self.environment
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }

    pub fn site_count(&self) -> usize {
        self.descriptors.len()
    }

    pub fn descriptors(&self) -> &[CompiledWebsiteRuntimeDescriptor] {
        &self.descriptors
    }

    pub fn tenant_scope_count(&self) -> usize {
        self.tenant_scope_hashes.len()
    }

    pub fn is_empty_or_single_tenant_scope(&self, tenant_scope_hash: &str) -> bool {
        self.descriptors.is_empty()
            || (self.tenant_scope_hashes.len() == 1
                && self.tenant_scope_hashes.contains(tenant_scope_hash))
    }

    pub fn contains_tenant_scope_hash(&self, tenant_scope_hash: &str) -> bool {
        self.tenant_scope_hashes.contains(tenant_scope_hash)
    }

    pub fn uses_provider_type(&self, provider_type: super::WebsiteProviderType) -> bool {
        self.provider_types.contains(&provider_type)
    }

    pub fn select_route<'a>(
        &'a self,
        host: &str,
        path: &str,
        context: WebsiteRequestRoutingContext<'_>,
    ) -> Result<Option<WebsiteRouteSelection<'a>>, WebsiteRouteSelectionError> {
        let normalized_host =
            normalize_request_hostname(host).ok_or(WebsiteRouteSelectionError::InvalidHost)?;
        let normalized_path =
            normalize_uri_path(path, HARD_MAXIMUM_PATH_BYTES, HARD_MAXIMUM_PATH_SEGMENTS)
                .map_err(|_| WebsiteRouteSelectionError::InvalidPath)?;
        let descriptor_index = if let Some(index) = self.exact_hosts.get(&normalized_host) {
            index.select(&normalized_path)
        } else {
            // One-level wildcard (`*.suffix`) semantics: the host must be
            // exactly one label plus the configured suffix (constant-time
            // map lookup on `host.split_once('.')`).
            normalized_host
                .split_once('.')
                .and_then(|(label, suffix)| (!label.is_empty()).then_some(suffix))
                .and_then(|suffix| self.wildcard_hosts.get(suffix))
                .and_then(|index| index.select(&normalized_path))
        };
        let Some(descriptor_index) = descriptor_index else {
            return Ok(None);
        };
        self.descriptors[descriptor_index].select_route(host, path, context)
    }
}

impl WebsiteRuntimeRegistry {
    pub fn new(node_uuid: impl Into<String>, environment: WebsiteRuntimeEnvironment) -> Self {
        Self {
            node_uuid: node_uuid.into(),
            environment,
            current: ArcSwapOption::empty(),
            previous: ArcSwapOption::empty(),
            activation_state: Mutex::new(WebsiteRuntimeActivationState::default()),
        }
    }

    pub fn current(&self) -> Option<Arc<CompiledWebsiteRuntimeSet>> {
        self.current.load_full()
    }

    pub fn is_ready(&self) -> bool {
        self.current.load().is_some()
    }

    pub fn compile_and_activate(
        &self,
        bytes: &[u8],
    ) -> Result<WebsiteRuntimeActivationReport, WebsiteRuntimeSetError> {
        let candidate = Arc::new(compile_website_runtime_set_snapshot(bytes)?);
        self.activate(candidate)
    }

    pub fn activate(
        &self,
        candidate: Arc<CompiledWebsiteRuntimeSet>,
    ) -> Result<WebsiteRuntimeActivationReport, WebsiteRuntimeSetError> {
        let mut activation_state = self.activation_guard();
        if candidate.node_uuid != self.node_uuid || candidate.environment != self.environment {
            return Err(WebsiteRuntimeSetError::ScopeMismatch {
                expected_node_uuid: self.node_uuid.clone(),
                expected_environment: self.environment,
                actual_node_uuid: candidate.node_uuid.clone(),
                actual_environment: candidate.environment,
            });
        }
        if let Some(current) = self.current.load_full() {
            if current.snapshot_sha256 == candidate.snapshot_sha256 {
                return Ok(WebsiteRuntimeActivationReport {
                    changed: false,
                    generation: current.generation,
                    snapshot_sha256: current.snapshot_sha256.clone(),
                    previous_generation: self
                        .previous
                        .load_full()
                        .map(|previous| previous.generation),
                    previous_snapshot_sha256: self
                        .previous
                        .load_full()
                        .map(|previous| previous.snapshot_sha256.clone()),
                });
            }
            if candidate.generation == current.generation {
                return Err(WebsiteRuntimeSetError::GenerationConflict {
                    generation: candidate.generation,
                    current_sha256: current.snapshot_sha256.clone(),
                    candidate_sha256: candidate.snapshot_sha256.clone(),
                });
            }
        }
        if candidate.generation <= activation_state.highest_generation {
            return Err(WebsiteRuntimeSetError::StaleGeneration {
                candidate_generation: candidate.generation,
                highest_generation: activation_state.highest_generation,
            });
        }
        let generation = candidate.generation;
        let snapshot_sha256 = candidate.snapshot_sha256.clone();
        let previous = self.current.swap(Some(candidate));
        let previous_generation = previous.as_ref().map(|previous| previous.generation);
        let previous_snapshot_sha256 = previous
            .as_ref()
            .map(|previous| previous.snapshot_sha256.clone());
        self.previous.store(previous);
        activation_state.highest_generation = generation;
        Ok(WebsiteRuntimeActivationReport {
            changed: true,
            generation,
            snapshot_sha256,
            previous_generation,
            previous_snapshot_sha256,
        })
    }

    pub fn rollback(&self) -> Option<WebsiteRuntimeRollbackReport> {
        let _activation_guard = self.activation_guard();
        let previous = self.previous.swap(None)?;
        let generation = previous.generation;
        let snapshot_sha256 = previous.snapshot_sha256.clone();
        let current = self.current.swap(Some(previous))?;
        Some(WebsiteRuntimeRollbackReport {
            generation,
            snapshot_sha256,
            rolled_back_from_generation: current.generation,
            rolled_back_from_sha256: current.snapshot_sha256.clone(),
        })
    }

    fn activation_guard(&self) -> MutexGuard<'_, WebsiteRuntimeActivationState> {
        self.activation_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn validate_schema(instance: &Value) -> Result<(), WebsiteRuntimeSetError> {
    let validator = SCHEMA_VALIDATOR
        .as_ref()
        .map_err(|error| WebsiteRuntimeSetError::InvalidSchema(error.clone()))?;
    let diagnostics = validator
        .iter_errors(instance)
        .take(MAX_SCHEMA_DIAGNOSTICS)
        .map(|error| {
            ConfigDiagnostic::new(
                error.instance_path().as_str(),
                truncate_diagnostic(&error.to_string()),
            )
        })
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(WebsiteRuntimeSetError::Validation { diagnostics })
    }
}

fn validate_snapshot(snapshot: &WebsiteRuntimeSetSnapshot) -> Result<(), WebsiteRuntimeSetError> {
    let mut diagnostics = Vec::new();
    if snapshot.schema_version != WEBSITE_RUNTIME_SET_SCHEMA_VERSION {
        diagnostics.push(ConfigDiagnostic::new(
            "/schemaVersion",
            format!("only {WEBSITE_RUNTIME_SET_SCHEMA_VERSION} is supported"),
        ));
    }
    if snapshot.kind != WEBSITE_RUNTIME_SET_KIND {
        diagnostics.push(ConfigDiagnostic::new(
            "/kind",
            format!("kind must be {WEBSITE_RUNTIME_SET_KIND}"),
        ));
    }
    for (path, value) in [
        ("/snapshotUuid", snapshot.snapshot_uuid.as_str()),
        ("/nodeUuid", snapshot.node_uuid.as_str()),
    ] {
        if !valid_opaque_id(value) {
            diagnostics.push(ConfigDiagnostic::new(
                path,
                "must be a bounded opaque identifier",
            ));
        }
    }
    if !valid_canonical_timestamp(&snapshot.generated_at) {
        diagnostics.push(ConfigDiagnostic::new(
            "/generatedAt",
            "must use canonical UTC RFC 3339 seconds format",
        ));
    }
    if snapshot.compiler_version.is_empty()
        || snapshot.compiler_version.len() > 128
        || snapshot
            .compiler_version
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        diagnostics.push(ConfigDiagnostic::new(
            "/compilerVersion",
            "must be a non-empty bounded version token",
        ));
    }
    if snapshot.maximum_sites == 0 || snapshot.maximum_sites > HARD_MAXIMUM_SITES {
        diagnostics.push(ConfigDiagnostic::new(
            "/maximumSites",
            format!("must be between 1 and runtime ceiling {HARD_MAXIMUM_SITES}"),
        ));
    }
    if snapshot.generation == 0 || snapshot.generation > HARD_MAXIMUM_GENERATION {
        diagnostics.push(ConfigDiagnostic::new(
            "/generation",
            format!("must be between 1 and runtime ceiling {HARD_MAXIMUM_GENERATION}"),
        ));
    }
    if snapshot.descriptors.len() > snapshot.maximum_sites {
        diagnostics.push(ConfigDiagnostic::new(
            "/descriptors",
            "descriptor count exceeds maximumSites",
        ));
    }
    if snapshot
        .descriptors
        .windows(2)
        .any(|pair| pair[0].app_uuid >= pair[1].app_uuid)
    {
        diagnostics.push(ConfigDiagnostic::new(
            "/descriptors",
            "descriptors must be uniquely ordered by appUuid",
        ));
    }
    for (index, descriptor) in snapshot.descriptors.iter().enumerate() {
        if descriptor.environment != snapshot.environment {
            diagnostics.push(ConfigDiagnostic::new(
                format!("/descriptors/{index}/environment"),
                "descriptor environment must match the runtime set environment",
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(WebsiteRuntimeSetError::Validation { diagnostics })
    }
}

fn valid_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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

fn truncate_diagnostic(message: &str) -> String {
    const MAX_DIAGNOSTIC_BYTES: usize = 512;
    if message.len() <= MAX_DIAGNOSTIC_BYTES {
        return message.to_owned();
    }
    let mut end = MAX_DIAGNOSTIC_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &message[..end])
}
