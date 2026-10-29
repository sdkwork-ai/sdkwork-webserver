//! SCGI protocol codec (`scgi_pass`).
//!
//! A SCGI request is one netstring — `<decimal-length>:<header-bytes>,` — where
//! `<header-bytes>` is the NUL-terminated `NAME\0VALUE\0` variable list. The
//! `CONTENT_LENGTH` variable is mandatory and nginx writes it first; the HTTP
//! request body follows the comma verbatim and is *not* part of the netstring
//! length.
//!
//! SCGI responses are HTTP responses: nginx first tries to parse an HTTP status
//! line and falls back to CGI-style headers (`Status:` / `Location:`) when the
//! response is header-only, so the parser accepts either shape.
//!
//! # Examples
//!
//! ```
//! use sdkwork_webserver_upstream_protocol::scgi::encode_request;
//!
//! let variables = vec![
//!     ("CONTENT_LENGTH".to_owned(), "0".to_owned()),
//!     ("REQUEST_METHOD".to_owned(), "GET".to_owned()),
//! ];
//! let wire = encode_request(&variables, b"", &Default::default()).expect("scgi request");
//! // `<decimal-length>:CONTENT_LENGTH\0CONTENT-LENGTH-VALUE\0...,`
//! let colon = wire.iter().position(|byte| *byte == b':').expect("netstring prefix");
//! assert!(wire[colon + 1..].starts_with(b"CONTENT_LENGTH\0"));
//! assert_eq!(wire.last().copied(), Some(b','));
//! ```

use crate::{
    cgi::enforce_bounds,
    error::{Error, Result},
    header_block::HttpResponseHead,
    limits::Limits,
    response_stream::{ResponseStreamParser, StreamEvent},
};

/// Protocol name used in error messages.
const PROTOCOL: &str = "scgi";
/// The netstring terminator that ends the SCGI request header.
pub const NETSTRING_TERMINATOR: u8 = b',';

/// Encode a SCGI request (netstring header plus body) into one buffer.
///
/// The variables must contain exactly one `CONTENT_LENGTH` entry; it is
/// re-emitted first (nginx always writes it first) and the remaining variables
/// keep their order. The body is appended after the netstring terminator and
/// must match the declared `CONTENT_LENGTH`.
///
/// # Errors
///
/// - [`Error::Protocol`] when `CONTENT_LENGTH` is missing/duplicated, a name is
///   not a valid token, a value contains a NUL byte, or the declared length
///   does not equal the body length.
/// - [`Error::LimitExceeded`] when the header exceeds `limits`
///   (`max_scgi_header_bytes` / `max_params` / `max_params_bytes`).
///
/// # Examples
///
/// ```
/// use sdkwork_webserver_upstream_protocol::scgi::encode_request;
///
/// let variables = vec![("CONTENT_LENGTH".to_owned(), "5".to_owned())];
/// let wire = encode_request(&variables, b"hello", &Default::default()).expect("scgi");
/// assert!(wire.ends_with(b"hello"));
/// ```
pub fn encode_request(
    variables: &[(String, String)],
    body: &[u8],
    limits: &Limits,
) -> Result<Vec<u8>> {
    let mut head = encode_request_head(variables, body.len(), limits)?;
    head.extend_from_slice(body);
    Ok(head)
}

/// Encode only the SCGI netstring header for a request whose body length is
/// `content_length`; the caller writes the body itself.
///
/// # Errors
///
/// See [`encode_request`].
pub fn encode_request_head(
    variables: &[(String, String)],
    content_length: usize,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    if variables.len() > limits.max_params {
        return Err(Error::limit(
            PROTOCOL,
            "SCGI variable count",
            variables.len(),
            limits.max_params,
        ));
    }
    let mut content_length_value: Option<&str> = None;
    let mut header_body = Vec::new();
    let expected = content_length.to_string();
    // CONTENT_LENGTH must come first; nginx rewrites the caller's value with
    // the request body length, and the body framing would be ambiguous if the
    // declared value disagreed with the bytes that follow the netstring.
    for (name, value) in variables {
        validate_name(name)?;
        if value.bytes().any(|byte| byte == 0) {
            return Err(Error::protocol(
                PROTOCOL,
                "SCGI variable value contains a NUL byte",
            ));
        }
        if name.eq_ignore_ascii_case("CONTENT_LENGTH") {
            if content_length_value.is_some() {
                return Err(Error::protocol(
                    PROTOCOL,
                    "SCGI request declares CONTENT_LENGTH more than once",
                ));
            }
            content_length_value = Some(value.as_str());
        }
    }
    let Some(declared) = content_length_value else {
        return Err(Error::protocol(
            PROTOCOL,
            "SCGI request requires a CONTENT_LENGTH variable",
        ));
    };
    if !declared.is_empty() {
        let declared_length = declared.parse::<usize>().map_err(|_| {
            Error::protocol(PROTOCOL, "SCGI CONTENT_LENGTH is not a decimal number")
        })?;
        if declared_length != content_length {
            return Err(Error::protocol(
                PROTOCOL,
                "SCGI CONTENT_LENGTH disagrees with the request body length",
            ));
        }
    }
    push_pair(&mut header_body, "CONTENT_LENGTH", &expected);
    for (name, value) in variables {
        if name.eq_ignore_ascii_case("CONTENT_LENGTH") {
            continue;
        }
        push_pair(&mut header_body, name, value);
    }
    if header_body.len() > limits.max_scgi_header_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "SCGI header bytes",
            header_body.len(),
            limits.max_scgi_header_bytes,
        ));
    }
    enforce_bounds(variables, &limits)?;

    let mut wire = Vec::with_capacity(header_body.len() + 12);
    wire.extend_from_slice(header_body.len().to_string().as_bytes());
    wire.push(b':');
    wire.extend_from_slice(&header_body);
    wire.push(NETSTRING_TERMINATOR);
    Ok(wire)
}

fn push_pair(out: &mut Vec<u8>, name: &str, value: &str) {
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(Error::protocol(
            PROTOCOL,
            "SCGI variable name must contain only ASCII alphanumerics and `_`",
        ));
    }
    Ok(())
}

/// A parsed SCGI response event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScgiResponseEvent {
    /// Response header block parsed (HTTP status line or CGI headers).
    Head(HttpResponseHead),
    /// Raw response body bytes.
    Body(Vec<u8>),
}

/// Incremental SCGI response parser.
///
/// SCGI responses may start with an HTTP status line or with CGI-style headers;
/// both shapes are accepted, exactly as nginx's SCGI status-line/header
/// fallback does.
#[derive(Debug, Clone)]
pub struct ScgiResponseParser {
    inner: ResponseStreamParser,
}

impl ScgiResponseParser {
    /// Create a parser enforcing `limits`.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            inner: ResponseStreamParser::new(
                PROTOCOL,
                crate::header_block::HeaderStyle::HttpOrCgi,
                limits,
            ),
        }
    }

    /// Feed raw upstream bytes.
    ///
    /// # Errors
    ///
    /// Malformed, oversized or incomplete-until-`finish` header blocks fail
    /// closed; see [`crate::header_block::parse_response_head`].
    pub fn feed(&mut self, input: &[u8]) -> Result<Vec<ScgiResponseEvent>> {
        Ok(self
            .inner
            .feed(input)?
            .into_iter()
            .map(|event| match event {
                StreamEvent::Head(head) => ScgiResponseEvent::Head(head),
                StreamEvent::Body(body) => ScgiResponseEvent::Body(body),
            })
            .collect())
    }

    /// The parsed response head, once available.
    #[must_use]
    pub fn head(&self) -> Option<&HttpResponseHead> {
        self.inner.head()
    }

    /// Validate that the response header block was complete.
    ///
    /// # Errors
    ///
    /// [`Error::Protocol`] when the stream ended inside the header block.
    pub fn finish(&self) -> Result<()> {
        self.inner.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variables(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// Decode the netstring header the encoder produced.
    fn split_netstring(wire: &[u8]) -> (&[u8], &[u8]) {
        let colon = wire.iter().position(|byte| *byte == b':').expect("colon");
        let declared = std::str::from_utf8(&wire[..colon])
            .expect("ascii length")
            .parse::<usize>()
            .expect("decimal length");
        let body = &wire[colon + 1..colon + 1 + declared];
        assert_eq!(wire[colon + 1 + declared], NETSTRING_TERMINATOR);
        (body, &wire[colon + 2 + declared..])
    }

    #[test]
    fn netstring_length_is_decimal_and_counts_nul_separated_pairs() {
        let wire = encode_request(
            &variables(&[
                ("CONTENT_LENGTH", "0"),
                ("REQUEST_METHOD", "GET"),
                ("SERVER_PROTOCOL", "HTTP/1.1"),
            ]),
            b"",
            &Limits::default(),
        )
        .expect("scgi");
        let (body, rest) = split_netstring(&wire);
        assert!(rest.is_empty());
        let expected_len = "CONTENT_LENGTH\0".len()
            + "0\0".len()
            + "REQUEST_METHOD\0".len()
            + "GET\0".len()
            + "SERVER_PROTOCOL\0".len()
            + "HTTP/1.1\0".len();
        assert_eq!(body.len(), expected_len);
        assert!(body.starts_with(b"CONTENT_LENGTH\0"));
    }

    #[test]
    fn body_follows_the_netstring_and_matches_content_length() {
        let wire = encode_request(
            &variables(&[("CONTENT_LENGTH", "5"), ("REQUEST_METHOD", "POST")]),
            b"hello",
            &Limits::default(),
        )
        .expect("scgi");
        let (_, rest) = split_netstring(&wire);
        assert_eq!(rest, b"hello");
    }

    #[test]
    fn content_length_is_always_first_and_rewritten() {
        let wire = encode_request_head(
            &variables(&[("REQUEST_METHOD", "POST"), ("CONTENT_LENGTH", "5")]),
            5,
            &Limits::default(),
        )
        .expect("head");
        let (body, _) = split_netstring(&wire);
        let occurrences = body
            .windows(b"CONTENT_LENGTH\0".len())
            .filter(|window| *window == b"CONTENT_LENGTH\0")
            .count();
        assert_eq!(occurrences, 1, "CONTENT_LENGTH must appear exactly once");
    }

    #[test]
    fn invalid_requests_fail_closed() {
        let limits = Limits::default();
        // Missing CONTENT_LENGTH.
        assert!(encode_request(&variables(&[("REQUEST_METHOD", "GET")]), b"", &limits).is_err());
        // Duplicate CONTENT_LENGTH.
        assert!(encode_request(
            &variables(&[("CONTENT_LENGTH", "0"), ("Content_Length", "0")]),
            b"",
            &limits
        )
        .is_err());
        // Declared length disagrees with the body.
        assert!(encode_request(&variables(&[("CONTENT_LENGTH", "9")]), b"hello", &limits).is_err());
        // Non-decimal declared length.
        assert!(encode_request(&variables(&[("CONTENT_LENGTH", "x")]), b"", &limits).is_err());
        // Body bytes are appended verbatim, so a NUL-free header is required.
        assert!(encode_request(
            &variables(&[("CONTENT_LENGTH", "0"), ("BAD\0NAME", "x")]),
            b"",
            &limits
        )
        .is_err());
        assert!(encode_request(
            &variables(&[("CONTENT_LENGTH", "0"), ("NAME", "va\0lue")]),
            b"",
            &limits
        )
        .is_err());
    }

    #[test]
    fn bounds_are_enforced() {
        let limits = Limits {
            max_scgi_header_bytes: 32,
            ..Limits::default()
        };
        assert!(matches!(
            encode_request(
                &variables(&[("CONTENT_LENGTH", "0"), ("LONG", &"v".repeat(64))]),
                b"",
                &limits
            ),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn header_style_response_is_parsed() {
        let mut parser = ScgiResponseParser::new(Limits::default());
        let events = parser
            .feed(b"Status: 204 No Content\r\nX-Empty: 1\r\n\r\n")
            .expect("head");
        let head = events
            .iter()
            .find_map(|event| match event {
                ScgiResponseEvent::Head(head) => Some(head.clone()),
                ScgiResponseEvent::Body(_) => None,
            })
            .expect("head event");
        assert_eq!(head.status, 204);
        assert_eq!(head.header("x-empty"), Some("1"));
        parser.finish().expect("complete");
    }

    #[test]
    fn http_style_response_is_parsed_with_streaming_body() {
        let mut parser = ScgiResponseParser::new(Limits::default());
        let events = parser
            .feed(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nab")
            .expect("events");
        assert!(events
            .iter()
            .any(|event| matches!(event, ScgiResponseEvent::Head(head) if head.status == 200)));
        assert_eq!(
            parser.feed(b"c").expect("body"),
            vec![ScgiResponseEvent::Body(b"c".to_vec())]
        );
        parser.finish().expect("complete");
    }

    #[test]
    fn malformed_response_fails_closed() {
        let mut parser = ScgiResponseParser::new(Limits::default());
        assert!(parser.feed(b"not a header block\r\n\r\n").is_err());
        let mut parser = ScgiResponseParser::new(Limits::default());
        assert!(parser.feed(b"Status: 200 OK\r\n").unwrap().is_empty());
        assert!(parser.finish().is_err());
    }
}
