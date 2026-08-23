//! Path containment security for server filesystem browsing.
//!
//! The whole feature is only safe if a caller can never reach a file or
//! directory outside the node's authorized root. This module enforces that:
//!
//! 1. The requested path is canonicalized (symlinks and `..` resolved).
//! 2. The canonical path must lexically and physically start with the
//!    canonical root, otherwise the request is rejected as a traversal.
//! 3. The root itself is validated to be an absolute path (or explicit
//!    virtual root such as `/` for the host gateway).
//!
//! All operations are cross-platform and avoid shell interpretation.

use std::path::{Component, Path, PathBuf};

/// Error returned when a requested path escapes the authorized root or is
/// otherwise invalid. Messages deliberately avoid echoing the raw requested
/// path to prevent path-injection / log-poisoning.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathContainmentError {
    #[error("The requested path is outside the authorized directory")]
    EscapesRoot,
    #[error("The requested path is empty or invalid")]
    InvalidPath,
    #[error("The configured filesystem root is invalid")]
    InvalidRoot,
    #[error("The path could not be resolved")]
    Unresolvable,
}

/// Validate that an allowed root is usable as a containment anchor.
pub fn validate_allowed_root(root: &str) -> Result<PathBuf, PathContainmentError> {
    if root.is_empty() || root == "." {
        return Err(PathContainmentError::InvalidRoot);
    }
    let mut path = PathBuf::from(root);
    // `/` and Windows drive roots (`C:\`) are legitimate deployment roots.
    let is_filesystem_root = is_fs_root(&path);
    if !is_filesystem_root {
        path = path
            .canonicalize()
            .map_err(|_| PathContainmentError::InvalidRoot)?;
    }
    Ok(path)
}

/// Resolve `requested` strictly inside `root`.
///
/// - `root` must have been validated by [`validate_allowed_root`] or be a
///   filesystem root.
/// - The requested path may be absolute or relative to the root.
/// - The canonical result is returned; `None` components are skipped and
///   `..` collapses in the canonical form. If the collapse would climb above
///   the root, the request is rejected.
pub fn resolve_contained_path(
    root: &Path,
    requested: &str,
) -> Result<PathBuf, PathContainmentError> {
    if requested.is_empty() {
        return Err(PathContainmentError::InvalidPath);
    }
    let root_is_fs_root = is_fs_root(root);

    // Canonicalize the root reference so containment compares canonical
    // against canonical (this also normalizes Windows `\\?\` prefixes and
    // case aliasing). If the root itself does not exist yet, canonicalize its
    // deepest existing ancestor.
    let root_reference = if root_is_fs_root {
        root.to_path_buf()
    } else {
        canonicalize_or_ancestor(root).map_err(|_| PathContainmentError::InvalidRoot)?
    };

    // Build the target as root.join(requested) when relative, or the absolute
    // requested path when the caller already provided a rooted path.
    let requested_path = Path::new(requested);
    let mut candidate: PathBuf = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        root_reference.join(requested_path)
    };

    // Collapse `.` and `..` lexically first so we can reject traversal without
    // touching the disk (fast, deterministic, and safe even before the root
    // directory exists).
    candidate = lexically_normalize(&candidate)
        .ok_or(PathContainmentError::InvalidPath)?;

    if !path_within(&candidate, &root_reference, root_is_fs_root) {
        return Err(PathContainmentError::EscapesRoot);
    }

    // Canonicalize against the live filesystem to defeat symlink escapes and
    // case/alias tricks. If the candidate doesn't exist yet, canonicalize the
    // deepest existing ancestor and re-append the remainder lexically.
    let canonical = canonicalize_or_ancestor(&candidate)
        .map_err(|_| PathContainmentError::Unresolvable)?;

    if !path_within(&canonical, &root_reference, root_is_fs_root) {
        return Err(PathContainmentError::EscapesRoot);
    }

    Ok(canonical)
}

fn is_fs_root(path: &Path) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    let components: Vec<Component> = path.components().collect();
    matches!(components.as_slice(), [Component::RootDir])
        // Windows drive root, e.g. `C:\`
        || (components.len() == 1 && matches!(components[0], Component::Prefix(_)))
}

/// Remove `.` and resolve `..` lexically; returns `None` if the result would
/// climb above the filesystem root (`/` or a drive root).
fn lexically_normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    // Climbing above the root.
                    return None;
                }
                depth -= 1;
                out.pop();
            }
            other => {
                depth += 1;
                out.push(other.as_os_str());
            }
        }
    }
    if out.as_os_str().is_empty() {
        out.push(Component::RootDir.as_os_str());
    }
    Some(out)
}

fn path_within(candidate: &Path, root: &Path, root_is_fs_root: bool) -> bool {
    if root_is_fs_root {
        // The root is `/` or a drive root; any absolute path is inside.
        return candidate.is_absolute();
    }
    candidate
        .components()
        .zip(root.components())
        .all(|(left, right)| left == right)
        && candidate.components().count() >= root.components().count()
}

/// Canonicalize `path`, falling back to the deepest existing ancestor so that
/// browsing into a not-yet-created directory remains safe and deterministic.
fn canonicalize_or_ancestor(path: &Path) -> std::io::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(canonical) => Ok(canonical),
        Err(_) => {
            let mut ancestor = path.to_path_buf();
            let mut tail: Vec<PathBuf> = Vec::new();
            loop {
                match std::fs::canonicalize(&ancestor) {
                    Ok(mut canonical) => {
                        for part in tail.iter().rev() {
                            canonical.push(part);
                        }
                        return Ok(canonical);
                    }
                    Err(_) => {
                        if let Some(name) = ancestor.file_name() {
                            tail.push(PathBuf::from(name));
                            ancestor.pop();
                        } else {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "no canonical ancestor",
                            ));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolves_relative_within_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let canonical_root = canonicalize_or_ancestor(root).unwrap();
        let resolved = resolve_contained_path(root, "sdkwork-space/apps").unwrap();
        assert!(resolved.starts_with(&canonical_root));
        assert!(resolved.ends_with("sdkwork-space/apps"));
    }

    #[test]
    fn rejects_parent_traversal() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        assert_eq!(
            resolve_contained_path(root, "../../etc/passwd").unwrap_err(),
            PathContainmentError::EscapesRoot
        );
        assert_eq!(
            resolve_contained_path(root, "sdkwork-space/../../..").unwrap_err(),
            PathContainmentError::EscapesRoot
        );
    }

    #[test]
    fn rejects_absolute_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let outside = dir.path().parent().unwrap().parent().unwrap();
        let escaped = format!("{}\\x", outside.to_string_lossy());
        assert_eq!(
            resolve_contained_path(root, &escaped).unwrap_err(),
            PathContainmentError::EscapesRoot
        );
    }

    #[test]
    #[cfg(unix)]
    fn root_can_be_fs_root() {
        let root = Path::new("/");
        let resolved = resolve_contained_path(root, "/opt/deploy").unwrap();
        assert!(resolved.starts_with("/opt/deploy"));
    }

    #[test]
    #[cfg(unix)]
    fn symlink_escape_is_rejected() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Create a symlink pointing outside the root.
        let outside = dir.path().join("outside");
        let link = root.join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert_eq!(
            resolve_contained_path(root, "link").unwrap_err(),
            PathContainmentError::EscapesRoot
        );
    }
}
