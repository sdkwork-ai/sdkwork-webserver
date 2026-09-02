//! nginx `secure_link` module verification (pure functions).
//!
//! Two modes mirror `ngx_http_secure_link_module`:
//!
//! - `Secret` (`secure_link_secret`): URIs are `/prefix/<hash>/<rest>` where
//!   `hash = md5_hex(secret + rest)`. Verification returns the serving URI
//!   (`/prefix/<rest>`), which the data plane uses for the request.
//! - `Md5` (`secure_link` + `secure_link_md5`): a query argument carries
//!   `md5_hex(evaluated template)` with an optional expiry timestamp.
//!
//! Unsupported variables in an MD5 template fail closed at materialization;
//! at request time an invalid or missing digest rejects with 403.

use md5::{Digest, Md5};

use super::model::SecureLinkMode;

/// Compute the lowercase hex MD5 digest of `input` (nginx `secure_link`
/// digests are lowercase hex).
pub fn md5_hex(input: &[u8]) -> String {
    let digest = Md5::digest(input);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

/// `secure_link_secret` verification. `uri` must start with `prefix` and
/// look like `prefix/<hash>/<rest>`; returns the serving URI
/// `prefix/<rest>` on success.
pub fn verify_secret_link(
    uri: &str,
    prefix: &str,
    secret: &str,
) -> Result<String, SecureLinkFailure> {
    let Some(after) = uri.strip_prefix(prefix) else {
        return Err(SecureLinkFailure);
    };
    let after = after.trim_start_matches('/');
    let Some((hash, tail)) = after.split_once('/') else {
        return Err(SecureLinkFailure);
    };
    if hash.is_empty() || tail.is_empty() {
        return Err(SecureLinkFailure);
    }
    let expected = md5_hex(format!("{secret}{tail}").as_bytes());
    if expected == hash {
        let base = prefix.trim_end_matches('/');
        Ok(format!("{base}/{tail}"))
    } else {
        Err(SecureLinkFailure)
    }
}

/// `secure_link` + `secure_link_md5` verification. `provided` is the digest
/// query argument and `expires` the optional expiry timestamp argument.
/// `now_unix_seconds` is injected for deterministic tests.
pub fn verify_md5_link(
    uri: &str,
    remote_addr: &str,
    provided: &str,
    expires: Option<&str>,
    template: &str,
    now_unix_seconds: u64,
) -> Result<(), SecureLinkFailure> {
    if provided.is_empty() {
        return Err(SecureLinkFailure);
    }
    let Some(evaluated) = evaluate_md5_template(template, uri, remote_addr, expires.unwrap_or(""))
    else {
        return Err(SecureLinkFailure);
    };
    if let Some(expires) = expires {
        let Ok(timestamp) = expires.parse::<u64>() else {
            return Err(SecureLinkFailure);
        };
        if timestamp < now_unix_seconds {
            return Err(SecureLinkFailure);
        }
    }
    if md5_hex(evaluated.as_bytes()) == provided {
        Ok(())
    } else {
        Err(SecureLinkFailure)
    }
}

/// Evaluate an MD5 template over the supported variable subset. Any other
/// `$variable` makes the template unverifiable (fail closed).
fn evaluate_md5_template(
    template: &str,
    uri: &str,
    remote_addr: &str,
    secure_link_expires: &str,
) -> Option<String> {
    let mut output = String::with_capacity(template.len());
    let mut remainder = template;
    while let Some(index) = remainder.find('$') {
        output.push_str(&remainder[..index]);
        let rest = &remainder[index..];
        let variable = if rest.starts_with("$secure_link_expires") {
            "$secure_link_expires"
        } else if rest.starts_with("$uri") {
            "$uri"
        } else if rest.starts_with("$remote_addr") {
            "$remote_addr"
        } else {
            return None;
        };
        match variable {
            "$secure_link_expires" => output.push_str(secure_link_expires),
            "$uri" => output.push_str(uri),
            "$remote_addr" => output.push_str(remote_addr),
            _ => unreachable!("matched variable"),
        }
        remainder = &rest[variable.len()..];
    }
    output.push_str(remainder);
    Some(output)
}

/// Marker error; the data plane answers 403 (nginx secure_link semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecureLinkFailure;

/// Verify a configured `secure_link` against a request, returning the
/// serving URI for `Secret` mode rewrites.
pub fn verify_secure_link(
    uri: &str,
    remote_addr: &str,
    query: Option<&str>,
    mode: &SecureLinkMode,
    prefix: &str,
    now_unix_seconds: u64,
) -> Result<Option<String>, SecureLinkFailure> {
    match mode {
        SecureLinkMode::Secret { secret } => verify_secret_link(uri, prefix, secret).map(Some),
        SecureLinkMode::Md5 {
            argument,
            template,
            expires_argument,
        } => {
            let query = query.unwrap_or("");
            let provided = query_argument(query, argument).unwrap_or_default();
            let expires = expires_argument
                .as_deref()
                .and_then(|name| query_argument(query, name));
            verify_md5_link(
                uri,
                remote_addr,
                &provided,
                expires.as_deref(),
                template,
                now_unix_seconds,
            )?;
            Ok(None)
        }
    }
}

fn query_argument(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key == name {
            Some(value.to_owned())
        } else {
            None
        }
    })
}

/// Validate an MD5 template at materialization: only the supported
/// variables are accepted (fail closed otherwise).
pub fn validate_md5_template(template: &str) -> Result<(), String> {
    let mut remainder = template;
    while let Some(index) = remainder.find('$') {
        let rest = &remainder[index..];
        let supported = ["$secure_link_expires", "$uri", "$remote_addr"];
        if let Some(variable) = supported
            .iter()
            .find(|candidate| rest.starts_with(**candidate))
        {
            remainder = &rest[variable.len()..];
        } else {
            return Err(format!(
                "unsupported variable in secure_link_md5 template `{template}`; supported: $uri $remote_addr $secure_link_expires"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_link_verifies_and_rewrites() {
        let secret = "s3cret";
        let tail = "report.pdf";
        let hash = md5_hex(format!("{secret}{tail}").as_bytes());
        let uri = format!("/downloads/{hash}/{tail}");
        let serving = verify_secret_link(&uri, "/downloads/", secret).expect("verify");
        assert_eq!(serving, "/downloads/report.pdf");
    }

    #[test]
    fn secret_link_rejects_wrong_hash_and_malformed_uris() {
        let secret = "s3cret";
        assert!(verify_secret_link("/downloads/beef/report.pdf", "/downloads/", secret).is_err());
        assert!(verify_secret_link("/downloads/report.pdf", "/downloads/", secret).is_err());
        assert!(verify_secret_link("/other/hash/rest", "/downloads/", secret).is_err());
        let hash = md5_hex(format!("{secret}rest").as_bytes());
        assert!(verify_secret_link(&format!("/downloads/{hash}/"), "/downloads/", secret).is_err());
    }

    #[test]
    fn md5_link_verifies_with_expiry_and_remote_addr() {
        let template = "$secure_link_expires$uri $remote_addr s3cret";
        let uri = "/links/file.txt";
        let remote = "203.0.113.7";
        let expires = "4102444800"; // 2100-01-01
        let evaluated = format!("{expires}{uri} {remote} s3cret");
        let provided = md5_hex(evaluated.as_bytes());
        assert!(verify_md5_link(
            uri,
            remote,
            &provided,
            Some(expires),
            template,
            1_700_000_000
        )
        .is_ok());
        // Expired links are rejected.
        assert!(
            verify_md5_link(uri, remote, &provided, Some("100"), template, 1_700_000_000).is_err()
        );
        // Wrong digest is rejected.
        assert!(
            verify_md5_link(uri, remote, "beef", Some(expires), template, 1_700_000_000).is_err()
        );
        // Missing digest is rejected.
        assert!(verify_md5_link(uri, remote, "", Some(expires), template, 1_700_000_000).is_err());
    }

    #[test]
    fn md5_template_without_expires_checks_digest_only() {
        let template = "$uri$remote_addr key";
        let uri = "/links/file.txt";
        let remote = "203.0.113.7";
        let provided = md5_hex(format!("{uri}{remote} key").as_bytes());
        assert!(verify_md5_link(uri, remote, &provided, None, template, 1_700_000_000).is_ok());
    }

    #[test]
    fn template_validation_rejects_unknown_variables() {
        assert!(validate_md5_template("$uri $remote_addr $secure_link_expires key").is_ok());
        assert!(validate_md5_template("$host key").is_err());
        assert!(validate_md5_template("no variables").is_ok());
    }

    #[test]
    fn verify_secure_link_dispatches_by_mode() {
        let secret = "word";
        let tail = "asset.bin";
        let hash = md5_hex(format!("{secret}{tail}").as_bytes());
        let uri = format!("/p/{hash}/{tail}");
        let mode = SecureLinkMode::Secret {
            secret: secret.to_owned(),
        };
        assert_eq!(
            verify_secure_link(&uri, "1.2.3.4", None, &mode, "/p/", 0).expect("verify"),
            Some("/p/asset.bin".to_owned())
        );
        let md5_mode = SecureLinkMode::Md5 {
            argument: "st".to_owned(),
            template: "$uri$remote_addr key".to_owned(),
            expires_argument: None,
        };
        let digest = md5_hex(format!("{uri}1.2.3.4 key").as_bytes());
        let query = format!("st={digest}");
        assert!(verify_secure_link(&uri, "1.2.3.4", Some(&query), &md5_mode, "/p/", 0).is_ok());
    }
}
