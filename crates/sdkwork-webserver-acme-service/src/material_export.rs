//! Disk export of issued certificate material.
//!
//! `IssuedCertificateMaterial::cert_path` / `key_path` point into the
//! canonical ACME live root (`<cert_root>/<cert_name>/…`). Single-host edges
//! (deb/rpm installers) terminate TLS directly from those paths, so the
//! export is what makes a freshly issued or renewed certificate reachable:
//! writes are atomic (temp file + rename) and the private key lands 0600.
//! Renewals overwrite the same per-certificate directory in place, so an
//! edge that reads the canonical paths picks the new material up without
//! any manual copy step.
//!
//! A failed export is never fatal: the material is durable in the database
//! (encrypted secret columns) and the caller logs the stale-path warning.

use std::path::Path;

use crate::model::IssuedCertificateMaterial;

/// Exports `material` to its declared `cert_path` / `key_path` atomically.
/// The private key's 0600 permission is best-effort: on failure the key file
/// still exists with default permissions, so the caller logs the export
/// error rather than swallowing it silently.
pub async fn export_material_to_disk(material: &IssuedCertificateMaterial) -> Result<(), String> {
    write_atomic(
        Path::new(&material.cert_path),
        material.cert_pem.as_bytes(),
        0o644,
    )
    .await?;
    write_atomic(
        Path::new(&material.key_path),
        material.private_key_pem.as_bytes(),
        0o600,
    )
    .await
}

async fn write_atomic(
    path: &Path,
    bytes: &[u8],
    #[cfg_attr(not(unix), allow(unused_variables))] mode: u32,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::File::create(&temporary)
            .await
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        file.write_all(bytes)
            .await
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        file.sync_all()
            .await
            .map_err(|error| format!("sync {}: {error}", temporary.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(mode))
                .await
                .map_err(|error| format!("chmod {}: {error}", temporary.display()))?;
        }
    }
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(format!("rename {}: {error}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_material(root: &Path) -> IssuedCertificateMaterial {
        let cert_dir = root.join("server.example.com");
        IssuedCertificateMaterial {
            cert_name: "server.example.com".to_owned(),
            cert_type: 1,
            issuer: "test issuer".to_owned(),
            subject: "server.example.com".to_owned(),
            san_list: vec!["server.example.com".to_owned()],
            serial_sha256: "serial".to_owned(),
            fingerprint_sha256: "fingerprint".to_owned(),
            spki_sha256: "spki".to_owned(),
            chain_sha256: "chain".to_owned(),
            key_algorithm: "ECDSA P-256".to_owned(),
            cert_pem: "-----BEGIN CERTIFICATE-----\nchain\n-----END CERTIFICATE-----\n".to_owned(),
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nkey\n-----END PRIVATE KEY-----\n"
                .to_owned(),
            chain_pem: None,
            not_before: "2026-10-08T00:00:00Z".to_owned(),
            not_after: "2027-01-06T00:00:00Z".to_owned(),
            cert_path: cert_dir
                .join("fullchain.pem")
                .to_string_lossy()
                .into_owned(),
            key_path: cert_dir.join("privkey.pem").to_string_lossy().into_owned(),
            chain_path: None,
        }
    }

    #[tokio::test]
    async fn exports_material_atomically_with_private_key_permissions() {
        let root = tempfile::tempdir().unwrap();
        let material = sample_material(root.path());
        export_material_to_disk(&material).await.unwrap();
        let cert = std::fs::read(&material.cert_path).unwrap();
        let key = std::fs::read(&material.key_path).unwrap();
        assert_eq!(cert, material.cert_pem.as_bytes());
        assert_eq!(key, material.private_key_pem.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let key_mode = std::fs::metadata(&material.key_path)
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(key_mode & 0o777, 0o600);
        }
        // No staging residue.
        let residue: Vec<_> = std::fs::read_dir(root.path().join("server.example.com"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(residue.is_empty(), "staging files must be renamed away");
    }

    #[tokio::test]
    async fn renewal_overwrites_the_same_paths_in_place() {
        let root = tempfile::tempdir().unwrap();
        let mut material = sample_material(root.path());
        export_material_to_disk(&material).await.unwrap();
        material.cert_pem = "renewed-chain".to_owned();
        material.private_key_pem = "renewed-key".to_owned();
        export_material_to_disk(&material).await.unwrap();
        assert_eq!(
            std::fs::read(&material.cert_path).unwrap(),
            b"renewed-chain"
        );
        assert_eq!(std::fs::read(&material.key_path).unwrap(), b"renewed-key");
    }
}
