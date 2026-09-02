//! Parse nginx `limit_req_zone` / `limit_req` directive strings.

use super::model::{LimitReqConfig, LimitReqZoneConfig};

/// Approximate bytes per tracked key in nginx shared memory accounting.
const BYTES_PER_KEY: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitReqParseError {
    Empty,
    UnsupportedKey(String),
    MissingZone,
    MissingRate,
    InvalidSize(String),
    InvalidRate(String),
    InvalidBurst(String),
    UnexpectedToken(String),
}

impl std::fmt::Display for LimitReqParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty limit_req directive"),
            Self::UnsupportedKey(key) => {
                write!(
                    f,
                    "unsupported limit_req_zone key `{key}`; only $binary_remote_addr and $remote_addr are executable"
                )
            }
            Self::MissingZone => write!(f, "limit_req_zone requires zone=<name>:<size>"),
            Self::MissingRate => write!(f, "limit_req_zone requires rate=<n>r/s|r/m"),
            Self::InvalidSize(value) => write!(f, "invalid zone size `{value}`"),
            Self::InvalidRate(value) => write!(f, "invalid rate `{value}`"),
            Self::InvalidBurst(value) => write!(f, "invalid burst `{value}`"),
            Self::UnexpectedToken(token) => write!(f, "unexpected token `{token}`"),
        }
    }
}

/// Parse one `limitReqZone` entry: `"$binary_remote_addr zone=one:10m rate=1r/s"`.
pub fn parse_limit_req_zone(entry: &str) -> Result<LimitReqZoneConfig, LimitReqParseError> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Err(LimitReqParseError::Empty);
    }
    let mut key = None;
    let mut name = None;
    let mut size_bytes = None;
    let mut rate_per_second = None;
    for token in trimmed.split_whitespace() {
        if token.starts_with('$') {
            if key.is_some() {
                return Err(LimitReqParseError::UnexpectedToken(token.to_owned()));
            }
            if token != "$binary_remote_addr" && token != "$remote_addr" {
                return Err(LimitReqParseError::UnsupportedKey(token.to_owned()));
            }
            key = Some(token.to_owned());
            continue;
        }
        if let Some(rest) = token.strip_prefix("zone=") {
            let (zone_name, size) = rest
                .split_once(':')
                .ok_or_else(|| LimitReqParseError::InvalidSize(rest.to_owned()))?;
            if zone_name.is_empty() {
                return Err(LimitReqParseError::MissingZone);
            }
            name = Some(zone_name.to_owned());
            size_bytes = Some(parse_size_bytes(size)?);
            continue;
        }
        if let Some(rest) = token.strip_prefix("rate=") {
            rate_per_second = Some(parse_rate_per_second(rest)?);
            continue;
        }
        return Err(LimitReqParseError::UnexpectedToken(token.to_owned()));
    }
    let key = key.ok_or(LimitReqParseError::UnsupportedKey(String::new()))?;
    let name = name.ok_or(LimitReqParseError::MissingZone)?;
    let size_bytes = size_bytes.ok_or(LimitReqParseError::MissingZone)?;
    let rate_per_second = rate_per_second.ok_or(LimitReqParseError::MissingRate)?;
    let max_keys = (size_bytes / BYTES_PER_KEY).clamp(1, u32::MAX as u64) as u32;
    Ok(LimitReqZoneConfig {
        name,
        key,
        max_keys,
        rate_per_second,
    })
}

/// Parse one `limitReq` entry: `"one burst=5 nodelay"` or `"zone=one burst=5"`.
pub fn parse_limit_req(entry: &str) -> Result<LimitReqConfig, LimitReqParseError> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Err(LimitReqParseError::Empty);
    }
    let mut tokens = trimmed.split_whitespace();
    let first = tokens.next().ok_or(LimitReqParseError::Empty)?;
    let zone = first.strip_prefix("zone=").unwrap_or(first).to_owned();
    if zone.is_empty() {
        return Err(LimitReqParseError::MissingZone);
    }
    let mut burst = 0_u32;
    let mut nodelay = false;
    for token in tokens {
        if let Some(rest) = token.strip_prefix("burst=") {
            burst = rest
                .parse::<u32>()
                .map_err(|_| LimitReqParseError::InvalidBurst(rest.to_owned()))?;
            continue;
        }
        if token == "nodelay" {
            nodelay = true;
            continue;
        }
        return Err(LimitReqParseError::UnexpectedToken(token.to_owned()));
    }
    Ok(LimitReqConfig {
        zone,
        burst,
        nodelay,
    })
}

fn parse_size_bytes(value: &str) -> Result<u64, LimitReqParseError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LimitReqParseError::InvalidSize(value.to_owned()));
    }
    let (number, multiplier) = match value.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&value[..value.len() - 1], 1024_u64),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 1024_u64 * 1024),
        Some(b'g' | b'G') => (&value[..value.len() - 1], 1024_u64 * 1024 * 1024),
        _ => (value, 1_u64),
    };
    let amount = number
        .parse::<u64>()
        .map_err(|_| LimitReqParseError::InvalidSize(value.to_owned()))?;
    Ok(amount.saturating_mul(multiplier))
}

fn parse_rate_per_second(value: &str) -> Result<f64, LimitReqParseError> {
    let value = value.trim();
    let (amount, unit) = value
        .split_once("r/")
        .ok_or_else(|| LimitReqParseError::InvalidRate(value.to_owned()))?;
    let amount = amount
        .parse::<f64>()
        .map_err(|_| LimitReqParseError::InvalidRate(value.to_owned()))?;
    if !amount.is_finite() || amount <= 0.0 {
        return Err(LimitReqParseError::InvalidRate(value.to_owned()));
    }
    match unit {
        "s" => Ok(amount),
        "m" => Ok(amount / 60.0),
        _ => Err(LimitReqParseError::InvalidRate(value.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_limit_req, parse_limit_req_zone};

    #[test]
    fn parses_zone_and_rate() {
        let zone = parse_limit_req_zone("$binary_remote_addr zone=one:10m rate=1r/s").unwrap();
        assert_eq!(zone.name, "one");
        assert_eq!(zone.key, "$binary_remote_addr");
        assert!(zone.max_keys > 0);
        assert!((zone.rate_per_second - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_limit_req_with_burst_nodelay() {
        let rule = parse_limit_req("one burst=5 nodelay").unwrap();
        assert_eq!(rule.zone, "one");
        assert_eq!(rule.burst, 5);
        assert!(rule.nodelay);
    }

    #[test]
    fn parses_rate_per_minute() {
        let zone = parse_limit_req_zone("$remote_addr zone=api:1m rate=60r/m").unwrap();
        assert!((zone.rate_per_second - 1.0).abs() < f64::EPSILON);
    }
}
