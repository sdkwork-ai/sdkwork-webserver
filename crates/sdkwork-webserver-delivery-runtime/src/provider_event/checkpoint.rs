use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use async_trait::async_trait;
use sdkwork_utils_rust::sha256_hash;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs as tokio_fs, io::AsyncWriteExt, sync::Semaphore};

use super::{provider_event_stream_shard, PROVIDER_EVENT_STREAM_SHARDS};

const CHECKPOINT_SCHEMA_VERSION: &str = "sdkwork.website-provider-event-checkpoint.v1";
const MAXIMUM_CHECKPOINT_BYTES: u64 = 256 * 1024;
/// Suffix of the atomic-commit staging file (`<digest>.<slot>.tmp`); see
/// [`write_checkpoint`] and the residue tolerance in the directory loader.
const TMP_FILE_SUFFIX: &str = "tmp";
const MAXIMUM_CHECKPOINT_STREAMS: usize = 65_536;
pub(super) const MAXIMUM_RECENT_EVENTS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecentProviderEvent {
    id: String,
    sequence_no: u64,
    payload_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebsiteProviderEventCheckpoint {
    stream_id: String,
    last_sequence_no: u64,
    last_event_id: String,
    last_payload_sha256: String,
    uncertain: bool,
    recent_events: Vec<RecentProviderEvent>,
}

impl WebsiteProviderEventCheckpoint {
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    pub fn last_sequence_no(&self) -> u64 {
        self.last_sequence_no
    }

    pub fn last_event_id(&self) -> &str {
        &self.last_event_id
    }

    pub fn is_uncertain(&self) -> bool {
        self.uncertain
    }

    pub(super) fn uncertain(stream_id: String) -> Self {
        Self {
            stream_id,
            last_sequence_no: 0,
            last_event_id: String::new(),
            last_payload_sha256: String::new(),
            uncertain: true,
            recent_events: Vec::new(),
        }
    }

    pub(super) fn set_uncertain(&mut self, uncertain: bool) {
        self.uncertain = uncertain;
    }

    pub(super) fn recent_event(&self, id: &str) -> Option<(u64, &str)> {
        self.recent_events
            .iter()
            .find(|event| event.id == id)
            .map(|event| (event.sequence_no, event.payload_sha256.as_str()))
    }

    pub(super) fn record(&mut self, id: String, sequence_no: u64, payload_sha256: String) {
        self.last_sequence_no = sequence_no;
        self.last_event_id.clone_from(&id);
        self.last_payload_sha256.clone_from(&payload_sha256);
        self.uncertain = false;
        self.recent_events.push(RecentProviderEvent {
            id,
            sequence_no,
            payload_sha256,
        });
        if self.recent_events.len() > MAXIMUM_RECENT_EVENTS {
            let remove = self.recent_events.len() - MAXIMUM_RECENT_EVENTS;
            self.recent_events.drain(..remove);
        }
    }
}

#[async_trait]
pub trait WebsiteProviderEventCheckpointStore: Send + Sync {
    async fn load(
        &self,
        stream_id: &str,
    ) -> Result<Option<WebsiteProviderEventCheckpoint>, WebsiteProviderEventCheckpointError>;

    async fn save(
        &self,
        checkpoint: &WebsiteProviderEventCheckpoint,
    ) -> Result<(), WebsiteProviderEventCheckpointError>;
}

#[derive(Debug, Error)]
pub enum WebsiteProviderEventCheckpointError {
    #[error("provider event checkpoint directory is invalid")]
    InvalidDirectory,
    #[error("provider event checkpoint stream bound is invalid or exhausted")]
    StreamLimit,
    #[error("provider event checkpoint is corrupt")]
    Corrupt,
    #[error("provider event checkpoint I/O failed")]
    Io,
}

#[derive(Clone)]
struct StoredCheckpoint {
    generation: u64,
    checkpoint: WebsiteProviderEventCheckpoint,
}

pub struct FileWebsiteProviderEventCheckpointStore {
    directory: PathBuf,
    maximum_streams: usize,
    stream_count: AtomicUsize,
    persistence: [Semaphore; PROVIDER_EVENT_STREAM_SHARDS],
    state: [Mutex<BTreeMap<String, StoredCheckpoint>>; PROVIDER_EVENT_STREAM_SHARDS],
}

impl FileWebsiteProviderEventCheckpointStore {
    pub fn open(
        directory: impl Into<PathBuf>,
        maximum_streams: usize,
    ) -> Result<Self, WebsiteProviderEventCheckpointError> {
        if maximum_streams == 0 || maximum_streams > MAXIMUM_CHECKPOINT_STREAMS {
            return Err(WebsiteProviderEventCheckpointError::StreamLimit);
        }
        let directory = directory.into();
        prepare_directory(&directory)?;
        let checkpoints = load_checkpoint_directory(&directory, maximum_streams)?;
        let stream_count = checkpoints.len();
        let mut state = std::array::from_fn(|_| Mutex::new(BTreeMap::new()));
        for (stream_id, checkpoint) in checkpoints {
            let shard = provider_event_stream_shard(&stream_id);
            state[shard]
                .get_mut()
                .map_err(|_| WebsiteProviderEventCheckpointError::Corrupt)?
                .insert(stream_id, checkpoint);
        }
        Ok(Self {
            directory,
            maximum_streams,
            stream_count: AtomicUsize::new(stream_count),
            persistence: std::array::from_fn(|_| Semaphore::new(1)),
            state,
        })
    }
}

#[async_trait]
impl WebsiteProviderEventCheckpointStore for FileWebsiteProviderEventCheckpointStore {
    async fn load(
        &self,
        stream_id: &str,
    ) -> Result<Option<WebsiteProviderEventCheckpoint>, WebsiteProviderEventCheckpointError> {
        let shard = provider_event_stream_shard(stream_id);
        let state = self.state[shard]
            .lock()
            .map_err(|_| WebsiteProviderEventCheckpointError::Corrupt)?;
        Ok(state.get(stream_id).map(|stored| stored.checkpoint.clone()))
    }

    async fn save(
        &self,
        checkpoint: &WebsiteProviderEventCheckpoint,
    ) -> Result<(), WebsiteProviderEventCheckpointError> {
        let shard = provider_event_stream_shard(checkpoint.stream_id());
        let _persistence = self.persistence[shard]
            .acquire()
            .await
            .map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
        let (generation, reservation) = {
            let state = self.state[shard]
                .lock()
                .map_err(|_| WebsiteProviderEventCheckpointError::Corrupt)?;
            let is_new = !state.contains_key(checkpoint.stream_id());
            let reservation =
                StreamCountReservation::reserve(&self.stream_count, self.maximum_streams, is_new)?;
            let generation = state
                .get(checkpoint.stream_id())
                .map_or(1, |stored| stored.generation.saturating_add(1));
            if generation == u64::MAX {
                return Err(WebsiteProviderEventCheckpointError::Corrupt);
            }
            (generation, reservation)
        };
        write_checkpoint(&self.directory, generation, checkpoint).await?;
        let mut state = self.state[shard]
            .lock()
            .map_err(|_| WebsiteProviderEventCheckpointError::Corrupt)?;
        state.insert(
            checkpoint.stream_id().to_owned(),
            StoredCheckpoint {
                generation,
                checkpoint: checkpoint.clone(),
            },
        );
        if let Some(reservation) = reservation {
            reservation.commit();
        }
        Ok(())
    }
}

struct StreamCountReservation<'a> {
    stream_count: &'a AtomicUsize,
    active: bool,
}

impl<'a> StreamCountReservation<'a> {
    fn reserve(
        stream_count: &'a AtomicUsize,
        maximum_streams: usize,
        required: bool,
    ) -> Result<Option<Self>, WebsiteProviderEventCheckpointError> {
        if !required {
            return Ok(None);
        }
        stream_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < maximum_streams).then_some(count + 1)
            })
            .map_err(|_| WebsiteProviderEventCheckpointError::StreamLimit)?;
        Ok(Some(Self {
            stream_count,
            active: true,
        }))
    }

    fn commit(mut self) {
        self.active = false;
    }
}

impl Drop for StreamCountReservation<'_> {
    fn drop(&mut self) {
        if self.active {
            self.stream_count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckpointDocument {
    schema_version: String,
    stream_id: String,
    generation: u64,
    checkpoint: WebsiteProviderEventCheckpoint,
    snapshot_sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointHashMaterial<'a> {
    schema_version: &'static str,
    stream_id: &'a str,
    generation: u64,
    checkpoint: &'a WebsiteProviderEventCheckpoint,
}

fn prepare_directory(directory: &Path) -> Result<(), WebsiteProviderEventCheckpointError> {
    if directory.as_os_str().is_empty() {
        return Err(WebsiteProviderEventCheckpointError::InvalidDirectory);
    }
    if directory.exists() {
        let metadata =
            fs::symlink_metadata(directory).map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(WebsiteProviderEventCheckpointError::InvalidDirectory);
        }
    } else {
        fs::create_dir_all(directory).map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
    }
    Ok(())
}

fn load_checkpoint_directory(
    directory: &Path,
    maximum_streams: usize,
) -> Result<BTreeMap<String, StoredCheckpoint>, WebsiteProviderEventCheckpointError> {
    let mut candidates: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let maximum_files = maximum_streams.saturating_mul(2);
    // Aggregate byte budget for everything the loader may retain: two slots
    // per stream at the per-document ceiling is the physical upper bound, and
    // the budget keeps startup memory bounded even when the directory has
    // been filled with maximum-size documents.
    let aggregate_budget = (maximum_files as u64).saturating_mul(MAXIMUM_CHECKPOINT_BYTES);
    let mut total_bytes: u64 = 0;
    for entry in fs::read_dir(directory).map_err(|_| WebsiteProviderEventCheckpointError::Io)? {
        let entry = entry.map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| WebsiteProviderEventCheckpointError::InvalidDirectory)?;
        if name.ends_with(TMP_FILE_SUFFIX) {
            // Atomic-commit residue from a write that crashed or was
            // cancelled between staging and rename. It never carries state,
            // so remove it best-effort and keep loading; treating it as a
            // hard error would permanently brick the store on startup.
            let _ = fs::remove_file(entry.path());
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(WebsiteProviderEventCheckpointError::InvalidDirectory);
        }
        if metadata.len() > MAXIMUM_CHECKPOINT_BYTES {
            return Err(WebsiteProviderEventCheckpointError::Corrupt);
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        if total_bytes > aggregate_budget {
            return Err(WebsiteProviderEventCheckpointError::StreamLimit);
        }
        let digest = checkpoint_file_digest(&name)
            .ok_or(WebsiteProviderEventCheckpointError::InvalidDirectory)?;
        candidates
            .entry(digest.to_owned())
            .or_default()
            .push(entry.path());
        if candidates.values().map(Vec::len).sum::<usize>() > maximum_files {
            return Err(WebsiteProviderEventCheckpointError::StreamLimit);
        }
    }
    if candidates.len() > maximum_streams {
        return Err(WebsiteProviderEventCheckpointError::StreamLimit);
    }

    let mut checkpoints: BTreeMap<String, StoredCheckpoint> = BTreeMap::new();
    for (digest, paths) in candidates {
        // Keep only the highest-generation document per stream: the other
        // slot is parsed for validation but never retained, so memory stays
        // O(streams) instead of O(streams x slots) during recovery.
        let mut best: Option<CheckpointDocument> = None;
        for path in paths {
            if let Ok(document) = read_checkpoint(&path, &digest) {
                if best.as_ref().is_none_or(|current: &CheckpointDocument| {
                    document.generation > current.generation
                }) {
                    best = Some(document);
                }
            }
        }
        let document = best.ok_or(WebsiteProviderEventCheckpointError::Corrupt)?;
        match checkpoints.get(&document.stream_id) {
            Some(existing) if existing.generation >= document.generation => {}
            _ => {
                checkpoints.insert(
                    document.stream_id,
                    StoredCheckpoint {
                        generation: document.generation,
                        checkpoint: document.checkpoint,
                    },
                );
            }
        }
    }
    Ok(checkpoints)
}

fn checkpoint_file_digest(name: &str) -> Option<&str> {
    let digest = name
        .strip_suffix(".a.json")
        .or_else(|| name.strip_suffix(".b.json"))?;
    if digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Some(digest)
    } else {
        None
    }
}

fn read_checkpoint(
    path: &Path,
    expected_digest: &str,
) -> Result<CheckpointDocument, WebsiteProviderEventCheckpointError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAXIMUM_CHECKPOINT_BYTES
    {
        return Err(WebsiteProviderEventCheckpointError::Corrupt);
    }
    let bytes = fs::read(path).map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
    let document: CheckpointDocument =
        serde_json::from_slice(&bytes).map_err(|_| WebsiteProviderEventCheckpointError::Corrupt)?;
    if document.schema_version != CHECKPOINT_SCHEMA_VERSION
        || document.stream_id != document.checkpoint.stream_id
        || sha256_hash(document.stream_id.as_bytes()) != expected_digest
        || document.snapshot_sha256 != document_hash(&document)?
    {
        return Err(WebsiteProviderEventCheckpointError::Corrupt);
    }
    Ok(document)
}

async fn write_checkpoint(
    directory: &Path,
    generation: u64,
    checkpoint: &WebsiteProviderEventCheckpoint,
) -> Result<(), WebsiteProviderEventCheckpointError> {
    let digest = sha256_hash(checkpoint.stream_id().as_bytes());
    let slot = if generation.is_multiple_of(2) {
        "a"
    } else {
        "b"
    };
    let path = directory.join(format!("{digest}.{slot}.json"));
    match tokio_fs::symlink_metadata(&path).await {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(WebsiteProviderEventCheckpointError::InvalidDirectory);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(WebsiteProviderEventCheckpointError::Io),
    }
    let mut document = CheckpointDocument {
        schema_version: CHECKPOINT_SCHEMA_VERSION.to_owned(),
        stream_id: checkpoint.stream_id().to_owned(),
        generation,
        checkpoint: checkpoint.clone(),
        snapshot_sha256: String::new(),
    };
    document.snapshot_sha256 = document_hash(&document)?;
    let bytes =
        serde_json::to_vec(&document).map_err(|_| WebsiteProviderEventCheckpointError::Corrupt)?;
    if bytes.is_empty() || bytes.len() as u64 > MAXIMUM_CHECKPOINT_BYTES {
        return Err(WebsiteProviderEventCheckpointError::Corrupt);
    }
    // Atomic commit via temporary sibling + rename: a crash never leaves a
    // truncated checkpoint. The loader removes `*.tmp` residue instead of
    // failing, so a cancelled write can never brick the store on startup.
    let temp_path = path.with_extension(TMP_FILE_SUFFIX);
    // A planted symlink at the temp path must not be followed on open.
    if let Ok(metadata) = tokio_fs::symlink_metadata(&temp_path).await {
        if metadata.file_type().is_symlink() {
            return Err(WebsiteProviderEventCheckpointError::InvalidDirectory);
        }
    }
    let write = async {
        let mut file = tokio_fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp_path)
            .await
            .map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
        file.write_all(&bytes)
            .await
            .map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
        file.sync_all()
            .await
            .map_err(|_| WebsiteProviderEventCheckpointError::Io)?;
        tokio_fs::rename(&temp_path, &path)
            .await
            .map_err(|_| WebsiteProviderEventCheckpointError::Io)
    };
    if let Err(error) = write.await {
        // Best-effort residue cleanup on every failure path so the next
        // start (or the next write to this slot) begins from a clean slate.
        let _ = tokio_fs::remove_file(&temp_path).await;
        return Err(error);
    }
    sync_directory(directory).await?;
    Ok(())
}

fn document_hash(
    document: &CheckpointDocument,
) -> Result<String, WebsiteProviderEventCheckpointError> {
    let material = CheckpointHashMaterial {
        schema_version: CHECKPOINT_SCHEMA_VERSION,
        stream_id: &document.stream_id,
        generation: document.generation,
        checkpoint: &document.checkpoint,
    };
    serde_json::to_vec(&material)
        .map(|bytes| sha256_hash(&bytes))
        .map_err(|_| WebsiteProviderEventCheckpointError::Corrupt)
}

#[cfg(unix)]
async fn sync_directory(directory: &Path) -> Result<(), WebsiteProviderEventCheckpointError> {
    tokio_fs::File::open(directory)
        .await
        .map_err(|_| WebsiteProviderEventCheckpointError::Io)?
        .sync_all()
        .await
        .map_err(|_| WebsiteProviderEventCheckpointError::Io)
}

#[cfg(not(unix))]
async fn sync_directory(_directory: &Path) -> Result<(), WebsiteProviderEventCheckpointError> {
    Ok(())
}
