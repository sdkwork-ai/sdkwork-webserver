//! Parse nginx `limit_conn_zone` / `limit_conn` directive strings.

use super::model::{LimitConnConfig, LimitConnZoneConfig};

/// Approximate bytes per tracked key in nginx shared memory accounting.
const BYTES_PER_KEY: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitConnParseError {
    Empty,
    UnsupportedKey(String),
    MissingZone,
    InvalidSize(String),
    InvalidCount(String),
    UnexpectedToken(String),
}

impl std::fmt::Display for LimitConnParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty limit_conn directive"),
            Self::UnsupportedKey(key) => {
                write!(
                    f,
                    "unsupported limit_conn_zone key `{key}`; only $binary_remote_addr and $remote_addr are executable"
                )
            }
            Self::MissingZone => write!(f, "limit_conn_zone requires zone=<name>:<size>"),
            Self::InvalidSize(value) => write!(f, "invalid zone size `{value}`"),
            Self::InvalidCount(value) => write!(f, "invalid connection count `{value}`"),
            Self::UnexpectedToken(token) => write!(f, "unexpected token `{token}`"),
        }
    }
}

/// Parse one `limitConnZone` entry: `"$binary_remote_addr zone=perip:10m"`.
pub fn parse_limit_conn_zone(entry: &str) -> Result<LimitConnZoneConfig, LimitConnParseError> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Err(LimitConnParseError::Empty);
    }
    let mut key = None;
    let mut name = None;
    let mut size_bytes = None;
    for token in trimmed.split_whitespace() {
        if token.starts_with('$') {
            if key.is_some() {
                return Err(LimitConnParseError::UnexpectedToken(token.to_owned()));
            }
            if token != "$binary_remote_addr" && token != "$remote_addr" {
                return Err(LimitConnParseError::UnsupportedKey(token.to_owned()));
            }
            key = Some(token.to_owned());
            continue;
        }
        if let Some(rest) = token.strip_prefix("zone=") {
            let (zone_name, size) = rest
                .split_once(':')
                .ok_or_else(|| LimitConnParseError::InvalidSize(rest.to_owned()))?;
            if zone_name.is_empty() {
                return Err(LimitConnParseError::MissingZone);
            }
            name = Some(zone_name.to_owned());
            size_bytes = Some(parse_size_bytes(size)?);
            continue;
        }
        return Err(LimitConnParseError::UnexpectedToken(token.to_owned()));
    }
    let key = key.ok_or(LimitConnParseError::UnsupportedKey(String::new()))?;
    let name = name.ok_or(LimitConnParseError::MissingZone)?;
    let size_bytes = size_bytes.ok_or(LimitConnParseError::MissingZone)?;
    let max_keys = (size_bytes / BYTES_PER_KEY).clamp(1, u32::MAX as u64) as u32;
    Ok(LimitConnZoneConfig {
        name,
        key,
        max_keys,
    })
}

/// Parse one `limitConn` entry: `"perip 10"` or `"zone=perip 10"`.
pub fn parse_limit_conn(entry: &str) -> Result<LimitConnConfig, LimitConnParseError> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Err(LimitConnParseError::Empty);
    }
    let mut tokens = trimmed.split_whitespace();
    let first = tokens.next().ok_or(LimitConnParseError::Empty)?;
    let zone = first.strip_prefix("zone=").unwrap_or(first).to_owned();
    if zone.is_empty() {
        return Err(LimitConnParseError::MissingZone);
    }
    let count = tokens
        .next()
        .ok_or(LimitConnParseError::InvalidCount(String::new()))?;
    let max_connections = count
        .parse::<u32>()
        .map_err(|_| LimitConnParseError::InvalidCount(count.to_owned()))?;
    if max_connections == 0 {
        return Err(LimitConnParseError::InvalidCount(count.to_owned()));
    }
    if let Some(extra) = tokens.next() {
        return Err(LimitConnParseError::UnexpectedToken(extra.to_owned()));
    }
    Ok(LimitConnConfig {
        zone,
        max_connections,
    })
}

fn parse_size_bytes(value: &str) -> Result<u64, LimitConnParseError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LimitConnParseError::InvalidSize(value.to_owned()));
    }
    let (number, multiplier) = match value.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&value[..value.len() - 1], 1024_u64),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 1024_u64 * 1024),
        Some(b'g' | b'G') => (&value[..value.len() - 1], 1024_u64 * 1024 * 1024),
        _ => (value, 1_u64),
    };
    let amount = number
        .parse::<u64>()
        .map_err(|_| LimitConnParseError::InvalidSize(value.to_owned()))?;
    Ok(amount.saturating_mul(multiplier))
}

#[cfg(test)]
mod tests {
    use super::{parse_limit_conn, parse_limit_conn_zone};

    #[test]
    fn parses_zone_without_rate() {
        let zone = parse_limit_conn_zone("$binary_remote_addr zone=perip:10m").unwrap();
        assert_eq!(zone.name, "perip");
        assert_eq!(zone.key, "$binary_remote_addr");
        assert!(zone.max_keys > 0);
    }

    #[test]
    fn parses_remote_addr_key_and_small_zone() {
        let zone = parse_limit_conn_zone("$remote_addr zone=perip:1m").unwrap();
        assert_eq!(zone.name, "perip");
        assert_eq!(zone.key, "$remote_addr");
    }

    #[test]
    fn rejects_unsupported_keys() {
        assert!(parse_limit_conn_zone("$server_name zone=perip:1m").is_err());
        assert!(parse_limit_conn_zone("$binary_remote_addr zone=perip").is_err());
    }

    #[test]
    fn parses_connection_count() {
        let rule = parse_limit_conn("perip 10").unwrap();
        assert_eq!(rule.zone, "perip");
        assert_eq!(rule.max_connections, 10);
        let rule = parse_limit_conn("zone=perip 3").unwrap();
        assert_eq!(rule.zone, "perip");
        assert_eq!(rule.max_connections, 3);
    }

    #[test]
    fn rejects_missing_or_zero_count() {
        assert!(parse_limit_conn("perip").is_err());
        assert!(parse_limit_conn("perip 0").is_err());
        assert!(parse_limit_conn("perip 5 extra").is_err());
    }
}
