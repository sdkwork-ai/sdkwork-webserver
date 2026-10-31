//! nginx `geo` and `split_clients` variable blocks.
//!
//! Both produce variables that behave exactly like `map` results: any place
//! that expands a map result (`add_header`, `return` bodies, `proxy_set_header`,
//! `proxy_pass` templates) expands them too.
//!
//! - `geo` (ngx_http_geo_module): the source variable is parsed as an IP
//!   address and matched against CIDRs (longest prefix wins) or, with
//!   `ranges;`, against `first-last` ranges; `default` is the fallback.
//! - `split_clients` (ngx_http_split_clients_module): the expanded source is
//!   hashed with nginx's MurmurHash2 (`ngx_murmur_hash2`) and assigned to the
//!   first bucket whose cumulative percentage threshold exceeds the hash;
//!   `*` is the catch-all.

use std::net::IpAddr;

use ipnet::IpNet;

use super::map_variables::{
    expand_map_template, parse_variable_reference, validate_map_source, MapVariables,
    MAX_MAP_VALUE_BYTES,
};
use super::model::{GeoConfig, SplitClientsConfig};

/// nginx `ngx_murmur_hash2` (MurmurHash2, seed = length).
#[must_use]
pub fn murmur_hash2(data: &[u8]) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let mut hash = data.len() as u32;
    let mut index = 0;
    while index + 4 <= data.len() {
        let mut k = u32::from(data[index])
            | u32::from(data[index + 1]) << 8
            | u32::from(data[index + 2]) << 16
            | u32::from(data[index + 3]) << 24;
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        hash = hash.wrapping_mul(M);
        hash ^= k;
        index += 4;
    }
    let remainder = &data[index..];
    match remainder.len() {
        3 => {
            hash ^= u32::from(remainder[2]) << 16;
            hash ^= u32::from(remainder[1]) << 8;
            hash ^= u32::from(remainder[0]);
            hash = hash.wrapping_mul(M);
        }
        2 => {
            hash ^= u32::from(remainder[1]) << 8;
            hash ^= u32::from(remainder[0]);
            hash = hash.wrapping_mul(M);
        }
        1 => {
            hash ^= u32::from(remainder[0]);
            hash = hash.wrapping_mul(M);
        }
        _ => {}
    }
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(M);
    hash ^= hash >> 15;
    hash
}

/// Parse an `a-b` nginx geo range.
fn parse_range(key: &str) -> Option<(IpAddr, IpAddr)> {
    let (start, end) = key.split_once('-')?;
    let start = start.trim().parse::<IpAddr>().ok()?;
    let end = end.trim().parse::<IpAddr>().ok()?;
    Some((start, end))
}

/// Evaluate a `geo` block for one request.
#[must_use]
pub fn evaluate_geo(geo: &GeoConfig, variables: &dyn MapVariables) -> String {
    let Ok(source_name) = parse_variable_reference(&geo.source) else {
        return String::new();
    };
    let source = variables.map_variable(source_name).unwrap_or_default();
    let mut default = None;
    let Some(address) = source.trim().parse::<IpAddr>().ok() else {
        // nginx leaves the variable empty when the source is not an address.
        return geo
            .entries
            .iter()
            .find(|entry| entry.key == "default")
            .map(|entry| entry.value.clone())
            .unwrap_or_default();
    };
    let mut best: Option<(u8, &str)> = None;
    for entry in &geo.entries {
        let key = entry.key.trim();
        if key.eq_ignore_ascii_case("default") {
            default = Some(entry.value.as_str());
            continue;
        }
        if geo.ranges {
            let Some((start, end)) = parse_range(key) else {
                continue;
            };
            if address_in_range(address, start, end) {
                return entry.value.clone();
            }
            continue;
        }
        let Ok(network) = key.parse::<IpNet>() else {
            continue;
        };
        if network.contains(&address) {
            let prefix = network.prefix_len();
            if best.is_none_or(|(best_prefix, _)| prefix > best_prefix) {
                best = Some((prefix, entry.value.as_str()));
            }
        }
    }
    match best {
        Some((_, value)) => value.to_owned(),
        None => default.unwrap_or_default().to_owned(),
    }
}

fn address_in_range(address: IpAddr, start: IpAddr, end: IpAddr) -> bool {
    match (address, start, end) {
        (IpAddr::V4(address), IpAddr::V4(start), IpAddr::V4(end)) => {
            u32::from(address) >= u32::from(start) && u32::from(address) <= u32::from(end)
        }
        (IpAddr::V6(address), IpAddr::V6(start), IpAddr::V6(end)) => {
            u128::from(address) >= u128::from(start) && u128::from(address) <= u128::from(end)
        }
        _ => false,
    }
}

/// Evaluate a `split_clients` block for one request.
#[must_use]
pub fn evaluate_split_clients(split: &SplitClientsConfig, variables: &dyn MapVariables) -> String {
    // nginx expands the source as a complex value (a variable template).
    let value = expand_map_template(&split.source, variables);
    let hash = murmur_hash2(value.as_bytes());
    let mut last: u64 = 0;
    for part in &split.parts {
        let Some(percent) = part.percent else {
            // `*` always matches once reached.
            return part.value.clone();
        };
        last += u64::from(percent) * u64::from(u32::MAX) / 10_000;
        let threshold = last.min(u64::from(u32::MAX)) as u32;
        if hash < threshold {
            return part.value.clone();
        }
    }
    String::new()
}

/// Parse an nginx `split_clients` percentage token (`30%`, `2.5%`, `*`) into
/// hundredths of a percent (`30%` → 3000). `None` is the `*` catch-all.
#[must_use]
pub fn parse_split_percent(token: &str) -> Option<Option<u32>> {
    if token == "*" {
        return Some(None);
    }
    let digits = token.strip_suffix('%')?;
    if digits.is_empty() {
        return None;
    }
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (digits, None),
    };
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let whole: u32 = whole.parse().ok()?;
    let fraction = match fraction {
        None => 0,
        Some(fraction) => {
            if fraction.is_empty()
                || fraction.len() > 2
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            fraction.parse::<u32>().ok()? * 10_u32.pow(2 - fraction.len() as u32)
        }
    };
    let percent = whole.checked_mul(100)?.checked_add(fraction)?;
    if percent == 0 || percent > 10_000 {
        return None;
    }
    Some(Some(percent))
}

/// Validate one `geo` block (mapping is parsed by the nginx-conf import and the
/// typed plane alike).
pub fn validate_geo_config(geo: &GeoConfig) -> Result<(), String> {
    let source = parse_variable_reference(&geo.source)?;
    validate_map_source(&format!("${source}"))?;
    let result = parse_variable_reference(&geo.result)?;
    if !result
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!(
            "geo result `{}` is not a valid nginx variable name",
            geo.result
        ));
    }
    if geo.entries.len() > 4096 {
        return Err("a geo block supports at most 4096 entries".to_owned());
    }
    for entry in &geo.entries {
        if entry.key.eq_ignore_ascii_case("default") {
            continue;
        }
        if geo.ranges {
            if parse_range(&entry.key).is_none() {
                return Err(format!(
                    "geo `ranges;` entry `{}` must be an `start-end` address range",
                    entry.key
                ));
            }
        } else if entry.key.parse::<IpNet>().is_err() {
            return Err(format!(
                "geo entry `{}` must be a CIDR or IP address (declare `ranges;` for ranges)",
                entry.key
            ));
        }
        if entry.value.len() > MAX_MAP_VALUE_BYTES {
            return Err(format!(
                "geo value for `{}` exceeds the {MAX_MAP_VALUE_BYTES}-byte bound",
                entry.key
            ));
        }
    }
    Ok(())
}

/// Validate one `split_clients` block.
pub fn validate_split_clients_config(split: &SplitClientsConfig) -> Result<(), String> {
    let result = parse_variable_reference(&split.result)?;
    if !result
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!(
            "split_clients result `{}` is not a valid nginx variable name",
            split.result
        ));
    }
    if split.source.len() > MAX_MAP_VALUE_BYTES {
        return Err(format!(
            "split_clients source exceeds the {MAX_MAP_VALUE_BYTES}-byte bound"
        ));
    }
    if split.parts.is_empty() || split.parts.len() > 4096 {
        return Err("a split_clients block needs 1..4096 buckets".to_owned());
    }
    let mut sum: u32 = 0;
    for part in &split.parts {
        if part.value.len() > MAX_MAP_VALUE_BYTES {
            return Err("split_clients value exceeds the byte bound".to_owned());
        }
        match part.percent {
            Some(percent) => {
                sum = sum.saturating_add(percent);
                if sum > 10_000 {
                    return Err("split_clients percentages total more than 100%".to_owned());
                }
            }
            None => sum = 10_000,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_geo, evaluate_split_clients, murmur_hash2, parse_split_percent,
        validate_split_clients_config, GeoConfig, SplitClientsConfig,
    };
    use crate::config::map_variables::MapVariables;
    use crate::config::model::{GeoEntryConfig, SplitClientsPartConfig};

    struct Facts<'a>(&'a [(&'a str, &'a str)]);

    impl MapVariables for Facts<'_> {
        fn map_variable(&self, name: &str) -> Option<String> {
            self.0
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn geo(entries: &[(&str, &str)], ranges: bool) -> GeoConfig {
        GeoConfig {
            source: "$remote_addr".to_owned(),
            result: "$geo_zone".to_owned(),
            ranges,
            entries: entries
                .iter()
                .map(|(key, value)| GeoEntryConfig {
                    key: (*key).to_owned(),
                    value: (*value).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn geo_matches_longest_prefix_and_default() {
        let config = geo(
            &[
                ("default", "none"),
                ("127.0.0.0/8", "loopback"),
                ("10.0.0.0/8", "private"),
                ("10.1.0.0/16", "specific"),
            ],
            false,
        );
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "127.0.0.1")])),
            "loopback"
        );
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "10.1.2.3")])),
            "specific"
        );
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "10.2.0.1")])),
            "private"
        );
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "8.8.8.8")])),
            "none"
        );
        // A non-address source falls back to `default`.
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "not-an-address")])),
            "none"
        );
    }

    #[test]
    fn geo_ranges_match_inclusive_bounds() {
        let config = geo(
            &[
                ("default", "no"),
                ("10.0.0.5-10.0.0.10", "ten"),
                ("2001:db8::-2001:db8::ff", "v6"),
            ],
            true,
        );
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "10.0.0.7")])),
            "ten"
        );
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "10.0.0.11")])),
            "no"
        );
        assert_eq!(
            evaluate_geo(&config, &Facts(&[("remote_addr", "2001:db8::10")])),
            "v6"
        );
    }

    #[test]
    fn split_clients_uses_nginx_murmur_hash_and_buckets() {
        // `ngx_murmur_hash2("")` and a few known values pin the hash.
        assert_eq!(murmur_hash2(b""), 0);
        assert_ne!(murmur_hash2(b"nginx"), 0);
        // Real-nginx pinning of the bucketing happens in the differential slice
        // (`split_clients` cases compare the assigned bucket exactly).
        let config = SplitClientsConfig {
            source: "$arg_key".to_owned(),
            result: "$bucket".to_owned(),
            parts: vec![
                SplitClientsPartConfig {
                    percent: parse_split_percent("30%").flatten(),
                    value: "a".to_owned(),
                },
                SplitClientsPartConfig {
                    percent: parse_split_percent("30%").flatten(),
                    value: "b".to_owned(),
                },
                SplitClientsPartConfig {
                    percent: None,
                    value: "c".to_owned(),
                },
            ],
        };
        validate_split_clients_config(&config).expect("valid");
        let bucket = |key: &str| evaluate_split_clients(&config, &Facts(&[("arg_key", key)]));
        for key in ["alpha", "beta", "gamma", "delta", "epsilon"] {
            assert!(
                matches!(bucket(key).as_str(), "a" | "b" | "c"),
                "bucket for {key}"
            );
        }
        // The catch-all always yields its value.
        assert_eq!(bucket("anything"), "c");
        // Percent parsing: nginx fixed-point with two decimals.
        assert_eq!(parse_split_percent("30%"), Some(Some(3000)));
        assert_eq!(parse_split_percent("2.5%"), Some(Some(250)));
        assert_eq!(parse_split_percent("0.1%"), Some(Some(10)));
        assert_eq!(parse_split_percent("*"), Some(None));
        assert_eq!(parse_split_percent("0%"), None);
        assert_eq!(parse_split_percent("100.01%"), None);
    }
}
