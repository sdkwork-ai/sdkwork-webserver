//! Cache-Control / Expires freshness parsing (RFC 9111).

use std::time::Duration;

/// Parsed freshness of a response.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Freshness {
    /// Seconds the representation is fresh, when explicitly declared.
    pub fresh_seconds: Option<u64>,
    /// `must-revalidate` / `no-cache` force revalidation after expiry.
    pub revalidate: bool,
}

/// Parse `Cache-Control` and `Expires` into an explicit freshness.
/// Order of preference (RFC 9111 §4.2.1): `s-maxage`, `max-age`,
/// `Expires` date.
pub(crate) fn freshness_for(
    cache_control: Option<&str>,
    expires: Option<&str>,
    date: Option<&str>,
) -> Freshness {
    let control = cache_control.unwrap_or_default().to_ascii_lowercase();
    let mut freshness = Freshness::default();
    let mut max_age: Option<u64> = None;
    for directive in control.split(',') {
        let directive = directive.trim();
        if let Some(value) = directive.strip_prefix("s-maxage=") {
            freshness.fresh_seconds = parse_seconds(value).or(freshness.fresh_seconds);
        } else if let Some(value) = directive.strip_prefix("max-age=") {
            max_age = parse_seconds(value);
        } else if directive == "no-cache" || directive == "must-revalidate" {
            freshness.revalidate = true;
        }
    }
    if freshness.fresh_seconds.is_none() {
        if let Some(seconds) = max_age {
            freshness.fresh_seconds = Some(seconds);
        } else {
            freshness.fresh_seconds = expires_freshness(expires, date);
        }
    }
    freshness
}

fn parse_seconds(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<u64>().ok()
}

/// `Expires` delta against the response `Date` header, clamped to 1 year.
fn expires_freshness(expires: Option<&str>, date: Option<&str>) -> Option<u64> {
    let expires = expires.and_then(|value| httpdate::parse_http_date(value).ok())?;
    let date = date
        .and_then(|value| httpdate::parse_http_date(value).ok())
        .unwrap_or_else(std::time::SystemTime::now);
    let delta = expires
        .duration_since(date)
        .ok()?
        .as_secs()
        .min(31_536_000);
    Some(delta)
}

impl Freshness {
    pub fn effective_ttl(&self, default_seconds: u64) -> Duration {
        Duration::from_secs(self.fresh_seconds.unwrap_or(default_seconds))
    }
}
