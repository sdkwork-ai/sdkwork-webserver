//! CLI command implementations (PRD §37, §83, §85, §122).
//!
//! Kept in the library for testability; the binary owns process startup
//! only (RUST_CODE_SPEC.md §1). All control operations go through
//! [`crate::service::TunnelService`] or the gateway's operations REST
//! surface — the CLI never re-implements routes, sessions, or devices
//! (PRD §81).

use std::time::Duration;

use sdkwork_webserver_tunnel_core::{
    Device, DeviceId, DevicePlatform, RoutePolicy, TunnelConfig, TunnelGatewayConfig,
    TunnelProtocolKind, TunnelRouteTemplate,
};
use sdkwork_webserver_tunnel_transport::{tls, RemoteEndpoint};

use crate::agent::{AgentEvent, AgentRuntime, AgentRuntimeOptions};
use crate::gateway::{TunnelGateway, TunnelGatewayOptions};
use crate::metrics::TunnelMetrics;
use crate::security::TokenAuthenticator;

/// Gateway command: run a public tunnel edge (PRD §6.1).
pub fn gateway_command(arguments: &[String]) -> Result<(), String> {
    let mut listen = std::env::var("SDKWORK_TUNNEL_LISTEN").unwrap_or_else(|_| {
        format!(
            "0.0.0.0:{}",
            sdkwork_webserver_tunnel_core::DEFAULT_TUNNEL_PORT
        )
    });
    let mut domain_suffixes: Vec<String> = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--listen" => {
                index += 1;
                listen = arguments
                    .get(index)
                    .ok_or("--listen requires a value")?
                    .clone();
            }
            "--domain-suffix" => {
                index += 1;
                let suffix = arguments
                    .get(index)
                    .ok_or("--domain-suffix requires a value")?;
                domain_suffixes.push(suffix.clone());
            }
            other => return Err(format!("unknown gateway flag `{other}`")),
        }
        index += 1;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async move {
        // TLS material: PEM files via environment, else a self-signed
        // development certificate whose fingerprint is printed for pinning.
        let (cert_pem, key_pem) = match (
            std::env::var("SDKWORK_TUNNEL_TLS_CERT").ok(),
            std::env::var("SDKWORK_TUNNEL_TLS_KEY").ok(),
        ) {
            (Some(cert_path), Some(key_path)) => {
                let cert = tokio::fs::read(&cert_path)
                    .await
                    .map_err(|error| format!("read {cert_path}: {error}"))?;
                let key = tokio::fs::read(&key_path)
                    .await
                    .map_err(|error| format!("read {key_path}: {error}"))?;
                (cert, key)
            }
            _ => {
                let sans = vec![
                    "localhost".to_owned(),
                    "127.0.0.1".to_owned(),
                ];
                let material = tls::generate_self_signed(&sans)
                    .map_err(|error| format!("self-signed material: {error}"))?;
                tracing::warn!(
                    pin = %material.sha256,
                    "using a SELF-SIGNED development certificate; agents should pin this fingerprint"
                );
                (material.cert_pem.into_bytes(), material.key_pem.into_bytes())
            }
        };
        let bind: std::net::SocketAddr = listen
            .parse()
            .map_err(|_| format!("--listen `{listen}` is not host:port"))?;
        let mut config = TunnelConfig::disabled();
        config.gateway = Some(TunnelGatewayConfig {
            domain_suffixes,
            ..TunnelGatewayConfig::default()
        });
        let mut options = TunnelGatewayOptions::from_config(&config, bind, cert_pem, key_pem);
        if !options.authenticator.is_configured() {
            tracing::warn!(
                "SDKWORK_TUNNEL_GATEWAY_TOKEN is not set; every agent will be REFUSED"
            );
            options.authenticator = TokenAuthenticator::new(Vec::new());
        }
        let gateway = TunnelGateway::spawn(options, None)
            .await
            .map_err(|error| format!("gateway spawn: {error}"))?;
        eprintln!("tunnel gateway listening on {listen}; press Ctrl+C to stop");
        shutdown_signal().await;
        gateway.shutdown().await;
        Ok(())
    })
}

/// Agent command: run an inside-network connector (PRD §6.2).
pub fn agent_command(arguments: &[String]) -> Result<(), String> {
    let parsed = parse_agent_arguments(arguments)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async move {
        let endpoint = resolve_endpoint(&parsed.endpoint)?;
        let device = load_or_create_device(&parsed.device_name)?;
        let token = resolve_token()?;
        let metrics = std::sync::Arc::new(TunnelMetrics::new());
        let mut config = TunnelConfig::disabled();
        config.routes = Some(parsed.routes.clone());
        let options = AgentRuntimeOptions::from_config(
            &config,
            endpoint,
            parsed.tls.clone(),
            device,
            token,
            metrics,
        );
        run_agent(options, parsed.once).await
    })
}

/// Expose command: one-shot agent for a single local HTTP port (PRD §85).
pub fn expose_command(arguments: &[String]) -> Result<(), String> {
    let mut endpoint = std::env::var("SDKWORK_TUNNEL_ENDPOINT").ok();
    let mut port: Option<u16> = None;
    let mut domain: Option<String> = None;
    let mut suffix = std::env::var("SDKWORK_TUNNEL_DOMAIN_SUFFIX").ok();
    let mut name = "expose".to_owned();
    let mut tls = tls::ClientTlsOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--endpoint" => {
                index += 1;
                endpoint = Some(
                    arguments
                        .get(index)
                        .ok_or("--endpoint requires a value")?
                        .clone(),
                );
            }
            "--domain" => {
                index += 1;
                domain = Some(
                    arguments
                        .get(index)
                        .ok_or("--domain requires a value")?
                        .clone(),
                );
            }
            "--domain-suffix" => {
                index += 1;
                suffix = Some(
                    arguments
                        .get(index)
                        .ok_or("--domain-suffix requires a value")?
                        .clone(),
                );
            }
            "--name" => {
                index += 1;
                name = arguments
                    .get(index)
                    .ok_or("--name requires a value")?
                    .clone();
            }
            "--ca" => {
                index += 1;
                tls.ca_pem_path =
                    Some(arguments.get(index).ok_or("--ca requires a value")?.clone());
            }
            "--pin" => {
                index += 1;
                tls.pinned_server_sha256 = Some(
                    arguments
                        .get(index)
                        .ok_or("--pin requires a value")?
                        .clone(),
                );
            }
            "--insecure" => tls.insecure_skip_verify = true,
            value if port.is_none() && value.parse::<u16>().is_ok() => {
                port = value.parse::<u16>().ok();
            }
            other => return Err(format!("unknown expose argument `{other}`")),
        }
        index += 1;
    }
    let port = port.ok_or("expose requires a local port, e.g. `tunnel expose 3000`")?;
    let suffix = suffix.ok_or(
        "expose requires a domain suffix (--domain-suffix sdkwork.link or SDKWORK_TUNNEL_DOMAIN_SUFFIX)",
    )?;
    let domain = domain.unwrap_or_else(|| format!("{}.{suffix}", random_label(8)));
    let template = TunnelRouteTemplate {
        name,
        protocol: TunnelProtocolKind::Http,
        domain: Some(domain),
        port: None,
        target: format!("127.0.0.1:{port}"),
        policy: Some(RoutePolicy {
            allow_public: true,
            ..RoutePolicy::private()
        }),
    };
    let endpoint_string = endpoint
        .clone()
        .ok_or("expose requires --endpoint or SDKWORK_TUNNEL_ENDPOINT")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async move {
        let endpoint = resolve_endpoint(&endpoint_string)?;
        let device = load_or_create_device("")?;
        let token = resolve_token()?;
        let mut config = TunnelConfig::disabled();
        config.routes = Some(vec![template]);
        let options = AgentRuntimeOptions::from_config(
            &config,
            endpoint,
            tls,
            device,
            token,
            std::sync::Arc::new(TunnelMetrics::new()),
        );
        run_agent(options, false).await
    })
}

/// `list` / `remove` / `status` talk to a running gateway's operations
/// REST surface.
pub fn list_command(arguments: &[String]) -> Result<(), String> {
    let base = operations_base(arguments)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async {
        let (_, body) = rest_call(&base, "GET", "/api/tunnel/routes", None).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        Ok(())
    })
}

pub fn remove_command(arguments: &[String]) -> Result<(), String> {
    let mut base = "http://127.0.0.1:9100".to_owned();
    let mut route_id: Option<String> = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--url" => {
                index += 1;
                base = arguments
                    .get(index)
                    .ok_or("--url requires a value")?
                    .clone();
            }
            value if route_id.is_none() => route_id = Some(value.to_owned()),
            other => return Err(format!("unknown remove argument `{other}`")),
        }
        index += 1;
    }
    let route_id = route_id.ok_or("remove requires a route id")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async move {
        let (status, _) = rest_call(
            &base,
            "DELETE",
            &format!("/api/tunnel/routes/{route_id}"),
            None,
        )
        .await?;
        println!("removed route {route_id} (HTTP {status})");
        Ok(())
    })
}

pub fn status_command(arguments: &[String]) -> Result<(), String> {
    let base = operations_base(arguments)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async {
        let (_, body) = rest_call(&base, "GET", "/api/tunnel/status", None).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        Ok(())
    })
}

/// Doctor: staged reachability checks (PRD §83, §84).
pub fn doctor_command(arguments: &[String]) -> Result<(), String> {
    let parsed = parse_agent_arguments(arguments)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async move {
        println!("SDKWORK tunnel doctor");
        let endpoint = resolve_endpoint(&parsed.endpoint)?;
        println!("✓ endpoint {}", endpoint.authority());

        let server_name = tls::server_name_for(&endpoint);
        let transport = sdkwork_webserver_tunnel_transport::QuicClientTransport::new(
            &parsed.tls,
            sdkwork_webserver_tunnel_transport::TransportOptions::default(),
        )
        .map_err(|error| format!("TLS options: {error}"));
        let transport = match transport {
            Ok(transport) => transport,
            Err(error) => {
                println!("✗ TLS options: {error}");
                return Ok(());
            }
        };
        println!("✓ TLS configuration ({})", describe_tls(&parsed.tls));
        let connect = tokio::time::timeout(
            Duration::from_secs(10),
            sdkwork_webserver_tunnel_transport::TunnelClientTransport::connect(
                &transport,
                &endpoint,
                &server_name,
            ),
        )
        .await;
        match connect {
            Err(_) => println!("✗ QUIC connect: timed out after 10s"),
            Ok(Err(error)) => println!("✗ QUIC connect: {error}"),
            Ok(Ok(connection)) => {
                println!("✓ QUIC handshake ({server_name})");
                // Control stream + STP handshake attempt with the loaded
                // token (PRD §84 steps 5-8).
                let token = std::env::var("SDKWORK_TUNNEL_TOKEN").unwrap_or_default();
                let device = load_or_create_device(&parsed.device_name)?;
                match control_handshake_probe(&*connection, &device, &token).await {
                    Ok(()) => println!("✓ STP handshake + authentication"),
                    Err(error) => println!("✗ STP handshake: {error}"),
                }
                connection.close("doctor finished");
            }
        }
        println!("done");
        Ok(())
    })
}

async fn control_handshake_probe(
    connection: &dyn sdkwork_webserver_tunnel_transport::TunnelConnection,
    device: &Device,
    token: &str,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut control = connection
        .open_stream()
        .await
        .map_err(|error| format!("control stream: {error}"))?;
    let hello = sdkwork_webserver_tunnel_protocol::ControlMessage::Hello(
        sdkwork_webserver_tunnel_protocol::Hello {
            protocol_versions: vec![sdkwork_webserver_tunnel_protocol::ProtocolVersion::v1()],
            device_id: device.id.to_string(),
            device_name: device.name.clone(),
            platform: device.platform.as_str().to_owned(),
        },
    );
    let frame = hello.to_frame().map_err(|error| error.to_string())?;
    control
        .write_all(&frame)
        .await
        .map_err(|error| format!("hello write: {error}"))?;
    let authenticate = sdkwork_webserver_tunnel_protocol::ControlMessage::Authenticate(
        sdkwork_webserver_tunnel_protocol::Authenticate {
            token: token.to_owned(),
        },
    );
    let frame = authenticate.to_frame().map_err(|error| error.to_string())?;
    control
        .write_all(&frame)
        .await
        .map_err(|error| format!("authenticate write: {error}"))?;
    let mut scratch = bytes::BytesMut::new();
    let payload = tokio::time::timeout(Duration::from_secs(10), {
        sdkwork_webserver_tunnel_protocol::frame::read_frame(&mut *control, &mut scratch)
    })
    .await
    .map_err(|_| "gateway did not answer within 10s".to_owned())
    .and_then(|inner| inner.map_err(|error| error.to_string()))?;
    match sdkwork_webserver_tunnel_protocol::ControlMessage::from_frame(&payload) {
        Ok(sdkwork_webserver_tunnel_protocol::ControlMessage::AuthResult(result)) => {
            if result.ok {
                Ok(())
            } else {
                Err(format!(
                    "rejected: {}",
                    result.error.unwrap_or_else(|| "unknown".to_owned())
                ))
            }
        }
        Ok(other) => Err(format!("unexpected reply {other:?}")),
        Err(error) => Err(error.to_string()),
    }
}

fn describe_tls(options: &tls::ClientTlsOptions) -> String {
    if options.insecure_skip_verify {
        "verification DISABLED".to_owned()
    } else if options.pinned_server_sha256.is_some() {
        "pinned fingerprint".to_owned()
    } else {
        "CA".to_owned()
    }
}

struct ParsedAgentArguments {
    endpoint: String,
    device_name: String,
    routes: Vec<TunnelRouteTemplate>,
    tls: tls::ClientTlsOptions,
    once: bool,
}

fn parse_agent_arguments(arguments: &[String]) -> Result<ParsedAgentArguments, String> {
    let mut endpoint = std::env::var("SDKWORK_TUNNEL_ENDPOINT").ok();
    let mut device_name = hostname();
    let mut routes: Vec<TunnelRouteTemplate> = Vec::new();
    let mut tls = tls::ClientTlsOptions::default();
    let mut once = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--endpoint" => {
                index += 1;
                endpoint = Some(
                    arguments
                        .get(index)
                        .ok_or("--endpoint requires a value")?
                        .clone(),
                );
            }
            "--name" => {
                index += 1;
                device_name = arguments
                    .get(index)
                    .ok_or("--name requires a value")?
                    .clone();
            }
            "--route" => {
                index += 1;
                let raw = arguments.get(index).ok_or("--route requires a value")?;
                routes.push(parse_route(raw)?);
            }
            "--ca" => {
                index += 1;
                tls.ca_pem_path =
                    Some(arguments.get(index).ok_or("--ca requires a value")?.clone());
            }
            "--pin" => {
                index += 1;
                tls.pinned_server_sha256 = Some(
                    arguments
                        .get(index)
                        .ok_or("--pin requires a value")?
                        .clone(),
                );
            }
            "--insecure" => tls.insecure_skip_verify = true,
            "--once" => once = true,
            other => return Err(format!("unknown agent flag `{other}`")),
        }
        index += 1;
    }
    Ok(ParsedAgentArguments {
        endpoint: endpoint.ok_or("agent requires --endpoint or SDKWORK_TUNNEL_ENDPOINT")?,
        device_name,
        routes,
        tls,
        once,
    })
}

/// Parses `name=http,domain,demo.example.com,127.0.0.1:3000` style route
/// arguments: `name:<http|tcp|udp>:<domain|port>:<target>`.
fn parse_route(raw: &str) -> Result<TunnelRouteTemplate, String> {
    // Shape: <name>:<http|tcp|udp>:<domain-or-port>:<target>.
    let segments: Vec<&str> = raw.splitn(4, ':').collect();
    if segments.len() != 4 {
        return Err(format!(
            "route `{raw}` must be <name>:<http|tcp|udp>:<domain-or-port>:<target>"
        ));
    }
    let name = segments[0].to_owned();
    let protocol = match segments[1] {
        "http" => TunnelProtocolKind::Http,
        "tcp" => TunnelProtocolKind::Tcp,
        "udp" => TunnelProtocolKind::Udp,
        other => return Err(format!("route protocol `{other}` must be http, tcp, or udp")),
    };
    let template = TunnelRouteTemplate {
        name: name.clone(),
        protocol,
        domain: (protocol == TunnelProtocolKind::Http).then(|| segments[2].to_owned()),
        port: (matches!(protocol, TunnelProtocolKind::Tcp | TunnelProtocolKind::Udp))
            .then(|| {
                segments[2]
                    .parse()
                    .map_err(|_| format!("route port `{}` is not a number", segments[2]))
            })
            .transpose()?,
        target: segments[3].to_owned(),
        policy: None,
    };
    template.validate().map_err(|error| error.to_string())?;
    Ok(template)
}

async fn run_agent(options: AgentRuntimeOptions, once: bool) -> Result<(), String> {
    let mut agent = AgentRuntime::spawn(options);
    loop {
        match agent.next_event().await {
            Some(AgentEvent::Ready { routes, rejected }) => {
                for route in &routes {
                    if let Some(url) = &route.public_url {
                        println!("tunnel ready: {url} -> local {}", route.name);
                    }
                }
                for rejection in &rejected {
                    eprintln!(
                        "route rejected: {} ({})",
                        rejection.route_id, rejection.error
                    );
                }
                if once {
                    // `expose` keeps running until Ctrl+C; `--once` stops
                    // after the first successful registration pass.
                    continue;
                }
            }
            Some(AgentEvent::Disconnected { reason }) => {
                eprintln!("disconnected: {reason}");
            }
            Some(AgentEvent::ReconnectingIn { after_ms }) => {
                eprintln!("reconnecting in {} ms", after_ms);
            }
            Some(AgentEvent::Stopped) | None => return Ok(()),
            Some(_) => {}
        }
    }
}

fn operations_base(arguments: &[String]) -> Result<String, String> {
    if let Ok(value) = std::env::var("SDKWORK_TUNNEL_OPERATIONS_URL") {
        return Ok(value);
    }
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == "--url" {
            return arguments
                .get(index + 1)
                .cloned()
                .ok_or_else(|| "--url requires a value".to_owned());
        }
        index += 1;
    }
    Ok("http://127.0.0.1:9100".to_owned())
}

/// Minimal loopback REST client (HTTP/1.1, no TLS) for the gateway
/// operations surface.
async fn rest_call(
    base: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<(u16, serde_json::Value), String> {
    let authority = base.trim_start_matches("http://").trim_end_matches('/');
    let mut socket = tokio::net::TcpStream::connect(authority)
        .await
        .map_err(|error| format!("connect {authority}: {error}"))?;
    let body_bytes = body.unwrap_or("").as_bytes();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    socket
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("write: {error}"))?;
    socket
        .write_all(body_bytes)
        .await
        .map_err(|error| format!("write body: {error}"))?;
    let mut response = Vec::new();
    socket
        .read_to_end(&mut response)
        .await
        .map_err(|error| format!("read: {error}"))?;
    let text = String::from_utf8_lossy(&response);
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or_default();
    let body_text = parts.next().unwrap_or_default();
    let status: u16 = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| "malformed HTTP response".to_owned())?;
    let value = if body_text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(body_text).unwrap_or(serde_json::Value::Null)
    };
    Ok((status, value))
}

fn resolve_endpoint(raw: &str) -> Result<RemoteEndpoint, String> {
    let (host, port) = raw
        .rsplit_once(':')
        .ok_or_else(|| format!("endpoint `{raw}` must be host:port"))?;
    let port: u16 = port
        .parse()
        .map_err(|_| format!("endpoint port `{port}`"))?;
    Ok(RemoteEndpoint::new(host.trim(), port))
}

fn resolve_token() -> Result<String, String> {
    std::env::var("SDKWORK_TUNNEL_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "SDKWORK_TUNNEL_TOKEN is not set".to_owned())
}

/// Loads the persisted device identity or creates one on first run
/// (PRD §26: `~/.sdkwork/identity/device.json`; the credential never
/// leaves this machine).
fn load_or_create_device(name: &str) -> Result<Device, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "cannot resolve the home directory".to_owned())?;
    let identity_dir = std::path::Path::new(&home).join(".sdkwork").join("tunnel");
    let identity_file = identity_dir.join("device.json");
    let device_id = if let Ok(existing) = std::fs::read_to_string(&identity_file) {
        serde_json::from_str::<serde_json::Value>(&existing)
            .ok()
            .and_then(|value| {
                value
                    .get("deviceId")
                    .and_then(|id| id.as_str())
                    .map(str::to_owned)
            })
    } else {
        None
    };
    let device_id = match device_id {
        Some(id) => DeviceId::parse(id).map_err(|error| error.to_string())?,
        None => {
            let generated = DeviceId::parse(format!("dev_{}", random_label(12)))
                .map_err(|error| error.to_string())?;
            std::fs::create_dir_all(&identity_dir)
                .map_err(|error| format!("create {}: {error}", identity_dir.display()))?;
            std::fs::write(
                &identity_file,
                serde_json::json!({ "deviceId": generated.to_string() }).to_string(),
            )
            .map_err(|error| format!("write {}: {error}", identity_file.display()))?;
            generated
        }
    };
    let resolved_name = if name.is_empty() {
        hostname()
    } else {
        name.to_owned()
    };
    Device::new(
        device_id,
        resolved_name,
        DevicePlatform::parse(std::env::consts::OS),
    )
    .map_err(|error| error.to_string())
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "sdkwork-agent".to_owned())
}

fn random_label(length: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    use rand::Rng;
    let mut rng = rand::rng();
    (0..length)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
