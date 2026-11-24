//! SDKWork Drive domain-certificate source.
//!
//! The Drive certificate authority publishes one directory per certificate
//! bundle into a host-shared Drive certificate root:
//!
//! ```text
//! <root>/<bundle-id>/fullchain.pem
//! <root>/<bundle-id>/privkey.pem
//! ```
//!
//! `<bundle-id>` is an opaque, bounded ASCII identifier. The bundle's own leaf
//! `subjectAltName` names decide which server names it serves, so a directory
//! named after one host can still carry a multi-name or wildcard certificate.
//! Resolution is by SNI name, not by directory name.
//!
//! # Why this layer exists
//!
//! An edge can serve certificates from three sources. Configured files are
//! local operator state and assigned certificates arrive through the control
//! plane, but each is bound to its own deployment step. The Drive certificate
//! authority is the shared, centrally managed source: a certificate published
//! there must answer a matching SNI name before either of the other two, so a
//! Drive rotation takes effect without a control-plane round trip or a restart.
//!
//! # Multi-level cache
//!
//! - **L1 — resolution index.** Every handshake reads an [`ArcSwap`]ed
//!   [`CertificateSourceIndex`]; no lock, no filesystem access.
//! - **L2 — parsed bundles.** A refreshed scan reuses the already-parsed
//!   `CertifiedKey` for a bundle whose two files are byte-for-byte the ones the
//!   last scan read (same length and modification time). An unchanged refresh
//!   therefore costs one directory scan and no certificate parsing.
//! - **L3 — disk cache.** Every parsed bundle is mirrored into the cache root
//!   (`<cache-root>/<bundle-id>/{fullchain.pem,privkey.pem,drive-bundle.json}`),
//!   written atomically and with owner-only permissions. The cache is the
//!   last-known-good source when the Drive root is unmounted or temporarily
//!   unreadable, and it lets a restarted process serve before the Drive mount
//!   is available. An authoritative empty root does not restore the cache:
//!   entries the authority no longer references are pruned on every successful
//!   scan, so a withdrawn certificate cannot reappear after a mount failure.
//!
//! # Refresh
//!
//! The refresh task polls the Drive root on a bounded interval and can be
//! poked by an explicit trigger (the data-plane `SIGHUP` handler does this). A
//! refresh swaps the L1 index atomically only when the set of bundle
//! fingerprints changed, so a listener never observes a half-applied scan.
//! A bundle that fails to parse keeps its previous last-known-good entry until
//! the Drive operator publishes a valid replacement; a bundle removed from the
//! root stops answering its names on the next refresh.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, SystemTime},
};

use arc_swap::ArcSwap;
use rustls::crypto::CryptoProvider;
use sdkwork_utils_rust::{crypto::sha256_hash, datetime};
use sdkwork_webserver_core::normalize_tls_server_name;
use tokio::{
    sync::{watch, Notify},
    task::JoinHandle,
    time::MissedTickBehavior,
};
use tracing::{debug, info, warn};

use super::{
    tls_material::{install_crypto_provider, load_certificate_bundle},
    tls_resolver::{CertificateSourceIndex, ResolvableCertificate},
};

/// Shared Drive certificate root. Container deployments bind-mount it from the
/// Drive-owned `/opt/deploy/drive` directory.
pub(crate) const DRIVE_CERTIFICATE_ROOT_ENV: &str = "SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ROOT";
/// Explicit opt-in/opt-out for the Drive certificate source.
pub(crate) const DRIVE_CERTIFICATE_ENABLED_ENV: &str =
    "SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ENABLED";
/// Disk-cache root; defaults to `<root>/.sdkwork-certificate-cache`.
pub(crate) const DRIVE_CERTIFICATE_CACHE_ROOT_ENV: &str =
    "SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_CACHE_ROOT";
/// Refresh poll interval in whole seconds.
pub(crate) const DRIVE_CERTIFICATE_REFRESH_SECONDS_ENV: &str =
    "SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_REFRESH_INTERVAL_SECS";
/// Maximum number of bundles a scan will index.
pub(crate) const DRIVE_CERTIFICATE_MAX_BUNDLES_ENV: &str =
    "SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_MAX_BUNDLES";

const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 10;
const MIN_REFRESH_INTERVAL_SECS: u64 = 1;
const MAX_REFRESH_INTERVAL_SECS: u64 = 300;
const DEFAULT_MAX_BUNDLES: usize = 4_096;
const MAX_MAX_BUNDLES: usize = 16_384;
const MAX_BUNDLE_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CACHE_METADATA_BYTES: u64 = 256 * 1024;
const CERTIFICATE_FILE_NAME: &str = "fullchain.pem";
const PRIVATE_KEY_FILE_NAME: &str = "privkey.pem";
const CACHE_METADATA_FILE_NAME: &str = "drive-bundle.json";
const CACHE_SCHEMA_VERSION: u32 = 1;

/// Why the Drive certificate source could not be used.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DriveCertificateError {
    #[error("Drive certificate configuration is invalid: {0}")]
    Config(String),
    #[error("Drive certificate root is unavailable: {0}")]
    RootUnavailable(String),
    #[error("Drive certificate bundle {bundle_id} is invalid: {detail}")]
    Bundle { bundle_id: String, detail: String },
    #[error("Drive certificate cache is invalid: {0}")]
    Cache(String),
}

/// Environment-resolved configuration for the Drive certificate source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DriveCertificateConfig {
    pub(crate) root: PathBuf,
    pub(crate) cache_root: PathBuf,
    pub(crate) refresh_interval: Duration,
    pub(crate) max_bundles: usize,
}

impl DriveCertificateConfig {
    /// Resolves the source from the process environment.
    ///
    /// An explicit `SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ROOT` wins. Without
    /// one, the canonical shared Drive mount (`/opt/deploy/drive/certificates`)
    /// is used when it exists, so a standard SDKWork deployment prefers the
    /// Drive authority without repeating the path in two processes.
    /// `SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ENABLED=false` disables the source.
    ///
    /// Returns `Ok(None)` when neither an explicit root nor the canonical mount
    /// is available, which leaves the data plane's existing certificate sources
    /// unchanged.
    pub(crate) fn from_env() -> Result<Option<Self>, DriveCertificateError> {
        let enabled = std::env::var(DRIVE_CERTIFICATE_ENABLED_ENV).ok();
        if enabled.as_deref().is_some_and(is_disabled_value) {
            return Ok(None);
        }
        let configured_root = std::env::var(DRIVE_CERTIFICATE_ROOT_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let root = match configured_root {
            Some(value) => {
                validate_path_value(DRIVE_CERTIFICATE_ROOT_ENV, &value)?;
                PathBuf::from(value)
            }
            None => {
                match sdkwork_webserver_core::drive_certificates::resolve_drive_certificate_root() {
                    Some(path) => path,
                    None if enabled.is_some() => {
                        return Err(DriveCertificateError::Config(format!(
                            "{DRIVE_CERTIFICATE_ROOT_ENV} is required when {DRIVE_CERTIFICATE_ENABLED_ENV} is set and the canonical Drive mount is absent"
                        )));
                    }
                    None => return Ok(None),
                }
            }
        };
        Self::from_root(root, |key| std::env::var(key).ok()).map(Some)
    }

    #[cfg(test)]
    fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<String>,
    ) -> Result<Option<Self>, DriveCertificateError> {
        let enabled = lookup(DRIVE_CERTIFICATE_ENABLED_ENV);
        if enabled.as_deref().is_some_and(is_disabled_value) {
            return Ok(None);
        }
        let Some(root) = lookup(DRIVE_CERTIFICATE_ROOT_ENV)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        else {
            if enabled.is_some() {
                return Err(DriveCertificateError::Config(format!(
                    "{DRIVE_CERTIFICATE_ROOT_ENV} is required when {DRIVE_CERTIFICATE_ENABLED_ENV} is set"
                )));
            }
            return Ok(None);
        };
        validate_path_value(DRIVE_CERTIFICATE_ROOT_ENV, &root)?;
        Self::from_root(PathBuf::from(root), lookup).map(Some)
    }

    fn from_root(
        root: PathBuf,
        mut lookup: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, DriveCertificateError> {
        let cache_root = match lookup(DRIVE_CERTIFICATE_CACHE_ROOT_ENV)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            Some(value) => {
                validate_path_value(DRIVE_CERTIFICATE_CACHE_ROOT_ENV, &value)?;
                PathBuf::from(value)
            }
            // The cache must outlive the Drive mount, so a default inside the
            // Drive root is only a last resort for an unconfigured local run.
            // Deployed nodes already carry a durable TLS state root; the cache
            // belongs beside it, not inside it, because the material root's own
            // stale-directory sweep would otherwise treat the cache as an
            // unreferenced version directory.
            None => match lookup("SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT")
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
            {
                Some(material_root) => {
                    PathBuf::from(material_root).with_file_name("drive-certificate-cache")
                }
                None => root.join(".sdkwork-certificate-cache"),
            },
        };
        if cache_root == root {
            return Err(DriveCertificateError::Config(
                "the Drive certificate cache root must differ from the Drive certificate root"
                    .to_owned(),
            ));
        }
        let refresh_interval = parse_bounded_seconds(
            lookup(DRIVE_CERTIFICATE_REFRESH_SECONDS_ENV).as_deref(),
            DEFAULT_REFRESH_INTERVAL_SECS,
            MIN_REFRESH_INTERVAL_SECS,
            MAX_REFRESH_INTERVAL_SECS,
            DRIVE_CERTIFICATE_REFRESH_SECONDS_ENV,
        )?;
        let max_bundles = parse_bounded_count(
            lookup(DRIVE_CERTIFICATE_MAX_BUNDLES_ENV).as_deref(),
            DEFAULT_MAX_BUNDLES,
            MAX_MAX_BUNDLES,
            DRIVE_CERTIFICATE_MAX_BUNDLES_ENV,
        )?;
        Ok(Self {
            root,
            cache_root,
            refresh_interval: Duration::from_secs(refresh_interval),
            max_bundles,
        })
    }
}

fn is_disabled_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

fn validate_path_value(key: &str, value: &str) -> Result<(), DriveCertificateError> {
    sdkwork_webserver_core::drive_certificates::validate_drive_certificate_path(key, value)
        .map_err(DriveCertificateError::Config)
}

fn parse_bounded_seconds(
    value: Option<&str>,
    default: u64,
    minimum: u64,
    maximum: u64,
    key: &str,
) -> Result<u64, DriveCertificateError> {
    let parsed = match value {
        None => default,
        Some(raw) => raw.trim().parse::<u64>().map_err(|_| {
            DriveCertificateError::Config(format!(
                "{key} must be an integer between {minimum} and {maximum}"
            ))
        })?,
    };
    if !(minimum..=maximum).contains(&parsed) {
        return Err(DriveCertificateError::Config(format!(
            "{key} must be between {minimum} and {maximum}"
        )));
    }
    Ok(parsed)
}

fn parse_bounded_count(
    value: Option<&str>,
    default: usize,
    maximum: usize,
    key: &str,
) -> Result<usize, DriveCertificateError> {
    let parsed = match value {
        None => default,
        Some(raw) => raw.trim().parse::<usize>().map_err(|_| {
            DriveCertificateError::Config(format!(
                "{key} must be an integer between 1 and {maximum}"
            ))
        })?,
    };
    if parsed == 0 || parsed > maximum {
        return Err(DriveCertificateError::Config(format!(
            "{key} must be between 1 and {maximum}"
        )));
    }
    Ok(parsed)
}

/// One bundle as the last scan parsed it.
#[derive(Clone)]
struct CachedBundle {
    bundle_id: String,
    certificate_stat: BundleFileStat,
    private_key_stat: BundleFileStat,
    /// Newest modification time of the two files, used to let a freshly
    /// published bundle outrank an older one that still claims the same name
    /// during the publication window before the superseded bundle is pruned.
    newest_modified: Option<SystemTime>,
    fingerprint_sha256: String,
    server_names: Vec<String>,
    not_before_unix_seconds: i64,
    not_after_unix_seconds: i64,
    resolvable: Arc<ResolvableCertificate>,
}

impl CachedBundle {
    fn matches(&self, certificate: BundleFileStat, private_key: BundleFileStat) -> bool {
        self.certificate_stat == certificate && self.private_key_stat == private_key
    }
}

/// The size and modification time of one bundle file, used as the L2 cache key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BundleFileStat {
    length: u64,
    modified: Option<SystemTime>,
}

/// Outcome of one refresh, reported for logs and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DriveRefreshSummary {
    /// Whether the set of bundle fingerprints changed (and the index swapped).
    pub(crate) changed: bool,
    /// Bundles indexed after the scan.
    pub(crate) bundles: usize,
    /// Monotonic index revision after the scan.
    pub(crate) revision: u64,
}

struct DriveScan {
    index: CertificateSourceIndex,
    digest: String,
    bundles: Vec<CachedBundle>,
}

/// Live Drive certificate source shared by every listener in the process.
pub(crate) struct DriveCertificateStore {
    config: DriveCertificateConfig,
    index: ArcSwap<CertificateSourceIndex>,
    /// Content digest of the index currently installed.
    digest: Mutex<String>,
    /// L2 parsed-bundle cache, keyed by bundle id.
    bundle_cache: Mutex<HashMap<String, CachedBundle>>,
    revision: AtomicU64,
    refresh_notify: Notify,
}

impl fmt::Debug for DriveCertificateStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DriveCertificateStore")
            .field("root", &self.config.root)
            .field("cache_root", &self.config.cache_root)
            .field("refresh_interval", &self.config.refresh_interval)
            .field("revision", &self.revision.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl DriveCertificateStore {
    /// Loads the source, scanning the Drive root once.
    ///
    /// A missing or unreadable root is not a startup failure: the disk cache is
    /// tried next, and an empty source simply leaves the configured and
    /// assigned certificate layers in charge. A later refresh picks the Drive
    /// root up as soon as it appears.
    ///
    /// An *authoritative empty* root is different from an unavailable one: a
    /// readable root with no bundles clears the index rather than restoring the
    /// cache. Serving a stale certificate after the Drive authority removed it
    /// would violate the file/data consistency contract; the cache is only the
    /// last-known-good fallback for a root this process cannot read.
    pub(crate) fn load(config: DriveCertificateConfig) -> Result<Arc<Self>, DriveCertificateError> {
        install_crypto_provider()
            .map_err(|error| DriveCertificateError::Config(error.to_string()))?;
        let store = Arc::new(Self {
            config,
            index: ArcSwap::from_pointee(CertificateSourceIndex::default()),
            digest: Mutex::new(String::new()),
            bundle_cache: Mutex::new(HashMap::new()),
            revision: AtomicU64::new(0),
            refresh_notify: Notify::new(),
        });
        match store.refresh() {
            Ok(summary) => {
                if summary.bundles > 0 {
                    info!(
                        root = %store.config.root.display(),
                        bundles = summary.bundles,
                        revision = summary.revision,
                        "Drive certificate source loaded"
                    );
                } else {
                    info!(
                        root = %store.config.root.display(),
                        "Drive certificate root is empty; the Drive source serves no names until a bundle is published"
                    );
                }
            }
            Err(error) => {
                warn!(
                    error = %error,
                    root = %store.config.root.display(),
                    "Drive certificate root is unavailable; restoring the disk cache"
                );
                match store.restore_from_cache() {
                    Ok(0) => {
                        warn!("Drive certificate source starts empty until the shared root appears")
                    }
                    Ok(count) => info!(bundles = count, "Drive certificate disk cache restored"),
                    Err(error) => {
                        warn!(error = %error, "Drive certificate disk cache could not be restored")
                    }
                }
            }
        }
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.index.load().is_empty()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn bundle_count(&self) -> usize {
        lock_unpoisoned(&self.bundle_cache).len()
    }

    /// The usable Drive certificate covering `server_name`, if any.
    pub(crate) fn select_usable(
        &self,
        server_name: &str,
        now_unix_seconds: i64,
    ) -> Option<Arc<ResolvableCertificate>> {
        self.index
            .load()
            .select_usable(server_name, now_unix_seconds)
            .cloned()
    }

    /// The Drive certificate covering `server_name`, ignoring validity.
    pub(crate) fn select_any(&self, server_name: &str) -> Option<Arc<ResolvableCertificate>> {
        self.index.load().select_any(server_name).cloned()
    }

    /// Whether every declared server name has a usable Drive certificate.
    ///
    /// The load path uses this to decide whether an unusable local certificate
    /// file can be dropped: Drive can take over exactly the names the local file
    /// declared. A wildcard declaration (`*.suffix`) is answered by the Drive
    /// wildcard table, and an expired Drive bundle does not count — the fallback
    /// never prefers an unusable Drive certificate over failing closed.
    pub(crate) fn covers_all_declared_names_at(
        &self,
        server_names: &[String],
        now_unix_seconds: i64,
    ) -> bool {
        if server_names.is_empty() {
            return false;
        }
        let index = self.index.load();
        server_names
            .iter()
            .all(|name| index.covers_declared_name_at(name, now_unix_seconds))
    }

    /// Re-scans the Drive root and swaps the L1 index when the fingerprint set
    /// changed.
    ///
    /// Runs on a blocking thread; the handshake path never calls it.
    pub(crate) fn refresh(&self) -> Result<DriveRefreshSummary, DriveCertificateError> {
        let provider = CryptoProvider::get_default().ok_or_else(|| {
            DriveCertificateError::Config("Rustls crypto provider is not installed".to_owned())
        })?;
        let scan = self.scan_drive(provider)?;
        let changed = {
            let mut digest = lock_unpoisoned(&self.digest);
            if *digest != scan.digest {
                *digest = scan.digest.clone();
                true
            } else {
                false
            }
        };
        if changed {
            self.index.store(Arc::new(scan.index));
            self.revision.fetch_add(1, Ordering::Release);
        }
        self.prune_cache(&scan.bundles);
        Ok(DriveRefreshSummary {
            changed,
            bundles: scan.bundles.len(),
            revision: self.revision(),
        })
    }

    /// Asks [`Self::spawn_refresh`] to refresh as soon as it can.
    pub(crate) fn trigger_refresh(&self) {
        self.refresh_notify.notify_one();
    }

    /// Spawns the background refresh loop.
    pub(crate) fn spawn_refresh(
        self: &Arc<Self>,
        mut shutdown: watch::Receiver<bool>,
    ) -> JoinHandle<()> {
        let store = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(store.config.refresh_interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            // The constructor already scanned once; skip the immediate tick.
            ticker.tick().await;
            let mut last_error: Option<String> = None;
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            return;
                        }
                    }
                    _ = store.refresh_notify.notified() => {}
                    _ = ticker.tick() => {}
                }
                let refresh_store = Arc::clone(&store);
                let result = tokio::task::spawn_blocking(move || refresh_store.refresh()).await;
                match result {
                    Ok(Ok(summary)) => {
                        if summary.changed {
                            info!(
                                bundles = summary.bundles,
                                revision = summary.revision,
                                "Drive certificate refresh applied"
                            );
                        }
                        last_error = None;
                    }
                    Ok(Err(error)) => {
                        // A Drive root that is missing for a while is expected;
                        // log each distinct failure once instead of every tick.
                        let text = error.to_string();
                        if last_error.as_deref() != Some(text.as_str()) {
                            warn!(error = %error, "Drive certificate refresh failed; the last-known-good index stays active");
                            last_error = Some(text);
                        }
                    }
                    Err(error) => {
                        warn!(error = %error, "Drive certificate refresh task failed");
                    }
                }
            }
        })
    }

    fn scan_drive(&self, provider: &CryptoProvider) -> Result<DriveScan, DriveCertificateError> {
        let metadata = fs::symlink_metadata(&self.config.root).map_err(|error| {
            DriveCertificateError::RootUnavailable(format!(
                "{}: {error}",
                self.config.root.display()
            ))
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(DriveCertificateError::RootUnavailable(format!(
                "{} is not a real directory",
                self.config.root.display()
            )));
        }
        let mut directories = Vec::new();
        let entries = fs::read_dir(&self.config.root).map_err(|error| {
            DriveCertificateError::RootUnavailable(format!(
                "{}: {error}",
                self.config.root.display()
            ))
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                DriveCertificateError::RootUnavailable(format!(
                    "{}: {error}",
                    self.config.root.display()
                ))
            })?;
            let name = entry.file_name();
            let Some(bundle_id) = name.to_str() else {
                debug!(name = ?name, "ignoring a non-UTF-8 Drive certificate entry");
                continue;
            };
            if !valid_bundle_id(bundle_id) {
                continue;
            }
            directories.push((bundle_id.to_owned(), entry.path()));
            if directories.len() >= self.config.max_bundles {
                warn!(
                    maximum = self.config.max_bundles,
                    root = %self.config.root.display(),
                    "Drive certificate root exceeds the configured bundle bound; the rest of the scan is skipped"
                );
                break;
            }
        }
        directories.sort_by(|left, right| left.0.cmp(&right.0));

        let mut bundles = Vec::with_capacity(directories.len());
        for (bundle_id, directory) in directories {
            match self.load_drive_bundle(&directory, &bundle_id, provider) {
                Ok(bundle) => bundles.push(bundle),
                Err(error) => {
                    // Keep the previous parsed bundle so a malformed rotation
                    // cannot take the name dark while Drive is being repaired.
                    let previous = lock_unpoisoned(&self.bundle_cache).get(&bundle_id).cloned();
                    if let Some(previous) = previous {
                        warn!(
                            error = %error,
                            bundle_id = %bundle_id,
                            "Drive certificate bundle is invalid; keeping the last-known-good bundle"
                        );
                        bundles.push(previous);
                    } else {
                        warn!(error = %error, bundle_id = %bundle_id, "Drive certificate bundle is invalid and is skipped");
                    }
                }
            }
        }
        // A bundle removed from the Drive root must stop answering its names.
        {
            let present = bundles
                .iter()
                .map(|bundle| bundle.bundle_id.clone())
                .collect::<HashSet<_>>();
            lock_unpoisoned(&self.bundle_cache).retain(|bundle_id, _| present.contains(bundle_id));
        }
        let index = build_index(&bundles);
        let digest = bundle_digest(&bundles);
        Ok(DriveScan {
            index,
            digest,
            bundles,
        })
    }

    fn load_drive_bundle(
        &self,
        directory: &Path,
        bundle_id: &str,
        provider: &CryptoProvider,
    ) -> Result<CachedBundle, DriveCertificateError> {
        let metadata =
            fs::symlink_metadata(directory).map_err(|error| DriveCertificateError::Bundle {
                bundle_id: bundle_id.to_owned(),
                detail: format!("stat bundle directory: {error}"),
            })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(DriveCertificateError::Bundle {
                bundle_id: bundle_id.to_owned(),
                detail: "bundle path is not a real directory".to_owned(),
            });
        }
        let certificate_path = directory.join(CERTIFICATE_FILE_NAME);
        let private_key_path = directory.join(PRIVATE_KEY_FILE_NAME);
        let certificate_stat = bundle_file_stat(&certificate_path, bundle_id)?;
        let private_key_stat = bundle_file_stat(&private_key_path, bundle_id)?;
        if let Some(cached) = lock_unpoisoned(&self.bundle_cache).get(bundle_id) {
            if cached.matches(certificate_stat, private_key_stat) {
                return Ok(cached.clone());
            }
        }
        let certificate_pem = read_bundle_file(&certificate_path, bundle_id)?;
        let private_key_pem = read_bundle_file(&private_key_path, bundle_id)?;
        let loaded = load_certificate_bundle(&certificate_pem, &private_key_pem, provider, true)
            .map_err(|error| DriveCertificateError::Bundle {
                bundle_id: bundle_id.to_owned(),
                detail: error.to_string(),
            })?;
        let server_names = normalize_bundle_server_names(&loaded.server_names);
        if server_names.is_empty() {
            return Err(DriveCertificateError::Bundle {
                bundle_id: bundle_id.to_owned(),
                detail: "leaf subjectAltName contains no server name the data plane can index"
                    .to_owned(),
            });
        }
        let fingerprint_sha256 = loaded.leaf_fingerprint_sha256.clone();
        let cached = CachedBundle {
            bundle_id: bundle_id.to_owned(),
            certificate_stat,
            private_key_stat,
            newest_modified: newest_modified(certificate_stat, private_key_stat),
            fingerprint_sha256,
            server_names,
            not_before_unix_seconds: loaded.not_before_unix_seconds,
            not_after_unix_seconds: loaded.not_after_unix_seconds,
            resolvable: Arc::new(ResolvableCertificate::new(
                Arc::new(loaded.certified_key),
                loaded.leaf_fingerprint_sha256,
                loaded.not_before_unix_seconds,
                loaded.not_after_unix_seconds,
            )),
        };
        lock_unpoisoned(&self.bundle_cache).insert(bundle_id.to_owned(), cached.clone());
        self.write_cache_entry(&cached, &certificate_pem, &private_key_pem);
        Ok(cached)
    }

    fn restore_from_cache(&self) -> Result<usize, DriveCertificateError> {
        let provider = CryptoProvider::get_default().ok_or_else(|| {
            DriveCertificateError::Config("Rustls crypto provider is not installed".to_owned())
        })?;
        let entries = match fs::read_dir(&self.config.cache_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => {
                return Err(DriveCertificateError::Cache(format!(
                    "{}: {error}",
                    self.config.cache_root.display()
                )))
            }
        };
        let mut bundles = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                DriveCertificateError::Cache(format!(
                    "{}: {error}",
                    self.config.cache_root.display()
                ))
            })?;
            let name = entry.file_name();
            let Some(bundle_id) = name.to_str() else {
                continue;
            };
            if !valid_bundle_id(bundle_id) {
                continue;
            }
            match self.load_drive_bundle(&entry.path(), bundle_id, provider) {
                Ok(bundle) => bundles.push(bundle),
                Err(error) => {
                    debug!(error = %error, "ignoring an unusable Drive certificate cache entry")
                }
            }
        }
        if bundles.is_empty() {
            return Ok(0);
        }
        let index = build_index(&bundles);
        let digest = bundle_digest(&bundles);
        *lock_unpoisoned(&self.digest) = digest;
        self.index.store(Arc::new(index));
        self.revision.fetch_add(1, Ordering::Release);
        Ok(bundles.len())
    }

    fn write_cache_entry(
        &self,
        bundle: &CachedBundle,
        certificate_pem: &[u8],
        private_key_pem: &[u8],
    ) {
        if let Err(error) = self.write_cache_entry_inner(bundle, certificate_pem, private_key_pem) {
            warn!(
                error = %error,
                bundle_id = %bundle.bundle_id,
                cache_root = %self.config.cache_root.display(),
                "Drive certificate disk-cache entry was not written; the in-memory index keeps serving"
            );
        }
    }

    fn write_cache_entry_inner(
        &self,
        bundle: &CachedBundle,
        certificate_pem: &[u8],
        private_key_pem: &[u8],
    ) -> Result<(), DriveCertificateError> {
        let directory = self.config.cache_root.join(&bundle.bundle_id);
        ensure_private_directory(&directory).map_err(|error| {
            DriveCertificateError::Cache(format!("create {}: {error}", directory.display()))
        })?;
        if read_cache_metadata(&directory, &bundle.bundle_id)
            .is_some_and(|existing| existing.fingerprint_sha256 == bundle.fingerprint_sha256)
            && directory.join(CERTIFICATE_FILE_NAME).is_file()
            && directory.join(PRIVATE_KEY_FILE_NAME).is_file()
        {
            return Ok(());
        }
        write_atomic_file(&directory.join(CERTIFICATE_FILE_NAME), certificate_pem)?;
        write_atomic_file(&directory.join(PRIVATE_KEY_FILE_NAME), private_key_pem)?;
        let metadata = CacheMetadata {
            schema_version: CACHE_SCHEMA_VERSION,
            bundle_id: bundle.bundle_id.clone(),
            fingerprint_sha256: bundle.fingerprint_sha256.clone(),
            server_names: bundle.server_names.clone(),
            not_before_unix_seconds: bundle.not_before_unix_seconds,
            not_after_unix_seconds: bundle.not_after_unix_seconds,
            written_at: datetime::format_datetime(datetime::now(), None),
        };
        let serialized = serde_json::to_vec(&metadata).map_err(|error| {
            DriveCertificateError::Cache(format!("serialize metadata: {error}"))
        })?;
        write_atomic_file(&directory.join(CACHE_METADATA_FILE_NAME), &serialized)?;
        sync_directory(&self.config.cache_root);
        Ok(())
    }

    fn prune_cache(&self, bundles: &[CachedBundle]) {
        let referenced = bundles
            .iter()
            .map(|bundle| bundle.bundle_id.as_str())
            .collect::<HashSet<_>>();
        let entries = match fs::read_dir(&self.config.cache_root) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(bundle_id) = name.to_str() else {
                continue;
            };
            if !valid_bundle_id(bundle_id) || referenced.contains(bundle_id) {
                continue;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            // Only remove directories this source owns. A foreign directory
            // that happens to use a valid bundle name (or a cache root pointed
            // at the wrong place) is not ours to delete.
            if read_cache_metadata(&path, bundle_id).is_none() {
                continue;
            }
            if fs::remove_dir_all(&path).is_ok() {
                info!(path = %path.display(), "pruned a stale Drive certificate cache entry");
            }
        }
    }
}

fn build_index(bundles: &[CachedBundle]) -> CertificateSourceIndex {
    let mut index = CertificateSourceIndex::default();
    let mut ordered = bundles.iter().collect::<Vec<_>>();
    // Newest publication first. A renewed certificate and its predecessor can
    // both still claim the same server name during the short window between the
    // new bundle being written and the old one being pruned. The validity
    // window is the deterministic signal (a renewal has a later notAfter), then
    // the file timestamps, then the bundle id so the result never depends on
    // scan order.
    ordered.sort_by(|left, right| {
        right
            .not_after_unix_seconds
            .cmp(&left.not_after_unix_seconds)
            .then_with(|| {
                right
                    .not_before_unix_seconds
                    .cmp(&left.not_before_unix_seconds)
            })
            .then_with(|| right.newest_modified.cmp(&left.newest_modified))
            .then_with(|| left.bundle_id.cmp(&right.bundle_id))
    });
    for bundle in ordered {
        for server_name in &bundle.server_names {
            if let Err(conflict) = index.insert_with_regexes(
                std::slice::from_ref(server_name),
                &[],
                Arc::clone(&bundle.resolvable),
            ) {
                warn!(
                    bundle_id = %bundle.bundle_id,
                    server_name = %conflict,
                    "Drive certificate bundle claims a server name already indexed by a newer bundle; the newer bundle wins"
                );
            }
        }
    }
    index
}

fn newest_modified(certificate: BundleFileStat, private_key: BundleFileStat) -> Option<SystemTime> {
    match (certificate.modified, private_key.modified) {
        (Some(certificate), Some(private_key)) => Some(certificate.max(private_key)),
        (certificate, private_key) => certificate.or(private_key),
    }
}

fn bundle_digest(bundles: &[CachedBundle]) -> String {
    let mut entries = bundles
        .iter()
        .map(|bundle| format!("{}:{}", bundle.bundle_id, bundle.fingerprint_sha256))
        .collect::<Vec<_>>();
    entries.sort();
    sha256_hash(entries.join("\n").as_bytes())
}

fn normalize_bundle_server_names(names: &[String]) -> Vec<String> {
    let mut normalized = BTreeSet::new();
    for name in names {
        match normalize_tls_server_name(name) {
            Some(name) => {
                normalized.insert(name);
            }
            None => debug!(
                server_name = %name,
                "Drive certificate leaf declares a subjectAltName the data plane cannot index"
            ),
        }
    }
    normalized.into_iter().collect()
}

/// A bundle id is a single ordinary path component that cannot be hidden or
/// confused with a dot segment.
fn valid_bundle_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('.')
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn bundle_file_stat(path: &Path, bundle_id: &str) -> Result<BundleFileStat, DriveCertificateError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| DriveCertificateError::Bundle {
        bundle_id: bundle_id.to_owned(),
        detail: format!("stat {}: {error}", path.display()),
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DriveCertificateError::Bundle {
            bundle_id: bundle_id.to_owned(),
            detail: format!("{} is not a regular file", path.display()),
        });
    }
    if metadata.len() == 0 || metadata.len() > MAX_BUNDLE_FILE_BYTES {
        return Err(DriveCertificateError::Bundle {
            bundle_id: bundle_id.to_owned(),
            detail: format!(
                "{} is {} bytes, outside 1..={MAX_BUNDLE_FILE_BYTES}",
                path.display(),
                metadata.len()
            ),
        });
    }
    Ok(BundleFileStat {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn read_bundle_file(path: &Path, bundle_id: &str) -> Result<Vec<u8>, DriveCertificateError> {
    let bytes = fs::read(path).map_err(|error| DriveCertificateError::Bundle {
        bundle_id: bundle_id.to_owned(),
        detail: format!("read {}: {error}", path.display()),
    })?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_BUNDLE_FILE_BYTES {
        return Err(DriveCertificateError::Bundle {
            bundle_id: bundle_id.to_owned(),
            detail: format!(
                "{} is {} bytes, outside 1..={MAX_BUNDLE_FILE_BYTES}",
                path.display(),
                bytes.len()
            ),
        });
    }
    Ok(bytes)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheMetadata {
    schema_version: u32,
    bundle_id: String,
    fingerprint_sha256: String,
    server_names: Vec<String>,
    not_before_unix_seconds: i64,
    not_after_unix_seconds: i64,
    written_at: String,
}

fn read_cache_metadata(directory: &Path, bundle_id: &str) -> Option<CacheMetadata> {
    let path = directory.join(CACHE_METADATA_FILE_NAME);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    if metadata.len() == 0 || metadata.len() > MAX_CACHE_METADATA_BYTES {
        return None;
    }
    let bytes = fs::read(&path).ok()?;
    let decoded: CacheMetadata = serde_json::from_slice(&bytes).ok()?;
    (decoded.schema_version == CACHE_SCHEMA_VERSION && decoded.bundle_id == bundle_id)
        .then_some(decoded)
}

fn write_atomic_file(path: &Path, content: &[u8]) -> Result<(), DriveCertificateError> {
    let directory = path.parent().ok_or_else(|| {
        DriveCertificateError::Cache(format!("{} has no parent directory", path.display()))
    })?;
    let staged = directory.join(staged_file_name(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("certificate"),
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staged)?;
        confine_file(&file)?;
        use std::io::Write as _;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staged, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&staged);
        return Err(DriveCertificateError::Cache(format!(
            "activate {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

fn staged_file_name(final_name: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!(
        ".{final_name}.tmp-{}-{nanos}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn ensure_private_directory(path: &Path) -> std::io::Result<()> {
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

fn confine_file(file: &fs::File) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = file.metadata()?.permissions();
        permissions.set_mode(0o600);
        file.set_permissions(permissions)?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) {
    if let Ok(handle) = fs::File::open(directory) {
        let _ = handle.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) {}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The process-wide Drive certificate source, initialized from the environment
/// on first use.
///
/// `None` means the deployment does not configure a Drive certificate root (or
/// the configuration is unusable), which leaves the existing certificate
/// sources unchanged.
pub(crate) fn shared() -> Option<Arc<DriveCertificateStore>> {
    static SHARED: OnceLock<Mutex<Option<Arc<DriveCertificateStore>>>> = OnceLock::new();
    let slot = SHARED.get_or_init(|| Mutex::new(None));
    if let Some(store) = lock_unpoisoned(slot).as_ref() {
        return Some(Arc::clone(store));
    }
    // Resolve outside the lock: a scan can take a moment and another generation
    // build or the refresh supervisor may initialize concurrently.
    let config = match DriveCertificateConfig::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => return None,
        Err(error) => {
            warn!(error = %error, "Drive certificate configuration is invalid; the configured and assigned sources stay authoritative");
            return None;
        }
    };
    match DriveCertificateStore::load(config) {
        Ok(store) => {
            let mut guard = lock_unpoisoned(slot);
            if let Some(existing) = guard.as_ref() {
                return Some(Arc::clone(existing));
            }
            *guard = Some(Arc::clone(&store));
            Some(store)
        }
        Err(error) => {
            warn!(error = %error, "Drive certificate source is unavailable; the configured and assigned sources stay authoritative");
            None
        }
    }
}

/// Spawns the process-wide refresh supervisor. Returns `None` when the source is
/// explicitly disabled.
///
/// The supervisor also covers a mount or a bundle that appears after startup: it
/// keeps checking [`shared`] until the Drive root becomes available, then runs
/// the store's refresh loop. Server startup order is not a certificate
/// dependency.
pub(crate) fn spawn_shared_refresh() -> Option<(watch::Sender<bool>, JoinHandle<()>)> {
    if sdkwork_webserver_core::drive_certificates::drive_certificate_source_disabled() {
        return None;
    }
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let mut shutdown = shutdown_rx;
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Consume the immediate first tick; the initial call below is immediate.
        ticker.tick().await;
        let store = loop {
            if let Some(store) = shared() {
                break store;
            }
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return;
                    }
                }
                _ = ticker.tick() => {}
            }
        };
        let refresher = store.spawn_refresh(shutdown);
        let _ = refresher.await;
    });
    Some((shutdown_tx, handle))
}

/// Forces the process-wide source (when configured) to refresh now.
pub(crate) fn trigger_shared_refresh() {
    if let Some(store) = shared() {
        store.trigger_refresh();
    }
}

#[cfg(test)]
mod tests {
    use rcgen::{date_time_ymd, CertificateParams, KeyPair};
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use crate::data_plane::tls_resolver::ResolverLayers;

    use super::*;

    /// 2023-11-14T22:13:20Z: inside every certificate a test generates unless it
    /// says otherwise.
    const NOW: i64 = 1_700_000_000;

    fn test_config(directory: &Path) -> DriveCertificateConfig {
        DriveCertificateConfig {
            root: directory.join("drive"),
            cache_root: directory.join("cache"),
            refresh_interval: Duration::from_secs(10),
            max_bundles: 32,
        }
    }

    fn store(directory: &Path) -> Arc<DriveCertificateStore> {
        load_store(directory, true)
    }

    fn load_store(directory: &Path, create_root: bool) -> Arc<DriveCertificateStore> {
        let config = test_config(directory);
        if create_root {
            fs::create_dir_all(&config.root).expect("create Drive root");
        }
        DriveCertificateStore::load(config).expect("load Drive certificate store")
    }

    fn write_bundle(root: &Path, bundle_id: &str, server_names: &[&str]) -> String {
        write_bundle_with_validity(root, bundle_id, server_names, 2020, 2040)
    }

    fn write_bundle_with_validity(
        root: &Path,
        bundle_id: &str,
        server_names: &[&str],
        not_before_year: i32,
        not_after_year: i32,
    ) -> String {
        let mut params = CertificateParams::new(
            server_names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
        )
        .expect("certificate parameters");
        params.not_before = date_time_ymd(not_before_year, 1, 1);
        params.not_after = date_time_ymd(not_after_year, 1, 1);
        let key = KeyPair::generate().expect("generate key");
        let certificate = params.self_signed(&key).expect("self-signed certificate");
        let directory = root.join(bundle_id);
        fs::create_dir_all(&directory).expect("create bundle directory");
        fs::write(directory.join(CERTIFICATE_FILE_NAME), certificate.pem())
            .expect("write certificate");
        fs::write(directory.join(PRIVATE_KEY_FILE_NAME), key.serialize_pem())
            .expect("write private key");
        sdkwork_utils_rust::hex_encode(Sha256::digest(certificate.der().as_ref()).as_slice())
    }

    fn resolvable(
        server_names: &[&str],
        not_before_year: i32,
        not_after_year: i32,
    ) -> Arc<ResolvableCertificate> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut params = CertificateParams::new(
            server_names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
        )
        .expect("certificate parameters");
        params.not_before = date_time_ymd(not_before_year, 1, 1);
        params.not_after = date_time_ymd(not_after_year, 1, 1);
        let key = KeyPair::generate().expect("generate key");
        let certificate = params.self_signed(&key).expect("self-signed certificate");
        let loaded = crate::data_plane::tls_material::load_certificate_bundle(
            certificate.pem().as_bytes(),
            key.serialize_pem().as_bytes(),
            CryptoProvider::get_default().expect("installed crypto provider"),
            true,
        )
        .expect("load certificate bundle");
        Arc::new(ResolvableCertificate::new(
            Arc::new(loaded.certified_key),
            loaded.leaf_fingerprint_sha256,
            loaded.not_before_unix_seconds,
            loaded.not_after_unix_seconds,
        ))
    }

    fn index(entries: &[(&str, Arc<ResolvableCertificate>)]) -> CertificateSourceIndex {
        let mut index = CertificateSourceIndex::default();
        for (server_name, certificate) in entries {
            index
                .insert(&[(*server_name).to_owned()], Arc::clone(certificate))
                .expect("unambiguous server name");
        }
        index
    }

    #[test]
    fn bundles_are_indexed_by_their_leaf_server_names() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        let multi_name = write_bundle(&root, "bundle-a", &["example.com", "www.example.com"]);
        let wildcard = write_bundle(&root, "bundle-w", &["*.wild.test"]);
        let store = store(directory.path());

        assert_eq!(
            store
                .select_usable("example.com", NOW)
                .expect("exact name")
                .fingerprint_sha256,
            multi_name
        );
        assert_eq!(
            store
                .select_usable("www.example.com", NOW)
                .expect("second SAN")
                .fingerprint_sha256,
            multi_name
        );
        assert_eq!(
            store
                .select_usable("api.wild.test", NOW)
                .expect("wildcard name")
                .fingerprint_sha256,
            wildcard
        );
        assert!(store.select_usable("a.b.wild.test", NOW).is_none());
        assert!(store.select_usable("example.com.evil", NOW).is_none());
        assert!(store.select_usable("uncovered.test", NOW).is_none());
        assert_eq!(store.bundle_count(), 2);
    }

    #[test]
    fn refresh_applies_a_rotated_bundle_and_raises_the_revision() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        let first = write_bundle(&root, "site", &["site.example.test"]);
        let store = store(directory.path());
        let before = store.revision();

        std::thread::sleep(Duration::from_millis(20));
        let second = write_bundle(&root, "site", &["site.example.test"]);
        assert_ne!(first, second, "the rotation must present new material");

        let summary = store.refresh().expect("refresh");
        assert!(summary.changed);
        assert!(store.revision() > before);
        assert_eq!(
            store
                .select_usable("site.example.test", NOW)
                .expect("rotated certificate")
                .fingerprint_sha256,
            second
        );
    }

    #[test]
    fn drive_coverage_matches_declared_exact_and_wildcard_names() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        write_bundle(&root, "site", &["site.example.test", "*.wild.test"]);
        let store = store(directory.path());

        assert!(store.covers_all_declared_names_at(&["site.example.test".to_owned()], NOW));
        assert!(store.covers_all_declared_names_at(&["*.wild.test".to_owned()], NOW));
        assert!(!store.covers_all_declared_names_at(
            &["site.example.test".to_owned(), "other.test".to_owned()],
            NOW
        ));
        assert!(!store.covers_all_declared_names_at(&[], NOW));
    }

    #[test]
    fn an_expired_drive_bundle_does_not_authorize_dropping_local_material() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        write_bundle_with_validity(&root, "old", &["site.example.test"], 2000, 2001);
        let store = store(directory.path());

        assert!(
            !store.covers_all_declared_names_at(&["site.example.test".to_owned()], NOW),
            "an expired Drive bundle must not hide an unusable local certificate"
        );
    }

    #[test]
    fn the_newer_validity_window_wins_a_duplicate_server_name() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        // Id and scan order would pick `aaa-old`; the certificate with the later
        // notAfter must win instead. This is the renewal window where the new
        // bundle is written before the old one is pruned.
        write_bundle_with_validity(&root, "aaa-old", &["site.example.test"], 2020, 2030);
        let newest =
            write_bundle_with_validity(&root, "zzz-new", &["site.example.test"], 2020, 2040);

        let store = store(directory.path());
        assert_eq!(
            store
                .select_usable("site.example.test", NOW)
                .expect("a duplicate name must still resolve")
                .fingerprint_sha256,
            newest
        );
    }

    #[test]
    fn an_unchanged_refresh_neither_swaps_nor_exceeds_the_revision() {
        let directory = TempDir::new().expect("tempdir");
        write_bundle(
            &directory.path().join("drive"),
            "site",
            &["site.example.test"],
        );
        let store = store(directory.path());
        let revision = store.revision();

        let summary = store.refresh().expect("refresh");
        assert!(!summary.changed);
        assert_eq!(store.revision(), revision);
    }

    #[test]
    fn an_invalid_rotation_keeps_the_last_known_good_bundle() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        let fingerprint = write_bundle(&root, "site", &["site.example.test"]);
        let store = store(directory.path());

        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            root.join("site").join(PRIVATE_KEY_FILE_NAME),
            b"not a private key",
        )
        .expect("corrupt the private key");

        let summary = store.refresh().expect("refresh");
        assert!(
            !summary.changed,
            "keeping the previous bundle means the index did not change"
        );
        assert_eq!(
            store
                .select_usable("site.example.test", NOW)
                .expect("last-known-good bundle")
                .fingerprint_sha256,
            fingerprint
        );
    }

    #[test]
    fn an_empty_drive_root_clears_the_index_instead_of_serving_the_stale_cache() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        write_bundle(&root, "site", &["site.example.test"]);
        assert!(store(directory.path())
            .select_usable("site.example.test", NOW)
            .is_some());

        // The authority removed the bundle but the mount is still readable: the
        // next load must not resurrect the disk-cache copy.
        fs::remove_dir_all(root.join("site")).expect("remove bundle");
        let cleared = store(directory.path());
        assert!(cleared.select_usable("site.example.test", NOW).is_none());
        assert_eq!(cleared.bundle_count(), 0);
    }

    #[test]
    fn cache_pruning_leaves_unmanaged_directories_alone() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        write_bundle(&root, "site", &["site.example.test"]);
        let store = store(directory.path());

        // A directory without this source's cache metadata is not ours to
        // delete, even when its name looks like a bundle id.
        let foreign = directory.path().join("cache").join("foreign-bundle");
        fs::create_dir_all(&foreign).expect("create foreign directory");
        fs::write(foreign.join("note.txt"), b"not a certificate cache").expect("write note");

        store.refresh().expect("refresh");
        assert!(
            foreign.join("note.txt").is_file(),
            "pruning must not delete an unmanaged cache directory"
        );
    }

    #[test]
    fn disk_cache_restores_bundles_when_the_drive_root_disappears() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        let fingerprint = write_bundle(&root, "site", &["site.example.test"]);
        drop(store(directory.path()));

        fs::remove_dir_all(&root).expect("unmount the Drive root");
        let restored = load_store(directory.path(), false);

        assert_eq!(restored.bundle_count(), 1);
        assert_eq!(
            restored
                .select_usable("site.example.test", NOW)
                .expect("cached certificate")
                .fingerprint_sha256,
            fingerprint
        );
    }

    #[test]
    fn hidden_and_unsafe_bundle_names_are_ignored() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        let fingerprint = write_bundle(&root, "site", &["site.example.test"]);
        // Hidden directories are cache or metadata, never bundles.
        write_bundle(&root, ".hidden", &["hidden.example.test"]);
        let store = store(directory.path());

        assert_eq!(store.bundle_count(), 1);
        assert!(store.select_usable("hidden.example.test", NOW).is_none());
        assert_eq!(
            store
                .select_usable("site.example.test", NOW)
                .expect("valid bundle")
                .fingerprint_sha256,
            fingerprint
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_bundles_are_ignored() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        write_bundle(&root, "real", &["real.example.test"]);
        std::os::unix::fs::symlink(root.join("real"), root.join("alias"))
            .expect("create bundle symlink");

        let store = store(directory.path());
        assert_eq!(store.bundle_count(), 1);
        assert!(store.select_usable("real.example.test", NOW).is_some());
        // `alias` was skipped, not followed: its names still resolve through the
        // real bundle, and it never became a second cached bundle.
    }

    #[test]
    fn a_drive_bundle_outranks_a_configured_file_and_an_assigned_certificate() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        let drive_fingerprint = write_bundle(&root, "site", &["site.example.test"]);
        let store = store(directory.path());

        let configured = resolvable(&["site.example.test"], 2020, 2040);
        let assigned = resolvable(&["site.example.test"], 2020, 2040);
        let layers = ResolverLayers::new(
            index(&[("site.example.test", configured)]),
            index(&[("site.example.test", assigned)]),
        )
        .with_drive(Some(store));

        let (source, certificate) = layers
            .select_with_source("site.example.test", NOW)
            .expect("a source must answer the name");
        assert_eq!(
            source,
            sdkwork_webserver_core::tls_runtime::CertificateSource::Drive
        );
        assert_eq!(certificate.fingerprint_sha256, drive_fingerprint);
    }

    #[test]
    fn an_expired_drive_bundle_yields_to_a_usable_configured_file() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        write_bundle_with_validity(&root, "old", &["site.example.test"], 2000, 2001);
        let store = store(directory.path());

        let configured = resolvable(&["site.example.test"], 2020, 2040);
        let configured_fingerprint = configured.fingerprint_sha256.clone();
        let layers = ResolverLayers::new(
            index(&[("site.example.test", configured)]),
            CertificateSourceIndex::default(),
        )
        .with_drive(Some(store.clone()));

        let (source, certificate) = layers
            .select_with_source("site.example.test", NOW)
            .expect("a usable configured certificate must win");
        assert_eq!(
            source,
            sdkwork_webserver_core::tls_runtime::CertificateSource::Config
        );
        assert_eq!(certificate.fingerprint_sha256, configured_fingerprint);

        // With nothing else covering the name the expired Drive bundle is still
        // served, matching the existing availability tolerance.
        let only_drive =
            ResolverLayers::policy_only(CertificateSourceIndex::default()).with_drive(Some(store));
        let (source, _) = only_drive
            .select_with_source("site.example.test", NOW)
            .expect("an expired bundle still covers its name");
        assert_eq!(
            source,
            sdkwork_webserver_core::tls_runtime::CertificateSource::Drive
        );
    }

    #[test]
    fn configuration_is_optional_and_validated() {
        assert_eq!(
            DriveCertificateConfig::from_lookup(|_| None).expect("no configuration"),
            None
        );
        let configured = DriveCertificateConfig::from_lookup(|key| match key {
            DRIVE_CERTIFICATE_ROOT_ENV => Some("/opt/deploy/drive/certificates".to_owned()),
            "SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT" => {
                Some("/var/lib/sdkwork/webserver/tls-materials".to_owned())
            }
            _ => None,
        })
        .expect("valid configuration")
        .expect("configured source");
        assert_eq!(
            configured.cache_root,
            PathBuf::from("/var/lib/sdkwork/webserver/drive-certificate-cache")
        );

        assert!(DriveCertificateConfig::from_lookup(|key| match key {
            DRIVE_CERTIFICATE_ROOT_ENV => Some("/srv/drive-certificates".to_owned()),
            DRIVE_CERTIFICATE_REFRESH_SECONDS_ENV => Some("0".to_owned()),
            _ => None,
        })
        .is_err());
        assert!(DriveCertificateConfig::from_lookup(|key| match key {
            DRIVE_CERTIFICATE_ROOT_ENV => Some("/srv/drive-certificates".to_owned()),
            DRIVE_CERTIFICATE_MAX_BUNDLES_ENV => Some("not-a-number".to_owned()),
            _ => None,
        })
        .is_err());
        assert!(DriveCertificateConfig::from_lookup(|key| match key {
            DRIVE_CERTIFICATE_ENABLED_ENV => Some("true".to_owned()),
            _ => None,
        })
        .is_err());
        assert!(DriveCertificateConfig::from_lookup(|key| match key {
            DRIVE_CERTIFICATE_ROOT_ENV => Some("/srv/drive-certificates".to_owned()),
            DRIVE_CERTIFICATE_CACHE_ROOT_ENV => Some("/srv/drive-certificates".to_owned()),
            _ => None,
        })
        .is_err());
    }

    #[test]
    fn bundle_ids_are_single_ordinary_path_components() {
        for valid in ["site", "site.example.test", "cert_1-2026"] {
            assert!(valid_bundle_id(valid), "{valid} must be valid");
        }
        let too_long = "a".repeat(129);
        for invalid in ["", ".", "..", ".hidden", "a/b", "a\\b", "a b"] {
            assert!(!valid_bundle_id(invalid), "{invalid:?} must be rejected");
        }
        assert!(!valid_bundle_id(&too_long));
    }

    #[tokio::test]
    async fn a_drive_rotation_applies_to_the_next_handshake_on_the_same_server_config() {
        let directory = TempDir::new().expect("tempdir");
        let root = directory.path().join("drive");
        fs::create_dir_all(&root).expect("create root");
        write_bundle(&root, "site", &["site.example.test"]);
        let store = store(directory.path());
        let layers = ResolverLayers::policy_only(CertificateSourceIndex::default())
            .with_drive(Some(store.clone()));
        let server_config = crate::data_plane::tls_material::build_layered_server_config(
            Arc::new(layers),
            sdkwork_webserver_core::TlsVersion::Tls12,
            sdkwork_webserver_core::TlsVersion::Tls13,
            &["http/1.1".to_owned()],
            None,
        )
        .expect("server configuration");
        let certificate_path = root.join("site").join(CERTIFICATE_FILE_NAME);

        let first = handshake_leaf(&server_config, &certificate_path).await;

        std::thread::sleep(Duration::from_millis(20));
        write_bundle(&root, "site", &["site.example.test"]);
        let refresh = store.refresh().expect("refresh the Drive source");
        assert!(refresh.changed);
        let second = handshake_leaf(&server_config, &certificate_path).await;

        assert_ne!(
            first, second,
            "the same immutable ServerConfig must serve the rotated Drive leaf through its live resolver"
        );
    }

    async fn handshake_leaf(
        server_config: &Arc<rustls::ServerConfig>,
        certificate_path: &Path,
    ) -> Vec<u8> {
        use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName};

        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::clone(server_config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept connection");
            acceptor
                .accept(stream)
                .await
                .expect("complete TLS handshake");
        });

        let pem = fs::read(certificate_path).expect("read the served certificate");
        let mut roots = rustls::RootCertStore::empty();
        for certificate in CertificateDer::pem_slice_iter(&pem) {
            roots
                .add(certificate.expect("certificate DER"))
                .expect("trust the Drive leaf");
        }
        let mut client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .expect("connect to test listener");
        let server_name = ServerName::try_from("site.example.test")
            .expect("server name")
            .to_owned();
        let client = connector
            .connect(server_name, stream)
            .await
            .expect("client TLS handshake");
        let leaf = client
            .get_ref()
            .1
            .peer_certificates()
            .expect("peer certificates")[0]
            .as_ref()
            .to_vec();
        server.await.expect("server task");
        leaf
    }
}
