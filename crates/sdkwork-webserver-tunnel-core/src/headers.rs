//! HTTP header rewriting on a route's HTTP plane (FRP `hostHeaderRewrite`,
//! `requestHeaders.set`, `responseHeaders.set`).
//!
//! The gateway is the only hop that parses HTTP: the agent is a byte pump, and
//! the tunnel protocol itself stays HTTP-agnostic. So a route's rewrite policy
//! is evaluated where the request and the response are still structured — the
//! data-plane relay — and the agent simply forwards what it receives.
//!
//! Validation is deliberately stricter than FRP's, because a rewritten header
//! that changes framing is a request-smuggling class defect rather than a
//! misconfiguration:
//!
//! - hop-by-hop and framing headers (`Content-Length`, `Transfer-Encoding`,
//!   `Connection`, `Keep-Alive`, `Upgrade`, `TE`, `Trailer`, `Expect`,
//!   `Proxy-*`) are refused: rewriting them desynchronizes the parser on one
//!   side of the relay from the bytes on the wire;
//! - `Host` is not settable through a header set — it has its own field, so
//!   "rewrite the authority" cannot be spelled two ways with different results;
//! - names must be RFC 9110 tokens and values must be free of CR, LF, NUL and
//!   other control characters (except HTAB), which is what makes header
//!   injection impossible by construction;
//! - the sets are bounded ([`RouteHeaderRewrite::MAX_PAIRS`]), so a
//!   configuration cannot turn into unbounded per-request work.
//!
//! Rewriting is applied by the data plane *after* the infrastructure headers
//! (hop-by-hop stripping, `X-Forwarded-For` / `X-Real-IP` injection), so an
//! operator's explicit value wins over the derived one. Admission decisions
//! never read these headers: the visitor address used for `allowed_ips` comes
//! from the connection, not from a header, so a rewrite cannot widen access.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};
use crate::route::TunnelProtocolKind;

/// Maximum number of header pairs in one direction's set.
pub const MAX_HEADER_PAIRS: usize = 32;
/// Maximum accepted header-name length (`tchar` token).
pub const MAX_HEADER_NAME_LEN: usize = 64;
/// Maximum accepted header-value length.
pub const MAX_HEADER_VALUE_LEN: usize = 2048;
/// Maximum accepted `hostHeaderRewrite` length.
pub const MAX_HOST_REWRITE_LEN: usize = 255;

/// Headers that must never be rewritten: they describe framing or the
/// connection itself, so changing one desynchronizes the two parsers.
const RESERVED_HEADERS: &[&str] = &[
    "connection",
    "content-length",
    "expect",
    "host",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// One validated header to set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderPair {
    /// Header name, already validated as an RFC 9110 token.
    pub name: String,
    /// Header value, already validated to be free of control characters.
    pub value: String,
}

impl HeaderPair {
    /// Validates one name/value pair.
    ///
    /// `direction` only shapes the error message ("request"/"response"), so a
    /// rejected configuration names the set the operator actually wrote.
    pub fn parse(name: &str, value: &str, direction: HeaderDirection) -> Result<Self> {
        let name = name.trim();
        let value = value.trim();
        Self::validate_name(name, direction)?;
        Self::validate_value(name, value, direction)?;
        Ok(Self {
            name: name.to_ascii_lowercase(),
            value: value.to_owned(),
        })
    }

    fn validate_name(name: &str, direction: HeaderDirection) -> Result<()> {
        if name.is_empty() || name.len() > MAX_HEADER_NAME_LEN {
            return Err(invalid(format!(
                "{} header name must be 1..={MAX_HEADER_NAME_LEN} characters",
                direction.label()
            )));
        }
        if !name.bytes().all(is_token_byte) {
            return Err(invalid(format!(
                "{} header name `{name}` is not a valid HTTP token",
                direction.label()
            )));
        }
        let lowered = name.to_ascii_lowercase();
        if RESERVED_HEADERS.contains(&lowered.as_str()) {
            return Err(invalid(format!(
                "{} header `{lowered}` is a framing or hop-by-hop header and must not be rewritten",
                direction.label()
            )));
        }
        Ok(())
    }

    fn validate_value(name: &str, value: &str, direction: HeaderDirection) -> Result<()> {
        if value.len() > MAX_HEADER_VALUE_LEN {
            return Err(invalid(format!(
                "{} header `{name}` value must be at most {MAX_HEADER_VALUE_LEN} characters",
                direction.label()
            )));
        }
        if value
            .chars()
            .any(|c| (c.is_control() && c != '\t') || c == '\u{7f}')
        {
            return Err(invalid(format!(
                "{} header `{name}` value must not contain control characters",
                direction.label()
            )));
        }
        Ok(())
    }
}

/// Which set a pair belongs to; used for error messages only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderDirection {
    /// `requestHeaders.set`: applied to the request the agent forwards.
    Request,
    /// `responseHeaders.set`: applied to the response the visitor receives.
    Response,
}

impl HeaderDirection {
    /// The word used in validation messages.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Response => "response",
        }
    }
}

/// FRP-shaped header set: a `set` map of name → value.
///
/// A map is what FRP's `requestHeaders.set` / `responseHeaders.set` are, so an
/// operator's existing configuration needs no translation; duplicate names are
/// therefore impossible at this layer (the internal pair list is still checked
/// because it can also be built from a registration message).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderSetConfig {
    /// Headers to set (replace or add) in this direction.
    #[serde(default)]
    pub set: BTreeMap<String, String>,
}

impl HeaderSetConfig {
    /// True when nothing would be rewritten.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Validates the map and converts it to ordered pairs.
    pub fn to_pairs(&self, direction: HeaderDirection) -> Result<Vec<HeaderPair>> {
        if self.set.len() > MAX_HEADER_PAIRS {
            return Err(invalid(format!(
                "{}Headers.set must contain at most {MAX_HEADER_PAIRS} entries",
                direction_set_name(direction)
            )));
        }
        self.set
            .iter()
            .map(|(name, value)| HeaderPair::parse(name, value, direction))
            .collect()
    }
}

/// The complete rewrite policy of one route.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteHeaderRewrite {
    /// Replace the `Host` header before the request reaches the agent
    /// (FRP `hostHeaderRewrite`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Headers set on the request forwarded to the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_set: Vec<HeaderPair>,
    /// Headers set on the response returned to the visitor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_set: Vec<HeaderPair>,
}

impl RouteHeaderRewrite {
    /// True when the route rewrites nothing (the default, and what every route
    /// predating this capability has).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.host.is_none() && self.request_set.is_empty() && self.response_set.is_empty()
    }

    /// The `Host` value to apply, if any.
    #[must_use]
    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    /// Request-side pairs.
    #[must_use]
    pub fn request_set(&self) -> &[HeaderPair] {
        &self.request_set
    }

    /// Response-side pairs.
    #[must_use]
    pub fn response_set(&self) -> &[HeaderPair] {
        &self.response_set
    }

    /// Validates a fully built policy.
    ///
    /// Used by every construction path (§38 "validate at the boundary"):
    /// configuration loading, registration, and declaration.
    pub fn validate(&self) -> Result<()> {
        if let Some(host) = self.host.as_deref() {
            validate_host_rewrite(host)?;
        }
        validate_pairs(&self.request_set, HeaderDirection::Request)?;
        validate_pairs(&self.response_set, HeaderDirection::Response)?;
        Ok(())
    }

    /// Rejects a rewrite policy on a plane that carries no headers.
    ///
    /// `stcp`/`sudp`/`tcp`/`udp` relay opaque bytes: a configured
    /// `hostHeaderRewrite` there would never take effect, and a silent no-op is
    /// exactly the "looks supported, does nothing" shape this repository
    /// refuses to ship.
    pub fn validate_for_protocol(&self, protocol: TunnelProtocolKind) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        if protocol == TunnelProtocolKind::Http {
            return Ok(());
        }
        Err(invalid(format!(
            "header rewriting (hostHeaderRewrite / requestHeaders / responseHeaders) is HTTP-only, but the route is `{}`",
            protocol.wire_label()
        )))
    }
}

fn validate_pairs(pairs: &[HeaderPair], direction: HeaderDirection) -> Result<()> {
    if pairs.len() > MAX_HEADER_PAIRS {
        return Err(invalid(format!(
            "{} set must contain at most {MAX_HEADER_PAIRS} headers",
            direction.label()
        )));
    }
    for pair in pairs {
        HeaderPair::parse(&pair.name, &pair.value, direction)?;
    }
    Ok(())
}

/// Validates a `hostHeaderRewrite` value: an authority, not a URL.
fn validate_host_rewrite(host: &str) -> Result<()> {
    let host = host.trim();
    if host.is_empty() || host.len() > MAX_HOST_REWRITE_LEN {
        return Err(invalid(format!(
            "hostHeaderRewrite must be 1..={MAX_HOST_REWRITE_LEN} characters"
        )));
    }
    if host
        .chars()
        .any(|c| c.is_ascii_control() || c.is_whitespace())
    {
        return Err(invalid(
            "hostHeaderRewrite must not contain whitespace or control characters",
        ));
    }
    if host.contains('/') || host.contains('?') || host.contains('#') || host.contains('@') {
        return Err(invalid(format!(
            "hostHeaderRewrite `{host}` must be a host or host:port, not a URL"
        )));
    }
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(']') => (name, Some(port)),
        _ => (host, None),
    };
    if name.is_empty() {
        return Err(invalid(format!(
            "hostHeaderRewrite `{host}` has an empty host part"
        )));
    }
    if let Some(port) = port {
        let parsed = port
            .parse::<u16>()
            .map_err(|_| invalid(format!("hostHeaderRewrite `{host}` has an invalid port")))?;
        if parsed == 0 {
            return Err(invalid(format!(
                "hostHeaderRewrite `{host}` must not use port 0"
            )));
        }
    }
    Ok(())
}

fn direction_set_name(direction: HeaderDirection) -> &'static str {
    match direction {
        HeaderDirection::Request => "request",
        HeaderDirection::Response => "response",
    }
}

/// RFC 9110 `tchar`.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn invalid(reason: impl Into<String>) -> TunnelError {
    TunnelError::Validation {
        field: ValidationField::Config,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(name: &str, value: &str) -> HeaderPair {
        HeaderPair::parse(name, value, HeaderDirection::Request).expect("pair is valid")
    }

    #[test]
    fn names_are_lowercased_and_values_trimmed() {
        let parsed = pair("X-From", "  gateway  ");
        assert_eq!(parsed.name, "x-from");
        assert_eq!(parsed.value, "gateway");
    }

    #[test]
    fn framing_and_hop_by_hop_headers_are_refused() {
        for name in [
            "Content-Length",
            "transfer-encoding",
            "Connection",
            "Upgrade",
            "TE",
            "Trailer",
            "Expect",
            "Proxy-Authorization",
        ] {
            let error = HeaderPair::parse(name, "x", HeaderDirection::Request)
                .expect_err("reserved header must be refused");
            assert!(
                error.to_string().contains("framing or hop-by-hop"),
                "{name}: {error}"
            );
        }
    }

    #[test]
    fn host_is_only_settable_through_its_own_field() {
        let error = HeaderPair::parse("Host", "backend.internal", HeaderDirection::Request)
            .expect_err("host must be refused inside a set");
        assert!(error.to_string().contains("hop-by-hop"), "{error}");
    }

    #[test]
    fn header_injection_is_impossible_by_construction() {
        for value in ["a\r\nX-Evil: 1", "a\nb", "a\rb", "a\u{0}b", "a\u{7f}b"] {
            assert!(
                HeaderPair::parse("X-Safe", value, HeaderDirection::Request).is_err(),
                "value {value:?} must be refused"
            );
        }
        assert!(HeaderPair::parse("X Bad", "v", HeaderDirection::Request).is_err());
        assert!(HeaderPair::parse("", "v", HeaderDirection::Request).is_err());
    }

    #[test]
    fn a_tab_inside_a_value_is_legal_but_other_controls_are_not() {
        assert!(HeaderPair::parse("X-Safe", "a\tb", HeaderDirection::Request).is_ok());
        assert!(HeaderPair::parse("X-Safe", "a\u{1}b", HeaderDirection::Request).is_err());
    }

    #[test]
    fn bounds_are_enforced() {
        let long_name = "x".repeat(MAX_HEADER_NAME_LEN + 1);
        assert!(HeaderPair::parse(&long_name, "v", HeaderDirection::Request).is_err());
        let long_value = "v".repeat(MAX_HEADER_VALUE_LEN + 1);
        assert!(HeaderPair::parse("X-Safe", &long_value, HeaderDirection::Request).is_err());

        let mut set = BTreeMap::new();
        for index in 0..=MAX_HEADER_PAIRS {
            set.insert(format!("x-h{index}"), "v".to_owned());
        }
        let error = HeaderSetConfig { set }
            .to_pairs(HeaderDirection::Request)
            .expect_err("too many pairs must be refused");
        assert!(error.to_string().contains("at most"), "{error}");
    }

    #[test]
    fn host_rewrite_accepts_authorities_and_rejects_urls() {
        for host in [
            "backend.internal",
            "backend.internal:8080",
            "127.0.0.1:3000",
        ] {
            let policy = RouteHeaderRewrite {
                host: Some(host.to_owned()),
                ..RouteHeaderRewrite::default()
            };
            policy.validate().expect("authority is valid");
        }
        for host in [
            "https://backend.internal",
            "backend.internal/path",
            "backend.internal?x=1",
            "user@backend.internal",
            "backend.internal:0",
            "backend.internal:not-a-port",
            "with space",
        ] {
            let policy = RouteHeaderRewrite {
                host: Some(host.to_owned()),
                ..RouteHeaderRewrite::default()
            };
            assert!(policy.validate().is_err(), "{host} must be refused");
        }
    }

    #[test]
    fn rewriting_is_refused_on_planes_without_headers() {
        let policy = RouteHeaderRewrite {
            host: Some("backend.internal".to_owned()),
            ..RouteHeaderRewrite::default()
        };
        assert!(policy
            .validate_for_protocol(TunnelProtocolKind::Http)
            .is_ok());
        for protocol in [
            TunnelProtocolKind::Tcp,
            TunnelProtocolKind::Udp,
            TunnelProtocolKind::Stcp,
            TunnelProtocolKind::Sudp,
        ] {
            let error = policy
                .validate_for_protocol(protocol)
                .expect_err("non-HTTP plane must be refused");
            assert!(error.to_string().contains("HTTP-only"), "{error}");
        }
        // An empty policy is fine everywhere: it is the default.
        RouteHeaderRewrite::default()
            .validate_for_protocol(TunnelProtocolKind::Tcp)
            .expect("empty policy is plane-agnostic");
    }

    #[test]
    fn empty_policy_reports_itself_as_empty() {
        assert!(RouteHeaderRewrite::default().is_empty());
        assert!(!RouteHeaderRewrite {
            request_set: vec![pair("x-from", "gw")],
            ..RouteHeaderRewrite::default()
        }
        .is_empty());
    }

    #[test]
    fn policy_round_trips_through_camel_case_wire_names() {
        let policy = RouteHeaderRewrite {
            host: Some("backend.internal".to_owned()),
            request_set: vec![pair("X-From", "gateway")],
            response_set: vec![HeaderPair::parse(
                "X-Served-By",
                "tunnel",
                HeaderDirection::Response,
            )
            .expect("pair")],
        };
        let encoded = serde_json::to_string(&policy).expect("serializes");
        assert!(encoded.contains("\"requestSet\""), "{encoded}");
        assert!(
            encoded.contains("\"host\":\"backend.internal\""),
            "{encoded}"
        );
        let decoded: RouteHeaderRewrite = serde_json::from_str(&encoded).expect("parses");
        assert_eq!(decoded, policy);
    }

    #[test]
    fn an_empty_policy_serializes_to_nothing() {
        let encoded = serde_json::to_string(&RouteHeaderRewrite::default()).expect("serializes");
        assert_eq!(encoded, "{}");
    }

    #[test]
    fn the_frp_shaped_set_configuration_converts_to_pairs() {
        let mut set = BTreeMap::new();
        set.insert("X-Real-Host".to_owned(), "backend.internal".to_owned());
        let pairs = HeaderSetConfig { set }
            .to_pairs(HeaderDirection::Request)
            .expect("converts");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].name, "x-real-host");
        assert_eq!(pairs[0].value, "backend.internal");
    }
}
