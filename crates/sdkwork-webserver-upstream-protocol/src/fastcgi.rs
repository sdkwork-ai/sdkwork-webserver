//! FastCGI 1.0 request encoder and response parser.
//!
//! Wire format (FastCGI Specification 1.0): every record starts with the
//! 8-byte header
//!
//! ```text
//! version(1) | type(1) | request_id_be(2) | content_length_be(2) | padding(1) | reserved(1)
//! ```
//!
//! A request is a `BEGIN_REQUEST` record (role `RESPONDER`, optional
//! `KEEP_CONN` flag), one or more `PARAMS` records (name/value pairs with the
//! FastCGI 1/4-byte length encoding), an empty `PARAMS` record, `STDIN` records
//! carrying the request body and a final empty `STDIN` record. A response is a
//! sequence of `STDOUT` records (a CGI-style header block followed by the body),
//! `STDERR` records, and one `END_REQUEST` record.
//!
//! Encoders mirror nginx's `ngx_http_fastcgi_create_request`: stdin chunks are
//! 32 KiB, every record body is zero-padded to an 8-byte boundary, and the
//! request id is the FastCGI connection id (nginx uses `1`). The parser is
//! incremental and enforces [`Limits`] before allocating, so an upstream that
//! sends a header bomb or a truncated record is rejected with a typed error.
//!
//! # Examples
//!
//! ```
//! use sdkwork_webserver_upstream_protocol::{
//!     fastcgi::{FastCgiParams, encode_request},
//!     Limits,
//! };
//!
//! let mut params = FastCgiParams::new();
//! params.push("REQUEST_METHOD", "GET").expect("bounded param");
//! params.push("REQUEST_URI", "/").expect("bounded param");
//! let request = encode_request(1, false, &params, b"", &Limits::default())
//!     .expect("request encodes");
//! assert_eq!(request[0], 1); // FastCGI version
//! assert_eq!(request[1], 1); // FCGI_BEGIN_REQUEST
//! ```

use crate::{
    cgi::enforce_bounds,
    error::{Error, Result},
    header_block::{parse_response_head, HeaderStyle, HttpResponseHead},
    limits::Limits,
};

/// Protocol name used in error messages.
const PROTOCOL: &str = "fastcgi";
/// FastCGI protocol version.
pub const FASTCGI_VERSION: u8 = 1;
/// `FCGI_BEGIN_REQUEST` record type.
pub const FCGI_BEGIN_REQUEST: u8 = 1;
/// `FCGI_ABORT_REQUEST` record type.
pub const FCGI_ABORT_REQUEST: u8 = 2;
/// `FCGI_END_REQUEST` record type.
pub const FCGI_END_REQUEST: u8 = 3;
/// `FCGI_PARAMS` record type.
pub const FCGI_PARAMS: u8 = 4;
/// `FCGI_STDIN` record type.
pub const FCGI_STDIN: u8 = 5;
/// `FCGI_STDOUT` record type.
pub const FCGI_STDOUT: u8 = 6;
/// `FCGI_STDERR` record type.
pub const FCGI_STDERR: u8 = 7;
/// `FCGI_RESPONDER` role.
pub const FCGI_RESPONDER: u16 = 1;
/// `FCGI_KEEP_CONN` flag.
pub const FCGI_KEEP_CONN: u8 = 1;
/// Fixed record header size.
pub const RECORD_HEADER_BYTES: usize = 8;
/// Maximum record content size (the header field is 16-bit).
pub const MAX_RECORD_CONTENT_BYTES: usize = u16::MAX as usize;
/// `END_REQUEST` content size (app status + protocol status + padding).
const END_REQUEST_CONTENT_BYTES: usize = 8;

/// Ordered, bounded FastCGI `PARAMS` name/value set.
///
/// The set enforces the [`Limits`] byte/count bounds at insertion time, so a
/// configuration or request cannot build a `PARAMS` block larger than the
/// caller allowed. Order is preserved on the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FastCgiParams {
    entries: Vec<(String, String)>,
}

impl FastCgiParams {
    /// Create an empty parameter set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one parameter.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Protocol`] when the name is not a valid FastCGI
    /// parameter name token or the value contains a NUL byte, and
    /// [`Error::LimitExceeded`] when the set would exceed the default
    /// [`Limits`] bounds. Use [`Self::push_with_limits`] to apply the caller's
    /// own bounds.
    pub fn push(&mut self, name: &str, value: &str) -> Result<()> {
        self.push_with_limits(name, value, &Limits::default().sanitized())
    }

    /// Append one parameter under explicit [`Limits`].
    ///
    /// # Errors
    ///
    /// Same conditions as [`Self::push`], evaluated against `limits`.
    pub fn push_with_limits(&mut self, name: &str, value: &str, limits: &Limits) -> Result<()> {
        validate_param(name, value)?;
        self.entries.push((name.to_owned(), value.to_owned()));
        enforce_bounds(&self.entries, limits)?;
        Ok(())
    }

    /// Build a parameter set from an ordered variable list.
    ///
    /// # Errors
    ///
    /// Returns the first insertion or bound error from
    /// [`Self::push_with_limits`].
    pub fn from_variables(variables: &[(String, String)], limits: &Limits) -> Result<Self> {
        enforce_bounds(variables, limits)?;
        let mut params = Self::new();
        for (name, value) in variables {
            validate_param(name, value)?;
        }
        params.entries = variables.to_vec();
        Ok(params)
    }

    /// The parameter entries in wire order.
    #[must_use]
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    /// Number of parameters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Encode the FastCGI 1/4-byte length-prefixed name/value payload.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LimitExceeded`] when the encoded payload exceeds
    /// 65535 bytes (one FastCGI record) or a name/value is too long for the
    /// 31-bit length field.
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_params_payload(&self.entries)
    }
}

fn validate_param(name: &str, value: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(Error::protocol(
            PROTOCOL,
            "parameter name must contain only ASCII alphanumerics and `_`",
        ));
    }
    if value.bytes().any(|byte| byte == 0) {
        return Err(Error::protocol(
            PROTOCOL,
            "parameter value contains a NUL byte",
        ));
    }
    Ok(())
}

/// Encode a list of name/value pairs into the FastCGI `PARAMS` payload.
///
/// # Errors
///
/// Propagates [`Error::Protocol`] for invalid names/values and
/// [`Error::LimitExceeded`] when a single record cannot carry the payload.
pub fn encode_params(pairs: &[(String, String)]) -> Result<Vec<u8>> {
    for (name, value) in pairs {
        validate_param(name, value)?;
    }
    encode_params_payload(pairs)
}

fn encode_params_payload(pairs: &[(String, String)]) -> Result<Vec<u8>> {
    // FastCGI 1.0 section 3.4: a name-value pair is `nameLength`,
    // `valueLength`, `name`, `value` — both lengths precede the bytes.
    let mut encoded = Vec::new();
    for (name, value) in pairs {
        write_length(&mut encoded, name.len())?;
        write_length(&mut encoded, value.len())?;
        encoded.extend_from_slice(name.as_bytes());
        encoded.extend_from_slice(value.as_bytes());
    }
    if encoded.len() > MAX_RECORD_CONTENT_BYTES {
        return Err(Error::limit(
            PROTOCOL,
            "FCGI_PARAMS payload",
            encoded.len(),
            MAX_RECORD_CONTENT_BYTES,
        ));
    }
    Ok(encoded)
}

/// Write the FastCGI name/value length encoding: one byte below 128, otherwise
/// four bytes with the high bit of the first byte set.
fn write_length(out: &mut Vec<u8>, length: usize) -> Result<()> {
    let length = u32::try_from(length).map_err(|_| {
        Error::limit(
            PROTOCOL,
            "FCGI_PARAMS name/value length",
            usize::MAX,
            u32::MAX as usize,
        )
    })?;
    if length < 128 {
        out.push(length as u8);
    } else {
        if length > 0x7fff_ffff {
            return Err(Error::limit(
                PROTOCOL,
                "FCGI_PARAMS name/value length",
                length as usize,
                0x7fff_ffff,
            ));
        }
        out.push(((length >> 24) as u8 & 0x7f) | 0x80);
        out.push((length >> 16) as u8);
        out.push((length >> 8) as u8);
        out.push(length as u8);
    }
    Ok(())
}

/// Decode a FastCGI `PARAMS` payload back into name/value pairs.
///
/// This is the inverse of [`FastCgiParams::encode`]; it exists for
/// management/echo tooling and for round-trip tests rather than for the edge's
/// responder path, but it uses the same length encoding and bounds.
///
/// # Errors
///
/// Returns [`Error::Framing`] for a truncated or unterminated length/value,
/// [`Error::InvalidUtf8`] for non-UTF-8 names/values, and
/// [`Error::LimitExceeded`] when the pair count or decoded bytes exceed
/// `limits`.
pub fn decode_params(payload: &[u8], limits: &Limits) -> Result<Vec<(String, String)>> {
    let limits = limits.sanitized();
    let mut pairs = Vec::new();
    let mut rest = payload;
    while !rest.is_empty() {
        // Both lengths precede the name and value bytes (FastCGI 1.0 §3.4).
        let (name_length, after_name_length) = take_length(rest)?;
        let (value_length, after_value_length) = take_length(after_name_length)?;
        let name = after_value_length
            .get(..name_length)
            .ok_or(Error::Truncated {
                protocol: PROTOCOL,
                field: "FCGI_PARAMS name",
                expected: name_length.saturating_sub(after_value_length.len()),
            })?;
        let after_name = &after_value_length[name_length..];
        let value = after_name.get(..value_length).ok_or(Error::Truncated {
            protocol: PROTOCOL,
            field: "FCGI_PARAMS value",
            expected: value_length.saturating_sub(after_name.len()),
        })?;
        rest = &after_name[value_length..];
        let name = std::str::from_utf8(name).map_err(|_| Error::InvalidUtf8 {
            protocol: PROTOCOL,
            field: "FCGI_PARAMS name",
        })?;
        let value = std::str::from_utf8(value).map_err(|_| Error::InvalidUtf8 {
            protocol: PROTOCOL,
            field: "FCGI_PARAMS value",
        })?;
        pairs.push((name.to_owned(), value.to_owned()));
        enforce_bounds(&pairs, &limits)?;
    }
    Ok(pairs)
}

fn take_length(payload: &[u8]) -> Result<(usize, &[u8])> {
    let Some(first) = payload.first().copied() else {
        return Err(Error::Truncated {
            protocol: PROTOCOL,
            field: "FCGI_PARAMS length",
            expected: 1,
        });
    };
    if first & 0x80 == 0 {
        return Ok((usize::from(first), &payload[1..]));
    }
    let length_bytes = payload.get(..4).ok_or(Error::Truncated {
        protocol: PROTOCOL,
        field: "FCGI_PARAMS length",
        expected: 4usize.saturating_sub(payload.len()),
    })?;
    let length = (u32::from(length_bytes[0] & 0x7f) << 24)
        | (u32::from(length_bytes[1]) << 16)
        | (u32::from(length_bytes[2]) << 8)
        | u32::from(length_bytes[3]);
    Ok((length as usize, &payload[4..]))
}

/// Allocate FastCGI request ids without ever using the reserved id `0`.
///
/// nginx uses the constant connection id `1` because it sends one request per
/// upstream connection. A pool that multiplexes must allocate a fresh id per
/// request; this allocator hands out `1..=65535` once and reports exhaustion
/// instead of wrapping into an id that would alias a live request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdAllocator {
    next: u32,
}

impl Default for RequestIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestIdAllocator {
    /// Start at request id `1`.
    #[must_use]
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// Allocate the next request id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RequestIdExhausted`] after id `65535` has been
    /// allocated.
    pub fn allocate(&mut self) -> Result<u16> {
        if self.next > u32::from(u16::MAX) {
            return Err(Error::RequestIdExhausted);
        }
        let id = self.next as u16;
        self.next += 1;
        Ok(id)
    }
}

/// Encode one `FCGI_BEGIN_REQUEST` record for `request_id`.
///
/// # Errors
///
/// Returns [`Error::Protocol`] for the reserved request id `0`.
pub fn encode_begin_request(request_id: u16, keep_conn: bool) -> Result<Vec<u8>> {
    validate_request_id(request_id)?;
    let mut content = [0u8; 8];
    content[0] = (FCGI_RESPONDER >> 8) as u8;
    content[1] = FCGI_RESPONDER as u8;
    content[2] = if keep_conn { FCGI_KEEP_CONN } else { 0 };
    let mut out = Vec::with_capacity(RECORD_HEADER_BYTES + content.len());
    write_record(&mut out, FCGI_BEGIN_REQUEST, request_id, &content)?;
    Ok(out)
}

/// Encode one `FCGI_ABORT_REQUEST` record for `request_id`.
///
/// # Errors
///
/// Returns [`Error::Protocol`] for the reserved request id `0`.
pub fn encode_abort_request(request_id: u16) -> Result<Vec<u8>> {
    validate_request_id(request_id)?;
    let mut out = Vec::with_capacity(RECORD_HEADER_BYTES);
    write_record(&mut out, FCGI_ABORT_REQUEST, request_id, &[])?;
    Ok(out)
}

/// Encode a complete FastCGI request (begin, params, stdin) into one buffer.
///
/// The buffer is the exact byte stream a caller writes to the upstream socket.
/// For a streamed request body use [`encode_request_head`] and
/// [`encode_stdin_chunks`] instead.
///
/// # Errors
///
/// Returns [`Error::Protocol`] for an invalid request id/parameter and
/// [`Error::LimitExceeded`] when the body or parameters exceed `limits`.
///
/// # Examples
///
/// ```
/// use sdkwork_webserver_upstream_protocol::{
///     fastcgi::{FastCgiParams, encode_request},
///     Limits,
/// };
///
/// let mut params = FastCgiParams::new();
/// params.push("REQUEST_METHOD", "POST").expect("param");
/// params.push("CONTENT_LENGTH", "5").expect("param");
/// let wire = encode_request(1, false, &params, b"hello", &Limits::default())
///     .expect("request");
/// assert!(!wire.is_empty());
/// ```
pub fn encode_request(
    request_id: u16,
    keep_conn: bool,
    params: &FastCgiParams,
    stdin: &[u8],
    limits: &Limits,
) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    let mut out = encode_request_head(request_id, keep_conn, params, &limits)?;
    out.extend_from_slice(&encode_stdin_chunks(request_id, stdin, &limits)?);
    Ok(out)
}

/// Encode the request head: `BEGIN_REQUEST` records followed by `PARAMS`
/// records and the terminating empty `PARAMS` record.
///
/// # Errors
///
/// See [`encode_request`].
pub fn encode_request_head(
    request_id: u16,
    keep_conn: bool,
    params: &FastCgiParams,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    validate_request_id(request_id)?;
    let payload = params.encode()?;
    if payload.len() > limits.max_params_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "FCGI_PARAMS payload",
            payload.len(),
            limits.max_params_bytes,
        ));
    }
    let mut out = encode_begin_request(request_id, keep_conn)?;
    encode_params_records(&mut out, request_id, &payload)?;
    write_record(&mut out, FCGI_PARAMS, request_id, &[])?;
    Ok(out)
}

/// Encode the `STDIN` records for `body`, including the terminating empty
/// `STDIN` record.
///
/// # Errors
///
/// Returns [`Error::Protocol`] for the reserved request id `0` and
/// [`Error::LimitExceeded`] when the chunk bound cannot hold the body.
pub fn encode_stdin_chunks(request_id: u16, body: &[u8], limits: &Limits) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    validate_request_id(request_id)?;
    let chunk = limits.max_stdin_chunk_bytes.min(MAX_RECORD_CONTENT_BYTES);
    let mut out = Vec::with_capacity(body.len() + RECORD_HEADER_BYTES * 2 + 8);
    for chunk in body.chunks(chunk.max(1)) {
        write_record(&mut out, FCGI_STDIN, request_id, chunk)?;
    }
    write_record(&mut out, FCGI_STDIN, request_id, &[])?;
    Ok(out)
}

fn validate_request_id(request_id: u16) -> Result<()> {
    if request_id == 0 {
        return Err(Error::protocol(
            PROTOCOL,
            "request id 0 is reserved for management records",
        ));
    }
    Ok(())
}

/// Append one record, zero-padding its content to an 8-byte boundary.
fn write_record(out: &mut Vec<u8>, record_type: u8, request_id: u16, content: &[u8]) -> Result<()> {
    if content.len() > MAX_RECORD_CONTENT_BYTES {
        return Err(Error::limit(
            PROTOCOL,
            "record content",
            content.len(),
            MAX_RECORD_CONTENT_BYTES,
        ));
    }
    let padding = (8 - content.len() % 8) % 8;
    let content_length = content.len() as u16;
    out.push(FASTCGI_VERSION);
    out.push(record_type);
    out.extend_from_slice(&request_id.to_be_bytes());
    out.extend_from_slice(&content_length.to_be_bytes());
    out.push(padding as u8);
    out.push(0);
    out.extend_from_slice(content);
    out.resize(out.len() + padding, 0);
    Ok(())
}

fn encode_params_records(out: &mut Vec<u8>, request_id: u16, payload: &[u8]) -> Result<()> {
    for chunk in payload.chunks(MAX_RECORD_CONTENT_BYTES) {
        write_record(out, FCGI_PARAMS, request_id, chunk)?;
    }
    Ok(())
}

/// `END_REQUEST` body: application status and protocol status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastCgiEndRequest {
    /// Application-level status (opaque to the edge, reported for logging).
    pub app_status: u32,
    /// FastCGI protocol status: `0` complete, `1` cannot multiplex, `2`
    /// overloaded, `3` unknown role.
    pub protocol_status: u8,
}

impl FastCgiEndRequest {
    /// Whether the upstream reported a complete, successful request.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.protocol_status == 0
    }
}

/// One parsed FastCGI response record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastCgiResponseEvent {
    /// The response header block was parsed. Emitted exactly once.
    Head(HttpResponseHead),
    /// A `STDOUT` body chunk (header block already consumed).
    Body(Vec<u8>),
    /// A `STDERR` chunk carried by the application.
    Stderr(Vec<u8>),
    /// The upstream sent `END_REQUEST`.
    End(FastCgiEndRequest),
}

/// Incremental FastCGI response parser.
///
/// Feed raw upstream bytes with [`Self::feed`]; the parser buffers only one
/// record and one header block at a time, enforcing the configured bounds.
/// After [`FastCgiResponseEvent::End`] any remaining bytes belong to the next
/// response on a `KEEP_CONN` connection: drain them with
/// [`Self::take_remaining`] and call [`Self::reset`] before reusing the parser.
#[derive(Debug, Clone)]
pub struct FastCgiResponseParser {
    limits: Limits,
    buffer: Vec<u8>,
    request_id: Option<u16>,
    head: Option<HttpResponseHead>,
    head_buffer: Vec<u8>,
    stdout_closed: bool,
    end: Option<FastCgiEndRequest>,
}

impl FastCgiResponseParser {
    /// Create a parser enforcing `limits` (zero fields are clamped to one).
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            limits: limits.sanitized(),
            buffer: Vec::new(),
            request_id: None,
            head: None,
            head_buffer: Vec::new(),
            stdout_closed: false,
            end: None,
        }
    }

    /// Feed upstream bytes, returning every event completed by this input.
    ///
    /// # Errors
    ///
    /// - [`Error::Framing`] for a malformed record header or `END_REQUEST`.
    /// - [`Error::Protocol`] for an unsupported record type, mismatched request
    ///   id, records after `END_REQUEST`, or a closed stdout before the header
    ///   block completed.
    /// - [`Error::Truncated`]-free: a partial record is buffered and reported
    ///   by [`Self::finish`]; this method only fails on structurally invalid
    ///   bytes.
    /// - [`Error::LimitExceeded`] for an oversized header block.
    pub fn feed(&mut self, input: &[u8]) -> Result<Vec<FastCgiResponseEvent>> {
        if self.end.is_some() {
            if !input.is_empty() {
                return Err(Error::protocol(
                    PROTOCOL,
                    "records received after FCGI_END_REQUEST",
                ));
            }
            return Ok(Vec::new());
        }
        let maximum_buffer = RECORD_HEADER_BYTES
            .saturating_add(MAX_RECORD_CONTENT_BYTES)
            .saturating_add(255);
        if self.buffer.len().saturating_add(input.len()) > maximum_buffer {
            return Err(Error::limit(
                PROTOCOL,
                "buffered response record",
                self.buffer.len().saturating_add(input.len()),
                maximum_buffer,
            ));
        }
        self.buffer.extend_from_slice(input);

        let mut events = Vec::new();
        loop {
            if self.end.is_some() || self.buffer.len() < RECORD_HEADER_BYTES {
                break;
            }
            let header = self
                .buffer
                .get(..RECORD_HEADER_BYTES)
                .ok_or_else(|| Error::framing(PROTOCOL, "record header disappeared"))?;
            let version = header[0];
            let record_type = header[1];
            let request_id = u16::from_be_bytes([header[2], header[3]]);
            let content_length = usize::from(u16::from_be_bytes([header[4], header[5]]));
            let padding_length = usize::from(header[6]);
            if version != FASTCGI_VERSION {
                return Err(Error::framing(
                    PROTOCOL,
                    format!("unsupported record version {version}"),
                ));
            }
            let total = RECORD_HEADER_BYTES
                .checked_add(content_length)
                .and_then(|total| total.checked_add(padding_length))
                .ok_or_else(|| Error::framing(PROTOCOL, "record length overflow"))?;
            if self.buffer.len() < total {
                break;
            }
            if request_id == 0 {
                return Err(Error::protocol(
                    PROTOCOL,
                    "management records (request id 0) are not accepted in a response",
                ));
            }
            match self.request_id {
                Some(expected) if expected != request_id => {
                    return Err(Error::protocol(
                        PROTOCOL,
                        "response record request id changed mid-response",
                    ));
                }
                None => self.request_id = Some(request_id),
                Some(_) => {}
            }
            let content_end = RECORD_HEADER_BYTES + content_length;
            let content = self
                .buffer
                .get(RECORD_HEADER_BYTES..content_end)
                .ok_or_else(|| Error::framing(PROTOCOL, "record content disappeared"))?
                .to_vec();
            self.buffer.drain(..total);

            match record_type {
                FCGI_STDOUT => {
                    if content.is_empty() {
                        self.stdout_closed = true;
                        if self.head.is_none() {
                            return Err(Error::protocol(
                                PROTOCOL,
                                "FCGI_STDOUT closed before the response header block completed",
                            ));
                        }
                        continue;
                    }
                    if self.stdout_closed {
                        return Err(Error::protocol(
                            PROTOCOL,
                            "FCGI_STDOUT data received after the stdout stream was closed",
                        ));
                    }
                    if self.head.is_some() {
                        events.push(FastCgiResponseEvent::Body(content));
                        continue;
                    }
                    self.head_buffer.extend_from_slice(&content);
                    if let Some((head, consumed)) = self.parse_head()? {
                        self.head = Some(head.clone());
                        events.push(FastCgiResponseEvent::Head(head));
                        let consumed = consumed.min(self.head_buffer.len());
                        let remainder = self.head_buffer.split_off(consumed);
                        if !remainder.is_empty() {
                            events.push(FastCgiResponseEvent::Body(remainder));
                        }
                    }
                }
                FCGI_STDERR => {
                    if !content.is_empty() {
                        events.push(FastCgiResponseEvent::Stderr(content));
                    }
                }
                FCGI_END_REQUEST => {
                    if content.len() != END_REQUEST_CONTENT_BYTES {
                        return Err(Error::framing(
                            PROTOCOL,
                            "FCGI_END_REQUEST content must be 8 bytes",
                        ));
                    }
                    let app_status =
                        u32::from_be_bytes([content[0], content[1], content[2], content[3]]);
                    let protocol_status = content[4];
                    if self.head.is_none() && protocol_status == 0 {
                        return Err(Error::protocol(
                            PROTOCOL,
                            "FCGI_END_REQUEST arrived before the response header block",
                        ));
                    }
                    let end = FastCgiEndRequest {
                        app_status,
                        protocol_status,
                    };
                    self.end = Some(end);
                    events.push(FastCgiResponseEvent::End(end));
                }
                other => {
                    return Err(Error::protocol(
                        PROTOCOL,
                        format!("unsupported record type {other} in a response"),
                    ));
                }
            }
        }
        Ok(events)
    }

    fn parse_head(&mut self) -> Result<Option<(HttpResponseHead, usize)>> {
        parse_response_head(PROTOCOL, &self.head_buffer, HeaderStyle::Cgi, &self.limits)
    }

    /// Whether `END_REQUEST` has been parsed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.end.is_some()
    }

    /// The parsed response head, when available.
    #[must_use]
    pub fn head(&self) -> Option<&HttpResponseHead> {
        self.head.as_ref()
    }

    /// Drain bytes buffered after the last complete record.
    ///
    /// On a `KEEP_CONN` connection those bytes are the start of the next
    /// response; the caller forwards them to a parser after [`Self::reset`].
    #[must_use]
    pub fn take_remaining(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buffer)
    }

    /// Reset per-response state while keeping the buffered bytes.
    ///
    /// Used after [`Self::take_remaining`] to parse the next response on a
    /// persistent FastCGI connection.
    pub fn reset(&mut self) {
        self.request_id = None;
        self.head = None;
        self.head_buffer.clear();
        self.stdout_closed = false;
        self.end = None;
    }

    /// Validate that the response is complete and no partial record remains.
    ///
    /// # Errors
    ///
    /// - [`Error::Protocol`] when `END_REQUEST` was never received or the
    ///   header block is incomplete.
    /// - [`Error::Truncated`] when buffered bytes form a partial record.
    pub fn finish(&self) -> Result<()> {
        if !self.buffer.is_empty() {
            return Err(Error::Truncated {
                protocol: PROTOCOL,
                field: "FastCGI record",
                expected: RECORD_HEADER_BYTES,
            });
        }
        if self.end.is_none() {
            return Err(Error::protocol(
                PROTOCOL,
                "response ended without FCGI_END_REQUEST",
            ));
        }
        if self.head.is_none() && self.end.is_some_and(|end| end.is_complete()) {
            return Err(Error::protocol(
                PROTOCOL,
                "response ended without a complete header block",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> FastCgiParams {
        let mut params = FastCgiParams::new();
        for (name, value) in pairs {
            params.push(name, value).expect("valid param");
        }
        params
    }

    #[test]
    fn begin_request_record_matches_the_specification() {
        let encoded = encode_begin_request(7, true).expect("begin");
        assert_eq!(encoded.len(), RECORD_HEADER_BYTES + 8);
        assert_eq!(encoded[0], FASTCGI_VERSION);
        assert_eq!(encoded[1], FCGI_BEGIN_REQUEST);
        assert_eq!(&encoded[2..4], &7u16.to_be_bytes());
        assert_eq!(&encoded[4..6], &8u16.to_be_bytes());
        assert_eq!(encoded[6], 0); // no padding: 8-byte content
        assert_eq!(&encoded[8..10], &FCGI_RESPONDER.to_be_bytes());
        assert_eq!(encoded[10], FCGI_KEEP_CONN);
        assert_eq!(&encoded[11..16], &[0, 0, 0, 0, 0]);
    }

    #[test]
    fn abort_request_is_a_body_less_record() {
        let encoded = encode_abort_request(3).expect("abort");
        assert_eq!(encoded.len(), RECORD_HEADER_BYTES);
        assert_eq!(encoded[1], FCGI_ABORT_REQUEST);
        assert_eq!(&encoded[4..6], &[0, 0]);
    }

    #[test]
    fn request_id_zero_and_exhaustion_fail_closed() {
        assert!(encode_begin_request(0, false).is_err());
        assert!(encode_abort_request(0).is_err());
        let mut allocator = RequestIdAllocator::new();
        assert_eq!(allocator.allocate().unwrap(), 1);
        assert_eq!(allocator.allocate().unwrap(), 2);
        allocator.next = u32::from(u16::MAX);
        assert_eq!(allocator.allocate().unwrap(), u16::MAX);
        assert!(matches!(
            allocator.allocate(),
            Err(Error::RequestIdExhausted)
        ));
    }

    #[test]
    fn params_length_encoding_uses_one_or_four_bytes() {
        let short =
            FastCgiParams::from_variables(&[("A".to_owned(), "b".to_owned())], &Limits::default())
                .expect("short");
        assert_eq!(short.encode().unwrap(), b"\x01\x01Ab");

        let long_value = "v".repeat(128);
        let long = FastCgiParams::from_variables(
            &[("A".to_owned(), long_value.clone())],
            &Limits::default(),
        )
        .expect("long");
        let encoded = long.encode().unwrap();
        // name length (1 byte), value length (4-byte form), then name + value.
        assert_eq!(&encoded[..7], &[1, 0x80, 0, 0, 128, b'A', b'v']);
        assert_eq!(encoded.len(), 6 + 128);
    }

    #[test]
    fn params_decode_round_trips_and_rejects_malformed_payloads() {
        let params = params(&[
            ("REQUEST_METHOD", "GET"),
            ("QUERY_STRING", ""),
            ("HTTP_X_LONG", &"v".repeat(300)),
        ]);
        let encoded = params.encode().expect("encode");
        assert_eq!(
            decode_params(&encoded, &Limits::default()).expect("decode"),
            params.entries()
        );

        // Truncated one-byte length.
        assert!(matches!(
            decode_params(&[], &Limits::default()).expect("empty"),
            pairs if pairs.is_empty()
        ));
        assert!(matches!(
            decode_params(&[0x05, b'A'], &Limits::default()),
            Err(Error::Truncated { .. })
        ));
        // Truncated four-byte length.
        assert!(matches!(
            decode_params(&[0x80, 0, 0], &Limits::default()),
            Err(Error::Truncated { .. })
        ));
        // Truncated value.
        assert!(matches!(
            decode_params(&[0x01, b'A', 0x05, b'b'], &Limits::default()),
            Err(Error::Truncated { .. })
        ));
        // Non-UTF-8 name (`nameLength=1`, `valueLength=1`, name 0xff, value `A`).
        assert!(matches!(
            decode_params(&[0x01, 0x01, 0xff, b'A'], &Limits::default()),
            Err(Error::InvalidUtf8 { .. })
        ));
        // Pair-count bound.
        let limits = Limits {
            max_params: 1,
            ..Limits::default()
        };
        assert!(matches!(
            decode_params(&encoded, &limits),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn full_request_has_begin_params_empty_params_and_stdin_terminator() {
        let params = params(&[("REQUEST_METHOD", "GET"), ("QUERY_STRING", "")]);
        let encoded = encode_request(1, false, &params, b"body", &Limits::default()).unwrap();
        let mut types = Vec::new();
        let mut offset = 0usize;
        while offset < encoded.len() {
            types.push(encoded[offset + 1]);
            let content_length = usize::from(u16::from_be_bytes([
                encoded[offset + 4],
                encoded[offset + 5],
            ]));
            let padding = usize::from(encoded[offset + 6]);
            offset += RECORD_HEADER_BYTES + content_length + padding;
        }
        assert_eq!(
            types,
            vec![
                FCGI_BEGIN_REQUEST,
                FCGI_PARAMS,
                FCGI_PARAMS,
                FCGI_STDIN,
                FCGI_STDIN
            ]
        );
        // The final STDIN record is empty and the request ends exactly there.
        assert_eq!(encoded[encoded.len() - RECORD_HEADER_BYTES + 1], FCGI_STDIN);
    }

    #[test]
    fn stdin_is_chunked_and_each_record_padded() {
        let limits = Limits {
            max_stdin_chunk_bytes: 10,
            ..Limits::default()
        };
        let body = vec![0x5au8; 25];
        let encoded = encode_stdin_chunks(1, &body, &limits).unwrap();
        let mut chunks = Vec::new();
        let mut offset = 0usize;
        while offset < encoded.len() {
            let content_length = usize::from(u16::from_be_bytes([
                encoded[offset + 4],
                encoded[offset + 5],
            ]));
            let padding = usize::from(encoded[offset + 6]);
            assert_eq!(padding, (8 - content_length % 8) % 8);
            if content_length > 0 {
                chunks.push(content_length);
            }
            offset += RECORD_HEADER_BYTES + content_length + padding;
        }
        assert_eq!(chunks, vec![10, 10, 5]);
        assert_eq!(offset, encoded.len());
    }

    fn stdout_record(request_id: u16, content: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_record(&mut out, FCGI_STDOUT, request_id, content).unwrap();
        out
    }

    fn end_request_record(request_id: u16, app_status: u32, protocol_status: u8) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(&app_status.to_be_bytes());
        content.push(protocol_status);
        content.extend_from_slice(&[0, 0, 0]);
        let mut out = Vec::new();
        write_record(&mut out, FCGI_END_REQUEST, request_id, &content).unwrap();
        out
    }

    fn parse_all(bytes: &[u8]) -> Result<(HttpResponseHead, Vec<u8>, Vec<Vec<u8>>)> {
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let events = parser.feed(bytes)?;
        parser.finish()?;
        let mut head = None;
        let mut body = Vec::new();
        let mut stderr = Vec::new();
        for event in events {
            match event {
                FastCgiResponseEvent::Head(value) => head = Some(value),
                FastCgiResponseEvent::Body(mut chunk) => body.append(&mut chunk),
                FastCgiResponseEvent::Stderr(chunk) => stderr.push(chunk),
                FastCgiResponseEvent::End(_) => {}
            }
        }
        Ok((head.expect("head"), body, stderr))
    }

    #[test]
    fn response_is_parsed_across_records_and_streaming_boundaries() {
        let mut wire = stdout_record(
            1,
            b"Status: 201 Created\r\nContent-Type: text/plain\r\n\r\nhello ",
        );
        wire.extend_from_slice(&stdout_record(1, b"world"));
        wire.extend_from_slice(&stdout_record(1, b""));
        wire.extend_from_slice(&end_request_record(1, 0, 0));

        // Byte-at-a-time feeding exercises every split point.
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let mut head = None;
        let mut body = Vec::new();
        for byte in &wire {
            for event in parser.feed(std::slice::from_ref(byte)).unwrap() {
                match event {
                    FastCgiResponseEvent::Head(value) => head = Some(value),
                    FastCgiResponseEvent::Body(mut chunk) => body.append(&mut chunk),
                    FastCgiResponseEvent::Stderr(_) | FastCgiResponseEvent::End(_) => {}
                }
            }
        }
        parser.finish().unwrap();
        assert_eq!(head.expect("head").status, 201);
        assert_eq!(body, b"hello world");
    }

    #[test]
    fn stderr_chunks_are_reported_separately() {
        let mut wire = stdout_record(1, b"Status: 200 OK\r\n\r\n");
        wire.extend_from_slice(&stdout_record(1, b""));
        let mut stderr = Vec::new();
        write_record(&mut stderr, FCGI_STDERR, 1, b"warning").unwrap();
        wire.extend_from_slice(&stderr);
        wire.extend_from_slice(&end_request_record(1, 0, 0));
        let (_, _, chunks) = parse_all(&wire).unwrap();
        assert_eq!(chunks, vec![b"warning".to_vec()]);
    }

    #[test]
    fn end_request_status_is_reported() {
        let mut wire = stdout_record(1, b"Status: 200 OK\r\n\r\n");
        wire.extend_from_slice(&stdout_record(1, b""));
        wire.extend_from_slice(&end_request_record(1, 9, 2));
        let (_, _, _) = parse_all(&wire).unwrap();
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let events = parser.feed(&wire).unwrap();
        let end = events
            .iter()
            .find_map(|event| match event {
                FastCgiResponseEvent::End(end) => Some(*end),
                _ => None,
            })
            .expect("end event");
        assert_eq!(end.app_status, 9);
        assert_eq!(end.protocol_status, 2);
        assert!(!end.is_complete());
    }

    #[test]
    fn truncated_and_unknown_records_fail_closed() {
        let mut parser = FastCgiResponseParser::new(Limits::default());
        assert!(parser.feed(&[1, 6, 0, 1]).unwrap().is_empty());
        assert!(matches!(parser.finish(), Err(Error::Truncated { .. })));

        let mut parser = FastCgiResponseParser::new(Limits::default());
        let error = parser
            .feed(&[2, FCGI_STDOUT, 0, 1, 0, 0, 0, 0])
            .expect_err("bad version");
        assert!(matches!(error, Error::Framing { .. }));

        let mut parser = FastCgiResponseParser::new(Limits::default());
        let mut unknown = Vec::new();
        write_record(&mut unknown, 99, 1, b"x").unwrap();
        assert!(parser.feed(&unknown).is_err());
    }

    #[test]
    fn response_without_end_request_fails_closed() {
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let wire = stdout_record(1, b"Status: 200 OK\r\n\r\nbody");
        parser.feed(&wire).unwrap();
        assert!(matches!(parser.finish(), Err(Error::Protocol { .. })));
    }

    #[test]
    fn stdout_closed_before_header_block_fails_closed() {
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let wire = stdout_record(1, b"");
        assert!(parser.feed(&wire).is_err());
    }

    #[test]
    fn mismatched_request_id_and_management_records_fail_closed() {
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let mut wire = stdout_record(1, b"Status: 200 OK\r\n\r\n");
        wire.extend_from_slice(&stdout_record(2, b"x"));
        assert!(parser.feed(&wire).is_err());

        let mut parser = FastCgiResponseParser::new(Limits::default());
        let mut management = Vec::new();
        write_record(&mut management, FCGI_STDOUT, 0, b"x").unwrap();
        assert!(parser.feed(&management).is_err());
    }

    #[test]
    fn oversized_header_block_is_rejected_before_growing() {
        let limits = Limits {
            max_header_block_bytes: 16,
            ..Limits::default()
        };
        let mut parser = FastCgiResponseParser::new(limits);
        let wire = stdout_record(1, b"X-Long: aaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert!(matches!(
            parser.feed(&wire),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn keep_conn_remainder_is_drainable_and_resettable() {
        let mut wire = stdout_record(1, b"Status: 200 OK\r\n\r\nfirst");
        wire.extend_from_slice(&stdout_record(1, b""));
        wire.extend_from_slice(&end_request_record(1, 0, 0));
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let events = parser.feed(&wire).unwrap();
        assert!(parser.is_complete());
        assert!(matches!(events.last(), Some(FastCgiResponseEvent::End(_))));
        assert!(parser.take_remaining().is_empty());
        parser.reset();
        assert!(!parser.is_complete());
        assert!(parser.feed(&[]).unwrap().is_empty());
    }

    #[test]
    fn oversized_buffered_input_is_rejected() {
        let mut parser = FastCgiResponseParser::new(Limits::default());
        let header = [1, 7, 0, 1, 0xff, 0xff, 255, 0];
        let mut input = header.to_vec();
        input.resize(RECORD_HEADER_BYTES + 20, 0);
        // A partial record is buffered, not an error...
        assert!(parser.feed(&input).unwrap().is_empty());
        // ...but a buffer beyond one maximal record plus header is refused.
        let error = parser.feed(&vec![0u8; MAX_RECORD_CONTENT_BYTES + 300]);
        assert!(matches!(error, Err(Error::LimitExceeded { .. })));
    }
}
