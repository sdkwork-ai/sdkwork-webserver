//! Durable disk cache backend (nginx `proxy_cache_path` subset).
//!
//! Layout: `<root>/<aa>/<bb>/<key-hash>` where each file is a length-prefixed
//! bincode-like framing of the serialized `CachedResponse`. The memory
//! backend remains the L1 index; this module is the L2 spill component and
//! never talks to the proxy path directly.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::{
    backend::{CacheBackend, MemoryCacheBackend},
    entry::CachedResponse,
    key::CacheKey,
};

const MAGIC: &[u8; 4] = b"SWC1";

/// Tiered backend: memory LRU index plus optional on-disk object store.
pub(crate) struct TieredCacheBackend {
    memory: MemoryCacheBackend,
    disk_root: Option<PathBuf>,
    write_lock: Mutex<()>,
}

impl TieredCacheBackend {
    pub(crate) fn memory_only(maximum_entries: usize) -> Self {
        Self {
            memory: MemoryCacheBackend::new(maximum_entries),
            disk_root: None,
            write_lock: Mutex::new(()),
        }
    }

    pub(crate) fn with_disk(maximum_entries: usize, disk_root: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&disk_root)?;
        Ok(Self {
            memory: MemoryCacheBackend::new(maximum_entries),
            disk_root: Some(disk_root),
            write_lock: Mutex::new(()),
        })
    }

    fn object_path(root: &Path, key: &CacheKey) -> PathBuf {
        let hash = key.stable_hash_hex();
        let a = &hash[0..2];
        let b = &hash[2..4];
        root.join(a).join(b).join(&hash)
    }
}

impl CacheBackend for TieredCacheBackend {
    fn get(&self, key: &CacheKey) -> Option<CachedResponse> {
        if let Some(entry) = self.memory.get(key) {
            return Some(entry);
        }
        let root = self.disk_root.as_ref()?;
        let path = Self::object_path(root, key);
        let entry = read_disk_entry(&path)?;
        if entry.expired() && !entry.stale_available() {
            let _ = fs::remove_file(&path);
            return None;
        }
        self.memory.insert(key.clone(), entry.clone());
        Some(entry)
    }

    fn insert(&self, key: CacheKey, entry: CachedResponse) {
        self.memory.insert(key.clone(), entry.clone());
        let Some(root) = self.disk_root.as_ref() else {
            return;
        };
        let path = Self::object_path(root, &key);
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = write_disk_entry(&path, &entry);
    }

    fn remove(&self, key: &CacheKey) {
        self.memory.remove(key);
        if let Some(root) = self.disk_root.as_ref() {
            let path = Self::object_path(root, key);
            let _ = fs::remove_file(path);
        }
    }
}

fn write_disk_entry(path: &Path, entry: &CachedResponse) -> std::io::Result<()> {
    let durable = entry.to_durable();
    let payload = serde_json::to_vec(&durable).map_err(std::io::Error::other)?;
    let mut file = fs::File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&(payload.len() as u32).to_le_bytes())?;
    file.write_all(&payload)?;
    file.sync_all()
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

    #[test]
    fn tiered_disk_round_trip() {
        let directory = tempfile::tempdir().expect("temp");
        let backend =
            TieredCacheBackend::with_disk(8, directory.path().to_path_buf()).expect("disk");
        let key = CacheKey::new("GET", "example.com", "/cached", None, &[], &HeaderMap::new());
        let entry = CachedResponse::new(
            ResponseMetadata {
                status: 200,
                headers: vec![("content-type".to_owned(), "text/plain".to_owned())],
                vary: Vec::new(),
                fresh_seconds: 60,
            },
            Bytes::from_static(b"disk-body"),
            Duration::from_secs(60),
            Duration::from_secs(60),
        );
        backend.insert(key.clone(), entry);
        // Drop memory by constructing a fresh backend on the same root.
        let cold =
            TieredCacheBackend::with_disk(8, directory.path().to_path_buf()).expect("cold");
        let hit = cold.get(&key).expect("disk hit");
        assert_eq!(hit.body.as_ref(), b"disk-body");
    }
}
