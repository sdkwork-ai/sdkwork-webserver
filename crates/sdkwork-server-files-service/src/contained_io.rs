//! Component-wise no-follow opens for the contained read surface.
//!
//! `resolve_contained_path` canonicalizes and checks containment, but both
//! `browse_directory` and `read_file` used to re-open the canonical path by
//! name afterwards. A local process racing inside the authorized root could
//! swap a symlink into place between the check and the open and escape the
//! root. Everything here opens from the root directory one component at a
//! time with `O_NOFOLLOW` semantics (`Dir::open_dir_nofollow`,
//! `FollowSymlinks::No`), so the bytes read are the bytes of a path that was
//! symlink-free at every step — the same discipline the gateway's static
//! server applies (`static_path.rs`).

use std::ffi::OsString;

use cap_fs_ext::{DirExt, OpenOptionsFollowExt};
use cap_std::ambient_authority;

use super::path_security::{resolve_contained_target, PathContainmentError};

/// What `open_contained` found at the requested path, already open.
pub(crate) enum ContainedTarget {
    Directory(cap_std::fs::Dir),
    /// The file handle and the handle's own metadata (post-open, so the size
    /// check and the read observe the same inode).
    File(std::fs::File, std::fs::Metadata),
}

/// Resolves `requested` inside `root` (canonicalize + containment, as
/// everywhere else) and then opens it through per-component no-follow opens
/// from the root directory. Any symlink met along the way — which can only be
/// one swapped in after the canonicalization — is rejected as an escape.
pub(crate) fn open_contained(
    root: &std::path::Path,
    requested: &str,
) -> Result<ContainedTarget, PathContainmentError> {
    let (canonical_root, canonical_target) = resolve_contained_target(root, requested)?;
    let root_dir = cap_std::fs::Dir::open_ambient_dir(&canonical_root, ambient_authority())
        .map_err(|_| PathContainmentError::InvalidRoot)?;
    let relative = canonical_target
        .strip_prefix(&canonical_root)
        .map_err(|_| PathContainmentError::EscapesRoot)?;
    let components: Vec<OsString> = relative.iter().map(OsString::from).collect();
    let (last, parents) = match components.split_last() {
        Some((last, parents)) => (last, parents),
        // The request is the root itself.
        None => return Ok(ContainedTarget::Directory(root_dir)),
    };
    let mut directory = root_dir;
    for component in parents {
        let metadata = directory
            .symlink_metadata(&component)
            .map_err(|_| PathContainmentError::Unresolvable)?;
        if metadata.file_type().is_symlink() {
            return Err(PathContainmentError::EscapesRoot);
        }
        if !metadata.is_dir() {
            return Err(PathContainmentError::InvalidPath);
        }
        directory = directory
            .open_dir_nofollow(&component)
            .map_err(|_| PathContainmentError::Unresolvable)?;
    }
    let metadata = directory
        .symlink_metadata(&last)
        .map_err(|_| PathContainmentError::Unresolvable)?;
    if metadata.file_type().is_symlink() {
        return Err(PathContainmentError::EscapesRoot);
    }
    if metadata.is_dir() {
        let opened = directory
            .open_dir_nofollow(&last)
            .map_err(|_| PathContainmentError::Unresolvable)?;
        return Ok(ContainedTarget::Directory(opened));
    }
    if !metadata.is_file() {
        return Err(PathContainmentError::InvalidPath);
    }
    let mut options = cap_fs_ext::OpenOptions::new();
    options.read(true).follow(cap_fs_ext::FollowSymlinks::No);
    let file = directory
        .open_with(&last, &options)
        .map_err(|_| PathContainmentError::Unresolvable)?
        .into_std();
    let metadata = file
        .metadata()
        .map_err(|_| PathContainmentError::Unresolvable)?;
    Ok(ContainedTarget::File(file, metadata))
}

