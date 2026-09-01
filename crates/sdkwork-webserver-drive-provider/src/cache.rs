//! Drive website content local delivery cache (disk-backed LRU).
//!
//! Standard: `DRIVE_SPEC.md` §17 (Website Content Local Delivery Cache) and
//! `DEPLOYMENT_SPEC.md` (container shared host directories). The cache stores
//! immutable, content-addressed Drive website objects on a host-shared
//! directory (default `/opt/deploy/drive/website-cache`) so every webserver
//! instance on the host shares one disk cache through a bind mount.
//!
//! Invariants (fail-safe by construction):
//!
//! - Cache keys bind `tenant_scope_hash + website_root_uuid +
//!   logical_node_version_id`; upstream resolution still runs per request, so
//!   generation/version pinning and conditional-request semantics stay
//!   authoritative (the cache never answers authorization questions).
//! - Entries are immutable: publishing an existing key discards the staging
//!   copy, never overwrites in place. Concurrent publishers converge on
//!   identical bytes because the key binds the content version.
//! - Reads are best-effort: a missing or unreadable file is a miss, never an
//!   error. Cache failures must not break content delivery.
//! - Eviction is LRU over an in-memory index (rebuilt from disk at startup),
//!   bounded by total bytes and entry count. Multi-instance eviction races are
//!   safe: another instance's stale index entry reopens as a miss and
//!   re-fetches.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use sdkwork_webserver_contract::provider::{
    WebsiteProviderContentStream, WebsiteProviderError, WebsiteProviderErrorKind,
    WebsiteProviderResult,
};

use crate::stream::BoundedDriveContentStream;

pub const DRIVE_WEBSITE_CACHE_DEFAULT_ROOT: &str = "/opt/deploy/drive/website-cache";
pub const DRIVE_WEBSITE_CACHE_DEFAULT_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const DRIVE_WEBSITE_CACHE_DEFAULT_MAX_ENTRIES: u64 = 100_000;
const READ_CHUNK_BYTES: usize = 64 * 1024;
const STANDARD_ENVIRONMENTS: [&str; 4] = ["development", "test", "staging", "production"];

/// Resolved cache configuration from the process environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveContentCacheConfig {
    pub root: PathBuf,
    pub environment: String,
    pub max_total_bytes: u64,
    pub max_entries: u64,
}

impl DriveContentCacheConfig {
    /// Environment-driven configuration (see `DRIVE_SPEC.md` §17.4):
    ///
    /// - `SDKWORK_DRIVE_WEBSITE_CACHE_ENABLED`: explicit `false` disables the
    ///   cache; otherwise the cache enables when a root is configured.
    /// - `SDKWORK_DRIVE_WEBSITE_CACHE_ROOT`: shared host directory (default
    ///   `/opt/deploy/drive/website-cache`, bind-mounted from `/opt/deploy/drive`).
    /// - `SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT`: lifecycle environment
    ///   segment; defaults to `SDKWORK_WEBSERVER_ENVIRONMENT` /
    ///   `SDKWORK_ENVIRONMENT` / `development`.
    /// - `SDKWORK_DRIVE_WEBSITE_CACHE_MAX_TOTAL_BYTES` /
    ///   `SDKWORK_DRIVE_WEBSITE_CACHE_MAX_ENTRIES`: LRU bounds.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub(crate) fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Option<Self> {
        let enabled = lookup("SDKWORK_DRIVE_WEBSITE_CACHE_ENABLED");
        if let Some(value) = enabled.as_deref() {
            if matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            ) {
                return None;
            }
        }
        let root_env = lookup("SDKWORK_DRIVE_WEBSITE_CACHE_ROOT")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if enabled.is_none() && root_env.is_none() {
            // No explicit root and no explicit opt-in: local runtimes stay
            // cache-free (no surprise disk writes); containers set the root.
            return None;
        }
        let root =
            PathBuf::from(root_env.unwrap_or_else(|| DRIVE_WEBSITE_CACHE_DEFAULT_ROOT.to_string()));
        let environment = sanitize_environment(
            lookup("SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT")
                .or_else(|| lookup("SDKWORK_WEBSERVER_ENVIRONMENT"))
                .or_else(|| lookup("SDKWORK_ENVIRONMENT"))
                .as_deref(),
        );
        let max_total_bytes = parse_positive(
            lookup("SDKWORK_DRIVE_WEBSITE_CACHE_MAX_TOTAL_BYTES").as_deref(),
            DRIVE_WEBSITE_CACHE_DEFAULT_MAX_TOTAL_BYTES,
        );
        let max_entries = parse_positive(
            lookup("SDKWORK_DRIVE_WEBSITE_CACHE_MAX_ENTRIES").as_deref(),
            DRIVE_WEBSITE_CACHE_DEFAULT_MAX_ENTRIES,
        );
        Some(Self {
            root,
            environment,
            max_total_bytes,
            max_entries,
        })
    }
}

fn sanitize_environment(value: Option<&str>) -> String {
    let raw = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("development")
        .to_ascii_lowercase();
    let sanitized: String = raw
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect();
    if STANDARD_ENVIRONMENTS.contains(&sanitized.as_str())
        || (sanitized.len() <= 32 && !sanitized.is_empty())
    {
        sanitized
    } else {
        sanitized.chars().take(32).collect()
    }
}

fn parse_positive(raw: Option<&str>, default: u64) -> u64 {
    raw.and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// Content-addressed cache key for one immutable Drive website object.
pub fn cache_key(
    tenant_scope_hash: &str,
    website_root_uuid: &str,
    node_version_id: &str,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(tenant_scope_hash.as_bytes());
    hasher.update([0]);
    hasher.update(website_root_uuid.as_bytes());
    hasher.update([0]);
    hasher.update(node_version_id.as_bytes());
    hasher.finalize().into()
}

struct CacheEntry {
    bytes: u64,
    last_access: u64,
}

#[derive(Default)]
struct CacheIndex {
    entries: HashMap<[u8; 32], CacheEntry>,
    total_bytes: u64,
    access_counter: u64,
}

/// Disk-backed LRU cache of immutable Drive website content.
pub struct DriveContentCache {
    config: DriveContentCacheConfig,
    index: std::sync::Mutex<CacheIndex>,
}

impl DriveContentCache {
    /// Opens (and re-indexes) the cache; returns `None` when the cache is
    /// disabled or the root cannot be created, in which case delivery keeps
    /// streaming from the Drive facade.
    pub fn open(config: DriveContentCacheConfig) -> Option<Arc<Self>> {
        let entries_root = entries_root(&config);
        if std::fs::create_dir_all(&entries_root).is_err() {
            tracing::warn!(
                root = %config.root.display(),
                "drive delivery cache root unavailable; cache disabled"
            );
            return None;
        }
        let staging_root = staging_root(&config);
        if std::fs::create_dir_all(&staging_root).is_err() {
            tracing::warn!(
                root = %config.root.display(),
                "drive delivery cache staging unavailable; cache disabled"
            );
            return None;
        }
        let cache = Arc::new(Self {
            config,
            index: std::sync::Mutex::new(CacheIndex::default()),
        });
        cache.rebuild_index();
        Some(cache)
    }

    /// Configuration from process environment; `None` when disabled.
    pub fn from_env() -> Option<Arc<Self>> {
        let config = DriveContentCacheConfig::from_env()?;
        Self::open(config)
    }

    fn rebuild_index(&self) {
        let entries_root = entries_root(&self.config);
        let mut index = self.lock_index();
        index.entries.clear();
        index.total_bytes = 0;
        let shards = match std::fs::read_dir(&entries_root) {
            Ok(shards) => shards,
            Err(error) => {
                tracing::warn!(error = %error, "drive delivery cache index rebuild failed");
                return;
            }
        };
        for shard in shards.flatten() {
            let shard_path = shard.path();
            if !shard_path.is_dir() {
                continue;
            }
            let leaves = match std::fs::read_dir(&shard_path) {
                Ok(leaves) => leaves,
                Err(_) => continue,
            };
            for leaf in leaves.flatten() {
                let leaf_path = leaf.path();
                if !leaf_path.is_dir() {
                    continue;
                }
                let objects = match std::fs::read_dir(&leaf_path) {
                    Ok(objects) => objects,
                    Err(_) => continue,
                };
                for object in objects.flatten() {
                    let path = object.path();
                    if path.extension().and_then(|value| value.to_str()) != Some("body") {
                        continue;
                    }
                    let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                        continue;
                    };
                    let Ok(key) = decode_key(stem) else {
                        continue;
                    };
                    let bytes = object.metadata().map(|meta| meta.len()).unwrap_or(0);
                    if index.entries.contains_key(&key) {
                        continue;
                    }
                    index.total_bytes += bytes;
                    index.entries.insert(
                        key,
                        CacheEntry {
                            bytes,
                            last_access: 0,
                        },
                    );
                }
            }
        }
        tracing::debug!(
            entries = index.entries.len(),
            bytes = index.total_bytes,
            "drive delivery cache index rebuilt"
        );
    }

    fn lock_index(&self) -> std::sync::MutexGuard<'_, CacheIndex> {
        self.index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Absolute path of one cached object body, without touching LRU state.
    pub fn body_path(&self, key: &[u8; 32]) -> PathBuf {
        let hex = hex::encode(key);
        entries_root(&self.config)
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(format!("{hex}.body"))
    }

    /// Marks one key as recently used and returns its body path. The file may
    /// still be missing (evicted by a peer instance); callers treat open
    /// failures as misses via [`Self::open_cached`].
    pub fn touch(&self, key: &[u8; 32]) -> PathBuf {
        let path = self.body_path(key);
        let mut index = self.lock_index();
        index.access_counter += 1;
        let access = index.access_counter;
        if let Some(entry) = index.entries.get_mut(key) {
            entry.last_access = access;
        }
        path
    }

    /// Opens the full cached object; `None` on any miss or read failure.
    pub async fn open_cached(
        &self,
        key: &[u8; 32],
        expected_length: u64,
    ) -> Option<CachedFileStream> {
        let path = self.touch(key);
        let file = tokio::fs::File::open(&path).await.ok()?;
        let metadata = file.metadata().await.ok()?;
        if u64::from(metadata.len()) != expected_length {
            // Truncated or stale entry: drop it from the index and refetch.
            self.forget(key);
            return None;
        }
        Some(CachedFileStream {
            file,
            remaining: expected_length,
        })
    }

    /// Opens a byte range of the cached object; `None` on miss or failure.
    pub async fn open_cached_range(
        &self,
        key: &[u8; 32],
        start: u64,
        end_inclusive: u64,
    ) -> Option<CachedFileStream> {
        if end_inclusive < start {
            return None;
        }
        let path = self.touch(key);
        let mut file = tokio::fs::File::open(&path).await.ok()?;
        if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
            return None;
        }
        Some(CachedFileStream {
            file,
            remaining: end_inclusive - start + 1,
        })
    }

    fn forget(&self, key: &[u8; 32]) {
        let mut index = self.lock_index();
        if let Some(entry) = index.entries.remove(key) {
            index.total_bytes = index.total_bytes.saturating_sub(entry.bytes);
        }
        let _ = std::fs::remove_file(self.body_path(key));
    }

    /// Unique staging path for one fill; collisions across processes are
    /// impossible because the suffix is process-unique.
    pub fn staging_path(&self, key: &[u8; 32]) -> PathBuf {
        let hex = hex::encode(key);
        staging_root(&self.config).join(format!("{hex}.{}.part", std::process::id()))
    }

    /// Publishes a completed staging file as the cache entry for `key`.
    /// Content-addressed immutability: an existing entry wins, the staging
    /// copy is discarded. Runs LRU eviction after admission.
    pub fn publish(&self, key: &[u8; 32], staging: &Path) -> Option<PathBuf> {
        let bytes = match std::fs::metadata(staging) {
            Ok(metadata) => metadata.len(),
            Err(_) => {
                // The fill failed or was abandoned; nothing to publish.
                let _ = std::fs::remove_file(staging);
                return None;
            }
        };
        if bytes > self.config.max_total_bytes {
            // A single object larger than the whole cache can never fit.
            let _ = std::fs::remove_file(staging);
            return None;
        }
        let destination = self.body_path(key);
        let mut index = self.lock_index();
        if index.entries.contains_key(key) {
            // Immutable content already cached; drop the duplicate staging copy.
            drop(index);
            let _ = std::fs::remove_file(staging);
            return Some(destination);
        }
        if let Some(parent) = destination.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                drop(index);
                let _ = std::fs::remove_file(staging);
                return None;
            }
        }
        // Windows rename fails onto an existing target; the contains_key check
        // above already ruled out in-process duplicates, and cross-process
        // duplicates converge on identical bytes — a failed rename is harmless.
        if std::fs::rename(staging, &destination).is_err() {
            drop(index);
            let _ = std::fs::remove_file(staging);
            return None;
        }
        index.access_counter += 1;
        let access = index.access_counter;
        index.entries.insert(
            *key,
            CacheEntry {
                bytes,
                last_access: access,
            },
        );
        index.total_bytes += bytes;
        self.evict_locked(&mut index);
        Some(destination)
    }

    fn evict_locked(&self, index: &mut CacheIndex) {
        while index.entries.len() as u64 > self.config.max_entries
            || index.total_bytes > self.config.max_total_bytes
        {
            let Some(victim) = index
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(key, _)| *key)
            else {
                break;
            };
            let entry_bytes = index
                .entries
                .remove(&victim)
                .map(|entry| entry.bytes)
                .unwrap_or(0);
            index.total_bytes = index.total_bytes.saturating_sub(entry_bytes);
            let _ = std::fs::remove_file(self.body_path(&victim));
        }
    }

    pub fn config(&self) -> &DriveContentCacheConfig {
        &self.config
    }

    /// Wraps an upstream fill stream so forwarded bytes tee into the cache
    /// staging area. The forwarded response never waits on cache I/O failures:
    /// recording is best-effort and publication happens at clean EOF.
    pub(crate) fn fill_stream(
        self: &Arc<Self>,
        key: [u8; 32],
        inner: BoundedDriveContentStream,
    ) -> CachingContentStream {
        let staging = self.staging_path(&key);
        CachingContentStream {
            inner,
            cache: Arc::clone(self),
            key,
            staging,
            file: None,
            written: 0,
            failed: false,
        }
    }
}

fn entries_root(config: &DriveContentCacheConfig) -> PathBuf {
    config.root.join(&config.environment).join("entries")
}

fn staging_root(config: &DriveContentCacheConfig) -> PathBuf {
    config.root.join(&config.environment).join("staging")
}

fn decode_key(stem: &str) -> Result<[u8; 32], ()> {
    let bytes = hex::decode(stem).map_err(|_| ())?;
    bytes.try_into().map_err(|_| ())
}

/// File-backed stream over one cached object (full object or byte range).
pub struct CachedFileStream {
    file: tokio::fs::File,
    remaining: u64,
}

#[async_trait::async_trait]
impl WebsiteProviderContentStream for CachedFileStream {
    async fn next_chunk(&mut self) -> WebsiteProviderResult<Option<Vec<u8>>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let mut buffer = vec![0u8; READ_CHUNK_BYTES.min(self.remaining.max(1) as usize)];
        let read = self
            .file
            .read(&mut buffer)
            .await
            .map_err(|_| WebsiteProviderError::new(WebsiteProviderErrorKind::Unavailable))?;
        if read == 0 {
            // Short read against the declared length: contract violation.
            return Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::ContractMismatch,
            ));
        }
        let read = read.min(self.remaining as usize);
        if read == 0 || read as u64 > self.remaining {
            return Err(contract_mismatch());
        }
        self.remaining -= read as u64;
        buffer.truncate(read);
        Ok(Some(buffer))
    }
}

fn contract_mismatch() -> WebsiteProviderError {
    WebsiteProviderError::new(WebsiteProviderErrorKind::ContractMismatch)
}

/// Teeing fill stream: forwards the bounded upstream stream while recording
/// bytes into the cache staging file, publishing at clean EOF.
pub struct CachingContentStream {
    inner: BoundedDriveContentStream,
    cache: Arc<DriveContentCache>,
    key: [u8; 32],
    staging: PathBuf,
    file: Option<tokio::fs::File>,
    written: u64,
    failed: bool,
}

impl CachingContentStream {
    async fn record(&mut self, chunk: &[u8]) {
        if self.failed {
            return;
        }
        if self.file.is_none() {
            match tokio::fs::File::create(&self.staging).await {
                Ok(file) => self.file = Some(file),
                Err(_) => {
                    self.failed = true;
                    return;
                }
            }
        }
        if let Some(file) = self.file.as_mut() {
            if file.write_all(chunk).await.is_err() {
                self.failed = true;
                self.file = None;
                let _ = std::fs::remove_file(&self.staging);
            } else {
                self.written += chunk.len() as u64;
            }
        }
    }
}

#[async_trait::async_trait]
impl WebsiteProviderContentStream for CachingContentStream {
    async fn next_chunk(&mut self) -> WebsiteProviderResult<Option<Vec<u8>>> {
        match self.inner.next_chunk().await {
            Ok(Some(chunk)) => {
                self.record(&chunk).await;
                Ok(Some(chunk))
            }
            Ok(None) => {
                // Clean EOF: close the staging file, then publish atomically.
                self.file = None;
                if !self.failed {
                    self.cache.publish(&self.key, &self.staging);
                }
                Ok(None)
            }
            Err(error) => {
                // Upstream failure: abandon the partial staging copy.
                self.failed = true;
                self.file = None;
                let _ = std::fs::remove_file(&self.staging);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(name: &str) -> DriveContentCacheConfig {
        let root = std::env::temp_dir().join(format!(
            "sdkwork-drive-cache-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        DriveContentCacheConfig {
            root,
            environment: "test".to_string(),
            max_total_bytes: 1024 * 1024,
            max_entries: 8,
        }
    }

    fn write_staging(cache: &DriveContentCache, key: &[u8; 32], payload: &[u8]) -> PathBuf {
        let staging = cache.staging_path(key);
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::write(&staging, payload).unwrap();
        staging
    }

    #[test]
    fn config_from_env_enables_with_root_and_disables_explicitly() {
        let disabled = DriveContentCacheConfig::from_lookup(|_| None);
        assert!(disabled.is_none(), "no env -> disabled");
        let explicit_off = DriveContentCacheConfig::from_lookup(|key| {
            if key == "SDKWORK_DRIVE_WEBSITE_CACHE_ENABLED" {
                Some("false".to_string())
            } else {
                None
            }
        });
        assert!(explicit_off.is_none(), "explicit false -> disabled");
        let enabled = DriveContentCacheConfig::from_lookup(|key| match key {
            "SDKWORK_DRIVE_WEBSITE_CACHE_ROOT" => {
                Some("/opt/deploy/drive/website-cache".to_string())
            }
            "SDKWORK_WEBSERVER_ENVIRONMENT" => Some("production".to_string()),
            _ => None,
        });
        let config = enabled.expect("root env enables cache");
        assert_eq!(config.environment, "production");
        assert_eq!(
            config.max_total_bytes,
            DRIVE_WEBSITE_CACHE_DEFAULT_MAX_TOTAL_BYTES
        );
    }

    #[test]
    fn config_rejects_path_unsafe_environment_segments() {
        let config = DriveContentCacheConfig::from_lookup(|key| match key {
            "SDKWORK_DRIVE_WEBSITE_CACHE_ROOT" => Some("/tmp/cache".to_string()),
            "SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT" => Some("../../etc".to_string()),
            _ => None,
        })
        .expect("config");
        assert!(!config.environment.contains('/') && !config.environment.contains('.'));
    }

    #[tokio::test]
    async fn publish_open_round_trip_and_immutability() {
        let config = temp_config("round-trip");
        let cache = DriveContentCache::open(config).expect("cache");
        let key = cache_key("tenant", "root-uuid", "node-version-1");
        let staging = write_staging(&cache, &key, b"hello drive");
        let published = cache.publish(&key, &staging).expect("published");
        assert!(!staging.exists(), "staging consumed by publish");
        let mut stream = cache
            .open_cached(&key, 11)
            .await
            .expect("cached full object");
        let mut collected = Vec::new();
        while let Some(chunk) = stream.next_chunk().await.unwrap() {
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(collected, b"hello drive");

        // Re-publishing the same key discards the staging copy: immutable.
        let duplicate = write_staging(&cache, &key, b"other bytes");
        let _ = cache.publish(&key, &duplicate);
        assert!(!duplicate.exists(), "duplicate staging copy discarded");
        assert_eq!(
            std::fs::read(published).unwrap(),
            b"hello drive",
            "existing entry never overwritten"
        );
        let _ = std::fs::remove_dir_all(cache.config().root.clone());
    }

    #[tokio::test]
    async fn lru_eviction_bounds_entries_and_bytes() {
        let mut config = temp_config("eviction");
        config.max_entries = 2;
        config.max_total_bytes = 32;
        let cache = DriveContentCache::open(config).expect("cache");
        let mut keys = Vec::new();
        for index in 0..4u8 {
            let key = cache_key("tenant", "root", &format!("node-{index}"));
            let staging = write_staging(&cache, &key, &[index; 8]);
            cache.publish(&key, &staging).expect("publish");
            keys.push(key);
        }
        let index = cache.lock_index();
        assert!(
            index.entries.len() <= 2,
            "entries bounded by max_entries: {}",
            index.entries.len()
        );
        assert!(index.total_bytes <= 32, "bytes bounded by max_total_bytes");
        drop(index);
        // The oldest entry (node-0) must be gone.
        assert!(
            cache.open_cached(&keys[0], 8).await.is_none(),
            "least recently used entry evicted"
        );
        let _ = std::fs::remove_dir_all(cache.config().root.clone());
    }

    #[tokio::test]
    async fn access_refreshes_recency() {
        let mut config = temp_config("recency");
        config.max_entries = 2;
        let cache = DriveContentCache::open(config).expect("cache");
        let first = cache_key("tenant", "root", "first");
        let second = cache_key("tenant", "root", "second");
        let third = cache_key("tenant", "root", "third");
        for (key, payload) in [(&first, b"a".to_vec()), (&second, b"b".to_vec())] {
            let staging = write_staging(&cache, key, &payload);
            cache.publish(key, &staging).expect("publish");
        }
        // Touch `first` so `second` becomes the LRU victim.
        cache.touch(&first);
        let staging = write_staging(&cache, &third, b"c");
        cache.publish(&third, &staging).expect("publish");
        assert!(
            cache.open_cached(&first, 1).await.is_some(),
            "recent entry kept"
        );
        assert!(
            cache.open_cached(&second, 1).await.is_none(),
            "LRU victim evicted"
        );
        let _ = std::fs::remove_dir_all(cache.config().root.clone());
    }

    #[tokio::test]
    async fn range_reads_serve_from_cached_object() {
        let config = temp_config("range");
        let cache = DriveContentCache::open(config).expect("cache");
        let key = cache_key("tenant", "root", "range-node");
        let payload: Vec<u8> = (0..64u8).collect();
        let staging = write_staging(&cache, &key, &payload);
        cache.publish(&key, &staging).expect("publish");
        let mut stream = cache
            .open_cached_range(&key, 10, 19)
            .await
            .expect("range from cache");
        let mut collected = Vec::new();
        while let Some(chunk) = stream.next_chunk().await.unwrap() {
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(collected, (10..=19u8).collect::<Vec<u8>>());
        let _ = std::fs::remove_dir_all(cache.config().root.clone());
    }

    #[tokio::test]
    async fn fill_stream_publishes_at_clean_eof_and_abandons_on_error() {
        // Build a bounded stream over the contract types without a Drive SDK.
        // The simplest route: construct BoundedDriveContentStream through a
        // local chunk stream implementing the SDK trait.
        struct StaticChunks {
            chunks: Vec<Option<Vec<u8>>>,
        }
        #[async_trait::async_trait]
        impl crate::sdk::DriveContentChunkStream for StaticChunks {
            async fn next_chunk(
                &mut self,
            ) -> Result<Option<Vec<u8>>, sdkwork_drive_internal_sdk::SdkworkError> {
                Ok(self.chunks.pop().flatten())
            }
        }

        let config = temp_config("fill");
        let cache = DriveContentCache::open(config).expect("cache");
        let key = cache_key("tenant", "root", "fill-node");
        let upstream = BoundedDriveContentStream::new(
            Box::new(StaticChunks {
                chunks: vec![None, Some(b"world".to_vec()), Some(b"hello ".to_vec())],
            }),
            11,
        );
        let mut stream = cache.fill_stream(key, upstream);
        let mut collected = Vec::new();
        while let Some(chunk) = stream.next_chunk().await.unwrap() {
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(collected, b"hello world");
        let mut cached = cache.open_cached(&key, 11).await.expect("published on EOF");
        let mut round_tripped = Vec::new();
        while let Some(chunk) = cached.next_chunk().await.unwrap() {
            round_tripped.extend_from_slice(&chunk);
        }
        assert_eq!(round_tripped, b"hello world");

        // Failure path: a length violation aborts the fill and leaves no entry.
        let bad_key = cache_key("tenant", "root", "fill-bad");
        let bad_upstream = BoundedDriveContentStream::new(
            Box::new(StaticChunks {
                chunks: vec![None, Some(b"toolong".to_vec())],
            }),
            3,
        );
        let mut bad_stream = cache.fill_stream(bad_key, bad_upstream);
        assert!(bad_stream.next_chunk().await.is_err());
        assert!(
            cache.open_cached(&bad_key, 7).await.is_none(),
            "failed fills never publish"
        );
        let _ = std::fs::remove_dir_all(cache.config().root.clone());
    }
}
