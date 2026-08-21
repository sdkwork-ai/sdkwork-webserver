use std::net::{IpAddr, SocketAddr};

use axum::http::{HeaderMap, HeaderName};
use ipnet::IpNet;
use sdkwork_webserver_core::{TrustedProxyConfig, TrustedProxyHeader};

const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RealIpError {
    DuplicateHeader,
    HeaderTooLarge,
    InvalidHeader,
    TooManyHops,
}

pub(super) fn resolve_client_ip(
    peer_ip: IpAddr,
    headers: &HeaderMap,
    policy: Option<&TrustedProxyConfig>,
) -> Result<IpAddr, RealIpError> {
    let peer_ip = canonical_ip(peer_ip);
    let Some(policy) = policy else {
        return Ok(peer_ip);
    };
    if !is_trusted(peer_ip, &policy.trusted_cidrs) {
        return Ok(peer_ip);
    }

    let header = match policy.header {
        TrustedProxyHeader::XForwardedFor => &X_FORWARDED_FOR,
    };
    let mut values = headers.get_all(header).iter();
    let Some(value) = values.next() else {
        return Ok(peer_ip);
    };
    if values.next().is_some() {
        return Err(RealIpError::DuplicateHeader);
    }
    if value.as_bytes().len() > policy.max_header_bytes {
        return Err(RealIpError::HeaderTooLarge);
    }
    let value = value.to_str().map_err(|_| RealIpError::InvalidHeader)?;

    let mut resolved = None;
    let mut hops = 0_usize;
    let mut trusted_chain = true;
    for token in value.rsplit(',') {
        hops = hops.checked_add(1).ok_or(RealIpError::TooManyHops)?;
        if hops > policy.max_hops {
            return Err(RealIpError::TooManyHops);
        }
        let address = parse_forwarded_ip(token).ok_or(RealIpError::InvalidHeader)?;
        if policy.recursive && trusted_chain {
            resolved = Some(address);
            trusted_chain = is_trusted(address, &policy.trusted_cidrs);
        } else if resolved.is_none() {
            resolved = Some(address);
        }
    }

    resolved.ok_or(RealIpError::InvalidHeader)
}

fn parse_forwarded_ip(token: &str) -> Option<IpAddr> {
    let token = token.trim_matches([' ', '\t']);
    if token.is_empty() {
        return None;
    }
    token
        .parse::<IpAddr>()
        .ok()
        .or_else(|| token.parse::<SocketAddr>().ok().map(|address| address.ip()))
        .or_else(|| {
            token
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
                .and_then(|value| value.parse::<IpAddr>().ok())
        })
        .map(canonical_ip)
}

pub(super) fn is_trusted(address: IpAddr, networks: &[IpNet]) -> bool {
    let address = canonical_ip(address);
    networks.iter().any(|network| {
        network.contains(&address)
            || match address {
                IpAddr::V4(address) => network.contains(&IpAddr::V6(address.to_ipv6_mapped())),
                IpAddr::V6(_) => false,
            }
    })
}

pub(super) fn canonical_ip(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map_or(IpAddr::V6(address), IpAddr::V4),
        address => address,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn policy(cidrs: &[&str], recursive: bool, max_hops: usize) -> TrustedProxyConfig {
        TrustedProxyConfig {
            trusted_cidrs: cidrs
                .iter()
                .map(|cidr| cidr.parse().expect("valid test CIDR"))
                .collect(),
            header: TrustedProxyHeader::XForwardedFor,
            recursive,
            max_hops,
            max_header_bytes: 4_096,
        }
    }

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            X_FORWARDED_FOR,
            HeaderValue::from_str(value).expect("valid test Header value"),
        );
        headers
    }

    #[test]
    fn omitted_policy_and_untrusted_peer_ignore_forwarding_metadata() {
        let peer = "192.0.2.10".parse().unwrap();
        let source = headers("203.0.113.20");
        assert_eq!(resolve_client_ip(peer, &source, None), Ok(peer));
        assert_eq!(
            resolve_client_ip(peer, &source, Some(&policy(&["10.0.0.0/8"], true, 16))),
            Ok(peer)
        );
    }

    #[test]
    fn non_recursive_mode_uses_rightmost_address_and_accepts_nginx_ports() {
        let source = headers("203.0.113.20, 10.0.0.8:8443");
        assert_eq!(
            resolve_client_ip(
                "127.0.0.1".parse().unwrap(),
                &source,
                Some(&policy(&["127.0.0.0/8"], false, 16))
            ),
            Ok("10.0.0.8".parse().unwrap())
        );

        let source = headers("[2001:db8::8]:443");
        assert_eq!(
            resolve_client_ip(
                "127.0.0.1".parse().unwrap(),
                &source,
                Some(&policy(&["127.0.0.0/8"], false, 16))
            ),
            Ok("2001:db8::8".parse().unwrap())
        );
    }

    #[test]
    fn recursive_mode_skips_trusted_hops_from_right_to_left() {
        let source = headers("203.0.113.20, 10.0.0.7, 10.0.0.8");
        assert_eq!(
            resolve_client_ip(
                "127.0.0.1".parse().unwrap(),
                &source,
                Some(&policy(&["127.0.0.0/8", "10.0.0.0/8"], true, 16))
            ),
            Ok("203.0.113.20".parse().unwrap())
        );
    }

    #[test]
    fn recursive_all_trusted_chain_uses_leftmost_address() {
        let source = headers("10.0.0.6, 10.0.0.7, 10.0.0.8");
        assert_eq!(
            resolve_client_ip(
                "127.0.0.1".parse().unwrap(),
                &source,
                Some(&policy(&["127.0.0.0/8", "10.0.0.0/8"], true, 16))
            ),
            Ok("10.0.0.6".parse().unwrap())
        );
    }

    #[test]
    fn trusted_entry_rejects_duplicate_malformed_over_budget_and_over_hop_headers() {
        let peer = "127.0.0.1".parse().unwrap();
        let mut duplicate = headers("203.0.113.20");
        duplicate.append(X_FORWARDED_FOR, HeaderValue::from_static("203.0.113.21"));
        assert_eq!(
            resolve_client_ip(peer, &duplicate, Some(&policy(&["127.0.0.0/8"], true, 16))),
            Err(RealIpError::DuplicateHeader)
        );
        assert_eq!(
            resolve_client_ip(
                peer,
                &headers("invalid, 203.0.113.20, 10.0.0.8"),
                Some(&policy(&["127.0.0.0/8"], true, 16))
            ),
            Err(RealIpError::InvalidHeader)
        );
        for malformed in ["", "fe80::1%eth0"] {
            assert_eq!(
                resolve_client_ip(
                    peer,
                    &headers(malformed),
                    Some(&policy(&["127.0.0.0/8"], true, 16))
                ),
                Err(RealIpError::InvalidHeader)
            );
        }
        let mut non_text = HeaderMap::new();
        non_text.insert(
            X_FORWARDED_FOR,
            HeaderValue::from_bytes(&[0x80]).expect("valid opaque Header bytes"),
        );
        assert_eq!(
            resolve_client_ip(peer, &non_text, Some(&policy(&["127.0.0.0/8"], true, 16))),
            Err(RealIpError::InvalidHeader)
        );
        assert_eq!(
            resolve_client_ip(
                peer,
                &headers("203.0.113.20, 203.0.113.21"),
                Some(&policy(&["127.0.0.0/8"], true, 1))
            ),
            Err(RealIpError::TooManyHops)
        );

        let mut small = policy(&["127.0.0.0/8"], true, 16);
        small.max_header_bytes = 8;
        assert_eq!(
            resolve_client_ip(peer, &headers("203.0.113.20"), Some(&small)),
            Err(RealIpError::HeaderTooLarge)
        );
    }

    #[test]
    fn mapped_ipv4_peer_matches_ipv4_trust_network() {
        assert_eq!(
            resolve_client_ip(
                "[::ffff:127.0.0.1]:443".parse::<SocketAddr>().unwrap().ip(),
                &headers("203.0.113.20"),
                Some(&policy(&["127.0.0.0/8"], false, 16))
            ),
            Ok("203.0.113.20".parse().unwrap())
        );
        assert_eq!(
            resolve_client_ip(
                "127.0.0.1".parse().unwrap(),
                &headers("203.0.113.20"),
                Some(&policy(&["::ffff:127.0.0.0/104"], false, 16))
            ),
            Ok("203.0.113.20".parse().unwrap())
        );
    }
}
