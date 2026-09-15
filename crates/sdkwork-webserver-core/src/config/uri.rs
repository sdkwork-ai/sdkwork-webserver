#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UriPathNormalizationError {
    Invalid,
    TooLong,
}

pub fn normalize_uri_path(
    path: &str,
    maximum_decoded_bytes: usize,
    maximum_segments: usize,
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
    use super::{normalize_uri_path, validate_canonical_uri_path, UriPathNormalizationError};

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
