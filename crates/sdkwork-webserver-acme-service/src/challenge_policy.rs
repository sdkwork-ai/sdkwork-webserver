//! Challenge-method policy: the one place that decides HTTP-01 vs DNS-01.
//!
//! Three inputs decide the challenge, and nothing else does:
//!
//! 1. **The declared method** — the operator's intent, mirroring the control
//!    plane's `webserver_tls_policy.challenge_method` vocabulary
//!    (`AUTO` | `HTTP_01` | `DNS_01`).
//! 2. **The identifier set** — a wildcard name can only ever be proven with
//!    DNS-01. No exact name needs DNS-01.
//! 3. **What the deployment can actually do** — an HTTP-01 webroot and/or a DNS
//!    account covering every identifier.
//!
//! The default (`AUTO`) is the product contract:
//!
//! | Identifiers | Resolved challenge |
//! | --- | --- |
//! | any wildcard | **DNS-01** (fail closed when no DNS account covers the zone) |
//! | exact only | **HTTP-01** whenever a webroot is configured |
//! | exact only, no webroot | **DNS-01** when a DNS account covers every identifier |
//! | exact only, neither available | fail closed with both remedies named |
//!
//! Previously this decision lived as an `if` at the call site, which meant a
//! deployment that never attached a DNS registry silently took the HTTP-01
//! branch for *every* request — and a wildcard then failed deep inside the ACME
//! order instead of at the configuration boundary. Keeping the rule in one
//! pure function makes it testable without an ACME server, and makes the two
//! availability inputs impossible to forget.

use crate::{AcmeServiceError, AcmeServiceResult};

/// Operator-declared challenge method (`webserver_tls_policy.challenge_method`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeclaredChallengeMethod {
    /// Derive from the identifiers: HTTP-01 for exact names, DNS-01 for wildcards.
    Auto,
    /// Force HTTP-01. Wildcards are rejected: the CA cannot validate them this way.
    Http01,
    /// Force DNS-01. Requires a DNS account covering every identifier.
    Dns01,
}

/// The wire/DDL spelling of [`DeclaredChallengeMethod::Auto`].
pub const CHALLENGE_METHOD_AUTO: &str = "AUTO";
/// The wire/DDL spelling of [`DeclaredChallengeMethod::Http01`].
pub const CHALLENGE_METHOD_HTTP_01: &str = "HTTP_01";
/// The wire/DDL spelling of [`DeclaredChallengeMethod::Dns01`].
pub const CHALLENGE_METHOD_DNS_01: &str = "DNS_01";

impl DeclaredChallengeMethod {
    /// Parses the control-plane vocabulary. Case-insensitive and surrounding
    /// whitespace is ignored so a TOML/env value never fails on cosmetics.
    pub fn parse(raw: &str) -> AcmeServiceResult<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            CHALLENGE_METHOD_AUTO => Ok(Self::Auto),
            CHALLENGE_METHOD_HTTP_01 => Ok(Self::Http01),
            CHALLENGE_METHOD_DNS_01 => Ok(Self::Dns01),
            other => Err(AcmeServiceError::config(format!(
                "invalid ACME challenge method `{other}`; expected {CHALLENGE_METHOD_AUTO}, \
                 {CHALLENGE_METHOD_HTTP_01}, or {CHALLENGE_METHOD_DNS_01}"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => CHALLENGE_METHOD_AUTO,
            Self::Http01 => CHALLENGE_METHOD_HTTP_01,
            Self::Dns01 => CHALLENGE_METHOD_DNS_01,
        }
    }
}

impl Default for DeclaredChallengeMethod {
    fn default() -> Self {
        Self::Auto
    }
}

/// The challenge the issuer will actually use for one order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedChallenge {
    Http01,
    Dns01,
}

impl ResolvedChallenge {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http01 => "HTTP_01",
            Self::Dns01 => "DNS_01",
        }
    }
}

/// What the running deployment can actually do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChallengeAvailability {
    /// A webroot is configured (`SDKWORK_WEBSERVER_ACME_WEBROOT`).
    pub http01_webroot_configured: bool,
    /// Cloud DNS accounts cover **every** identifier of this order.
    pub dns01_accounts_cover_all: bool,
}

/// True when any identifier is a wildcard (`*.` in the leading label).
///
/// The one implementation of this predicate in the crate: the ACME order path
/// and the policy path must agree, or a wildcard order would be routed to
/// HTTP-01 and rejected by the CA instead of by the policy. Tolerant of a
/// leading dot and of ASCII case, and deliberately refuses to see a mid-name
/// `*` as a wildcard: `a.*.example.com` is not a name any CA will issue.
pub(crate) fn is_wildcard_identifier(hostname: &str) -> bool {
    hostname
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .starts_with("*.")
}

pub fn contains_wildcard_identifier(hostnames: &[String]) -> bool {
    hostnames
        .iter()
        .any(|hostname| is_wildcard_identifier(hostname))
}

/// Resolves the challenge for one order.
///
/// Fails closed with an actionable message rather than letting the order reach
/// the CA and die there: a wildcard without a DNS account is a *configuration*
/// gap, not an ACME error, and saying so is the difference between a five
/// minute fix and a support ticket.
pub fn resolve_challenge_method(
    declared: DeclaredChallengeMethod,
    hostnames: &[String],
    availability: ChallengeAvailability,
) -> AcmeServiceResult<ResolvedChallenge> {
    let wildcard = contains_wildcard_identifier(hostnames);
    match declared {
        DeclaredChallengeMethod::Http01 => {
            if wildcard {
                return Err(AcmeServiceError::validation(
                    "wildcard identifiers require the DNS-01 validation method; \
                     HTTP-01 cannot prove control of a wildcard name",
                ));
            }
            require_http01_available(availability)?;
            Ok(ResolvedChallenge::Http01)
        }
        DeclaredChallengeMethod::Dns01 => {
            require_dns01_available(availability)?;
            Ok(ResolvedChallenge::Dns01)
        }
        DeclaredChallengeMethod::Auto => {
            if wildcard {
                require_dns01_available(availability)?;
                return Ok(ResolvedChallenge::Dns01);
            }
            if availability.http01_webroot_configured {
                return Ok(ResolvedChallenge::Http01);
            }
            if availability.dns01_accounts_cover_all {
                return Ok(ResolvedChallenge::Dns01);
            }
            Err(AcmeServiceError::config(
                "no ACME challenge method is available for this certificate: \
                 set SDKWORK_WEBSERVER_ACME_WEBROOT to serve HTTP-01 challenges, \
                 or associate every identifier's zone with a DNS provider account for DNS-01",
            ))
        }
    }
}

fn require_http01_available(availability: ChallengeAvailability) -> AcmeServiceResult<()> {
    if availability.http01_webroot_configured {
        return Ok(());
    }
    Err(AcmeServiceError::config(
        "HTTP-01 issuance requires SDKWORK_WEBSERVER_ACME_WEBROOT to point at the webroot \
         the edge serves /.well-known/acme-challenge/ from",
    ))
}

fn require_dns01_available(availability: ChallengeAvailability) -> AcmeServiceResult<()> {
    if availability.dns01_accounts_cover_all {
        return Ok(());
    }
    Err(AcmeServiceError::config(
        "DNS-01 issuance requires a DNS provider account covering every identifier's zone; \
         configure the ACME DNS accounts so wildcard certificates can be issued",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact() -> Vec<String> {
        vec!["www.example.com".to_string()]
    }

    fn wildcard_and_apex() -> Vec<String> {
        vec!["*.example.com".to_string(), "example.com".to_string()]
    }

    fn http_only() -> ChallengeAvailability {
        ChallengeAvailability {
            http01_webroot_configured: true,
            dns01_accounts_cover_all: false,
        }
    }

    fn dns_only() -> ChallengeAvailability {
        ChallengeAvailability {
            http01_webroot_configured: false,
            dns01_accounts_cover_all: true,
        }
    }

    fn both() -> ChallengeAvailability {
        ChallengeAvailability {
            http01_webroot_configured: true,
            dns01_accounts_cover_all: true,
        }
    }

    fn neither() -> ChallengeAvailability {
        ChallengeAvailability {
            http01_webroot_configured: false,
            dns01_accounts_cover_all: false,
        }
    }

    #[test]
    fn declared_method_parses_the_control_plane_vocabulary() {
        assert_eq!(
            DeclaredChallengeMethod::parse("AUTO").unwrap(),
            DeclaredChallengeMethod::Auto
        );
        assert_eq!(
            DeclaredChallengeMethod::parse(" http_01 ").unwrap(),
            DeclaredChallengeMethod::Http01
        );
        assert_eq!(
            DeclaredChallengeMethod::parse("dns_01").unwrap(),
            DeclaredChallengeMethod::Dns01
        );
        assert!(DeclaredChallengeMethod::parse("TLS_ALPN_01").is_err());
        assert_eq!(DeclaredChallengeMethod::default().as_str(), "AUTO");
    }

    /// The product default, stated as a table: single-domain ⇒ HTTP-01.
    #[test]
    fn auto_prefers_http01_for_exact_identifiers() {
        assert_eq!(
            resolve_challenge_method(DeclaredChallengeMethod::Auto, &exact(), both()).unwrap(),
            ResolvedChallenge::Http01,
            "an exact-only order must default to HTTP-01 even when DNS-01 is available"
        );
        assert_eq!(
            resolve_challenge_method(DeclaredChallengeMethod::Auto, &exact(), http_only()).unwrap(),
            ResolvedChallenge::Http01
        );
    }

    /// The other half of the product default: wildcard ⇒ DNS-01.
    #[test]
    fn auto_requires_dns01_for_wildcards() {
        assert_eq!(
            resolve_challenge_method(DeclaredChallengeMethod::Auto, &wildcard_and_apex(), both())
                .unwrap(),
            ResolvedChallenge::Dns01
        );
        assert_eq!(
            resolve_challenge_method(
                DeclaredChallengeMethod::Auto,
                &wildcard_and_apex(),
                dns_only()
            )
            .unwrap(),
            ResolvedChallenge::Dns01
        );
    }

    #[test]
    fn auto_falls_back_to_dns01_for_exact_identifiers_without_a_webroot() {
        assert_eq!(
            resolve_challenge_method(DeclaredChallengeMethod::Auto, &exact(), dns_only()).unwrap(),
            ResolvedChallenge::Dns01
        );
    }

    #[test]
    fn auto_fails_closed_naming_both_remedies_when_nothing_is_available() {
        let error = resolve_challenge_method(DeclaredChallengeMethod::Auto, &exact(), neither())
            .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("ACME challenge method is available"),
            "{message}"
        );
        assert!(
            message.contains("SDKWORK_WEBSERVER_ACME_WEBROOT"),
            "{message}"
        );
        assert!(message.contains("DNS provider account"), "{message}");
    }

    #[test]
    fn wildcard_without_dns_accounts_fails_at_the_configuration_boundary() {
        for availability in [http_only(), neither()] {
            let error = resolve_challenge_method(
                DeclaredChallengeMethod::Auto,
                &wildcard_and_apex(),
                availability,
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("DNS-01 issuance requires"),
                "{error}"
            );
        }
    }

    #[test]
    fn explicit_http01_rejects_wildcards_instead_of_failing_at_the_ca() {
        let error = resolve_challenge_method(
            DeclaredChallengeMethod::Http01,
            &wildcard_and_apex(),
            both(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("wildcard identifiers require"),
            "{error}"
        );
    }

    #[test]
    fn explicit_http01_needs_a_webroot() {
        let error = resolve_challenge_method(DeclaredChallengeMethod::Http01, &exact(), dns_only())
            .unwrap_err();
        assert!(
            error.to_string().contains("SDKWORK_WEBSERVER_ACME_WEBROOT"),
            "{error}"
        );
    }

    #[test]
    fn explicit_dns01_needs_covering_accounts_even_for_exact_identifiers() {
        assert_eq!(
            resolve_challenge_method(DeclaredChallengeMethod::Dns01, &exact(), dns_only()).unwrap(),
            ResolvedChallenge::Dns01
        );
        let error = resolve_challenge_method(DeclaredChallengeMethod::Dns01, &exact(), http_only())
            .unwrap_err();
        assert!(
            error.to_string().contains("DNS provider account"),
            "{error}"
        );
    }

    #[test]
    fn wildcard_detection_matches_the_engine_identifier_rules() {
        assert!(contains_wildcard_identifier(&["*.example.com".to_string()]));
        assert!(contains_wildcard_identifier(&["*.EXAMPLE.COM".to_string()]));
        assert!(contains_wildcard_identifier(
            &["*.example.com.".to_string()]
        ));
        // A leading dot is tolerated because some callers normalize a trailing
        // dot into a leading one before reaching the engine.
        assert!(contains_wildcard_identifier(
            &[".*.example.com".to_string()]
        ));
        assert!(!contains_wildcard_identifier(&["example.com".to_string()]));
        assert!(!contains_wildcard_identifier(&[
            "a.*.example.com".to_string()
        ]));
        assert!(!contains_wildcard_identifier(&["*".to_string()]));
        assert!(!contains_wildcard_identifier(&[]));
    }

    /// Only one wildcard among many exact names still forces DNS-01.
    #[test]
    fn one_wildcard_forces_dns01_for_the_whole_order() {
        let mixed = vec![
            "www.example.com".to_string(),
            "*.shop.example.com".to_string(),
        ];
        assert_eq!(
            resolve_challenge_method(DeclaredChallengeMethod::Auto, &mixed, both()).unwrap(),
            ResolvedChallenge::Dns01
        );
    }
}
