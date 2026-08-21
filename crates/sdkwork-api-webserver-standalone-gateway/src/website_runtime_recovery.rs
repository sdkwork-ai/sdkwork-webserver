use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use sdkwork_webserver_core::website_runtime::{
    compile_website_runtime_set_snapshot, CompiledWebsiteRuntimeSet, WebsiteRuntimeEnvironment,
    WebsiteRuntimeSetError, MAX_WEBSITE_RUNTIME_SET_BYTES,
};
use thiserror::Error;
use tokio::{fs as tokio_fs, io::AsyncWriteExt, sync::Semaphore};

const SLOT_A_FILE: &str = "website-runtime-set.a.json";
const SLOT_B_FILE: &str = "website-runtime-set.b.json";

#[derive(Debug, Error)]
pub(crate) enum WebsiteRuntimeSetRecoveryError {
    #[error("website runtime-set recovery directory is invalid")]
    InvalidDirectory,
    #[error("website runtime-set recovery state contains conflicting snapshots")]
    Conflict,
    #[error("website runtime-set recovery candidate is stale")]
    Stale,
    #[error("website runtime-set recovery candidate belongs to another node or environment")]
    ScopeMismatch,
    #[error("website runtime-set recovery I/O failed")]
    Io,
    #[error(transparent)]
    RuntimeSet(#[from] WebsiteRuntimeSetError),
}

pub(crate) struct LoadedWebsiteRuntimeSet {
    bytes: Vec<u8>,
    runtime_set: Arc<CompiledWebsiteRuntimeSet>,
}

impl LoadedWebsiteRuntimeSet {
    pub(crate) fn compile(bytes: Vec<u8>) -> Result<Self, WebsiteRuntimeSetError> {
        let runtime_set = Arc::new(compile_website_runtime_set_snapshot(&bytes)?);
        Ok(Self { bytes, runtime_set })
    }

    pub(crate) fn runtime_set(&self) -> &Arc<CompiledWebsiteRuntimeSet> {
        &self.runtime_set
    }
}

pub(crate) struct WebsiteRuntimeSetRecoveryOpen {
    pub(crate) store: Arc<WebsiteRuntimeSetRecoveryStore>,
    pub(crate) recovered: Option<LoadedWebsiteRuntimeSet>,
    pub(crate) corruption_detected: bool,
}

pub(crate) struct WebsiteRuntimeSetRecoveryStore {
    directory: PathBuf,
    persistence: Semaphore,
    state: Mutex<RecoveryState>,
}

struct RecoveryState {
    active_slot: Option<RecoverySlot>,
    generation: Option<u64>,
    snapshot_sha256: Option<String>,
    node_uuid: Option<String>,
    environment: Option<WebsiteRuntimeEnvironment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoverySlot {
    A,
    B,
}

impl RecoverySlot {
    fn filename(self) -> &'static str {
        match self {
            Self::A => SLOT_A_FILE,
            Self::B => SLOT_B_FILE,
        }
    }

    fn inactive_after(active: Option<Self>) -> Self {
        match active {
            Some(Self::A) => Self::B,
            Some(Self::B) | None => Self::A,
        }
    }
}

struct RecoveryCandidate {
    slot: RecoverySlot,
    loaded: LoadedWebsiteRuntimeSet,
}

impl WebsiteRuntimeSetRecoveryStore {
    pub(crate) fn open(
        directory: impl Into<PathBuf>,
    ) -> Result<WebsiteRuntimeSetRecoveryOpen, WebsiteRuntimeSetRecoveryError> {
        let directory = directory.into();
        prepare_directory(&directory)?;
        let mut candidates = Vec::new();
        let mut corruption_detected = false;
        for entry in fs::read_dir(&directory).map_err(|_| WebsiteRuntimeSetRecoveryError::Io)? {
            let entry = entry.map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| WebsiteRuntimeSetRecoveryError::InvalidDirectory)?;
            let slot = match name.as_str() {
                SLOT_A_FILE => RecoverySlot::A,
                SLOT_B_FILE => RecoverySlot::B,
                _ => return Err(WebsiteRuntimeSetRecoveryError::InvalidDirectory),
            };
            match read_candidate(&entry.path(), slot) {
                Ok(candidate) => candidates.push(candidate),
                Err(_) => corruption_detected = true,
            }
        }
        let selected = select_recovery_candidate(candidates)?;
        let state = match selected.as_ref() {
            Some(candidate) => RecoveryState {
                active_slot: Some(candidate.slot),
                generation: Some(candidate.loaded.runtime_set.generation()),
                snapshot_sha256: Some(candidate.loaded.runtime_set.snapshot_sha256().to_owned()),
                node_uuid: Some(candidate.loaded.runtime_set.node_uuid().to_owned()),
                environment: Some(candidate.loaded.runtime_set.environment()),
            },
            None => RecoveryState {
                active_slot: None,
                generation: None,
                snapshot_sha256: None,
                node_uuid: None,
                environment: None,
            },
        };
        let recovered = selected.map(|candidate| candidate.loaded);
        Ok(WebsiteRuntimeSetRecoveryOpen {
            store: Arc::new(Self {
                directory,
                persistence: Semaphore::new(1),
                state: Mutex::new(state),
            }),
            recovered,
            corruption_detected,
        })
    }

    pub(crate) async fn persist(
        &self,
        candidate: &LoadedWebsiteRuntimeSet,
    ) -> Result<bool, WebsiteRuntimeSetRecoveryError> {
        if candidate.bytes.is_empty() || candidate.bytes.len() > MAX_WEBSITE_RUNTIME_SET_BYTES {
            return Err(WebsiteRuntimeSetRecoveryError::Io);
        }
        let _persistence = self
            .persistence
            .acquire()
            .await
            .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
        let target_slot = {
            let state = self
                .state
                .lock()
                .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
            if let (Some(node_uuid), Some(environment)) =
                (state.node_uuid.as_deref(), state.environment)
            {
                if node_uuid != candidate.runtime_set.node_uuid()
                    || environment != candidate.runtime_set.environment()
                {
                    return Err(WebsiteRuntimeSetRecoveryError::ScopeMismatch);
                }
            }
            if let Some(generation) = state.generation {
                if candidate.runtime_set.generation() < generation {
                    return Err(WebsiteRuntimeSetRecoveryError::Stale);
                }
                if candidate.runtime_set.generation() == generation {
                    if state.snapshot_sha256.as_deref()
                        == Some(candidate.runtime_set.snapshot_sha256())
                    {
                        return Ok(false);
                    }
                    return Err(WebsiteRuntimeSetRecoveryError::Conflict);
                }
            }
            RecoverySlot::inactive_after(state.active_slot)
        };

        validate_directory_async(&self.directory).await?;
        let target = self.directory.join(target_slot.filename());
        validate_slot_target(&target).await?;
        // Atomic commit: write the temporary sibling, fsync it, rename over
        // the target, then fsync the directory so a crash never leaves a
        // truncated or half-written recovery slot.
        let temp = self
            .directory
            .join(format!("{}.tmp", target_slot.filename()));
        validate_slot_target(&temp).await?;
        {
            let mut file = tokio_fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temp)
                .await
                .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
            file.write_all(&candidate.bytes)
                .await
                .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
            file.sync_all()
                .await
                .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
        }
        tokio_fs::rename(&temp, &target)
            .await
            .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
        sync_directory(&self.directory).await?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
        state.active_slot = Some(target_slot);
        state.generation = Some(candidate.runtime_set.generation());
        state.snapshot_sha256 = Some(candidate.runtime_set.snapshot_sha256().to_owned());
        state.node_uuid = Some(candidate.runtime_set.node_uuid().to_owned());
        state.environment = Some(candidate.runtime_set.environment());
        Ok(true)
    }
}

fn prepare_directory(directory: &Path) -> Result<(), WebsiteRuntimeSetRecoveryError> {
    if directory.as_os_str().is_empty() {
        return Err(WebsiteRuntimeSetRecoveryError::InvalidDirectory);
    }
    if directory.exists() {
        let metadata =
            fs::symlink_metadata(directory).map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(WebsiteRuntimeSetRecoveryError::InvalidDirectory);
        }
    } else {
        fs::create_dir_all(directory).map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
    }
    Ok(())
}

fn read_candidate(
    path: &Path,
    slot: RecoverySlot,
) -> Result<RecoveryCandidate, WebsiteRuntimeSetRecoveryError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > u64::try_from(MAX_WEBSITE_RUNTIME_SET_BYTES).unwrap_or(u64::MAX)
    {
        return Err(WebsiteRuntimeSetRecoveryError::InvalidDirectory);
    }
    let bytes = fs::read(path).map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
    Ok(RecoveryCandidate {
        slot,
        loaded: LoadedWebsiteRuntimeSet::compile(bytes)?,
    })
}

fn select_recovery_candidate(
    mut candidates: Vec<RecoveryCandidate>,
) -> Result<Option<RecoveryCandidate>, WebsiteRuntimeSetRecoveryError> {
    candidates.sort_unstable_by_key(|candidate| candidate.loaded.runtime_set.generation());
    let Some(latest) = candidates.pop() else {
        return Ok(None);
    };
    if let Some(previous) = candidates.last() {
        if previous.loaded.runtime_set.node_uuid() != latest.loaded.runtime_set.node_uuid()
            || previous.loaded.runtime_set.environment() != latest.loaded.runtime_set.environment()
        {
            return Err(WebsiteRuntimeSetRecoveryError::ScopeMismatch);
        }
        if previous.loaded.runtime_set.generation() == latest.loaded.runtime_set.generation()
            && previous.loaded.runtime_set.snapshot_sha256()
                != latest.loaded.runtime_set.snapshot_sha256()
        {
            return Err(WebsiteRuntimeSetRecoveryError::Conflict);
        }
    }
    Ok(Some(latest))
}

async fn validate_directory_async(directory: &Path) -> Result<(), WebsiteRuntimeSetRecoveryError> {
    let metadata = tokio_fs::symlink_metadata(directory)
        .await
        .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(WebsiteRuntimeSetRecoveryError::InvalidDirectory);
    }
    Ok(())
}

async fn validate_slot_target(path: &Path) -> Result<(), WebsiteRuntimeSetRecoveryError> {
    match tokio_fs::symlink_metadata(path).await {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            Err(WebsiteRuntimeSetRecoveryError::InvalidDirectory)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(WebsiteRuntimeSetRecoveryError::Io),
    }
}

#[cfg(unix)]
async fn sync_directory(directory: &Path) -> Result<(), WebsiteRuntimeSetRecoveryError> {
    tokio_fs::File::open(directory)
        .await
        .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)?
        .sync_all()
        .await
        .map_err(|_| WebsiteRuntimeSetRecoveryError::Io)
}

#[cfg(not(unix))]
async fn sync_directory(_directory: &Path) -> Result<(), WebsiteRuntimeSetRecoveryError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use sdkwork_webserver_core::website_runtime::{
        website_runtime_set_snapshot_sha256, WebsiteRuntimeSetSnapshot,
    };
    use serde_json::{json, Value};

    use super::*;

    #[tokio::test]
    async fn dual_slot_store_recovers_the_previous_valid_generation() {
        let root = tempfile::tempdir().unwrap();
        let opened = WebsiteRuntimeSetRecoveryStore::open(root.path()).unwrap();
        let first = fixture(1, "node-1", WebsiteRuntimeEnvironment::Production, "first");
        let second = fixture(2, "node-1", WebsiteRuntimeEnvironment::Production, "second");
        assert!(opened.store.persist(&first).await.unwrap());
        assert!(opened.store.persist(&second).await.unwrap());
        std::fs::write(root.path().join(SLOT_B_FILE), b"corrupt").unwrap();

        let reopened = WebsiteRuntimeSetRecoveryStore::open(root.path()).unwrap();
        assert!(reopened.corruption_detected);
        assert_eq!(reopened.recovered.unwrap().runtime_set.generation(), 1);
    }

    #[tokio::test]
    async fn store_rejects_stale_conflicting_and_cross_scope_candidates() {
        let root = tempfile::tempdir().unwrap();
        let opened = WebsiteRuntimeSetRecoveryStore::open(root.path()).unwrap();
        let current = fixture(
            2,
            "node-1",
            WebsiteRuntimeEnvironment::Production,
            "current",
        );
        opened.store.persist(&current).await.unwrap();
        assert!(matches!(
            opened
                .store
                .persist(&fixture(
                    1,
                    "node-1",
                    WebsiteRuntimeEnvironment::Production,
                    "stale"
                ))
                .await,
            Err(WebsiteRuntimeSetRecoveryError::Stale)
        ));
        assert!(matches!(
            opened
                .store
                .persist(&fixture(
                    2,
                    "node-1",
                    WebsiteRuntimeEnvironment::Production,
                    "conflict"
                ))
                .await,
            Err(WebsiteRuntimeSetRecoveryError::Conflict)
        ));
        assert!(matches!(
            opened
                .store
                .persist(&fixture(
                    3,
                    "node-2",
                    WebsiteRuntimeEnvironment::Production,
                    "wrong-node"
                ))
                .await,
            Err(WebsiteRuntimeSetRecoveryError::ScopeMismatch)
        ));
    }

    #[test]
    fn open_rejects_same_generation_hash_conflicts() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join(SLOT_A_FILE),
            fixture(7, "node-1", WebsiteRuntimeEnvironment::Production, "a").bytes,
        )
        .unwrap();
        std::fs::write(
            root.path().join(SLOT_B_FILE),
            fixture(7, "node-1", WebsiteRuntimeEnvironment::Production, "b").bytes,
        )
        .unwrap();
        assert!(matches!(
            WebsiteRuntimeSetRecoveryStore::open(root.path()),
            Err(WebsiteRuntimeSetRecoveryError::Conflict)
        ));
    }

    fn fixture(
        generation: u64,
        node_uuid: &str,
        environment: WebsiteRuntimeEnvironment,
        identity: &str,
    ) -> LoadedWebsiteRuntimeSet {
        let environment_value = match environment {
            WebsiteRuntimeEnvironment::Development => "development",
            WebsiteRuntimeEnvironment::Test => "test",
            WebsiteRuntimeEnvironment::Staging => "staging",
            WebsiteRuntimeEnvironment::Production => "production",
        };
        let mut value = json!({
            "schemaVersion": "sdkwork.website-runtime-set.v1",
            "kind": "sdkwork.website-runtime-set.snapshot",
            "snapshotUuid": format!("snapshot-{generation}-{identity}"),
            "nodeUuid": node_uuid,
            "environment": environment_value,
            "generation": generation,
            "generatedAt": "2026-07-22T00:00:00Z",
            "compilerVersion": "deploy-runtime-set-compiler/1",
            "snapshotSha256": "0".repeat(64),
            "maximumSites": 8,
            "descriptors": []
        });
        let snapshot: WebsiteRuntimeSetSnapshot = serde_json::from_value(value.clone()).unwrap();
        value["snapshotSha256"] =
            Value::String(website_runtime_set_snapshot_sha256(&snapshot).unwrap());
        LoadedWebsiteRuntimeSet::compile(serde_json::to_vec(&value).unwrap()).unwrap()
    }
}
