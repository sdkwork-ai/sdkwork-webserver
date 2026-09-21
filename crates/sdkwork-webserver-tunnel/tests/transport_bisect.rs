//! Bisect probe: raw QUIC client against a spawned TunnelGateway.

use std::time::Duration;

use sdkwork_webserver_tunnel::gateway::{TunnelGateway, TunnelGatewayOptions};
use sdkwork_webserver_tunnel::security::TokenAuthenticator;
use sdkwork_webserver_tunnel_core::TunnelConfig;
use sdkwork_webserver_tunnel_transport::{
    tls, QuicClientTransport, RemoteEndpoint, TunnelClientTransport,
};

#[tokio::test]
async fn raw_client_connects_to_gateway() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("debug"))
        .with_test_writer()
        .try_init();
    let material = tls::generate_self_signed(&["127.0.0.1".to_owned()]).expect("material");
    let mut options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        material.cert_pem.clone().into_bytes(),
        material.key_pem.clone().into_bytes(),
    );
    options.authenticator = TokenAuthenticator::new(vec!["t".to_owned()]);
    let gateway = TunnelGateway::spawn(options, None).await.expect("gateway");
    let port = gateway.quic_port();

    let client = QuicClientTransport::new(
        &tls::ClientTlsOptions {
            pinned_server_sha256: Some(material.sha256.clone()),
            ..tls::ClientTlsOptions::default()
        },
        sdkwork_webserver_tunnel_transport::TransportOptions::default(),
    )
    .expect("client");
    let connection = tokio::time::timeout(
        Duration::from_secs(5),
        client.connect(
            &RemoteEndpoint::new("127.0.0.1", port),
            "sdkwork-tunnel-gateway",
        ),
    )
    .await
    .expect("connect within 5s")
    .expect("connect ok");

    // Open the control stream exactly like the agent does and write a Hello
    // frame; the gateway control loop must log its outcome.
    let mut control = connection.open_stream().await.expect("control stream");
    let hello = sdkwork_webserver_tunnel_protocol::ControlMessage::Hello(
        sdkwork_webserver_tunnel_protocol::Hello {
            protocol_versions: vec![sdkwork_webserver_tunnel_protocol::ProtocolVersion::v1()],
            device_id: "dev_probe".to_owned(),
            device_name: "probe".to_owned(),
            platform: "linux".to_owned(),
        },
    );
    let frame = hello.to_frame().expect("frame");
    let authenticate = sdkwork_webserver_tunnel_protocol::ControlMessage::Authenticate(
        sdkwork_webserver_tunnel_protocol::Authenticate {
            token: "t".to_owned(),
        },
    );
    let auth_frame = authenticate.to_frame().expect("auth frame");
    use tokio::io::AsyncWriteExt;
    // Exactly the agent's pattern: two write+flush pairs back-to-back.
    control.write_all(&frame).await.expect("hello written");
    control.flush().await.expect("hello flushed");
    control.write_all(&auth_frame).await.expect("auth written");
    control.flush().await.expect("auth flushed");
    // Give the gateway control loop a moment to process.
    tokio::time::sleep(Duration::from_millis(300)).await;
    gateway.shutdown().await;
}
