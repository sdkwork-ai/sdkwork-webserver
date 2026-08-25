use std::net::{IpAddr, Ipv4Addr};

use ipnet::IpNet;

const ALLOWABLE_RESTRICTED_NETWORKS: &[&str] = &[
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

const HARD_FORBIDDEN_NETWORKS: &[&str] = &[
    "0.0.0.0/8",
    "100.100.100.200/32",
    "169.254.169.254/32",
    "192.0.0.0/24",
    "192.0.2.0/24",
    "192.88.99.0/24",
    "198.18.0.0/15",
    "198.51.100.0/24",
    "203.0.113.0/24",
    "224.0.0.0/4",
    "240.0.0.0/4",
    "::/128",
    "64:ff9b:1::/48",
    "100::/64",
    "2001::/32",
    "2001:2::/48",
    "2001:10::/28",
    "2001:20::/28",
    "2001:db8::/32",
    "3fff::/20",
    "5f00::/16",
    "fd00:ec2::254/128",
    "fec0::/10",
    "ff00::/8",
];

pub fn upstream_ip_is_allowed(ip: IpAddr, allowed_cidrs: &[IpNet]) -> bool {
    let ip = embedded_ipv4(ip).map_or(ip, IpAddr::V4);
    if network_list_contains(HARD_FORBIDDEN_NETWORKS, ip) {
        return false;
    }
    if network_list_contains(ALLOWABLE_RESTRICTED_NETWORKS, ip) {
        return allowed_cidrs
            .iter()
            .filter(|network| is_supported_upstream_allowed_cidr(network))
            .any(|network| network.contains(&ip));
    }
    true
}

/// CIDRs authorized when an operator declares a hostname upstream target
/// (for example Docker Compose service DNS `gateway:3900`). Hard-forbidden
/// ranges remain blocked by [`upstream_ip_is_allowed`].
pub fn hostname_upstream_allowed_cidrs() -> Vec<IpNet> {
    ALLOWABLE_RESTRICTED_NETWORKS
        .iter()
        .map(|value| {
            value
                .parse::<IpNet>()
                .expect("static restricted network is valid")
        })
        .collect()
}

pub fn is_supported_upstream_allowed_cidr(network: &IpNet) -> bool {
    ALLOWABLE_RESTRICTED_NETWORKS.iter().any(|candidate| {
        candidate
            .parse::<IpNet>()
            .expect("static restricted network is valid")
            .contains(network)
    })
}

fn network_list_contains(networks: &[&str], ip: IpAddr) -> bool {
    networks.iter().any(|network| {
        network
            .parse::<IpNet>()
            .expect("static special-use network is valid")
            .contains(&ip)
    })
}

fn embedded_ipv4(ip: IpAddr) -> Option<Ipv4Addr> {
    let IpAddr::V6(ipv6) = ip else {
        return None;
    };
    if let Some(mapped) = ipv6.to_ipv4_mapped() {
        return Some(mapped);
    }

    let octets = ipv6.octets();
    let is_ipv4_compatible = !ipv6.is_unspecified()
        && !ipv6.is_loopback()
        && octets[..12] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    if is_ipv4_compatible {
        return Some(Ipv4Addr::new(
            octets[12], octets[13], octets[14], octets[15],
        ));
    }
    let is_well_known_nat64 = octets[..12] == [0x00, 0x64, 0xff, 0x9b, 0, 0, 0, 0, 0, 0, 0, 0];
    if is_well_known_nat64 {
        return Some(Ipv4Addr::new(
            octets[12], octets[13], octets[14], octets[15],
        ));
    }
    if octets[0] == 0x20 && octets[1] == 0x02 {
        return Some(Ipv4Addr::new(octets[2], octets[3], octets[4], octets[5]));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nets(values: &[&str]) -> Vec<IpNet> {
        values
            .iter()
            .map(|value| value.parse().expect("valid test CIDR"))
            .collect()
    }

    #[test]
    fn hostname_upstream_policy_authorizes_docker_private_ranges() {
        let allowed = hostname_upstream_allowed_cidrs();
        assert!(upstream_ip_is_allowed("192.168.0.5".parse().unwrap(), &allowed));
        assert!(upstream_ip_is_allowed("10.0.0.2".parse().unwrap(), &allowed));
        assert!(upstream_ip_is_allowed("127.0.0.1".parse().unwrap(), &allowed));
        assert!(!upstream_ip_is_allowed("169.254.169.254".parse().unwrap(), &allowed));
    }

    #[test]
    fn defaults_to_public_unicast_and_requires_explicit_private_ranges() {
        assert!(upstream_ip_is_allowed("8.8.8.8".parse().unwrap(), &[]));
        assert!(upstream_ip_is_allowed(
            "2001:4860:4860::8888".parse().unwrap(),
            &[]
        ));
        assert!(!upstream_ip_is_allowed("127.0.0.1".parse().unwrap(), &[]));
        assert!(!upstream_ip_is_allowed("::1".parse().unwrap(), &[]));
        assert!(!upstream_ip_is_allowed("10.1.2.3".parse().unwrap(), &[]));
        assert!(!upstream_ip_is_allowed("fc00::1".parse().unwrap(), &[]));

        let allowed = nets(&["127.0.0.0/8", "::1/128", "10.1.0.0/16", "fc00::/8"]);
        assert!(upstream_ip_is_allowed(
            "127.0.0.1".parse().unwrap(),
            &allowed
        ));
        assert!(upstream_ip_is_allowed("::1".parse().unwrap(), &allowed));
        assert!(upstream_ip_is_allowed(
            "10.1.2.3".parse().unwrap(),
            &allowed
        ));
        assert!(!upstream_ip_is_allowed(
            "10.2.2.3".parse().unwrap(),
            &allowed
        ));
        assert!(upstream_ip_is_allowed("fc00::1".parse().unwrap(), &allowed));
    }

    #[test]
    fn hard_forbidden_and_embedded_private_addresses_cannot_be_allowed_broadly() {
        let broad = nets(&["127.0.0.0/8", "10.0.0.0/8", "169.254.0.0/16"]);
        for address in [
            "0.0.0.0",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "2001:db8::1",
            "ff02::1",
            "::ffff:169.254.169.254",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(
                !upstream_ip_is_allowed(address.parse().unwrap(), &broad),
                "{address}"
            );
        }

        assert!(!upstream_ip_is_allowed(
            "2002:0a00:0001::1".parse().unwrap(),
            &[]
        ));
        assert!(!upstream_ip_is_allowed("::10.0.0.1".parse().unwrap(), &[]));
        assert!(upstream_ip_is_allowed(
            "2002:0a00:0001::1".parse().unwrap(),
            &broad
        ));
        assert!(upstream_ip_is_allowed(
            "::10.0.0.1".parse().unwrap(),
            &broad
        ));
    }

    #[test]
    fn allowlist_networks_must_be_narrow_restricted_subnets() {
        for accepted in ["127.0.0.1/32", "10.2.0.0/16", "100.64.0.0/10", "fc00::/8"] {
            assert!(is_supported_upstream_allowed_cidr(
                &accepted.parse().unwrap()
            ));
        }
        for rejected in ["0.0.0.0/0", "8.8.8.8/32", "192.0.2.0/24", "ff00::/8"] {
            assert!(!is_supported_upstream_allowed_cidr(
                &rejected.parse().unwrap()
            ));
        }
    }

    #[test]
    fn unsupported_allowlist_networks_never_authorize_restricted_addresses() {
        let unsupported = nets(&["0.0.0.0/0", "::/0"]);
        assert!(!upstream_ip_is_allowed(
            "127.0.0.1".parse().unwrap(),
            &unsupported
        ));
        assert!(!upstream_ip_is_allowed(
            "::1".parse().unwrap(),
            &unsupported
        ));
    }
}
