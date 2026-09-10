//! The cache backend plugin interface.

use async_trait::async_trait;

use crate::record::ResolvedRecord;

/// Storage boundary of the resolution cache component. Every cache layer —
/// in-process memory, distributed Redis, or any future backend — implements
/// this trait and is swapped in by configuration without changing callers
/// (component decoupling, plugin pattern).
///
/// The methods are asynchronous so remote backends can bound their
/// round-trips without blocking executor workers; local layers return
/// immediately inside the async surface.
#[async_trait]
pub trait ResolverCacheBackend: Send + Sync {
    /// Read a record for `domain`; `None` when absent or expired.
    async fn get(&self, domain: &str) -> Option<ResolvedRecord>;

    /// Write a record (positive or negative) with its TTL.
    async fn set(&self, record: ResolvedRecord);

    /// Drop a record (explicit invalidation).
    async fn remove(&self, domain: &str);
}

/// A chain of backends walked in priority order by the resolution chain.
pub(crate) fn normalize_domain(domain: &str) -> String {
    domain.trim().trim_end_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::normalize_domain;

    #[test]
    fn normalizes_case_and_trailing_dot() {
        assert_eq!(normalize_domain("API.Example.COM."), "api.example.com");
        assert_eq!(normalize_domain("  host.local "), "host.local");
    }
}
