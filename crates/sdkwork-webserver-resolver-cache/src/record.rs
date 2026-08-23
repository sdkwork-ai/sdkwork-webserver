//! The resolution cache value type.

use serde::{Deserialize, Serialize};

/// One cached resolution for a hostname. An empty `addresses` list with
/// `negative = true` records a negative cache entry (the upstream failed to
/// resolve); negative entries expire after the configured short TTL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRecord {
    /// Normalized lowercase hostname.
    pub domain: String,
    /// Resolved IP addresses (IPv4/IPv6 literals).
    pub addresses: Vec<String>,
    /// True when this entry records a failed resolution (negative cache).
    pub negative: bool,
    /// Unix seconds at which this entry expires.
    pub expires_at_unix: u64,
}

impl ResolvedRecord {
    pub fn fresh(
        domain: impl Into<String>,
        addresses: Vec<String>,
        ttl_seconds: u64,
        now_unix: u64,
    ) -> Self {
        Self {
            domain: domain.into(),
            addresses,
            negative: false,
            expires_at_unix: now_unix.saturating_add(ttl_seconds),
        }
    }

    pub fn negative(domain: impl Into<String>, ttl_seconds: u64, now_unix: u64) -> Self {
        Self {
            domain: domain.into(),
            addresses: Vec::new(),
            negative: true,
            expires_at_unix: now_unix.saturating_add(ttl_seconds),
        }
    }

    pub fn expired(&self, now_unix: u64) -> bool {
        now_unix >= self.expires_at_unix
    }
}
