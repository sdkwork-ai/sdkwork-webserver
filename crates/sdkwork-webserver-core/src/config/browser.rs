//! nginx `ngx_http_browser_module` compatibility: `$modern_browser`,
//! `$ancient_browser` and the `modern_browser` / `ancient_browser` directives.
//!
//! The tables and the version comparison are transcribed from nginx 1.29.6
//! (`src/http/modules/ngx_http_browser_module.c`), including the module's
//! non-obvious details:
//!
//! - only five browser names exist (`opera`, `msie`, `gecko`, `safari`,
//!   `konqueror`); there is no `firefox` or `chrome` name. Firefox is matched by
//!   the `rv:` token, Chrome by its `Safari/` token;
//! - each mask searches from a fixed offset (`opera` from 0, `msie` from 20,
//!   `gecko`/`safari` from 13, `konqueror` from 20);
//! - the first matching browser name in slot order decides the class, even when
//!   its version is below the configured threshold;
//! - when a level declares any `modern_browser` rule but omits Opera, nginx
//!   appends Opera with an unreachable version (`4001000000`) so an Opera user
//!   agent can never be classified modern by a list that does not mention it;
//! - `modern_browser unlisted` and a `modern_browser` rule list are one merge
//!   family: declaring either replaces the inherited list;
//! - `ancient_browser` takes literal case-sensitive substrings and the special
//!   `netscape4` token (`Mozilla/1`..`Mozilla/4` prefix); declaring any
//!   `ancient_browser` replaces the inherited declarations;
//! - both variables default to the nginx true value `1`, so `$ancient_browser`
//!   is `1` for every user agent when no browser directive is configured.
//!
//! The version grammar accepts `X`, `X.X`, `X.X.X` and `X.X.X.X`, with the
//! documented maxima `4000`, `4000.99`, `4000.99.99`, `4000.99.99.99`.

use serde::{Deserialize, Serialize};

/// The nginx browser masks, in the order nginx checks them (Opera first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModernBrowserName {
    /// `opera` — search `Opera ` from offset 0.
    Opera,
    /// `msie` — search `MSIE ` from offset 20.
    Msie,
    /// `gecko` — search `rv:` from offset 13 (this is how Firefox matches).
    Gecko,
    /// `safari` — search `Safari/` from offset 13 (Chrome also matches here).
    Safari,
    /// `konqueror` — search `Konqueror/` from offset 20.
    Konqueror,
}

impl ModernBrowserName {
    /// nginx's detection slot (`ngx_http_modern_browser_masks` order).
    #[must_use]
    pub fn slot(self) -> u8 {
        match self {
            Self::Opera => 0,
            Self::Msie => 1,
            Self::Gecko => 2,
            Self::Safari => 3,
            Self::Konqueror => 4,
        }
    }

    /// `(skip, add, token)` from `ngx_http_modern_browser_masks`.
    #[must_use]
    pub fn mask(self) -> (usize, usize, &'static str) {
        match self {
            Self::Opera => (0, 6, "Opera"),
            Self::Msie => (25, 5, "MSIE "),
            Self::Gecko => (13, 3, "rv:"),
            Self::Safari => (13, 7, "Safari/"),
            Self::Konqueror => (25, 10, "Konqueror/"),
        }
    }

    /// Case-insensitive parsing of the directive's browser name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("opera") {
            Some(Self::Opera)
        } else if name.eq_ignore_ascii_case("msie") {
            Some(Self::Msie)
        } else if name.eq_ignore_ascii_case("gecko") {
            Some(Self::Gecko)
        } else if name.eq_ignore_ascii_case("safari") {
            Some(Self::Safari)
        } else if name.eq_ignore_ascii_case("konqueror") {
            Some(Self::Konqueror)
        } else {
            None
        }
    }

    /// The nginx spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opera => "opera",
            Self::Msie => "msie",
            Self::Gecko => "gecko",
            Self::Safari => "safari",
            Self::Konqueror => "konqueror",
        }
    }
}

/// One `modern_browser <name> <version>` rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModernBrowserRule {
    /// The browser name.
    pub browser: ModernBrowserName,
    /// The encoded minimum modern version (`X*1e6 + X*1e4 + X*1e2 + X`).
    pub version: u32,
}

/// Opera's version when nginx injects the unreachable entry at merge time.
pub const UNREACHABLE_OPERA_VERSION: u32 = 4_001_000_000;

/// nginx browser module configuration resolved for a host or route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserConfig {
    /// `modern_browser` rules, in declaration order. Evaluation applies nginx's
    /// slot ordering itself, so a typed configuration need not pre-sort.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modern: Vec<ModernBrowserRule>,
    /// Whether nginx filled the browser masks for `modern`.
    ///
    /// This is an nginx import detail, not an operator switch. nginx only fills
    /// the `skip`/`add`/`name` masks when a **child** configuration declares a
    /// `modern_browser` list; an `http`-level list that a server simply
    /// inherits keeps the raw declaration slots (slot index as `skip`, `add` 0,
    /// empty search token), so the first rule parses the version from
    /// `User-Agent[slot]`. The nginx materializer records that state so the
    /// runtime reproduces the reference binary instead of the module's
    /// documented intent. Typed configurations always use `true`.
    #[serde(default = "default_browser_masks", skip_serializing_if = "is_true")]
    pub modern_masks_filled: bool,
    /// `modern_browser unlisted`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub modern_unlisted: bool,
    /// `ancient_browser` literal substrings (the `netscape4` token is carried
    /// in `netscape4`, not here).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ancient: Vec<String>,
    /// `ancient_browser netscape4`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub netscape4: bool,
    /// `modern_browser_value` (default `1`).
    #[serde(default = "default_browser_value")]
    pub modern_value: String,
    /// `ancient_browser_value` (default `1`).
    #[serde(default = "default_browser_value")]
    pub ancient_value: String,
}

fn default_browser_value() -> String {
    "1".to_owned()
}

fn default_browser_masks() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            modern: Vec::new(),
            modern_masks_filled: true,
            modern_unlisted: false,
            ancient: Vec::new(),
            netscape4: false,
            modern_value: default_browser_value(),
            ancient_value: default_browser_value(),
        }
    }
}

/// The class `ngx_http_browser()` assigns to a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserClass {
    /// `NGX_HTTP_MODERN_BROWSER`.
    Modern,
    /// `NGX_HTTP_ANCIENT_BROWSER`.
    Ancient,
}

/// `ngx_http_browser()`: classify one `User-Agent` value.
///
/// `None` means the request carried no `User-Agent` header, which nginx treats
/// as ancient unless `modern_browser unlisted` is configured.
#[must_use]
pub fn classify_browser(config: Option<&BrowserConfig>, user_agent: Option<&str>) -> BrowserClass {
    let default = BrowserConfig::default();
    let config = config.unwrap_or(&default);

    let Some(user_agent) = user_agent else {
        return if config.modern_unlisted {
            BrowserClass::Modern
        } else {
            BrowserClass::Ancient
        };
    };

    let bytes = user_agent.as_bytes();
    if !config.modern.is_empty() && !config.modern_masks_filled {
        // nginx never filled the masks for this inherited list: the first rule
        // whose slot exists in the User-Agent decides, searching for an empty
        // token at that slot (`add` is still zero).
        for rule in &config.modern {
            let skip = rule.browser.slot() as usize;
            if skip >= bytes.len() {
                continue;
            }
            let version = parse_browser_version_bytes(&bytes[skip..]);
            return if version >= u64::from(rule.version) {
                BrowserClass::Modern
            } else {
                BrowserClass::Ancient
            };
        }
        if !config.modern_unlisted {
            return BrowserClass::Ancient;
        }
    }
    if !config.modern.is_empty() && config.modern_masks_filled {
        let mut rules: Vec<ModernBrowserRule> = config.modern.clone();
        if !rules
            .iter()
            .any(|rule| rule.browser == ModernBrowserName::Opera)
        {
            rules.push(ModernBrowserRule {
                browser: ModernBrowserName::Opera,
                version: UNREACHABLE_OPERA_VERSION,
            });
        }
        rules.sort_by_key(|rule| rule.browser.slot());
        for rule in rules {
            let (skip, add, token) = rule.browser.mask();
            if skip >= bytes.len() {
                continue;
            }
            let Some(position) = find_bytes(&bytes[skip..], token.as_bytes()) else {
                continue;
            };
            let version_start = skip + position + add;
            if version_start >= bytes.len() {
                continue;
            }
            let version = parse_browser_version_bytes(&bytes[version_start..]);
            return if version >= u64::from(rule.version) {
                BrowserClass::Modern
            } else {
                BrowserClass::Ancient
            };
        }
        if !config.modern_unlisted {
            return BrowserClass::Ancient;
        }
    }

    if config.netscape4
        && bytes.len() > 13
        && bytes.starts_with(b"Mozilla/")
        && bytes[8] > b'0'
        && bytes[8] < b'5'
    {
        return BrowserClass::Ancient;
    }

    if config
        .ancient
        .iter()
        .any(|entry| user_agent.contains(entry.as_str()))
    {
        return BrowserClass::Ancient;
    }

    if config.modern_unlisted {
        BrowserClass::Modern
    } else {
        BrowserClass::Ancient
    }
}

/// `$modern_browser` and `$ancient_browser` values for one request.
///
/// nginx's variables are `not_found` when the request is not in that class; a
/// header template expands that to the empty string. The configured value
/// defaults to `1`.
#[must_use]
pub fn browser_variable_values<'a>(
    config: Option<&'a BrowserConfig>,
    user_agent: Option<&str>,
) -> (&'a str, &'a str) {
    const TRUE_VALUE: &str = "1";
    let (modern_value, ancient_value) = match config {
        Some(config) => (config.modern_value.as_str(), config.ancient_value.as_str()),
        None => (TRUE_VALUE, TRUE_VALUE),
    };
    match classify_browser(config, user_agent) {
        BrowserClass::Modern => (modern_value, ""),
        BrowserClass::Ancient => ("", ancient_value),
    }
}

/// nginx's per-character version parser (`ngx_http_browser`).
///
/// The comparison stops at the first character that is neither a digit nor a
/// dot; a version that reaches the threshold only by its dotted prefix is
/// modern. nginx compares after every dot and at the end, so the return value
/// here is the final accumulated version (with saturation instead of the
/// reference build's silent unsigned wrap).
fn parse_browser_version_bytes(bytes: &[u8]) -> u64 {
    let mut version = 0_u64;
    let mut part = 0_u64;
    let mut scale = 1_000_000_u64;
    for &byte in bytes {
        if byte.is_ascii_digit() {
            part = part
                .saturating_mul(10)
                .saturating_add(u64::from(byte - b'0'));
            continue;
        }
        if byte == b'.' {
            version = version.saturating_add(part.saturating_mul(scale));
            part = 0;
            scale /= 100;
            continue;
        }
        break;
    }
    version.saturating_add(part.saturating_mul(scale))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Parse `modern_browser <name> <version>`'s version token with nginx's
/// grammar and documented maxima.
pub fn parse_browser_version(value: &str) -> Result<u32, String> {
    if value.is_empty() {
        return Ok(0);
    }
    let mut version = 0_u64;
    let mut part = 0_u64;
    let mut scale = 1_000_000_u64;
    let mut groups = 0_usize;
    let mut digits = 0_usize;
    for character in value.chars() {
        if character.is_ascii_digit() {
            let digit = character.to_digit(10).unwrap_or(0);
            part = part
                .checked_mul(10)
                .and_then(|part| part.checked_add(u64::from(digit)))
                .ok_or_else(|| format!("invalid browser version `{value}`"))?;
            digits += 1;
            continue;
        }
        if character == '.' {
            groups += 1;
            if groups >= 4 || digits == 0 {
                return Err(format!("invalid browser version `{value}`"));
            }
            if groups == 1 {
                if part > 4_000 {
                    return Err(format!("invalid browser version `{value}`"));
                }
            } else if part > 99 {
                return Err(format!("invalid browser version `{value}`"));
            }
            version += part.saturating_mul(scale);
            part = 0;
            scale /= 100;
            digits = 0;
            continue;
        }
        return Err(format!("invalid browser version `{value}`"));
    }
    if digits == 0 && groups > 0 {
        return Err(format!("invalid browser version `{value}`"));
    }
    if groups == 0 {
        if part > 4_000 {
            return Err(format!("invalid browser version `{value}`"));
        }
    } else if part > 99 {
        return Err(format!("invalid browser version `{value}`"));
    }
    version = version.saturating_add(part.saturating_mul(scale));
    u32::try_from(version).map_err(|_| format!("invalid browser version `{value}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(pairs: &[(ModernBrowserName, u32)]) -> BrowserConfig {
        BrowserConfig {
            modern: pairs
                .iter()
                .map(|(browser, version)| ModernBrowserRule {
                    browser: *browser,
                    version: *version,
                })
                .collect(),
            ..BrowserConfig::default()
        }
    }

    #[test]
    fn unconfigured_module_classifies_every_agent_ancient() {
        assert_eq!(
            classify_browser(None, Some("Mozilla/5.0")),
            BrowserClass::Ancient
        );
        assert_eq!(classify_browser(None, None), BrowserClass::Ancient);
        assert_eq!(
            browser_variable_values(None, Some("Mozilla/5.0")),
            ("", "1")
        );
        assert_eq!(browser_variable_values(None, None), ("", "1"));
    }

    #[test]
    fn msie_and_opera_version_thresholds_match_nginx() {
        let config = rules(&[
            (ModernBrowserName::Msie, 6_000_000),
            (ModernBrowserName::Opera, 8_000_000),
        ]);
        let modern = "Mozilla/4.0 (compatible; MSIE 7.0; Windows NT 5.1)";
        let ancient = "Mozilla/4.0 (compatible; MSIE 5.5; Windows NT 5.0)";
        assert_eq!(
            classify_browser(Some(&config), Some(modern)),
            BrowserClass::Modern
        );
        assert_eq!(
            classify_browser(Some(&config), Some(ancient)),
            BrowserClass::Ancient
        );
        assert_eq!(
            classify_browser(Some(&config), Some("Opera/9.80 (X11; Linux i686)")),
            BrowserClass::Modern
        );
        assert_eq!(
            classify_browser(Some(&config), Some("Opera/7.50 (X11; FreeBSD i386)")),
            BrowserClass::Ancient
        );
    }

    #[test]
    fn gecko_matches_firefox_rv_token_and_safari_matches_chrome() {
        let config = rules(&[
            (ModernBrowserName::Gecko, 1_090_000),
            (ModernBrowserName::Safari, 400_000_000),
        ]);
        assert_eq!(
            classify_browser(
                Some(&config),
                Some("Mozilla/5.0 (Windows; rv:1.9) Gecko/2008052906 Firefox/3.0"),
            ),
            BrowserClass::Modern
        );
        assert_eq!(
            classify_browser(
                Some(&config),
                Some("Mozilla/5.0 (X11) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/500.1"),
            ),
            BrowserClass::Modern
        );
        assert_eq!(
            classify_browser(
                Some(&config),
                Some("Mozilla/5.0 (Macintosh) AppleWebKit/418 (KHTML, like Gecko) Safari/125.7"),
            ),
            BrowserClass::Ancient
        );
    }

    #[test]
    fn first_matching_browser_decides_even_without_a_modern_rule() {
        // Only Gecko is listed; an MSIE user agent never reaches the threshold
        // because MSIE has no rule, and the auto-added Opera entry cannot match.
        let config = rules(&[(ModernBrowserName::Gecko, 1_000_000)]);
        assert_eq!(
            classify_browser(
                Some(&config),
                Some("Mozilla/4.0 (compatible; MSIE 9.0; Windows NT 6.1)"),
            ),
            BrowserClass::Ancient
        );
    }

    #[test]
    fn unlisted_and_netscape4_follow_nginx_order() {
        let mut config = BrowserConfig {
            modern_unlisted: true,
            ..BrowserConfig::default()
        };
        assert_eq!(
            classify_browser(Some(&config), Some("curl/8.0")),
            BrowserClass::Modern
        );
        config.netscape4 = true;
        assert_eq!(
            classify_browser(Some(&config), Some("Mozilla/4.72 [en] (X11; U)")),
            BrowserClass::Ancient
        );
        config.ancient = vec!["curl".to_owned()];
        assert_eq!(
            classify_browser(Some(&config), Some("curl/8.0")),
            BrowserClass::Ancient
        );
    }

    #[test]
    fn configured_values_are_exposed_per_class() {
        let config = BrowserConfig {
            modern_value: "modern!".to_owned(),
            ancient_value: "ancient!".to_owned(),
            ..BrowserConfig::default()
        };
        assert_eq!(
            browser_variable_values(Some(&config), Some("curl/8")),
            ("", "ancient!")
        );
        let config = BrowserConfig {
            modern: vec![ModernBrowserRule {
                browser: ModernBrowserName::Safari,
                version: 1,
            }],
            modern_value: "modern!".to_owned(),
            ancient_value: "ancient!".to_owned(),
            ..BrowserConfig::default()
        };
        assert_eq!(
            browser_variable_values(Some(&config), Some("Mozilla/5.0 (Safari/537.36)")),
            ("modern!", "")
        );
    }

    #[test]
    fn version_parser_accepts_nginx_documented_forms() {
        for (text, expected) in [
            ("6", 6_000_000),
            ("6.0", 6_000_000),
            ("1.9.3", 1_090_300),
            ("4000.99.99.99", 4_000_999_999),
        ] {
            assert_eq!(parse_browser_version(text), Ok(expected), "value {text}");
        }
        for bad in ["abc", "1.", ".1", "1.2.3.4.5", "4001", "1.100"] {
            assert!(parse_browser_version(bad).is_err(), "value {bad}");
        }
    }
}
