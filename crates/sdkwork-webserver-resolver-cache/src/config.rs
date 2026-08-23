//! Resolution cache configuration (authored through the app config or the
//! runtime environment).

use serde::{Deserialize, Serialize};

/// Redis distributed cache configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RedisCacheConfig {
    /// Redis connection URL (`redis://host:port`, `rediss://` for TLS).
    pub url: String,
    /// Record TTL in seconds (per layer default; negative entries use the
    /// chain-level negative TTL).
    #[serde(default = "default_redis_ttl_seconds")]
    pub ttl_seconds: u64,
    /// Key prefix for cluster-wide namespacing.
    #[serde(default = "default_redis_prefix")]
    pub prefix: String,
}

fn default_redis_ttl_seconds() -> u64 {
    300
}

fn default_redis_prefix() -> String {
    "sdkwork:resolver".to_owned()
}

/// Multi-layer resolution cache configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolutionCacheConfig {
    /// Master switch; disabled caches fall straight through to the system
    /// resolver (nginx default behavior).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Local file seed (`/etc/hosts` style or a deployment-exported file).
    /// When set, the file is parsed into the memory layer at startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// In-process memory layer.
    #[serde(default = "default_memory_enabled")]
    pub memory: bool,
    /// Memory entry cap (LRU eviction).
    #[serde(default = "default_memory_max_entries")]
    pub memory_max_entries: usize,
    /// Positive record TTL in seconds for the memory layer.
    #[serde(default = "default_memory_ttl_seconds")]
    pub memory_ttl_seconds: u64,
    /// Redis distributed layer (enabled when present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redis: Option<RedisCacheConfig>,
    /// Database layer (enabled when a `ResolutionDatabase` is supplied).
    #[serde(default)]
    pub database: bool,
    /// Negative cache TTL in seconds (failed resolutions are cached
    /// briefly to absorb upstream DNS outages).
    #[serde(default = "default_negative_ttl_seconds")]
    pub negative_ttl_seconds: u64,
}

fn default_enabled() -> bool {
    true
}

fn default_memory_enabled() -> bool {
    true
}

fn default_memory_max_entries() -> usize {
    65_536
}

fn default_memory_ttl_seconds() -> u64 {
    60
}

fn default_negative_ttl_seconds() -> u64 {
    10
}

impl Default for ResolutionCacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            file: None,
            memory: default_memory_enabled(),
            memory_max_entries: default_memory_max_entries(),
            memory_ttl_seconds: default_memory_ttl_seconds(),
            redis: None,
            database: false,
            negative_ttl_seconds: default_negative_ttl_seconds(),
        }
    }
}
