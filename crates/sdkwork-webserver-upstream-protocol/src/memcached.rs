//! memcached text-protocol subset used by `memcached_pass`.
//!
//! nginx's memcached module only ever issues `get <key>\r\n` and parses a
//! `VALUE`/`END` reply, so that is the required subset here; a bounded `set`
//! encoder is provided as well because the edge may later populate cache
//! entries from responses. Values are carried as raw bytes (binary-safe), but
//! the memcached *binary* protocol is out of scope for this crate.
//!
//! Key escaping matches nginx `NGX_ESCAPE_MEMCACHED`: bytes `0x00..=0x1f`,
//! space and `%` are percent-encoded with uppercase hex; every other byte is
//! written verbatim.
//!
//! # Examples
//!
//! ```
//! use sdkwork_webserver_upstream_protocol::memcached::{parse_get_response, MemcachedGetResponse};
//!
//! let response = b"VALUE key 0 5\r\nhello\r\nEND\r\n";
//! let parsed = parse_get_response(response, &Default::default()).expect("parse");
//! match parsed {
//!     MemcachedGetResponse::Hit(value) => assert_eq!(value.data, b"hello"),
//!     MemcachedGetResponse::Miss => panic!("expected a value"),
//! }
//! ```

use crate::{
    error::{Error, Result},
    limits::Limits,
};

/// Protocol name used in error messages.
const PROTOCOL: &str = "memcached";
/// Maximum length of one protocol line; the memcached response lines are small
/// but a hostile upstream must not grow the line buffer.
const MAXIMUM_LINE_BYTES: usize = 8 * 1024;

/// Escape one memcached key exactly like nginx (`NGX_ESCAPE_MEMCACHED`).
#[must_use]
pub fn escape_key(key: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut escaped = Vec::with_capacity(key.len());
    for byte in key.bytes() {
        if byte <= b' ' || byte == b'%' {
            escaped.push(b'%');
            escaped.push(HEX[usize::from(byte >> 4)]);
            escaped.push(HEX[usize::from(byte & 0x0f)]);
        } else {
            // Only ASCII bytes are rewritten, so a multi-byte UTF-8 key stays
            // valid UTF-8 and the lossy conversion below can never replace it.
            escaped.push(byte);
        }
    }
    String::from_utf8_lossy(&escaped).into_owned()
}

fn validate_key(key: &str, limits: &Limits) -> Result<String> {
    if key.is_empty() {
        return Err(Error::protocol(PROTOCOL, "memcached key must not be empty"));
    }
    if key.len() > limits.max_memcached_key_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "memcached key",
            key.len(),
            limits.max_memcached_key_bytes,
        ));
    }
    let escaped = escape_key(key);
    if escaped.len() > limits.max_memcached_key_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "escaped memcached key",
            escaped.len(),
            limits.max_memcached_key_bytes,
        ));
    }
    Ok(escaped)
}

/// Encode a `get <key>\r\n` request.
///
/// # Errors
///
/// Returns [`Error::Protocol`] for an empty key and [`Error::LimitExceeded`]
/// when the key (before or after escaping) exceeds the configured bound.
pub fn encode_get(key: &str, limits: &Limits) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    let escaped = validate_key(key, &limits)?;
    let mut wire = Vec::with_capacity(escaped.len() + 7);
    wire.extend_from_slice(b"get ");
    wire.extend_from_slice(escaped.as_bytes());
    wire.extend_from_slice(b"\r\n");
    Ok(wire)
}

/// Encode a `set <key> <flags> <expiry> <bytes>\r\n<value>\r\n` request.
///
/// # Errors
///
/// Returns [`Error::Protocol`] for an empty key or an empty value, and
/// [`Error::LimitExceeded`] when the key or value exceeds the configured bound.
pub fn encode_set(
    key: &str,
    value: &[u8],
    flags: u32,
    expiry_seconds: u32,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    if value.is_empty() {
        return Err(Error::protocol(
            PROTOCOL,
            "memcached value must not be empty",
        ));
    }
    if value.len() > limits.max_memcached_value_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "memcached value",
            value.len(),
            limits.max_memcached_value_bytes,
        ));
    }
    let escaped = validate_key(key, &limits)?;
    let header = format!("set {escaped} {flags} {expiry_seconds} {}\r\n", value.len());
    let mut wire = Vec::with_capacity(header.len() + value.len() + 2);
    wire.extend_from_slice(header.as_bytes());
    wire.extend_from_slice(value);
    wire.extend_from_slice(b"\r\n");
    Ok(wire)
}

/// One value returned by a memcached `get`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemcachedValue {
    /// Key echoed by the server; the caller should compare it with the
    /// requested key (nginx rejects a mismatch as an invalid upstream header).
    pub key: String,
    /// Opaque server flags (nginx only interprets them when configured to).
    pub flags: u32,
    /// Raw value bytes.
    pub data: Vec<u8>,
}

impl MemcachedValue {
    /// Whether the server echoed `key` (case-sensitive, as memcached does).
    #[must_use]
    pub fn matches_key(&self, key: &str) -> bool {
        self.key == key
    }
}

/// Result of a parsed `get` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemcachedGetResponse {
    /// The key was found.
    Hit(MemcachedValue),
    /// The server answered `END` without a value.
    Miss,
}

/// One parsed memcached response event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemcachedResponseEvent {
    /// A `VALUE` payload.
    Value(MemcachedValue),
    /// `END` without a value (get miss).
    Miss,
    /// A store command succeeded (`STORED`).
    Stored,
    /// A store command did not store (`NOT_STORED`/`EXISTS`/`NOT_FOUND`).
    NotStored,
}

#[derive(Debug, Clone)]
enum State {
    Line,
    Value {
        remaining: usize,
        key: String,
        flags: u32,
    },
    Done,
}

/// Incremental memcached text-protocol response parser.
///
/// Supports the `get` reply (`VALUE … <data>\r\n … END\r\n`, including multiple
/// `VALUE` blocks for a multi-key request) and the `set` replies
/// (`STORED`/`NOT_STORED`/`EXISTS`/`NOT_FOUND`). Any other line is rejected as
/// an upstream protocol error, matching nginx's "invalid response" handling.
#[derive(Debug, Clone)]
pub struct MemcachedResponseParser {
    limits: Limits,
    buffer: Vec<u8>,
    state: State,
    saw_value: bool,
}

impl MemcachedResponseParser {
    /// Create a parser enforcing `limits`.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            limits: limits.sanitized(),
            buffer: Vec::new(),
            state: State::Line,
            saw_value: false,
        }
    }

    /// Feed raw upstream bytes, returning every completed event.
    ///
    /// # Errors
    ///
    /// - [`Error::Protocol`] for an unknown response line, a malformed
    ///   `VALUE` line, data after the final `END`, or a store reply in a get
    ///   transaction.
    /// - [`Error::LimitExceeded`] for an oversized line, value, or header.
    /// - [`Error::Truncated`] is *not* returned here; incomplete input is
    ///   buffered and reported by [`Self::finish`].
    pub fn feed(&mut self, input: &[u8]) -> Result<Vec<MemcachedResponseEvent>> {
        if matches!(self.state, State::Done) {
            if !input.is_empty() {
                return Err(Error::protocol(
                    PROTOCOL,
                    "bytes received after the memcached response terminator",
                ));
            }
            return Ok(Vec::new());
        }
        self.buffer.extend_from_slice(input);
        let mut events = Vec::new();
        loop {
            // The state carries the pending VALUE metadata; cloning keeps the
            // `match` arms free to mutate `self.buffer` and `self.state`.
            let state = self.state.clone();
            match state {
                State::Line => {
                    let Some(end) = find_crlf(&self.buffer) else {
                        if self.buffer.len() > MAXIMUM_LINE_BYTES {
                            return Err(Error::limit(
                                PROTOCOL,
                                "memcached response line",
                                self.buffer.len(),
                                MAXIMUM_LINE_BYTES,
                            ));
                        }
                        break;
                    };
                    let line = self.buffer[..end].to_vec();
                    self.buffer.drain(..end + 2);
                    let line = std::str::from_utf8(&line).map_err(|_| Error::InvalidUtf8 {
                        protocol: PROTOCOL,
                        field: "response line",
                    })?;
                    self.handle_line(line, &mut events)?;
                }
                State::Value {
                    remaining,
                    key,
                    flags,
                } => {
                    if self.buffer.len() < remaining + 2 {
                        if self.buffer.len() > self.limits.max_memcached_value_bytes + 2 {
                            return Err(Error::limit(
                                PROTOCOL,
                                "memcached value",
                                self.buffer.len(),
                                self.limits.max_memcached_value_bytes,
                            ));
                        }
                        break;
                    }
                    let data = self.buffer[..remaining].to_vec();
                    if &self.buffer[remaining..remaining + 2] != b"\r\n" {
                        return Err(Error::framing(
                            PROTOCOL,
                            "value data is not terminated by CRLF",
                        ));
                    }
                    self.buffer.drain(..remaining + 2);
                    self.saw_value = true;
                    self.state = State::Line;
                    events.push(MemcachedResponseEvent::Value(MemcachedValue {
                        key,
                        flags,
                        data,
                    }));
                }
                State::Done => break,
            }
        }
        Ok(events)
    }

    fn handle_line(&mut self, line: &str, events: &mut Vec<MemcachedResponseEvent>) -> Result<()> {
        if let Some(rest) = line.strip_prefix("VALUE ") {
            let mut parts = rest.split(' ');
            let (Some(key), Some(flags), Some(bytes), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(Error::protocol(PROTOCOL, "malformed VALUE response line"));
            };
            if key.is_empty() {
                return Err(Error::protocol(
                    PROTOCOL,
                    "VALUE response line has an empty key",
                ));
            }
            let flags = flags
                .parse::<u32>()
                .map_err(|_| Error::protocol(PROTOCOL, "VALUE response line has invalid flags"))?;
            let length = bytes.parse::<usize>().map_err(|_| {
                Error::protocol(PROTOCOL, "VALUE response line has an invalid byte length")
            })?;
            if length > self.limits.max_memcached_value_bytes {
                return Err(Error::limit(
                    PROTOCOL,
                    "memcached value",
                    length,
                    self.limits.max_memcached_value_bytes,
                ));
            }
            self.state = State::Value {
                remaining: length,
                key: key.to_owned(),
                flags,
            };
            return Ok(());
        }
        match line {
            "END" => {
                if self.saw_value {
                    self.state = State::Done;
                } else {
                    self.state = State::Done;
                    events.push(MemcachedResponseEvent::Miss);
                }
                Ok(())
            }
            "STORED" => {
                self.state = State::Done;
                events.push(MemcachedResponseEvent::Stored);
                Ok(())
            }
            "NOT_STORED" | "EXISTS" | "NOT_FOUND" | "DELETED" | "TOUCHED" => {
                self.state = State::Done;
                events.push(MemcachedResponseEvent::NotStored);
                Ok(())
            }
            "ERROR" | "CLIENT_ERROR" | "SERVER_ERROR" => Err(Error::protocol(
                PROTOCOL,
                "memcached server returned an error reply",
            )),
            _ => Err(Error::protocol(
                PROTOCOL,
                "unrecognised memcached response line",
            )),
        }
    }

    /// Whether a terminal response line has been parsed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        matches!(self.state, State::Done)
    }

    /// Validate that the response is complete and no partial frame remains.
    ///
    /// # Errors
    ///
    /// [`Error::Protocol`] when the server has not sent its terminating line,
    /// and [`Error::Truncated`] when buffered bytes form a partial value.
    pub fn finish(&self) -> Result<()> {
        if self.is_complete() {
            if !self.buffer.is_empty() {
                return Err(Error::protocol(
                    PROTOCOL,
                    "trailing bytes after the memcached response terminator",
                ));
            }
            return Ok(());
        }
        if self.buffer.is_empty() {
            return Err(Error::protocol(
                PROTOCOL,
                "memcached response ended before its terminating line",
            ));
        }
        Err(Error::Truncated {
            protocol: PROTOCOL,
            field: "memcached response",
            expected: 2,
        })
    }
}

fn find_crlf(buffer: &[u8]) -> Option<usize> {
    buffer.windows(2).position(|window| window == b"\r\n")
}

/// Parse one complete `get` response buffer into a hit or a miss.
///
/// # Errors
///
/// Returns the first parser error and fails when trailing bytes remain after
/// the terminating line.
pub fn parse_get_response(bytes: &[u8], limits: &Limits) -> Result<MemcachedGetResponse> {
    let mut parser = MemcachedResponseParser::new(*limits);
    let events = parser.feed(bytes)?;
    parser.finish()?;
    let mut value = None;
    let mut miss = false;
    for event in events {
        match event {
            MemcachedResponseEvent::Value(found) => value = Some(found),
            MemcachedResponseEvent::Miss => miss = true,
            MemcachedResponseEvent::Stored | MemcachedResponseEvent::NotStored => {
                return Err(Error::protocol(
                    PROTOCOL,
                    "store reply received for a get request",
                ));
            }
        }
    }
    match (value, miss) {
        (Some(value), _) => Ok(MemcachedGetResponse::Hit(value)),
        (None, true) => Ok(MemcachedGetResponse::Miss),
        (None, false) => Err(Error::protocol(PROTOCOL, "empty memcached get response")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Result<Vec<MemcachedResponseEvent>> {
        let mut parser = MemcachedResponseParser::new(Limits::default());
        let events = parser.feed(bytes)?;
        parser.finish()?;
        Ok(events)
    }

    #[test]
    fn escape_matches_nginx_memcached_rules() {
        assert_eq!(escape_key("plain-key_1.2~"), "plain-key_1.2~");
        assert_eq!(escape_key("a b"), "a%20b");
        assert_eq!(escape_key("100%"), "100%25");
        assert_eq!(escape_key("a\nb"), "a%0Ab");
        assert_eq!(escape_key("a\x1fb"), "a%1Fb");
        // Del and high bytes are not escaped by nginx's memcached table.
        assert_eq!(escape_key("a\u{7f}b"), "a\u{7f}b");
    }

    #[test]
    fn get_and_set_requests_are_encoded() {
        assert_eq!(
            encode_get("user:1 key", &Limits::default()).expect("get"),
            b"get user:1%20key\r\n".to_vec()
        );
        assert_eq!(
            encode_set("k", b"hello", 7, 60, &Limits::default()).expect("set"),
            b"set k 7 60 5\r\nhello\r\n".to_vec()
        );
    }

    #[test]
    fn request_bounds_fail_closed() {
        assert!(encode_get("", &Limits::default()).is_err());
        let limits = Limits {
            max_memcached_key_bytes: 4,
            max_memcached_value_bytes: 4,
            ..Limits::default()
        };
        assert!(matches!(
            encode_get("toolong", &limits),
            Err(Error::LimitExceeded { .. })
        ));
        assert!(matches!(
            encode_set("k", b"hello", 0, 0, &limits),
            Err(Error::LimitExceeded { .. })
        ));
        assert!(encode_set("k", b"", 0, 0, &Limits::default()).is_err());
    }

    #[test]
    fn value_response_is_parsed_with_binary_safe_data() {
        let events = parse(b"VALUE key 7 5\r\nhe\0lo\r\nEND\r\n").expect("parse");
        assert_eq!(events.len(), 1);
        match &events[0] {
            MemcachedResponseEvent::Value(value) => {
                assert_eq!(value.key, "key");
                assert_eq!(value.flags, 7);
                assert_eq!(value.data, b"he\0lo");
                assert!(value.matches_key("key"));
                assert!(!value.matches_key("other"));
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn miss_is_reported() {
        assert_eq!(
            parse(b"END\r\n").expect("parse"),
            vec![MemcachedResponseEvent::Miss]
        );
    }

    #[test]
    fn store_replies_are_reported() {
        assert_eq!(
            parse(b"STORED\r\n").expect("parse"),
            vec![MemcachedResponseEvent::Stored]
        );
        for line in [&b"NOT_STORED\r\n"[..], b"EXISTS\r\n", b"NOT_FOUND\r\n"] {
            assert_eq!(
                parse(line).expect("parse"),
                vec![MemcachedResponseEvent::NotStored]
            );
        }
    }

    #[test]
    fn multiple_values_are_streamed() {
        let wire = b"VALUE a 0 1\r\nx\r\nVALUE b 0 2\r\nyy\r\nEND\r\n";
        let events = parse(wire).expect("parse");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], MemcachedResponseEvent::Value(value) if value.key == "a"));
        assert!(matches!(&events[1], MemcachedResponseEvent::Value(value) if value.key == "b"));
    }

    #[test]
    fn byte_at_a_time_feeding_works() {
        let wire = b"VALUE key 0 5\r\nhello\r\nEND\r\n";
        let mut parser = MemcachedResponseParser::new(Limits::default());
        let mut events = Vec::new();
        for byte in wire {
            events.extend(parser.feed(std::slice::from_ref(byte)).expect("feed"));
        }
        parser.finish().expect("complete");
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], MemcachedResponseEvent::Value(value) if value.data == b"hello")
        );
    }

    #[test]
    fn malformed_responses_fail_closed() {
        for wire in [
            &b"BOGUS\r\n"[..],
            b"ERROR\r\n",
            b"SERVER_ERROR out of memory\r\n",
            b"VALUE key 0\r\n",
            b"VALUE key x 1\r\n",
            b"VALUE key 0 1\r\nxEND\r\n",
        ] {
            assert!(parse(wire).is_err(), "wire {wire:?} must fail closed");
        }
        assert!(parse(b"VALUE key 0 2\r\nab\r\nEND\r\nEXTRA").is_err());
        assert!(parse(b"VALUE key 0 2\r\nab\r\n").is_err());
        assert!(parse(b"VALUE key 0 999999999\r\n").is_err());
    }

    #[test]
    fn get_response_helper_returns_hit_or_miss() {
        match parse_get_response(b"VALUE k 0 3\r\nabc\r\nEND\r\n", &Limits::default()).expect("hit")
        {
            MemcachedGetResponse::Hit(value) => assert_eq!(value.data, b"abc"),
            MemcachedGetResponse::Miss => panic!("expected hit"),
        }
        assert_eq!(
            parse_get_response(b"END\r\n", &Limits::default()).expect("miss"),
            MemcachedGetResponse::Miss
        );
        assert!(parse_get_response(b"STORED\r\n", &Limits::default()).is_err());
    }

    #[test]
    fn value_bounds_are_enforced_before_buffering_the_value() {
        let limits = Limits {
            max_memcached_value_bytes: 4,
            ..Limits::default()
        };
        assert!(matches!(
            parse_get_response(b"VALUE k 0 5\r\nhello\r\nEND\r\n", &limits),
            Err(Error::LimitExceeded { .. })
        ));
        let mut parser = MemcachedResponseParser::new(limits);
        // Declared value length is checked before the data arrives.
        assert!(parser.feed(b"VALUE k 0 5\r\nhe").is_err());
    }
}
