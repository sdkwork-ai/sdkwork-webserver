use std::net::IpAddr;

use axum::http::{HeaderMap, HeaderName};
use sdkwork_webserver_core::TrustedProxyConfig;
use sdkwork_webserver_delivery_runtime::WebsiteDeliveryScheme;

use super::real_ip::{canonical_ip, is_trusted};

const X_FORWARDED_PROTO: HeaderName = HeaderName::from_static("x-forwarded-proto");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForwardedSchemeError {
    DuplicateHeader,
    HeaderTooLarge,
    InvalidHeader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResolvedRequestScheme {
    Http,
    Https,
}

impl ResolvedRequestScheme {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    pub(super) const fn website_delivery_scheme(self) -> WebsiteDeliveryScheme {
        match self {
            Self::Http => WebsiteDeliveryScheme::Http,
            Self::Https => WebsiteDeliveryScheme::Https,
        }
    }
}

pub(super) fn resolve_request_scheme(
    peer_ip: IpAddr,
    headers: &HeaderMap,
    policy: Option<&TrustedProxyConfig>,
    is_tls: bool,
) -> Result<ResolvedRequestScheme, ForwardedSchemeError> {
    if is_tls {
        return Ok(ResolvedRequestScheme::Https);
    }

    let peer_ip = canonical_ip(peer_ip);
    let Some(policy) = policy else {
        return Ok(ResolvedRequestScheme::Http);
    };
    if !is_trusted(peer_ip, &policy.trusted_cidrs) {
        return Ok(ResolvedRequestScheme::Http);
    }

    let mut values = headers.get_all(&X_FORWARDED_PROTO).iter();
    let Some(value) = values.next() else {
        return Ok(ResolvedRequestScheme::Http);
    };
    if values.next().is_some() {
        return Err(ForwardedSchemeError::DuplicateHeader);
    }
    if value.as_bytes().len() > policy.max_header_bytes {
        return Err(ForwardedSchemeError::HeaderTooLarge);
    }
    let value = value
        .to_str()
        .map_err(|_| ForwardedSchemeError::InvalidHeader)?;
    if value.eq_ignore_ascii_case("http") {
        Ok(ResolvedRequestScheme::Http)
    } else if value.eq_ignore_ascii_case("https") {
        Ok(ResolvedRequestScheme::Https)
    } else {
        Err(ForwardedSchemeError::InvalidHeader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use sdkwork_webserver_core::TrustedProxyHeader;

    fn policy(cidrs: &[&str]) -> TrustedProxyConfig {
        TrustedProxyConfig {
            trusted_cidrs: cidrs
                .iter()
                .map(|cidr| cidr.parse().expect("valid test CIDR"))
                .collect(),
            header: TrustedProxyHeader::XForwardedFor,
            recursive: true,
            max_hops: 16,
            max_header_bytes: 4_096,
        }
    }

    fn headers(value: HeaderValue) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(X_FORWARDED_PROTO, value);
        headers
    }

    #[test]
    fn omitted_policy_and_untrusted_peer_ignore_forwarded_scheme() {
        let peer = "192.0.2.10".parse().unwrap();
        let source = headers(HeaderValue::from_static("https"));
        assert_eq!(
            resolve_request_scheme(peer, &source, None, false),
            Ok(ResolvedRequestScheme::Http)
        );
        assert_eq!(
            resolve_request_scheme(peer, &source, Some(&policy(&["10.0.0.0/8"])), false),
            Ok(ResolvedRequestScheme::Http)
        );
    }

    #[test]
    fn trusted_peer_accepts_one_explicit_http_or_https_value() {
        let peer = "127.0.0.1".parse().unwrap();
        let policy = policy(&["127.0.0.0/8"]);
        assert_eq!(
            resolve_request_scheme(
                peer,
                &headers(HeaderValue::from_static("http")),
                Some(&policy),
                false
            ),
            Ok(ResolvedRequestScheme::Http)
        );
        assert_eq!(
            resolve_request_scheme(
                peer,
                &headers(HeaderValue::from_static("HTTPS")),
                Some(&policy),
                false
            ),
            Ok(ResolvedRequestScheme::Https)
        );
    }

    #[test]
    fn trusted_peer_without_forwarded_scheme_uses_listener_transport() {
        assert_eq!(
            resolve_request_scheme(
                "127.0.0.1".parse().unwrap(),
                &HeaderMap::new(),
                Some(&policy(&["127.0.0.0/8"])),
                false
            ),
            Ok(ResolvedRequestScheme::Http)
        );
    }

    #[test]
    fn trusted_peer_rejects_duplicate_chained_non_text_and_over_budget_values() {
        let peer = "127.0.0.1".parse().unwrap();
        let policy = policy(&["127.0.0.0/8"]);
        let mut duplicate = headers(HeaderValue::from_static("https"));
        duplicate.append(X_FORWARDED_PROTO, HeaderValue::from_static("https"));
        assert_eq!(
            resolve_request_scheme(peer, &duplicate, Some(&policy), false),
            Err(ForwardedSchemeError::DuplicateHeader)
        );

        for invalid in ["https,http", " https", "https ", "wss", ""] {
            assert_eq!(
                resolve_request_scheme(
                    peer,
                    &headers(HeaderValue::from_str(invalid).unwrap()),
                    Some(&policy),
                    false
                ),
                Err(ForwardedSchemeError::InvalidHeader)
            );
        }

        assert_eq!(
            resolve_request_scheme(
                peer,
                &headers(HeaderValue::from_bytes(&[0x80]).unwrap()),
                Some(&policy),
                false
            ),
            Err(ForwardedSchemeError::InvalidHeader)
        );

        let mut small = policy;
        small.max_header_bytes = 4;
        assert_eq!(
            resolve_request_scheme(
                peer,
                &headers(HeaderValue::from_static("https")),
                Some(&small),
                false
            ),
            Err(ForwardedSchemeError::HeaderTooLarge)
        );
    }

    #[test]
    fn native_tls_cannot_be_downgraded_by_forwarding_metadata() {
        let peer = "127.0.0.1".parse().unwrap();
        let policy = policy(&["127.0.0.0/8"]);
        let mut headers = headers(HeaderValue::from_static("http"));
        headers.append(X_FORWARDED_PROTO, HeaderValue::from_static("invalid"));
        assert_eq!(
            resolve_request_scheme(peer, &headers, Some(&policy), true),
            Ok(ResolvedRequestScheme::Https)
        );
    }

    #[test]
    fn mapped_ipv4_peer_matches_ipv4_trust_network() {
        assert_eq!(
            resolve_request_scheme(
                "::ffff:127.0.0.1".parse().unwrap(),
                &headers(HeaderValue::from_static("https")),
                Some(&policy(&["127.0.0.0/8"])),
                false
            ),
            Ok(ResolvedRequestScheme::Https)
        );
    }
}
