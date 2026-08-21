//! Narrow-precedence ACME HTTP-01 challenge serving.
//!
//! A listener that configures `acmeHttp01.webroot` serves only the exact
//! `/.well-known/acme-challenge/<token>` path from a single bounded regular
//! file under the webroot. The token is validated with the strict ACME token
//! character set, every path component is opened without following symlinks
//! inside the compile-time confined webroot, and the response is bounded to
//! the key-authorization size ceiling. No directory listing, unrelated route,
//! or unregistered token is ever served.

use std::io::Read;
use std::path::Path;

use axum::{
    body::Body,
    http::{Response, StatusCode},
};
use sdkwork_webserver_core::ListenerConfig;

use super::{
    proxy::text_response,
    static_path::{open_challenge_file, StaticPathError},
    ListenerState,
};

const ACME_CHALLENGE_PREFIX: &str = "/.well-known/acme-challenge/";
const MAX_CHALLENGE_TOKEN_BYTES: usize = 256;
/// The key-authorization ceiling mirrors the issuer-side bound so an
/// oversized or malicious file is never served.
const MAX_CHALLENGE_BODY_BYTES: u64 = 2 * 1024;

/// True when the request path enters the ACME challenge namespace on a
/// listener configured for HTTP-01 serving.
pub(crate) fn acme_http01_request_enabled(
    listener: &ListenerConfig,
    normalized_path: &str,
) -> bool {
    listener.acme_http_01.is_some() && normalized_path.starts_with(ACME_CHALLENGE_PREFIX)
}

/// Serves the exact ACME HTTP-01 challenge path for the current listener.
///
/// Callers must gate this with [`acme_http01_request_enabled`] so unrelated
/// paths fall through to normal routing untouched. A configured listener never
/// lets a challenge namespace path fall through to unrelated routes: invalid
/// tokens, methods, and missing files all fail closed with a plain 404/405.
pub(crate) async fn serve_acme_http01_challenge(
    state: &ListenerState,
    normalized_path: &str,
    method: &str,
) -> Option<Response<Body>> {
    let token = normalized_path.strip_prefix(ACME_CHALLENGE_PREFIX)?;
    if token.is_empty() {
        // The bare challenge directory path is part of the reserved namespace;
        // it must fail closed instead of falling through to unrelated routes.
        return Some(text_response(
            StatusCode::NOT_FOUND,
            "challenge was not found\n",
        ));
    }
    if !valid_challenge_token(token) {
        return Some(text_response(
            StatusCode::NOT_FOUND,
            "challenge was not found\n",
        ));
    }
    if !matches!(method, "GET" | "HEAD") {
        return Some(text_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "method is not allowed\n",
        ));
    }
    let generation = state.runtime.current();
    let Some(webroot) = generation.app.acme_webroot(&state.listener_id) else {
        return Some(text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "challenge service is unavailable\n",
        ));
    };
    let key_authorization = match read_challenge_key_authorization(webroot, token).await {
        Ok(key_authorization) => key_authorization,
        Err(error) => {
            tracing::debug!(
                listener_id = %state.listener_id,
                error = ?error,
                "ACME challenge file read failed"
            );
            return Some(text_response(
                StatusCode::NOT_FOUND,
                "challenge was not found\n",
            ));
        }
    };
    let Some(key_authorization) = key_authorization else {
        return Some(text_response(
            StatusCode::NOT_FOUND,
            "challenge was not found\n",
        ));
    };
    let body = if method == "HEAD" {
        Body::empty()
    } else {
        Body::from(key_authorization.clone())
    };
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    // The key authorization is validation-window material: never cached by
    // intermediaries, and HEAD reports the same length as GET.
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    if method == "HEAD" {
        if let Ok(value) = axum::http::HeaderValue::from_str(&key_authorization.len().to_string()) {
            response
                .headers_mut()
                .insert(axum::http::header::CONTENT_LENGTH, value);
        }
    }
    Some(response)
}

async fn read_challenge_key_authorization(
    webroot: &Path,
    token: &str,
) -> Result<Option<Vec<u8>>, StaticPathError> {
    let mut opened = match open_challenge_file(webroot, token).await {
        Ok(opened) => opened,
        Err(StaticPathError::NotFound) => return Ok(None),
        Err(error) => return Err(error),
    };
    if opened.metadata.len() > MAX_CHALLENGE_BODY_BYTES {
        return Ok(None);
    }
    let mut key_authorization = Vec::with_capacity(opened.metadata.len() as usize);
    opened
        .file
        .read_to_end(&mut key_authorization)
        .map_err(|_| StaticPathError::Io)?;
    Ok(Some(key_authorization))
}

fn valid_challenge_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_CHALLENGE_TOKEN_BYTES
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_token_validation_matches_acme_token_set() {
        assert!(valid_challenge_token("token"));
        assert!(valid_challenge_token("abc-ABC_123"));
        assert!(valid_challenge_token(
            &"t".repeat(MAX_CHALLENGE_TOKEN_BYTES)
        ));
        assert!(!valid_challenge_token(""));
        assert!(!valid_challenge_token("../escape"));
        assert!(!valid_challenge_token("has space"));
        assert!(!valid_challenge_token("dot.token"));
        assert!(!valid_challenge_token(
            &"t".repeat(MAX_CHALLENGE_TOKEN_BYTES + 1)
        ));
    }
}
