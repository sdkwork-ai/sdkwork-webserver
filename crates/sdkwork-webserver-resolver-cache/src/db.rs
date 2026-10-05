//! Database layer of the resolution cache.

use crate::record::ResolvedRecord;

/// Database persistence boundary. The SQL implementation lives in the
/// webserver repository crate (`webserver_resolution_cache` table) so the cache
/// component itself stays database-agnostic; the deploy control plane
/// (`sdkwork-deployments`) seeds and maintains the same table.
#[async_trait::async_trait]
pub trait ResolutionDatabase: Send + Sync {
    /// Load a record for `domain`; `None` when absent or expired.
    async fn load(&self, domain: &str) -> Option<ResolvedRecord>;

    /// Persist a record with its TTL (`expires_at_unix`). Deploy and data
    /// plane callers share this table, so writes are idempotent upserts.
    async fn save(&self, record: ResolvedRecord);

    /// Drop any stored record for `domain` (cache invalidation). A no-op
    /// default so backends without delete support can opt out; without this,
    /// `ResolutionChain::invalidate` clears memory and Redis but the database
    /// layer keeps serving the stale row until its own `expires_at_unix`.
    async fn remove(&self, _domain: &str) {}
}
