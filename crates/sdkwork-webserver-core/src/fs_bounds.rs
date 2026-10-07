//! Bounded text-file reads for every config-plane input that is fed from
//! operator- or module-owned files (nginx sidecars, includes, htpasswd,
//! snippets, runtime TOML). A stat cap plus a `take()` re-check keeps memory
//! bounded even when the file grows between the two observations, so a
//! multi-gigabyte file can never become an unbounded allocation on the
//! startup or reload path.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Per-file ceiling for the nginx-compatible import plane: sidecar configs,
/// include targets, htpasswd files, and snippets. Generous for real module
/// configurations while far below anything that could threaten memory.
pub(crate) const MAX_IMPORT_SOURCE_BYTES: u64 = 4 * 1024 * 1024;

/// Reads a UTF-8 text file, refusing inputs larger than `max_bytes`.
pub(crate) fn read_text_capped(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "file {} is {} bytes, exceeding the {} byte cap",
                path.display(),
                metadata.len(),
                max_bytes
            ),
        ));
    }
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes) as usize);
    file.by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "file {} grew past the {} byte cap while being read",
                path.display(),
                max_bytes
            ),
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("file {} is not valid UTF-8: {}", path.display(), error),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversize_file_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.conf");
        std::fs::write(&path, vec![b'a'; 128]).unwrap();
        let error = read_text_capped(&path, 64).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn reads_file_within_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ok.conf");
        std::fs::write(&path, "server { listen 80; }").unwrap();
        assert_eq!(
            read_text_capped(&path, 1024).unwrap(),
            "server { listen 80; }"
        );
    }
}
