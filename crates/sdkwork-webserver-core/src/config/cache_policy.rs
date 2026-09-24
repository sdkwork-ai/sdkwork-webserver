//! Shared parsers for the nginx response-freshness directives
//! (`expires`, `etag`, `if_modified_since`).
//!
//! Both configuration planes need the same grammar: the nginx-conf plane reads
//! `expires 1d;` / `expires modified 1h;` token-by-token, and the typed TOML
//! plane reads `expires = "1d"` as one string (SDKWORK_WEBSERVER_SPEC.md §11).
//! Keeping the grammar in one place is what guarantees the two planes cannot
//! drift into accepting different subsets of the nginx syntax.

use super::model::{ExpiresMode, IfModifiedSinceMode};

/// Time units accepted by `ngx_parse_time(value, 1)` (second granularity),
/// expressed in integer microseconds. Ordered so `ms` is matched before the
/// `m` prefix; `M` is months and `y` is years, both counted in seconds the way
/// nginx defines them (`30 * 24h` and `365 * 24h`), not as calendar maths.
const NGINX_TIME_UNITS: &[(&str, i128)] = &[
    ("ms", 1_000),
    ("y", 31_536_000_000_000),
    ("M", 2_592_000_000_000),
    ("w", 604_800_000_000),
    ("d", 86_400_000_000),
    ("h", 3_600_000_000),
    ("m", 60_000_000),
    ("s", 1_000_000),
];

/// One day in seconds; nginx refuses a daily (`@`) time above this.
const SECONDS_PER_DAY: i64 = 86_400;

/// `ngx_parse_time(value, 1)`: one or more `<number><unit>` groups, or a bare
/// number meaning seconds.
///
/// Returns whole seconds, rounding half-up the way nginx's
/// `(ngx_int_t) (sec + 0.5)` does but on integer microseconds, so no float
/// error can creep in and `expires 500ms` becomes `1` exactly as nginx
/// computes it.
pub fn parse_nginx_time(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    let mut rest = text;
    let mut micros: i128 = 0;
    let mut groups = 0_usize;
    while !rest.is_empty() {
        let digits_end = rest
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits_end == 0 {
            return None;
        }
        let magnitude: i128 = rest[..digits_end].parse().ok()?;
        rest = &rest[digits_end..];
        let (scale, consumed) = if rest.is_empty() {
            // A trailing bare number is seconds.
            (1_000_000_i128, 0_usize)
        } else {
            let (unit, scale) = NGINX_TIME_UNITS
                .iter()
                .find(|(unit, _)| rest.starts_with(unit))?;
            (*scale, unit.len())
        };
        micros = micros.checked_add(magnitude.checked_mul(scale)?)?;
        rest = &rest[consumed..];
        groups += 1;
    }
    if groups == 0 {
        return None;
    }
    i64::try_from((micros + 500_000) / 1_000_000).ok()
}

/// Parse the value half of an `expires` directive, mirroring
/// `ngx_http_parse_expires`.
///
/// `off`/`epoch`/`max` are keywords and only recognized in the single-argument
/// form (`mode == `[`ExpiresMode::Access`]`); `@` selects the daily mode and is
/// mutually exclusive with `modified`; a leading `+`/`-` sign is honored for
/// the `Access`/`Modified` modes only. Errors are returned as the detail text
/// nginx reports (`invalid value`, `daily time value must be less than 24
/// hours`, …) so both planes can wrap them in their own diagnostic type.
pub fn parse_expires_value(value: &str, mode: ExpiresMode) -> Result<(ExpiresMode, i64), String> {
    if mode == ExpiresMode::Access {
        match value {
            "off" => return Ok((ExpiresMode::Off, 0)),
            "epoch" => return Ok((ExpiresMode::Epoch, 0)),
            "max" => return Ok((ExpiresMode::Max, 0)),
            _ => {}
        }
    }
    let (mode, body) = match value.strip_prefix('@') {
        Some(rest) => {
            if mode == ExpiresMode::Modified {
                return Err("daily time cannot be used with \"modified\"".to_owned());
            }
            (ExpiresMode::Daily, rest)
        }
        None => (mode, value),
    };
    // nginx's `@` branch is an `else if` sibling of the sign branches, so a
    // daily offset never consumes a leading `+`/`-`; `@-1h` therefore reaches
    // the time parser intact and fails there, exactly as nginx reports it.
    let (negative, magnitude) = if mode == ExpiresMode::Daily {
        (false, body)
    } else {
        match body.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, body.strip_prefix('+').unwrap_or(body)),
        }
    };
    let seconds = parse_nginx_time(magnitude).ok_or_else(|| "invalid value".to_owned())?;
    if mode == ExpiresMode::Daily && seconds > SECONDS_PER_DAY {
        return Err("daily time value must be less than 24 hours".to_owned());
    }
    Ok((mode, if negative { -seconds } else { seconds }))
}

/// Parse a complete `expires` argument list.
///
/// `["1d"]` → relative to the response time; `["modified", "1h"]` → relative to
/// `Last-Modified`; `["off"|"epoch"|"max"|"@15h30m"]` → absolute modes. A
/// two-argument form whose selector is not `modified` is refused rather than
/// guessed at.
pub fn parse_expires_arguments(args: &[String]) -> Result<(ExpiresMode, i64), String> {
    match args {
        [value] => parse_expires_value(value, ExpiresMode::Access)
            .map_err(|detail| format!("invalid expires value `{value}`: {detail}")),
        [selector, value] if selector == "modified" => {
            parse_expires_value(value, ExpiresMode::Modified)
                .map_err(|detail| format!("invalid expires modified value `{value}`: {detail}"))
        }
        [selector, _] => Err(format!(
            "expires accepts `off|epoch|max|@<time>|<time>` or `<time>` after `modified`, found `{selector}`"
        )),
        _ => Err(format!(
            "expires takes one argument (or `modified <time>`), found {}",
            args.len()
        )),
    }
}

/// Parse the single `if_modified_since` token (nginx matches it exactly,
/// `ngx_strcmp`-style, so no case folding here).
pub fn parse_if_modified_since_token(token: &str) -> Option<IfModifiedSinceMode> {
    match token {
        "off" => Some(IfModifiedSinceMode::Off),
        "exact" => Some(IfModifiedSinceMode::Exact),
        "before" => Some(IfModifiedSinceMode::Before),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nginx_time_grammar_matches_the_documented_units() {
        assert_eq!(parse_nginx_time("1"), Some(1));
        assert_eq!(parse_nginx_time("30s"), Some(30));
        assert_eq!(parse_nginx_time("5m"), Some(300));
        assert_eq!(parse_nginx_time("1h"), Some(3_600));
        assert_eq!(parse_nginx_time("1h30m"), Some(5_400));
        assert_eq!(parse_nginx_time("1d"), Some(86_400));
        assert_eq!(parse_nginx_time("7d"), Some(604_800));
        assert_eq!(parse_nginx_time("1w"), Some(604_800));
        assert_eq!(parse_nginx_time("1M"), Some(2_592_000));
        assert_eq!(parse_nginx_time("1y"), Some(31_536_000));
        // Sub-second magnitudes round half-up, exactly like nginx's
        // `(ngx_int_t) (sec + 0.5)`.
        assert_eq!(parse_nginx_time("500ms"), Some(1));
        assert_eq!(parse_nginx_time("499ms"), Some(0));
        assert_eq!(parse_nginx_time("1500ms"), Some(2));
        // Rejections: empty, unit-only, unknown unit, overflow.
        assert_eq!(parse_nginx_time(""), None);
        assert_eq!(parse_nginx_time("h"), None);
        assert_eq!(parse_nginx_time("1x"), None);
        assert_eq!(parse_nginx_time("99999999999999999999d"), None);
    }

    #[test]
    fn expires_argument_list_selects_mode_and_sign() {
        assert_eq!(
            parse_expires_arguments(&["off".to_owned()]),
            Ok((ExpiresMode::Off, 0))
        );
        assert_eq!(
            parse_expires_arguments(&["epoch".to_owned()]),
            Ok((ExpiresMode::Epoch, 0))
        );
        assert_eq!(
            parse_expires_arguments(&["max".to_owned()]),
            Ok((ExpiresMode::Max, 0))
        );
        assert_eq!(
            parse_expires_arguments(&["24h".to_owned()]),
            Ok((ExpiresMode::Access, 86_400))
        );
        assert_eq!(
            parse_expires_arguments(&["-1h".to_owned()]),
            Ok((ExpiresMode::Access, -3_600))
        );
        assert_eq!(
            parse_expires_arguments(&["+2h".to_owned()]),
            Ok((ExpiresMode::Access, 7_200))
        );
        assert_eq!(
            parse_expires_arguments(&["modified".to_owned(), "1h".to_owned()]),
            Ok((ExpiresMode::Modified, 3_600))
        );
        assert_eq!(
            parse_expires_arguments(&["modified".to_owned(), "-1h".to_owned()]),
            Ok((ExpiresMode::Modified, -3_600))
        );
        assert_eq!(
            parse_expires_arguments(&["@15h30m".to_owned()]),
            Ok((ExpiresMode::Daily, 55_800))
        );
    }

    #[test]
    fn expires_rejects_what_nginx_rejects() {
        // Keywords are not recognized after `modified`.
        assert!(parse_expires_arguments(&["modified".to_owned(), "off".to_owned()]).is_err());
        assert!(parse_expires_arguments(&["modified".to_owned(), "max".to_owned()]).is_err());
        // `@` and `modified` are mutually exclusive.
        assert!(
            parse_expires_arguments(&["modified".to_owned(), "@1h".to_owned()])
                .is_err_and(|detail| detail.contains("daily time cannot be used with \"modified\""))
        );
        // The daily time must stay inside one day, and cannot carry a sign.
        assert!(parse_expires_arguments(&["@25h".to_owned()])
            .is_err_and(|detail| detail.contains("less than 24 hours")));
        assert!(parse_expires_arguments(&["@-1h".to_owned()]).is_err());
        // Unknown second-argument selectors are refused, not guessed.
        assert!(parse_expires_arguments(&["before".to_owned(), "1h".to_owned()]).is_err());
        // Both too many and too few arguments are refused.
        assert!(parse_expires_arguments(&[]).is_err());
        assert!(
            parse_expires_arguments(&["a".to_owned(), "b".to_owned(), "c".to_owned()]).is_err()
        );
        assert!(parse_expires_arguments(&["nonsense".to_owned()]).is_err());
    }

    #[test]
    fn if_modified_since_token_is_exact_ngx_strcmp() {
        assert_eq!(
            parse_if_modified_since_token("exact"),
            Some(IfModifiedSinceMode::Exact)
        );
        assert_eq!(
            parse_if_modified_since_token("before"),
            Some(IfModifiedSinceMode::Before)
        );
        assert_eq!(
            parse_if_modified_since_token("off"),
            Some(IfModifiedSinceMode::Off)
        );
        assert_eq!(parse_if_modified_since_token("EXACT"), None);
        assert_eq!(parse_if_modified_since_token(""), None);
    }
}
