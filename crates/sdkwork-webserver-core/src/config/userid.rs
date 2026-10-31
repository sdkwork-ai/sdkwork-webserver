//! nginx `ngx_http_userid_filter_module` compatibility.
//!
//! The module issues an opaque client identifier cookie (`uid` by default) and
//! exposes it to response-header and rewrite templates through `$uid_got` /
//! `$uid_set`. The behaviour implemented here is transcribed from nginx
//! 1.29.6 (`src/http/modules/ngx_http_userid_filter_module.c`) and confirmed
//! against the reference binary:
//!
//! - `userid off|log|v1|on` selects the mode. `off` disables the module, `log`
//!   only parses an inbound cookie (`$uid_got`), and `v1` / `on` mint an id when
//!   the request carries none and echo an existing one (optionally re-marking
//!   it) otherwise.
//! - The cookie carries a 16-byte identifier in standard base64 (24 characters
//!   including `==`). The two padding characters are the only bytes a
//!   configured `userid_mark` rewrites: a mark replaces the first `=`.
//! - `$uid_set` is `name=` followed by four uppercase hexadecimal words of the
//!   identifier, each formatted from its native little-endian word (the byte
//!   order `ngx_sprintf("%08XD")` produces on the reference x86-64 build).
//! - `userid_expires max` emits the fixed `Thu, 31-Dec-37 23:55:55 GMT`; a
//!   duration emits `ngx_http_cookie_time(now + duration)` in the same
//!   `%a, %d-%b-%y %H:%M:%S GMT` shape.
//! - Attributes are serialised in nginx's fixed order: `expires`, `domain`,
//!   `path`, `secure`, `httponly`, `samesite`. `userid_p3p` adds a `P3P` field
//!   after the `Set-Cookie`.
//!
//! nginx derives the identifier from the peer address, current time, a process
//! start value and a sequencer. Those fields are not a secret, and predicting
//! the sequencer leaks how many cookies a process issued. The runtime keeps the
//! observable shape (service word, timestamp word, mark placement) but fills
//! the remaining eight bytes from the operating-system CSPRNG so an id cannot
//! be predicted from a previous one.

use std::{
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// nginx `userid` modes, ordered exactly as nginx compares them
/// (`off < log < v1 < on`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserIdMode {
    /// `userid off`: the module does not read or issue cookies.
    #[default]
    Off,
    /// `userid log`: parse `$uid_got`, but never issue a cookie.
    Log,
    /// `userid v1`: issue the legacy 16-byte id and echo an inbound one.
    V1,
    /// `userid on`: issue the current-format id and echo an inbound one.
    On,
}

impl UserIdMode {
    /// The nginx spelling (`off`, `log`, `v1`, `on`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Log => "log",
            Self::V1 => "v1",
            Self::On => "on",
        }
    }
}

/// `userid_expires` value.
///
/// The JSON form is `"max"` or `{"seconds": N}`; the derived externally-tagged
/// representation would require `{"seconds": {"seconds": N}}`, so the enum
/// carries a hand-written (de)serializer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserIdExpires {
    /// `userid_expires max`: the fixed 2037 expiry.
    Max,
    /// `userid_expires 1h`: `now + seconds`.
    Seconds {
        /// Positive lifetime in seconds.
        seconds: u64,
    },
}

impl Serialize for UserIdExpires {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Max => serializer.serialize_str("max"),
            Self::Seconds { seconds } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("seconds", seconds)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for UserIdExpires {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ExpiresVisitor;

        impl<'de> serde::de::Visitor<'de> for ExpiresVisitor {
            type Value = UserIdExpires;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("`\"max\"` or `{\"seconds\": <positive integer>}`")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value == "max" {
                    Ok(UserIdExpires::Max)
                } else {
                    Err(E::invalid_value(
                        serde::de::Unexpected::Str(value),
                        &"`\"max\"` or `{\"seconds\": <positive integer>}`",
                    ))
                }
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let Some(key) = map.next_key::<String>()? else {
                    return Err(serde::de::Error::custom(
                        "userid_expires object must contain `seconds`",
                    ));
                };
                if key != "seconds" {
                    return Err(serde::de::Error::unknown_field(&key, &["seconds"]));
                }
                let seconds: u64 = map.next_value()?;
                if map.next_key::<String>()?.is_some() {
                    return Err(serde::de::Error::custom(
                        "userid_expires object must contain only `seconds`",
                    ));
                }
                Ok(UserIdExpires::Seconds { seconds })
            }
        }

        deserializer.deserialize_any(ExpiresVisitor)
    }
}

/// `userid_flags samesite=…` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserIdSameSite {
    /// `samesite=strict`
    Strict,
    /// `samesite=lax`
    Lax,
    /// `samesite=none`
    #[serde(rename = "none")]
    None,
}

impl UserIdSameSite {
    /// The attribute spelling (`strict`, `lax`, `none`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Lax => "lax",
            Self::None => "none",
        }
    }
}

/// `userid_flags` bit set. `userid_flags off` clears every inherited flag and
/// is represented by the empty value here (nginx's `COOKIE_OFF` bit has no
/// runtime effect beyond suppressing the other flags).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserIdFlags {
    /// `secure`
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secure: bool,
    /// `httponly`
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub httponly: bool,
    /// `samesite=strict|lax|none`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samesite: Option<UserIdSameSite>,
}

impl UserIdFlags {
    /// Whether no attribute flag is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.secure && !self.httponly && self.samesite.is_none()
    }
}

/// nginx userid module configuration resolved for a host or route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserIdConfig {
    /// `userid off|log|v1|on` (default `off`).
    #[serde(default)]
    pub mode: UserIdMode,
    /// `userid_name` (default `uid`).
    #[serde(
        default = "default_userid_name",
        skip_serializing_if = "is_default_userid_name"
    )]
    pub name: String,
    /// `userid_domain`; `None` means nginx's `none` (no `Domain` attribute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// `userid_path` (default `/`). nginx always emits `; path=<value>`.
    #[serde(
        default = "default_userid_path",
        skip_serializing_if = "is_default_userid_path"
    )]
    pub path: String,
    /// `userid_expires`; `None` means `off` (a session cookie).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<UserIdExpires>,
    /// `userid_p3p`; `None` means `none` (no `P3P` header).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p3p: Option<String>,
    /// `userid_flags` (default: no flags).
    #[serde(default, skip_serializing_if = "UserIdFlags::is_empty")]
    pub flags: UserIdFlags,
    /// `userid_mark`; `None` means `off`. The mark is one letter, digit or `=`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<char>,
    /// `userid_service`; `None` means the listener's local address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<u64>,
}

fn default_userid_name() -> String {
    "uid".to_owned()
}

fn is_default_userid_name(value: &str) -> bool {
    value == "uid"
}

fn default_userid_path() -> String {
    "/".to_owned()
}

fn is_default_userid_path(value: &str) -> bool {
    value == "/"
}

impl Default for UserIdConfig {
    fn default() -> Self {
        Self {
            mode: UserIdMode::Off,
            name: default_userid_name(),
            domain: None,
            path: default_userid_path(),
            expires: None,
            p3p: None,
            flags: UserIdFlags::default(),
            mark: None,
            service: None,
        }
    }
}

/// `ngx_http_userid_variable` value (`name=` + four `%08X` words) for the
/// inbound and outbound identifiers, plus the headers the response filter
/// would add.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserIdRequestState {
    /// `$uid_got`: the inbound cookie's identifier, `None` when absent/invalid.
    pub got: Option<String>,
    /// `$uid_set`: the identifier issued (or echoed) for this request.
    pub set: Option<String>,
    /// The `Set-Cookie` value the header filter adds, when it issues one.
    pub set_cookie: Option<String>,
    /// The `P3P` value appended after `Set-Cookie`, when configured.
    pub p3p: Option<String>,
}

impl UserIdRequestState {
    /// `$uid_got` expansion (empty when nginx would report `not_found`).
    #[must_use]
    pub fn got_value(&self) -> &str {
        self.got.as_deref().unwrap_or("")
    }

    /// `$uid_set` expansion (empty when nginx would report `not_found`).
    #[must_use]
    pub fn set_value(&self) -> &str {
        self.set.as_deref().unwrap_or("")
    }
}

/// Evaluate the module for one main request.
///
/// `cookie_values` holds every inbound `Cookie` header value in order, the way
/// nginx's `ngx_http_parse_cookie_lines` walks them. `local_ip` is the address
/// of the accepted socket (`c->local_sockaddr`) and `now` the current time.
#[must_use]
pub fn evaluate_userid(
    config: &UserIdConfig,
    cookie_values: &[&str],
    local_ip: IpAddr,
    now: SystemTime,
) -> UserIdRequestState {
    if config.mode == UserIdMode::Off {
        return UserIdRequestState::default();
    }

    let raw_cookie = find_cookie(cookie_values, &config.name);
    let got_bytes = raw_cookie.and_then(decode_inbound_uid);
    let got = got_bytes.map(|bytes| format_uid_variable(&config.name, &bytes));

    let mut state = UserIdRequestState {
        got,
        ..UserIdRequestState::default()
    };

    if config.mode < UserIdMode::V1 {
        // `log` mode never issues a cookie.
        return state;
    }

    let mut echo: Option<[u8; 16]> = None;
    let mut issued: Option<[u8; 16]> = None;
    if let (Some(bytes), Some(raw)) = (got_bytes, raw_cookie) {
        let marked = match config.mark {
            None => true,
            Some(mark) => {
                raw.len() > 23 && raw.as_bytes()[22] == mark as u8 && raw.as_bytes()[23] == b'='
            }
        };
        if !marked {
            // The cookie is valid but carries no (or a stale) mark: echo it
            // with the configured mark instead of minting a new identity.
            echo = Some(bytes);
        }
    } else {
        issued = Some(generate_uid(config, local_ip, now));
    }

    if let Some(bytes) = issued {
        state.set = Some(format_uid_variable(&config.name, &bytes));
        state.set_cookie = Some(serialize_cookie(
            config,
            &encode_new_uid(&bytes, config.mark),
            now,
        ));
        state.p3p = config
            .p3p
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned();
    } else if let Some(bytes) = echo {
        state.set = Some(format_uid_variable(&config.name, &bytes));
        let raw = raw_cookie.unwrap_or_default();
        let mark = config.mark.unwrap_or('=');
        state.set_cookie = Some(serialize_cookie(config, &echo_uid(raw, mark), now));
        state.p3p = config
            .p3p
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned();
    }

    state
}

/// Locate `name` in a sequence of `Cookie` header values and return its value.
fn find_cookie<'a>(cookie_values: &[&'a str], name: &str) -> Option<&'a str> {
    for header in cookie_values {
        for pair in header.split(';') {
            let pair = pair.trim();
            let Some((candidate, value)) = pair.split_once('=') else {
                continue;
            };
            if candidate.trim() == name {
                return Some(value.trim());
            }
        }
    }
    None
}

/// nginx limits the encoded identifier to 22 characters (the mark occupies the
/// 23rd) and base64-decodes a fixed 16 bytes. Appending `==` makes the standard
/// engine decode the same 16 bytes nginx's decoder produces.
fn decode_inbound_uid(raw: &str) -> Option<[u8; 16]> {
    if raw.len() < 22 {
        return None;
    }
    let head = raw.get(..22)?;
    let mut padded = String::with_capacity(24);
    padded.push_str(head);
    padded.push_str("==");
    let decoded = BASE64.decode(padded.as_bytes()).ok()?;
    let bytes: [u8; 16] = decoded.try_into().ok()?;
    // nginx treats a zero final word as "no identifier"; a cookie whose tail
    // decodes to zero cannot identify anybody.
    if u32::from_le_bytes(bytes[12..16].try_into().ok()?) == 0 {
        return None;
    }
    Some(bytes)
}

/// `$uid_*` rendering: nginx prints each native word as `%08X`, which on the
/// reference little-endian build is the raw bytes read little-endian.
fn format_uid_variable(name: &str, bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(name.len() + 33);
    out.push_str(name);
    out.push('=');
    for index in [0_usize, 4, 8, 12] {
        let word = u32::from_le_bytes([
            bytes[index],
            bytes[index + 1],
            bytes[index + 2],
            bytes[index + 3],
        ]);
        out.push_str(&format!("{word:08X}"));
    }
    out
}

/// Mint a 16-byte identifier preserving nginx's observable fields:
///
/// - `v1`: service (host-endian word, zero when unset), then the time word;
/// - `on`: local address (or the service word in network byte order), then the
///   time word in network byte order.
///
/// The last eight bytes come from the OS CSPRNG instead of nginx's process
/// start value and sequencer.
fn generate_uid(config: &UserIdConfig, local_ip: IpAddr, now: SystemTime) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    let seconds = unix_seconds(now) as u32;
    match config.mode {
        UserIdMode::V1 => {
            bytes[0..4].copy_from_slice(
                &u32::try_from(config.service.unwrap_or(0))
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            bytes[4..8].copy_from_slice(&seconds.to_le_bytes());
        }
        _ => {
            let service = match config.service {
                Some(service) => u32::try_from(service).unwrap_or(u32::MAX).to_be_bytes(),
                None => match local_ip {
                    IpAddr::V4(address) => address.octets(),
                    IpAddr::V6(address) => {
                        let octets = address.octets();
                        [octets[12], octets[13], octets[14], octets[15]]
                    }
                },
            };
            bytes[0..4].copy_from_slice(&service);
            bytes[4..8].copy_from_slice(&seconds.to_be_bytes());
        }
    }
    // Eight further random bytes from the shared CSPRNG-backed generator.
    // Hashing its output keeps the tail independent of the alphanumeric
    // alphabet while staying inside an already-declared dependency.
    let entropy = sdkwork_utils_rust::random_string(32);
    let digest = sdkwork_utils_rust::sha256_digest(entropy.as_bytes());
    bytes[8..16].copy_from_slice(&digest[..8]);
    bytes
}

/// Base64-encode a freshly minted identifier, applying `userid_mark` to the
/// first padding character exactly as nginx's `*(p - 2) = conf->mark`.
fn encode_new_uid(bytes: &[u8; 16], mark: Option<char>) -> String {
    let mut encoded = BASE64.encode(bytes);
    if let Some(mark) = mark {
        if encoded.len() == 24 {
            encoded.replace_range(22..23, &mark.to_string());
        }
    }
    encoded
}

/// nginx's echo branch: the first 22 characters of the inbound cookie,
/// followed by the configured mark and one `=`.
fn echo_uid(raw: &str, mark: char) -> String {
    let head = raw.get(..22).unwrap_or(raw);
    let mut out = String::with_capacity(24);
    out.push_str(head);
    out.push(mark);
    out.push('=');
    out
}

/// Serialise the complete `Set-Cookie` value in nginx's fixed attribute order.
fn serialize_cookie(config: &UserIdConfig, encoded: &str, now: SystemTime) -> String {
    let mut cookie = String::with_capacity(64 + config.name.len() + config.path.len());
    cookie.push_str(&config.name);
    cookie.push('=');
    cookie.push_str(encoded);
    match config.expires {
        Some(UserIdExpires::Max) => {
            cookie.push_str("; expires=Thu, 31-Dec-37 23:55:55 GMT");
        }
        Some(UserIdExpires::Seconds { seconds }) => {
            let expiry = unix_seconds(now).saturating_add(seconds);
            cookie.push_str("; expires=");
            cookie.push_str(&format_cookie_time(expiry));
        }
        None => {}
    }
    if let Some(domain) = config.domain.as_ref().filter(|domain| !domain.is_empty()) {
        cookie.push_str("; domain=");
        cookie.push_str(domain);
    }
    cookie.push_str("; path=");
    cookie.push_str(&config.path);
    if config.flags.secure {
        cookie.push_str("; secure");
    }
    if config.flags.httponly {
        cookie.push_str("; httponly");
    }
    if let Some(samesite) = config.flags.samesite {
        cookie.push_str("; samesite=");
        cookie.push_str(samesite.as_str());
    }
    cookie
}

fn unix_seconds(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `ngx_http_cookie_time`: `%s, %02d-%s-%02d %02d:%02d:%02d GMT` with a
/// two-digit year. 1970-01-01 was a Thursday.
#[must_use]
pub fn format_cookie_time(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let weekday = (days + 4).rem_euclid(7) as usize;
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;
    format!(
        "{}, {:02}-{}-{:02} {:02}:{:02}:{:02} GMT",
        WEEKDAYS[weekday],
        day,
        MONTHS[(month - 1) as usize],
        year % 100,
        hour,
        minute,
        second
    )
}

/// Howard Hinnant's `civil_from_days` (days since 1970-01-01).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn config() -> UserIdConfig {
        UserIdConfig {
            mode: UserIdMode::On,
            ..UserIdConfig::default()
        }
    }

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn first_request_issues_a_cookie_with_nginx_attributes() {
        let mut config = config();
        config.domain = Some("example.com".to_owned());
        config.path = "/sub".to_owned();
        config.expires = Some(UserIdExpires::Seconds { seconds: 3600 });
        config.flags = UserIdFlags {
            secure: true,
            httponly: true,
            samesite: Some(UserIdSameSite::Lax),
        };
        let state = evaluate_userid(
            &config,
            &[],
            "127.0.0.1".parse().expect("ip"),
            at(1_769_000_000),
        );
        let cookie = state.set_cookie.clone().expect("cookie");
        assert!(cookie.starts_with("uid="));
        assert!(cookie.contains("; expires=Wed, 21-Jan-26 13:53:20 GMT"));
        assert!(cookie.ends_with("; domain=example.com; path=/sub; secure; httponly; samesite=lax"));
        assert_eq!(state.set_value().len(), "uid=".len() + 32);
        assert!(state.got_value().is_empty());
    }

    #[test]
    fn second_request_echoes_without_reissuing_when_no_mark_is_configured() {
        let first = evaluate_userid(&config(), &[], "127.0.0.1".parse().expect("ip"), at(100));
        let cookie = first
            .set_cookie
            .expect("cookie")
            .split(';')
            .next()
            .expect("pair")
            .to_owned();
        let state = evaluate_userid(
            &config(),
            &[cookie.as_str()],
            "127.0.0.1".parse().expect("ip"),
            at(200),
        );
        assert!(
            state.set_cookie.is_none(),
            "nginx does not re-send an unmarked cookie"
        );
        assert!(state.set_value().is_empty());
        assert!(!state.got_value().is_empty());
    }

    #[test]
    fn configured_mark_reissues_an_unmarked_cookie() {
        let mut marked = config();
        marked.mark = Some('Z');
        let first = evaluate_userid(&marked, &[], "127.0.0.1".parse().expect("ip"), at(100));
        let cookie = first
            .set_cookie
            .expect("cookie")
            .split(';')
            .next()
            .expect("pair")
            .to_owned();
        // nginx's first cookie already carries the mark in the padding slot.
        assert!(cookie.ends_with("Z="), "cookie was {cookie}");
        // Strip the mark back to the canonical `==` and observe the reissue.
        let unmarked = format!("uid={}==", &cookie["uid=".len()..cookie.len() - 2]);
        let state = evaluate_userid(
            &marked,
            &[unmarked.as_str()],
            "127.0.0.1".parse().expect("ip"),
            at(200),
        );
        let reissued = state.set_cookie.expect("reissued cookie");
        assert!(reissued.ends_with("Z=; path=/"), "cookie was {reissued}");
        assert_eq!(
            &reissued["uid=".len()..][..22],
            &unmarked["uid=".len()..][..22]
        );
    }

    #[test]
    fn log_mode_reads_but_never_issues() {
        let mut log = config();
        log.mode = UserIdMode::Log;
        let first = evaluate_userid(&config(), &[], "127.0.0.1".parse().expect("ip"), at(100));
        let cookie = first
            .set_cookie
            .expect("cookie")
            .split(';')
            .next()
            .expect("pair")
            .to_owned();
        let state = evaluate_userid(
            &log,
            &[cookie.as_str()],
            "127.0.0.1".parse().expect("ip"),
            at(200),
        );
        assert!(!state.got_value().is_empty());
        assert!(state.set_cookie.is_none());
        assert!(state.set_value().is_empty());
    }

    #[test]
    fn off_mode_ignores_cookies() {
        let mut off = config();
        off.mode = UserIdMode::Off;
        let state = evaluate_userid(
            &off,
            &["uid=AAAAAAAAAAAAAAAAAAAAAA=="],
            "127.0.0.1".parse().expect("ip"),
            at(100),
        );
        assert_eq!(state, UserIdRequestState::default());
    }

    #[test]
    fn service_word_uses_nginx_byte_order_per_mode() {
        let mut v1 = config();
        v1.mode = UserIdMode::V1;
        v1.service = Some(12_345);
        let state = evaluate_userid(&v1, &[], "127.0.0.1".parse().expect("ip"), at(100));
        let cookie = state.set_cookie.expect("cookie");
        let encoded = cookie
            .strip_prefix("uid=")
            .and_then(|rest| rest.split(';').next())
            .expect("value");
        let mut padded = encoded.to_owned();
        while padded.len() % 4 != 0 {
            padded.push('=');
        }
        let bytes = BASE64.decode(padded).expect("decode");
        assert_eq!(&bytes[0..4], &12_345_u32.to_le_bytes());

        let mut on = config();
        on.service = Some(12_345);
        let state = evaluate_userid(&on, &[], "127.0.0.1".parse().expect("ip"), at(100));
        let cookie = state.set_cookie.expect("cookie");
        let encoded = cookie
            .strip_prefix("uid=")
            .and_then(|rest| rest.split(';').next())
            .expect("value");
        let mut padded = encoded.to_owned();
        while padded.len() % 4 != 0 {
            padded.push('=');
        }
        let bytes = BASE64.decode(padded).expect("decode");
        assert_eq!(&bytes[0..4], &12_345_u32.to_be_bytes());
    }

    #[test]
    fn max_expiry_is_the_fixed_nginx_date() {
        let mut config = config();
        config.expires = Some(UserIdExpires::Max);
        let state = evaluate_userid(&config, &[], "127.0.0.1".parse().expect("ip"), at(100));
        assert!(state
            .set_cookie
            .expect("cookie")
            .contains("; expires=Thu, 31-Dec-37 23:55:55 GMT"));
    }

    #[test]
    fn cookie_time_matches_nginx_format() {
        assert_eq!(
            format_cookie_time(1_769_000_000),
            "Wed, 21-Jan-26 12:53:20 GMT"
        );
        assert_eq!(
            format_cookie_time(2_145_916_555),
            "Thu, 31-Dec-37 23:55:55 GMT"
        );
    }

    #[test]
    fn invalid_or_short_cookies_are_ignored() {
        for cookie in [
            "uid=abc",
            "uid=!!!!!!!!!!!!!!!!!!!!!!",
            "other=AAAAAAAAAAAAAAAAAAAAAA==",
        ] {
            let state = evaluate_userid(
                &config(),
                &[cookie],
                "127.0.0.1".parse().expect("ip"),
                at(100),
            );
            assert!(state.got_value().is_empty(), "cookie {cookie}");
            assert!(!state.set_cookie.is_none());
        }
    }
}
