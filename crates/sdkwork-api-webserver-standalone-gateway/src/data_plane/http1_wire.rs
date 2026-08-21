use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use axum_server::accept::Accept;
use http::Request;
use sdkwork_webserver_core::WebServerLimits;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::server::TlsStream;
use tower::Service;

use super::connection_limit::ConnectionLimitedStream;
use super::metrics::{DataPlaneMetrics, ProtocolErrorKind};

const READ_SCRATCH_BYTES: usize = 8 * 1024;
const MAX_WIRE_BODY_BYTES: u64 = 2_147_483_648;

#[derive(Clone, Copy)]
struct WireLimits {
    max_pipeline_depth: usize,
    max_header_bytes: usize,
    max_request_line_bytes: usize,
    max_request_method_bytes: usize,
    max_request_target_bytes: usize,
    max_header_name_bytes: usize,
    max_header_value_bytes: usize,
    max_headers: usize,
    max_body_bytes: u64,
    max_chunk_line_bytes: usize,
    max_trailer_bytes: usize,
    max_trailers: usize,
}

impl From<&WebServerLimits> for WireLimits {
    fn from(limits: &WebServerLimits) -> Self {
        Self {
            max_pipeline_depth: limits.http1_max_pipeline_depth,
            max_header_bytes: limits.max_request_header_bytes,
            max_request_line_bytes: limits.max_request_line_bytes,
            max_request_method_bytes: limits.max_request_method_bytes,
            max_request_target_bytes: limits.max_request_target_bytes,
            max_header_name_bytes: limits.max_header_name_bytes,
            max_header_value_bytes: limits.max_header_value_bytes,
            max_headers: limits.max_request_headers,
            max_body_bytes: MAX_WIRE_BODY_BYTES,
            max_chunk_line_bytes: limits.max_chunk_line_bytes,
            max_trailer_bytes: limits.max_trailer_bytes,
            max_trailers: limits.max_trailers,
        }
    }
}

pub(crate) trait NegotiatedHttpProtocol {
    fn is_http1(&self) -> bool;
}

impl<I> NegotiatedHttpProtocol for ConnectionLimitedStream<I> {
    fn is_http1(&self) -> bool {
        true
    }
}

impl<I> NegotiatedHttpProtocol for TlsStream<I> {
    fn is_http1(&self) -> bool {
        self.get_ref().1.alpn_protocol() != Some(b"h2")
    }
}

#[derive(Clone)]
pub(crate) struct Http1WireGuardAcceptor<A> {
    inner: A,
    limits: WireLimits,
    metrics: Option<Arc<DataPlaneMetrics>>,
}

impl<A> Http1WireGuardAcceptor<A> {
    pub(crate) fn new_observed(
        inner: A,
        limits: &WebServerLimits,
        metrics: Arc<DataPlaneMetrics>,
    ) -> Self {
        Self {
            inner,
            limits: limits.into(),
            metrics: Some(metrics),
        }
    }
}

impl<A, I, S> Accept<I, S> for Http1WireGuardAcceptor<A>
where
    A: Accept<I, S> + Clone + Send + Sync + 'static,
    A::Future: Send + 'static,
    A::Stream: NegotiatedHttpProtocol + AsyncRead + AsyncWrite + Send + Unpin + 'static,
    A::Service: Send + 'static,
    I: Send + 'static,
    S: Send + 'static,
{
    type Stream = Http1WireGuardStream<A::Stream>;
    type Service = Http1PipelineService<A::Service>;
    type Future =
        Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send + 'static>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.inner.clone();
        let limits = self.limits;
        let metrics = self.metrics.clone();
        Box::pin(async move {
            let (stream, service) = inner.accept(stream, service).await?;
            let pipeline = stream
                .is_http1()
                .then(|| Arc::new(PipelineState::new(limits.max_pipeline_depth)));
            let parser = pipeline
                .as_ref()
                .map(|pipeline| Http1WireParser::new(limits, pipeline.clone()));
            Ok((
                Http1WireGuardStream {
                    inner: stream,
                    parser,
                    metrics,
                    wire_error_recorded: false,
                },
                Http1PipelineService {
                    inner: service,
                    pipeline,
                },
            ))
        })
    }
}

struct PipelineState {
    maximum: usize,
    pending_heads: AtomicUsize,
    upgraded: AtomicBool,
}

impl PipelineState {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            pending_heads: AtomicUsize::new(0),
            upgraded: AtomicBool::new(false),
        }
    }

    fn enqueue_request_head(&self) -> io::Result<()> {
        self.pending_heads
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < self.maximum).then_some(current + 1)
            })
            .map(|_| ())
            .map_err(|_| wire_error("HTTP/1 Pipeline depth exceeds the configured limit"))
    }

    fn begin_dispatch(&self) {
        let result =
            self.pending_heads
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_sub(1)
                });
        debug_assert!(
            result.is_ok(),
            "HTTP/1 service dispatch must follow a parsed request head"
        );
    }

    fn activate_upgrade(&self) {
        self.upgraded.store(true, Ordering::Release);
    }

    fn is_upgraded(&self) -> bool {
        self.upgraded.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub(crate) struct Http1UpgradeGuard {
    pipeline: Arc<PipelineState>,
}

impl Http1UpgradeGuard {
    pub(crate) fn activate(&self) {
        self.pipeline.activate_upgrade();
    }
}

#[derive(Clone)]
pub(crate) struct Http1PipelineService<S> {
    inner: S,
    pipeline: Option<Arc<PipelineState>>,
}

impl<S, R> Service<Request<R>> for Http1PipelineService<S>
where
    S: Service<Request<R>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, mut request: Request<R>) -> Self::Future {
        if let Some(pipeline) = &self.pipeline {
            pipeline.begin_dispatch();
            request.extensions_mut().insert(Http1UpgradeGuard {
                pipeline: pipeline.clone(),
            });
        }
        self.inner.call(request)
    }
}

pub(crate) struct Http1WireGuardStream<I> {
    inner: I,
    parser: Option<Http1WireParser>,
    metrics: Option<Arc<DataPlaneMetrics>>,
    wire_error_recorded: bool,
}

impl<I: AsyncRead + Unpin> AsyncRead for Http1WireGuardStream<I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.parser.is_none() {
            return Pin::new(&mut self.inner).poll_read(context, output);
        }
        if self
            .parser
            .as_ref()
            .is_some_and(Http1WireParser::is_upgraded)
        {
            return Pin::new(&mut self.inner).poll_read(context, output);
        }
        if output.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let mut scratch = [0_u8; READ_SCRATCH_BYTES];
        let maximum = output.remaining().min(scratch.len());
        let mut guarded = ReadBuf::new(&mut scratch[..maximum]);
        match Pin::new(&mut self.inner).poll_read(context, &mut guarded) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let bytes = guarded.filled();
                let inspection = self.parser.as_mut().map(|parser| parser.inspect(bytes));
                if let Some(Err(error)) = inspection {
                    if !self.wire_error_recorded {
                        if let Some(metrics) = &self.metrics {
                            metrics.record_protocol_error(ProtocolErrorKind::Http1Wire);
                        }
                        self.wire_error_recorded = true;
                    }
                    return Poll::Ready(Err(error));
                }
                output.put_slice(bytes);
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<I: AsyncWrite + Unpin> AsyncWrite for Http1WireGuardStream<I> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(context, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

impl<I: NegotiatedHttpProtocol> NegotiatedHttpProtocol for Http1WireGuardStream<I> {
    fn is_http1(&self) -> bool {
        self.inner.is_http1()
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Headers,
    FixedBody { remaining: u64 },
    ChunkSize,
    ChunkData { remaining: u64 },
    ChunkDataCr,
    ChunkDataLf,
    Trailers,
}

struct Http1WireParser {
    limits: WireLimits,
    pipeline: Arc<PipelineState>,
    phase: Phase,
    line: Vec<u8>,
    saw_cr: bool,
    header_bytes: usize,
    header_lines: usize,
    header_count: usize,
    content_length: Option<u64>,
    transfer_encoding: bool,
    chunk_body_bytes: u64,
    trailer_bytes: usize,
    trailer_count: usize,
}

impl Http1WireParser {
    fn new(limits: WireLimits, pipeline: Arc<PipelineState>) -> Self {
        Self {
            limits,
            pipeline,
            phase: Phase::Headers,
            line: Vec::with_capacity(limits.max_request_line_bytes.min(256)),
            saw_cr: false,
            header_bytes: 0,
            header_lines: 0,
            header_count: 0,
            content_length: None,
            transfer_encoding: false,
            chunk_body_bytes: 0,
            trailer_bytes: 0,
            trailer_count: 0,
        }
    }

    fn inspect(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut position = 0;
        while position < bytes.len() {
            match self.phase {
                Phase::FixedBody { remaining } => {
                    let consumed = remaining.min((bytes.len() - position) as u64);
                    position += consumed as usize;
                    let remaining = remaining - consumed;
                    self.phase = if remaining == 0 {
                        Phase::Headers
                    } else {
                        Phase::FixedBody { remaining }
                    };
                }
                Phase::ChunkData { remaining } => {
                    let consumed = remaining.min((bytes.len() - position) as u64);
                    position += consumed as usize;
                    let remaining = remaining - consumed;
                    self.phase = if remaining == 0 {
                        Phase::ChunkDataCr
                    } else {
                        Phase::ChunkData { remaining }
                    };
                }
                phase => {
                    let byte = bytes[position];
                    position += 1;
                    match phase {
                        Phase::Headers => self.inspect_header_byte(byte)?,
                        Phase::ChunkSize => self.inspect_chunk_size_byte(byte)?,
                        Phase::ChunkDataCr if byte == b'\r' => {
                            self.phase = Phase::ChunkDataLf;
                        }
                        Phase::ChunkDataLf if byte == b'\n' => {
                            self.phase = Phase::ChunkSize;
                        }
                        Phase::Trailers => self.inspect_trailer_byte(byte)?,
                        Phase::ChunkDataCr | Phase::ChunkDataLf => {
                            return Err(wire_error("invalid chunk data terminator"));
                        }
                        Phase::FixedBody { .. } | Phase::ChunkData { .. } => unreachable!(),
                    }
                }
            }
        }
        Ok(())
    }

    fn is_upgraded(&self) -> bool {
        self.pipeline.is_upgraded()
    }

    fn inspect_header_byte(&mut self, byte: u8) -> io::Result<()> {
        self.header_bytes = self.header_bytes.saturating_add(1);
        if self.header_bytes > self.limits.max_header_bytes {
            return Err(wire_error(
                "HTTP/1 request headers exceed the configured byte limit",
            ));
        }
        let line_limit = if self.header_lines == 0 {
            self.limits.max_request_line_bytes
        } else {
            self.header_field_line_limit()
        };
        if self.push_line_byte(byte, line_limit)? {
            self.finish_header_line()?;
        }
        Ok(())
    }

    fn finish_header_line(&mut self) -> io::Result<()> {
        if self.header_lines == 0 {
            if self.line.is_empty() {
                return Err(wire_error("HTTP/1 request line is empty"));
            }
            validate_request_line(&self.line, self.limits)?;
            self.header_lines = 1;
            self.line.clear();
            return Ok(());
        }
        if self.line.is_empty() {
            if self.content_length.is_some() && self.transfer_encoding {
                return Err(wire_error(
                    "HTTP/1 request contains Transfer-Encoding and Content-Length",
                ));
            }
            self.pipeline.enqueue_request_head()?;
            let phase = if self.transfer_encoding {
                self.chunk_body_bytes = 0;
                Phase::ChunkSize
            } else if let Some(length) = self.content_length {
                if length == 0 {
                    Phase::Headers
                } else {
                    Phase::FixedBody { remaining: length }
                }
            } else {
                Phase::Headers
            };
            self.reset_header_state();
            self.phase = phase;
            return Ok(());
        }
        if self.line[0].is_ascii_whitespace() {
            return Err(wire_error("obsolete HTTP/1 header folding is forbidden"));
        }
        self.header_count = self.header_count.saturating_add(1);
        if self.header_count > self.limits.max_headers {
            return Err(wire_error("HTTP/1 request has too many headers"));
        }
        let (name, value) = parse_header_line(&self.line)?;
        validate_header_field(name, value, self.limits)?;
        if name.eq_ignore_ascii_case(b"content-length") {
            if self.content_length.is_some() {
                return Err(wire_error("duplicate Content-Length is forbidden"));
            }
            self.content_length = Some(parse_content_length(value)?);
        } else if name.eq_ignore_ascii_case(b"transfer-encoding") {
            if self.transfer_encoding {
                return Err(wire_error("duplicate Transfer-Encoding is forbidden"));
            }
            if !trim_ows(value).eq_ignore_ascii_case(b"chunked") {
                return Err(wire_error("only chunked Transfer-Encoding is supported"));
            }
            self.transfer_encoding = true;
        }
        self.line.clear();
        Ok(())
    }

    fn inspect_chunk_size_byte(&mut self, byte: u8) -> io::Result<()> {
        if self.push_line_byte(byte, self.limits.max_chunk_line_bytes)? {
            let size = parse_chunk_size(&self.line)?;
            self.line.clear();
            if size == 0 {
                self.trailer_bytes = 0;
                self.trailer_count = 0;
                self.phase = Phase::Trailers;
            } else {
                self.chunk_body_bytes = self.chunk_body_bytes.saturating_add(size);
                if self.chunk_body_bytes > self.limits.max_body_bytes {
                    return Err(wire_error(
                        "chunked request body exceeds the configured limit",
                    ));
                }
                self.phase = Phase::ChunkData { remaining: size };
            }
        }
        Ok(())
    }

    fn inspect_trailer_byte(&mut self, byte: u8) -> io::Result<()> {
        let line_limit = self
            .limits
            .max_trailer_bytes
            .max(2)
            .min(self.header_field_line_limit());
        if self.push_line_byte(byte, line_limit)? {
            if self.line.is_empty() {
                self.line.clear();
                self.phase = Phase::Headers;
                self.trailer_bytes = 0;
                self.trailer_count = 0;
                return Ok(());
            }
            if self.line[0].is_ascii_whitespace() {
                return Err(wire_error("obsolete HTTP/1 trailer folding is forbidden"));
            }
            self.trailer_count = self.trailer_count.saturating_add(1);
            self.trailer_bytes = self
                .trailer_bytes
                .saturating_add(self.line.len().saturating_add(2));
            if self.trailer_count > self.limits.max_trailers
                || self.trailer_bytes > self.limits.max_trailer_bytes
            {
                return Err(wire_error("HTTP/1 trailers exceed the configured limit"));
            }
            let (name, value) = parse_header_line(&self.line)?;
            validate_header_field(name, value, self.limits)?;
            if is_forbidden_trailer(name) {
                return Err(wire_error("forbidden HTTP/1 trailer field"));
            }
            self.line.clear();
        }
        Ok(())
    }

    fn push_line_byte(&mut self, byte: u8, maximum: usize) -> io::Result<bool> {
        if self.saw_cr {
            self.saw_cr = false;
            if byte == b'\n' {
                return Ok(true);
            }
            return Err(wire_error("HTTP/1 lines must use CRLF"));
        }
        match byte {
            b'\r' => self.saw_cr = true,
            b'\n' => return Err(wire_error("HTTP/1 lines must use CRLF")),
            _ => {
                if self.line.len() >= maximum {
                    return Err(wire_error("HTTP/1 line exceeds the configured limit"));
                }
                self.line.push(byte);
            }
        }
        Ok(false)
    }

    fn reset_header_state(&mut self) {
        self.line.clear();
        self.saw_cr = false;
        self.header_bytes = 0;
        self.header_lines = 0;
        self.header_count = 0;
        self.content_length = None;
        self.transfer_encoding = false;
    }

    fn header_field_line_limit(&self) -> usize {
        self.limits.max_header_bytes.min(
            self.limits
                .max_header_name_bytes
                .saturating_add(1)
                .saturating_add(self.limits.max_header_value_bytes),
        )
    }
}

fn validate_request_line(line: &[u8], limits: WireLimits) -> io::Result<()> {
    let mut parts = line.split(|byte| *byte == b' ');
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if parts.next().is_some() || method.is_empty() || target.is_empty() || version.is_empty() {
        return Err(wire_error("HTTP/1 request line is malformed"));
    }
    if method.len() > limits.max_request_method_bytes
        || !method.iter().all(|byte| is_token_byte(*byte))
    {
        return Err(wire_error("HTTP/1 request method is invalid or too long"));
    }
    if target.len() > limits.max_request_target_bytes
        || !target.iter().all(|byte| (0x21..=0x7e).contains(byte))
    {
        return Err(wire_error("HTTP/1 request target is invalid or too long"));
    }
    if !matches!(version, b"HTTP/1.0" | b"HTTP/1.1") {
        return Err(wire_error("HTTP/1 request version is unsupported"));
    }
    Ok(())
}

fn validate_header_field(name: &[u8], value: &[u8], limits: WireLimits) -> io::Result<()> {
    if name.len() > limits.max_header_name_bytes {
        return Err(wire_error(
            "HTTP/1 header name exceeds the configured limit",
        ));
    }
    if value.len() > limits.max_header_value_bytes {
        return Err(wire_error(
            "HTTP/1 header value exceeds the configured limit",
        ));
    }
    if value
        .iter()
        .any(|byte| (*byte < b' ' && *byte != b'\t') || *byte == 0x7f)
    {
        return Err(wire_error("HTTP/1 header value contains a control byte"));
    }
    Ok(())
}

fn parse_header_line(line: &[u8]) -> io::Result<(&[u8], &[u8])> {
    let Some(separator) = line.iter().position(|byte| *byte == b':') else {
        return Err(wire_error("HTTP/1 header line has no colon"));
    };
    let name = &line[..separator];
    if name.is_empty() || !name.iter().all(|byte| is_token_byte(*byte)) {
        return Err(wire_error("HTTP/1 header name is invalid"));
    }
    Ok((name, &line[separator + 1..]))
}

fn parse_content_length(value: &[u8]) -> io::Result<u64> {
    let value = trim_ows(value);
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
        return Err(wire_error("Content-Length is invalid"));
    }
    let value = std::str::from_utf8(value).map_err(|_| wire_error("Content-Length is invalid"))?;
    value
        .parse()
        .map_err(|_| wire_error("Content-Length is invalid"))
}

fn parse_chunk_size(line: &[u8]) -> io::Result<u64> {
    let (size, extension) = match line.iter().position(|byte| *byte == b';') {
        Some(index) => (&line[..index], Some(&line[index + 1..])),
        None => (line, None),
    };
    if size.is_empty() || size.len() > 16 || !size.iter().all(u8::is_ascii_hexdigit) {
        return Err(wire_error("chunk size is invalid"));
    }
    if extension.is_some_and(|extension| extension.iter().any(|byte| *byte < b' ' || *byte == 0x7f))
    {
        return Err(wire_error("chunk extension contains a control byte"));
    }
    let size = std::str::from_utf8(size).map_err(|_| wire_error("chunk size is invalid"))?;
    u64::from_str_radix(size, 16).map_err(|_| wire_error("chunk size is invalid"))
}

fn trim_ows(mut value: &[u8]) -> &[u8] {
    while value
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[1..];
    }
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[..value.len() - 1];
    }
    value
}

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

fn is_forbidden_trailer(name: &[u8]) -> bool {
    [
        b"content-length".as_slice(),
        b"transfer-encoding".as_slice(),
        b"host".as_slice(),
        b"connection".as_slice(),
        b"trailer".as_slice(),
    ]
    .iter()
    .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
}

fn wire_error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::Ordering, Arc};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{Http1WireGuardStream, Http1WireParser, PipelineState, WireLimits};
    use crate::{
        data_plane::metrics::{DataPlaneMetrics, ProtocolErrorKind},
        metric_dimensions::CanonicalMetricDimensions,
    };

    fn parser(maximum_body_bytes: u64) -> Http1WireParser {
        let limits = WireLimits {
            max_pipeline_depth: 16,
            max_header_bytes: 8 * 1024,
            max_request_line_bytes: 256,
            max_request_method_bytes: 32,
            max_request_target_bytes: 128,
            max_header_name_bytes: 64,
            max_header_value_bytes: 256,
            max_headers: 32,
            max_body_bytes: maximum_body_bytes,
            max_chunk_line_bytes: 128,
            max_trailer_bytes: 1024,
            max_trailers: 8,
        };
        Http1WireParser::new(limits, Arc::new(PipelineState::new(16)))
    }

    #[test]
    fn accepts_fragmented_chunked_body_trailers_and_next_request() {
        let request = b"POST /one HTTP/1.1\r\nHost: test\r\nTransfer-Encoding: chunked\r\nTrailer: X-Checksum\r\n\r\n4;ok=yes\r\ntest\r\n0\r\nX-Checksum: value\r\n\r\nGET /two HTTP/1.1\r\nHost: test\r\n\r\n";
        let mut parser = parser(16);
        for fragment in request.chunks(3) {
            parser.inspect(fragment).expect("valid fragmented framing");
        }
    }

    #[test]
    fn rejects_te_cl_in_both_header_orders_and_duplicate_lengths() {
        for request in [
            b"POST / HTTP/1.1\r\nHost: test\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n0\r\n\r\n".as_slice(),
            b"POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".as_slice(),
            b"POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx".as_slice(),
        ] {
            assert!(parser(16).inspect(request).is_err());
        }
    }

    #[test]
    fn rejects_fixed_and_chunked_bodies_over_limit() {
        parser(3)
            .inspect(b"POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 4\r\n\r\ntest")
            .expect("fixed length is rejected by the global HTTP handler with 413");
        assert!(parser(3)
            .inspect(b"POST / HTTP/1.1\r\nHost: test\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\n\r\n")
            .is_err());
    }

    #[test]
    fn bounds_request_line_and_individual_fields() {
        let limits = WireLimits {
            max_pipeline_depth: 16,
            max_header_bytes: 128,
            max_request_line_bytes: 32,
            max_request_method_bytes: 4,
            max_request_target_bytes: 8,
            max_header_name_bytes: 8,
            max_header_value_bytes: 8,
            max_headers: 8,
            max_body_bytes: 16,
            max_chunk_line_bytes: 16,
            max_trailer_bytes: 32,
            max_trailers: 2,
        };
        let mut valid = Http1WireParser::new(limits, Arc::new(PipelineState::new(16)));
        for fragment in b"GET /ok HTTP/1.1\r\nHost: x\r\n\r\nGET / HTTP/1.0\r\n\r\n".chunks(2) {
            valid.inspect(fragment).expect("bounded pipelined requests");
        }

        for request in [
            b"TRACE / HTTP/1.1\r\n\r\n".as_slice(),
            b"GET /12345678 HTTP/1.1\r\n\r\n".as_slice(),
            b"GET  / HTTP/1.1\r\n\r\n".as_slice(),
            b"GET / HTTP/1.1\r\nX-Too-Long: x\r\n\r\n".as_slice(),
            b"GET / HTTP/1.1\r\nX: 12345678\r\n\r\n".as_slice(),
            b"GET / HTTP/1.1\r\nX: ok\x01bad\r\n\r\n".as_slice(),
        ] {
            assert!(
                Http1WireParser::new(limits, Arc::new(PipelineState::new(16)))
                    .inspect(request)
                    .is_err()
            );
        }
        let trailer_limits = WireLimits {
            max_header_name_bytes: 32,
            ..limits
        };
        assert!(Http1WireParser::new(trailer_limits, Arc::new(PipelineState::new(16)))
            .inspect(
                b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX: 12345678\r\n\r\n"
            )
            .is_err());
    }

    #[test]
    fn bounds_pending_pipeline_heads_and_releases_on_dispatch() {
        let pipeline = Arc::new(PipelineState::new(2));
        let limits = WireLimits {
            max_pipeline_depth: 2,
            max_header_bytes: 8 * 1024,
            max_request_line_bytes: 256,
            max_request_method_bytes: 32,
            max_request_target_bytes: 128,
            max_header_name_bytes: 64,
            max_header_value_bytes: 256,
            max_headers: 32,
            max_body_bytes: 16,
            max_chunk_line_bytes: 128,
            max_trailer_bytes: 1024,
            max_trailers: 8,
        };
        let mut parser = Http1WireParser::new(limits, pipeline.clone());
        parser
            .inspect(
                b"GET /one HTTP/1.1\r\nHost: test\r\n\r\nGET /two HTTP/1.1\r\nHost: test\r\n\r\n",
            )
            .expect("two pending request heads fit the limit");
        assert_eq!(pipeline.pending_heads.load(Ordering::Acquire), 2);

        pipeline.begin_dispatch();
        parser
            .inspect(b"GET /three HTTP/1.1\r\nHost: test\r\n\r\n")
            .expect("dispatch releases one Pipeline slot");
        assert_eq!(pipeline.pending_heads.load(Ordering::Acquire), 2);
        assert!(parser
            .inspect(b"GET /four HTTP/1.1\r\nHost: test\r\n\r\n")
            .is_err());
    }

    #[tokio::test]
    async fn malformed_stream_records_one_fixed_http1_wire_error() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        let (inner, mut peer) = tokio::io::duplex(256);
        let mut stream = Http1WireGuardStream {
            inner,
            parser: Some(parser(16)),
            metrics: Some(metrics.clone()),
            wire_error_recorded: false,
        };
        peer.write_all(b"GET  / HTTP/1.1\r\n\r\n")
            .await
            .expect("write malformed request");
        peer.shutdown().await.expect("close peer");

        let mut output = Vec::new();
        let error = stream
            .read_to_end(&mut output)
            .await
            .expect_err("malformed HTTP/1 request must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            metrics.protocol_error_count(ProtocolErrorKind::Http1Wire),
            1
        );
    }
}
