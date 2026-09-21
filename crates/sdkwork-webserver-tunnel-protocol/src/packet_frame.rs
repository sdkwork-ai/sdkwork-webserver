//! Datagram framing for UDP relays over QUIC streams (FRP UDP proxy
//! parity).
//!
//! QUIC streams are byte streams; UDP payloads are datagrams with message
//! boundaries that must survive the relay. Each packet is framed as a
//! 2-byte big-endian length prefix plus the payload (identical shape to
//! nginx stream-style datagram relays). The maximum payload equals the
//! maximum UDP/IPv4 payload, so no legitimate datagram is ever split.

use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio::io::AsyncReadExt;

use sdkwork_webserver_tunnel_core::TunnelError;

/// Bytes occupied by the packet length prefix.
pub const PACKET_LENGTH_BYTES: usize = 2;

/// Maximum UDP payload this relay frames: the maximum UDP/IPv4 payload
/// (65,507 bytes). Anything larger cannot have come from a real datagram.
pub const MAX_PACKET_PAYLOAD: usize = 65_507;

/// Appends one framed packet to `buffer`.
pub fn encode_packet(payload: &[u8], buffer: &mut BytesMut) -> Result<(), TunnelError> {
    let length = u16::try_from(payload.len()).map_err(|_| {
        TunnelError::Protocol(format!(
            "UDP payload of {} bytes exceeds the {} byte datagram ceiling",
            payload.len(),
            MAX_PACKET_PAYLOAD
        ))
    })?;
    buffer.reserve(PACKET_LENGTH_BYTES + payload.len());
    buffer.put_u16(length);
    buffer.put_slice(payload);
    Ok(())
}

/// Parses one packet from the front of `buffer`, returning the payload and
/// the consumed length. `Ok(None)` when the buffer holds an incomplete
/// packet.
pub fn decode_packet(buffer: &[u8]) -> Result<Option<(Bytes, usize)>, TunnelError> {
    if buffer.len() < PACKET_LENGTH_BYTES {
        return Ok(None);
    }
    let length = u16::from_be_bytes([buffer[0], buffer[1]]) as usize;
    if length > MAX_PACKET_PAYLOAD {
        return Err(TunnelError::Protocol(format!(
            "declared packet length {length} exceeds the {MAX_PACKET_PAYLOAD} byte datagram ceiling"
        )));
    }
    let total = PACKET_LENGTH_BYTES + length;
    if buffer.len() < total {
        return Ok(None);
    }
    let payload = Bytes::copy_from_slice(&buffer[PACKET_LENGTH_BYTES..total]);
    Ok(Some((payload, total)))
}

/// Reads one complete framed packet, discarding nothing: trailing bytes
/// after the first packet remain in `scratch` for the next call.
pub async fn read_packet<R>(reader: &mut R, scratch: &mut BytesMut) -> Result<Bytes, TunnelError>
where
    R: tokio::io::AsyncRead + Unpin + Send + ?Sized,
{
    loop {
        if let Some((payload, consumed)) = decode_packet(scratch)? {
            scratch.advance(consumed);
            return Ok(payload);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_round_trips() {
        let mut buffer = BytesMut::new();
        encode_packet(b"datagram", &mut buffer).expect("encode");
        let (payload, consumed) = decode_packet(&buffer)
            .expect("parse")
            .expect("complete");
        assert_eq!(consumed, buffer.len());
        assert_eq!(payload.as_ref(), b"datagram");
    }

    #[test]
    fn two_packets_decode_sequentially() {
        let mut buffer = BytesMut::new();
        encode_packet(b"one", &mut buffer).expect("one");
        encode_packet(b"twotwo", &mut buffer).expect("two");
        let (first, consumed) = decode_packet(&buffer).expect("parse").expect("first");
        assert_eq!(first.as_ref(), b"one");
        buffer.advance(consumed);
        let (second, consumed) = decode_packet(&buffer).expect("parse").expect("second");
        assert_eq!(second.as_ref(), b"twotwo");
        buffer.advance(consumed);
        assert!(buffer.is_empty());
    }

    #[test]
    fn incomplete_packet_is_pending() {
        let frame = frame_bytes(b"payload");
        let partial = &frame[..frame.len() - 1];
        assert!(decode_packet(partial).expect("parse").is_none());
        assert!(decode_packet(&[]).expect("parse").is_none());
    }

    #[test]
    fn oversized_declared_length_is_rejected() {
        let hostile = [0xFF, 0xFF];
        let error = decode_packet(&hostile).expect_err("oversized must fail");
        assert!(error.to_string().contains("datagram ceiling"));
    }

    fn frame_bytes(payload: &[u8]) -> Bytes {
        let mut buffer = BytesMut::new();
        encode_packet(payload, &mut buffer).expect("encode");
        buffer.freeze()
    }
}
