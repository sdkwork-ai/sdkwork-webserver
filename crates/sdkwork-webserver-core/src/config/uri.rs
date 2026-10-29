#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UriPathNormalizationError {
    Invalid,
    TooLong,
}

/// Normalize a request path with nginx's default `merge_slashes on`
/// behavior: percent-decoded, dot-segments resolved, repeated adjacent slashes
/// collapsed.
pub fn normalize_uri_path(
    path: &str,
    maximum_decoded_bytes: usize,
    maximum_segments: usize,
) -> Result<String, UriPathNormalizationError> {
    normalize_uri_path_with_options(path, maximum_decoded_bytes, maximum_segments, true)
}

/// Normalize a request path, selecting nginx's `merge_slashes` behavior.
///
/// With `merge_slashes` off the canonical path keeps every repeated adjacent
/// slash that the client sent (`/a//b` stays `/a//b`, `$uri` and exact-location
/// matching see the unmerged form) while dot-segments are still resolved and
/// `..` can never escape the root. Differential-probed against nginx 1.29.6:
/// `/a//../b` normalizes to `/a/b` (the `..` pops the empty segment),
/// `/a///` stays `/a///`, `//` stays `//`, and a `..` that would escape the
/// root is rejected with 400.
pub fn normalize_uri_path_with_options(
    path: &str,
    maximum_decoded_bytes: usize,
    maximum_segments: usize,
    merge_slashes: bool,
) -> Result<String, UriPathNormalizationError> {
    if !path.starts_with('/') {
        return Err(UriPathNormalizationError::Invalid);
    }
    let source = path.as_bytes();
    let mut decoded = Vec::with_capacity(source.len().min(maximum_decoded_bytes));
    let mut position = 0_usize;
    while position < source.len() {
        let byte = source[position];
        if byte == b'%' {
            if position.saturating_add(2) >= source.len() {
                return Err(UriPathNormalizationError::Invalid);
            }
            let high = hex(source[position + 1]).ok_or(UriPathNormalizationError::Invalid)?;
            let low = hex(source[position + 2]).ok_or(UriPathNormalizationError::Invalid)?;
            decoded.push((high << 4) | low);
            position += 3;
        } else {
            decoded.push(byte);
            position += 1;
        }
        if decoded.len() > maximum_decoded_bytes {
            return Err(UriPathNormalizationError::TooLong);
        }
    }
    if decoded
        .iter()
        .any(|byte| *byte == 0 || *byte == b'\\' || *byte < b' ' || *byte == 0x7f)
    {
        return Err(UriPathNormalizationError::Invalid);
    }
    let trailing_slash =
        decoded.ends_with(b"/") || decoded.ends_with(b"/.") || decoded.ends_with(b"/..");
    if merge_slashes {
        let mut normalized = Vec::with_capacity(decoded.len().max(1));
        normalized.push(b'/');
        let mut checkpoints = Vec::with_capacity(maximum_segments.min(32));
        for segment in decoded.split(|byte| *byte == b'/') {
            if segment.is_empty() || segment == b"." {
                continue;
            }
            if segment == b".." {
                let checkpoint = checkpoints
                    .pop()
                    .ok_or(UriPathNormalizationError::Invalid)?;
                normalized.truncate(checkpoint);
                continue;
            }
            if checkpoints.len() >= maximum_segments {
                return Err(UriPathNormalizationError::TooLong);
            }
            let checkpoint = normalized.len();
            if normalized.len() > 1 {
                normalized.push(b'/');
            }
            normalized.extend_from_slice(segment);
            checkpoints.push(checkpoint);
        }
        if trailing_slash && normalized.len() > 1 {
            normalized.push(b'/');
        }
        return String::from_utf8(normalized).map_err(|_| UriPathNormalizationError::Invalid);
    }
    // `merge_slashes off`: nginx keeps repeated slashes as their own (empty)
    // path segments, still removes `.` segments, still resolves `..` (popping
    // the previous segment, which may itself be an empty one), and rejects a
    // `..` that would escape the root.
    let pieces: Vec<&[u8]> = decoded.split(|byte| *byte == b'/').skip(1).collect();
    let piece_count = pieces.len();
    let mut segments: Vec<&[u8]> = Vec::with_capacity(piece_count.min(32));
    let mut counted = 0_usize;
    for (index, segment) in pieces.into_iter().enumerate() {
        let last = index + 1 == piece_count;
        if segment.is_empty() {
            // A trailing empty piece is the trailing slash, appended below.
            if last {
                continue;
            }
            segments.push(segment);
            continue;
        }
        if segment == b"." {
            continue;
        }
        if segment == b".." {
            if segments.pop().is_none() {
                return Err(UriPathNormalizationError::Invalid);
            }
            continue;
        }
        counted = counted.saturating_add(1);
        if counted > maximum_segments {
            return Err(UriPathNormalizationError::TooLong);
        }
        segments.push(segment);
    }
    let mut normalized = Vec::with_capacity(decoded.len().max(1));
    normalized.push(b'/');
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            normalized.push(b'/');
        }
        normalized.extend_from_slice(segment);
    }
    if trailing_slash && !segments.is_empty() {
        normalized.push(b'/');
    }
    String::from_utf8(normalized).map_err(|_| UriPathNormalizationError::Invalid)
}

pub(super) fn validate_canonical_uri_path(
    path: &str,
    maximum_decoded_bytes: usize,
    maximum_segments: usize,
) -> Result<(), UriPathNormalizationError> {
    if !path.starts_with('/') {
        return Err(UriPathNormalizationError::Invalid);
    }
    let bytes = path.as_bytes();
    if bytes.len() > maximum_decoded_bytes {
        return Err(UriPathNormalizationError::TooLong);
    }
    if bytes
        .iter()
        .any(|byte| *byte == 0 || *byte == b'\\' || *byte < b' ' || *byte == 0x7f)
    {
        return Err(UriPathNormalizationError::Invalid);
    }

    let mut segments = 0_usize;
    let mut path_segments = bytes[1..].split(|byte| *byte == b'/').peekable();
    while let Some(segment) = path_segments.next() {
        if segment.is_empty() {
            if path_segments.peek().is_some() {
                return Err(UriPathNormalizationError::Invalid);
            }
            continue;
        }
        if segment == b"." || segment == b".." {
            return Err(UriPathNormalizationError::Invalid);
        }
        segments = segments.saturating_add(1);
        if segments > maximum_segments {
            return Err(UriPathNormalizationError::TooLong);
        }
    }
    Ok(())
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)] // WORKSPACE-PATH:allow-fixture-block: this module is the file's #[cfg(test)] unit-test fixture data
mod tests {
    use super::{
        normalize_uri_path, normalize_uri_path_with_options, validate_canonical_uri_path,
        UriPathNormalizationError,
    };

    #[test]
    fn merge_slashes_off_keeps_repeated_slashes_like_nginx() {
        // Differential-probed against nginx 1.29.6 (merge_slashes off).
        let cases: &[(&str, &str)] = &[
            ("/a//b", "/a//b"),
            ("/a//", "/a//"),
            ("/a//b/", "/a//b/"),
            ("//a", "//a"),
            ("///a", "///a"),
            ("//", "//"),
            ("/a/./b", "/a/b"),
            ("/a//./b", "/a//b"),
            ("/a/..", "/"),
            ("/a/../", "/"),
            ("/a//../b", "/a/b"),
            ("/show//../show/x", "/show/show/x"),
            ("/a/...", "/a/..."),
            ("/", "/"),
            // Percent-decoded slashes participate identically.
            ("/a%2F%2Fb", "/a//b"),
        ];
        for (source, expected) in cases {
            assert_eq!(
                normalize_uri_path_with_options(source, 256, 16, false).as_deref(),
                Ok(*expected),
                "merge_slashes off: {source}"
            );
        }
        assert_eq!(
            normalize_uri_path_with_options("/../a", 256, 16, false),
            Err(UriPathNormalizationError::Invalid)
        );
        // A percent-encoded `..` that would escape the root is rejected the
        // same way (`/%2e%2e/a` is 400 in nginx 1.29.6).
        assert_eq!(
            normalize_uri_path_with_options("/%2e%2e/a", 256, 16, false),
            Err(UriPathNormalizationError::Invalid)
        );
        assert_eq!(
            normalize_uri_path_with_options("/a/../..", 256, 16, false),
            Err(UriPathNormalizationError::Invalid)
        );
        // The default remains nginx's `merge_slashes on`.
        assert_eq!(
            normalize_uri_path_with_options("/a//b", 256, 16, true).as_deref(),
            Ok("/a/b")
        );
    }

    #[test]
    fn matches_nginx_core_normalization_cases() {
        for (source, expected) in [
            ("/a/../b", "/b"),
            ("/a/%2e%2e/b", "/b"),
            ("//a///b", "/a/b"),
            ("/a%2fb", "/a/b"),
            ("/a/%2E/b", "/a/b"),
            ("/a/.", "/a/"),
            ("/a/..", "/"),
            ("/a%3fb", "/a?b"),
            ("/a%23b", "/a#b"),
            ("/a%25b", "/a%b"),
            ("/%E4%B8%AD", "/中"),
        ] {
            assert_eq!(normalize_uri_path(source, 128, 16).as_deref(), Ok(expected));
        }
        assert_eq!(
            normalize_uri_path("/../../b", 128, 16),
            Err(UriPathNormalizationError::Invalid)
        );
    }

    #[test]
    fn rejects_invalid_or_over_budget_raw_paths() {
        for source in ["relative", "/bad%2", "/bad%zz", "/bad%00", "/bad%5cpath"] {
            assert_eq!(
                normalize_uri_path(source, 128, 16),
                Err(UriPathNormalizationError::Invalid),
                "raw path {source} must fail"
            );
        }
        assert_eq!(
            normalize_uri_path("/%FF", 128, 16),
            Err(UriPathNormalizationError::Invalid)
        );
        assert_eq!(
            normalize_uri_path("/long", 4, 16),
            Err(UriPathNormalizationError::TooLong)
        );
        assert_eq!(
            normalize_uri_path("/one/two", 128, 1),
            Err(UriPathNormalizationError::TooLong)
        );
    }

    #[test]
    fn canonical_paths_treat_reserved_characters_as_path_data() {
        assert_eq!(validate_canonical_uri_path("/a?b#c%d/中", 128, 2), Ok(()));
        assert_eq!(validate_canonical_uri_path("/a%2Fb", 128, 1), Ok(()));
        for source in ["relative", "//a", "/a//b", "/a/./b", "/a/../b", "/a\\b"] {
            assert_eq!(
                validate_canonical_uri_path(source, 128, 16),
                Err(UriPathNormalizationError::Invalid),
                "canonical path {source} must fail"
            );
        }
        assert_eq!(
            validate_canonical_uri_path("/one/two", 128, 1),
            Err(UriPathNormalizationError::TooLong)
        );
    }
}
