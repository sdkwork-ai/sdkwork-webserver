//! nginx response freshness (`expires`) applied at the response choke point.
//!
//! The nginx `expires` directive rewrites `Expires` and `Cache-Control` on the
//! *final* response, whatever produced it — a static file, `return`, a proxy
//! upstream, or an error page hop. That is why the policy is applied here
//! rather than inside the static-file layer: `ngx_http_headers_filter` runs
//! after every content handler, and reproducing it anywhere narrower would
//! silently drop the directive for proxied and generated responses.
//!
//! Faithfulness notes (from `ngx_http_set_expires`):
//!
//! - only the ten "safe" status codes are touched; `expires` has no `always`
//!   parameter, so a 404/403/500 keeps whatever the content handler sent;
//! - `Cache-Control` is *replaced*, never appended to;
//! - the `Epoch`/`Max` header values are the byte-exact literals nginx writes,
//!   so a differential comparison sees identical strings, not merely
//!   equivalent dates;
//! - the daily (`@time`) form is evaluated in UTC. nginx evaluates it in the
//!   server's local time zone, which is identical when `TZ=UTC` — the
//!   deployment image's setting — and the difference is catalogued in
//!   `specs/nginx-gap.catalog.json`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, Response, StatusCode},
};
use sdkwork_webserver_core::{CachePolicyConfig, ExpiresMode};

/// nginx advertises a ten-year freshness for `expires max`.
const MAX_CACHE_CONTROL: &str = "max-age=315360000";
/// The byte-exact `Expires` literal nginx writes for `expires epoch`.
const EPOCH_EXPIRES: &str = "Thu, 01 Jan 1970 00:00:01 GMT";
/// The byte-exact `Expires` literal nginx writes for `expires max`.
const MAX_EXPIRES: &str = "Thu, 31 Dec 2037 23:55:55 GMT";
/// One day in seconds; the `@time` (daily) form cannot exceed it.
const SECONDS_PER_DAY: i64 = 86_400;

/// Apply the route/host `expires` policy to a finished response.
pub(crate) fn apply_response_cache_policy(
    response: &mut Response<Body>,
    policy: &CachePolicyConfig,
    now: SystemTime,
) {
    if !policy.expires.is_declared() || !carries_freshness(response.status()) {
        return;
    }
    let Some((expires, cache_control)) = resolve_expires(policy, response.headers(), now) else {
        return;
    };
    let headers = response.headers_mut();
    set_single_header(headers, header::EXPIRES, &expires);
    set_single_header(headers, header::CACHE_CONTROL, &cache_control);
}

/// The status codes nginx considers safe for a freshness rewrite. Anything
/// else (4xx/5xx) leaves `Expires`/`Cache-Control` exactly as the content
/// handler produced them, because `expires` has no `always` form.
fn carries_freshness(status: StatusCode) -> bool {
    matches!(
        status.as_u16(),
        200 | 201 | 204 | 206 | 301 | 302 | 303 | 304 | 307 | 308
    )
}

/// `ngx_http_set_expires`: resolve the absolute `Expires` value and the
/// replacement `Cache-Control` value.
fn resolve_expires(
    policy: &CachePolicyConfig,
    headers: &HeaderMap,
    now: SystemTime,
) -> Option<(String, String)> {
    let delta = policy.expires_seconds;
    match policy.expires {
        ExpiresMode::Off => None,
        ExpiresMode::Epoch => Some((EPOCH_EXPIRES.to_owned(), "no-cache".to_owned())),
        ExpiresMode::Max => Some((MAX_EXPIRES.to_owned(), MAX_CACHE_CONTROL.to_owned())),
        ExpiresMode::Daily => {
            let expires_at = next_daily_occurrence(delta, now)?;
            let max_age = expires_at.duration_since(now).ok()?.as_secs();
            Some((
                httpdate::fmt_http_date(expires_at),
                format!("max-age={max_age}"),
            ))
        }
        ExpiresMode::Access | ExpiresMode::Modified => {
            // nginx takes the zero-delta shortcut for both remaining modes.
            if delta == 0 {
                return Some((httpdate::fmt_http_date(now), "max-age=0".to_owned()));
            }
            // `modified` means "relative to Last-Modified"; a response without
            // one (or a proxied response carrying none) falls back to the
            // access-time computation, exactly as nginx does.
            let base = match policy.expires {
                ExpiresMode::Modified => last_modified(headers).unwrap_or(now),
                _ => now,
            };
            let expires_at = shift(base, delta)?;
            let max_age = expires_at.duration_since(now).ok().map(|age| age.as_secs());
            match max_age {
                // A negative configured delta, or a `Last-Modified` far enough
                // in the past that the computed expiry already elapsed, means
                // "do not cache".
                Some(max_age) if delta > 0 => Some((
                    httpdate::fmt_http_date(expires_at),
                    format!("max-age={max_age}"),
                )),
                _ => Some((httpdate::fmt_http_date(expires_at), "no-cache".to_owned())),
            }
        }
    }
}

/// Next occurrence of a wall-clock time of day, mirroring `ngx_next_time`:
/// today's instance when it is still ahead, otherwise tomorrow's. An offset of
/// exactly one day is legal in nginx and therefore lands on tomorrow's
/// midnight.
fn next_daily_occurrence(offset_seconds: i64, now: SystemTime) -> Option<SystemTime> {
    let offset = u64::try_from(offset_seconds).ok()?;
    if offset > SECONDS_PER_DAY as u64 {
        return None;
    }
    let now_seconds = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let start_of_day = now_seconds - (now_seconds % SECONDS_PER_DAY as u64);
    let candidate = UNIX_EPOCH + Duration::from_secs(start_of_day.checked_add(offset)?);
    if candidate > now {
        Some(candidate)
    } else {
        Some(candidate + Duration::from_secs(SECONDS_PER_DAY as u64))
    }
}

fn last_modified(headers: &HeaderMap) -> Option<SystemTime> {
    let value = headers.get(header::LAST_MODIFIED)?.to_str().ok()?;
    httpdate::parse_http_date(value).ok()
}

/// Shift a `SystemTime` by a signed number of seconds.
fn shift(base: SystemTime, seconds: i64) -> Option<SystemTime> {
    if seconds >= 0 {
        base.checked_add(Duration::from_secs(seconds as u64))
    } else {
        base.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    }
}

/// Replace every existing value of `name` with exactly one. nginx overwrites
/// the single `Cache-Control` node it owns and disables any others, so a
/// response must never end up carrying two freshness declarations.
fn set_single_header(headers: &mut HeaderMap, name: header::HeaderName, value: &str) {
    let Ok(value) = HeaderValue::from_str(value) else {
        return;
    };
    headers.remove(&name);
    headers.insert(name, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(expires: ExpiresMode, seconds: i64) -> CachePolicyConfig {
        CachePolicyConfig {
            expires,
            expires_seconds: seconds,
            ..CachePolicyConfig::default()
        }
    }

    fn response_for(status: StatusCode, headers: &[(&str, &str)]) -> Response<Body> {
        let mut builder = Response::builder().status(status);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(Body::empty()).unwrap()
    }

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn epoch_and_max_write_nginx_literal_values() {
        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(
            &mut response,
            &policy(ExpiresMode::Epoch, 0),
            at(1_700_000_000),
        );
        assert_eq!(response.headers()[header::EXPIRES], EPOCH_EXPIRES);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");

        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(
            &mut response,
            &policy(ExpiresMode::Max, 0),
            at(1_700_000_000),
        );
        assert_eq!(response.headers()[header::EXPIRES], MAX_EXPIRES);
        assert_eq!(response.headers()[header::CACHE_CONTROL], MAX_CACHE_CONTROL);
    }

    #[test]
    fn relative_expires_shifts_from_the_response_time() {
        let now = at(1_700_000_000);
        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Access, 86_400), now);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "max-age=86400");
        assert_eq!(
            response.headers()[header::EXPIRES],
            httpdate::fmt_http_date(now + Duration::from_secs(86_400))
        );
    }

    #[test]
    fn negative_expires_is_no_cache_regardless_of_absolute_value() {
        let now = at(1_700_000_000);
        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Access, -3_600), now);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(
            response.headers()[header::EXPIRES],
            httpdate::fmt_http_date(now - Duration::from_secs(3_600))
        );
    }

    #[test]
    fn zero_delta_expires_is_max_age_zero() {
        let now = at(1_700_000_000);
        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Access, 0), now);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "max-age=0");
        assert_eq!(
            response.headers()[header::EXPIRES],
            httpdate::fmt_http_date(now)
        );
    }

    #[test]
    fn modified_expires_uses_last_modified_and_can_go_stale() {
        let now = at(1_700_000_000);
        // Last-Modified an hour ago, delta one day: still fresh, max-age is the
        // remaining time (one day minus one hour), not the raw delta.
        let last_modified = httpdate::fmt_http_date(now - Duration::from_secs(3_600));
        let mut response = response_for(StatusCode::OK, &[("last-modified", &last_modified)]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Modified, 86_400), now);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            format!("max-age={}", 86_400 - 3_600)
        );

        // Last-Modified two days ago with a one-hour delta: the computed expiry
        // is already in the past, so nginx answers with `no-cache` even though
        // the configured delta is positive.
        let stale = httpdate::fmt_http_date(now - Duration::from_secs(2 * 86_400));
        let mut response = response_for(StatusCode::OK, &[("last-modified", &stale)]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Modified, 3_600), now);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    }

    #[test]
    fn modified_expires_without_last_modified_falls_back_to_now() {
        let now = at(1_700_000_000);
        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Modified, 3_600), now);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "max-age=3600");
        assert_eq!(
            response.headers()[header::EXPIRES],
            httpdate::fmt_http_date(now + Duration::from_secs(3_600))
        );
    }

    #[test]
    fn daily_expires_targets_the_next_occurrence() {
        // 2023-11-14T22:13:20Z. `@1h` (3600s after midnight) has already
        // passed today, so the answer is tomorrow's 01:00.
        let now = at(1_700_000_000);
        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Daily, 3_600), now);
        let today = 1_700_000_000 - (1_700_000_000 % 86_400);
        let expected = UNIX_EPOCH + Duration::from_secs(today + 86_400 + 3_600);
        assert_eq!(
            response.headers()[header::EXPIRES],
            httpdate::fmt_http_date(expected)
        );
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            format!("max-age={}", today + 86_400 + 3_600 - 1_700_000_000)
        );

        // `@24h` is the one offset at the boundary: `ngx_next_time` assigns it
        // to `tm_hour = 24`, which `mktime` normalises to the next day's
        // midnight, and that instant is still ahead of `now`, so it is taken as
        // is instead of rolling a further day forward.
        let mut response = response_for(StatusCode::OK, &[]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Daily, 86_400), now);
        let expected = UNIX_EPOCH + Duration::from_secs(today + 86_400);
        assert_eq!(
            response.headers()[header::EXPIRES],
            httpdate::fmt_http_date(expected),
            "@24h lands on tomorrow's midnight, matching ngx_next_time"
        );
    }

    #[test]
    fn off_and_unsafe_statuses_leave_the_response_untouched() {
        let now = at(1_700_000_000);
        let mut response = response_for(StatusCode::OK, &[("cache-control", "public, no-cache")]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Max, 0), now);
        assert_eq!(response.headers()[header::CACHE_CONTROL], MAX_CACHE_CONTROL);
        // `off` restores nothing: the directive simply does not run.
        let mut response = response_for(StatusCode::OK, &[("cache-control", "public, no-cache")]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Off, 0), now);
        assert!(!response.headers().contains_key(header::EXPIRES));
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, no-cache",
            "`expires off` must keep the deployment-level freshness policy"
        );
        // 404 is not in nginx's safe-status set.
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::FORBIDDEN,
            StatusCode::BAD_GATEWAY,
        ] {
            let mut response = response_for(status, &[("cache-control", "orig")]);
            apply_response_cache_policy(&mut response, &policy(ExpiresMode::Max, 0), now);
            assert!(
                !response.headers().contains_key(header::EXPIRES),
                "{status}"
            );
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "orig",
                "{status}"
            );
        }
        // 304 is in the set, so a revalidated response still carries freshness.
        let mut response = response_for(StatusCode::NOT_MODIFIED, &[]);
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Max, 0), now);
        assert_eq!(response.headers()[header::EXPIRES], MAX_EXPIRES);
    }

    #[test]
    fn expires_replaces_rather_than_appends_cache_control() {
        let now = at(1_700_000_000);
        let mut response = response_for(StatusCode::OK, &[]);
        response.headers_mut().append(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
        response
            .headers_mut()
            .append(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        apply_response_cache_policy(&mut response, &policy(ExpiresMode::Access, 3_600), now);
        let values: Vec<_> = response
            .headers()
            .get_all(header::CACHE_CONTROL)
            .iter()
            .collect();
        assert_eq!(
            values.len(),
            1,
            "nginx disables every Cache-Control but one"
        );
        assert_eq!(values[0], "max-age=3600");
    }
}
