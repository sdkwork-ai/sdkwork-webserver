use std::{
    io,
    path::{Component, Path, PathBuf},
};

use cap_fs_ext::{
    ambient_authority, DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsMaybeDirExt,
};
use cap_std::fs::{Dir, OpenOptions};

pub(crate) struct OpenedStaticFile {
    pub(crate) file: std::fs::File,
    pub(crate) metadata: std::fs::Metadata,
    pub(crate) path_hint: PathBuf,
}

pub(crate) enum StaticPathTarget {
    File(OpenedStaticFile),
    RedirectToDirectory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StaticPathError {
    Invalid,
    Forbidden,
    NotFound,
    Io,
}

enum OpenedEntry {
    File(OpenedStaticFile),
    Directory(Dir),
}

pub(crate) async fn open_static_path(
    root: &Path,
    relative: &str,
    request_path_has_trailing_slash: bool,
    spa_fallback: Option<&str>,
) -> Result<StaticPathTarget, StaticPathError> {
    let root = root.to_owned();
    let relative = relative.to_owned();
    let spa_fallback = spa_fallback.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        open_static_path_sync(
            &root,
            &relative,
            request_path_has_trailing_slash,
            spa_fallback.as_deref(),
        )
    })
    .await
    .map_err(|_| StaticPathError::Io)?
}

fn open_static_path_sync(
    root: &Path,
    relative: &str,
    request_path_has_trailing_slash: bool,
    spa_fallback: Option<&str>,
) -> Result<StaticPathTarget, StaticPathError> {
    let components = validated_components(relative)?;
    let root_dir = Dir::open_ambient_dir(root, ambient_authority()).map_err(map_root_error)?;

    match open_relative_entry(root_dir, &components, Path::new(relative)) {
        Ok(OpenedEntry::File(file)) => Ok(StaticPathTarget::File(file)),
        Ok(OpenedEntry::Directory(_directory)) if !request_path_has_trailing_slash => {
            Ok(StaticPathTarget::RedirectToDirectory)
        }
        Ok(OpenedEntry::Directory(directory)) => match open_index(directory, relative) {
            Ok(file) => Ok(StaticPathTarget::File(file)),
            Err(StaticPathError::NotFound) => open_fallback(root, spa_fallback),
            Err(error) => Err(error),
        },
        Err(StaticPathError::NotFound) => open_fallback(root, spa_fallback),
        Err(error) => Err(error),
    }
}

fn open_fallback(
    root: &Path,
    spa_fallback: Option<&str>,
) -> Result<StaticPathTarget, StaticPathError> {
    let Some(fallback) = spa_fallback else {
        return Err(StaticPathError::NotFound);
    };
    let components = validated_components(fallback)?;
    let root_dir = Dir::open_ambient_dir(root, ambient_authority()).map_err(map_root_error)?;
    match open_relative_entry(root_dir, &components, Path::new(fallback))? {
        OpenedEntry::File(file) => Ok(StaticPathTarget::File(file)),
        OpenedEntry::Directory(_) => Err(StaticPathError::NotFound),
    }
}

/// Opens the ACME HTTP-01 challenge file for `token` under the webroot.
///
/// The path is fixed to `<webroot>/.well-known/acme-challenge/<token>` and
/// every component is opened without following symlinks, so the challenge
/// never escapes the webroot and never exposes a directory or another file.
/// `token` must already satisfy the strict ACME token character set.
pub(crate) async fn open_challenge_file(
    root: &Path,
    token: &str,
) -> Result<OpenedStaticFile, StaticPathError> {
    let root = root.to_owned();
    let token = token.to_owned();
    tokio::task::spawn_blocking(move || {
        let root_dir = Dir::open_ambient_dir(&root, ambient_authority()).map_err(map_root_error)?;
        let well_known = open_directory_component(root_dir, Path::new(".well-known"))?;
        let challenge = open_directory_component(well_known, Path::new("acme-challenge"))?;
        match open_entry(&challenge, Path::new(&token), Path::new(&token))? {
            OpenedEntry::File(file) => Ok(file),
            OpenedEntry::Directory(_) => Err(StaticPathError::NotFound),
        }
    })
    .await
    .map_err(|_| StaticPathError::Io)?
}

fn open_relative_entry(
    mut directory: Dir,
    components: &[PathBuf],
    path_hint: &Path,
) -> Result<OpenedEntry, StaticPathError> {
    let Some((last, parents)) = components.split_last() else {
        return Ok(OpenedEntry::Directory(directory));
    };
    for component in parents {
        directory = open_directory_component(directory, component)?;
    }
    open_entry(&directory, last, path_hint)
}

fn open_directory_component(directory: Dir, component: &Path) -> Result<Dir, StaticPathError> {
    let metadata = directory
        .symlink_metadata(component)
        .map_err(map_entry_error)?;
    if metadata.file_type().is_symlink() {
        return Err(StaticPathError::Forbidden);
    }
    if !metadata.is_dir() {
        return Err(StaticPathError::NotFound);
    }
    directory
        .open_dir_nofollow(component)
        .map_err(map_entry_error)
}

fn open_entry(
    directory: &Dir,
    component: &Path,
    path_hint: &Path,
) -> Result<OpenedEntry, StaticPathError> {
    let metadata = directory
        .symlink_metadata(component)
        .map_err(map_entry_error)?;
    if metadata.file_type().is_symlink() {
        return Err(StaticPathError::Forbidden);
    }
    if metadata.is_dir() {
        return directory
            .open_dir_nofollow(component)
            .map(OpenedEntry::Directory)
            .map_err(map_entry_error);
    }
    if !metadata.is_file() {
        return Err(StaticPathError::Forbidden);
    }

    let mut options = OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .maybe_dir(false);
    let file = directory
        .open_with(component, &options)
        .map_err(map_entry_error)?
        .into_std();
    let metadata = file.metadata().map_err(|_| StaticPathError::Io)?;
    if !metadata.is_file() {
        return Err(StaticPathError::Forbidden);
    }
    Ok(OpenedEntry::File(OpenedStaticFile {
        file,
        metadata,
        path_hint: path_hint.to_owned(),
    }))
}

fn open_index(directory: Dir, parent_hint: &str) -> Result<OpenedStaticFile, StaticPathError> {
    let path_hint = Path::new(parent_hint).join("index.html");
    match open_entry(&directory, Path::new("index.html"), &path_hint)? {
        OpenedEntry::File(file) => Ok(file),
        OpenedEntry::Directory(_) => Err(StaticPathError::NotFound),
    }
}

fn validated_components(path: &str) -> Result<Vec<PathBuf>, StaticPathError> {
    if path.contains('\\') || path.contains('\0') {
        return Err(StaticPathError::Invalid);
    }
    path.trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(|segment| {
            let component = Path::new(segment);
            match component.components().next() {
                Some(Component::Normal(_)) if component.components().count() == 1 => {
                    Ok(component.to_owned())
                }
                _ => Err(StaticPathError::Forbidden),
            }
        })
        .collect()
}

fn map_root_error(error: io::Error) -> StaticPathError {
    match error.kind() {
        io::ErrorKind::NotFound => StaticPathError::NotFound,
        io::ErrorKind::PermissionDenied => StaticPathError::Forbidden,
        _ => StaticPathError::Io,
    }
}

fn map_entry_error(error: io::Error) -> StaticPathError {
    if error.raw_os_error() == Some(40) {
        return StaticPathError::Forbidden;
    }
    match error.kind() {
        io::ErrorKind::NotFound => StaticPathError::NotFound,
        io::ErrorKind::PermissionDenied => StaticPathError::Forbidden,
        io::ErrorKind::InvalidInput => StaticPathError::Invalid,
        _ => StaticPathError::Io,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn opened_file_handle_is_stable_after_path_replacement() {
        let temp = TempDir::new().unwrap();
        let public = temp.path().join("public");
        std::fs::create_dir(&public).unwrap();
        std::fs::write(public.join("asset.txt"), "original").unwrap();
        let target = open_static_path_sync(&public, "asset.txt", false, None).unwrap();
        let StaticPathTarget::File(mut opened) = target else {
            panic!("expected opened file");
        };

        if std::fs::rename(public.join("asset.txt"), public.join("moved.txt")).is_ok() {
            std::fs::write(public.join("asset.txt"), "replacement").unwrap();
        }

        let mut content = String::new();
        opened.file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "original");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_final_and_intermediate_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let public = temp.path().join("public");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&public).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(outside.join("secret.txt"), public.join("final.txt")).unwrap();
        symlink(&outside, public.join("nested")).unwrap();

        assert!(matches!(
            open_static_path_sync(&public, "final.txt", false, None),
            Err(StaticPathError::Forbidden)
        ));
        assert!(matches!(
            open_static_path_sync(&public, "nested/secret.txt", false, None),
            Err(StaticPathError::Forbidden)
        ));
    }
}
