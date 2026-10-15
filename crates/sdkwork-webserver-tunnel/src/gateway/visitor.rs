//! Visitor streams on the gateway (FRP `stcp`/`sudp` visitor parity).
//!
//! A visitor client authenticates like an agent, then opens one stream per
//! local connection it accepts. Each stream starts with a
//! [`VisitorStreamHeader`] naming a secret route and presenting its shared
//! key; the gateway authorizes the request, opens a second stream to the
//! provider's agent, and bridges the two byte-for-byte.
//!
//! Nothing here is public: there is no host or port to match, and a visitor
//! that does not hold the key learns nothing beyond "refused".

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::copy_bidirectional;
use tokio::sync::watch;

use sdkwork_webserver_tunnel_core::TunnelError;
use sdkwork_webserver_tunnel_protocol::{frame, VisitorAddress, VisitorStreamHeader};
use sdkwork_webserver_tunnel_transport::{TunnelConnection, TunnelStream};

use super::{dispatch, GatewayShared};

/// Accepts visitor streams for one authenticated session until it ends.
///
/// Runs for every session (agents simply never open streams toward the
/// gateway), so the gateway can tell a visitor request apart by its header
/// rather than by a role negotiated at handshake time.
pub(crate) async fn visitor_accept_loop(
    shared: Arc<GatewayShared>,
    connection: Arc<dyn TunnelConnection>,
    peer: SocketAddr,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => return,
            accepted = connection.accept_stream() => {
                match accepted {
                    Ok(stream) => {
                        let shared = shared.clone();
                        tokio::spawn(async move {
                            handle_visitor_stream(shared, stream, peer).await;
                        });
                    }
                    Err(error) => {
                        tracing::debug!(%peer, %error, "visitor stream accept ended");
                        return;
                    }
                }
            }
        }
    }
}

/// Authorizes and bridges one visitor stream.
async fn handle_visitor_stream(
    shared: Arc<GatewayShared>,
    mut visitor_stream: Box<dyn TunnelStream>,
    peer: SocketAddr,
) {
    let header = match read_visitor_header(&shared, &mut visitor_stream).await {
        Ok(header) => header,
        Err(error) => {
            // An agent's session never opens a stream here, and a malformed
            // header is not worth a warning per occurrence.
            tracing::debug!(%peer, %error, "stream did not carry a visitor header");
            return;
        }
    };

    let provider = dispatch::connect_secret_stream(
        &shared,
        &header.name,
        &header.secret_key,
        &header.protocol,
        Some(VisitorAddress::new(peer.ip(), peer.port())),
    )
    .await;

    let mut provider = match provider {
        Ok(provider) => provider,
        Err(error) => {
            // The refusal reason is logged for the operator, never sent back
            // in a distinguishable form: a visitor without the key must not
            // learn whether the name exists.
            tracing::info!(
                %peer,
                name = %header.name,
                stream = header.stream_id,
                %error,
                "visitor request refused"
            );
            match error {
                TunnelError::NoHealthyReplica => shared.metrics.record_no_healthy_replica(),
                _ => {}
            }
            return;
        }
    };

    shared.metrics.record_visitor_stream_open();
    tracing::debug!(
        %peer,
        name = %header.name,
        stream = header.stream_id,
        "visitor stream bridged"
    );
    match copy_bidirectional(&mut visitor_stream, &mut provider).await {
        Ok((inbound, outbound)) => {
            shared.metrics.add_bytes_in(inbound);
            shared.metrics.add_bytes_out(outbound);
        }
        Err(error) => {
            // A visitor that hangs up mid-transfer is routine, not an error.
            tracing::debug!(%peer, %error, "visitor stream ended");
        }
    }
    shared.metrics.record_visitor_stream_close();
}

/// Reads and decodes the stream's first frame, bounded by the stream-open
/// budget so a silent visitor cannot pin a task forever.
async fn read_visitor_header(
    shared: &Arc<GatewayShared>,
    stream: &mut Box<dyn TunnelStream>,
) -> Result<VisitorStreamHeader, TunnelError> {
    let budget = shared.timeouts.stream_open_duration();
    let read = tokio::time::timeout(budget, frame::read_exact_frame(&mut *stream)).await;
    let payload = match read {
        Ok(Ok(payload)) => payload,
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err(TunnelError::Timeout("visitor stream header")),
    };
    VisitorStreamHeader::decode(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_visitor_header_round_trips_through_a_frame() {
        let header = VisitorStreamHeader {
            name: "ssh-box".to_owned(),
            secret_key: "s3cret-key".to_owned(),
            protocol: "stcp".to_owned(),
            stream_id: 7,
        };
        let frame = header.to_frame().expect("frame");
        let (payload, consumed) = frame::decode_frame(&frame)
            .expect("decode")
            .expect("complete");
        assert_eq!(consumed, frame.len());
        assert_eq!(
            VisitorStreamHeader::decode(&payload).expect("header"),
            header
        );
    }

    #[test]
    fn a_malformed_header_is_a_protocol_error() {
        assert!(VisitorStreamHeader::decode(b"{}").is_err());
        assert!(VisitorStreamHeader::decode(b"not json").is_err());
    }
}
