//! uWSGI protocol codec (`uwsgi_pass`).
//!
//! The uwsgi protocol packet is a 4-byte header followed by a variable-length
//! body (uWSGI Protocol documentation):
//!
//! ```text
//! modifier1(1) | datasize_le(2) | modifier2(1) | body(datasize)
//! ```
//!
//! For a standard request (`ngx_http_uwsgi_module` defaults: modifier1 `0`,
//! modifier2 `0`) the body is a sequence of vars, each one encoded as a 16-bit
//! little-endian key length, the key bytes, a 16-bit little-endian value length
//! and the value bytes. nginx appends the configured `uwsgi_string` after the
//! vars, and the HTTP request body follows the packet verbatim (it is *not*
//! counted in `datasize`).
//!
//! nginx parses a uWSGI response as a raw HTTP response (status line, header
//! block, body) — the `modifier1 = 72` "HTTP response" form — and falls back to
//! CGI-style headers (`Status:` / `Location:`) when the status line is absent,
//! so the response parser accepts either shape.
//!
//! # Examples
//!
//! ```
//! use sdkwork_webserver_upstream_protocol::uwsgi::{decode_vars, encode_vars};
//!
//! let vars = vec![("REQUEST_METHOD".to_owned(), "GET".to_owned())];
//! let payload = encode_vars(&vars).expect("vars encode");
//! assert_eq!(decode_vars(&payload).expect("vars decode"), vars);
//! ```

use crate::{
    cgi::enforce_bounds,
    error::{Error, Result},
    header_block::{HeaderStyle, HttpResponseHead},
    limits::Limits,
    response_stream::{ResponseStreamParser, StreamEvent},
};

/// Protocol name used in error messages.
const PROTOCOL: &str = "uwsgi";
/// Request packet header size.
pub const PACKET_HEADER_BYTES: usize = 4;
/// `modifier1` for a standard WSGI request.
pub const MODIFIER1_REQUEST: u8 = 0;
/// `modifier1` uWSGI uses for a raw HTTP response (the "HTTP" signature).
pub const MODIFIER1_HTTP: u8 = 72;
/// `modifier2` for a standard request.
pub const MODIFIER2_REQUEST: u8 = 0;

/// A decoded uWSGI packet header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UwsgiPacketHeader {
    /// Packet modifier 1 (packet type / handler selector).
    pub modifier1: u8,
    /// Packet modifier 2 (sub-selector).
    pub modifier2: u8,
    /// Body size from the header.
    pub data_size: u16,
}

impl UwsgiPacketHeader {
    /// Create a header for a body of `data_size` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LimitExceeded`] when `data_size` does not fit the
    /// 16-bit `datasize` field.
    pub fn new(modifier1: u8, modifier2: u8, data_size: usize) -> Result<Self> {
        let data_size = u16::try_from(data_size)
            .map_err(|_| Error::limit(PROTOCOL, "uwsgi datasize", data_size, u16::MAX as usize))?;
        Ok(Self {
            modifier1,
            modifier2,
            data_size,
        })
    }

    /// Encode the 4-byte packet header.
    #[must_use]
    pub fn encode(&self) -> [u8; PACKET_HEADER_BYTES] {
        let [size_lo, size_hi] = self.data_size.to_le_bytes();
        [self.modifier1, size_lo, size_hi, self.modifier2]
    }

    /// Decode a 4-byte packet header.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] when fewer than four bytes are available.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let header = bytes.get(..PACKET_HEADER_BYTES).ok_or(Error::Truncated {
            protocol: PROTOCOL,
            field: "packet header",
            expected: PACKET_HEADER_BYTES.saturating_sub(bytes.len()),
        })?;
        Ok(Self {
            modifier1: header[0],
            data_size: u16::from_le_bytes([header[1], header[2]]),
            modifier2: header[3],
        })
    }
}

/// Encode uWSGI vars (length-prefixed key/value pairs).
///
/// # Errors
///
/// Returns [`Error::Protocol`] when a key/value is too long for the 16-bit
/// length prefix or the names/values are invalid, and [`Error::LimitExceeded`]
/// when the encoded payload does not fit one packet body.
pub fn encode_vars(vars: &[(String, String)]) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    for (name, value) in vars {
        validate_var(name, value)?;
        let name_len = u16::try_from(name.len())
            .map_err(|_| Error::limit(PROTOCOL, "var name", name.len(), u16::MAX as usize))?;
        let value_len = u16::try_from(value.len())
            .map_err(|_| Error::limit(PROTOCOL, "var value", value.len(), u16::MAX as usize))?;
        payload.extend_from_slice(&name_len.to_le_bytes());
        payload.extend_from_slice(name.as_bytes());
        payload.extend_from_slice(&value_len.to_le_bytes());
        payload.extend_from_slice(value.as_bytes());
    }
    if payload.len() > u16::MAX as usize {
        return Err(Error::limit(
            PROTOCOL,
            "vars payload",
            payload.len(),
            u16::MAX as usize,
        ));
    }
    Ok(payload)
}

/// Decode uWSGI vars from a packet body.
///
/// A trailing empty var (`0x0000`) is tolerated because some implementations
/// terminate the list that way.
///
/// # Errors
///
/// Returns [`Error::Framing`] for truncated length prefixes, a payload that
/// ends in the middle of a var, or an oversized packet.
pub fn decode_vars(payload: &[u8]) -> Result<Vec<(String, String)>> {
    if payload.len() > u16::MAX as usize {
        return Err(Error::limit(
            PROTOCOL,
            "vars payload",
            payload.len(),
            u16::MAX as usize,
        ));
    }
    let mut vars = Vec::new();
    let mut rest = payload;
    while !rest.is_empty() {
        let (name, after_name) = take_var(rest, "var")?;
        rest = after_name;
        if name.is_empty() && rest.is_empty() {
            break;
        }
        let (value, after_value) = take_var(rest, "var value")?;
        rest = after_value;
        let name = std::str::from_utf8(name).map_err(|_| Error::InvalidUtf8 {
            protocol: PROTOCOL,
            field: "var name",
        })?;
        let value = std::str::from_utf8(value).map_err(|_| Error::InvalidUtf8 {
            protocol: PROTOCOL,
            field: "var value",
        })?;
        vars.push((name.to_owned(), value.to_owned()));
    }
    Ok(vars)
}

fn take_var<'a>(payload: &'a [u8], field: &'static str) -> Result<(&'a [u8], &'a [u8])> {
    let Some(length_bytes) = payload.get(..2) else {
        return Err(Error::Truncated {
            protocol: PROTOCOL,
            field,
            expected: 2usize.saturating_sub(payload.len()),
        });
    };
    let length = usize::from(u16::from_le_bytes([length_bytes[0], length_bytes[1]]));
    let body = payload
        .get(2..)
        .and_then(|rest| rest.get(..length))
        .ok_or(Error::Truncated {
            protocol: PROTOCOL,
            field,
            expected: length.saturating_sub(payload.len().saturating_sub(2)),
        })?;
    Ok((body, &payload[2 + length..]))
}

fn validate_var(name: &str, value: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(Error::protocol(
            PROTOCOL,
            "var name must contain only ASCII alphanumerics and `_`",
        ));
    }
    if value.bytes().any(|byte| byte == 0) {
        return Err(Error::protocol(PROTOCOL, "var value contains a NUL byte"));
    }
    Ok(())
}

/// Encode a complete uWSGI request packet for an empty request body.
///
/// The returned bytes are the 4-byte header plus the vars payload (and the
/// optional `extra` bytes nginx appends as `uwsgi_string`). The HTTP request
/// body, if any, is written to the upstream socket immediately after these
/// bytes and is not part of the packet `datasize`.
///
/// # Errors
///
/// Returns [`Error::LimitExceeded`] when the encoded vars exceed the configured
/// packet bound, and the encoding errors of [`encode_vars`].
pub fn encode_request(
    vars: &[(String, String)],
    extra: &[u8],
    modifier1: u8,
    modifier2: u8,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    enforce_bounds(vars, &limits)?;
    let mut payload = encode_vars(vars)?;
    let total = payload.len().checked_add(extra.len()).ok_or_else(|| {
        Error::limit(
            PROTOCOL,
            "packet body",
            usize::MAX,
            limits.max_uwsgi_packet_bytes,
        )
    })?;
    if total > limits.max_uwsgi_packet_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "packet body",
            total,
            limits.max_uwsgi_packet_bytes,
        ));
    }
    payload.extend_from_slice(extra);
    let header = UwsgiPacketHeader::new(modifier1, modifier2, payload.len())?;
    let mut out = Vec::with_capacity(PACKET_HEADER_BYTES + payload.len());
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// A parsed uWSGI response event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UwsgiResponseEvent {
    /// Raw HTTP response header block parsed.
    Head(HttpResponseHead),
    /// Raw response body bytes.
    Body(Vec<u8>),
}

/// Incremental uWSGI response parser.
///
/// nginx parses a uWSGI response as a raw HTTP response and falls back to
/// CGI-style headers when no status line is present, so this parser is a thin
/// typed wrapper over the shared header-block stream parser in
/// [`HeaderStyle::HttpOrCgi`] mode.
#[derive(Debug, Clone)]
pub struct UwsgiResponseParser {
    inner: ResponseStreamParser,
}

impl UwsgiResponseParser {
    /// Create a parser enforcing `limits`.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            inner: ResponseStreamParser::new(PROTOCOL, HeaderStyle::HttpOrCgi, limits),
        }
    }

    /// Feed raw upstream bytes.
    ///
    /// # Errors
    ///
    /// See [`crate::header_block::parse_response_head`]; malformed or oversized
    /// header blocks fail closed.
    pub fn feed(&mut self, input: &[u8]) -> Result<Vec<UwsgiResponseEvent>> {
        Ok(self
            .inner
            .feed(input)?
            .into_iter()
            .map(|event| match event {
                StreamEvent::Head(head) => UwsgiResponseEvent::Head(head),
                StreamEvent::Body(body) => UwsgiResponseEvent::Body(body),
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

    #[test]
    fn packet_header_round_trips_in_little_endian_datasize() {
        let header = UwsgiPacketHeader::new(0, 0, 0x1234).expect("header");
        let encoded = header.encode();
        assert_eq!(encoded, [0, 0x34, 0x12, 0]);
        assert_eq!(UwsgiPacketHeader::decode(&encoded).unwrap(), header);
        assert!(matches!(
            UwsgiPacketHeader::decode(&encoded[..3]),
            Err(Error::Truncated { .. })
        ));
        assert!(UwsgiPacketHeader::new(0, 0, usize::from(u16::MAX) + 1).is_err());
    }

    #[test]
    fn vars_round_trip_with_16_bit_length_prefixes() {
        let vars = vec![
            ("REQUEST_METHOD".to_owned(), "GET".to_owned()),
            ("QUERY_STRING".to_owned(), String::new()),
            ("HTTP_X_LONG".to_owned(), "v".repeat(300)),
        ];
        let payload = encode_vars(&vars).expect("encode");
        assert_eq!(decode_vars(&payload).expect("decode"), vars);
        // Little-endian key length for a 14-byte name.
        assert_eq!(&payload[..2], &14u16.to_le_bytes());
    }

    #[test]
    fn request_packet_header_datasize_covers_vars_and_extra_only() {
        let vars = vec![("REQUEST_METHOD".to_owned(), "GET".to_owned())];
        let wire = encode_request(
            &vars,
            b"X-EXTRA",
            MODIFIER1_REQUEST,
            MODIFIER2_REQUEST,
            &Limits::default(),
        )
        .expect("request");
        let header = UwsgiPacketHeader::decode(&wire).expect("header");
        assert_eq!(header.modifier1, MODIFIER1_REQUEST);
        assert_eq!(header.modifier2, MODIFIER2_REQUEST);
        assert_eq!(
            usize::from(header.data_size),
            wire.len() - PACKET_HEADER_BYTES
        );
        let vars_payload = &wire[PACKET_HEADER_BYTES..wire.len() - b"X-EXTRA".len()];
        assert_eq!(decode_vars(vars_payload).unwrap(), vars);
    }

    #[test]
    fn malformed_vars_fail_closed() {
        assert!(decode_vars(&[0x05, 0x00, b'a']).is_err());
        assert!(decode_vars(&[0xff, 0xff]).is_err());
        let mut payload = encode_vars(&[("A".to_owned(), "b".to_owned())]).unwrap();
        payload.truncate(payload.len() - 1);
        assert!(decode_vars(&payload).is_err());
        let mut invalid_utf8 = Vec::new();
        invalid_utf8.extend_from_slice(&1u16.to_le_bytes());
        invalid_utf8.push(0xff);
        invalid_utf8.extend_from_slice(&1u16.to_le_bytes());
        invalid_utf8.push(b'x');
        assert!(matches!(
            decode_vars(&invalid_utf8),
            Err(Error::InvalidUtf8 { .. })
        ));
    }

    #[test]
    fn packet_bounds_are_enforced() {
        let limits = Limits {
            max_uwsgi_packet_bytes: 32,
            ..Limits::default()
        };
        let vars = vec![("REQUEST_METHOD".to_owned(), "x".repeat(64))];
        assert!(matches!(
            encode_request(&vars, b"", 0, 0, &limits),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn response_is_parsed_as_raw_http_with_streaming_body() {
        let mut parser = UwsgiResponseParser::new(Limits::default());
        assert!(parser
            .feed(b"HTTP/1.1 200 OK\r\nContent-")
            .unwrap()
            .is_empty());
        let events = parser
            .feed(b"Type: text/plain\r\n\r\na")
            .expect("head events");
        assert!(events.iter().any(|event| matches!(
            event,
            UwsgiResponseEvent::Head(head) if head.status == 200
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event, UwsgiResponseEvent::Body(body) if body == b"a")));
        let events = parser.feed(b"bc").expect("body events");
        assert_eq!(events, vec![UwsgiResponseEvent::Body(b"bc".to_vec())]);
        parser.finish().expect("complete");
    }

    #[test]
    fn cgi_style_response_fallback_is_accepted_like_nginx() {
        let mut parser = UwsgiResponseParser::new(Limits::default());
        let events = parser.feed(b"Status: 404 Not Found\r\n\r\n").expect("head");
        assert!(events.iter().any(|event| matches!(
            event,
            UwsgiResponseEvent::Head(head) if head.status == 404
        )));
        parser.finish().expect("complete");

        let mut parser = UwsgiResponseParser::new(Limits::default());
        assert!(parser.feed(b"not a header line\r\n\r\n").is_err());
    }
}
