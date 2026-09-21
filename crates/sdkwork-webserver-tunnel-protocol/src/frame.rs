//! Length-prefixed frame codec shared by the control plane and data-stream
//! headers.
//!
//! Frame layout: `u32` big-endian payload length, then the payload bytes.
//! Decoding enforces [`MAX_FRAME_LENGTH`] before allocating, so a hostile
//! peer cannot force an oversized buffer (PRD §108).

use std::convert::TryInto;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use sdkwork_webserver_tunnel_core::TunnelError;

/// Bytes occupied by the frame length prefix.
pub const FRAME_LENGTH_BYTES: usize = 4;

/// Hard wire-level ceiling for any single frame. Control messages and data
/// stream headers are both capped well below this by their own decoders
/// (`maxControlMessageBytes`); raw data-plane payloads are not framed at
/// all and never pass through this codec.
pub const MAX_FRAME_LENGTH: u32 = 1024 * 1024;

/// Appends one length-prefixed frame to `buffer`.
pub fn encode_frame(payload: &[u8], buffer: &mut BytesMut) -> Result<(), TunnelError> {
    let length = u32::try_from(payload.len()).map_err(|_| {
        TunnelError::Protocol(format!(
            "frame payload of {} bytes does not fit the u32 length prefix",
            payload.len()
        ))
    })?;
    if length > MAX_FRAME_LENGTH {
        return Err(TunnelError::Protocol(format!(
            "frame payload of {length} bytes exceeds the {MAX_FRAME_LENGTH} byte wire ceiling"
        )));
    }
    buffer.reserve(FRAME_LENGTH_BYTES + payload.len());
    buffer.put_u32(length);
    buffer.put_slice(payload);
    Ok(())
}

/// Renders one length-prefixed frame as a fresh byte vector.
pub fn frame_bytes(payload: &[u8]) -> Result<Bytes, TunnelError> {
    let mut buffer = BytesMut::with_capacity(FRAME_LENGTH_BYTES + payload.len());
    encode_frame(payload, &mut buffer)?;
    Ok(buffer.freeze())
}

/// Parses one frame from the front of `buffer`, returning the payload and
/// the number of bytes consumed. Returns `Ok(None)` when the buffer does not
/// yet hold a complete frame.
pub fn decode_frame(buffer: &[u8]) -> Result<Option<(Bytes, usize)>, TunnelError> {
    if buffer.len() < FRAME_LENGTH_BYTES {
        return Ok(None);
    }
    let length = u32::from_be_bytes(
        buffer[..FRAME_LENGTH_BYTES]
            .try_into()
            .expect("slice length checked against FRAME_LENGTH_BYTES immediately above"),
    ) as usize;
    if length as u32 > MAX_FRAME_LENGTH {
        return Err(TunnelError::Protocol(format!(
            "declared frame length {length} exceeds the {MAX_FRAME_LENGTH} byte wire ceiling"
        )));
    }
    let total = FRAME_LENGTH_BYTES + length;
    if buffer.len() < total {
        return Ok(None);
    }
    let payload = Bytes::copy_from_slice(&buffer[FRAME_LENGTH_BYTES..total]);
    Ok(Some((payload, total)))
}

/// Reads one complete frame payload from an async reader, enforcing the wire
/// ceiling before any payload allocation. Accepts unsized readers so
/// `dyn TunnelStream` trait objects work directly.
pub async fn read_frame<R>(reader: &mut R, scratch: &mut BytesMut) -> Result<Bytes, TunnelError>
where
    R: AsyncRead + Unpin + Send + ?Sized,
{
    loop {
        if let Some((payload, consumed)) = decode_frame(scratch)? {
            scratch.advance(consumed);
            return Ok(payload);
        }
        if scratch.len() > (MAX_FRAME_LENGTH as usize + FRAME_LENGTH_BYTES) {
            return Err(TunnelError::Protocol(format!(
                "buffered frame exceeded the {MAX_FRAME_LENGTH} byte wire ceiling"
            )));
        }
        // Read into initialized memory rather than `read_buf`'s uninit
        // `BufMut` path: control frames are small, and an initialized read
        // keeps the poll behavior identical across every `AsyncRead`
        // implementation behind the `dyn TunnelStream` boundary.
        let mut chunk = [0_u8; 4096];
        let read = reader
            .read(&mut chunk)
            .await
            .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
        if read == 0 {
            return Err(TunnelError::ConnectionClosed);
        }
        scratch.extend_from_slice(&chunk[..read]);
    }
}

/// Reads exactly one frame: the 4-byte length prefix, then exactly the
/// declared payload. Never over-reads, so bytes that follow the frame on
/// the wire stay untouched — required for the data-plane stream header,
/// where raw application bytes start immediately after the frame.
pub async fn read_exact_frame<R>(reader: &mut R) -> Result<Bytes, TunnelError>
where
    R: AsyncRead + Unpin + Send + ?Sized,
{
    let mut prefix = [0_u8; FRAME_LENGTH_BYTES];
    reader
        .read_exact(&mut prefix)
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length as u32 > MAX_FRAME_LENGTH {
        return Err(TunnelError::Protocol(format!(
            "declared frame length {length} exceeds the {MAX_FRAME_LENGTH} byte wire ceiling"
        )));
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    Ok(Bytes::from(payload))
}

/// Writes one length-prefixed frame and flushes. Accepts unsized writers.
pub async fn write_frame<W>(writer: &mut W, payload: &[u8]) -> Result<(), TunnelError>
where
    W: AsyncWrite + Unpin + Send + ?Sized,
{
    let frame = frame_bytes(payload)?;
    writer
        .write_all(&frame)
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trips() {
        let payload = b"hello tunnel";
        let frame = frame_bytes(payload).expect("encode");
        assert_eq!(frame.len(), FRAME_LENGTH_BYTES + payload.len());
        let (decoded, consumed) = decode_frame(&frame)
            .expect("parse")
            .expect("complete frame");
        assert_eq!(consumed, frame.len());
        assert_eq!(decoded.as_ref(), payload);
    }

    #[test]
    fn partial_frames_are_pending() {
        let frame = frame_bytes(b"abcdef").expect("encode");
        // One byte short: the frame is pending, not an error.
        assert!(decode_frame(&frame[..frame.len() - 1])
            .expect("parse")
            .is_none());
        let (payload, consumed) = decode_frame(&frame)
            .expect("parse")
            .expect("complete frame");
        assert_eq!(payload.as_ref(), b"abcdef");
        assert_eq!(consumed, frame.len());
        assert!(decode_frame(&frame[..2]).expect("parse").is_none());
        assert!(decode_frame(&[]).expect("parse").is_none());
    }

    #[test]
    fn oversized_declared_length_is_rejected() {
        let mut hostile = Vec::new();
        hostile.extend_from_slice(&u32::MAX.to_be_bytes());
        let error = decode_frame(&hostile).expect_err("oversized frames are rejected");
        assert!(error.to_string().contains("exceeds"));
    }

    #[test]
    fn encode_rejects_oversized_payload() {
        let oversized = vec![0_u8; (MAX_FRAME_LENGTH + 1) as usize];
        assert!(frame_bytes(&oversized).is_err());
    }

    #[tokio::test]
    async fn read_write_round_trip_over_duplex() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let writer = tokio::spawn(async move {
            write_frame(&mut client, b"ping").await.expect("write");
        });
        let mut scratch = BytesMut::new();
        let payload = read_frame(&mut server, &mut scratch).await.expect("read");
        assert_eq!(payload.as_ref(), b"ping");
        writer.await.expect("writer task");
    }
}
