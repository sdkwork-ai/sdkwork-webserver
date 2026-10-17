use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use crate::{AcmeServiceError, AcmeServiceResult};

const CHALLENGE_DIR: &str = ".well-known/acme-challenge";
const MAX_ACTIVE_CHALLENGES: usize = 64;
const MAX_TOKEN_BYTES: usize = 256;
const MAX_KEY_AUTH_BYTES: usize = 2_048;

struct ChallengeEntry {
    key_auth: String,
    generation: u64,
    ready: bool,
}

/// Bounded HTTP-01 challenge token store with optional atomic webroot persistence.
pub struct ChallengeStore {
    tokens: RwLock<HashMap<String, ChallengeEntry>>,
    next_generation: AtomicU64,
}

impl Default for ChallengeStore {
    fn default() -> Self {
        Self {
            tokens: RwLock::new(HashMap::new()),
            next_generation: AtomicU64::new(1),
        }
    }
}

impl ChallengeStore {
    /// Compatibility API for callers that explicitly clear the token later.
    pub fn register(
        &self,
        webroot: Option<&Path>,
        token: &str,
        key_auth: &str,
    ) -> AcmeServiceResult<()> {
        let generation = self.reserve(token, key_auth)?;
        if let Err(error) = write_challenge_file(webroot, token, key_auth) {
            self.clear_generation(token, generation);
            return Err(error);
        }
        self.publish_or_cleanup(webroot, token, generation)
    }

    pub(crate) async fn register_scoped<'a>(
        &'a self,
        webroot: Option<&Path>,
        token: &str,
        key_auth: &str,
    ) -> AcmeServiceResult<ChallengeLease<'a>> {
        let generation = self.reserve(token, key_auth)?;
        let lease = ChallengeLease {
            store: self,
            webroot: webroot.map(Path::to_path_buf),
            token: token.to_string(),
            generation,
        };
        write_challenge_file_async(webroot, token, key_auth).await?;
        self.publish_or_cleanup(webroot, token, generation)?;
        Ok(lease)
    }

    pub fn lookup(&self, token: &str) -> Option<String> {
        if validate_token(token).is_err() {
            return None;
        }
        self.tokens.read().ok().and_then(|guard| {
            guard
                .get(token)
                .filter(|entry| entry.ready)
                .map(|entry| entry.key_auth.clone())
        })
    }

    pub fn clear_token(&self, webroot: Option<&Path>, token: &str) {
        if validate_token(token).is_err() {
            return;
        }
        if let Ok(mut guard) = self.tokens.write() {
            guard.remove(token);
            remove_challenge_file(challenge_path(webroot, token).as_deref());
        }
    }

    pub fn challenge_dir(webroot: &Path) -> PathBuf {
        webroot.join(CHALLENGE_DIR)
    }

    fn reserve(&self, token: &str, key_auth: &str) -> AcmeServiceResult<u64> {
        validate_challenge(token, key_auth)?;
        let mut guard = self
            .tokens
            .write()
            .map_err(|_| AcmeServiceError::Internal("challenge store lock poisoned".to_string()))?;
        if guard.contains_key(token) {
            return Err(AcmeServiceError::validation(
                "duplicate active ACME challenge token",
            ));
        }
        if guard.len() >= MAX_ACTIVE_CHALLENGES {
            return Err(AcmeServiceError::validation(format!(
                "active ACME challenges exceed maximum {MAX_ACTIVE_CHALLENGES}"
            )));
        }
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        guard.insert(
            token.to_string(),
            ChallengeEntry {
                key_auth: key_auth.to_string(),
                generation,
                ready: false,
            },
        );
        Ok(generation)
    }

    fn publish_or_cleanup(
        &self,
        webroot: Option<&Path>,
        token: &str,
        generation: u64,
    ) -> AcmeServiceResult<()> {
        let mut guard = self
            .tokens
            .write()
            .map_err(|_| AcmeServiceError::Internal("challenge store lock poisoned".to_string()))?;
        let published = guard
            .get_mut(token)
            .filter(|entry| entry.generation == generation)
            .map(|entry| entry.ready = true)
            .is_some();
        if !published {
            remove_challenge_file(challenge_path(webroot, token).as_deref());
            return Err(AcmeServiceError::Internal(
                "ACME challenge registration was cancelled before publication".to_string(),
            ));
        }
        Ok(())
    }

    fn clear_generation(&self, token: &str, generation: u64) {
        if let Ok(mut guard) = self.tokens.write() {
            if guard
                .get(token)
                .is_some_and(|entry| entry.generation == generation)
            {
                guard.remove(token);
            }
        }
    }

    fn clear_generation_and_file(&self, webroot: Option<&Path>, token: &str, generation: u64) {
        if let Ok(mut guard) = self.tokens.write() {
            if guard
                .get(token)
                .is_some_and(|entry| entry.generation == generation)
            {
                guard.remove(token);
                remove_challenge_file(challenge_path(webroot, token).as_deref());
            }
        }
    }
}

pub(crate) struct ChallengeLease<'a> {
    store: &'a ChallengeStore,
    webroot: Option<PathBuf>,
    token: String,
    generation: u64,
}

impl Drop for ChallengeLease<'_> {
    fn drop(&mut self) {
        self.store
            .clear_generation_and_file(self.webroot.as_deref(), &self.token, self.generation);
    }
}

fn validate_challenge(token: &str, key_auth: &str) -> AcmeServiceResult<()> {
    validate_token(token)?;
    if key_auth.is_empty()
        || key_auth.len() > MAX_KEY_AUTH_BYTES
        || !key_auth
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AcmeServiceError::validation(
            "ACME key authorization must contain 1..2048 base64url/dot bytes",
        ));
    }
    Ok(())
}

fn validate_token(token: &str) -> AcmeServiceResult<()> {
    if token.is_empty()
        || token.len() > MAX_TOKEN_BYTES
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AcmeServiceError::validation(
            "ACME token must contain 1..256 base64url bytes",
        ));
    }
    Ok(())
}

fn write_challenge_file(
    webroot: Option<&Path>,
    token: &str,
    key_auth: &str,
) -> AcmeServiceResult<()> {
    let Some(root) = webroot else {
        return Ok(());
    };
    let challenge_dir = ChallengeStore::challenge_dir(root);
    std::fs::create_dir_all(&challenge_dir)
        .map_err(|error| AcmeServiceError::Internal(format!("create ACME webroot: {error}")))?;
    let target = challenge_dir.join(token);
    let mut staged = NamedTempFile::new_in(&challenge_dir)
        .map_err(|error| AcmeServiceError::Internal(format!("stage ACME challenge: {error}")))?;
    staged
        .write_all(key_auth.as_bytes())
        .and_then(|_| staged.flush())
        .and_then(|_| staged.as_file().sync_all())
        .map_err(|error| AcmeServiceError::Internal(format!("write ACME challenge: {error}")))?;
    #[cfg(unix)]
    {
        // Challenge responses are public by design (the CA fetches them
        // anonymously over HTTP-01), so they must be world-readable or a
        // serving worker running as a different user fails validation.
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .map_err(|error| {
                AcmeServiceError::Internal(format!("set ACME challenge permissions: {error}"))
            })?;
    }
    staged.persist(&target).map_err(|error| {
        AcmeServiceError::Internal(format!("activate ACME challenge: {}", error.error))
    })?;
    Ok(())
}

async fn write_challenge_file_async(
    webroot: Option<&Path>,
    token: &str,
    key_auth: &str,
) -> AcmeServiceResult<()> {
    let Some(root) = webroot else {
        return Ok(());
    };
    let challenge_dir = ChallengeStore::challenge_dir(root);
    tokio::fs::create_dir_all(&challenge_dir)
        .await
        .map_err(|error| AcmeServiceError::Internal(format!("create ACME webroot: {error}")))?;
    let target = challenge_dir.join(token);
    let staged = NamedTempFile::new_in(&challenge_dir)
        .map_err(|error| AcmeServiceError::Internal(format!("stage ACME challenge: {error}")))?;
    let (file, staged_path) = staged.into_parts();
    let mut file = tokio::fs::File::from_std(file);
    file.write_all(key_auth.as_bytes())
        .await
        .map_err(|error| AcmeServiceError::Internal(format!("write ACME challenge: {error}")))?;
    file.flush()
        .await
        .map_err(|error| AcmeServiceError::Internal(format!("flush ACME challenge: {error}")))?;
    file.sync_all()
        .await
        .map_err(|error| AcmeServiceError::Internal(format!("sync ACME challenge: {error}")))?;
    #[cfg(unix)]
    {
        // World-readable, same rationale as the synchronous path.
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o644))
            .await
            .map_err(|error| {
                AcmeServiceError::Internal(format!("set ACME challenge permissions: {error}"))
            })?;
    }
    drop(file);
    staged_path.persist(&target).map_err(|error| {
        AcmeServiceError::Internal(format!("activate ACME challenge: {}", error.error))
    })?;
    Ok(())
}

fn challenge_path(webroot: Option<&Path>, token: &str) -> Option<PathBuf> {
    webroot.map(|root| ChallengeStore::challenge_dir(root).join(token))
}

fn remove_challenge_file(path: Option<&Path>) {
    if let Some(path) = path {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to remove ACME challenge file"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn register_and_lookup_in_memory() {
        let store = ChallengeStore::default();
        store
            .register(None, "token-a", "token-a.thumb")
            .expect("register");
        assert_eq!(store.lookup("token-a").as_deref(), Some("token-a.thumb"));
        assert!(store
            .register(None, "token-a", "replacement.thumb")
            .is_err());
    }

    #[tokio::test]
    async fn scoped_registration_cleans_memory_and_file() {
        let root = TempDir::new().expect("tempdir");
        let store = ChallengeStore::default();
        let path = ChallengeStore::challenge_dir(root.path()).join("token-a");
        {
            let _lease = store
                .register_scoped(Some(root.path()), "token-a", "token-a.thumb")
                .await
                .expect("register");
            assert_eq!(store.lookup("token-a").as_deref(), Some("token-a.thumb"));
            assert!(path.is_file());
        }
        assert!(store.lookup("token-a").is_none());
        assert!(!path.exists());
    }

    #[test]
    fn rejects_path_traversal_and_bounds_active_entries() {
        let store = ChallengeStore::default();
        assert!(store.register(None, "../escape", "safe.thumb").is_err());
        for index in 0..MAX_ACTIVE_CHALLENGES {
            let token = format!("token-{index}");
            store
                .register(None, &token, &format!("{token}.thumb"))
                .expect("bounded registration");
        }
        assert!(store
            .register(None, "one-too-many", "one-too-many.thumb")
            .is_err());
    }

    #[test]
    fn failed_file_write_does_not_leave_memory_entry() {
        let root = TempDir::new().expect("tempdir");
        let blocking_file = root.path().join("blocking-file");
        std::fs::write(&blocking_file, "not a directory").expect("write blocker");
        let store = ChallengeStore::default();
        assert!(store
            .register(Some(&blocking_file), "token-a", "token-a.thumb")
            .is_err());
        assert!(store.lookup("token-a").is_none());
    }
}
