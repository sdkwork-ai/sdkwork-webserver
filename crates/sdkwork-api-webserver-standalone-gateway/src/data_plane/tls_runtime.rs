use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use axum_server::tls_rustls::RustlsConfig;
use sdkwork_webserver_core::{
    tls_runtime::{
        compile_tls_assignment_snapshot, TlsRuntimeSnapshotError, TlsRuntimeVersion,
        MAX_TLS_RUNTIME_SNAPSHOT_BYTES,
    },
    ListenerConfig, ListenerProtocol, TlsVersion,
};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::{sync::watch, time::MissedTickBehavior};

use super::tls_material::{build_sni_server_config, install_crypto_provider, load_certified_key};

const CERTIFICATE_FILE_NAME: &str = "fullchain.pem";
const PRIVATE_KEY_FILE_NAME: &str = "privkey.pem";
const RECOVERY_SLOT_A_FILE: &str = "tls-runtime.a.json";
const RECOVERY_SLOT_B_FILE: &str = "tls-runtime.b.json";

#[derive(Clone, Debug)]
pub struct FileTlsRuntimeConfig {
    pub snapshot_file: PathBuf,
    pub material_root: PathBuf,
    pub listener_id: String,
    pub node_uuid: String,
    pub poll_interval: Duration,
    pub recovery_directory: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum FileTlsRuntimeError {
    #[error("TLS runtime configuration is invalid: {0}")]
    Config(String),
    #[error("TLS runtime snapshot source is unavailable or not a stable bounded regular file")]
    SnapshotSource,
    #[error(transparent)]
    Snapshot(#[from] TlsRuntimeSnapshotError),
    #[error("TLS runtime snapshot targets node {actual}, expected {expected}")]
    NodeScope { expected: String, actual: String },
    #[error("TLS runtime snapshot contains no certificate assignments")]
    EmptyAssignments,
    #[error("TLS runtime assignments do not share one listener-wide policy")]
    MixedPolicy,
    #[error("TLS material reference {reference} is not an authorized file reference")]
    MaterialReference { reference: String },
    #[error("TLS material directory for reference {reference} escapes the protected root")]
    MaterialBoundary { reference: String },
    #[error("TLS material for assignment {assignment_uuid} is invalid: {source}")]
    Material {
        assignment_uuid: String,
        #[source]
        source: super::DataPlaneError,
    },
    #[error(
        "TLS certificate fingerprint for assignment {assignment_uuid} does not match the snapshot"
    )]
    Fingerprint { assignment_uuid: String },
    #[error(
        "TLS certificate validity for assignment {assignment_uuid} does not match the snapshot"
    )]
    Validity { assignment_uuid: String },
    #[error("TLS server name {server_name} is assigned more than once")]
    AmbiguousServerName { server_name: String },
    #[error("TLS runtime watcher failed: {0}")]
    Watcher(#[source] tokio::task::JoinError),
    #[error("TLS runtime recovery directory or slot state is invalid")]
    Recovery,
    #[error("TLS runtime recovery candidate conflicts with the active generation")]
    RecoveryConflict,
    #[error("TLS runtime recovery candidate is older than the active generation")]
    RecoveryStale,
    #[error("TLS runtime candidate generation is older than the active generation")]
    CandidateStale,
    #[error("TLS runtime candidate conflicts with the active generation")]
    CandidateConflict,
    #[error("TLS runtime ALPN policy is incompatible with listener {listener_id}")]
    ListenerProtocol { listener_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceFingerprint {
    length: u64,
    modified: Option<SystemTime>,
}

struct Candidate {
    server_config: Arc<rustls::ServerConfig>,
    snapshot_sha256: String,
    generation: u64,
    bytes: Vec<u8>,
    alpn: Vec<String>,
}

pub struct FileTlsRuntimeController {
    config: FileTlsRuntimeConfig,
    canonical_material_root: PathBuf,
    rustls: RustlsConfig,
    active_snapshot_sha256: Mutex<String>,
    active_generation: Mutex<u64>,
    recovery: Option<Mutex<TlsRecoveryStore>>,
    listener_protocols: Mutex<Option<(bool, bool)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoverySlot {
    A,
    B,
}

impl RecoverySlot {
    fn filename(self) -> &'static str {
        match self {
            Self::A => RECOVERY_SLOT_A_FILE,
            Self::B => RECOVERY_SLOT_B_FILE,
        }
    }

    fn inactive_after(active: Option<Self>) -> Self {
        match active {
            Some(Self::A) => Self::B,
            Some(Self::B) | None => Self::A,
        }
    }
}

struct TlsRecoveryStore {
    directory: PathBuf,
    active_slot: Option<RecoverySlot>,
    generation: Option<u64>,
    snapshot_sha256: Option<String>,
}

impl FileTlsRuntimeController {
    pub fn load(config: FileTlsRuntimeConfig) -> Result<Arc<Self>, FileTlsRuntimeError> {
        validate_config(&config)?;
        let canonical_material_root = fs::canonicalize(&config.material_root).map_err(|_| {
            FileTlsRuntimeError::Config("materialRoot must be an existing directory".to_owned())
        })?;
        if !fs::metadata(&canonical_material_root)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            return Err(FileTlsRuntimeError::Config(
                "materialRoot must be an existing directory".to_owned(),
            ));
        }
        install_crypto_provider().map_err(|source| FileTlsRuntimeError::Material {
            assignment_uuid: "bootstrap".to_owned(),
            source,
        })?;
        let (mut recovery, recovered) = match config.recovery_directory.as_ref() {
            Some(directory) => {
                let (store, recovered) =
                    TlsRecoveryStore::open(directory, &config, &canonical_material_root)?;
                (Some(store), recovered)
            }
            None => (None, None),
        };
        let source = load_candidate(&config, &canonical_material_root, None);
        let (candidate, source_selected) = select_initial_candidate(source, recovered)?;
        if source_selected {
            if let Some(store) = recovery.as_mut() {
                store.persist(&candidate)?;
            }
        } else {
            tracing::warn!(
                tls_runtime_snapshot_sha256 = %candidate.snapshot_sha256,
                "TLS runtime restored from node recovery state"
            );
        }
        let controller = Self {
            config,
            canonical_material_root,
            rustls: RustlsConfig::from_config(candidate.server_config),
            active_snapshot_sha256: Mutex::new(candidate.snapshot_sha256),
            active_generation: Mutex::new(candidate.generation),
            recovery: recovery.map(Mutex::new),
            listener_protocols: Mutex::new(None),
        };
        Ok(Arc::new(controller))
    }

    pub(crate) fn listener_id(&self) -> &str {
        &self.config.listener_id
    }

    pub(crate) fn rustls_config(&self) -> RustlsConfig {
        self.rustls.clone()
    }

    pub(crate) fn configure_listener(
        &self,
        listener: &ListenerConfig,
    ) -> Result<(), FileTlsRuntimeError> {
        let protocols = (
            listener.protocols.contains(&ListenerProtocol::Http1),
            listener.protocols.contains(&ListenerProtocol::Http2),
        );
        let active = self.rustls.get_inner();
        if !alpn_is_compatible(&active.alpn_protocols, protocols) {
            return Err(FileTlsRuntimeError::ListenerProtocol {
                listener_id: listener.id.clone(),
            });
        }
        let mut configured = lock_unpoisoned(&self.listener_protocols);
        match *configured {
            Some(existing) if existing != protocols => Err(FileTlsRuntimeError::ListenerProtocol {
                listener_id: listener.id.clone(),
            }),
            _ => {
                *configured = Some(protocols);
                Ok(())
            }
        }
    }

    pub(crate) async fn watch_until(
        self: Arc<Self>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), FileTlsRuntimeError> {
        let mut ticker = tokio::time::interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
                _ = ticker.tick() => {
                    let controller = Arc::clone(&self);
                    let active_snapshot_sha256 =
                        lock_unpoisoned(&self.active_snapshot_sha256).clone();
                    let candidate = tokio::task::spawn_blocking(move || {
                        let candidate = load_candidate(
                            &controller.config,
                            &controller.canonical_material_root,
                            Some(&active_snapshot_sha256),
                        )?;
                        if let Some(candidate) = candidate.as_ref() {
                            controller.validate_candidate_order(candidate)?;
                            controller.validate_candidate_listener(candidate)?;
                            controller.persist_recovery(candidate)?;
                        }
                        Ok::<_, FileTlsRuntimeError>(candidate)
                    })
                    .await
                    .map_err(FileTlsRuntimeError::Watcher)?;
                    match candidate {
                        Ok(Some(candidate)) => self.activate(candidate),
                        Ok(None) => {}
                        Err(error) => tracing::warn!(error = %error, "TLS runtime candidate rejected; retaining last-known-good configuration"),
                    }
                }
            }
        }
    }

    fn activate(&self, candidate: Candidate) {
        let mut active_snapshot_sha256 = lock_unpoisoned(&self.active_snapshot_sha256);
        if *active_snapshot_sha256 == candidate.snapshot_sha256 {
            return;
        }
        self.rustls.reload_from_config(candidate.server_config);
        *active_snapshot_sha256 = candidate.snapshot_sha256;
        *lock_unpoisoned(&self.active_generation) = candidate.generation;
        tracing::info!(
            tls_runtime_snapshot_sha256 = %*active_snapshot_sha256,
            tls_runtime_generation = candidate.generation,
            listener_id = %self.config.listener_id,
            "TLS runtime snapshot activated"
        );
    }

    fn persist_recovery(&self, candidate: &Candidate) -> Result<(), FileTlsRuntimeError> {
        if let Some(recovery) = self.recovery.as_ref() {
            lock_unpoisoned(recovery).persist(candidate)?;
        }
        Ok(())
    }

    fn validate_candidate_listener(
        &self,
        candidate: &Candidate,
    ) -> Result<(), FileTlsRuntimeError> {
        if let Some(protocols) = *lock_unpoisoned(&self.listener_protocols) {
            let alpn = candidate
                .alpn
                .iter()
                .map(|protocol| protocol.as_bytes().to_vec())
                .collect::<Vec<_>>();
            if !alpn_is_compatible(&alpn, protocols) {
                return Err(FileTlsRuntimeError::ListenerProtocol {
                    listener_id: self.config.listener_id.clone(),
                });
            }
        }
        Ok(())
    }

    fn validate_candidate_order(&self, candidate: &Candidate) -> Result<(), FileTlsRuntimeError> {
        let active_generation = lock_unpoisoned(&self.active_generation);
        match candidate.generation.cmp(&active_generation) {
            std::cmp::Ordering::Less => Err(FileTlsRuntimeError::CandidateStale),
            std::cmp::Ordering::Equal => Err(FileTlsRuntimeError::CandidateConflict),
            std::cmp::Ordering::Greater => Ok(()),
        }
    }
}

impl TlsRecoveryStore {
    fn open(
        directory: &Path,
        config: &FileTlsRuntimeConfig,
        canonical_material_root: &Path,
    ) -> Result<(Self, Option<Candidate>), FileTlsRuntimeError> {
        prepare_recovery_directory(directory)?;
        let mut candidates = Vec::new();
        for entry in fs::read_dir(directory).map_err(|_| FileTlsRuntimeError::Recovery)? {
            let entry = entry.map_err(|_| FileTlsRuntimeError::Recovery)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| FileTlsRuntimeError::Recovery)?;
            let slot = match name.as_str() {
                RECOVERY_SLOT_A_FILE => RecoverySlot::A,
                RECOVERY_SLOT_B_FILE => RecoverySlot::B,
                _ => return Err(FileTlsRuntimeError::Recovery),
            };
            if let Ok(bytes) = read_recovery_slot(&entry.path()) {
                if let Ok(Some(candidate)) =
                    compile_candidate(config, canonical_material_root, bytes, None)
                {
                    candidates.push((slot, candidate));
                }
            }
        }
        candidates.sort_unstable_by(|left, right| {
            left.1
                .generation
                .cmp(&right.1.generation)
                .then_with(|| left.1.snapshot_sha256.cmp(&right.1.snapshot_sha256))
        });
        if candidates.len() == 2
            && candidates[0].1.generation == candidates[1].1.generation
            && candidates[0].1.snapshot_sha256 != candidates[1].1.snapshot_sha256
        {
            return Err(FileTlsRuntimeError::RecoveryConflict);
        }
        let selected = candidates.pop();
        let (active_slot, generation, snapshot_sha256, recovered) = match selected {
            Some((slot, candidate)) => (
                Some(slot),
                Some(candidate.generation),
                Some(candidate.snapshot_sha256.clone()),
                Some(candidate),
            ),
            None => (None, None, None, None),
        };
        Ok((
            Self {
                directory: directory.to_path_buf(),
                active_slot,
                generation,
                snapshot_sha256,
            },
            recovered,
        ))
    }

    fn persist(&mut self, candidate: &Candidate) -> Result<bool, FileTlsRuntimeError> {
        if self.snapshot_sha256.as_deref() == Some(&candidate.snapshot_sha256) {
            return Ok(false);
        }
        if let Some(generation) = self.generation {
            if candidate.generation < generation {
                return Err(FileTlsRuntimeError::RecoveryStale);
            }
            if candidate.generation == generation {
                return Err(FileTlsRuntimeError::RecoveryConflict);
            }
        }
        prepare_recovery_directory(&self.directory)?;
        let slot = RecoverySlot::inactive_after(self.active_slot);
        let target = self.directory.join(slot.filename());
        validate_recovery_target(&target)?;
        // Atomic commit via temporary sibling + rename (see website
        // runtime recovery): a crash never leaves a truncated slot.
        let temp = self.directory.join(format!("{}.tmp", slot.filename()));
        validate_recovery_target(&temp)?;
        {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temp)
                .map_err(|_| FileTlsRuntimeError::Recovery)?;
            use std::io::Write;
            file.write_all(&candidate.bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| FileTlsRuntimeError::Recovery)?;
        }
        fs::rename(&temp, &target).map_err(|_| FileTlsRuntimeError::Recovery)?;
        sync_recovery_directory(&self.directory)?;
        self.active_slot = Some(slot);
        self.generation = Some(candidate.generation);
        self.snapshot_sha256 = Some(candidate.snapshot_sha256.clone());
        Ok(true)
    }
}

fn select_initial_candidate(
    source: Result<Option<Candidate>, FileTlsRuntimeError>,
    recovered: Option<Candidate>,
) -> Result<(Candidate, bool), FileTlsRuntimeError> {
    match (source, recovered) {
        (Ok(Some(source)), Some(recovered)) => match source.generation.cmp(&recovered.generation) {
            std::cmp::Ordering::Greater => Ok((source, true)),
            std::cmp::Ordering::Less => Ok((recovered, false)),
            std::cmp::Ordering::Equal if source.snapshot_sha256 == recovered.snapshot_sha256 => {
                Ok((source, true))
            }
            std::cmp::Ordering::Equal => Err(FileTlsRuntimeError::RecoveryConflict),
        },
        (Ok(Some(source)), None) => Ok((source, true)),
        (Err(_), Some(recovered)) => Ok((recovered, false)),
        (Err(error), None) => Err(error),
        (Ok(None), _) => unreachable!("initial TLS candidate comparison has no active hash"),
    }
}

fn validate_config(config: &FileTlsRuntimeConfig) -> Result<(), FileTlsRuntimeError> {
    if config.listener_id.is_empty()
        || config.node_uuid.is_empty()
        || config.poll_interval < Duration::from_millis(250)
        || config.poll_interval > Duration::from_secs(60)
    {
        return Err(FileTlsRuntimeError::Config(
            "listenerId and nodeUuid are required and pollInterval must be between 250ms and 60s"
                .to_owned(),
        ));
    }
    Ok(())
}

fn load_candidate(
    config: &FileTlsRuntimeConfig,
    canonical_material_root: &Path,
    unchanged_snapshot_sha256: Option<&str>,
) -> Result<Option<Candidate>, FileTlsRuntimeError> {
    let bytes = read_stable_snapshot(&config.snapshot_file)?;
    compile_candidate(
        config,
        canonical_material_root,
        bytes,
        unchanged_snapshot_sha256,
    )
}

fn compile_candidate(
    config: &FileTlsRuntimeConfig,
    canonical_material_root: &Path,
    bytes: Vec<u8>,
    unchanged_snapshot_sha256: Option<&str>,
) -> Result<Option<Candidate>, FileTlsRuntimeError> {
    let compiled = compile_tls_assignment_snapshot(&bytes)?;
    if unchanged_snapshot_sha256 == Some(compiled.snapshot_sha256()) {
        return Ok(None);
    }
    let snapshot = compiled.snapshot();
    if snapshot.node_uuid != config.node_uuid {
        return Err(FileTlsRuntimeError::NodeScope {
            expected: config.node_uuid.clone(),
            actual: snapshot.node_uuid.clone(),
        });
    }
    let policy = snapshot
        .assignments
        .first()
        .map(|assignment| assignment.policy.clone())
        .ok_or(FileTlsRuntimeError::EmptyAssignments)?;
    if snapshot
        .assignments
        .iter()
        .any(|assignment| assignment.policy != policy)
    {
        return Err(FileTlsRuntimeError::MixedPolicy);
    }
    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect("the Rustls crypto provider is installed before TLS material is loaded");
    let mut certificates = Vec::with_capacity(snapshot.assignments.len());
    for assignment in &snapshot.assignments {
        let (certificate_file, private_key_file) =
            resolve_material_paths(canonical_material_root, &assignment.material_reference)?;
        let loaded = load_certified_key(
            &certificate_file,
            &private_key_file,
            &assignment.server_names,
            provider,
        )
        .map_err(|source| FileTlsRuntimeError::Material {
            assignment_uuid: assignment.assignment_uuid.clone(),
            source,
        })?;
        if loaded.leaf_fingerprint_sha256 != assignment.expected_fingerprint_sha256 {
            return Err(FileTlsRuntimeError::Fingerprint {
                assignment_uuid: assignment.assignment_uuid.clone(),
            });
        }
        let not_before = parse_timestamp(&assignment.not_before)?;
        let not_after = parse_timestamp(&assignment.not_after)?;
        if loaded.not_before_unix_seconds != not_before
            || loaded.not_after_unix_seconds != not_after
        {
            return Err(FileTlsRuntimeError::Validity {
                assignment_uuid: assignment.assignment_uuid.clone(),
            });
        }
        certificates.push((assignment.server_names.clone(), loaded.certified_key));
    }
    let server_config = build_sni_server_config(
        certificates,
        map_version(policy.minimum_version),
        map_version(policy.maximum_version),
        &policy.alpn,
        None,
    )
    .map_err(|server_name| FileTlsRuntimeError::AmbiguousServerName { server_name })?;
    Ok(Some(Candidate {
        server_config,
        snapshot_sha256: compiled.snapshot_sha256().to_owned(),
        generation: snapshot.generation,
        bytes,
        alpn: policy.alpn,
    }))
}

fn alpn_is_compatible(alpn: &[Vec<u8>], protocols: (bool, bool)) -> bool {
    !alpn.is_empty()
        && alpn.iter().all(|protocol| match protocol.as_slice() {
            b"http/1.1" => protocols.0,
            b"h2" => protocols.1,
            _ => false,
        })
}

fn prepare_recovery_directory(directory: &Path) -> Result<(), FileTlsRuntimeError> {
    if directory.as_os_str().is_empty() {
        return Err(FileTlsRuntimeError::Recovery);
    }
    if directory.exists() {
        let metadata =
            fs::symlink_metadata(directory).map_err(|_| FileTlsRuntimeError::Recovery)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(FileTlsRuntimeError::Recovery);
        }
    } else {
        fs::create_dir_all(directory).map_err(|_| FileTlsRuntimeError::Recovery)?;
    }
    Ok(())
}

fn validate_recovery_target(path: &Path) -> Result<(), FileTlsRuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            Err(FileTlsRuntimeError::Recovery)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(FileTlsRuntimeError::Recovery),
    }
}

fn read_recovery_slot(path: &Path) -> Result<Vec<u8>, FileTlsRuntimeError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| FileTlsRuntimeError::Recovery)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_TLS_RUNTIME_SNAPSHOT_BYTES as u64
    {
        return Err(FileTlsRuntimeError::Recovery);
    }
    fs::read(path).map_err(|_| FileTlsRuntimeError::Recovery)
}

#[cfg(unix)]
fn sync_recovery_directory(directory: &Path) -> Result<(), FileTlsRuntimeError> {
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| FileTlsRuntimeError::Recovery)
}

#[cfg(not(unix))]
fn sync_recovery_directory(_directory: &Path) -> Result<(), FileTlsRuntimeError> {
    Ok(())
}

fn resolve_material_paths(
    canonical_material_root: &Path,
    material_reference: &str,
) -> Result<(PathBuf, PathBuf), FileTlsRuntimeError> {
    let id = material_reference
        .strip_prefix("file:")
        .filter(|id| {
            !id.is_empty()
                && *id != "."
                && *id != ".."
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
        .ok_or_else(|| FileTlsRuntimeError::MaterialReference {
            reference: material_reference.to_owned(),
        })?;
    let material_directory = fs::canonicalize(canonical_material_root.join(id)).map_err(|_| {
        FileTlsRuntimeError::MaterialReference {
            reference: material_reference.to_owned(),
        }
    })?;
    if !material_directory.starts_with(canonical_material_root) {
        return Err(FileTlsRuntimeError::MaterialBoundary {
            reference: material_reference.to_owned(),
        });
    }
    let certificate_file = canonical_material_file(
        canonical_material_root,
        &material_directory,
        CERTIFICATE_FILE_NAME,
        material_reference,
    )?;
    let private_key_file = canonical_material_file(
        canonical_material_root,
        &material_directory,
        PRIVATE_KEY_FILE_NAME,
        material_reference,
    )?;
    Ok((certificate_file, private_key_file))
}

fn canonical_material_file(
    canonical_material_root: &Path,
    material_directory: &Path,
    file_name: &str,
    material_reference: &str,
) -> Result<PathBuf, FileTlsRuntimeError> {
    let path = fs::canonicalize(material_directory.join(file_name)).map_err(|_| {
        FileTlsRuntimeError::MaterialReference {
            reference: material_reference.to_owned(),
        }
    })?;
    let regular = fs::metadata(&path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false);
    if !regular || !path.starts_with(canonical_material_root) {
        return Err(FileTlsRuntimeError::MaterialBoundary {
            reference: material_reference.to_owned(),
        });
    }
    Ok(path)
}

fn read_stable_snapshot(path: &Path) -> Result<Vec<u8>, FileTlsRuntimeError> {
    let before = source_fingerprint(path)?;
    let bytes = fs::read(path).map_err(|_| FileTlsRuntimeError::SnapshotSource)?;
    if bytes.len() > MAX_TLS_RUNTIME_SNAPSHOT_BYTES {
        return Err(TlsRuntimeSnapshotError::TooLarge {
            actual_bytes: bytes.len(),
            maximum_bytes: MAX_TLS_RUNTIME_SNAPSHOT_BYTES,
        }
        .into());
    }
    let after = source_fingerprint(path)?;
    if before != after || bytes.len() as u64 != after.length {
        return Err(FileTlsRuntimeError::SnapshotSource);
    }
    Ok(bytes)
}

fn source_fingerprint(path: &Path) -> Result<SourceFingerprint, FileTlsRuntimeError> {
    let metadata = fs::metadata(path).map_err(|_| FileTlsRuntimeError::SnapshotSource)?;
    if !metadata.is_file() || metadata.len() > MAX_TLS_RUNTIME_SNAPSHOT_BYTES as u64 {
        return Err(FileTlsRuntimeError::SnapshotSource);
    }
    Ok(SourceFingerprint {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn parse_timestamp(value: &str) -> Result<i64, FileTlsRuntimeError> {
    if value.len() != 20
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
        || value.as_bytes().get(10) != Some(&b'T')
        || value.as_bytes().get(13) != Some(&b':')
        || value.as_bytes().get(16) != Some(&b':')
        || value.as_bytes().get(19) != Some(&b'Z')
    {
        return Err(FileTlsRuntimeError::Config(
            "snapshot timestamp is not canonical UTC RFC 3339 seconds".to_owned(),
        ));
    }
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|timestamp| timestamp.unix_timestamp())
        .map_err(|_| FileTlsRuntimeError::Config("snapshot timestamp is invalid".to_owned()))
}

fn map_version(version: TlsRuntimeVersion) -> TlsVersion {
    match version {
        TlsRuntimeVersion::Tls12 => TlsVersion::Tls12,
        TlsRuntimeVersion::Tls13 => TlsVersion::Tls13,
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use rcgen::{date_time_ymd, CertificateParams, KeyPair};
    use sdkwork_webserver_core::tls_runtime::{
        tls_assignment_snapshot_sha256, TlsAssignmentSnapshot, TlsCertificateAssignment,
        TlsRuntimeLimits, TlsRuntimePolicy,
    };
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn material_reference_accepts_only_bounded_opaque_file_ids() {
        let root = tempdir().unwrap();
        let canonical_root = fs::canonicalize(root.path()).unwrap();
        for reference in [
            "vault:certificate-1",
            "file:../escape",
            "file:sub/path",
            "file:C:stream",
            "file:.",
            "file:..",
            "file:",
        ] {
            assert!(resolve_material_paths(&canonical_root, reference).is_err());
        }
    }

    #[test]
    fn canonical_timestamp_is_compared_as_unix_seconds() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:00Z").unwrap(), 0);
        assert!(parse_timestamp("2026-07-23 00:00:00Z").is_err());
    }

    #[test]
    fn snapshot_alpn_must_be_supported_by_the_bound_listener() {
        assert!(alpn_is_compatible(
            &[b"h2".to_vec(), b"http/1.1".to_vec()],
            (true, true)
        ));
        assert!(!alpn_is_compatible(&[b"h2".to_vec()], (true, false)));
        assert!(!alpn_is_compatible(&[b"http/1.1".to_vec()], (false, true)));
    }

    #[test]
    fn controller_loads_verified_material_and_rejects_scope_or_fingerprint_drift() {
        let fixture = tls_fixture("node-0001", "certificate-v1", "compiler/1");
        let controller = FileTlsRuntimeController::load(fixture.config.clone()).unwrap();
        assert_eq!(controller.listener_id(), "https-public");

        let wrong_node = tls_fixture("node-0002", "certificate-v1", "compiler/1");
        let error = FileTlsRuntimeController::load(FileTlsRuntimeConfig {
            node_uuid: "node-0001".to_owned(),
            ..wrong_node.config
        })
        .err()
        .expect("cross-node snapshot must fail");
        assert!(matches!(error, FileTlsRuntimeError::NodeScope { .. }));

        let mut snapshot = fixture.snapshot;
        snapshot.assignments[0].expected_fingerprint_sha256 = "0".repeat(64);
        sign_and_write_snapshot(&fixture.snapshot_file, &mut snapshot);
        let error = FileTlsRuntimeController::load(fixture.config)
            .err()
            .expect("fingerprint drift must fail");
        assert!(matches!(error, FileTlsRuntimeError::Fingerprint { .. }));
    }

    #[test]
    fn valid_candidate_atomically_replaces_the_active_rustls_config() {
        let mut fixture = tls_fixture("node-0001", "certificate-v1", "compiler/1");
        let controller = FileTlsRuntimeController::load(fixture.config.clone()).unwrap();
        let before = controller.rustls.get_inner();

        let (certificate_pem, private_key_pem, fingerprint, _) = certificate_material();
        let material_directory = fixture.material_root.join("certificate-v2");
        fs::create_dir_all(&material_directory).unwrap();
        fs::write(
            material_directory.join(CERTIFICATE_FILE_NAME),
            certificate_pem,
        )
        .unwrap();
        fs::write(
            material_directory.join(PRIVATE_KEY_FILE_NAME),
            private_key_pem,
        )
        .unwrap();
        fixture.snapshot.assignments[0].certificate_version = "certificate-v2".to_owned();
        fixture.snapshot.assignments[0].material_reference = "file:certificate-v2".to_owned();
        fixture.snapshot.assignments[0].expected_fingerprint_sha256 = fingerprint;
        fixture.snapshot.generation = 2;
        fixture.snapshot.compiler_version = "compiler/22".to_owned();
        sign_and_write_snapshot(&fixture.snapshot_file, &mut fixture.snapshot);

        let candidate = load_candidate(&fixture.config, &fixture.material_root, None)
            .unwrap()
            .unwrap();
        controller.activate(candidate);
        let after = controller.rustls.get_inner();
        assert!(!Arc::ptr_eq(&before, &after));
    }

    #[tokio::test]
    async fn dynamic_tls_config_completes_a_real_sni_handshake_with_the_declared_version() {
        let mut fixture = tls_fixture("node-0001", "certificate-v1", "compiler/1");
        fixture.snapshot.assignments[0].policy.maximum_version = TlsRuntimeVersion::Tls12;
        sign_and_write_snapshot(&fixture.snapshot_file, &mut fixture.snapshot);
        let controller = FileTlsRuntimeController::load(fixture.config).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(controller.rustls.get_inner());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let stream = acceptor.accept(stream).await.unwrap();
            stream.get_ref().1.protocol_version()
        });

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                fixture.certificate_der,
            ))
            .unwrap();
        let mut client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let server_name = rustls::pki_types::ServerName::try_from("site.example.test")
            .unwrap()
            .to_owned();
        let client = connector.connect(server_name, stream).await.unwrap();
        assert_eq!(
            client.get_ref().1.protocol_version(),
            Some(rustls::ProtocolVersion::TLSv1_2)
        );
        assert_eq!(
            server.await.unwrap(),
            Some(rustls::ProtocolVersion::TLSv1_2)
        );
    }

    #[test]
    fn recovery_restores_last_known_good_when_the_source_is_corrupt() {
        let mut fixture = tls_fixture("node-0001", "certificate-v1", "compiler/1");
        let recovery_directory = fixture._directory.path().join("tls-recovery");
        fixture.config.recovery_directory = Some(recovery_directory.clone());
        let expected_sha256 = fixture.snapshot.snapshot_sha256.clone();
        let controller = FileTlsRuntimeController::load(fixture.config.clone()).unwrap();
        assert_eq!(
            *lock_unpoisoned(&controller.active_snapshot_sha256),
            expected_sha256
        );
        assert!(recovery_directory.join(RECOVERY_SLOT_A_FILE).is_file());

        fs::write(&fixture.snapshot_file, b"corrupt source").unwrap();
        let recovered = FileTlsRuntimeController::load(fixture.config).unwrap();
        assert_eq!(
            *lock_unpoisoned(&recovered.active_snapshot_sha256),
            fixture.snapshot.snapshot_sha256
        );
    }

    #[test]
    fn candidate_order_uses_monotonic_generation_instead_of_wall_clock_time() {
        let mut fixture = tls_fixture("node-0001", "certificate-v1", "compiler/1");
        let controller = FileTlsRuntimeController::load(fixture.config.clone()).unwrap();

        fixture.snapshot.generation = 2;
        fixture.snapshot.generated_at = "2020-01-01T00:00:00Z".to_owned();
        fixture.snapshot.compiler_version = "compiler/2".to_owned();
        sign_and_write_snapshot(&fixture.snapshot_file, &mut fixture.snapshot);
        let candidate = load_candidate(&fixture.config, &fixture.material_root, None)
            .unwrap()
            .unwrap();
        controller.validate_candidate_order(&candidate).unwrap();
        controller.activate(candidate);

        fixture.snapshot.generation = 1;
        fixture.snapshot.generated_at = "2040-01-01T00:00:00Z".to_owned();
        fixture.snapshot.compiler_version = "compiler/3".to_owned();
        sign_and_write_snapshot(&fixture.snapshot_file, &mut fixture.snapshot);
        let stale = load_candidate(&fixture.config, &fixture.material_root, None)
            .unwrap()
            .unwrap();
        assert!(matches!(
            controller.validate_candidate_order(&stale),
            Err(FileTlsRuntimeError::CandidateStale)
        ));

        fixture.snapshot.generation = 2;
        fixture.snapshot.compiler_version = "compiler/4".to_owned();
        sign_and_write_snapshot(&fixture.snapshot_file, &mut fixture.snapshot);
        let conflict = load_candidate(&fixture.config, &fixture.material_root, None)
            .unwrap()
            .unwrap();
        assert!(matches!(
            controller.validate_candidate_order(&conflict),
            Err(FileTlsRuntimeError::CandidateConflict)
        ));
    }

    struct TlsFixture {
        _directory: tempfile::TempDir,
        config: FileTlsRuntimeConfig,
        material_root: PathBuf,
        snapshot_file: PathBuf,
        snapshot: TlsAssignmentSnapshot,
        certificate_der: Vec<u8>,
    }

    fn tls_fixture(node_uuid: &str, material_id: &str, compiler_version: &str) -> TlsFixture {
        let directory = tempdir().unwrap();
        let material_root = directory.path().join("materials");
        let material_directory = material_root.join(material_id);
        fs::create_dir_all(&material_directory).unwrap();
        let (certificate_pem, private_key_pem, fingerprint, certificate_der) =
            certificate_material();
        fs::write(
            material_directory.join(CERTIFICATE_FILE_NAME),
            certificate_pem,
        )
        .unwrap();
        fs::write(
            material_directory.join(PRIVATE_KEY_FILE_NAME),
            private_key_pem,
        )
        .unwrap();
        let snapshot_file = directory.path().join("tls-runtime.json");
        let mut snapshot = TlsAssignmentSnapshot {
            schema_version: "sdkwork.tls-runtime.v1".to_owned(),
            kind: "sdkwork.tls-runtime.snapshot".to_owned(),
            snapshot_uuid: "snapshot-0001".to_owned(),
            node_uuid: node_uuid.to_owned(),
            generation: 1,
            generated_at: "2026-07-23T00:00:00Z".to_owned(),
            compiler_version: compiler_version.to_owned(),
            snapshot_sha256: "0".repeat(64),
            assignments: vec![TlsCertificateAssignment {
                assignment_uuid: "assignment-0001".to_owned(),
                certificate_uuid: "certificate-0001".to_owned(),
                certificate_version: material_id.to_owned(),
                material_reference: format!("file:{material_id}"),
                expected_fingerprint_sha256: fingerprint,
                server_names: vec!["site.example.test".to_owned()],
                not_before: "2020-01-01T00:00:00Z".to_owned(),
                not_after: "2040-01-01T00:00:00Z".to_owned(),
                policy: TlsRuntimePolicy {
                    minimum_version: TlsRuntimeVersion::Tls12,
                    maximum_version: TlsRuntimeVersion::Tls13,
                    alpn: vec!["h2".to_owned(), "http/1.1".to_owned()],
                },
            }],
            limits: TlsRuntimeLimits {
                maximum_assignments: 8,
                maximum_server_names_per_assignment: 8,
            },
        };
        sign_and_write_snapshot(&snapshot_file, &mut snapshot);
        let config = FileTlsRuntimeConfig {
            snapshot_file: snapshot_file.clone(),
            material_root: material_root.clone(),
            listener_id: "https-public".to_owned(),
            node_uuid: node_uuid.to_owned(),
            poll_interval: Duration::from_millis(250),
            recovery_directory: None,
        };
        TlsFixture {
            _directory: directory,
            config,
            material_root: fs::canonicalize(material_root).unwrap(),
            snapshot_file,
            snapshot,
            certificate_der,
        }
    }

    fn certificate_material() -> (String, String, String, Vec<u8>) {
        let mut params = CertificateParams::new(vec!["site.example.test".to_owned()]).unwrap();
        params.not_before = date_time_ymd(2020, 1, 1);
        params.not_after = date_time_ymd(2040, 1, 1);
        let key = KeyPair::generate().unwrap();
        let certificate = params.self_signed(&key).unwrap();
        let fingerprint = format!("{:x}", Sha256::digest(certificate.der().as_ref()));
        (
            certificate.pem(),
            key.serialize_pem(),
            fingerprint,
            certificate.der().as_ref().to_vec(),
        )
    }

    fn sign_and_write_snapshot(path: &Path, snapshot: &mut TlsAssignmentSnapshot) {
        snapshot.snapshot_sha256 = "0".repeat(64);
        snapshot.snapshot_sha256 = tls_assignment_snapshot_sha256(snapshot).unwrap();
        fs::write(path, serde_json::to_vec(snapshot).unwrap()).unwrap();
    }
}
