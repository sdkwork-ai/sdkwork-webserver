//! Development-only credential-entry bootstrap Access-Token handoff for
//! module-import SPA surfaces served by the imports data plane.
//!
//! `IAM_CREDENTIAL_ENTRY_SPEC.md` §3/§4/§5: credential-entry routes (login,
//! registration, password reset, OAuth session bootstrap, IAM runtime policy)
//! require a bootstrap `Access-Token` before dispatch, and the generated SDK
//! transport fails closed before the network when the TokenManager has none.
//! Development renderers may receive the token through the canonical
//! `globalThis.__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__` handoff;
//! staging and production must never inject or embed the token into browser
//! artifacts.
//!
//! The Adaptive Web app shell (`app_shell.rs`) injects the token for its own
//! PC/H5 surfaces. Module apps (for example `sdkwork-im`) are served through
//! this data plane, so the same development handoff is provided here:
//! `development` environments with a configured bootstrap token get the token
//! injected into served SPA `index.html` documents; every other environment
//! fails closed exactly as the spec requires (no injection, no startup
//! failure).

use std::path::PathBuf;
use std::sync::OnceLock;

/// Canonical browser handoff global defined by
/// `@sdkwork/iam-credential-entry` (`IAM_CREDENTIAL_ENTRY_SPEC.md` §4).
pub(crate) const CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN_GLOBAL_KEY: &str =
    "__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__";

const CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN_ENV: &str =
    "SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN";
const DEFAULT_CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_PATH: &str =
    "/etc/sdkwork/webserver/secrets/credential-entry-bootstrap-access-token";
const ENVIRONMENT_ENV: &str = "SDKWORK_WEBSERVER_ENVIRONMENT";

static INJECTION_TOKEN: OnceLock<Option<String>> = OnceLock::new();

/// Resolves the credential-entry bootstrap injection token once per process.
///
/// Returns `None` unless the data plane runs in a development environment and
/// a bootstrap token is configured through the environment variable or the
/// packaged runtime secrets file. The process runs as the unprivileged
/// `sdkwork` user, which owns the secrets file with mode `0600`.
pub(crate) fn injection_token() -> Option<&'static str> {
    INJECTION_TOKEN
        .get_or_init(|| {
            resolve_injection_token(
                std::env::var(ENVIRONMENT_ENV).ok().as_deref(),
                std::env::var(CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN_ENV)
                    .ok()
                    .as_deref(),
                read_default_bootstrap_token_file().as_deref(),
            )
        })
        .as_deref()
}

/// Pure resolution core so the environment policy stays unit-testable.
///
/// Development only (`development` | `dev`); `test`, `staging`, and
/// `production` must never receive the bootstrap token in browser artifacts
/// (`IAM_CREDENTIAL_ENTRY_SPEC.md` §4/§5). Tokens that cannot be embedded
/// safely into an HTML attribute are rejected (fail closed).
fn resolve_injection_token(
    environment: Option<&str>,
    env_token: Option<&str>,
    file_token: Option<&str>,
) -> Option<String> {
    let environment = environment?.trim();
    if !matches!(environment, "development" | "dev") {
        return None;
    }
    let token = env_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            file_token
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(str::to_owned)
        })?;
    if token.chars().any(is_unsafe_for_html_attribute) {
        return None;
    }
    Some(token)
}

fn read_default_bootstrap_token_file() -> Option<String> {
    std::fs::read_to_string(PathBuf::from(DEFAULT_CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_PATH)).ok()
}

/// JWTs are compact and URL-safe; reject anything that could break out of the
/// inline attribute serialization instead of escaping.
fn is_unsafe_for_html_attribute(character: char) -> bool {
    matches!(character, '"' | '\'' | '<' | '>' | '&' | '\n' | '\r' | '\0')
}

pub(crate) fn bootstrap_injection_script(token: &str) -> String {
    format!(
        "<script>globalThis.{}=\"{}\";</script>",
        CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN_GLOBAL_KEY, token
    )
}

/// Injects the bootstrap script before `</head>`, mirroring
/// `app_shell::serve_index_with_bootstrap_token`. Documents without a head
/// closing tag receive the script appended before the document end so the
/// global is still assigned before application modules execute in practice.
pub(crate) fn inject_bootstrap_token(html: &[u8], token: &str) -> Vec<u8> {
    let injected = bootstrap_injection_script(token);
    let Ok(html) = std::str::from_utf8(html) else {
        return html.to_vec();
    };
    if html.contains("</head>") {
        return html
            .replacen("</head>", &format!("{injected}</head>"), 1)
            .into_bytes();
    }
    let mut output = Vec::with_capacity(html.len() + injected.len());
    output.extend_from_slice(html.as_bytes());
    output.extend_from_slice(injected.as_bytes());
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_is_development_only() {
        let token = "header.payload.signature";
        assert_eq!(
            resolve_injection_token(Some("development"), Some(token), None).as_deref(),
            Some(token)
        );
        assert_eq!(
            resolve_injection_token(Some("dev"), Some(token), None).as_deref(),
            Some(token)
        );
        // Spec §4/§5: test/staging/production never inject.
        for environment in ["test", "staging", "production", ""] {
            assert_eq!(
                resolve_injection_token(Some(environment), Some(token), None),
                None,
                "environment {environment} must not inject"
            );
        }
        assert_eq!(resolve_injection_token(None, Some(token), None), None);
    }

    #[test]
    fn missing_or_blank_token_fails_closed() {
        assert_eq!(
            resolve_injection_token(Some("development"), None, None),
            None
        );
        assert_eq!(
            resolve_injection_token(Some("development"), Some("  "), None),
            None
        );
        assert_eq!(
            resolve_injection_token(Some("development"), None, Some("\n")),
            None
        );
    }

    #[test]
    fn env_var_takes_precedence_over_secrets_file() {
        assert_eq!(
            resolve_injection_token(Some("development"), Some("env.token"), Some("file.token"),)
                .as_deref(),
            Some("env.token")
        );
        assert_eq!(
            resolve_injection_token(Some("development"), Some("  "), Some("file.token")).as_deref(),
            Some("file.token")
        );
    }

    #[test]
    fn unsafe_token_characters_fail_closed() {
        for token in ["a\"b", "a'b", "a<b", "a>b", "a&b", "a\nb", "a\rb", "a\0b"] {
            assert_eq!(
                resolve_injection_token(Some("development"), Some(token), None),
                None,
                "token {token:?} must be rejected"
            );
        }
    }

    #[test]
    fn injection_script_uses_canonical_global_key() {
        assert_eq!(
            bootstrap_injection_script("header.payload.signature"),
            "<script>globalThis.__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__=\"header.payload.signature\";</script>"
        );
    }

    #[test]
    fn injection_prefers_head_close_and_falls_back_to_tail() {
        let token = "t";
        let script = bootstrap_injection_script(token);
        let html = b"<!doctype html><html><head><title>x</title></head><body></body></html>";
        let injected = inject_bootstrap_token(html, token);
        let injected = std::str::from_utf8(&injected).unwrap();
        assert_eq!(
            injected,
            "<!doctype html><html><head><title>x</title>{script}</head><body></body></html>"
                .replace("{script}", &script)
        );

        let tail = inject_bootstrap_token(b"<html><body></body></html>", token);
        assert!(std::str::from_utf8(&tail).unwrap().ends_with(&script));
    }

    #[test]
    fn non_utf8_documents_pass_through_unchanged() {
        let html = vec![0xff, 0xfe, 0x00];
        assert_eq!(inject_bootstrap_token(&html, "t"), html);
    }
}
