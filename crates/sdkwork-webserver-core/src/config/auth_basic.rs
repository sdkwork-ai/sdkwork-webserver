//! nginx `auth_basic` / `auth_basic_user_file` (htpasswd) verification.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use md5::{Digest as _, Md5};
use sha1::Sha1;

use super::model::{AuthBasicConfig, AuthBasicUserConfig};

const APR1_MAGIC: &str = "$apr1$";
const APR1_ITOA64: &[u8; 64] =
    b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthBasicDecision {
    /// No auth_basic configured for the route.
    Inactive,
    /// Credentials accepted.
    Allow,
    /// Missing or invalid credentials — respond 401 + WWW-Authenticate.
    Challenge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtpasswdParseError {
    Empty,
    InvalidLine(String),
    DuplicateUser(String),
}

impl std::fmt::Display for HtpasswdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "htpasswd file has no usable user entries"),
            Self::InvalidLine(line) => write!(f, "invalid htpasswd line `{line}`"),
            Self::DuplicateUser(user) => write!(f, "duplicate htpasswd user `{user}`"),
        }
    }
}

/// Parse an Apache-style htpasswd file body into username/hash entries.
pub fn parse_htpasswd(contents: &str) -> Result<Vec<AuthBasicUserConfig>, HtpasswdParseError> {
    let mut users = Vec::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((username, password_hash)) = line.split_once(':') else {
            return Err(HtpasswdParseError::InvalidLine(line.to_owned()));
        };
        if username.is_empty() || password_hash.is_empty() {
            return Err(HtpasswdParseError::InvalidLine(line.to_owned()));
        }
        if users
            .iter()
            .any(|entry: &AuthBasicUserConfig| entry.username == username)
        {
            return Err(HtpasswdParseError::DuplicateUser(username.to_owned()));
        }
        users.push(AuthBasicUserConfig {
            username: username.to_owned(),
            password_hash: password_hash.to_owned(),
        });
    }
    if users.is_empty() {
        return Err(HtpasswdParseError::Empty);
    }
    Ok(users)
}

/// Evaluate HTTP Basic credentials against a materialized `authBasic` config.
pub fn evaluate_auth_basic(
    authorization: Option<&str>,
    config: Option<&AuthBasicConfig>,
) -> AuthBasicDecision {
    let Some(config) = config else {
        return AuthBasicDecision::Inactive;
    };
    let Some(header) = authorization.map(str::trim).filter(|value| !value.is_empty()) else {
        return AuthBasicDecision::Challenge;
    };
    let Some(encoded) = header
        .strip_prefix("Basic ")
        .or_else(|| header.strip_prefix("basic "))
    else {
        return AuthBasicDecision::Challenge;
    };
    let Ok(decoded) = BASE64.decode(encoded.trim().as_bytes()) else {
        return AuthBasicDecision::Challenge;
    };
    let Ok(decoded) = String::from_utf8(decoded) else {
        return AuthBasicDecision::Challenge;
    };
    let Some((username, password)) = decoded.split_once(':') else {
        return AuthBasicDecision::Challenge;
    };
    let Some(user) = config.users.iter().find(|entry| entry.username == username) else {
        return AuthBasicDecision::Challenge;
    };
    if verify_password(password, &user.password_hash) {
        AuthBasicDecision::Allow
    } else {
        AuthBasicDecision::Challenge
    }
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    if password_hash.starts_with(APR1_MAGIC) {
        return verify_apr1(password, password_hash);
    }
    if let Some(encoded) = password_hash.strip_prefix("{SHA}") {
        let digest = Sha1::digest(password.as_bytes());
        let expected = BASE64.encode(digest);
        return constant_eq(encoded.as_bytes(), expected.as_bytes());
    }
    if password_hash.starts_with("$2y$")
        || password_hash.starts_with("$2a$")
        || password_hash.starts_with("$2b$")
    {
        let normalized = if let Some(rest) = password_hash.strip_prefix("$2y$") {
            format!("$2b${rest}")
        } else {
            password_hash.to_owned()
        };
        return bcrypt::verify(password, &normalized).unwrap_or(false);
    }
    false
}

fn verify_apr1(password: &str, password_hash: &str) -> bool {
    let Some(rest) = password_hash.strip_prefix(APR1_MAGIC) else {
        return false;
    };
    let Some((salt, _)) = rest.split_once('$') else {
        return false;
    };
    if salt.is_empty() || salt.len() > 8 {
        return false;
    }
    let computed = apr1_hash(password, salt);
    constant_eq(computed.as_bytes(), password_hash.as_bytes())
}

/// Apache `$apr1$` MD5-based password hash (htpasswd `-m`).
pub fn apr1_hash(password: &str, salt: &str) -> String {
    let salt = &salt[..salt.len().min(8)];
    let password_bytes = password.as_bytes();
    let salt_bytes = salt.as_bytes();

    let mut ctx = Md5::new();
    ctx.update(password_bytes);
    ctx.update(APR1_MAGIC.as_bytes());
    ctx.update(salt_bytes);

    let mut ctx_alt = Md5::new();
    ctx_alt.update(password_bytes);
    ctx_alt.update(salt_bytes);
    ctx_alt.update(password_bytes);
    let digest_alt = ctx_alt.finalize();

    let mut remaining = password_bytes.len();
    while remaining > 16 {
        ctx.update(&digest_alt);
        remaining -= 16;
    }
    ctx.update(&digest_alt[..remaining]);

    let mut bit = password_bytes.len();
    while bit != 0 {
        if bit & 1 == 1 {
            ctx.update(&[0u8]);
        } else {
            ctx.update(&password_bytes[..1]);
        }
        bit >>= 1;
    }
    let mut digest = ctx.finalize().to_vec();

    for i in 0..1000 {
        let mut ctx_loop = Md5::new();
        if i % 2 != 0 {
            ctx_loop.update(password_bytes);
        } else {
            ctx_loop.update(&digest);
        }
        if i % 3 != 0 {
            ctx_loop.update(salt_bytes);
        }
        if i % 7 != 0 {
            ctx_loop.update(password_bytes);
        }
        if i % 2 != 0 {
            ctx_loop.update(&digest);
        } else {
            ctx_loop.update(password_bytes);
        }
        digest = ctx_loop.finalize().to_vec();
    }

    let mut out = String::with_capacity(6 + salt.len() + 1 + 22);
    out.push_str(APR1_MAGIC);
    out.push_str(salt);
    out.push('$');
    out.push_str(&to64(
        (u32::from(digest[0]) << 16) | (u32::from(digest[6]) << 8) | u32::from(digest[12]),
        4,
    ));
    out.push_str(&to64(
        (u32::from(digest[1]) << 16) | (u32::from(digest[7]) << 8) | u32::from(digest[13]),
        4,
    ));
    out.push_str(&to64(
        (u32::from(digest[2]) << 16) | (u32::from(digest[8]) << 8) | u32::from(digest[14]),
        4,
    ));
    out.push_str(&to64(
        (u32::from(digest[3]) << 16) | (u32::from(digest[9]) << 8) | u32::from(digest[15]),
        4,
    ));
    out.push_str(&to64(
        (u32::from(digest[4]) << 16) | (u32::from(digest[10]) << 8) | u32::from(digest[5]),
        4,
    ));
    out.push_str(&to64(u32::from(digest[11]), 2));
    out
}

fn to64(mut value: u32, count: usize) -> String {
    let mut out = String::with_capacity(count);
    for _ in 0..count {
        out.push(APR1_ITOA64[(value & 0x3f) as usize] as char);
        value >>= 6;
    }
    out
}

fn constant_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::{apr1_hash, evaluate_auth_basic, parse_htpasswd, AuthBasicDecision};
    use crate::config::model::{AuthBasicConfig, AuthBasicUserConfig};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use sha1::{Digest as _, Sha1};

    #[test]
    fn parses_htpasswd_and_rejects_duplicates() {
        let users = parse_htpasswd("alice:$apr1$salt$hash\nbob:{SHA}abc\n").expect("parse");
        assert_eq!(users.len(), 2);
        assert!(parse_htpasswd("alice:a\nalice:b\n").is_err());
    }

    #[test]
    fn verifies_apr1_round_trip() {
        let hash = apr1_hash("secret", "saltsalt");
        assert!(hash.starts_with("$apr1$saltsalt$"));
        let config = AuthBasicConfig {
            realm: "Restricted".to_owned(),
            users: vec![AuthBasicUserConfig {
                username: "alice".to_owned(),
                password_hash: hash,
            }],
        };
        let header = format!("Basic {}", BASE64.encode("alice:secret".as_bytes()));
        assert_eq!(
            evaluate_auth_basic(Some(&header), Some(&config)),
            AuthBasicDecision::Allow
        );
        let bad = format!("Basic {}", BASE64.encode("alice:wrong".as_bytes()));
        assert_eq!(
            evaluate_auth_basic(Some(&bad), Some(&config)),
            AuthBasicDecision::Challenge
        );
    }

    #[test]
    fn verifies_sha_password() {
        let digest = Sha1::digest(b"password");
        let hash = format!("{{SHA}}{}", BASE64.encode(digest));
        let config = AuthBasicConfig {
            realm: "x".to_owned(),
            users: vec![AuthBasicUserConfig {
                username: "user".to_owned(),
                password_hash: hash,
            }],
        };
        let header = format!("Basic {}", BASE64.encode("user:password".as_bytes()));
        assert_eq!(
            evaluate_auth_basic(Some(&header), Some(&config)),
            AuthBasicDecision::Allow
        );
    }

    #[test]
    fn verifies_bcrypt_password() {
        let hash = bcrypt::hash("secret", 4).expect("hash");
        let config = AuthBasicConfig {
            realm: "x".to_owned(),
            users: vec![AuthBasicUserConfig {
                username: "user".to_owned(),
                password_hash: hash,
            }],
        };
        let header = format!("Basic {}", BASE64.encode("user:secret".as_bytes()));
        assert_eq!(
            evaluate_auth_basic(Some(&header), Some(&config)),
            AuthBasicDecision::Allow
        );
    }

    #[test]
    fn missing_header_challenges() {
        let config = AuthBasicConfig {
            realm: "x".to_owned(),
            users: vec![AuthBasicUserConfig {
                username: "user".to_owned(),
                password_hash: "{SHA}x".to_owned(),
            }],
        };
        assert_eq!(
            evaluate_auth_basic(None, Some(&config)),
            AuthBasicDecision::Challenge
        );
        assert_eq!(evaluate_auth_basic(None, None), AuthBasicDecision::Inactive);
    }
}
