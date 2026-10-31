//! gRPC-over-HTTP/2 adaptation helpers (`grpc_pass`).
//!
//! The crate does not own an HTTP/2 client; these are the pure parts of a gRPC
//! call that the edge's HTTP/2 upstream model needs:
//!
//! - the request header plan (`:method POST`, `:scheme`, `:authority`,
//!   `:path`, `content-type: application/grpc`, `te: trailers`, an optional
//!   `grpc-timeout`, plus caller metadata),
//! - the length-prefixed gRPC message framing (`1` flag byte + 4-byte
//!   big-endian length + payload, repeatable for streaming calls),
//! - trailer parsing: `grpc-status` and `grpc-message` (percent-encoded UTF-8),
//!   with the remaining trailers preserved as metadata.
//!
//! Compression is signalled, not implemented: a frame's compressed flag is
//! preserved so the caller can decompress according to `grpc-encoding`. Trailer
//! → HTTP response mapping (for example `grpc-status` to an edge 502) is the
//! caller's policy and deliberately not encoded here.
//!
//! # Examples
//!
//! ```
//! use sdkwork_webserver_upstream_protocol::grpc::{encode_message, GrpcMessageDecoder};
//!
//! let frame = encode_message(b"hello", &Default::default()).expect("frame");
//! let mut decoder = GrpcMessageDecoder::new(Default::default());
//! let messages = decoder.feed(&frame).expect("decode");
//! assert_eq!(messages[0].payload, b"hello");
//! assert!(!messages[0].compressed);
//! decoder.finish().expect("complete");
//! ```

use crate::{
    error::{Error, Result},
    header_block::is_token,
    limits::Limits,
};

/// Protocol name used in error messages.
const PROTOCOL: &str = "grpc";
/// gRPC request content type.
pub const GRPC_CONTENT_TYPE: &str = "application/grpc";
/// gRPC trailer carrying the final status code.
pub const GRPC_STATUS_HEADER: &str = "grpc-status";
/// gRPC trailer carrying the percent-encoded status message.
pub const GRPC_MESSAGE_HEADER: &str = "grpc-message";
/// gRPC request header declaring trailer support.
pub const TE_HEADER_VALUE: &str = "trailers";
/// gRPC message frame prefix size.
pub const MESSAGE_PREFIX_BYTES: usize = 5;
/// Largest delay expressible in the `grpc-timeout` header (eight digits).
const MAXIMUM_TIMEOUT_MILLIS: u64 = 99_999_999;

/// Metadata fields a caller may not set: the pseudo headers and the gRPC
/// transport headers this module owns.
const RESERVED_METADATA: &[&str] = &[
    ":method",
    ":scheme",
    ":authority",
    ":path",
    "content-type",
    "te",
    "grpc-timeout",
    "grpc-status",
    "grpc-message",
    "grpc-status-details-bin",
];

/// A bounded `grpc-timeout` header value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrpcTimeout {
    millis: u64,
}

impl GrpcTimeout {
    /// Build a timeout from milliseconds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LimitExceeded`] when the value needs more than eight
    /// decimal digits (the gRPC timeout syntax limit).
    pub fn from_millis(millis: u64) -> Result<Self> {
        if millis == 0 {
            return Err(Error::protocol(
                PROTOCOL,
                "grpc-timeout must be greater than zero",
            ));
        }
        if millis > MAXIMUM_TIMEOUT_MILLIS {
            return Err(Error::limit(
                PROTOCOL,
                "grpc-timeout milliseconds",
                usize::try_from(millis).unwrap_or(usize::MAX),
                MAXIMUM_TIMEOUT_MILLIS as usize,
            ));
        }
        Ok(Self { millis })
    }

    /// Build a timeout from whole seconds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LimitExceeded`] when the result is zero or needs more
    /// than eight decimal digits.
    pub fn from_seconds(seconds: u64) -> Result<Self> {
        let millis = seconds.checked_mul(1000).ok_or_else(|| {
            Error::limit(
                PROTOCOL,
                "grpc-timeout milliseconds",
                usize::MAX,
                MAXIMUM_TIMEOUT_MILLIS as usize,
            )
        })?;
        Self::from_millis(millis)
    }

    /// The header value, in milliseconds (`<n>m`).
    #[must_use]
    pub fn header_value(&self) -> String {
        format!("{}m", self.millis)
    }
}

/// Build the gRPC request header list.
///
/// The order is fixed (`:method`, `:scheme`, `:authority`, `:path`, `te`,
/// `content-type`, optional `grpc-timeout`, then caller metadata in order).
///
/// # Errors
///
/// - [`Error::Protocol`] when `path` is not an absolute `/<service>/<method>`
///   path, `authority` is empty, the scheme is not `http`/`https`, a metadata
///   name is invalid/reserved, or a metadata value contains a control character.
/// - [`Error::LimitExceeded`] when the metadata count or byte size exceeds
///   `limits`.
pub fn grpc_request_headers(
    path: &str,
    authority: &str,
    scheme: &str,
    timeout: Option<GrpcTimeout>,
    metadata: &[(String, String)],
    limits: &Limits,
) -> Result<Vec<(String, String)>> {
    let limits = limits.sanitized();
    if !is_grpc_path(path) {
        return Err(Error::protocol(
            PROTOCOL,
            "grpc path must be an absolute `/<service>/<method>` path",
        ));
    }
    if authority.is_empty()
        || authority
            .bytes()
            .any(|byte| byte == 0 || byte < 0x20 || byte == 0x7f || byte == b' ')
    {
        return Err(Error::protocol(
            PROTOCOL,
            "grpc authority must be a non-empty host[:port] without controls",
        ));
    }
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(Error::protocol(
            PROTOCOL,
            "grpc scheme must be `http` or `https`",
        ));
    }
    if metadata.len() > limits.max_grpc_metadata {
        return Err(Error::limit(
            PROTOCOL,
            "grpc metadata count",
            metadata.len(),
            limits.max_grpc_metadata,
        ));
    }
    let mut headers = Vec::with_capacity(metadata.len() + 7);
    headers.push((":method".to_owned(), "POST".to_owned()));
    headers.push((":scheme".to_owned(), scheme.to_ascii_lowercase()));
    headers.push((":authority".to_owned(), authority.to_owned()));
    headers.push((":path".to_owned(), path.to_owned()));
    headers.push(("te".to_owned(), TE_HEADER_VALUE.to_owned()));
    headers.push(("content-type".to_owned(), GRPC_CONTENT_TYPE.to_owned()));
    if let Some(timeout) = timeout {
        headers.push(("grpc-timeout".to_owned(), timeout.header_value()));
    }
    let mut bytes = 0usize;
    for (name, value) in metadata {
        validate_metadata(name, value, &limits)?;
        bytes = bytes
            .checked_add(name.len())
            .and_then(|total| total.checked_add(value.len()))
            .ok_or_else(|| {
                Error::limit(
                    PROTOCOL,
                    "grpc metadata bytes",
                    usize::MAX,
                    limits.max_header_block_bytes,
                )
            })?;
        if bytes > limits.max_header_block_bytes {
            return Err(Error::limit(
                PROTOCOL,
                "grpc metadata bytes",
                bytes,
                limits.max_header_block_bytes,
            ));
        }
        headers.push((name.clone(), value.clone()));
    }
    Ok(headers)
}

fn is_grpc_path(path: &str) -> bool {
    if !path.starts_with('/') || path.len() == 1 {
        return false;
    }
    let inner = &path[1..];
    let mut segments = inner.split('/');
    let (Some(service), Some(method), None) = (segments.next(), segments.next(), segments.next())
    else {
        return false;
    };
    !service.is_empty()
        && !method.is_empty()
        && service.bytes().all(is_grpc_path_byte)
        && method.bytes().all(is_grpc_path_byte)
}

fn is_grpc_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~' | b'+')
}

fn validate_metadata(name: &str, value: &str, limits: &Limits) -> Result<()> {
    if !is_token(name)
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
    {
        return Err(Error::protocol(
            PROTOCOL,
            "grpc metadata name must be lowercase ASCII (alphanumerics, `-`, `_`, `.`)",
        ));
    }
    if name.starts_with("grpc-") {
        return Err(Error::protocol(
            PROTOCOL,
            "grpc metadata must not use the reserved `grpc-` prefix",
        ));
    }
    if RESERVED_METADATA.contains(&name) {
        return Err(Error::protocol(
            PROTOCOL,
            "grpc metadata must not override a transport header this module owns",
        ));
    }
    if value.len() > limits.max_header_line_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "grpc metadata value",
            value.len(),
            limits.max_header_line_bytes,
        ));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n' || byte == 0x7f)
    {
        return Err(Error::protocol(
            PROTOCOL,
            "grpc metadata value contains a control character",
        ));
    }
    Ok(())
}

/// Encode one uncompressed gRPC message frame.
///
/// # Errors
///
/// Returns [`Error::LimitExceeded`] when the payload exceeds
/// `limits.max_grpc_message_bytes`.
pub fn encode_message(payload: &[u8], limits: &Limits) -> Result<Vec<u8>> {
    encode_message_with_flag(payload, false, limits)
}

/// Encode one gRPC message frame with an explicit compressed flag.
///
/// A `compressed` frame's payload is already compressed by the caller's
/// `grpc-encoding`; this crate only frames it.
///
/// # Errors
///
/// Returns [`Error::LimitExceeded`] when the payload exceeds
/// `limits.max_grpc_message_bytes`.
pub fn encode_message_with_flag(
    payload: &[u8],
    compressed: bool,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    if payload.len() > limits.max_grpc_message_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "grpc message",
            payload.len(),
            limits.max_grpc_message_bytes,
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| Error::limit(PROTOCOL, "grpc message", payload.len(), u32::MAX as usize))?;
    let mut frame = Vec::with_capacity(MESSAGE_PREFIX_BYTES + payload.len());
    frame.push(u8::from(compressed));
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// One decoded gRPC message frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcFrame {
    /// Whether the payload is compressed with the declared `grpc-encoding`.
    pub compressed: bool,
    /// Raw message payload bytes.
    pub payload: Vec<u8>,
}

/// Incremental gRPC message decoder for a streamed request/response body.
///
/// The decoder enforces the message-size bound from the 5-byte prefix before
/// buffering the payload, so an oversized or truncated frame fails closed.
#[derive(Debug, Clone)]
pub struct GrpcMessageDecoder {
    limits: Limits,
    buffer: Vec<u8>,
}

impl GrpcMessageDecoder {
    /// Create a decoder enforcing `limits`.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            limits: limits.sanitized(),
            buffer: Vec::new(),
        }
    }

    /// Feed raw gRPC body bytes, returning every complete message frame.
    ///
    /// # Errors
    ///
    /// - [`Error::Protocol`] for a compressed flag other than `0`/`1`.
    /// - [`Error::LimitExceeded`] when the declared message length exceeds
    ///   `limits.max_grpc_message_bytes` or the buffer bound is exceeded.
    pub fn feed(&mut self, input: &[u8]) -> Result<Vec<GrpcFrame>> {
        let maximum_buffer =
            MESSAGE_PREFIX_BYTES.saturating_add(self.limits.max_grpc_message_bytes);
        if self.buffer.len().saturating_add(input.len()) > maximum_buffer {
            return Err(Error::limit(
                PROTOCOL,
                "buffered grpc message",
                self.buffer.len().saturating_add(input.len()),
                maximum_buffer,
            ));
        }
        self.buffer.extend_from_slice(input);
        let mut frames = Vec::new();
        loop {
            if self.buffer.len() < MESSAGE_PREFIX_BYTES {
                break;
            }
            let prefix = self
                .buffer
                .get(..MESSAGE_PREFIX_BYTES)
                .ok_or_else(|| Error::framing(PROTOCOL, "message prefix disappeared"))?;
            let compressed = match prefix[0] {
                0 => false,
                1 => true,
                other => {
                    return Err(Error::protocol(
                        PROTOCOL,
                        format!("unsupported grpc compressed flag {other}"),
                    ));
                }
            };
            let length = u32::from_be_bytes([prefix[1], prefix[2], prefix[3], prefix[4]]);
            let length = usize::try_from(length).map_err(|_| {
                Error::limit(PROTOCOL, "grpc message", u32::MAX as usize, usize::MAX)
            })?;
            if length > self.limits.max_grpc_message_bytes {
                return Err(Error::limit(
                    PROTOCOL,
                    "grpc message",
                    length,
                    self.limits.max_grpc_message_bytes,
                ));
            }
            let total = MESSAGE_PREFIX_BYTES + length;
            if self.buffer.len() < total {
                break;
            }
            let payload = self
                .buffer
                .get(MESSAGE_PREFIX_BYTES..total)
                .ok_or_else(|| Error::framing(PROTOCOL, "message payload disappeared"))?
                .to_vec();
            self.buffer.drain(..total);
            frames.push(GrpcFrame {
                compressed,
                payload,
            });
        }
        Ok(frames)
    }

    /// Validate that the stream ended on a message boundary.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`] when buffered bytes form a partial frame.
    pub fn finish(&self) -> Result<()> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(Error::Truncated {
                protocol: PROTOCOL,
                field: "grpc message",
                expected: MESSAGE_PREFIX_BYTES,
            })
        }
    }
}

/// Parsed gRPC trailers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcTrailer {
    /// Final gRPC status code (`0` = OK).
    pub status: u32,
    /// Decoded `grpc-message`, when the server sent one.
    pub message: Option<String>,
    /// Remaining trailer fields in wire order (for example
    /// `grpc-status-details-bin`).
    pub metadata: Vec<(String, String)>,
}

impl GrpcTrailer {
    /// Whether the call succeeded (`grpc-status: 0`).
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.status == 0
    }
}

/// Parse gRPC trailers from a header list.
///
/// `headers` is the trailing header list (or, for a trailers-only response, the
/// response header list). `grpc-status` is mandatory per the gRPC protocol;
/// `grpc-message` is percent-decoded; every other field is preserved as
/// metadata.
///
/// # Errors
///
/// - [`Error::Protocol`] when `grpc-status` is missing, duplicated, not a
///   decimal `u32`, or when `grpc-message` is duplicated or contains an invalid
///   percent escape.
/// - [`Error::InvalidUtf8`] when the decoded `grpc-message` is not UTF-8.
/// - [`Error::LimitExceeded`] when a header value exceeds the line bound.
pub fn parse_grpc_trailers(headers: &[(String, String)], limits: &Limits) -> Result<GrpcTrailer> {
    let limits = limits.sanitized();
    let mut status: Option<u32> = None;
    let mut message: Option<String> = None;
    let mut metadata = Vec::new();
    for (name, value) in headers {
        if value.len() > limits.max_header_line_bytes {
            return Err(Error::limit(
                PROTOCOL,
                "grpc trailer value",
                value.len(),
                limits.max_header_line_bytes,
            ));
        }
        if name.eq_ignore_ascii_case(GRPC_STATUS_HEADER) {
            if status.is_some() {
                return Err(Error::protocol(
                    PROTOCOL,
                    "grpc-status trailer is duplicated",
                ));
            }
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(Error::protocol(
                    PROTOCOL,
                    "grpc-status trailer is not a decimal status code",
                ));
            }
            status =
                Some(value.parse::<u32>().map_err(|_| {
                    Error::protocol(PROTOCOL, "grpc-status trailer is out of range")
                })?);
        } else if name.eq_ignore_ascii_case(GRPC_MESSAGE_HEADER) {
            if message.is_some() {
                return Err(Error::protocol(
                    PROTOCOL,
                    "grpc-message trailer is duplicated",
                ));
            }
            message = Some(percent_decode_grpc_message(value)?);
        } else {
            metadata.push((name.clone(), value.clone()));
        }
    }
    let Some(status) = status else {
        return Err(Error::protocol(PROTOCOL, "grpc-status trailer is missing"));
    };
    Ok(GrpcTrailer {
        status,
        message,
        metadata,
    })
}

/// Percent-decode a `grpc-message` value into UTF-8.
///
/// gRPC percent-encodes bytes outside the printable-ASCII range (and `%`);
/// decoding is strict so a malformed escape fails closed instead of reaching a
/// client as a corrupted diagnostic.
///
/// # Errors
///
/// Returns [`Error::Protocol`] for an incomplete or non-hex escape and
/// [`Error::InvalidUtf8`] when the decoded bytes are not UTF-8.
pub fn percent_decode_grpc_message(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let (Some(high), Some(low)) = (bytes.get(index + 1), bytes.get(index + 2)) else {
                return Err(Error::protocol(
                    PROTOCOL,
                    "grpc-message has an incomplete percent escape",
                ));
            };
            let hex = |byte: u8| -> Option<u8> {
                match byte {
                    b'0'..=b'9' => Some(byte - b'0'),
                    b'a'..=b'f' => Some(byte - b'a' + 10),
                    b'A'..=b'F' => Some(byte - b'A' + 10),
                    _ => None,
                }
            };
            let (Some(high), Some(low)) = (hex(*high), hex(*low)) else {
                return Err(Error::protocol(
                    PROTOCOL,
                    "grpc-message has a non-hex percent escape",
                ));
            };
            decoded.push((high << 4) | low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(decoded).map_err(|_| Error::InvalidUtf8 {
        protocol: PROTOCOL,
        field: "grpc-message",
    })
}

/// Human-readable name of a gRPC status code.
///
/// Unknown codes (extensions) are reported as `UNKNOWN`; the numeric value
/// remains authoritative in [`GrpcTrailer::status`].
#[must_use]
pub fn grpc_status_name(status: u32) -> &'static str {
    match status {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "UNKNOWN",
    }
}

/// Whether a gRPC status code reports success.
#[must_use]
pub fn grpc_status_is_ok(status: u32) -> bool {
    status == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn request_headers_have_the_grpc_transport_fields() {
        let headers = grpc_request_headers(
            "/helloworld.Greeter/SayHello",
            "api.example.com",
            "https",
            Some(GrpcTimeout::from_seconds(2).expect("timeout")),
            &metadata(&[("x-request-id", "abc")]),
            &Limits::default(),
        )
        .expect("headers");
        let find = |name: &str| {
            headers
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(find(":method"), Some("POST"));
        assert_eq!(find(":path"), Some("/helloworld.Greeter/SayHello"));
        assert_eq!(find(":authority"), Some("api.example.com"));
        assert_eq!(find(":scheme"), Some("https"));
        assert_eq!(find("content-type"), Some(GRPC_CONTENT_TYPE));
        assert_eq!(find("te"), Some("trailers"));
        assert_eq!(find("grpc-timeout"), Some("2000m"));
        assert_eq!(find("x-request-id"), Some("abc"));
    }

    #[test]
    fn timeout_bounds_are_enforced() {
        assert!(GrpcTimeout::from_millis(0).is_err());
        assert!(GrpcTimeout::from_millis(MAXIMUM_TIMEOUT_MILLIS).is_ok());
        assert!(GrpcTimeout::from_millis(MAXIMUM_TIMEOUT_MILLIS + 1).is_err());
        assert!(GrpcTimeout::from_seconds(u64::MAX).is_err());
        assert_eq!(
            GrpcTimeout::from_millis(5).expect("timeout").header_value(),
            "5m"
        );
    }

    #[test]
    fn invalid_request_targets_and_metadata_fail_closed() {
        let limits = Limits::default();
        for path in ["", "/", "/service", "/service/", "service/method", "/a/b/c"] {
            assert!(
                grpc_request_headers(path, "host", "https", None, &[], &limits).is_err(),
                "path {path:?} must fail closed"
            );
        }
        assert!(grpc_request_headers("/a/b", "", "https", None, &[], &limits).is_err());
        assert!(grpc_request_headers("/a/b", "host", "ftp", None, &[], &limits).is_err());
        for (name, value) in [
            ("Content-Type", "x"),
            ("grpc-foo", "x"),
            ("content-type", "x"),
            ("x-bad", "a\r\nb"),
        ] {
            assert!(
                grpc_request_headers(
                    "/a/b",
                    "host",
                    "https",
                    None,
                    &metadata(&[(name, value)]),
                    &limits
                )
                .is_err(),
                "metadata {name:?} must fail closed"
            );
        }
    }

    #[test]
    fn metadata_bounds_are_enforced() {
        let limits = Limits {
            max_grpc_metadata: 1,
            ..Limits::default()
        };
        assert!(matches!(
            grpc_request_headers(
                "/a/b",
                "host",
                "https",
                None,
                &metadata(&[("a", "1"), ("b", "2")]),
                &limits
            ),
            Err(Error::LimitExceeded { .. })
        ));
        let limits = Limits {
            max_header_line_bytes: 2,
            ..Limits::default()
        };
        assert!(matches!(
            grpc_request_headers(
                "/a/b",
                "host",
                "https",
                None,
                &metadata(&[("a", "123")]),
                &limits
            ),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn message_frames_round_trip_and_stream() {
        let limits = Limits::default();
        let first = encode_message(b"one", &limits).expect("first");
        let second = encode_message_with_flag(b"two", true, &limits).expect("second");
        let mut decoder = GrpcMessageDecoder::new(limits);
        let mut frames = Vec::new();
        // Split in the middle of a prefix and in the middle of a payload.
        for byte in first.iter().chain(second.iter()) {
            frames.extend(decoder.feed(std::slice::from_ref(byte)).expect("feed"));
        }
        decoder.finish().expect("complete");
        assert_eq!(frames[0].payload, b"one");
        assert!(!frames[0].compressed);
        assert_eq!(frames[1].payload, b"two");
        assert!(frames[1].compressed);
    }

    #[test]
    fn message_prefix_is_big_endian_with_flag() {
        let frame = encode_message(b"hello", &Limits::default()).expect("frame");
        assert_eq!(&frame[..5], &[0, 0, 0, 0, 5]);
        let compressed = encode_message_with_flag(b"x", true, &Limits::default()).expect("frame");
        assert_eq!(&compressed[..5], &[1, 0, 0, 0, 1]);
    }

    #[test]
    fn oversized_and_truncated_frames_fail_closed() {
        let limits = Limits {
            max_grpc_message_bytes: 2,
            ..Limits::default()
        };
        assert!(matches!(
            encode_message(b"abc", &limits),
            Err(Error::LimitExceeded { .. })
        ));
        let mut decoder = GrpcMessageDecoder::new(limits);
        // Declared length above the bound is rejected from the prefix alone.
        assert!(matches!(
            decoder.feed(&[0, 0, 0, 0, 3]),
            Err(Error::LimitExceeded { .. })
        ));
        let mut decoder = GrpcMessageDecoder::new(Limits::default());
        assert!(decoder.feed(&[0, 0, 0]).unwrap().is_empty());
        assert!(matches!(decoder.finish(), Err(Error::Truncated { .. })));
        let mut decoder = GrpcMessageDecoder::new(Limits::default());
        assert!(decoder.feed(&[2, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn trailers_parse_status_and_percent_decoded_message() {
        let trailer = parse_grpc_trailers(
            &metadata(&[
                ("grpc-status", "3"),
                ("grpc-message", "bad%20argument%3A%20x"),
                ("x-detail", "y"),
            ]),
            &Limits::default(),
        )
        .expect("trailers");
        assert_eq!(trailer.status, 3);
        assert_eq!(trailer.message.as_deref(), Some("bad argument: x"));
        assert!(!trailer.is_ok());
        assert_eq!(trailer.metadata, metadata(&[("x-detail", "y")]));
        assert!(grpc_status_is_ok(0));
        assert_eq!(grpc_status_name(4), "DEADLINE_EXCEEDED");
        assert_eq!(grpc_status_name(999), "UNKNOWN");
    }

    #[test]
    fn malformed_trailers_fail_closed() {
        let limits = Limits::default();
        assert!(parse_grpc_trailers(&metadata(&[("x", "y")]), &limits).is_err());
        assert!(parse_grpc_trailers(
            &metadata(&[("grpc-status", "0"), ("grpc-status", "0")]),
            &limits
        )
        .is_err());
        assert!(parse_grpc_trailers(&metadata(&[("grpc-status", "ok")]), &limits).is_err());
        assert!(parse_grpc_trailers(
            &metadata(&[("grpc-status", "0"), ("grpc-message", "a%2")]),
            &limits
        )
        .is_err());
        assert!(parse_grpc_trailers(
            &metadata(&[("grpc-status", "0"), ("grpc-message", "a%zz")]),
            &limits
        )
        .is_err());
        assert!(parse_grpc_trailers(
            &metadata(&[("grpc-status", "0"), ("grpc-message", "%ff")]),
            &limits
        )
        .is_err());
    }

    #[test]
    fn percent_decoding_keeps_plus_literal() {
        assert_eq!(
            percent_decode_grpc_message("a+b%20c").expect("decode"),
            "a+b c"
        );
        assert_eq!(
            percent_decode_grpc_message("h%C3%A9llo").expect("decode"),
            "héllo"
        );
    }
}
