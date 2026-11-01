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

// One of the two variants carries the opened file handle, its metadata and the
// resolved path, so the enum is inherently as large as that struct. Boxing the
// file variant would add one allocation to every static request for no size
// benefit at the only use site, which matches on the value immediately.
#[allow(clippy::large_enum_variant)]
pub(crate) enum StaticPathTarget {
    File(OpenedStaticFile),
    RedirectToDirectory,
    /// Directory listing: `autoindex on` for a directory request whose `index`
    /// file does not exist. Entries are already filtered (hidden names
    /// skipped), stat-ed and bounded; the renderer sorts and formats them.
    Autoindex {
        entries: Vec<AutoindexEntry>,
    },
}

/// One directory entry read for an nginx `autoindex` listing.
pub(crate) struct AutoindexEntry {
    pub(crate) name: String,
    pub(crate) is_dir: bool,
    pub(crate) is_file: bool,
    pub(crate) size: u64,
    /// Modification time as Unix seconds (0 when unavailable).
    pub(crate) mtime: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StaticPathError {
    Invalid,
    Forbidden,
    NotFound,
    Io,
}

// Same shape as `StaticPathTarget`: a transient stack value returned to the
// caller and consumed in the same function, not a long-lived collection element.
#[allow(clippy::large_enum_variant)]
enum OpenedEntry {
    File(OpenedStaticFile),
    Directory(Dir),
}

pub(crate) async fn open_static_path(
    root: &Path,
    relative: &str,
    request_path_has_trailing_slash: bool,
    spa_fallback: Option<&str>,
    autoindex: bool,
    index_files: Option<&[String]>,
    random_index: bool,
) -> Result<StaticPathTarget, StaticPathError> {
    let root = root.to_owned();
    let relative = relative.to_owned();
    let spa_fallback = spa_fallback.map(str::to_owned);
    let index_files = index_files.map(<[String]>::to_vec);
    tokio::task::spawn_blocking(move || {
        open_static_path_sync(
            &root,
            &relative,
            request_path_has_trailing_slash,
            spa_fallback.as_deref(),
            autoindex,
            index_files.as_deref(),
            random_index,
        )
    })
    .await
    .map_err(|_| StaticPathError::Io)?
}

#[allow(clippy::too_many_arguments)]
fn open_static_path_sync(
    root: &Path,
    relative: &str,
    request_path_has_trailing_slash: bool,
    spa_fallback: Option<&str>,
    autoindex: bool,
    index_files: Option<&[String]>,
    random_index: bool,
) -> Result<StaticPathTarget, StaticPathError> {
    let components = validated_components(relative)?;
    let root_dir = Dir::open_ambient_dir(root, ambient_authority()).map_err(map_root_error)?;

    match open_relative_entry(root_dir, &components, Path::new(relative)) {
        Ok(OpenedEntry::File(file)) => Ok(StaticPathTarget::File(file)),
        Ok(OpenedEntry::Directory(_directory)) if !request_path_has_trailing_slash => {
            Ok(StaticPathTarget::RedirectToDirectory)
        }
        Ok(OpenedEntry::Directory(directory)) => {
            // nginx `random_index on`: the module is registered in the content
            // phase ahead of `index`/`autoindex`, so a directory request picks
            // a random non-hidden regular file before those handlers run. An
            // empty candidate set falls through to the ordinary index/autoindex
            // behavior, exactly like nginx's `NGX_DECLINED`.
            if random_index {
                if let Some(file) = select_random_index_file(&directory, relative)? {
                    return Ok(StaticPathTarget::File(file));
                }
            }
            match open_index(&directory, relative, index_files) {
                Ok(file) => Ok(StaticPathTarget::File(file)),
                // nginx `autoindex off` + an `index` that names no existing file
                // answers 403 Forbidden for a directory request, not 404: the
                // directory exists, so the representation is present but not
                // served/listed (probed against nginx 1.29.6: `/dir/` with a
                // missing `index.html` is 403, with no `ETag`).
                //
                // This also pre-empts an SPA fallback: nginx's `try_files $uri/`
                // matches the existing directory, the index module then refuses
                // it, and the fallback target is never reached.
                //
                // nginx `autoindex on` replaces that 403 with a generated listing:
                // the index module hands the request to the autoindex module when
                // no index file exists.
                Err(StaticPathError::NotFound) if autoindex => Ok(StaticPathTarget::Autoindex {
                    entries: read_autoindex_entries(directory)?,
                }),
                Err(StaticPathError::NotFound) => Err(StaticPathError::Forbidden),
                Err(error) => Err(error),
            }
        }
        Err(StaticPathError::NotFound) => open_fallback(root, spa_fallback),
        Err(error) => {
            if random_index {
                // nginx logs the failing directory path for an unreadable
                // directory; keep the diagnostic located without changing the
                // client-visible status mapping.
                tracing::warn!(
                    directory = relative,
                    error = ?error,
                    "random_index could not open the requested directory"
                );
            }
            Err(error)
        }
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

fn open_index(
    directory: &Dir,
    parent_hint: &str,
    index_files: Option<&[String]>,
) -> Result<OpenedStaticFile, StaticPathError> {
    // nginx `index` tries its declared names in order; when the directive is
    // undeclared the default is `index.html`.
    let default = ["index.html".to_owned()];
    for name in index_files.unwrap_or(&default) {
        let path_hint = Path::new(parent_hint).join(name);
        match open_entry(directory, Path::new(name), &path_hint)? {
            OpenedEntry::File(file) => return Ok(file),
            OpenedEntry::Directory(_) => continue,
        }
    }
    Err(StaticPathError::NotFound)
}

/// Upper bound on directory entries inspected by `random_index`.
///
/// nginx has no such bound; the runtime refuses an oversized directory with
/// 403 instead of spending unbounded blocking-pool time scanning it, matching
/// the bounded `autoindex` listing policy.
const MAXIMUM_RANDOM_INDEX_ENTRIES: usize = 4096;

/// Deterministic per-process request counter for `random_index` selection.
///
/// The counter is the only shared mutable state and it is a plain atomic
/// increment; the seed is derived from the requested directory path, so the
/// selection is deterministic and reproducible for a given directory while
/// successive requests still distribute across the candidate files.
static RANDOM_INDEX_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Select one random regular file from an opened directory.
///
/// nginx reads every directory entry, skips hidden names (a leading `.`) and
/// anything that is not a regular file, then chooses uniformly with
/// `ngx_random()`. The runtime uses a bounded scan and a deterministic seeded
/// SplitMix64 selection instead of the C library's per-process RNG (so the
/// selection can be reproduced and the differential can assert the candidate
/// set). Symlinks are skipped even when they resolve to a regular file: every
/// other static delivery path in the runtime refuses symlinks, and a random
/// index must not become a symlink-following escape hatch.
///
/// Returns `Ok(None)` when the directory has no regular non-hidden file, which
/// lets the caller continue to `index`/`autoindex` like nginx's `DECLINED`.
fn select_random_index_file(
    directory: &Dir,
    parent_hint: &str,
) -> Result<Option<OpenedStaticFile>, StaticPathError> {
    let iterator = directory.entries().map_err(|error| {
        tracing::warn!(
            directory = parent_hint,
            %error,
            "random_index directory enumeration failed"
        );
        map_entry_error(error)
    })?;
    let mut names = Vec::new();
    let mut scanned = 0_usize;
    for item in iterator {
        let item = item.map_err(|error| {
            tracing::warn!(
                directory = parent_hint,
                %error,
                "random_index directory entry read failed"
            );
            map_entry_error(error)
        })?;
        scanned = scanned.saturating_add(1);
        if scanned > MAXIMUM_RANDOM_INDEX_ENTRIES {
            tracing::warn!(
                directory = parent_hint,
                limit = MAXIMUM_RANDOM_INDEX_ENTRIES,
                "random_index directory exceeds the bounded entry scan"
            );
            return Err(StaticPathError::Forbidden);
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let file_type = item.file_type().map_err(|error| {
            tracing::warn!(
                directory = parent_hint,
                entry = %name,
                %error,
                "random_index entry type lookup failed"
            );
            map_entry_error(error)
        })?;
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
        names.push(name);
    }
    // Sorted candidates make the seeded selection independent of filesystem
    // readdir order (nginx's order is implementation-defined and unobservable).
    names.sort();
    let Some(index) = next_random_index(&random_index_seed(parent_hint), names.len()) else {
        return Ok(None);
    };
    let name = &names[index];
    let path_hint = Path::new(parent_hint).join(name);
    match open_entry(directory, Path::new(name), &path_hint).inspect_err(|error| {
        tracing::warn!(
            directory = parent_hint,
            selected = name,
            error = ?error,
            "random_index selected candidate could not be opened"
        );
    })? {
        OpenedEntry::File(file) => Ok(Some(file)),
        // The file vanished between the scan and the open (or was replaced by
        // a directory): nginx's internal redirect would then fail in the
        // static handler, so surface the same not-found result.
        OpenedEntry::Directory(_) => Err(StaticPathError::NotFound),
    }
}

/// SplitMix64 finalizer: the deterministic mixing step of the seeded selector.
fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

/// Stable seed for one directory: FNV-1a over the root-relative request path.
fn random_index_seed(parent_hint: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in parent_hint.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    splitmix64(hash)
}

/// The selection function, isolated so its coverage is unit-testable.
fn select_random_index(seed: u64, counter: u64, length: usize) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let mixed = splitmix64(seed ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    Some((mixed % u64::try_from(length).ok()?) as usize)
}

fn next_random_index(seed: &u64, length: usize) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let counter = RANDOM_INDEX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    select_random_index(*seed, counter, length)
}

/// Read nginx `autoindex` entries from an opened directory.
///
/// Hidden names (a leading `.`) are skipped like nginx; metadata follows
/// symlinks (`ngx_de_info`, with a no-follow fallback for broken links) so a
/// symlinked directory lists as a directory. The collection is bounded by
/// [`MAXIMUM_AUTOINDEX_ENTRIES`]; a larger directory is refused instead of
/// building an unbounded listing.
fn read_autoindex_entries(directory: Dir) -> Result<Vec<AutoindexEntry>, StaticPathError> {
    const MAXIMUM: usize = super::autoindex::MAXIMUM_AUTOINDEX_ENTRIES;
    let mut entries = Vec::new();
    let iterator = directory.entries().map_err(map_entry_error)?;
    for item in iterator {
        let item = item.map_err(map_entry_error)?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if entries.len() >= MAXIMUM {
            return Err(StaticPathError::Forbidden);
        }
        let (is_dir, is_file, size, mtime) = match item.metadata() {
            Ok(metadata) => (
                metadata.is_dir(),
                metadata.is_file(),
                metadata.len(),
                metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.into_std().duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|elapsed| elapsed.as_secs() as i64)
                    .unwrap_or(0),
            ),
            Err(_) => {
                // Broken symlink or a transient stat failure: nginx falls back
                // to lstat, which classifies it as neither file nor directory.
                let file_type = item.file_type().ok();
                (
                    file_type.as_ref().is_some_and(|kind| kind.is_dir()),
                    file_type.as_ref().is_some_and(|kind| kind.is_file()),
                    0,
                    0,
                )
            }
        };
        entries.push(AutoindexEntry {
            name,
            is_dir,
            is_file,
            size,
            mtime,
        });
    }
    Ok(entries)
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
        let target =
            open_static_path_sync(&public, "asset.txt", false, None, false, None, false).unwrap();
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

    #[test]
    fn a_directory_without_an_index_is_forbidden_like_nginx() {
        let temp = TempDir::new().unwrap();
        let public = temp.path().join("public");
        let nodx = public.join("nodx");
        std::fs::create_dir_all(&nodx).unwrap();
        std::fs::write(nodx.join("keep.txt"), "keep").unwrap();
        std::fs::write(public.join("asset.txt"), "asset").unwrap();

        // A trailing-slash request for an existing directory with no index
        // document is nginx's 403 (autoindex off), whether or not a try_files
        // fallback is configured: `$uri/` matches the directory, then the index
        // module refuses it.
        for spa_fallback in [None, Some("index.html")] {
            assert!(matches!(
                open_static_path_sync(&public, "nodx/", true, spa_fallback, false, None, false),
                Err(StaticPathError::Forbidden)
            ));
        }

        // Without the trailing slash nginx first redirects to the canonical
        // directory URI; that branch is unchanged by the 403 rule.
        assert!(matches!(
            open_static_path_sync(&public, "nodx", false, None, false, None, false),
            Ok(StaticPathTarget::RedirectToDirectory)
        ));

        // A real file inside the directory is still served.
        assert!(matches!(
            open_static_path_sync(&public, "nodx/keep.txt", false, None, false, None, false),
            Ok(StaticPathTarget::File(_))
        ));
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
            open_static_path_sync(&public, "final.txt", false, None, false, None, false),
            Err(StaticPathError::Forbidden)
        ));
        assert!(matches!(
            open_static_path_sync(
                &public,
                "nested/secret.txt",
                false,
                None,
                false,
                None,
                false
            ),
            Err(StaticPathError::Forbidden)
        ));
    }

    #[test]
    fn seeded_random_index_covers_every_candidate() {
        // Every window of `length * 64` consecutive counters must hit every
        // candidate for the small directory sizes this selector serves. This
        // is what makes the differential candidate-set evidence deterministic
        // rather than relying on a lucky random draw.
        for length in 1..=16_usize {
            for start in 0..2048_u64 {
                let seen = (0..length * 64)
                    .filter_map(|offset| {
                        select_random_index(0x5EED_1234_ABCD_0001, start + offset as u64, length)
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(
                    seen.len(),
                    length,
                    "random_index did not cover all {length} candidates from counter {start}"
                );
            }
        }
        assert_eq!(select_random_index(7, 1, 0), None);
    }

    #[test]
    fn random_index_selects_only_visible_regular_files() {
        let temp = TempDir::new().unwrap();
        let public = temp.path().join("public");
        let pick = public.join("pick");
        std::fs::create_dir_all(pick.join("nested")).unwrap();
        std::fs::write(pick.join(".hidden"), "hidden").unwrap();
        std::fs::write(pick.join("one.txt"), "one").unwrap();
        std::fs::write(pick.join("two.txt"), "two").unwrap();
        std::fs::write(pick.join("three.txt"), "three").unwrap();

        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..(3 * 64) {
            let target =
                open_static_path_sync(&public, "pick/", true, None, false, None, true).unwrap();
            match target {
                StaticPathTarget::File(file) => {
                    seen.insert(
                        file.path_hint
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
                _ => panic!("random_index must serve an opened file"),
            }
        }
        assert_eq!(
            seen,
            ["one.txt", "three.txt", "two.txt"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );

        // A directory with no candidate (only hidden names and a subdirectory)
        // falls through to nginx's ordinary directory behavior: 403 with
        // `autoindex off`, a listing with it on.
        let empty = public.join("empty");
        std::fs::create_dir_all(empty.join("nested")).unwrap();
        std::fs::write(empty.join(".hidden"), "hidden").unwrap();
        assert!(matches!(
            open_static_path_sync(&public, "empty/", true, None, false, None, true),
            Err(StaticPathError::Forbidden)
        ));
        assert!(matches!(
            open_static_path_sync(&public, "empty/", true, None, true, None, true),
            Ok(StaticPathTarget::Autoindex { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn random_index_never_selects_a_symlinked_file() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let public = temp.path().join("public");
        let outside = temp.path().join("outside.txt");
        std::fs::create_dir(&public).unwrap();
        std::fs::write(&outside, "outside secret").unwrap();
        symlink(&outside, public.join("linked.txt")).unwrap();

        assert!(matches!(
            open_static_path_sync(&public, "", true, None, false, None, true),
            Err(StaticPathError::Forbidden)
        ));
    }
}
