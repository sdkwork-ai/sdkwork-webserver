use std::{
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum_server::accept::Accept;
use sdkwork_webserver_core::WebServerLimits;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::http1_wire::NegotiatedHttpProtocol;
use super::metrics::{DataPlaneMetrics, ProtocolErrorKind};

const CLIENT_PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const FRAME_HEADER_BYTES: usize = 9;
const READ_SCRATCH_BYTES: usize = 8 * 1024;
const FRAME_HEADERS: u8 = 0x1;
const FRAME_RST_STREAM: u8 = 0x3;
const FRAME_CONTINUATION: u8 = 0x9;
const FLAG_END_HEADERS: u8 = 0x4;

#[derive(Clone, Copy)]
struct Http2WireLimits {
    window: Duration,
    max_frames: usize,
    max_new_streams: usize,
    max_reset_frames: usize,
    max_continuation_frames: usize,
    max_encoded_header_block_bytes: usize,
    max_frame_bytes: usize,
}

impl From<&WebServerLimits> for Http2WireLimits {
    fn from(limits: &WebServerLimits) -> Self {
        Self {
            window: Duration::from_millis(limits.http2_abuse_window_ms),
            max_frames: limits.http2_max_frames_per_window,
            max_new_streams: limits.http2_max_new_streams_per_window,
            max_reset_frames: limits.http2_max_reset_frames_per_window,
            max_continuation_frames: limits.http2_max_continuation_frames,
            max_encoded_header_block_bytes: limits.http2_max_encoded_header_block_bytes,
            max_frame_bytes: limits.http2_max_frame_bytes as usize,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Http2WireGuardAcceptor<A> {
    inner: A,
    limits: Http2WireLimits,
    metrics: Option<Arc<DataPlaneMetrics>>,
}

impl<A> Http2WireGuardAcceptor<A> {
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

impl<A, I, S> Accept<I, S> for Http2WireGuardAcceptor<A>
where
    A: Accept<I, S> + Clone + Send + Sync + 'static,
    A::Future: Send + 'static,
    A::Stream: NegotiatedHttpProtocol + AsyncRead + AsyncWrite + Send + Unpin + 'static,
    A::Service: Send + 'static,
    I: Send + 'static,
    S: Send + 'static,
{
    type Stream = Http2WireGuardStream<A::Stream>;
    type Service = A::Service;
    type Future =
        Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send + 'static>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.inner.clone();
        let limits = self.limits;
        let metrics = self.metrics.clone();
        Box::pin(async move {
            let (stream, service) = inner.accept(stream, service).await?;
            let parser = (!stream.is_http1()).then(|| Http2WireParser::new(limits));
            Ok((
                Http2WireGuardStream {
                    inner: stream,
                    parser,
                    metrics,
                    wire_error_recorded: false,
                },
                service,
            ))
        })
    }
}

pub(crate) struct Http2WireGuardStream<I> {
    inner: I,
    parser: Option<Http2WireParser>,
    metrics: Option<Arc<DataPlaneMetrics>>,
    wire_error_recorded: bool,
}

impl<I: AsyncRead + Unpin> AsyncRead for Http2WireGuardStream<I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.parser.is_none() {
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
                            metrics.record_protocol_error(ProtocolErrorKind::Http2Wire);
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

impl<I: AsyncWrite + Unpin> AsyncWrite for Http2WireGuardStream<I> {
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

impl<I: NegotiatedHttpProtocol> NegotiatedHttpProtocol for Http2WireGuardStream<I> {
    fn is_http1(&self) -> bool {
        self.inner.is_http1()
    }
}

#[derive(Clone, Copy)]
struct HeaderBlock {
    stream_id: u32,
    encoded_bytes: usize,
    continuation_frames: usize,
}

struct Http2WireParser {
    limits: Http2WireLimits,
    preface_position: usize,
    frame_header: [u8; FRAME_HEADER_BYTES],
    frame_header_position: usize,
    payload_remaining: usize,
    continuation: Option<HeaderBlock>,
    highest_client_stream_id: u32,
    window_started: Instant,
    frames_in_window: usize,
    new_streams_in_window: usize,
    resets_in_window: usize,
}

impl Http2WireParser {
    fn new(limits: Http2WireLimits) -> Self {
        Self {
            limits,
            preface_position: 0,
            frame_header: [0; FRAME_HEADER_BYTES],
            frame_header_position: 0,
            payload_remaining: 0,
            continuation: None,
            highest_client_stream_id: 0,
            window_started: Instant::now(),
            frames_in_window: 0,
            new_streams_in_window: 0,
            resets_in_window: 0,
        }
    }

    fn inspect(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut position = 0;
        while position < bytes.len() {
            if self.preface_position < CLIENT_PREFACE.len() {
                let remaining = CLIENT_PREFACE.len() - self.preface_position;
                let consumed = remaining.min(bytes.len() - position);
                let expected =
                    &CLIENT_PREFACE[self.preface_position..self.preface_position + consumed];
                if &bytes[position..position + consumed] != expected {
                    return Err(wire_error("HTTP/2 client preface is invalid"));
                }
                self.preface_position += consumed;
                position += consumed;
                continue;
            }
            if self.payload_remaining > 0 {
                let consumed = self.payload_remaining.min(bytes.len() - position);
                self.payload_remaining -= consumed;
                position += consumed;
                continue;
            }

            let needed = FRAME_HEADER_BYTES - self.frame_header_position;
            let consumed = needed.min(bytes.len() - position);
            self.frame_header[self.frame_header_position..self.frame_header_position + consumed]
                .copy_from_slice(&bytes[position..position + consumed]);
            self.frame_header_position += consumed;
            position += consumed;
            if self.frame_header_position == FRAME_HEADER_BYTES {
                self.payload_remaining = self.inspect_frame_header()?;
                self.frame_header_position = 0;
            }
        }
        Ok(())
    }

    fn inspect_frame_header(&mut self) -> io::Result<usize> {
        let length = ((self.frame_header[0] as usize) << 16)
            | ((self.frame_header[1] as usize) << 8)
            | self.frame_header[2] as usize;
        if length > self.limits.max_frame_bytes {
            return Err(wire_error("HTTP/2 frame exceeds the configured limit"));
        }
        if self.frame_header[5] & 0x80 != 0 {
            return Err(wire_error("HTTP/2 reserved stream-id bit is set"));
        }
        let frame_type = self.frame_header[3];
        let flags = self.frame_header[4];
        let stream_id = u32::from_be_bytes([
            self.frame_header[5],
            self.frame_header[6],
            self.frame_header[7],
            self.frame_header[8],
        ]);

        self.record_frame()?;
        if self.continuation.is_some() && frame_type != FRAME_CONTINUATION {
            return Err(wire_error("HTTP/2 Header Block is interleaved"));
        }
        match frame_type {
            FRAME_HEADERS => self.start_header_block(stream_id, flags, length)?,
            FRAME_CONTINUATION => self.continue_header_block(stream_id, flags, length)?,
            FRAME_RST_STREAM => {
                if stream_id == 0 || length != 4 {
                    return Err(wire_error("HTTP/2 RST_STREAM frame is invalid"));
                }
                self.record_reset()?;
            }
            _ => {}
        }
        Ok(length)
    }

    fn start_header_block(&mut self, stream_id: u32, flags: u8, length: usize) -> io::Result<()> {
        if stream_id == 0 || stream_id.is_multiple_of(2) {
            return Err(wire_error("HTTP/2 client HEADERS stream id is invalid"));
        }
        if stream_id > self.highest_client_stream_id {
            self.highest_client_stream_id = stream_id;
            self.record_new_stream()?;
        }
        if length > self.limits.max_encoded_header_block_bytes {
            return Err(wire_error(
                "HTTP/2 encoded Header Block exceeds the configured limit",
            ));
        }
        if flags & FLAG_END_HEADERS == 0 {
            self.continuation = Some(HeaderBlock {
                stream_id,
                encoded_bytes: length,
                continuation_frames: 0,
            });
        }
        Ok(())
    }

    fn continue_header_block(
        &mut self,
        stream_id: u32,
        flags: u8,
        length: usize,
    ) -> io::Result<()> {
        let Some(mut block) = self.continuation else {
            return Err(wire_error("unexpected HTTP/2 CONTINUATION frame"));
        };
        if block.stream_id != stream_id {
            return Err(wire_error("HTTP/2 CONTINUATION stream id changed"));
        }
        block.continuation_frames = block.continuation_frames.saturating_add(1);
        block.encoded_bytes = block.encoded_bytes.saturating_add(length);
        if block.continuation_frames > self.limits.max_continuation_frames {
            return Err(wire_error(
                "HTTP/2 Header Block has too many CONTINUATION frames",
            ));
        }
        if block.encoded_bytes > self.limits.max_encoded_header_block_bytes {
            return Err(wire_error(
                "HTTP/2 encoded Header Block exceeds the configured limit",
            ));
        }
        self.continuation = if flags & FLAG_END_HEADERS == 0 {
            Some(block)
        } else {
            None
        };
        Ok(())
    }

    fn record_frame(&mut self) -> io::Result<()> {
        self.refresh_window();
        self.frames_in_window = self.frames_in_window.saturating_add(1);
        if self.frames_in_window > self.limits.max_frames {
            return Err(wire_error("HTTP/2 frame rate exceeds the configured limit"));
        }
        Ok(())
    }

    fn record_new_stream(&mut self) -> io::Result<()> {
        self.new_streams_in_window = self.new_streams_in_window.saturating_add(1);
        if self.new_streams_in_window > self.limits.max_new_streams {
            return Err(wire_error(
                "HTTP/2 new-stream rate exceeds the configured limit",
            ));
        }
        Ok(())
    }

    fn record_reset(&mut self) -> io::Result<()> {
        self.resets_in_window = self.resets_in_window.saturating_add(1);
        if self.resets_in_window > self.limits.max_reset_frames {
            return Err(wire_error("HTTP/2 reset rate exceeds the configured limit"));
        }
        Ok(())
    }

    fn refresh_window(&mut self) {
        if self.window_started.elapsed() >= self.limits.window {
            self.window_started = Instant::now();
            self.frames_in_window = 0;
            self.new_streams_in_window = 0;
            self.resets_in_window = 0;
        }
    }
}

fn wire_error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{Http2WireGuardStream, Http2WireLimits, Http2WireParser, CLIENT_PREFACE};
    use crate::{
        data_plane::metrics::{DataPlaneMetrics, ProtocolErrorKind},
        metric_dimensions::CanonicalMetricDimensions,
    };

    fn limits() -> Http2WireLimits {
        Http2WireLimits {
            window: Duration::from_secs(60),
            max_frames: 8,
            max_new_streams: 3,
            max_reset_frames: 2,
            max_continuation_frames: 2,
            max_encoded_header_block_bytes: 16,
            max_frame_bytes: 16_384,
        }
    }

    fn frame(frame_type: u8, flags: u8, stream_id: u32, length: usize) -> Vec<u8> {
        let mut frame = vec![0_u8; 9 + length];
        frame[0] = ((length >> 16) & 0xff) as u8;
        frame[1] = ((length >> 8) & 0xff) as u8;
        frame[2] = (length & 0xff) as u8;
        frame[3] = frame_type;
        frame[4] = flags;
        frame[5..9].copy_from_slice(&stream_id.to_be_bytes());
        frame
    }

    #[test]
    fn accepts_fragmented_preface_frames_and_header_blocks() {
        let mut bytes = CLIENT_PREFACE.to_vec();
        bytes.extend(frame(0x4, 0, 0, 0));
        bytes.extend(frame(0x1, 0, 1, 4));
        bytes.extend(frame(0x9, 0x4, 1, 4));
        bytes.extend(frame(0x3, 0, 1, 4));
        let mut parser = Http2WireParser::new(limits());
        for fragment in bytes.chunks(2) {
            parser
                .inspect(fragment)
                .expect("valid fragmented HTTP/2 wire");
        }
    }

    #[test]
    fn rejects_frame_stream_reset_and_continuation_abuse() {
        let mut rate_limits = limits();
        rate_limits.max_frames = 2;
        let mut frames = CLIENT_PREFACE.to_vec();
        frames.extend(frame(0x4, 0, 0, 0));
        frames.extend(frame(0x8, 0, 0, 4));
        frames.extend(frame(0x6, 0, 0, 8));
        assert!(Http2WireParser::new(rate_limits).inspect(&frames).is_err());

        let mut stream_limits = limits();
        stream_limits.max_new_streams = 1;
        let mut streams = CLIENT_PREFACE.to_vec();
        streams.extend(frame(0x1, 0x4, 1, 1));
        streams.extend(frame(0x1, 0x4, 3, 1));
        assert!(Http2WireParser::new(stream_limits)
            .inspect(&streams)
            .is_err());

        let mut reset_limits = limits();
        reset_limits.max_reset_frames = 1;
        let mut resets = CLIENT_PREFACE.to_vec();
        resets.extend(frame(0x3, 0, 1, 4));
        resets.extend(frame(0x3, 0, 3, 4));
        assert!(Http2WireParser::new(reset_limits).inspect(&resets).is_err());

        let mut continuation_limits = limits();
        continuation_limits.max_continuation_frames = 1;
        let mut continuations = CLIENT_PREFACE.to_vec();
        continuations.extend(frame(0x1, 0, 1, 4));
        continuations.extend(frame(0x9, 0, 1, 4));
        continuations.extend(frame(0x9, 0x4, 1, 4));
        assert!(Http2WireParser::new(continuation_limits)
            .inspect(&continuations)
            .is_err());
    }

    #[test]
    fn rejects_invalid_preface_interleaving_and_cross_stream_continuation() {
        let mut invalid_preface = CLIENT_PREFACE.to_vec();
        invalid_preface[0] = b'X';
        assert!(Http2WireParser::new(limits())
            .inspect(&invalid_preface)
            .is_err());

        let mut interleaved = CLIENT_PREFACE.to_vec();
        interleaved.extend(frame(0x1, 0, 1, 4));
        interleaved.extend(frame(0x0, 0, 1, 1));
        assert!(Http2WireParser::new(limits())
            .inspect(&interleaved)
            .is_err());

        let mut cross_stream = CLIENT_PREFACE.to_vec();
        cross_stream.extend(frame(0x1, 0, 1, 4));
        cross_stream.extend(frame(0x9, 0x4, 3, 4));
        assert!(Http2WireParser::new(limits())
            .inspect(&cross_stream)
            .is_err());
    }

    #[tokio::test]
    async fn malformed_stream_records_one_fixed_http2_wire_error() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        let (inner, mut peer) = tokio::io::duplex(64);
        let mut stream = Http2WireGuardStream {
            inner,
            parser: Some(Http2WireParser::new(limits())),
            metrics: Some(metrics.clone()),
            wire_error_recorded: false,
        };
        peer.write_all(b"XRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .expect("write malformed preface");
        peer.shutdown().await.expect("close peer");

        let mut output = Vec::new();
        let error = stream
            .read_to_end(&mut output)
            .await
            .expect_err("malformed HTTP/2 preface must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            metrics.protocol_error_count(ProtocolErrorKind::Http2Wire),
            1
        );
    }
}
