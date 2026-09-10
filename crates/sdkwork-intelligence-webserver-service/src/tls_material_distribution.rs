//! TLS material distribution for the self-hosted TLS runtime.
//!
//! After certificate operations succeed, the control plane projects the
//! node's listener certificate bindings into decrypted versioned material
//! files under the TLS material root and publishes a monotonic `tls-runtime`
//! snapshot that the data plane watches for hot reload. This is the
//! self-hosted replacement for external artifact activation: no nginx
//! directive or external process is involved.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use sdkwork_database_id::uuid_v4;
use sdkwork_webserver_contract::{
    TlsCertificateAssignmentMaterial, WebServiceError, WebServiceResult,
};
use sdkwork_webserver_core::{
    tls_runtime::{
        tls_assignment_snapshot_sha256, TlsAssignmentSnapshot, TlsCertificateAssignment,
        TlsRuntimeLimits, TlsRuntimePolicy, TlsRuntimeVersion, TLS_RUNTIME_SCHEMA_VERSION,
    },
    website_runtime::normalize_website_hostname,
};

use crate::WebService;

const TLS_RUNTIME_SNAPSHOT_KIND: &str = "sdkwork.tls-runtime.snapshot";
const TLS_DISTRIBUTOR_COMPILER_VERSION: &str = "sdkwork-web-tls-distributor/v1";
const TLS_DISTRIBUTION_LOCK_FILE: &str = ".sdkwork-tls-distribution.lock";
const DEFAULT_MATERIAL_ROOT: &str = ".sdkwork/secrets/tls-materials";
const DEFAULT_SNAPSHOT_FILE_NAME: &str = "tls-runtime.json";
const MAX_NODE_TLS_ASSIGNMENTS: usize = 256;
const MAX_SERVER_NAMES_PER_ASSIGNMENT: usize = 128;
const MAX_MATERIAL_FILE_BYTES: u64 = 1024 * 1024;
const DISTRIBUTION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
const DISTRIBUTION_LOCK_RETRY: Duration = Duration::from_millis(100);

/// Node TLS distribution configuration resolved from environment variables.
///
/// The distributor shares the data plane's material root and snapshot file
/// configuration so a single deployment keeps one authority. Distribution is
/// skipped entirely when `SDKWORK_WEBSERVER_NODE_UUID` is absent, which keeps
/// existing deployments (and workers that do not own TLS material) working
/// unchanged.
#[derive(Clone, Debug)]
pub struct TlsMaterialDistributionConfig {
    pub material_root: PathBuf,
    pub snapshot_file: PathBuf,
    pub node_uuid: Option<String>,
    pub alpn: Vec<String>,
}

impl TlsMaterialDistributionConfig {
    pub fn from_env() -> WebServiceResult<Self> {
        let material_root = std::env::var("SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT")
            .unwrap_or_else(|_| DEFAULT_MATERIAL_ROOT.to_string());
        if material_root.is_empty()
            || material_root.len() > 4_096
            || material_root
                .bytes()
                .any(|byte| byte == 0 || byte.is_ascii_control())
        {
            return Err(WebServiceError::validation(
                "SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT must contain 1..4096 safe path bytes",
            ));
        }
        let material_root = PathBuf::from(material_root);
        let snapshot_file = std::env::var("SDKWORK_WEBSERVER_TLS_RUNTIME_SNAPSHOT_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| material_root.join(DEFAULT_SNAPSHOT_FILE_NAME));
        let node_uuid = std::env::var("SDKWORK_WEBSERVER_NODE_UUID").ok();
        if let Some(node_uuid) = &node_uuid {
            if node_uuid.is_empty()
                || node_uuid.len() > 128
                || !node_uuid.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
            {
                return Err(WebServiceError::validation(
                    "SDKWORK_WEBSERVER_NODE_UUID must contain 1..128 safe ASCII bytes",
                ));
            }
        }
        let alpn = parse_alpn(
            std::env::var("SDKWORK_WEBSERVER_TLS_SNAPSHOT_ALPN")
                .unwrap_or_else(|_| "h2,http/1.1".to_string())
                .as_str(),
        )?;
        Ok(Self {
            material_root,
            snapshot_file,
            node_uuid,
            alpn,
        })
    }

    pub fn enabled(&self) -> bool {
        self.node_uuid.is_some()
    }
}

impl WebService {
    /// Best-effort node TLS material publication for control-plane callers
    /// (binding changes, revocation). Distribution configuration absence or
    /// failure is logged, never propagated to the caller.
    pub async fn publish_node_tls_material_best_effort(&self, trigger: &'static str) {
        if let Err(error) = self.publish_node_tls_material().await {
            tracing::warn!(
                trigger,
                error = ?error,
                "node TLS material distribution failed"
            );
        }
    }

    /// Projects the node's listener certificate bindings into the self-hosted
    /// TLS runtime snapshot. Idempotent: when the projected snapshot content
    /// is unchanged the on-disk snapshot is left untouched. Errors are
    /// reported to the caller; the caller decides whether distribution
    /// failure fails the surrounding operation.
    pub async fn publish_node_tls_material(&self) -> WebServiceResult<()> {
        let config = TlsMaterialDistributionConfig::from_env()?;
        if !config.enabled() {
            return Ok(());
        }
        let node_uuid = config.node_uuid.clone().expect("enabled config");
        let assignments = self
            .repository
            .load_node_tls_certificate_assignments(&node_uuid)
            .await?;
        if assignments.is_empty() {
            return Ok(());
        }
        // File-system publication (distribution lock, material writes,
        // fsync, stale pruning) is blocking work: run it on the blocking
        // thread pool so tokio worker threads never stall on disk I/O under
        // concurrent control-plane traffic.
        tokio::task::spawn_blocking(move || {
            publish_node_tls_material_blocking(&config, &assignments)
        })
        .await
        .map_err(|error| {
            WebServiceError::Internal(format!("join node TLS material distribution: {error}"))
        })?
    }
}

/// Synchronous projection of the node TLS runtime snapshot. Executed on the
/// blocking thread pool by [`WebService::publish_node_tls_material`].
fn publish_node_tls_material_blocking(
    config: &TlsMaterialDistributionConfig,
    assignments: &[TlsCertificateAssignmentMaterial],
) -> WebServiceResult<()> {
    let node_uuid = config.node_uuid.as_deref().expect("enabled config");
    let _lock = DistributionLock::acquire(&config.material_root)?;
    create_private_directory(&config.material_root).map_err(|error| {
        WebServiceError::Internal(format!(
            "create TLS material root {}: {error}",
            config.material_root.display()
        ))
    })?;
    for assignment in assignments {
        write_material_files(&config.material_root, assignment)?;
    }
    let generation = next_snapshot_generation(&config.snapshot_file)?;
    let snapshot = build_snapshot(node_uuid, &config.alpn, generation, assignments.to_vec())?;
    let snapshot_sha256 = tls_assignment_snapshot_sha256(&snapshot)
        .map_err(|error| WebServiceError::Internal(error.to_string()))?;
    if existing_snapshot_matches(&config.snapshot_file, &snapshot_sha256)? {
        return Ok(());
    }
    let serialized = serde_json::to_vec(&snapshot)
        .map_err(|error| WebServiceError::Internal(format!("serialize TLS snapshot: {error}")))?;
    write_snapshot_atomically(&config.snapshot_file, &serialized)?;
    prune_stale_material_directories(&config.material_root, &snapshot)?;
    tracing::info!(
        node_uuid,
        assignments = snapshot.assignments.len(),
        generation,
        snapshot_sha256 = %snapshot_sha256,
        "published node TLS runtime snapshot"
    );
    Ok(())
}

fn build_snapshot(
    node_uuid: &str,
    alpn: &[String],
    generation: u64,
    materials: Vec<TlsCertificateAssignmentMaterial>,
) -> WebServiceResult<TlsAssignmentSnapshot> {
    if materials.is_empty() || materials.len() > MAX_NODE_TLS_ASSIGNMENTS {
        return Err(WebServiceError::validation(
            "TLS snapshot assignment count is outside the supported range",
        ));
    }
    let mut assignments = Vec::with_capacity(materials.len());
    for material in &materials {
        if material.version_uuid.is_empty()
            || material.version_uuid.len() > 128
            || !material
                .version_uuid
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(WebServiceError::Internal(
                "TLS assignment version uuid is not a safe opaque identifier".to_string(),
            ));
        }
        let mut server_names = material
            .hostnames
            .iter()
            .map(|hostname| {
                normalize_website_hostname(hostname).ok_or_else(|| {
                    WebServiceError::Internal(format!(
                        "TLS assignment hostname {hostname} is not canonical"
                    ))
                })
            })
            .collect::<WebServiceResult<Vec<_>>>()?;
        if server_names.is_empty() || server_names.len() > MAX_SERVER_NAMES_PER_ASSIGNMENT {
            return Err(WebServiceError::Internal(
                "TLS assignment server name count is outside the supported range".to_string(),
            ));
        }
        server_names.sort();
        assignments.push(TlsCertificateAssignment {
            assignment_uuid: uuid_v4(),
            certificate_uuid: material.certificate_id.clone(),
            certificate_version: material.version_uuid.clone(),
            material_reference: format!("file:{}", material.version_uuid),
            expected_fingerprint_sha256: material.fingerprint_sha256.clone(),
            server_names,
            not_before: material.not_before.clone(),
            not_after: material.not_after.clone(),
            policy: TlsRuntimePolicy {
                minimum_version: TlsRuntimeVersion::Tls12,
                maximum_version: TlsRuntimeVersion::Tls13,
                alpn: alpn.to_vec(),
            },
        });
    }
    assignments.sort_by(|left, right| left.assignment_uuid.cmp(&right.assignment_uuid));
    let mut snapshot = TlsAssignmentSnapshot {
        schema_version: TLS_RUNTIME_SCHEMA_VERSION.to_string(),
        kind: TLS_RUNTIME_SNAPSHOT_KIND.to_string(),
        snapshot_uuid: uuid_v4(),
        node_uuid: node_uuid.to_string(),
        generation,
        generated_at: Utc::now().to_rfc3339(),
        compiler_version: TLS_DISTRIBUTOR_COMPILER_VERSION.to_string(),
        snapshot_sha256: String::new(),
        assignments,
        limits: TlsRuntimeLimits {
            maximum_assignments: MAX_NODE_TLS_ASSIGNMENTS,
            maximum_server_names_per_assignment: MAX_SERVER_NAMES_PER_ASSIGNMENT,
        },
    };
    snapshot.snapshot_sha256 = tls_assignment_snapshot_sha256(&snapshot)
        .map_err(|error| WebServiceError::Internal(error.to_string()))?;
    Ok(snapshot)
}

fn write_material_files(
    material_root: &Path,
    material: &TlsCertificateAssignmentMaterial,
) -> WebServiceResult<()> {
    let directory = material_root.join(&material.version_uuid);
    create_private_directory(&directory).map_err(|error| {
        WebServiceError::Internal(format!(
            "create TLS material directory {}: {error}",
            directory.display()
        ))
    })?;
    write_bounded_file(
        &directory.join("fullchain.pem"),
        material.fullchain_pem.as_bytes(),
    )?;
    write_bounded_file(
        &directory.join("privkey.pem"),
        material.private_key_pem.as_bytes(),
    )?;
    Ok(())
}

fn write_bounded_file(path: &Path, content: &[u8]) -> WebServiceResult<()> {
    if content.is_empty() || content.len() as u64 > MAX_MATERIAL_FILE_BYTES {
        return Err(WebServiceError::validation(
            "TLS material file size is outside the supported range",
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        WebServiceError::Internal("TLS material file has no parent directory".to_string())
    })?;
    let staged = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("material"),
        uuid_v4()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged)
        .map_err(|error| {
            WebServiceError::Internal(format!("stage TLS material {}: {error}", path.display()))
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // file.metadata() yields Metadata; the chmod must be applied to the
        // Permissions view of it (Metadata has no permissions_mut).
        let mut permissions = file
            .metadata()
            .map_err(|error| {
                WebServiceError::Internal(format!(
                    "inspect staged TLS material {}: {error}",
                    path.display()
                ))
            })?
            .permissions();
        permissions.set_mode(0o600);
        file.set_permissions(permissions).map_err(|error| {
            WebServiceError::Internal(format!(
                "confine staged TLS material {}: {error}",
                path.display()
            ))
        })?;
    }
    file.write_all(content)
        .and_then(|_| file.sync_all())
        .map_err(|error| {
            WebServiceError::Internal(format!("write TLS material {}: {error}", path.display()))
        })?;
    drop(file);
    fs::rename(&staged, path).map_err(|error| {
        WebServiceError::Internal(format!("activate TLS material {}: {error}", path.display()))
    })?;
    // Directory fsync makes the material rename durable across a crash.
    sync_directory(parent)?;
    Ok(())
}

fn next_snapshot_generation(snapshot_file: &Path) -> WebServiceResult<u64> {
    let bytes = match fs::read(snapshot_file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(1),
        Err(error) => {
            return Err(WebServiceError::Internal(format!(
                "read TLS snapshot {}: {error}",
                snapshot_file.display()
            )));
        }
    };
    if bytes.len() as u64 > 1024 * 1024 {
        return Err(WebServiceError::Internal(
            "TLS snapshot file exceeds the size limit".to_string(),
        ));
    }
    let snapshot: TlsAssignmentSnapshot = serde_json::from_slice(&bytes).map_err(|_| {
        WebServiceError::Internal("existing TLS snapshot is not valid JSON".to_string())
    })?;
    snapshot
        .generation
        .checked_add(1)
        .ok_or_else(|| WebServiceError::Internal("TLS snapshot generation overflow".to_string()))
}

fn existing_snapshot_matches(
    snapshot_file: &Path,
    snapshot_sha256: &str,
) -> WebServiceResult<bool> {
    let bytes = match fs::read(snapshot_file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(WebServiceError::Internal(format!(
                "read TLS snapshot {}: {error}",
                snapshot_file.display()
            )));
        }
    };
    if bytes.len() as u64 > 1024 * 1024 {
        return Err(WebServiceError::Internal(
            "TLS snapshot file exceeds the size limit".to_string(),
        ));
    }
    let snapshot: TlsAssignmentSnapshot = serde_json::from_slice(&bytes).map_err(|_| {
        WebServiceError::Internal("existing TLS snapshot is not valid JSON".to_string())
    })?;
    Ok(snapshot.snapshot_sha256 == snapshot_sha256)
}

fn write_snapshot_atomically(snapshot_file: &Path, serialized: &[u8]) -> WebServiceResult<()> {
    let parent = snapshot_file.parent().ok_or_else(|| {
        WebServiceError::Internal("TLS snapshot file has no parent directory".to_string())
    })?;
    create_private_directory(parent).map_err(|error| {
        WebServiceError::Internal(format!(
            "create TLS snapshot directory {}: {error}",
            parent.display()
        ))
    })?;
    let staged = parent.join(format!(".tls-runtime.tmp-{}", uuid_v4()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged)
        .map_err(|error| {
            WebServiceError::Internal(format!(
                "stage TLS snapshot {}: {error}",
                snapshot_file.display()
            ))
        })?;
    file.write_all(serialized)
        .and_then(|_| file.sync_all())
        .map_err(|error| {
            WebServiceError::Internal(format!(
                "write TLS snapshot {}: {error}",
                snapshot_file.display()
            ))
        })?;
    drop(file);
    fs::rename(&staged, snapshot_file).map_err(|error| {
        WebServiceError::Internal(format!(
            "activate TLS snapshot {}: {error}",
            snapshot_file.display()
        ))
    })?;
    // Directory fsync makes the rename durable across a crash; a failure here
    // means the activated snapshot may not survive power loss and must be
    // reported instead of silently ignored.
    sync_directory(parent)?;
    Ok(())
}

fn sync_directory(directory: &Path) -> WebServiceResult<()> {
    let handle = fs::File::open(directory).map_err(|error| {
        WebServiceError::Internal(format!(
            "open TLS snapshot directory {}: {error}",
            directory.display()
        ))
    })?;
    handle.sync_all().map_err(|error| {
        WebServiceError::Internal(format!(
            "sync TLS snapshot directory {}: {error}",
            directory.display()
        ))
    })
}

/// Grace period before an unreferenced versioned material directory is
/// pruned. The data plane polls the snapshot at its configured interval and
/// keeps serving the previously loaded Rustls configuration; the grace period
/// guarantees a crash-restarting data plane can still resolve the material it
/// last read from the recovery slots.
const STALE_MATERIAL_GRACE: std::time::Duration = std::time::Duration::from_secs(3_600);

/// Removes versioned material directories that are not referenced by the
/// published snapshot and were not modified within the grace period. Only
/// directories whose names match the opaque version-uuid shape are considered,
/// so unrelated files (the distribution lock, the snapshot) are never touched.
fn prune_stale_material_directories(
    material_root: &Path,
    snapshot: &TlsAssignmentSnapshot,
) -> WebServiceResult<()> {
    let referenced = snapshot
        .assignments
        .iter()
        .map(|assignment| assignment.certificate_version.as_str())
        .collect::<std::collections::HashSet<_>>();
    let deadline = std::time::SystemTime::now() - STALE_MATERIAL_GRACE;
    let entries = fs::read_dir(material_root).map_err(|error| {
        WebServiceError::Internal(format!(
            "list TLS material root {}: {error}",
            material_root.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            WebServiceError::Internal(format!("read TLS material directory entry: {error}"))
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if referenced.contains(name)
            || name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| {
            WebServiceError::Internal(format!(
                "inspect TLS material directory {}: {error}",
                entry.path().display()
            ))
        })?;
        if !file_type.is_dir() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                WebServiceError::Internal(format!(
                    "inspect TLS material directory timestamp {}: {error}",
                    entry.path().display()
                ))
            })?;
        if modified > deadline {
            continue;
        }
        fs::remove_dir_all(entry.path()).map_err(|error| {
            WebServiceError::Internal(format!(
                "prune stale TLS material directory {}: {error}",
                entry.path().display()
            ))
        })?;
        tracing::info!(
            pruned = %entry.path().display(),
            "pruned stale TLS material directory"
        );
    }
    Ok(())
}

fn parse_alpn(value: &str) -> WebServiceResult<Vec<String>> {
    let mut protocols = Vec::new();
    for protocol in value.split(',') {
        let protocol = protocol.trim();
        match protocol {
            "h2" | "http/1.1" => {
                if !protocols.contains(&protocol.to_string()) {
                    protocols.push(protocol.to_string());
                }
            }
            _ => {
                return Err(WebServiceError::validation(format!(
                    "SDKWORK_WEBSERVER_TLS_SNAPSHOT_ALPN contains unsupported protocol {protocol}"
                )));
            }
        }
    }
    if protocols.is_empty() {
        return Err(WebServiceError::validation(
            "SDKWORK_WEBSERVER_TLS_SNAPSHOT_ALPN must select h2 and/or http/1.1",
        ));
    }
    Ok(protocols)
}

/// Cross-process exclusion for snapshot publication. Multiple workers can
/// publish concurrently; the lock serializes the read-modify-write of the
/// generation counter so the published snapshot always carries a fresh
/// monotonic generation.
///
/// The exclusion is a kernel file lock (`File::try_lock`), not an
/// existence-checked sentinel: a worker killed while holding the lock lets
/// the kernel release it immediately, so a crash can never orphan the lock
/// and silently block every future publication (the same model as the Web
/// Node Daemon state lock).
struct DistributionLock {
    _file: fs::File,
}

impl DistributionLock {
    fn acquire(material_root: &Path) -> WebServiceResult<Self> {
        create_private_directory(material_root).map_err(|error| {
            WebServiceError::Internal(format!(
                "create TLS material root {}: {error}",
                material_root.display()
            ))
        })?;
        let path = material_root.join(TLS_DISTRIBUTION_LOCK_FILE);
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|error| {
                WebServiceError::Internal(format!(
                    "open TLS distribution lock {}: {error}",
                    path.display()
                ))
            })?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            WebServiceError::Internal(format!(
                "inspect TLS distribution lock {}: {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(WebServiceError::Internal(format!(
                "TLS distribution lock {} must be a regular file",
                path.display()
            )));
        }
        let deadline = std::time::Instant::now() + DISTRIBUTION_LOCK_TIMEOUT;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(std::fs::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(WebServiceError::Internal(
                            "TLS distribution lock is held by another publisher".to_string(),
                        ));
                    }
                    std::thread::sleep(DISTRIBUTION_LOCK_RETRY);
                }
                Err(std::fs::TryLockError::Error(error)) => {
                    return Err(WebServiceError::Internal(format!(
                        "acquire TLS distribution lock {}: {error}",
                        path.display()
                    )));
                }
            }
        }
    }
}

/// Creates a directory with owner-only permissions on Unix. TLS material
/// directories hold private key bundles; umask-dependent default modes are
/// not acceptable for them.
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn material(version_uuid: &str, hostnames: &[&str]) -> TlsCertificateAssignmentMaterial {
        TlsCertificateAssignmentMaterial {
            certificate_id: "certificate-1".to_string(),
            version_uuid: version_uuid.to_string(),
            cert_name: "cert-1".to_string(),
            hostnames: hostnames
                .iter()
                .map(|hostname| hostname.to_string())
                .collect(),
            fingerprint_sha256: "a".repeat(64),
            not_before: "2026-01-01T00:00:00Z".to_string(),
            not_after: "2027-01-01T00:00:00Z".to_string(),
            fullchain_pem: "certificate".to_string(),
            private_key_pem: "private-key".to_string(),
        }
    }

    #[test]
    fn snapshot_is_canonical_and_self_consistent() {
        let snapshot = build_snapshot(
            "node-1",
            &["h2".to_string(), "http/1.1".to_string()],
            7,
            vec![
                material("version-b", &["www.example.com", "example.com"]),
                material("version-a", &["api.example.com"]),
            ],
        )
        .expect("build snapshot");
        assert_eq!(snapshot.generation, 7);
        assert_eq!(snapshot.node_uuid, "node-1");
        assert_eq!(snapshot.snapshot_sha256.len(), 64);
        assert!(snapshot
            .snapshot_sha256
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit()));
        assert_eq!(snapshot.assignments.len(), 2);
        assert!(snapshot
            .assignments
            .windows(2)
            .all(|pair| pair[0].assignment_uuid < pair[1].assignment_uuid));
        let by_version = snapshot
            .assignments
            .iter()
            .map(|assignment| assignment.certificate_version.as_str())
            .collect::<Vec<_>>();
        assert!(by_version.contains(&"version-a"));
        assert!(by_version.contains(&"version-b"));
        let sorted = snapshot.assignments[0].server_names.clone();
        assert!(sorted.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn snapshot_normalizes_and_rejects_invalid_hostnames() {
        let snapshot = build_snapshot(
            "node-1",
            &["h2".to_string()],
            1,
            vec![material("version-a", &["Example.COM"])],
        )
        .expect("non-canonical hostnames are normalized");
        assert_eq!(snapshot.assignments[0].server_names, vec!["example.com"]);
        assert!(build_snapshot(
            "node-1",
            &["h2".to_string()],
            1,
            vec![material("version-b", &["bad/name"])],
        )
        .is_err());
    }

    #[test]
    fn snapshot_rejects_empty_assignments() {
        assert!(build_snapshot("node-1", &["h2".to_string()], 1, vec![]).is_err());
    }

    #[test]
    fn alpn_parsing_is_bounded_and_fail_closed() {
        assert_eq!(
            parse_alpn("h2,http/1.1").expect("both"),
            vec!["h2", "http/1.1"]
        );
        assert_eq!(parse_alpn("http/1.1").expect("http1"), vec!["http/1.1"]);
        assert!(parse_alpn("spdy").is_err());
        assert!(parse_alpn("").is_err());
    }
}
