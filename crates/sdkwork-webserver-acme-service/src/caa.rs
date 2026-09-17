//! ACME directory profile: the CA identity a CAA check matches against, plus
//! the certificate shapes that directory can actually issue.
//!
//! RFC 8659 §4.2 matches a CAA `issue`/`issuewild` property against the CA's
//! *issuer-domain-name*, never against the ACME API hostname. Let's Encrypt's
//! identity is `letsencrypt.org` while its API lives at
//! `acme-v02.api.letsencrypt.org`, and ZeroSSL's identities are the Sectigo
//! family rather than `zerossl.com`, so the mapping cannot be derived from the
//! API host and cannot be guessed from the product name.
//!
//! The authoritative source is the ACME directory metadata's `caaIdentities`
//! field (RFC 8555 §7.1.1), which is a list rather than a single value.
//! `instant-acme` 0.8 keeps `Directory`/`Meta` crate-private and exposes no
//! accessor, so the table below is the current source of truth. When a future
//! release exposes the metadata this function should read it and the table
//! should become the offline fallback. An unknown directory returns `None`,
//! which callers must treat as "not evaluable" rather than as an authorization.

/// What an ACME directory is able to issue, and the CA identity CAA checks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcmeDirectoryProfile {
    /// The CAA `issuer-domain-name` values this CA recognizes as itself
    /// (`meta.caaIdentities`). An `issue`/`issuewild` property matching any of
    /// these authorizes the CA.
    pub issuer_domains: &'static [&'static str],
    /// Whether the directory issues certificates for wildcard identifiers.
    /// A hostname-only API cannot express `*.example.com`, so a wildcard order
    /// must be refused before it burns CA rate-limit budget.
    pub supports_wildcard: bool,
    /// Whether one certificate may carry more than one DNS identifier.
    pub supports_multi_domain: bool,
}

/// The profile for a supported ACME directory, or `None` when unknown.
///
/// The URL is normalized before matching because operators write directory URLs
/// inconsistently: the scheme and host are case-insensitive, and a trailing `/`
/// is not significant.
pub fn acme_directory_profile(directory_url: &str) -> Option<&'static AcmeDirectoryProfile> {
    let normalized = normalize_directory_url(directory_url)?;
    ACME_DIRECTORY_PROFILES
        .iter()
        .find_map(|(directory, profile)| (*directory == normalized).then_some(profile))
}

/// Longest directory URL considered, matching `AcmeConfig`'s own ceiling.
const MAX_DIRECTORY_URL_BYTES: usize = 2_048;

/// Normalizes a directory URL to `scheme://host[:port]/path` for table matching.
///
/// Only the scheme and host are case-folded. The path is left byte-exact: ACME
/// directory paths are case-sensitive (`/v2/DV90` is not `/v2/dv90`), so folding
/// the whole string would silently stop matching real directories.
///
/// A trailing `/` is removed, and a query string is preserved because it can
/// make a directory URL distinctive.
fn normalize_directory_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > MAX_DIRECTORY_URL_BYTES {
        return None;
    }
    let parsed = url::Url::parse(raw).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let mut normalized = format!("{}://{host}", parsed.scheme().to_ascii_lowercase());
    if let Some(port) = parsed.port() {
        normalized.push_str(&format!(":{port}"));
    }
    normalized.push_str(parsed.path().trim_end_matches('/'));
    if let Some(query) = parsed.query() {
        normalized.push('?');
        normalized.push_str(query);
    }
    Some(normalized)
}

/// `(directory URL, profile)` pairs for the directories this engine can be
/// pointed at.
///
/// Every entry is a CA whose identity and capabilities are stated by the CA
/// itself or by the ACME client ecosystem's maintained server list. Guessing is
/// worse than not knowing: a wrong `issuer_domains` value authorizes the wrong
/// CA, whereas a missing table entry only degrades to "not evaluable".
const ACME_DIRECTORY_PROFILES: &[(&str, AcmeDirectoryProfile)] = &[
    // https://letsencrypt.org/docs/certificate-compatibility/ and the
    // `caaIdentities` field of both directories' metadata.
    (
        "https://acme-v02.api.letsencrypt.org/directory",
        AcmeDirectoryProfile {
            issuer_domains: &["letsencrypt.org"],
            supports_wildcard: true,
            supports_multi_domain: true,
        },
    ),
    (
        "https://acme-staging-v02.api.letsencrypt.org/directory",
        AcmeDirectoryProfile {
            issuer_domains: &["letsencrypt.org"],
            supports_wildcard: true,
            supports_multi_domain: true,
        },
    ),
    // https://developers.google.com/trust-services/acme — the production
    // directory's `meta.caaIdentities` is exactly `["pki.goog"]`.
    (
        "https://dv.acme-v02.api.pki.goog/directory",
        AcmeDirectoryProfile {
            issuer_domains: &["pki.goog"],
            supports_wildcard: true,
            supports_multi_domain: true,
        },
    ),
    (
        "https://dv.acme-v02.test-api.pki.goog/directory",
        AcmeDirectoryProfile {
            issuer_domains: &["pki.goog"],
            supports_wildcard: true,
            supports_multi_domain: true,
        },
    ),
    // ZeroSSL issues through Sectigo/Entrust infrastructure and recognizes the
    // whole family as itself; `zerossl.com` is deliberately NOT an identity.
    (
        "https://acme.zerossl.com/v2/DV90",
        AcmeDirectoryProfile {
            issuer_domains: &[
                "sectigo.com",
                "trust-provider.com",
                "usertrust.com",
                "comodoca.com",
                "comodo.com",
                "entrust.net",
                "affirmtrust.com",
            ],
            supports_wildcard: true,
            supports_multi_domain: true,
        },
    ),
    // Buypass explicitly does not support wildcard identifiers.
    (
        "https://api.buypass.com/acme/directory",
        AcmeDirectoryProfile {
            issuer_domains: &["buypass.com"],
            supports_wildcard: false,
            supports_multi_domain: true,
        },
    ),
    (
        "https://api.test4.buypass.no/acme/directory",
        AcmeDirectoryProfile {
            issuer_domains: &["buypass.com"],
            supports_wildcard: false,
            supports_multi_domain: true,
        },
    ),
    // SSL.com's ACME issuance is single-domain only.
    (
        "https://acme.ssl.com/sslcom-dv-ecc",
        AcmeDirectoryProfile {
            issuer_domains: &["ssl.com"],
            supports_wildcard: false,
            supports_multi_domain: false,
        },
    ),
    (
        "https://acme.ssl.com/sslcom-dv-rsa",
        AcmeDirectoryProfile {
            issuer_domains: &["ssl.com"],
            supports_wildcard: false,
            supports_multi_domain: false,
        },
    ),
];

#[cfg(test)]
mod tests {
    use super::acme_directory_profile;

    #[test]
    fn resolves_the_ca_identity_from_the_directory_url() {
        assert_eq!(
            acme_directory_profile("https://acme-v02.api.letsencrypt.org/directory")
                .expect("Let's Encrypt production")
                .issuer_domains,
            ["letsencrypt.org"]
        );
        assert_eq!(
            acme_directory_profile("https://acme-staging-v02.api.letsencrypt.org/directory")
                .expect("Let's Encrypt staging")
                .issuer_domains,
            ["letsencrypt.org"]
        );
        assert_eq!(
            acme_directory_profile("https://dv.acme-v02.api.pki.goog/directory")
                .expect("Google Trust Services")
                .issuer_domains,
            ["pki.goog"]
        );
    }

    #[test]
    fn a_ca_with_several_recognized_identities_keeps_all_of_them() {
        // ZeroSSL authorizes the Sectigo/Entrust family. Collapsing this to a
        // single value would refuse issuance for a correctly configured zone
        // that pins `sectigo.com`.
        let profile = acme_directory_profile("https://acme.zerossl.com/v2/DV90").expect("ZeroSSL");
        assert!(profile.issuer_domains.contains(&"sectigo.com"));
        assert!(profile.issuer_domains.contains(&"affirmtrust.com"));
        // The product name is not one of the CA's CAA identities.
        assert!(!profile.issuer_domains.contains(&"zerossl.com"));
    }

    #[test]
    fn capability_flags_record_what_each_directory_can_actually_issue() {
        let lets_encrypt =
            acme_directory_profile("https://acme-v02.api.letsencrypt.org/directory").expect("LE");
        assert!(lets_encrypt.supports_wildcard);
        assert!(lets_encrypt.supports_multi_domain);

        let buypass =
            acme_directory_profile("https://api.buypass.com/acme/directory").expect("Buypass");
        assert!(!buypass.supports_wildcard);

        let ssl_com =
            acme_directory_profile("https://acme.ssl.com/sslcom-dv-ecc").expect("SSL.com");
        assert!(!ssl_com.supports_wildcard);
        assert!(!ssl_com.supports_multi_domain);
    }

    #[test]
    fn normalization_accepts_the_spellings_operators_actually_write() {
        for spelling in [
            "  HTTPS://ACME-V02.API.LETSENCRYPT.ORG/directory  ",
            "https://acme-v02.api.letsencrypt.org/directory/",
        ] {
            assert!(
                acme_directory_profile(spelling).is_some(),
                "must normalize {spelling}"
            );
        }
    }

    #[test]
    fn a_case_folded_path_does_not_match() {
        // ACME directory paths are case-sensitive. Normalizing the whole URL to
        // lower case would match `/v2/dv90` against the real `/v2/DV90` and
        // silently profile the wrong directory.
        assert!(acme_directory_profile("https://acme.zerossl.com/v2/dv90").is_none());
        assert!(acme_directory_profile("https://acme.zerossl.com/v2/DV90").is_some());
    }

    #[test]
    fn a_non_http_scheme_is_not_a_directory() {
        for not_a_url in ["not-a-url", "acme-v02.api.letsencrypt.org/directory", "://"] {
            assert!(acme_directory_profile(not_a_url).is_none(), "{not_a_url}");
        }
    }

    #[test]
    fn an_unknown_directory_is_not_evaluable_rather_than_authorized() {
        for unknown in [
            "",
            "https://acme.example.test/directory",
            // A lookalike host must not match the Let's Encrypt entry.
            "https://acme-v02.api.letsencrypt.org.evil.test/directory",
            // The API host is not the issuer domain.
            "https://letsencrypt.org",
        ] {
            assert!(acme_directory_profile(unknown).is_none(), "{unknown}");
        }
    }
}
