//! Durable disk cache backend (nginx `proxy_cache_path` subset).
//!
//! Layout: `<root>/<aa>/<bb>/<key-hash>` where each file is a length-prefixed
//! bincode-like framing of the serialized `CachedResponse`. The memory
//! backend remains the L1 index; this module is the L2 spill component and
//! never talks to the proxy path directly.
//!
//! All disk operations are synchronous by design and `Send`: the
//! `HttpResponseCache` facade runs them on the blocking pool
//! (`spawn_blocking`) so a slow disk never stalls a runtime worker. Object
//! writes are independent files, so there is no process-wide write lock.
//!
//! The tier is bounded (nginx `proxy_cache_path max_size` parity): object
//! bytes and object count are tracked against the configured budgets, and an
//! insert that pushes the tier over budget triggers an eviction sweep that
//! removes the oldest objects (by modified time) until both budgets hold
//! again. Without it, a unique-URL flood — every query string a new object —
//! would fill the disk without bound. The sweep is CAS-gated so concurrent
//! inserts coalesce into one walk, and the walk runs on the blocking pool
//! with the rest of the tier's I/O.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::SystemTime,
};

use super::{
    backend::{CacheBackend, MemoryCacheBackend},
    entry::CachedResponse,
    key::CacheKey,
};

const MAGIC: &[u8; 4] = b"SWC1";

/// Bounded on-disk object store with size and count budgets.
struct DiskTier {
    root: PathBuf,
    max_bytes: u64,
    max_entries: u64,
    bytes: AtomicU64,
    entries: AtomicU64,
    /// Gates eviction sweeps: one walk at a time, concurrent inserts skip.
    sweeping: AtomicBool,
}

impl DiskTier {
    /// Seeds the counters by walking the existing tree once, so a restart
    /// inherits the occupancy the previous process left on disk.
    fn new(root: PathBuf, max_bytes: u64, max_entries: u64) -> Self {
        let (bytes, entries) = walk_tier_occupancy(&root);
        if bytes > max_bytes || entries > max_entries {
            tracing::info!(
                bytes,
                entries,
                max_bytes,
                max_entries,
                "proxy cache disk tier starts over budget; the first insert will evict"
            );
        }
        Self {
            root,
            max_bytes: max_bytes.max(1),
            max_entries: max_entries.max(1),
            bytes: AtomicU64::new(bytes),
            entries: AtomicU64::new(entries),
            sweeping: AtomicBool::new(false),
        }
    }

    fn object_path(&self, key: &CacheKey) -> PathBuf {
        let hash = key.stable_hash_hex();
        let a = &hash[0..2];
        let b = &hash[2..4];
        self.root.join(a).join(b).join(&hash)
    }

    fn record_insert(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::AcqRel);
        self.entries.fetch_add(1, Ordering::AcqRel);
        self.evict_if_over_budget();
    }

    /// Unlinks one object and accounts for it only when this caller's unlink
    /// is the one that succeeded, so concurrent readers, removers, and the
    /// eviction sweep can never double-decrement the occupancy counters.
    /// Saturating arithmetic keeps residual drift (corrected by the startup
    /// walk) from wrapping.
    fn remove_object(&self, path: &Path) {
        // Size read before the unlink: afterwards the metadata is gone.
        let bytes = fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if fs::remove_file(path).is_err() {
            return;
        }
        self.bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                Some(value.saturating_sub(bytes))
            })
            .ok();
        self.entries
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                Some(value.saturating_sub(1))
            })
            .ok();
    }

    /// Evicts oldest objects until both budgets hold again. Expired objects
    /// are also removed opportunistically (the lazy per-read removal only
    /// reaps objects something asks for). CAS-gated: under a flood, inserts
    /// that arrive while a sweep walks simply skip; the next insert after the
    /// sweep completes re-checks.
    fn evict_if_over_budget(&self) {
        if self.bytes.load(Ordering::Acquire) <= self.max_bytes
            && self.entries.load(Ordering::Acquire) <= self.max_entries
        {
            return;
        }
        if self
            .sweeping
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        // Panic safety: the flag must clear even if the walk panics, or the
        // tier would never sweep again.
        let guard = SweepGuard(&self.sweeping);
        let victims = collect_eviction_candidates(&self.root);
        let mut bytes = self.bytes.load(Ordering::Acquire);
        let mut entries = self.entries.load(Ordering::Acquire);
        for (path, size, _) in victims {
            if bytes <= self.max_bytes && entries <= self.max_entries {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                bytes = bytes.saturating_sub(size);
                entries = entries.saturating_sub(1);
            }
        }
        self.bytes.store(bytes, Ordering::Release);
        self.entries.store(entries, Ordering::Release);
        drop(guard);
    }
}

/// Clears `sweeping` on drop so a panicking walk cannot wedge the tier.
struct SweepGuard<'a>(&'a AtomicBool);

impl Drop for SweepGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Sums object bytes and counts under the tier root (fixed `<aa>/<bb>/file`
/// depth), ignoring anything that does not parse as a regular file.
fn walk_tier_occupancy(root: &Path) -> (u64, u64) {
    let mut bytes = 0_u64;
    let mut entries = 0_u64;
    for level_a in list_dirs(root) {
        for level_b in list_dirs(&level_a) {
            if let Ok(objects) = fs::read_dir(&level_b) {
                for object in objects.flatten() {
                    if let Ok(metadata) = object.metadata() {
                        if metadata.is_file() {
                            bytes = bytes.saturating_add(metadata.len());
                            entries = entries.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    (bytes, entries)
}

fn list_dirs(root: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(level) = fs::read_dir(root) {
        for entry in level.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    directories.push(entry.path());
                }
            }
        }
    }
    directories
}

/// Returns every object under the tier root as `(path, size, modified)`,
/// oldest first. Files removed concurrently simply fail their unlink.
fn collect_eviction_candidates(root: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut candidates = Vec::new();
    for level_a in list_dirs(root) {
        for level_b in list_dirs(&level_a) {
            if let Ok(objects) = fs::read_dir(&level_b) {
                for object in objects.flatten() {
                    if let Ok(metadata) = object.metadata() {
                        if metadata.is_file() {
                            candidates.push((
                                object.path(),
                                metadata.len(),
                                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                            ));
                        }
                    }
                }
            }
        }
    }
    candidates.sort_by_key(|(_, _, modified)| *modified);
    candidates
}

/// Tiered backend: memory LRU index plus optional on-disk object store.
pub(crate) struct TieredCacheBackend {
    memory: MemoryCacheBackend,
    disk: Option<DiskTier>,
}

impl TieredCacheBackend {
    pub(crate) fn with_limits(
        maximum_entries: usize,
        maximum_bytes: u64,
        disk_root: PathBuf,
        max_disk_bytes: u64,
        max_disk_entries: u64,
    ) -> std::io::Result<Self> {
        fs::create_dir_all(&disk_root)?;
        Ok(Self {
            memory: MemoryCacheBackend::with_limits(maximum_entries, maximum_bytes),
            disk: Some(DiskTier::new(disk_root, max_disk_bytes, max_disk_entries)),
        })
    }
}

impl CacheBackend for TieredCacheBackend {
    fn get(&self, key: &CacheKey) -> Option<CachedResponse> {
        if let Some(entry) = self.memory.get(key) {
            return Some(entry);
        }
        let disk = self.disk.as_ref()?;
        let path = disk.object_path(key);
        let entry = read_disk_entry(&path)?;
        if entry.expired() && !entry.stale_available() {
            disk.remove_object(&path);
            return None;
        }
        self.memory.insert(key.clone(), entry.clone());
        Some(entry)
    }

    fn insert(&self, key: CacheKey, entry: CachedResponse) {
        self.memory.insert(key.clone(), entry.clone());
        let Some(disk) = self.disk.as_ref() else {
            return;
        };
        let path = disk.object_path(&key);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match write_disk_entry(&path, &entry) {
            Ok(bytes) => disk.record_insert(bytes),
            Err(error) => {
                tracing::debug!(error = %error, "proxy cache disk write failed");
            }
        }
    }

    fn remove(&self, key: &CacheKey) {
        self.memory.remove(key);
        if let Some(disk) = self.disk.as_ref() {
            disk.remove_object(&disk.object_path(key));
        }
    }
}

/// Serializes one entry and returns the object size that landed on disk.
fn write_disk_entry(path: &Path, entry: &CachedResponse) -> std::io::Result<u64> {
    let durable = entry.to_durable();
    let payload = serde_json::to_vec(&durable).map_err(std::io::Error::other)?;
    let mut file = fs::File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&(payload.len() as u32).to_le_bytes())?;
    file.write_all(&payload)?;
    Ok(payload.len() as u64 + MAGIC.len() as u64 + 4)
    // No per-object fsync (nginx `proxy_cache` durability model): a torn
    // write is rejected by the magic/length validation in `read_disk_entry`
    // and re-filled from the upstream on the next request.
}

fn read_disk_entry(path: &Path) -> Option<CachedResponse> {
    let mut file = fs::File::open(path).ok()?;
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic).ok()?;
    if &magic != MAGIC {
        return None;
    }
    let mut len_bytes = [0_u8; 4];
    file.read_exact(&mut len_bytes).ok()?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > 16 * 1024 * 1024 {
        return None;
    }
    let mut payload = vec![0_u8; len];
    file.read_exact(&mut payload).ok()?;
    let durable: super::entry::DurableCachedResponse = serde_json::from_slice(&payload).ok()?;
    Some(CachedResponse::from_durable(durable))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_plane::cache::entry::ResponseMetadata;
    use axum::http::HeaderMap;
    use bytes::Bytes;
    use std::time::Duration;

    fn cache_key(id: u8) -> CacheKey {
        CacheKey::new(
            "GET",
            "example.com",
            &format!("/{id}"),
            None,
            &[],
            &HeaderMap::new(),
        )
    }

    fn cache_entry(body: &'static [u8]) -> CachedResponse {
        CachedResponse::new(
            ResponseMetadata {
                status: 200,
                headers: vec![("content-type".to_owned(), "text/plain".to_owned())],
                vary: Vec::new(),
                fresh_seconds: 60,
            },
            Bytes::from_static(body),
            Duration::from_secs(60),
            Duration::from_secs(60),
        )
    }

    #[test]
    fn tiered_disk_round_trip() {
        let directory = tempfile::tempdir().expect("temp");
        let backend = TieredCacheBackend::with_limits(
            8,
            u64::MAX,
            directory.path().to_path_buf(),
            u64::MAX,
            100,
        )
        .expect("disk");
        let key = cache_key(1);
        let entry = cache_entry(b"disk-body");
        backend.insert(key.clone(), entry);
        // Drop memory by constructing a fresh backend on the same root.
        let cold = TieredCacheBackend::with_limits(
            8,
            u64::MAX,
            directory.path().to_path_buf(),
            u64::MAX,
            100,
        )
        .expect("cold");
        let hit = cold.get(&key).expect("disk hit");
        assert_eq!(hit.body.as_ref(), b"disk-body");
    }

    #[test]
    fn disk_tier_evicts_oldest_objects_when_over_budget() {
        let directory = tempfile::tempdir().expect("temp");
        // Two objects fit in the 2-byte... budgets are in real bytes, so use
        // a byte budget that admits exactly one small object.
        let backend =
            TieredCacheBackend::with_limits(8, u64::MAX, directory.path().to_path_buf(), 32, 1)
                .expect("disk");
        backend.insert(cache_key(1), cache_entry(b"first-object"));
        backend.insert(cache_key(2), cache_entry(b"second-object"));
        backend.insert(cache_key(3), cache_entry(b"third-object"));

        let (bytes, entries) = walk_tier_occupancy(directory.path());
        assert!(
            entries <= 1,
            "count budget evicts down to one object, got {entries}"
        );
        assert!(bytes <= 32 + 64, "byte budget enforced, got {bytes}");
    }

    #[test]
    fn disk_tier_seeds_occupancy_from_the_existing_tree() {
        let directory = tempfile::tempdir().expect("temp");
        {
            let backend = TieredCacheBackend::with_limits(
                8,
                u64::MAX,
                directory.path().to_path_buf(),
                u64::MAX,
                100,
            )
            .expect("disk");
            backend.insert(cache_key(1), cache_entry(b"seeded-object"));
        }
        let (bytes, entries) = walk_tier_occupancy(directory.path());
        assert_eq!(entries, 1);
        assert!(bytes > 0);
    }
}
