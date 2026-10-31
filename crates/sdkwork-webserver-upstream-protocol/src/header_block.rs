//! CGI/HTTP-like response header-block parsing shared by FastCGI, SCGI and
//! uWSGI.
//!
//! The three protocols frame their response header block differently but the
//! *content* is one of two shapes, both defined by the CGI/1.1 conventions
//! nginx implements:
//!
//! - **CGI-style** — `Name: value` lines terminated by an empty line; the
//!   status comes from a `Status:` header (default `200`, and `Location:`
//!   without `Status:` means `302`). FastCGI always uses this shape.
//! - **HTTP-style** — an `HTTP/<version> <code> [reason]` status line followed
//!   by header lines. uWSGI `modifier1 = 72` always uses this shape; SCGI
//!   accepts either (nginx tries the status line first and falls back to CGI
//!   headers).
//!
//! [`parse_response_head`] is incremental: it returns `None` until the
//! terminating empty line has arrived, and every size/count bound is checked
//! before the block is accepted, so a header bomb cannot grow an unbounded
//! buffer in the caller.

use crate::{
    error::{Error, Result},
    limits::Limits,
};

/// Which header-block shape is legal for the upstream protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderStyle {
    /// CGI-style headers only (FastCGI `STDOUT`).
    Cgi,
    /// An HTTP status line when present, otherwise CGI-style headers (SCGI).
    HttpOrCgi,
    /// An HTTP status line is required (uWSGI `modifier1 = 72`).
    Http,
}

/// Parsed upstream response head: final status, reason phrase and header fields.
///
/// The synthetic CGI `Status` pseudo-header is consumed into [`Self::status`]
/// and is not part of [`Self::headers`]; every other field (including
/// `Location`) is preserved in wire order, with its original spelling and
/// value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponseHead {
    /// Response status code (`100..=599`).
    pub status: u16,
    /// Reason phrase from the status line or `Status:` header; empty if absent.
    pub reason: String,
    /// Header fields in wire order.
    pub headers: Vec<(String, String)>,
}

impl HttpResponseHead {
    /// First field value for `name`, matched case-insensitively.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Every field value for `name`, in wire order, matched case-insensitively.
    pub fn header_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers
            .iter()
            .filter(move |(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Declared `Content-Length`, when present and a valid decimal number.
    #[must_use]
    pub fn content_length(&self) -> Option<u64> {
        self.header("content-length")
            .and_then(|value| value.parse::<u64>().ok())
    }

    /// Whether the status is informational (`1xx`), which the caller must not
    /// treat as the final response.
    #[must_use]
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.status)
    }
}

/// Maximum number of bytes scanned for the header terminator beyond the
/// configured block bound (the terminator itself is four bytes at most).
const TERMINATOR_SENTINEL_BYTES: usize = 4;

/// Parse one upstream response header block incrementally.
///
/// Returns `Ok(None)` when the terminating empty line has not arrived yet;
/// returns `Ok(Some((head, consumed)))` when it has, where `consumed` is the
/// number of leading bytes of `bytes` that belong to the header block
/// (including the blank-line terminator). Any body bytes follow `consumed`.
///
/// # Errors
///
/// - [`Error::LimitExceeded`] when no terminator arrives within the configured
///   header-block bound, or a single line exceeds the line bound.
/// - [`Error::Protocol`] for malformed lines, duplicate `Status:` headers,
///   obsolete line folding, too many fields, or inconclusive CGI status.
/// - [`Error::InvalidStatus`] when a status line/header carries a value outside
///   `100..=599`.
pub fn parse_response_head(
    protocol: &'static str,
    bytes: &[u8],
    style: HeaderStyle,
    limits: &Limits,
) -> Result<Option<(HttpResponseHead, usize)>> {
    let Some((block_end, consumed)) = find_header_terminator(bytes) else {
        if bytes.len()
            > limits
                .max_header_block_bytes
                .saturating_add(TERMINATOR_SENTINEL_BYTES)
        {
            return Err(Error::limit(
                protocol,
                "response header block",
                bytes.len(),
                limits.max_header_block_bytes,
            ));
        }
        return Ok(None);
    };
    if block_end > limits.max_header_block_bytes {
        return Err(Error::limit(
            protocol,
            "response header block",
            block_end,
            limits.max_header_block_bytes,
        ));
    }
    let head = parse_header_block(protocol, &bytes[..block_end], style, limits)?;
    Ok(Some((head, consumed)))
}

/// Locate the end of the header block.
///
/// Returns `(block_length, consumed_length)` where `block_length` excludes the
/// terminating empty line and `consumed_length` includes it. Accepts CRLF and
/// bare-LF line endings because both CGI implementations and nginx's header
/// parser tolerate them.
fn find_header_terminator(bytes: &[u8]) -> Option<(usize, usize)> {
    for index in 0..bytes.len() {
        if bytes[index] != b'\n' {
            continue;
        }
        // `\n\n`: the blank line is this LF; the block keeps the previous LF.
        if index >= 1 && bytes[index - 1] == b'\n' {
            return Some((index, index + 1));
        }
        // `\r\n\r\n`: the blank line starts at the CR before this LF.
        if index >= 2 && bytes[index - 1] == b'\r' && bytes[index - 2] == b'\n' {
            return Some((index - 1, index + 1));
        }
        // A block that starts with an empty line has no fields at all.
        if index == 0 {
            return Some((0, 1));
        }
        if index == 1 && bytes[0] == b'\r' {
            return Some((0, 2));
        }
    }
    None
}

fn parse_header_block(
    protocol: &'static str,
    block: &[u8],
    style: HeaderStyle,
    limits: &Limits,
) -> Result<HttpResponseHead> {
    let block = decode_utf8(protocol, "response header block", block)?;
    let mut lines = block
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    let first = lines.next().unwrap_or("");
    let (mut status, mut reason) = (None, String::new());
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut cgi_status = false;
    let mut cgi_location = false;

    let first_is_status_line = first.starts_with("HTTP/");
    if first_is_status_line {
        if style == HeaderStyle::Cgi {
            return Err(Error::protocol(
                protocol,
                "response begins with an HTTP status line, but only CGI-style headers are allowed",
            ));
        }
        let (code, phrase) = parse_status_line(protocol, first)?;
        status = Some(code);
        reason = phrase;
    } else if style == HeaderStyle::Http {
        return Err(Error::protocol(
            protocol,
            "response does not begin with an HTTP status line",
        ));
    } else if !first.is_empty() {
        // CGI-style: the first line is already a header line.
        push_header_line(
            protocol,
            first,
            limits,
            &mut headers,
            &mut status,
            &mut reason,
            &mut cgi_status,
            &mut cgi_location,
        )?;
    }

    for line in lines {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(Error::protocol(
                protocol,
                "obsolete header line folding is not accepted",
            ));
        }
        push_header_line(
            protocol,
            line,
            limits,
            &mut headers,
            &mut status,
            &mut reason,
            &mut cgi_status,
            &mut cgi_location,
        )?;
    }

    // nginx's CGI-header fallback: with no explicit Status, a Location field
    // makes the response a 302 redirect; otherwise the status is 200.
    let status = match status {
        Some(status) => status,
        None if cgi_location => {
            reason = "Moved Temporarily".to_owned();
            302
        }
        None => {
            reason = "OK".to_owned();
            200
        }
    };
    Ok(HttpResponseHead {
        status,
        reason,
        headers,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_header_line(
    protocol: &'static str,
    line: &str,
    limits: &Limits,
    headers: &mut Vec<(String, String)>,
    status: &mut Option<u16>,
    reason: &mut String,
    cgi_status: &mut bool,
    cgi_location: &mut bool,
) -> Result<()> {
    if line.len() > limits.max_header_line_bytes {
        return Err(Error::limit(
            protocol,
            "response header line",
            line.len(),
            limits.max_header_line_bytes,
        ));
    }
    let Some((name, value)) = line.split_once(':') else {
        return Err(Error::protocol(
            protocol,
            "response header line has no `:` separator",
        ));
    };
    let name = name.trim_end_matches([' ', '\t']);
    if !is_token(name) {
        return Err(Error::protocol(
            protocol,
            "response header name is not a valid token",
        ));
    }
    let value = value.trim_matches([' ', '\t']);
    if value
        .bytes()
        .any(|byte| byte == 0 || (byte < 0x20 && byte != b'\t') || byte == 0x7f)
    {
        return Err(Error::protocol(
            protocol,
            "response header value contains a control character",
        ));
    }
    if headers.len() >= limits.max_header_count {
        return Err(Error::limit(
            protocol,
            "response header count",
            headers.len().saturating_add(1),
            limits.max_header_count,
        ));
    }

    if name.eq_ignore_ascii_case("status") {
        if *cgi_status {
            return Err(Error::protocol(
                protocol,
                "response carries more than one `Status:` header",
            ));
        }
        *cgi_status = true;
        let (code, phrase) = parse_cgi_status(protocol, value)?;
        *status = Some(code);
        *reason = phrase;
        // The synthetic status is consumed; it is not forwarded as a field.
        return Ok(());
    }
    if name.eq_ignore_ascii_case("location") {
        *cgi_location = true;
    }
    headers.push((name.to_owned(), value.to_owned()));
    Ok(())
}

fn parse_status_line(protocol: &'static str, line: &str) -> Result<(u16, String)> {
    let mut parts = line.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") || version.len() <= b"HTTP/".len() {
        return Err(Error::protocol(
            protocol,
            "response status line has an unsupported protocol version",
        ));
    }
    let Some(code) = parts.next() else {
        return Err(Error::protocol(
            protocol,
            "response status line is missing its status code",
        ));
    };
    if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::protocol(
            protocol,
            "response status line has a malformed status code",
        ));
    }
    let code = code.parse::<u16>().map_err(|_| Error::InvalidStatus {
        protocol,
        status: None,
    })?;
    validate_status(protocol, code)?;
    Ok((code, parts.next().unwrap_or("").to_owned()))
}

fn parse_cgi_status(protocol: &'static str, value: &str) -> Result<(u16, String)> {
    let (code, phrase) = match value.split_once(' ') {
        Some((code, phrase)) => (code, phrase),
        None => (value, ""),
    };
    if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::protocol(
            protocol,
            "`Status:` header has a malformed status code",
        ));
    }
    let code = code.parse::<u16>().map_err(|_| Error::InvalidStatus {
        protocol,
        status: None,
    })?;
    validate_status(protocol, code)?;
    Ok((code, phrase.to_owned()))
}

fn validate_status(protocol: &'static str, status: u16) -> Result<()> {
    if (100..=599).contains(&status) {
        Ok(())
    } else {
        Err(Error::InvalidStatus {
            protocol,
            status: Some(status),
        })
    }
}

fn decode_utf8<'a>(
    protocol: &'static str,
    field: &'static str,
    bytes: &'a [u8],
) -> Result<&'a str> {
    std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8 { protocol, field })
}

/// Whether `value` is an RFC 9110 `token`.
#[must_use]
pub fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
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
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits::default()
    }

    fn parse(style: HeaderStyle, text: &str) -> Result<Option<(HttpResponseHead, usize)>> {
        parse_response_head("test", text.as_bytes(), style, &limits())
    }

    fn head(style: HeaderStyle, text: &str) -> HttpResponseHead {
        parse(style, text)
            .expect("parse result")
            .expect("complete block")
            .0
    }

    #[test]
    fn cgi_status_and_headers_are_parsed_in_wire_order() {
        let parsed = head(
            HeaderStyle::Cgi,
            "Status: 201 Created\r\nX-First: one\r\nX-Second: two\r\n\r\nbody",
        );
        assert_eq!(parsed.status, 201);
        assert_eq!(parsed.reason, "Created");
        assert_eq!(
            parsed.headers,
            vec![
                ("X-First".to_owned(), "one".to_owned()),
                ("X-Second".to_owned(), "two".to_owned())
            ]
        );
        assert_eq!(parsed.header("x-second"), Some("two"));
    }

    #[test]
    fn cgi_defaults_and_location_redirect_match_nginx() {
        let parsed = head(HeaderStyle::Cgi, "Content-Type: text/plain\r\n\r\n");
        assert_eq!(parsed.status, 200);
        assert_eq!(parsed.reason, "OK");
        assert_eq!(parsed.header("content-type"), Some("text/plain"));

        let parsed = head(HeaderStyle::Cgi, "Location: /elsewhere\r\n\r\n");
        assert_eq!(parsed.status, 302);
        assert_eq!(parsed.reason, "Moved Temporarily");
        assert_eq!(parsed.header("location"), Some("/elsewhere"));

        // An explicit status always wins over the Location fallback.
        let parsed = head(
            HeaderStyle::Cgi,
            "Status: 307 Temporary Redirect\r\nLocation: /elsewhere\r\n\r\n",
        );
        assert_eq!(parsed.status, 307);
    }

    #[test]
    fn http_status_line_is_parsed_for_http_styles() {
        let parsed = head(
            HeaderStyle::HttpOrCgi,
            "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 5\r\n\r\n",
        );
        assert_eq!(parsed.status, 503);
        assert_eq!(parsed.reason, "Service Unavailable");
        assert_eq!(parsed.header("retry-after"), Some("5"));

        let parsed = head(HeaderStyle::Http, "HTTP/2 200\r\nX-A: 1\r\n\r\n");
        assert_eq!(parsed.status, 200);
        assert_eq!(parsed.reason, "");
    }

    #[test]
    fn http_or_cgi_falls_back_to_cgi_headers() {
        let parsed = head(HeaderStyle::HttpOrCgi, "Status: 404 Not Found\r\n\r\n");
        assert_eq!(parsed.status, 404);
    }

    #[test]
    fn incomplete_blocks_need_more_input() {
        assert!(parse(HeaderStyle::Cgi, "Status: 200 OK\r\n")
            .unwrap()
            .is_none());
        assert!(parse(HeaderStyle::Cgi, "").unwrap().is_none());
        assert!(parse(HeaderStyle::Cgi, "\r\n").unwrap().is_some());
    }

    #[test]
    fn consumed_length_excludes_the_body() {
        let (_, consumed) = parse(HeaderStyle::Cgi, "A: b\r\n\r\npayload")
            .unwrap()
            .unwrap();
        assert_eq!(consumed, "A: b\r\n\r\n".len());
    }

    #[test]
    fn bare_lf_line_endings_are_accepted() {
        let parsed = head(HeaderStyle::Cgi, "A: b\n\npayload");
        assert_eq!(parsed.header("a"), Some("b"));
    }

    #[test]
    fn malformed_blocks_fail_closed() {
        assert!(parse(HeaderStyle::Cgi, "no separator\r\n\r\n").is_err());
        assert!(parse(HeaderStyle::Cgi, "Bad Name: x\r\n\r\n").is_err());
        assert!(parse(HeaderStyle::Cgi, "A: x\r\n folded\r\n\r\n").is_err());
        assert!(parse(HeaderStyle::Cgi, "A: x\u{7f}\r\n\r\n").is_err());
        assert!(parse(HeaderStyle::Http, "Status: 200 OK\r\n\r\n").is_err());
        assert!(parse(HeaderStyle::Cgi, "HTTP/1.1 200 OK\r\n\r\n").is_err());
        assert!(parse(
            HeaderStyle::Cgi,
            "Status: 200 OK\r\nStatus: 404 Not Found\r\n\r\n"
        )
        .is_err());
        assert!(parse(HeaderStyle::Cgi, "Status: 99 Too Small\r\n\r\n").is_err());
        assert!(parse(HeaderStyle::Http, "HTTP/1.1 600 Too Big\r\n\r\n").is_err());
        assert!(parse(HeaderStyle::Http, "HTTP/1.1 abc Bad\r\n\r\n").is_err());
    }

    #[test]
    fn oversized_blocks_and_lines_fail_closed_without_growing() {
        let limits = Limits {
            max_header_block_bytes: 16,
            max_header_line_bytes: 8,
            ..Limits::default()
        };
        let error = parse_response_head(
            "test",
            b"X-Long-Header: aaaaaaaaaaaaaaaa\r\n\r\n",
            HeaderStyle::Cgi,
            &limits,
        )
        .expect_err("oversized line");
        assert!(matches!(error, Error::LimitExceeded { .. }));

        let error = parse_response_head(
            "test",
            b"X-A: 1\r\nX-B: 2\r\nX-C: 3\r\nX-D: 4\r\n",
            HeaderStyle::Cgi,
            &limits,
        )
        .expect_err("oversized block without terminator");
        assert!(matches!(error, Error::LimitExceeded { .. }));
    }

    #[test]
    fn header_count_is_bounded() {
        let limits = Limits {
            max_header_count: 2,
            ..Limits::default()
        };
        let error = parse_response_head(
            "test",
            b"A: 1\r\nB: 2\r\nC: 3\r\n\r\n",
            HeaderStyle::Cgi,
            &limits,
        )
        .expect_err("too many headers");
        assert!(matches!(error, Error::LimitExceeded { .. }));
    }

    #[test]
    fn content_length_and_informational_status_helpers() {
        let parsed = head(
            HeaderStyle::Http,
            "HTTP/1.1 103 Early Hints\r\nContent-Length: 12\r\n\r\n",
        );
        assert_eq!(parsed.content_length(), Some(12));
        assert!(parsed.is_informational());
        assert_eq!(
            head(HeaderStyle::Cgi, "Content-Length: nope\r\n\r\n").content_length(),
            None
        );
    }

    #[test]
    fn non_utf8_header_block_fails_closed() {
        let error = parse_response_head("test", b"X-A: \xff\r\n\r\n", HeaderStyle::Cgi, &limits())
            .expect_err("non-utf8");
        assert!(matches!(error, Error::InvalidUtf8 { .. }));
    }
}
