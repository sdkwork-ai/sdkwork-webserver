//! The [`WebserverConfigService`]: catalog, bounded read, and validated
//! atomic write over the configuration files of the current deployment.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sdkwork_server_files_service::{resolve_contained_path, validate_allowed_root};

use crate::language::config_language_for;
use crate::wire;

/// Directory under the Web Server runtime config root holding the module
/// import plane (SDKWORK_WEBSERVER_SPEC.md §17.3).
pub const IMPORTS_CONFIG_DIRECTORY: &str = "imports.d";
/// Directory inside each sibling-module checkout holding its nginx sidecar
/// configuration tree (single source of truth owned by the module).
pub const MODULE_CONFIG_RELATIVE_DIRECTORY: &str = "deployments/webserver";
/// Workspace directory under the deploy root holding sibling-module checkouts.
pub const SDKWORK_SPACE_DIRECTORY: &str = "sdkwork-space";

/// How many timestamped backups to retain per configuration file.
const BACKUP_RETENTION: usize = 3;
/// Maximum catalog enumeration depth under `imports.d/`.
const IMPORT_SCAN_DEPTH: usize = 2;
/// Maximum enumeration depth for sibling-module sidecar directories.
const MODULE_SCAN_DEPTH: usize = 2;
/// Maximum sibling modules scanned for sidecar configs.
const MAX_MODULE_SCAN_ROOTS: usize = 256;

/// Configuration for a [`WebserverConfigService`].
#[derive(Debug, Clone)]
pub struct WebserverConfigServiceConfig {
    /// Web Server runtime configuration root (for example
    /// `/etc/sdkwork/webserver`). Owns the default config and `imports.d/`.
    pub config_root: String,
    /// Deployment root (for example `/opt/deploy`) used to enumerate
    /// sibling-module sidecar configs. `None` disables the module group.
    pub deploy_root: Option<String>,
    /// Maximum file content bytes readable through `read`.
    pub maximum_file_bytes: usize,
    /// Maximum file content bytes writable through `write`.
    pub maximum_write_bytes: usize,
    /// Maximum catalog entries returned per enumeration.
    pub maximum_entries: usize,
}

impl Default for WebserverConfigServiceConfig {
    fn default() -> Self {
        Self {
            config_root: "/etc/sdkwork/webserver".to_string(),
            deploy_root: None,
            maximum_file_bytes: 4 * 1024 * 1024,   // 4 MiB
            maximum_write_bytes: 2 * 1024 * 1024,  // 2 MiB
            maximum_entries: 4096,
        }
    }
}

/// Configuration group of a catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WebserverConfigKind {
    /// Top-level Web Server runtime configuration (for example
    /// `config.toml`).
    #[serde(rename = "default-config")]
    Default,
    /// Module import plane configuration under `imports.d/`.
    #[serde(rename = "import-config")]
    Import,
    /// Sibling-module sidecar configuration; read-only because the file is
    /// owned by its module checkout.
    #[serde(rename = "module-config")]
    Module,
}

impl WebserverConfigKind {
    /// Group-relative filesystem root the entry's `path` is resolved from.
    fn identity(self) -> &'static str {
        match self {
            WebserverConfigKind::Default => "default-config",
            WebserverConfigKind::Import => "import-config",
            WebserverConfigKind::Module => "module-config",
        }
    }

    /// Catalog display order.
    fn order(self) -> u8 {
        match self {
            WebserverConfigKind::Default => 0,
            WebserverConfigKind::Import => 1,
            WebserverConfigKind::Module => 2,
        }
    }
}

/// One manageable configuration file in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebserverConfigEntry {
    /// Stable, content-independent id (SHA-256 over the catalog identity).
    pub id: String,
    pub kind: WebserverConfigKind,
    /// File name only (for example `import.conf`).
    pub name: String,
    /// Posix path relative to the owning group root (for example
    /// `imports.d/import.conf` or `sdkwork-space/<module>/deployments/webserver/a.conf`).
    pub path: String,
    /// Monaco editor language id for the file content.
    pub language: String,
    #[serde(with = "wire::option_u64_as_string")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Last modification time, Unix seconds.
    #[serde(with = "wire::option_i64_as_string")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    /// Whether the control plane allows writing this entry.
    pub writable: bool,
}

/// The full configuration catalog of the current deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebserverConfigCatalog {
    pub config_root: String,
    pub items: Vec<WebserverConfigEntry>,
}

/// A configuration file content response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebserverConfigFile {
    pub id: String,
    pub kind: WebserverConfigKind,
    pub name: String,
    pub path: String,
    pub language: String,
    pub writable: bool,
    pub content: String,
    #[serde(with = "wire::u64_as_string")]
    pub size: u64,
    /// SHA-256 hex digest of the on-disk bytes; pass back as
    /// `expected_sha256` to write with optimistic concurrency.
    pub sha256: String,
    #[serde(with = "wire::option_i64_as_string")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// A successful write result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebserverConfigWriteResult {
    pub id: String,
    pub path: String,
    #[serde(with = "wire::u64_as_string")]
    pub size: u64,
    pub sha256: String,
    /// Backup file holding the previous content, when an existing file was
    /// overwritten.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    #[serde(with = "wire::i64_as_string")]
    pub updated_at: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum WebserverConfigError {
    #[error(transparent)]
    Containment(#[from] sdkwork_server_files_service::PathContainmentError),
    #[error("The configuration entry does not exist")]
    NotFound,
    #[error("This configuration entry is owned by its module checkout and is read-only")]
    ReadOnlyEntry,
    #[error("The file exceeds the readable size limit")]
    TooLarge,
    #[error("The content exceeds the writable size limit")]
    WriteTooLarge,
    #[error("The content is invalid: {0}")]
    InvalidContent(String),
    #[error("The file changed since it was read (concurrent edit)")]
    Conflict {
        current_sha256: String,
    },
    #[error("The file system operation failed: {0}")]
    Io(String),
}

/// Async configuration-file service bound to one deployment's config root.
#[derive(Debug, Clone)]
pub struct WebserverConfigService {
    config: WebserverConfigServiceConfig,
    config_root: PathBuf,
    deploy_root: Option<PathBuf>,
}

impl WebserverConfigService {
    /// Build a service, validating the configured roots eagerly.
    pub fn new(config: WebserverConfigServiceConfig) -> Result<Self, WebserverConfigError> {
        let config_root = validate_allowed_root(&config.config_root)?;
        let deploy_root = match &config.deploy_root {
            Some(root) if !root.is_empty() => Some(validate_allowed_root(root)?),
            _ => None,
        };
        Ok(Self {
            config,
            config_root,
            deploy_root,
        })
    }

    /// The validated, canonical Web Server configuration root.
    pub fn config_root(&self) -> &Path {
        &self.config_root
    }

    /// Enumerate the managed configuration catalog.
    ///
    /// Groups: top-level default config files, the `imports.d/` import
    /// plane, and (when a deploy root is configured) sibling-module sidecar
    /// configs. Symlinks and dotfiles are never listed, so only regular,
    /// intentional configuration files are addressable.
    pub async fn catalog(&self) -> Result<WebserverConfigCatalog, WebserverConfigError> {
        let mut items = Vec::new();

        // Default config: regular files directly in the config root.
        self.collect_group(&self.config_root.clone(), 0, WebserverConfigKind::Default, "", &mut items)
            .await?;

        // Import plane: bounded-depth files under `<config_root>/imports.d`.
        let imports_root = self.config_root.join(IMPORTS_CONFIG_DIRECTORY);
        self.collect_group(&imports_root, IMPORT_SCAN_DEPTH, WebserverConfigKind::Import, IMPORTS_CONFIG_DIRECTORY, &mut items)
            .await?;

        // Sibling-module sidecars: read-only by ownership contract.
        if let Some(deploy_root) = &self.deploy_root {
            self.collect_module_group(deploy_root, &mut items).await?;
        }

        items.sort_by(|left, right| {
            left.kind
                .order()
                .cmp(&right.kind.order())
                .then_with(|| left.path.cmp(&right.path))
        });
        items.truncate(self.config.maximum_entries);

        Ok(WebserverConfigCatalog {
            config_root: self.config.config_root.clone(),
            items,
        })
    }

    /// Read a catalog-listed configuration file with its content digest.
    pub async fn read(&self, id: &str) -> Result<WebserverConfigFile, WebserverConfigError> {
        let entry = self.lookup(id).await?;
        let absolute = self.absolute_path_for(&entry)?;
        let metadata = tokio::fs::metadata(&absolute)
            .await
            .map_err(|_| WebserverConfigError::NotFound)?;
        if !metadata.is_file() {
            return Err(WebserverConfigError::NotFound);
        }
        if metadata.len() > self.config.maximum_file_bytes as u64 {
            return Err(WebserverConfigError::TooLarge);
        }
        let bytes = tokio::fs::read(&absolute)
            .await
            .map_err(|error| WebserverConfigError::Io(error.to_string()))?;
        Ok(WebserverConfigFile {
            id: entry.id,
            kind: entry.kind,
            name: entry.name,
            path: entry.path,
            language: entry.language,
            writable: entry.writable,
            content: String::from_utf8_lossy(&bytes).into_owned(),
            size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
            updated_at: unix_seconds(metadata.modified().ok()),
        })
    }

    /// Validate and atomically overwrite a catalog-listed configuration file.
    ///
    /// - `expected_sha256` enables optimistic concurrency: when provided and
    ///   the on-disk digest differs, the write is rejected with
    ///   [`WebserverConfigError::Conflict`] so concurrent editors cannot
    ///   silently clobber each other.
    /// - A timestamped `.bak-<unix-seconds>` backup of the previous content
    ///   is created before every overwrite (newest [`BACKUP_RETENTION`]
    ///   kept).
    pub async fn write(
        &self,
        id: &str,
        content: &str,
        expected_sha256: Option<&str>,
    ) -> Result<WebserverConfigWriteResult, WebserverConfigError> {
        let entry = self.lookup(id).await?;
        if !entry.writable {
            return Err(WebserverConfigError::ReadOnlyEntry);
        }
        let absolute = self.absolute_path_for(&entry)?;

        let content_bytes = content.as_bytes();
        if content_bytes.len() > self.config.maximum_write_bytes {
            return Err(WebserverConfigError::WriteTooLarge);
        }
        if content_bytes.contains(&0) {
            return Err(WebserverConfigError::InvalidContent(
                "content must not contain NUL bytes".to_string(),
            ));
        }
        validate_content(&entry.name, content)?;

        let current_bytes = tokio::fs::read(&absolute)
            .await
            .map_err(|_| WebserverConfigError::NotFound)?;
        let current_sha256 = sha256_hex(&current_bytes);
        if let Some(expected) = expected_sha256.map(str::trim).filter(|value| !value.is_empty()) {
            if !expected.eq_ignore_ascii_case(&current_sha256) {
                return Err(WebserverConfigError::Conflict { current_sha256 });
            }
        }

        // Backup the previous content before overwriting.
        let backup_path = self
            .create_backup(&absolute)
            .await
            .map_err(|error| WebserverConfigError::Io(error.to_string()))?;

        // Atomic replacement: temp file in the same directory, fsync, rename.
        let temporary = absolute.with_file_name(format!(
            ".{}.tmp-{}",
            entry.name,
            std::process::id()
        ));
        {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&temporary)
                .await
                .map_err(|error| WebserverConfigError::Io(error.to_string()))?;
            file.write_all(content_bytes)
                .await
                .map_err(|error| WebserverConfigError::Io(error.to_string()))?;
            file.sync_all()
                .await
                .map_err(|error| WebserverConfigError::Io(error.to_string()))?;
        }
        tokio::fs::rename(&temporary, &absolute)
            .await
            .map_err(|error| {
                WebserverConfigError::Io(format!("atomic replace failed: {error}"))
            })?;

        let metadata = tokio::fs::metadata(&absolute)
            .await
            .map_err(|error| WebserverConfigError::Io(error.to_string()))?;
        let updated_at = unix_seconds(metadata.modified().ok()).unwrap_or_default();

        Ok(WebserverConfigWriteResult {
            id: entry.id,
            path: entry.path,
            size: content_bytes.len() as u64,
            sha256: sha256_hex(content_bytes),
            backup_path,
            updated_at,
        })
    }

    // ---- internals ----

    /// Resolve a catalog id through a fresh catalog lookup. Only files the
    /// catalog currently enumerates are addressable.
    async fn lookup(&self, id: &str) -> Result<WebserverConfigEntry, WebserverConfigError> {
        if id.is_empty() {
            return Err(WebserverConfigError::NotFound);
        }
        let catalog = self.catalog().await?;
        catalog
            .items
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or(WebserverConfigError::NotFound)
    }

    /// Containment-resolve an entry's absolute path (defense in depth; the
    /// path itself came from our own enumeration).
    fn absolute_path_for(&self, entry: &WebserverConfigEntry) -> Result<PathBuf, WebserverConfigError> {
        let root = match entry.kind {
            WebserverConfigKind::Default | WebserverConfigKind::Import => &self.config_root,
            WebserverConfigKind::Module => self
                .deploy_root
                .as_ref()
                .ok_or(WebserverConfigError::NotFound)?,
        };
        let resolved = resolve_contained_path(root, &entry.path)?;
        Ok(resolved)
    }

    /// Collect one group of entries from `directory` (files only, no
    /// symlinks, no dotfiles), recursing up to `max_depth` levels.
    async fn collect_group(
        &self,
        directory: &Path,
        max_depth: usize,
        kind: WebserverConfigKind,
        relative_prefix: &str,
        items: &mut Vec<WebserverConfigEntry>,
    ) -> Result<(), WebserverConfigError> {
        if items.len() >= self.config.maximum_entries {
            return Ok(());
        }
        if !directory.is_dir() {
            return Ok(());
        }
        let mut read_dir = match tokio::fs::read_dir(directory).await {
            Ok(read_dir) => read_dir,
            Err(_) => return Ok(()),
        };
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            if items.len() >= self.config.maximum_entries {
                return Ok(());
            }
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let relative = if relative_prefix.is_empty() {
                name.clone()
            } else {
                format!("{relative_prefix}/{name}")
            };
            if metadata.is_dir() {
                if max_depth > 0 {
                    Box::pin(self.collect_group(
                        &directory.join(&name),
                        max_depth - 1,
                        kind,
                        &relative,
                        items,
                    ))
                    .await?;
                }
                continue;
            }
            if !metadata.is_file() {
                continue; // skip symlinks and other non-regular entries
            }
            let language = config_language_for(items_path_name(&relative)).to_string();
            items.push(WebserverConfigEntry {
                id: config_entry_id(kind, &relative),
                kind,
                name,
                path: relative,
                language,
                size: Some(metadata.len()),
                updated_at: unix_seconds(metadata.modified().ok()),
                writable: true,
            });
        }
        Ok(())
    }

    /// Enumerate sibling-module sidecar configs under
    /// `<deploy_root>/sdkwork-space/<module>/deployments/webserver`.
    async fn collect_module_group(
        &self,
        deploy_root: &Path,
        items: &mut Vec<WebserverConfigEntry>,
    ) -> Result<(), WebserverConfigError> {
        let space_root = deploy_root.join(SDKWORK_SPACE_DIRECTORY);
        if !space_root.is_dir() {
            return Ok(());
        }
        let mut read_dir = match tokio::fs::read_dir(&space_root).await {
            Ok(read_dir) => read_dir,
            Err(_) => return Ok(()),
        };
        let mut scanned = 0usize;
        while let Ok(Some(module)) = read_dir.next_entry().await {
            if scanned >= MAX_MODULE_SCAN_ROOTS || items.len() >= self.config.maximum_entries {
                return Ok(());
            }
            let Ok(metadata) = module.metadata().await else {
                continue;
            };
            if !metadata.is_dir() {
                continue;
            }
            let module_name = module.file_name().to_string_lossy().into_owned();
            if module_name.starts_with('.') {
                continue;
            }
            scanned += 1;
            let sidecar_root = space_root
                .join(&module_name)
                .join(MODULE_CONFIG_RELATIVE_DIRECTORY);
            let prefix = format!(
                "{SDKWORK_SPACE_DIRECTORY}/{module_name}/{MODULE_CONFIG_RELATIVE_DIRECTORY}"
            );
            Box::pin(self.collect_group(
                &sidecar_root,
                MODULE_SCAN_DEPTH,
                WebserverConfigKind::Module,
                &prefix,
                items,
            ))
            .await?;
        }
        // Module sidecars are read-only regardless of what was collected.
        for item in items.iter_mut() {
            if item.kind == WebserverConfigKind::Module {
                item.writable = false;
            }
        }
        Ok(())
    }

    /// Create a timestamped backup of `target` and prune old backups.
    async fn create_backup(
        &self,
        target: &Path,
    ) -> Result<Option<String>, std::io::Error> {
        let Some(file_name) = target.file_name().and_then(|name| name.to_str()) else {
            return Ok(None);
        };
        let directory = target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let timestamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let backup = directory.join(format!("{file_name}.bak-{timestamp}"));
        if backup.exists() {
            // Same-second rewrite: disambiguate instead of clobbering.
            for suffix in 1..1000 {
                let candidate = directory.join(format!("{file_name}.bak-{timestamp}.{suffix}"));
                if !candidate.exists() {
                    return finalize_backup(target, candidate, file_name).await.map(Some);
                }
            }
            return Ok(None);
        }
        finalize_backup(target, backup, file_name).await.map(Some)
    }
}

async fn finalize_backup(
    target: &Path,
    backup: PathBuf,
    file_name: &str,
) -> Result<String, std::io::Error> {
    tokio::fs::copy(target, &backup).await?;
    prune_backups(
        target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        file_name,
    )
    .await;
    Ok(backup.to_string_lossy().into_owned())
}

/// Keep only the newest [`BACKUP_RETENTION`] `.bak-*` files for `file_name`.
async fn prune_backups(directory: PathBuf, file_name: &str) {
    let prefix = format!("{file_name}.bak-");
    let mut backups: Vec<(String, std::time::SystemTime)> = Vec::new();
    let Ok(mut read_dir) = tokio::fs::read_dir(&directory).await else {
        return;
    };
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) {
            backups.push((name, metadata.modified().unwrap_or(UNIX_EPOCH)));
        }
    }
    if backups.len() <= BACKUP_RETENTION {
        return;
    }
    backups.sort_by(|left, right| right.1.cmp(&left.1));
    for (name, _) in backups.into_iter().skip(BACKUP_RETENTION) {
        let _ = tokio::fs::remove_file(directory.join(name)).await;
    }
}

/// Syntax-validate content when a parser exists for the file type.
fn validate_content(file_name: &str, content: &str) -> Result<(), WebserverConfigError> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".json") {
        serde_json::from_str::<serde_json::Value>(content).map_err(|error| {
            WebserverConfigError::InvalidContent(format!("invalid JSON: {error}"))
        })?;
    } else if lower.ends_with(".toml") {
        toml::from_str::<toml::Value>(content).map_err(|error| {
            WebserverConfigError::InvalidContent(format!("invalid TOML: {error}"))
        })?;
    }
    Ok(())
}

/// Compute the stable catalog id for a group-relative path.
fn config_entry_id(kind: WebserverConfigKind, relative_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.identity().as_bytes());
    hasher.update([0x1f]);
    hasher.update(relative_path.as_bytes());
    hex::encode(hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

fn unix_seconds(system_time: Option<std::time::SystemTime>) -> Option<i64> {
    system_time?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs() as i64)
}

/// Last path segment of a relative catalog path (the file name).
fn items_path_name(relative_path: &str) -> &str {
    relative_path.rsplit('/').next().unwrap_or(relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.unwrap();
        }
        tokio::fs::write(path, content).await.unwrap();
    }

    fn service_for(root: &Path, deploy_root: Option<&Path>) -> WebserverConfigService {
        WebserverConfigService::new(WebserverConfigServiceConfig {
            config_root: root.to_string_lossy().into_owned(),
            deploy_root: deploy_root.map(|root| root.to_string_lossy().into_owned()),
            ..WebserverConfigServiceConfig::default()
        })
        .unwrap()
    }

    #[tokio::test]
    async fn catalog_groups_default_import_and_module_configs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("config.toml"), "[webserver]\n").await;
        write_file(&root.join("imports.d/import.conf"), "# active\n").await;
        write_file(&root.join("imports.d/import.conf.cloud"), "# cloud\n").await;
        let deploy = tempfile::tempdir().unwrap();
        write_file(
            &deploy
                .path()
                .join("sdkwork-space/some-module/deployments/webserver/nginx.standalone.development.conf"),
            "# sidecar\n",
        )
        .await;

        let service = service_for(root, Some(deploy.path()));
        let catalog = service.catalog().await.unwrap();

        let default: Vec<_> = catalog
            .items
            .iter()
            .filter(|entry| entry.kind == WebserverConfigKind::Default)
            .collect();
        let import: Vec<_> = catalog
            .items
            .iter()
            .filter(|entry| entry.kind == WebserverConfigKind::Import)
            .collect();
        let module: Vec<_> = catalog
            .items
            .iter()
            .filter(|entry| entry.kind == WebserverConfigKind::Module)
            .collect();

        assert_eq!(default.len(), 1);
        assert_eq!(default[0].name, "config.toml");
        assert_eq!(default[0].language, "ini");
        assert!(default[0].writable);
        assert_eq!(import.len(), 2);
        assert!(import.iter().any(|entry| entry.path == "imports.d/import.conf"));
        assert_eq!(module.len(), 1);
        assert_eq!(
            module[0].path,
            "sdkwork-space/some-module/deployments/webserver/nginx.standalone.development.conf"
        );
        // Module sidecars are read-only.
        assert!(!module[0].writable);
    }

    #[tokio::test]
    async fn read_returns_content_and_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("imports.d/import.conf"), "hello\n").await;
        let service = service_for(root, None);
        let catalog = service.catalog().await.unwrap();
        let entry = catalog
            .items
            .iter()
            .find(|entry| entry.path == "imports.d/import.conf")
            .unwrap()
            .clone();

        let file = service.read(&entry.id).await.unwrap();
        assert_eq!(file.content, "hello\n");
        assert_eq!(file.sha256, sha256_hex(b"hello\n"));
        assert_eq!(file.size, 6);
        assert!(file.writable);
    }

    #[tokio::test]
    async fn unknown_id_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let service = service_for(dir.path(), None);
        assert!(matches!(
            service.read("does-not-exist").await,
            Err(WebserverConfigError::NotFound)
        ));
    }

    #[tokio::test]
    async fn write_updates_atomically_and_creates_backup() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("imports.d/import.conf"), "one\n").await;
        let service = service_for(root, None);
        let catalog = service.catalog().await.unwrap();
        let entry = catalog
            .items
            .iter()
            .find(|entry| entry.path == "imports.d/import.conf")
            .unwrap()
            .clone();

        let result = service
            .write(&entry.id, "two\n", Some(&sha256_hex(b"one\n")))
            .await
            .unwrap();
        assert_eq!(result.sha256, sha256_hex(b"two\n"));
        let updated = service.read(&entry.id).await.unwrap();
        assert_eq!(updated.content, "two\n");

        let backup_path = result.backup_path.unwrap();
        let backup_name = Path::new(&backup_path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(backup_name.starts_with("import.conf.bak-"));
        let backup = tokio::fs::read(dir.path().join("imports.d").join(&backup_name))
            .await
            .unwrap();
        assert_eq!(backup, b"one\n");
        // No temporary files left behind.
        let leftovers = std::fs::read_dir(dir.path().join("imports.d"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[tokio::test]
    async fn write_conflicts_on_stale_expected_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("imports.d/import.conf"), "one\n").await;
        let service = service_for(root, None);
        let catalog = service.catalog().await.unwrap();
        let entry = catalog
            .items
            .iter()
            .find(|entry| entry.path == "imports.d/import.conf")
            .unwrap()
            .clone();

        match service
            .write(&entry.id, "two\n", Some("stale-digest"))
            .await
        {
            Err(WebserverConfigError::Conflict { current_sha256 }) => {
                assert_eq!(current_sha256, sha256_hex(b"one\n"));
            }
            other => panic!("expected conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_rejects_invalid_json_and_module_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("web.json"), "{}").await;
        let deploy = tempfile::tempdir().unwrap();
        write_file(
            &deploy
                .path()
                .join("sdkwork-space/mod/deployments/webserver/a.conf"),
            "# sidecar\n",
        )
        .await;
        let service = service_for(root, Some(deploy.path()));
        let catalog = service.catalog().await.unwrap();

        let json_entry = catalog
            .items
            .iter()
            .find(|entry| entry.name == "web.json")
            .unwrap()
            .clone();
        assert!(matches!(
            service.write(&json_entry.id, "{ not json }", None).await,
            Err(WebserverConfigError::InvalidContent(_))
        ));

        let module_entry = catalog
            .items
            .iter()
            .find(|entry| entry.kind == WebserverConfigKind::Module)
            .unwrap()
            .clone();
        assert!(matches!(
            service.write(&module_entry.id, "# rewritten\n", None).await,
            Err(WebserverConfigError::ReadOnlyEntry)
        ));
    }

    #[tokio::test]
    async fn symlinks_and_dotfiles_are_not_listed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("config.toml"), "[webserver]\n").await;
        write_file(&root.join(".hidden.toml"), "secret\n").await;
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", root.join("escape.toml")).unwrap();

        let service = service_for(root, None);
        let catalog = service.catalog().await.unwrap();
        let names: Vec<_> = catalog
            .items
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        assert_eq!(names, vec!["config.toml".to_string()]);
    }
}
