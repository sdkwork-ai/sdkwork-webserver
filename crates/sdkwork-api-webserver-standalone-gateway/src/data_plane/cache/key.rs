//! Canonical cache keys and the Vary input fingerprint.

use std::collections::BTreeMap;

/// A request fingerprint that identifies one cacheable representation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    pub method: String,
    pub host: String,
    pub path: String,
    pub query: String,
    /// Vary input: normalized request header values the cached response
    /// varies on (lowercase names, sorted).
    pub vary_input: BTreeMap<String, String>,
}

impl CacheKey {
    pub fn new(
        method: &str,
        host: &str,
        path: &str,
        query: Option<&str>,
        vary_names: &[String],
        request_headers: &axum::http::HeaderMap,
    ) -> Self {
        let mut vary_input = BTreeMap::new();
        for name in vary_names {
            let lower = name.to_ascii_lowercase();
            if let Some(value) = request_headers
                .get(&lower)
                .and_then(|value| value.to_str().ok())
            {
                vary_input.insert(lower, value.trim().to_owned());
            }
        }
        Self {
            method: method.to_owned(),
            host: host.to_owned(),
            path: path.to_owned(),
            query: query.unwrap_or_default().to_owned(),
            vary_input,
        }
    }

    /// Stable hex digest for durable object filenames (`proxy_cache_path`).
    pub fn stable_hash_hex(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.method.as_bytes());
        hasher.update([0]);
        hasher.update(self.host.as_bytes());
        hasher.update([0]);
        hasher.update(self.path.as_bytes());
        hasher.update([0]);
        hasher.update(self.query.as_bytes());
        for (name, value) in &self.vary_input {
            hasher.update([0]);
            hasher.update(name.as_bytes());
            hasher.update([0]);
            hasher.update(value.as_bytes());
        }
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// Normalize a `Vary` response header into its field list.
pub(crate) fn parse_vary_header(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty() && !token.eq_ignore_ascii_case("*"))
        .map(str::to_owned)
        .collect()
}
