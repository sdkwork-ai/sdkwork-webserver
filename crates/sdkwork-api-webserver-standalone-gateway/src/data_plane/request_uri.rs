use axum::http::Uri;
use sdkwork_webserver_core::{normalize_uri_path, UriPathNormalizationError, WebServerLimits};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestUriError {
    Invalid,
    TooLong,
}

pub(super) fn validate_request_uri(
    uri: &Uri,
    limits: &WebServerLimits,
) -> Result<String, RequestUriError> {
    validate_path(uri.path().as_bytes(), limits)?;
    validate_query(uri.query().map(str::as_bytes), limits)?;
    normalize_uri_path(
        uri.path(),
        limits.max_decoded_path_bytes,
        limits.max_path_segments,
    )
    .map_err(|error| match error {
        UriPathNormalizationError::Invalid => RequestUriError::Invalid,
        UriPathNormalizationError::TooLong => RequestUriError::TooLong,
    })
}

fn validate_path(path: &[u8], limits: &WebServerLimits) -> Result<(), RequestUriError> {
    if path.len() > limits.max_uri_path_bytes {
        return Err(RequestUriError::TooLong);
    }
    let mut position = 0_usize;
    let mut decoded = 0_usize;
    let mut segments = 0_usize;
    let mut segment_has_bytes = false;
    while position < path.len() {
        let (byte, consumed) = decode_byte(path, position)?;
        position += consumed;
        decoded = decoded.saturating_add(1);
        if decoded > limits.max_decoded_path_bytes {
            return Err(RequestUriError::TooLong);
        }
        if byte == b'/' {
            if segment_has_bytes {
                segments = segments.saturating_add(1);
                if segments > limits.max_path_segments {
                    return Err(RequestUriError::TooLong);
                }
                segment_has_bytes = false;
            }
        } else {
            validate_decoded_byte(byte)?;
            segment_has_bytes = true;
        }
    }
    if segment_has_bytes {
        segments = segments.saturating_add(1);
    }
    if segments > limits.max_path_segments {
        return Err(RequestUriError::TooLong);
    }
    Ok(())
}

fn validate_query(query: Option<&[u8]>, limits: &WebServerLimits) -> Result<(), RequestUriError> {
    let Some(query) = query else {
        return Ok(());
    };
    if limits.max_query_string_bytes == 0
        || limits.max_query_parameters == 0
        || limits.max_query_component_bytes == 0
        || query.len() > limits.max_query_string_bytes
    {
        return Err(RequestUriError::TooLong);
    }
    if query.is_empty() {
        return Ok(());
    }

    let mut parameters = 0_usize;
    for parameter in query.split(|byte| *byte == b'&') {
        parameters = parameters.saturating_add(1);
        if parameters > limits.max_query_parameters {
            return Err(RequestUriError::TooLong);
        }
        let separator = parameter.iter().position(|byte| *byte == b'=');
        let (name, value) = match separator {
            Some(index) => (&parameter[..index], &parameter[index + 1..]),
            None => (parameter, &[][..]),
        };
        validate_query_component(name, limits.max_query_component_bytes)?;
        validate_query_component(value, limits.max_query_component_bytes)?;
    }
    Ok(())
}

fn validate_query_component(bytes: &[u8], maximum: usize) -> Result<(), RequestUriError> {
    if bytes.len() > maximum {
        return Err(RequestUriError::TooLong);
    }
    let mut position = 0_usize;
    while position < bytes.len() {
        let (byte, consumed) = decode_byte(bytes, position)?;
        position += consumed;
        validate_decoded_byte(byte)?;
    }
    Ok(())
}

fn decode_byte(bytes: &[u8], position: usize) -> Result<(u8, usize), RequestUriError> {
    let byte = bytes[position];
    if byte != b'%' {
        return Ok((byte, 1));
    }
    if position.saturating_add(2) >= bytes.len() {
        return Err(RequestUriError::Invalid);
    }
    let high = hex_value(bytes[position + 1]).ok_or(RequestUriError::Invalid)?;
    let low = hex_value(bytes[position + 2]).ok_or(RequestUriError::Invalid)?;
    Ok(((high << 4) | low, 3))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_decoded_byte(byte: u8) -> Result<(), RequestUriError> {
    if byte == b'\\' || byte == 0 || byte < b' ' || byte == 0x7f {
        Err(RequestUriError::Invalid)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;
    use sdkwork_webserver_core::WebServerLimits;

    use super::{validate_request_uri, RequestUriError};

    #[test]
    fn validates_path_query_and_percent_budgets_without_decoding_allocations() {
        let mut limits = WebServerLimits {
            max_uri_path_bytes: 32,
            max_decoded_path_bytes: 16,
            max_path_segments: 3,
            max_query_string_bytes: 32,
            max_query_parameters: 2,
            max_query_component_bytes: 8,
            ..WebServerLimits::default()
        };
        assert_eq!(
            validate_request_uri(&"/one/two?q=ok&x=1".parse().expect("valid URI"), &limits)
                .as_deref(),
            Ok("/one/two")
        );
        assert_eq!(
            validate_request_uri(&"/one/two/three/four".parse().expect("URI"), &limits),
            Err(RequestUriError::TooLong)
        );
        assert_eq!(
            validate_request_uri(&"/?a=1&b=2&c=3".parse().expect("URI"), &limits),
            Err(RequestUriError::TooLong)
        );
        limits.max_query_parameters = 4;
        assert_eq!(
            validate_request_uri(&"/?component=123456789".parse().expect("URI"), &limits),
            Err(RequestUriError::TooLong)
        );
    }

    #[test]
    fn rejects_malformed_escapes_and_decoded_control_or_backslash() {
        let limits = WebServerLimits::default();
        for uri in ["/bad%2", "/bad%zz", "/bad%00", "/bad%5cpath", "/?q=%0d"] {
            let uri = uri
                .parse::<Uri>()
                .expect("HTTP Uri accepts raw escape text");
            assert_eq!(
                validate_request_uri(&uri, &limits),
                Err(RequestUriError::Invalid),
                "URI {uri} must fail"
            );
        }
    }
}
