//! The `ServerFilesService`: async directory browsing and file reading over a
//! contained root directory, plus project classification and operations.

use std::path::{Path, PathBuf};

use tokio::io::AsyncReadExt;

use super::operations::{operations_for, ProjectClassification};
use super::path_security::{
    is_sensitive_file_name, resolve_contained_path, validate_allowed_root, PathContainmentError,
};
use super::project::{classify_directory, ProjectType};

/// Configuration for a [`ServerFilesService`] instance bound to one node root.
#[derive(Debug, Clone)]
pub struct ServerFilesServiceConfig {
    /// The node id the service is bound to.
    pub node_id: String,
    /// The authorized filesystem root (for example `/opt/deploy`).
    pub filesystem_root: String,
    /// Maximum file content bytes readable through `read_file`.
    pub maximum_file_bytes: usize,
    /// Maximum directory entries returned per browse.
    pub maximum_entries: usize,
}

impl Default for ServerFilesServiceConfig {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            filesystem_root: "/opt/deploy".to_string(),
            maximum_file_bytes: 4 * 1024 * 1024, // 4 MiB
            maximum_entries: 4096,
        }
    }
}

/// A single directory entry returned by browse.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerEntry {
    pub name: String,
    pub kind: EntryKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_type: Option<ProjectType>,
    #[serde(default)]
    pub is_project_root: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Directory,
    File,
    Symlink,
}

/// A directory listing response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirectoryListing {
    pub node_id: String,
    pub path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<ServerEntry>,
}

/// A file content response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileContent {
    pub node_id: String,
    pub path: String,
    pub content: String,
    pub size: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum BrowseDirectoryError {
    #[error(transparent)]
    Containment(#[from] PathContainmentError),
    #[error("The directory could not be read: {0}")]
    Io(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ReadFileError {
    #[error(transparent)]
    Containment(#[from] PathContainmentError),
    #[error("The file is not a regular file")]
    NotAFile,
    #[error("The file exceeds the readable size limit")]
    TooLarge,
    #[error("The file could not be read: {0}")]
    Io(String),
    #[error("Credential and secret files cannot be read through the file explorer")]
    Sensitive,
}

/// Async, node-bound filesystem browsing service.
#[derive(Debug, Clone)]
pub struct ServerFilesService {
    config: ServerFilesServiceConfig,
    root: PathBuf,
}

impl ServerFilesService {
    /// Build a service for a node, validating the configured root eagerly.
    pub fn new(config: ServerFilesServiceConfig) -> Result<Self, PathContainmentError> {
        let root = validate_allowed_root(&config.filesystem_root)?;
        Ok(Self { config, root })
    }

    /// The validated, canonical filesystem root.
    pub fn filesystem_root(&self) -> &Path {
        &self.root
    }

    /// Resolve a requested path strictly inside the configured root.
    pub fn contained_path(&self, requested: &str) -> Result<PathBuf, PathContainmentError> {
        resolve_contained_path(&self.root, requested)
    }

    /// List a directory inside the root, classifying each child directory.
    pub async fn browse_directory(
        &self,
        requested_path: &str,
    ) -> Result<DirectoryListing, BrowseDirectoryError> {
        let resolved = self.contained_path(requested_path)?;
        let parent_path = resolved
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned());

        let mut read_dir = tokio::fs::read_dir(&resolved)
            .await
            .map_err(|error| BrowseDirectoryError::Io(error.to_string()))?;

        let mut entries = Vec::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|error| BrowseDirectoryError::Io(error.to_string()))?
        {
            if entries.len() >= self.config.maximum_entries {
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let metadata = match entry.metadata().await {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let kind = if metadata.file_type().is_symlink() {
                EntryKind::Symlink
            } else if metadata.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::File
            };
            let path_string = path.to_string_lossy().into_owned();
            let mut server_entry = ServerEntry {
                name,
                kind,
                path: path_string,
                size: metadata.is_file().then_some(metadata.len()),
                project_type: None,
                is_project_root: false,
            };
            if kind == EntryKind::Directory {
                if let Some(classification) = classify_directory(&path) {
                    server_entry.project_type = Some(classification.project_type);
                    server_entry.is_project_root = classification.is_project_root;
                }
            }
            entries.push(server_entry);
        }

        Ok(DirectoryListing {
            node_id: self.config.node_id.clone(),
            path: resolved.to_string_lossy().into_owned(),
            parent_path,
            entries,
        })
    }

    /// Read a text file inside the root, bounded by the configured size
    /// limit. The size check and the read share one open file handle, so a
    /// file that grows between the stat and the read cannot expand the read
    /// beyond the limit (bounded take), and credential-shaped file names
    /// (`.env`, private keys) are refused outright.
    pub async fn read_file(&self, requested_path: &str) -> Result<FileContent, ReadFileError> {
        let resolved = self.contained_path(requested_path)?;
        if is_sensitive_file_name(
            &resolved
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ) {
            return Err(ReadFileError::Sensitive);
        }
        let mut file = tokio::fs::File::open(&resolved)
            .await
            .map_err(|error| ReadFileError::Io(error.to_string()))?;
        let metadata = file
            .metadata()
            .await
            .map_err(|error| ReadFileError::Io(error.to_string()))?;
        if !metadata.is_file() {
            return Err(ReadFileError::NotAFile);
        }
        let maximum_bytes = self.config.maximum_file_bytes as u64;
        if metadata.len() > maximum_bytes {
            return Err(ReadFileError::TooLarge);
        }
        // Bound the read itself: a file grown past the limit between the
        // metadata probe and the read is truncated at the limit + 1 byte and
        // rejected instead of materialized.
        let mut limited = (&mut file).take(maximum_bytes.saturating_add(1));
        let mut bytes = Vec::new();
        limited
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| ReadFileError::Io(error.to_string()))?;
        if bytes.len() as u64 > maximum_bytes {
            return Err(ReadFileError::TooLarge);
        }
        let content = String::from_utf8_lossy(&bytes).into_owned();
        Ok(FileContent {
            node_id: self.config.node_id.clone(),
            path: resolved.to_string_lossy().into_owned(),
            content,
            size: bytes.len(),
        })
    }

    /// Compute the operation manifest for a project directory.
    pub fn operations_for(
        &self,
        requested_path: &str,
        classification: &ProjectClassification,
    ) -> Result<super::operations::ServerProjectOperations, PathContainmentError> {
        let resolved = self.contained_path(requested_path)?;
        Ok(operations_for(
            &self.config.node_id,
            &resolved.to_string_lossy(),
            classification,
        ))
    }
}

#[cfg(test)]
mod sensitive_name_tests {
    use super::super::path_security::is_sensitive_file_name;

    #[test]
    fn credential_shaped_names_are_refused() {
        for name in [
            ".env",
            ".env.local",
            ".env.production",
            "server.pem",
            "private.key",
            "identity.p12",
            "cert.pfx",
            "putty.ppk",
            "id_rsa",
            "id_ed25519.old",
            "vault.kdbx",
            "htpasswd",
            ".git-credentials",
            ".npmrc",
        ] {
            assert!(is_sensitive_file_name(name), "{name} must be refused");
        }
    }

    #[test]
    fn ordinary_names_are_readable() {
        for name in [
            "index.html",
            "app.js",
            "readme.md",
            "certificate.crt",
            "env.sh",
            "server.conf",
        ] {
            assert!(!is_sensitive_file_name(name), "{name} must be readable");
        }
    }
}
