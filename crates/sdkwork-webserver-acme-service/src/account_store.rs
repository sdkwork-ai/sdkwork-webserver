//! Durable ACME account credential storage.
//!
//! ACME account credentials (the account private key and URL) must be reused
//! across issuances. Creating a fresh account per operation consumes the CA's
//! account-creation rate limit (Let's Encrypt: 50 accounts per 3 hours per IP)
//! and loses account identity. The store persists encrypted credentials so
//! issuance and renewal restore the same account.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use instant_acme::AccountCredentials;
use sdkwork_utils_rust::{
    aes_gcm_decrypt, aes_gcm_encrypt, crypto::sha256_hash, derive_aes_256_key,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::{AcmeServiceError, AcmeServiceResult};

/// Maximum serialized account credential envelope accepted from disk (64 KiB).
const MAX_ACCOUNT_CREDENTIAL_FILE_BYTES: u64 = 64 * 1024;
/// Maximum serialized account credentials before encryption (8 KiB).
const MAX_ACCOUNT_CREDENTIAL_PLAINTEXT_BYTES: usize = 8 * 1024;
/// File name fingerprint length (hex chars).
const FINGERPRINT_LEN: usize = 16;

const ACCOUNT_KEY_SALT: &[u8] = b"sdkwork-web-acme-account";
const ACCOUNT_KEY_INFO: &[u8] = b"sdkwork-web-acme-account-key";

/// Loads and persists ACME account credentials for a directory URL.
///
/// Implementations must be safe under concurrent issuance. The store keys
/// credentials by the CA directory URL so distinct CAs never share accounts.
#[async_trait]
pub trait AcmeAccountStore: Send + Sync {
    /// Load persisted credentials for `directory_url`, or `None` when no
    /// account has been created for this CA yet.
    async fn load(&self, directory_url: &str) -> AcmeServiceResult<Option<AccountCredentials>>;

    /// Persist credentials for `directory_url`. Called once immediately after
    /// the account was created; subsequent operations restore via `load`.
    async fn save(
        &self,
        directory_url: &str,
        credentials: &AccountCredentials,
    ) -> AcmeServiceResult<()>;
}

/// Process-lifetime in-memory store used when no durable store is configured.
///
/// Guarantees that concurrent issuances in one process share one account
/// (no per-operation account creation) while remaining inert on disk.
#[derive(Default)]
pub struct MemoryAcmeAccountStore {
    accounts: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl AcmeAccountStore for MemoryAcmeAccountStore {
    async fn load(&self, directory_url: &str) -> AcmeServiceResult<Option<AccountCredentials>> {
        let encoded = self.accounts.lock().await.get(directory_url).cloned();
        match encoded {
            Some(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| {
                AcmeServiceError::Internal(format!(
                    "decode in-memory ACME account credentials: {error}"
                ))
            }),
            None => Ok(None),
        }
    }

    async fn save(
        &self,
        directory_url: &str,
        credentials: &AccountCredentials,
    ) -> AcmeServiceResult<()> {
        let encoded = serde_json::to_vec(credentials).map_err(|error| {
            AcmeServiceError::Internal(format!("serialize ACME account credentials: {error}"))
        })?;
        self.accounts
            .lock()
            .await
            .insert(directory_url.to_string(), encoded);
        Ok(())
    }
}

/// Encrypted, atomic, permission-confined file store.
///
/// Credentials are AES-256-GCM encrypted with a key derived from the process
/// secret master key (`SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY`) before they reach
/// disk. Files are written through a temporary sibling and renamed, so a
/// crash never leaves a truncated credential file. Unix permissions are
/// confined to the owner (`0600`).
pub struct EncryptedFileAcmeAccountStore {
    account_root: PathBuf,
    encryption_key: [u8; 32],
    serialize: Mutex<()>,
}

impl EncryptedFileAcmeAccountStore {
    /// Create a store rooted at `account_root` using `master_key` (the process
    /// secret master key) to derive the account-key encryption key.
    pub fn new(account_root: impl Into<PathBuf>, master_key: &[u8]) -> Self {
        Self {
            account_root: account_root.into(),
            encryption_key: derive_aes_256_key(master_key, ACCOUNT_KEY_SALT, ACCOUNT_KEY_INFO),
            serialize: Mutex::new(()),
        }
    }

    fn credential_path(&self, directory_url: &str) -> PathBuf {
        let fingerprint = sha256_hash(directory_url.as_bytes());
        self.account_root
            .join(format!("account-{}.json", &fingerprint[..FINGERPRINT_LEN]))
    }
}

#[async_trait]
impl AcmeAccountStore for EncryptedFileAcmeAccountStore {
    async fn load(&self, directory_url: &str) -> AcmeServiceResult<Option<AccountCredentials>> {
        let path = self.credential_path(directory_url);
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(AcmeServiceError::Internal(format!(
                    "inspect ACME account credential file {}: {error}",
                    path.display()
                )));
            }
        };
        if metadata.len() > MAX_ACCOUNT_CREDENTIAL_FILE_BYTES {
            return Err(AcmeServiceError::provider(format!(
                "ACME account credential file {} exceeds the size limit",
                path.display()
            )));
        }
        let encoded = tokio::fs::read(&path).await.map_err(|error| {
            AcmeServiceError::Internal(format!(
                "read ACME account credential file {}: {error}",
                path.display()
            ))
        })?;
        let encoded = String::from_utf8(encoded).map_err(|_| {
            AcmeServiceError::provider(format!(
                "ACME account credential file {} is not valid UTF-8",
                path.display()
            ))
        })?;
        let plaintext = aes_gcm_decrypt(&self.encryption_key, &encoded).map_err(|_| {
            AcmeServiceError::provider(format!(
                "ACME account credential file {} could not be decrypted; refusing to create a replacement account (restore the credential file or remove it after review)",
                path.display()
            ))
        })?;
        if plaintext.len() > MAX_ACCOUNT_CREDENTIAL_PLAINTEXT_BYTES {
            return Err(AcmeServiceError::provider(format!(
                "ACME account credential file {} exceeds the plaintext size limit",
                path.display()
            )));
        }
        let credentials = serde_json::from_slice(&plaintext).map_err(|_| {
            AcmeServiceError::provider(format!(
                "ACME account credential file {} is corrupted",
                path.display()
            ))
        })?;
        Ok(Some(credentials))
    }

    async fn save(
        &self,
        directory_url: &str,
        credentials: &AccountCredentials,
    ) -> AcmeServiceResult<()> {
        let plaintext = serde_json::to_vec(credentials).map_err(|error| {
            AcmeServiceError::Internal(format!("serialize ACME account credentials: {error}"))
        })?;
        if plaintext.len() > MAX_ACCOUNT_CREDENTIAL_PLAINTEXT_BYTES {
            return Err(AcmeServiceError::Internal(
                "ACME account credentials exceed the serialized size limit".to_string(),
            ));
        }
        let encoded = aes_gcm_encrypt(&self.encryption_key, &plaintext)
            .map_err(AcmeServiceError::Internal)?;
        let path = self.credential_path(directory_url);

        let _guard = self.serialize.lock().await;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                AcmeServiceError::Internal(format!(
                    "create ACME account credential directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        // Write through a temporary sibling and rename so a crash between
        // write and rename never leaves a truncated credential file.
        let temp_path = path.with_extension("json.tmp");
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp_path)
            .await
            .map_err(|error| {
                AcmeServiceError::Internal(format!(
                    "create ACME account credential file {}: {error}",
                    temp_path.display()
                ))
            })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = file.metadata().await.map_err(|error| {
                AcmeServiceError::Internal(format!(
                    "inspect ACME account credential file {}: {error}",
                    temp_path.display()
                ))
            })?;
            let mut perms = permissions.permissions();
            perms.set_mode(0o600);
            file.set_permissions(perms).await.map_err(|error| {
                AcmeServiceError::Internal(format!(
                    "confine ACME account credential file {}: {error}",
                    temp_path.display()
                ))
            })?;
        }
        file.write_all(encoded.as_bytes()).await.map_err(|error| {
            AcmeServiceError::Internal(format!(
                "write ACME account credential file {}: {error}",
                temp_path.display()
            ))
        })?;
        file.sync_all().await.map_err(|error| {
            AcmeServiceError::Internal(format!(
                "sync ACME account credential file {}: {error}",
                temp_path.display()
            ))
        })?;
        drop(file);
        tokio::fs::rename(&temp_path, &path)
            .await
            .map_err(|error| {
                AcmeServiceError::Internal(format!(
                    "rename ACME account credential file {}: {error}",
                    temp_path.display()
                ))
            })?;
        sync_directory(path.parent().unwrap_or(Path::new("."))).await;
        Ok(())
    }
}

async fn sync_directory(directory: &Path) {
    if let Ok(handle) = tokio::fs::File::open(directory).await {
        let _ = handle.sync_all().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> AccountCredentials {
        // AccountCredentials cannot be constructed directly; decode a real
        // P-256 PKCS#8 key serialized in the same URL-safe base64 shape
        // instant-acme writes, so the round trip exercises the true wire
        // format.
        serde_json::from_str(
            r#"{"id":"https://acme.example/acct/123","key_pkcs8":"MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgv9lP30vBg9q7t9OaAeqegk-K6tgSBuIYoPSQ3z10R2ahRANCAATGCh4HNWaAQJL23ZKVtLaoiKV0f5PS_4aqorKhZt0Z9xISbkNWkpKMlR_1q6qH93POYTrMGgSkemvFvLrhyGe3"}"#,
        )
        .expect("decode synthetic credentials")
    }

    #[tokio::test]
    async fn memory_store_round_trips_per_directory() {
        let store = MemoryAcmeAccountStore::default();
        assert!(store
            .load("https://acme.example/directory")
            .await
            .expect("load missing")
            .is_none());
        let expected = credentials();
        store
            .save("https://acme.example/directory", &expected)
            .await
            .expect("save");
        let loaded = store
            .load("https://acme.example/directory")
            .await
            .expect("load")
            .expect("account present");
        assert_eq!(
            serde_json::to_string(&loaded).expect("serialize loaded"),
            serde_json::to_string(&expected).expect("serialize expected")
        );
        // A different CA directory must never share the account.
        assert!(store
            .load("https://other.example/directory")
            .await
            .expect("load other")
            .is_none());
    }

    #[tokio::test]
    async fn encrypted_file_store_round_trips_atomically() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = EncryptedFileAcmeAccountStore::new(root.path(), b"test-master-key");
        assert!(store
            .load("https://acme.example/directory")
            .await
            .expect("load missing")
            .is_none());
        store
            .save("https://acme.example/directory", &credentials())
            .await
            .expect("save");
        let loaded = store
            .load("https://acme.example/directory")
            .await
            .expect("load")
            .expect("account present");
        assert_eq!(
            serde_json::to_string(&loaded).expect("serialize loaded"),
            serde_json::to_string(&credentials()).expect("serialize expected")
        );
        // Files on disk must be encrypted, never plaintext JSON.
        let directory = std::fs::read_dir(root.path())
            .expect("read dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("entries");
        assert_eq!(directory.len(), 1);
        let raw = std::fs::read(directory[0].path()).expect("read raw");
        let text = String::from_utf8_lossy(&raw);
        assert!(
            !text.contains("acme.example/acct"),
            "credentials must be encrypted at rest"
        );
    }

    #[tokio::test]
    async fn encrypted_file_store_isolates_directories_and_fails_closed_on_tamper() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = EncryptedFileAcmeAccountStore::new(root.path(), b"test-master-key");
        store
            .save("https://acme.example/directory", &credentials())
            .await
            .expect("save");

        // A different master key must not decrypt the stored account.
        let other = EncryptedFileAcmeAccountStore::new(root.path(), b"other-master-key");
        let error = match other.load("https://acme.example/directory").await {
            Ok(_) => panic!("wrong key must fail closed"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("could not be decrypted"));

        // A tampered file must fail closed instead of creating a new account.
        let path = store.credential_path("https://acme.example/directory");
        std::fs::write(&path, "tampered").expect("tamper");
        let error = match store.load("https://acme.example/directory").await {
            Ok(_) => panic!("tampered file must fail closed"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("could not be decrypted"));
    }
}
