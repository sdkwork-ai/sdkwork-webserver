//! CLI command implementations (PRD §37, §83, §85, §122).
//!
//! Kept in the library for testability; the binary owns process startup
//! only (RUST_CODE_SPEC.md §1). All control operations go through
//! [`crate::service::TunnelService`] or the gateway's operations REST
//! surface — the CLI never re-implements routes, sessions, or devices
//! (PRD §81).

use std::time::Duration;

use sdkwork_webserver_tunnel_core::{
    parse_allowed_ips, parse_bandwidth_limit, validate_load_balancer_group, validate_location,
    AuthPolicy, Device, DeviceId, DevicePlatform, ProxyProtocolVersion, RoutePolicy, TunnelConfig,
    TunnelGatewayConfig, TunnelProtocolKind, TunnelRouteTemplate,
};
use sdkwork_webserver_tunnel_transport::{tls, RemoteEndpoint};

use crate::agent::{AgentEvent, AgentRuntime, AgentRuntimeOptions};
use crate::gateway::{TunnelGateway, TunnelGatewayOptions};
use crate::metrics::TunnelMetrics;
use crate::security::TokenAuthenticator;
use crate::visitor::{
    VisitorEntry, VisitorEvent, VisitorProtocol, VisitorRuntime, VisitorRuntimeOptions,
};

/// Visitor command: consume a secret route (`stcp`) on a local port.
///
/// The counterpart of an agent that registered a secret route: the provider
/// opens no public surface, and this client is what turns the private service
/// into "localhost:PORT" for the people who hold the key (FRP visitor parity).
pub fn visitor_command(arguments: &[String]) -> Result<(), String> {
    let parsed = parse_visitor_arguments(arguments)?;
    println!(
        "visitor: {} entry(ies) via {}",
        parsed.entries.len(),
        parsed.endpoint
    );
    for entry in &parsed.entries {
        println!(
            "  {} {} -> {}:{}",
            entry.protocol.wire_label(),
            entry.name,
            entry.bind_addr,
            entry.bind_port
        );
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async move {
        let endpoint = resolve_endpoint(&parsed.endpoint)?;
        let token = resolve_token()?;
        let device = load_or_create_device(&parsed.device_name)?;
        let metrics = std::sync::Arc::new(TunnelMetrics::new());
        let options = VisitorRuntimeOptions::from_config(
            &TunnelConfig::disabled(),
            endpoint,
            parsed.tls.clone(),
            device,
            token,
            parsed.entries.clone(),
            metrics,
        );
        let mut visitor = VisitorRuntime::spawn(options);
        loop {
            tokio::select! {
                event = visitor.next_event() => {
                    match event {
                        Some(VisitorEvent::Listening { name, local_addr }) => {
                            println!("visitor listening: {name} on {local_addr}");
                        }
                        Some(VisitorEvent::BindFailed { name, error }) => {
                            eprintln!("visitor could not bind {name}: {error}");
                        }
                        Some(VisitorEvent::Connected { session_id }) => {
                            eprintln!("visitor connected (session {session_id})");
                        }
                        Some(VisitorEvent::Disconnected { reason }) => {
                            eprintln!("visitor disconnected: {reason}");
                        }
                        None => return Ok(()),
                    }
                }
                _ = shutdown_signal() => {
                    visitor.shutdown().await;
                    return Ok(());
                }
            }
        }
    })
}

/// Parsed `visitor` arguments.
#[derive(Debug)]
struct ParsedVisitorArguments {
    endpoint: String,
    device_name: String,
    tls: tls::ClientTlsOptions,
    entries: Vec<VisitorEntry>,
}

/// Parses `visitor` arguments.
///
/// `--visitor <name>:<port>[:<bind-addr>]` is repeatable, and the shared key
/// is read from the environment variable named by `--secret-key-env`
/// (default `SDKWORK_TUNNEL_SECRET_KEY`) — never from the command line, where
/// it would land in a shell history and a process listing.
fn parse_visitor_arguments(arguments: &[String]) -> Result<ParsedVisitorArguments, String> {
    let mut endpoint = std::env::var("SDKWORK_TUNNEL_ENDPOINT").ok();
    let mut device_name = format!("visitor-{}", hostname());
    let mut tls = tls::ClientTlsOptions::default();
    let mut entries: Vec<VisitorEntry> = Vec::new();
    let mut secret_key_env = DEFAULT_SECRET_KEY_ENV.to_owned();
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
            "--visitor" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--visitor requires <name>:<port>[:<bind-addr>]")?;
                entries.push(parse_visitor_entry(raw)?);
            }
            "--secret-key-env" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--secret-key-env requires an environment variable name")?;
                if raw.trim().is_empty() {
                    return Err("--secret-key-env must not be empty".to_owned());
                }
                secret_key_env = raw.trim().to_owned();
            }
            "--secret-key" => {
                index += 1;
                let _ = arguments
                    .get(index)
                    .ok_or("--secret-key requires a value")?;
                return Err(
                    "the shared key must not be passed on the command line; set the environment \
                     variable named by --secret-key-env instead"
                        .to_owned(),
                );
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
            other => return Err(format!("unknown visitor flag `{other}`")),
        }
        index += 1;
    }
    if entries.is_empty() {
        return Err("visitor requires at least one --visitor <name>:<port>".to_owned());
    }
    let secret_key = std::env::var(&secret_key_env).map_err(|_| {
        format!("environment variable `{secret_key_env}` is not set (the route's shared key)")
    })?;
    let secret_key = secret_key.trim().to_owned();
    for entry in &mut entries {
        entry.secret_key = secret_key.clone();
    }
    crate::visitor::validate_entries(&entries).map_err(|error| error.to_string())?;
    Ok(ParsedVisitorArguments {
        endpoint: endpoint.ok_or("visitor requires --endpoint or SDKWORK_TUNNEL_ENDPOINT")?,
        device_name,
        tls,
        entries,
    })
}

/// Parses one `--visitor` value:
/// `[stcp|sudp:]<name>:<local-port>[:<bind-addr>]`.
///
/// The protocol prefix is optional and defaults to `stcp`, so the common case
/// stays as short as it was before datagram visitors existed.
fn parse_visitor_entry(raw: &str) -> Result<VisitorEntry, String> {
    let segments: Vec<&str> = raw.split(':').collect();
    let (protocol, rest) = match VisitorProtocol::parse(segments[0]) {
        Some(protocol) if segments.len() > 1 => (protocol, &segments[1..]),
        _ => (VisitorProtocol::Stcp, &segments[..]),
    };
    if rest.len() < 2 || rest.len() > 3 {
        return Err(format!(
            "visitor entry `{raw}` must be [stcp|sudp:]<name>:<local-port>[:<bind-addr>]"
        ));
    }
    let name = rest[0].trim();
    if name.is_empty() {
        return Err(format!("visitor entry `{raw}` has an empty route name"));
    }
    let port: u16 = rest[1]
        .trim()
        .parse()
        .map_err(|_| format!("visitor entry `{raw}`: `{}` is not a port", rest[1]))?;
    let bind_addr = match rest.get(2) {
        Some(raw_addr) => raw_addr
            .trim()
            .parse()
            .map_err(|_| format!("visitor entry `{raw}`: `{raw_addr}` is not an IP address"))?,
        // Loopback by default: a secret route is private, so publishing it to
        // the local network has to be asked for.
        None => std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    };
    Ok(VisitorEntry {
        protocol,
        name: name.to_owned(),
        secret_key: String::new(),
        bind_addr,
        bind_port: port,
    })
}

/// Verify command: offline preflight of a route set (FRP `frpc verify` parity).
///
/// Nothing is dialed: the point is to catch what a running agent would only
/// discover at registration time (a missing secret key, two routes on one
/// surface, a probe that can never pass) while the operator is still looking
/// at the command they typed.
pub fn verify_command(arguments: &[String]) -> Result<(), String> {
    let mut routes: Vec<TunnelRouteTemplate> = Vec::new();
    let mut policy = CliRoutePolicy::default();
    let mut secret_key_env: Option<String> = None;
    let mut check_environment = true;
    let mut as_json = false;
    // Connection-side preflight (FRP `serverAddr` list, `dnsServer`, `user`,
    // `loginFailExit`, `transport.dialServerLocalIP`). These do not change what
    // the routes mean, but they are the half of a configuration an operator
    // cannot check by reading a route: whether the agent will reach the right
    // gateway, as the right user, through the right resolver.
    let mut endpoints: Vec<String> = Vec::new();
    let mut dns_server: Option<String> = None;
    let mut dial_local_addr: Option<std::net::IpAddr> = None;
    let mut user: Option<String> = None;
    let mut login_fail_exit = false;
    // FRP `transport.dialServerLocalIP`: the egress address for dialing the
    // routes' local targets.
    let mut dial_local_addr: Option<std::net::IpAddr> = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--route" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--route requires <name>:<protocol>:<domain-or-port>:<target>")?;
                routes.push(parse_route(raw)?);
            }
            "--endpoint" => {
                index += 1;
                let raw = arguments.get(index).ok_or("--endpoint requires a value")?;
                for candidate in split_endpoint_list(raw) {
                    sdkwork_webserver_tunnel_core::validate_endpoint_authority(candidate)
                        .map_err(|error| error.to_string())?;
                    endpoints.push(candidate.to_owned());
                }
            }
            "--dns-server" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--dns-server requires a value")?;
                sdkwork_webserver_tunnel_core::parse_dns_server(raw)?;
                dns_server = Some(raw.trim().to_owned());
            }
            "--dial-server-local-ip" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--dial-server-local-ip requires an IP address")?;
                dial_local_addr = Some(raw.trim().parse().map_err(|_| {
                    format!("--dial-server-local-ip `{raw}` is not a literal IP address")
                })?);
            }
            "--user" => {
                index += 1;
                let raw = arguments.get(index).ok_or("--user requires a value")?;
                sdkwork_webserver_tunnel_core::validate_user_name(raw)
                    .map_err(|error| error.to_string())?;
                user = Some(raw.trim().to_owned());
            }
            "--login-fail-exit" => login_fail_exit = true,
            "--dial-server-local-ip" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--dial-server-local-ip requires an IP address")?;
                dial_local_addr = Some(raw.trim().parse().map_err(|_| {
                    format!("--dial-server-local-ip `{raw}` is not a literal IP address")
                })?);
            }
            "--secret-key-env" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--secret-key-env requires an environment variable name")?;
                if raw.trim().is_empty() {
                    return Err("--secret-key-env must not be empty".to_owned());
                }
                secret_key_env = Some(raw.trim().to_owned());
            }
            // The same admission flags the agent accepts, so a configuration
            // can be preflighted exactly as it will be started.
            "--private" => policy.allow_public = false,
            "--allowed-ip" => {
                index += 1;
                let entry = arguments
                    .get(index)
                    .ok_or("--allowed-ip requires a CIDR")?
                    .clone();
                // Parse here so a typo is reported while the operator is still
                // looking at the command; the policy keeps the textual form
                // because that is what `RoutePolicy` carries.
                parse_allowed_ips(std::slice::from_ref(&entry))
                    .map_err(|error| error.to_string())?;
                policy.allowed_ips.push(entry);
            }
            "--visitor-token" => {
                index += 1;
                policy.visitor_tokens.push(
                    arguments
                        .get(index)
                        .ok_or("--visitor-token requires a value")?
                        .clone(),
                );
            }
            "--no-environment" => check_environment = false,
            "--json" => as_json = true,
            other => return Err(format!("unknown verify flag `{other}`")),
        }
        index += 1;
    }
    if routes.is_empty() {
        return Err("verify requires at least one --route".to_owned());
    }
    for route in &mut routes {
        policy.apply(route)?;
        if route.protocol.is_secret() {
            route.secret_key_env = Some(
                secret_key_env
                    .clone()
                    .unwrap_or_else(|| DEFAULT_SECRET_KEY_ENV.to_owned()),
            );
        }
    }

    // Interaction checks need the whole set, exactly like a started agent.
    let mut config = TunnelConfig::disabled();
    config.enabled = true;
    config.routes = Some(routes);
    // The connection-side knobs are validated through the same core model the
    // runtime reads, so a preflight cannot accept what the agent would refuse.
    config.agent = Some(sdkwork_webserver_tunnel_core::TunnelAgentConfig {
        endpoint: endpoints.first().cloned().unwrap_or_default(),
        endpoints: endpoints.iter().skip(1).cloned().collect(),
        login_fail_exit,
        user: user.clone().unwrap_or_default(),
        ..sdkwork_webserver_tunnel_core::TunnelAgentConfig::default()
    });
    config.network = Some(sdkwork_webserver_tunnel_core::TunnelNetworkConfig {
        dns_server: dns_server.clone(),
        dial_local_addr,
        ..sdkwork_webserver_tunnel_core::TunnelNetworkConfig::default()
    });
    let report = sdkwork_webserver_tunnel_core::verify_config(&config, check_environment);

    if as_json {
        let findings: Vec<serde_json::Value> = report
            .findings
            .iter()
            .map(|finding| {
                serde_json::json!({
                    "severity": finding.severity.label(),
                    "scope": finding.scope,
                    "message": finding.message,
                })
            })
            .collect();
        let payload = serde_json::json!({
            "ok": report.is_ok(),
            "errors": report.count(sdkwork_webserver_tunnel_core::Severity::Error),
            "warnings": report.count(sdkwork_webserver_tunnel_core::Severity::Warning),
            "findings": findings,
            // What the agent will actually do, in the order it will try.
            "connection": {
                "endpoints": endpoints,
                "dnsServer": dns_server,
                "dialServerLocalIP": dial_local_addr.map(|address| address.to_string()),
                "user": user,
                "loginFailExit": login_fail_exit,
            },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        );
    } else {
        // Connection preflight first: it answers "will this agent reach the
        // right gateway as the right user", which is the question a route list
        // cannot answer.
        if !endpoints.is_empty() {
            println!(
                "{:<7} {:<24} {}",
                "info",
                "connection.endpoints",
                endpoints.join(" -> ")
            );
        }
        if let Some(server) = &dns_server {
            println!("{:<7} {:<24} {server}", "info", "connection.dnsServer");
        }
        if let Some(address) = dial_local_addr {
            println!(
                "{:<7} {:<24} {address}",
                "info", "connection.dialServerLocalIP"
            );
        }
        if let Some(user) = &user {
            println!("{:<7} {:<24} {user}", "info", "connection.user");
        }
        if login_fail_exit {
            println!(
                "{:<7} {:<24} a rejected token stops the agent",
                "info", "connection.loginFailExit"
            );
        }
        for finding in &report.findings {
            println!(
                "{:<7} {:<24} {}",
                finding.severity.label(),
                finding.scope,
                finding.message
            );
        }
        println!(
            "{}: {} error(s), {} warning(s)",
            if report.is_ok() { "ok" } else { "failed" },
            report.count(sdkwork_webserver_tunnel_core::Severity::Error),
            report.count(sdkwork_webserver_tunnel_core::Severity::Warning)
        );
    }

    if report.is_ok() {
        Ok(())
    } else {
        Err("the configuration has errors; see the report above".to_owned())
    }
}

/// Gateway command: run a public tunnel edge (PRD §6.1).
pub fn gateway_command(arguments: &[String]) -> Result<(), String> {
    let mut listen = std::env::var("SDKWORK_TUNNEL_LISTEN").unwrap_or_else(|_| {
        format!(
            "0.0.0.0:{}",
            sdkwork_webserver_tunnel_core::DEFAULT_TUNNEL_PORT
        )
    });
    let mut domain_suffixes: Vec<String> = Vec::new();
    // FRP `proxyBindAddr`: where agent-registered ports appear. Empty means the
    // configuration default (every interface).
    let mut proxy_bind_addr: Option<String> = None;
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
            "--proxy-bind-addr" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--proxy-bind-addr requires an IP address")?;
                // Validated through the gateway model: a hostname or a
                // `host:port` form would bind somewhere the operator did not
                // ask for, so it is a usage error here.
                sdkwork_webserver_tunnel_core::TunnelGatewayConfig {
                    proxy_bind_addr: raw.trim().to_owned(),
                    ..sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default()
                }
                .proxy_bind_addr()?;
                proxy_bind_addr = Some(raw.trim().to_owned());
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
            // FRP `proxyBindAddr`: unset keeps the configuration default.
            proxy_bind_addr: proxy_bind_addr
                .clone()
                .unwrap_or_else(|| sdkwork_webserver_tunnel_core::DEFAULT_PROXY_BIND_ADDR.to_owned()),
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
    // Print the admission policy before dialing: the operator must see
    // whether the routes they just declared are reachable by the visitors
    // they had in mind, not discover it from a 403 later.
    println!(
        "route admission policy: {} ({} route(s))",
        parsed.policy_summary,
        parsed.routes.len()
    );
    // The address list is printed because failover is only observable here: an
    // operator who configured three gateways must see all three were parsed,
    // not discover the typo in the second one during an outage.
    println!("gateway addresses: {}", parsed.endpoints.join(" -> "));
    if let Some(server) = &parsed.dns_server {
        // Printed for the same reason the address list is: the operator must
        // see which resolver their gateway name will be looked up in.
        println!("gateway DNS server: {server}");
    }
    if let Some(user) = &parsed.user {
        println!("user claim: {user}");
    }
    if parsed.login_fail_exit {
        // Worth printing: it changes what a rejected token does, and an
        // operator debugging a stopped agent needs to know it was configured.
        println!("loginFailExit: enabled (a rejected token stops the agent)");
    }
    if let Some(address) = parsed.dial_local_addr {
        // Same reason as the DNS server: the source address is invisible in
        // the local service's logs until someone goes looking for it.
        println!("target dial source address: {address}");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(async move {
        let mut endpoints = parsed.endpoints.iter();
        let primary = endpoints
            .next()
            .ok_or("agent requires --endpoint or SDKWORK_TUNNEL_ENDPOINT")?;
        let endpoint = resolve_endpoint(primary)?;
        // FRP `user`: the machine's claim about which team it serves. The
        // gateway compares it with the user its token speaks for, so a wrong
        // name is refused there — printing it here makes that obvious before
        // the first reconnect.
        let device = load_or_create_device(&parsed.device_name)?
            .with_user(parsed.user.clone())
            .map_err(|error| error.to_string())?;
        let token = resolve_token()?;
        let metrics = std::sync::Arc::new(TunnelMetrics::new());
        let mut config = TunnelConfig::disabled();
        config.routes = Some(parsed.routes.clone());
        let mut options = AgentRuntimeOptions::from_config(
            &config,
            endpoint,
            parsed.tls.clone(),
            device,
            token,
            metrics,
        );
        // FRP `dnsServer`: the resolver for the gateway host, when the
        // operator named one.
        options.network.dns_server = parsed.dns_server.clone();
        // FRP `loginFailExit`: a rejected credential stops the agent instead of
        // being retried forever.
        options.login_fail_exit = parsed.login_fail_exit;
        // FRP `transport.dialServerLocalIP`: this command line loads no
        // `[tunnel.network]` section, so the flag is the only way to set it.
        options.network.dial_local_addr = parsed.dial_local_addr;
        // The command line owns the address list here: the config-driven
        // alternates belong to the server-embedded agent, and this
        // configuration carries no `[tunnel.agent]` section at all.
        options.alternates = endpoints
            .map(|address| resolve_endpoint(address))
            .collect::<Result<Vec<_>, _>>()?;
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
    let mut policy = CliRoutePolicy::default();
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
            "--private" => policy.allow_public = false,
            "--allowed-ip" => {
                index += 1;
                policy.allowed_ips.push(
                    arguments
                        .get(index)
                        .ok_or("--allowed-ip requires a value")?
                        .clone(),
                );
            }
            "--visitor-token" => {
                index += 1;
                policy.visitor_tokens.push(
                    arguments
                        .get(index)
                        .ok_or("--visitor-token requires a value")?
                        .clone(),
                );
            }
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
    let mut template = TunnelRouteTemplate {
        name,
        protocol: TunnelProtocolKind::Http,
        domain: Some(domain),
        port: None,
        target: format!("127.0.0.1:{port}"),
        policy: Some(RoutePolicy::private()),
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    policy.apply(&mut template)?;
    println!("route admission policy: {}", policy.describe());
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
        // `doctor` diagnoses the primary address on purpose: it answers "can
        // this machine reach that gateway", and silently walking a fallback
        // list would report a healthy tunnel while the primary stays broken.
        let endpoint = resolve_endpoint(
            parsed
                .endpoints
                .first()
                .ok_or("doctor requires --endpoint or SDKWORK_TUNNEL_ENDPOINT")?,
        )?;
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
            user: device.user.clone(),
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

#[derive(Debug)]
struct ParsedAgentArguments {
    /// Gateway addresses in preference order: the primary first, then any
    /// failover candidates (never empty).
    endpoints: Vec<String>,
    device_name: String,
    routes: Vec<TunnelRouteTemplate>,
    tls: tls::ClientTlsOptions,
    once: bool,
    /// Human-readable description of the admission policy applied to every
    /// route, printed at startup.
    policy_summary: String,
    /// DNS server for the gateway host (FRP `dnsServer`), validated at parse
    /// time so a typo is a usage error rather than a silent fallback.
    dns_server: Option<String>,
    /// User this machine serves (FRP `user`); the gateway checks it against
    /// the token before labelling anything.
    user: Option<String>,
    /// Stop instead of retrying when the gateway rejects our token (FRP
    /// `loginFailExit`).
    login_fail_exit: bool,
    /// Source address for target dials (FRP `transport.dialServerLocalIP`).
    dial_local_addr: Option<std::net::IpAddr>,
}

/// Log level the tunnel CLI runs at (FRP `log.level`).
///
/// Accepted as `--log-level <level>` / `--log-level=<level>` anywhere on the
/// command line, falling back to `TUNNEL_LOG_LEVEL` and then `info`. The level
/// is validated here rather than handed to the filter as-is: a typo would
/// otherwise silently produce a filter that matches nothing, which looks
/// exactly like "the agent is doing nothing".
///
/// Returns the canonical level name for the binary to build its filter from.
pub fn log_level_filter(arguments: &[String]) -> Result<String, String> {
    Ok(split_log_level(arguments)?.0)
}

/// Splits the global `--log-level` out of the command line.
///
/// Returns the canonical level and the arguments with the flag removed. The
/// removal matters: the flag is *global*, so leaving it in place would make the
/// command dispatcher read it as the command name (`unknown command
/// '--log-level'`) — which is exactly what the first version did.
pub fn split_log_level(arguments: &[String]) -> Result<(String, Vec<String>), String> {
    let mut requested: Option<String> = None;
    let mut remaining: Vec<String> = Vec::with_capacity(arguments.len());
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        if let Some(value) = argument.strip_prefix("--log-level=") {
            requested = Some(value.to_owned());
        } else if argument == "--log-level" {
            index += 1;
            requested = Some(
                arguments
                    .get(index)
                    .ok_or("--log-level requires a value")?
                    .clone(),
            );
        } else {
            remaining.push(arguments[index].clone());
        }
        index += 1;
    }
    let requested = requested
        .or_else(|| std::env::var("TUNNEL_LOG_LEVEL").ok())
        .unwrap_or_else(|| "info".to_owned());
    Ok((parse_log_level(&requested)?, remaining))
}

/// Validates one level name.
fn parse_log_level(raw: &str) -> Result<String, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "error" => Ok("error".to_owned()),
        "warn" | "warning" => Ok("warn".to_owned()),
        "info" => Ok("info".to_owned()),
        "debug" => Ok("debug".to_owned()),
        "trace" => Ok("trace".to_owned()),
        other => Err(format!(
            "unknown log level `{other}` (expected error|warn|info|debug|trace)"
        )),
    }
}

fn parse_agent_arguments(arguments: &[String]) -> Result<ParsedAgentArguments, String> {
    // Address precedence is "flag first, environment after": `--endpoint` is
    // repeatable and also accepts a comma-separated list, so
    // `--endpoint a --endpoint b` and `--endpoint a,b` both mean "primary a,
    // fall back to b" (FRP's ordered `serverAddr` list).
    let mut flag_endpoints: Vec<String> = Vec::new();
    let mut device_name = hostname();
    let mut routes: Vec<TunnelRouteTemplate> = Vec::new();
    let mut tls = tls::ClientTlsOptions::default();
    let mut once = false;
    let mut policy = CliRoutePolicy::default();
    // Secret routes read their shared key from the environment (FRP `sk`
    // parity): the flag names the variable, so the key never reaches a shell
    // history, a process listing, or a configuration file.
    let mut secret_key_env: Option<String> = None;
    // FRP `dnsServer`: the resolver for the gateway host, when the operator
    // names one (a split-horizon or offline network publishes the name there).
    let mut dns_server: Option<String> = None;
    // FRP `user` / `loginFailExit`: the team this machine claims to serve, and
    // whether a rejected credential is fatal rather than retried.
    let mut user: Option<String> = None;
    let mut login_fail_exit = false;
    // FRP `transport.dialServerLocalIP`: the egress address for dialing the
    // routes' local targets.
    let mut dial_local_addr: Option<std::net::IpAddr> = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--endpoint" => {
                index += 1;
                let raw = arguments.get(index).ok_or("--endpoint requires a value")?;
                for candidate in split_endpoint_list(raw) {
                    sdkwork_webserver_tunnel_core::validate_endpoint_authority(candidate)
                        .map_err(|error| error.to_string())?;
                    flag_endpoints.push(candidate.to_owned());
                }
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
            "--private" => policy.allow_public = false,
            "--bandwidth-limit" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--bandwidth-limit requires a value, e.g. 2MB")?;
                policy.bandwidth_limit_bytes_per_second =
                    Some(parse_bandwidth_limit(raw).map_err(|error| error.to_string())?);
            }
            "--lb-group" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--lb-group requires a group name")?;
                policy.load_balancer_group =
                    Some(validate_load_balancer_group(raw).map_err(|error| error.to_string())?);
            }
            "--secret-key-env" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--secret-key-env requires an environment variable name")?;
                let name = raw.trim();
                if name.is_empty() {
                    return Err("--secret-key-env must not be empty".to_owned());
                }
                secret_key_env = Some(name.to_owned());
            }
            "--health-check" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--health-check requires tcp, http, or http:/path")?;
                let (kind_raw, path) = match raw.split_once(':') {
                    Some((kind, path)) => (kind, Some(path.to_owned())),
                    None => (raw.as_str(), None),
                };
                let kind = sdkwork_webserver_tunnel_core::HealthCheckKind::parse(kind_raw)
                    .ok_or_else(|| format!("--health-check `{raw}` must be tcp or http"))?;
                policy.health_check = Some(sdkwork_webserver_tunnel_core::HealthCheckConfig {
                    kind,
                    path,
                    ..policy.health_check.clone().unwrap_or_default()
                });
            }
            "--health-interval" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--health-interval requires seconds")?;
                let seconds: u64 = raw
                    .parse()
                    .map_err(|_| format!("--health-interval `{raw}` is not a number"))?;
                let mut check = policy.health_check.clone().unwrap_or_default();
                check.interval_seconds = seconds;
                policy.health_check = Some(check);
            }
            "--health-timeout" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--health-timeout requires seconds")?;
                let seconds: u64 = raw
                    .parse()
                    .map_err(|_| format!("--health-timeout `{raw}` is not a number"))?;
                let mut check = policy.health_check.clone().unwrap_or_default();
                check.timeout_seconds = seconds;
                policy.health_check = Some(check);
            }
            "--health-max-failed" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--health-max-failed requires a count")?;
                let count: u32 = raw
                    .parse()
                    .map_err(|_| format!("--health-max-failed `{raw}` is not a number"))?;
                let mut check = policy.health_check.clone().unwrap_or_default();
                check.max_failed = count;
                policy.health_check = Some(check);
            }
            "--location" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--location requires a path prefix, e.g. /api")?;
                let location = validate_location(raw).map_err(|error| error.to_string())?;
                if !policy.locations.contains(&location) {
                    policy.locations.push(location);
                }
            }
            "--proxy-protocol" => {
                index += 1;
                let label = arguments
                    .get(index)
                    .ok_or("--proxy-protocol requires v1 or v2")?;
                policy.proxy_protocol = Some(
                    ProxyProtocolVersion::parse(label)
                        .ok_or_else(|| format!("--proxy-protocol `{label}` must be v1 or v2"))?,
                );
            }
            "--allowed-ip" => {
                index += 1;
                policy.allowed_ips.push(
                    arguments
                        .get(index)
                        .ok_or("--allowed-ip requires a value")?
                        .clone(),
                );
            }
            "--visitor-token" => {
                index += 1;
                policy.visitor_tokens.push(
                    arguments
                        .get(index)
                        .ok_or("--visitor-token requires a value")?
                        .clone(),
                );
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
            "--user" => {
                index += 1;
                let raw = arguments.get(index).ok_or("--user requires a value")?;
                // Validated here: a name the gateway would refuse is a usage
                // error, not a registration that mysteriously fails later.
                sdkwork_webserver_tunnel_core::validate_user_name(raw)
                    .map_err(|error| error.to_string())?;
                user = Some(raw.trim().to_owned());
            }
            "--login-fail-exit" => login_fail_exit = true,
            "--dial-server-local-ip" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--dial-server-local-ip requires an IP address")?;
                dial_local_addr = Some(raw.trim().parse().map_err(|_| {
                    format!("--dial-server-local-ip `{raw}` is not a literal IP address")
                })?);
            }
            "--dns-server" => {
                index += 1;
                let raw = arguments
                    .get(index)
                    .ok_or("--dns-server requires a value")?;
                // Validated while parsing: at connect time a bad resolver
                // address would be indistinguishable from an unreachable
                // gateway.
                sdkwork_webserver_tunnel_core::parse_dns_server(raw)?;
                dns_server = Some(raw.trim().to_owned());
            }
            other => return Err(format!("unknown agent flag `{other}`")),
        }
        index += 1;
    }
    for route in &mut routes {
        policy.apply(route)?;
        // Every secret route reads one variable unless the operator names
        // another: one flag, one place to rotate the key.
        if route.protocol.is_secret() {
            route.secret_key_env = Some(
                secret_key_env
                    .clone()
                    .unwrap_or_else(|| DEFAULT_SECRET_KEY_ENV.to_owned()),
            );
            route.validate().map_err(|error| error.to_string())?;
        }
    }
    let policy_summary = policy.describe();
    // Flags win over the environment; environment addresses follow, so an
    // operator can keep a fallback in `SDKWORK_TUNNEL_ENDPOINT` while naming
    // the primary on the command line.
    let mut ordered = flag_endpoints;
    if let Some(from_env) = std::env::var("SDKWORK_TUNNEL_ENDPOINT").ok() {
        for candidate in split_endpoint_list(&from_env) {
            sdkwork_webserver_tunnel_core::validate_endpoint_authority(candidate)
                .map_err(|error| error.to_string())?;
            if !ordered.iter().any(|seen| seen == candidate) {
                ordered.push(candidate.to_owned());
            }
        }
    }
    let endpoint = ordered
        .first()
        .cloned()
        .ok_or("agent requires --endpoint or SDKWORK_TUNNEL_ENDPOINT")?;
    let alternates = ordered.split_off(1);
    let mut ordered = vec![endpoint];
    ordered.extend(alternates);
    Ok(ParsedAgentArguments {
        endpoints: ordered,
        device_name,
        routes,
        tls,
        once,
        policy_summary,
        dns_server,
        user,
        login_fail_exit,
        dial_local_addr,
    })
}

/// Splits one `--endpoint` value into addresses: comma-separated, blanks
/// dropped.
fn split_endpoint_list(raw: &str) -> impl Iterator<Item = &str> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Default environment variable holding a secret route's shared key.
const DEFAULT_SECRET_KEY_ENV: &str = "SDKWORK_TUNNEL_SECRET_KEY";

/// Admission policy the CLI applies to every `--route` template.
///
/// Defaults follow FRP: an operator who runs an agent to expose a local port
/// means it to be reachable, and the gateway's own `domain_suffixes` /
/// route-port range still gate what may be registered. Without this a
/// documented `agent --route web:http:…` registered a route that denied every
/// visitor, because the CLI had no way to express an admission channel at all.
struct CliRoutePolicy {
    /// Serve any visitor the network allow-list admits.
    allow_public: bool,
    /// Visitor network allow-list (CIDR entries).
    allowed_ips: Vec<String>,
    /// Shared visitor bearer tokens; a non-empty list implies
    /// `auth = bearer_token`.
    visitor_tokens: Vec<String>,
    /// PROXY protocol version prepended toward each route's local target.
    proxy_protocol: Option<ProxyProtocolVersion>,
    /// Path prefixes every route answers on its host (FRP `locations` parity).
    locations: Vec<String>,
    /// Relay ceiling in bytes per second applied to every route.
    bandwidth_limit_bytes_per_second: Option<u64>,
    /// Load-balancer group applied to every route (FRP `loadBalancer.group`).
    load_balancer_group: Option<String>,
    /// Health probe applied to every route (FRP `healthCheck`).
    health_check: Option<sdkwork_webserver_tunnel_core::HealthCheckConfig>,
}

impl Default for CliRoutePolicy {
    fn default() -> Self {
        Self {
            allow_public: true,
            allowed_ips: Vec::new(),
            visitor_tokens: Vec::new(),
            proxy_protocol: None,
            locations: Vec::new(),
            bandwidth_limit_bytes_per_second: None,
            load_balancer_group: None,
            health_check: None,
        }
    }
}

impl CliRoutePolicy {
    /// Materializes the policy onto one route template and validates the
    /// combination, so a flag that cannot work (a visitor token on a raw TCP
    /// route, a PROXY header on a datagram route) fails at startup instead of
    /// registering an unreachable route.
    fn apply(&self, template: &mut TunnelRouteTemplate) -> Result<(), String> {
        let policy = RoutePolicy {
            allow_public: self.allow_public,
            allowed_ips: parse_allowed_ips(&self.allowed_ips).map_err(|error| error.to_string())?,
            auth: if self.visitor_tokens.is_empty() {
                AuthPolicy::None
            } else {
                AuthPolicy::BearerToken
            },
            visitor_tokens: self.visitor_tokens.clone(),
        };
        template.policy = Some(policy);
        template.proxy_protocol = self.proxy_protocol;
        template.locations = self.locations.clone();
        template.bandwidth_limit_bytes_per_second = self.bandwidth_limit_bytes_per_second;
        template.load_balancer_group = self.load_balancer_group.clone();
        template.health_check = self.health_check.clone();
        template.validate().map_err(|error| error.to_string())
    }

    /// One-line description of the effective policy, printed when the agent
    /// becomes ready.
    fn describe(&self) -> String {
        let transport = match self.proxy_protocol {
            Some(version) => format!(", PROXY protocol {}", version.label()),
            None => String::new(),
        };
        let paths = if self.locations.is_empty() {
            String::new()
        } else {
            format!(", paths {}", self.locations.join(" "))
        };
        let ceiling = match self.bandwidth_limit_bytes_per_second {
            Some(bytes) => format!(", limited to {bytes} B/s each way"),
            None => String::new(),
        };
        let balancing = match (&self.load_balancer_group, &self.health_check) {
            (Some(group), Some(check)) => format!(
                ", group {group} with {} probe every {}s",
                check.kind.label(),
                check.interval_seconds
            ),
            (Some(group), None) => format!(", group {group}"),
            (None, Some(check)) => format!(
                ", {} probe every {}s (no group)",
                check.kind.label(),
                check.interval_seconds
            ),
            (None, None) => String::new(),
        };
        if !self.visitor_tokens.is_empty() {
            return format!(
                "visitor token required ({} token(s)){transport}{paths}{ceiling}{balancing}",
                self.visitor_tokens.len()
            );
        }
        if !self.allowed_ips.is_empty() {
            return format!(
                "network allow-list: {}{transport}{paths}{ceiling}{balancing}",
                self.allowed_ips.join(", ")
            );
        }
        if self.allow_public {
            return format!("public (any visitor){transport}{paths}{ceiling}{balancing}");
        }
        format!("private (no visitor can be admitted){transport}{paths}{ceiling}{balancing}")
    }
}

/// Parses one `--route` value:
/// `<name>:<http|tcp|udp|stcp>:<domain-or-port>:<target>` — for example
/// `web:http:demo.example.com:127.0.0.1:3000` (HTTP) or
/// `ssh:tcp:2222:127.0.0.1:22` (TCP/UDP, where the third segment is the
/// gateway port the agent asks for).
///
/// A secret route opens no public surface, so its third segment is a
/// placeholder: `private-ssh:stcp:-:127.0.0.1:22` (the key comes from
/// `--secret-key-env`, never from the command line).
fn parse_route(raw: &str) -> Result<TunnelRouteTemplate, String> {
    let segments: Vec<&str> = raw.splitn(4, ':').collect();
    if segments.len() != 4 {
        return Err(format!(
            "route `{raw}` must be <name>:<http|tcp|udp|stcp>:<domain-or-port>:<target>"
        ));
    }
    let name = segments[0].to_owned();
    let protocol = TunnelProtocolKind::parse(segments[1]).ok_or_else(|| {
        format!(
            "route protocol `{}` must be http, tcp, udp, or stcp",
            segments[1]
        )
    })?;
    // `stcp` (and later `sudp`) has no host or port: the middle segment must be
    // an explicit placeholder so a mistyped domain cannot look configured.
    if protocol.is_secret() && !matches!(segments[2], "" | "-") {
        return Err(format!(
            "secret route `{name}` opens no public surface; write `-` for its domain-or-port segment"
        ));
    }
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
        proxy_protocol: None,
        locations: Vec::new(),
        // Defaulted here so the template validates; `--secret-key-env`
        // replaces it once every flag is known.
        secret_key_env: protocol
            .is_secret()
            .then(|| DEFAULT_SECRET_KEY_ENV.to_owned()),
        ..TunnelRouteTemplate::default()
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
    use rand::RngExt;
    let mut rng = rand::rng();
    (0..length)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_args(extra: &[&str]) -> Vec<String> {
        let mut arguments = vec![
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--route".to_owned(),
            "web:http:demo.tunnel.test:127.0.0.1:3000".to_owned(),
        ];
        arguments.extend(extra.iter().map(|value| (*value).to_owned()));
        arguments
    }

    /// `--endpoint` is repeatable and accepts a comma-separated list; the
    /// first address is the primary and the rest are failover candidates.
    #[test]
    fn repeated_and_comma_separated_endpoints_build_an_ordered_list() {
        let repeated = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "gw1.sdkwork.link:8443".to_owned(),
            "--endpoint".to_owned(),
            "gw2.sdkwork.link:8443".to_owned(),
            "--route".to_owned(),
            "web:http:demo.tunnel.test:127.0.0.1:3000".to_owned(),
        ])
        .expect("parses");
        assert_eq!(
            repeated.endpoints,
            vec![
                "gw1.sdkwork.link:8443".to_owned(),
                "gw2.sdkwork.link:8443".to_owned(),
            ]
        );

        let comma = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "gw1.sdkwork.link:8443, gw2.sdkwork.link:8443 ,gw3.sdkwork.link:8443".to_owned(),
            "--route".to_owned(),
            "web:http:demo.tunnel.test:127.0.0.1:3000".to_owned(),
        ])
        .expect("parses");
        assert_eq!(
            comma.endpoints,
            vec![
                "gw1.sdkwork.link:8443".to_owned(),
                "gw2.sdkwork.link:8443".to_owned(),
                "gw3.sdkwork.link:8443".to_owned(),
            ]
        );

        // A single address keeps the old shape exactly.
        let single = parse_agent_arguments(&agent_args(&[])).expect("parses");
        assert_eq!(single.endpoints, vec!["127.0.0.1:8443".to_owned()]);
    }

    /// A malformed address is refused where it is written, not turned into a
    /// fallback that never connects.
    #[test]
    fn a_malformed_cli_endpoint_is_refused() {
        for address in ["gw1.sdkwork.link", "https://gw1.sdkwork.link:8443", "gw1:0"] {
            let arguments = vec![
                "--endpoint".to_owned(),
                address.to_owned(),
                "--route".to_owned(),
                "web:http:demo.tunnel.test:127.0.0.1:3000".to_owned(),
            ];
            assert!(
                parse_agent_arguments(&arguments).is_err(),
                "`{address}` must be refused"
            );
        }
    }

    #[test]
    fn cli_routes_are_public_by_default_so_the_documented_quick_start_works() {
        // The CLI used to build every `--route` template with the deny-all
        // default policy and offered no way to change it, so the documented
        // quick start registered a route that refused every visitor.
        let parsed = parse_agent_arguments(&agent_args(&[])).expect("parses");
        let policy = parsed.routes[0].policy_or_default();
        assert!(policy.allow_public);
        assert_eq!(policy.auth, AuthPolicy::None);
        assert!(policy.allowed_ips.is_empty());
        assert_eq!(parsed.policy_summary, "public (any visitor)");
    }

    #[test]
    fn private_flag_restores_the_deny_all_policy() {
        let parsed = parse_agent_arguments(&agent_args(&["--private"])).expect("parses");
        let policy = parsed.routes[0].policy_or_default();
        assert!(!policy.allow_public);
        assert!(policy.visitor_tokens.is_empty());
        assert_eq!(
            parsed.policy_summary,
            "private (no visitor can be admitted)"
        );
    }

    #[test]
    fn allowed_ip_entries_become_a_network_allow_list() {
        let parsed = parse_agent_arguments(&agent_args(&[
            "--allowed-ip",
            "10.0.0.0/8",
            "--allowed-ip",
            "203.0.113.7/32",
        ]))
        .expect("parses");
        let policy = parsed.routes[0].policy_or_default();
        assert_eq!(policy.allowed_ips.len(), 2);
        assert_eq!(
            parsed.policy_summary,
            "network allow-list: 10.0.0.0/8, 203.0.113.7/32"
        );
    }

    #[test]
    fn visitor_token_flag_switches_the_route_to_bearer_auth() {
        let parsed =
            parse_agent_arguments(&agent_args(&["--visitor-token", "s3cret"])).expect("parses");
        let policy = parsed.routes[0].policy_or_default();
        assert_eq!(policy.auth, AuthPolicy::BearerToken);
        assert_eq!(policy.visitor_tokens, vec!["s3cret".to_owned()]);
        assert_eq!(parsed.policy_summary, "visitor token required (1 token(s))");
    }

    #[test]
    fn a_visitor_token_on_a_raw_tcp_route_is_refused_at_startup() {
        // A bearer demand on the raw TCP plane rejects every visitor forever;
        // it must fail here instead of registering an unreachable route.
        let error = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--route".to_owned(),
            "ssh:tcp:7022:127.0.0.1:22".to_owned(),
            "--visitor-token".to_owned(),
            "s3cret".to_owned(),
        ])
        .expect_err("bearer on tcp must be refused");
        assert!(error.contains("bearer"), "unexpected error: {error}");
    }

    #[test]
    fn an_unparsable_allowed_ip_is_refused_at_startup() {
        let error = parse_agent_arguments(&agent_args(&["--allowed-ip", "not-a-network"]))
            .expect_err("invalid cidr must be refused");
        assert!(error.contains("not-a-network"), "unexpected error: {error}");
    }

    #[test]
    fn proxy_protocol_flag_applies_to_every_route_and_is_printed() {
        let parsed =
            parse_agent_arguments(&agent_args(&["--proxy-protocol", "v1"])).expect("parses");
        assert_eq!(
            parsed.routes[0].proxy_protocol,
            Some(ProxyProtocolVersion::V1)
        );
        assert_eq!(
            parsed.policy_summary,
            "public (any visitor), PROXY protocol v1"
        );

        let error = parse_agent_arguments(&agent_args(&["--proxy-protocol", "v9"]))
            .expect_err("an unknown version must be refused");
        assert!(error.contains("v1 or v2"), "unexpected error: {error}");
    }

    #[test]
    fn location_flags_become_a_normalized_path_claim_on_every_route() {
        let parsed = parse_agent_arguments(&agent_args(&[
            "--location",
            "/api/",
            "--location",
            "/api",
            "--location",
            "/static",
        ]))
        .expect("parses");
        assert_eq!(
            parsed.routes[0].locations,
            vec!["/api".to_owned(), "/static".to_owned()]
        );
        assert_eq!(
            parsed.policy_summary,
            "public (any visitor), paths /api /static"
        );

        let error = parse_agent_arguments(&agent_args(&["--location", "api"]))
            .expect_err("a location without a leading slash must be refused");
        assert!(error.contains("must start with `/`"), "unexpected: {error}");
    }

    #[test]
    fn bandwidth_limit_flag_accepts_sizes_and_reaches_every_route() {
        let parsed =
            parse_agent_arguments(&agent_args(&["--bandwidth-limit", "2MB"])).expect("parses");
        assert_eq!(
            parsed.routes[0].bandwidth_limit_bytes_per_second,
            Some(2 * 1024 * 1024)
        );
        assert_eq!(
            parsed.policy_summary,
            "public (any visitor), limited to 2097152 B/s each way"
        );

        // A bare number is bytes per second, and zero is refused: "unlimited"
        // and "no traffic" are opposite readings of the same value.
        let bare =
            parse_agent_arguments(&agent_args(&["--bandwidth-limit", "4096"])).expect("parses");
        assert_eq!(bare.routes[0].bandwidth_limit_bytes_per_second, Some(4096));
        let error = parse_agent_arguments(&agent_args(&["--bandwidth-limit", "0"]))
            .expect_err("a zero ceiling must be refused");
        assert!(error.contains("greater than zero"), "unexpected: {error}");
    }

    #[test]
    fn load_balancer_and_health_flags_reach_every_route() {
        let parsed = parse_agent_arguments(&agent_args(&[
            "--lb-group",
            "web",
            "--health-check",
            "http:/health",
            "--health-interval",
            "5",
            "--health-timeout",
            "2",
            "--health-max-failed",
            "3",
        ]))
        .expect("parses");
        let template = &parsed.routes[0];
        assert_eq!(template.load_balancer_group.as_deref(), Some("web"));
        let probe = template.health_check.clone().expect("probe attached");
        assert_eq!(
            probe.kind,
            sdkwork_webserver_tunnel_core::HealthCheckKind::Http
        );
        assert_eq!(probe.path.as_deref(), Some("/health"));
        assert_eq!(probe.interval_seconds, 5);
        assert_eq!(probe.timeout_seconds, 2);
        assert_eq!(probe.max_failed, 3);
        assert!(
            parsed.policy_summary.contains("group web"),
            "the summary must name the group: {}",
            parsed.policy_summary
        );

        // A path on a TCP probe means the operator believes an HTTP probe is
        // running; fail at startup instead of checking the socket.
        let error = parse_agent_arguments(&agent_args(&["--health-check", "tcp:/health"]))
            .expect_err("a tcp probe with a path must be refused");
        assert!(error.contains("path"), "unexpected error: {error}");

        // A datagram route cannot join a group or carry a probe.
        let udp = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--route".to_owned(),
            "dns:udp:7053:127.0.0.1:53".to_owned(),
            "--lb-group".to_owned(),
            "dns".to_owned(),
        ])
        .expect_err("a udp route cannot join a group");
        assert!(udp.contains("udp routes cannot join"), "unexpected: {udp}");
    }

    #[test]
    fn a_secret_route_is_expressed_on_the_command_line_without_its_key() {
        let parsed = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--route".to_owned(),
            "private-ssh:stcp:-:127.0.0.1:22".to_owned(),
            "--secret-key-env".to_owned(),
            "MY_SSH_TUNNEL_KEY".to_owned(),
        ])
        .expect("parses");
        let template = &parsed.routes[0];
        assert_eq!(template.protocol, TunnelProtocolKind::Stcp);
        assert_eq!(template.target, "127.0.0.1:22");
        assert!(template.domain.is_none() && template.port.is_none());
        assert_eq!(
            template.secret_key_env.as_deref(),
            Some("MY_SSH_TUNNEL_KEY")
        );

        // The default variable name applies when the flag is omitted, so a
        // single-key deployment needs no extra flag.
        let defaulted = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--route".to_owned(),
            "private-ssh:stcp:-:127.0.0.1:22".to_owned(),
        ])
        .expect("parses");
        assert_eq!(
            defaulted.routes[0].secret_key_env.as_deref(),
            Some("SDKWORK_TUNNEL_SECRET_KEY")
        );

        // A domain or port on a secret route would promise a surface the
        // gateway never binds, so it is refused rather than ignored.
        let error = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--route".to_owned(),
            "private-ssh:stcp:ssh.example.com:127.0.0.1:22".to_owned(),
        ])
        .expect_err("a secret route denies a public surface");
        assert!(error.contains("no public surface"), "unexpected: {error}");

        // An unknown protocol lists the supported set, `stcp` included.
        let unknown = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--route".to_owned(),
            "weird:quic:1:127.0.0.1:22".to_owned(),
        ])
        .expect_err("an unknown protocol is refused");
        assert!(
            unknown.contains("http, tcp, udp, or stcp"),
            "unexpected: {unknown}"
        );
    }

    #[test]
    fn visitor_arguments_name_a_route_and_never_carry_the_key() {
        std::env::set_var("SDKWORK_TUNNEL_TEST_VISITOR_KEY", "visitor-key-456");
        let parsed = parse_visitor_arguments(&[
            "--endpoint".to_owned(),
            "127.0.0.1:8443".to_owned(),
            "--visitor".to_owned(),
            "private-ssh:2222".to_owned(),
            "--visitor".to_owned(),
            "private-db:5432:0.0.0.0".to_owned(),
            "--secret-key-env".to_owned(),
            "SDKWORK_TUNNEL_TEST_VISITOR_KEY".to_owned(),
        ])
        .expect("parses");
        assert_eq!(parsed.entries.len(), 2);
        // Loopback by default: a secret route must not reach the local network
        // unless the operator asks for it.
        assert_eq!(
            parsed.entries[0].bind_addr,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );
        assert_eq!(parsed.entries[0].bind_port, 2222);
        assert_eq!(parsed.entries[0].secret_key, "visitor-key-456");
        assert_eq!(
            parsed.entries[1].bind_addr,
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
        );
        // The protocol prefix selects the datagram form; without it the entry
        // stays a plain secret-TCP visitor.
        assert_eq!(parsed.entries[0].protocol, VisitorProtocol::Stcp);

        let datagram = parse_visitor_entry("sudp:private-dns:5353").expect("parses");
        assert_eq!(datagram.protocol, VisitorProtocol::Sudp);
        assert_eq!(datagram.name, "private-dns");
        assert_eq!(datagram.bind_port, 5353);
        assert_eq!(
            datagram.bind_addr,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );

        // The key is never a command-line argument: it would land in a shell
        // history and in every process listing.
        let refused =
            parse_visitor_arguments(&["--secret-key".to_owned(), "visitor-key-456".to_owned()])
                .expect_err("a command-line key is refused");
        assert!(
            refused.contains("must not be passed on the command line"),
            "unexpected: {refused}"
        );

        // A missing environment variable is named, rather than read as empty
        // and rejected later by the gateway.
        std::env::remove_var("SDKWORK_TUNNEL_TEST_VISITOR_KEY");
        let missing = parse_visitor_arguments(&[
            "--visitor".to_owned(),
            "private-ssh:2222".to_owned(),
            "--secret-key-env".to_owned(),
            "SDKWORK_TUNNEL_TEST_VISITOR_KEY".to_owned(),
        ])
        .expect_err("a missing key variable is refused");
        assert!(
            missing.contains("SDKWORK_TUNNEL_TEST_VISITOR_KEY"),
            "unexpected: {missing}"
        );

        let empty = parse_visitor_arguments(&[]).expect_err("no entry is a usage error");
        assert!(
            empty.contains("at least one --visitor"),
            "unexpected: {empty}"
        );
    }

    #[test]
    fn verify_reports_success_and_failure_without_a_gateway() {
        // A well-formed route set verifies with no gateway, no token and no
        // environment: preflight must work on a laptop and in CI.
        verify_command(&[
            "--no-environment".to_owned(),
            "--route".to_owned(),
            "web:http:demo.sdkwork.link:127.0.0.1:3000".to_owned(),
        ])
        .expect("a healthy route set verifies");

        // Two routes fighting over one host is the failure a running agent
        // discovers only at registration time.
        let error = verify_command(&[
            "--no-environment".to_owned(),
            "--route".to_owned(),
            "web:http:demo.sdkwork.link:127.0.0.1:3000".to_owned(),
            "--route".to_owned(),
            "api:http:demo.sdkwork.link:127.0.0.1:3001".to_owned(),
        ])
        .expect_err("a surface conflict fails the preflight");
        assert!(error.contains("errors"), "{error}");

        // A secret route without its key variable is refused, and the message
        // names the variable rather than complaining about a route.
        std::env::remove_var("SDKWORK_TUNNEL_TEST_VERIFY_MISSING");
        let error = verify_command(&[
            "--route".to_owned(),
            "private-ssh:stcp:-:127.0.0.1:22".to_owned(),
            "--secret-key-env".to_owned(),
            "SDKWORK_TUNNEL_TEST_VERIFY_MISSING".to_owned(),
        ])
        .expect_err("a missing key variable fails the preflight");
        assert!(error.contains("errors"), "{error}");

        // With the variable set and long enough, the same route verifies.
        std::env::set_var("SDKWORK_TUNNEL_TEST_VERIFY_KEY", "verify-key-123");
        verify_command(&[
            "--route".to_owned(),
            "private-ssh:stcp:-:127.0.0.1:22".to_owned(),
            "--secret-key-env".to_owned(),
            "SDKWORK_TUNNEL_TEST_VERIFY_KEY".to_owned(),
        ])
        .expect("a resolvable secret key verifies");

        // JSON output is machine-readable, for CI gates.
        verify_command(&[
            "--json".to_owned(),
            "--no-environment".to_owned(),
            "--route".to_owned(),
            "web:http:demo.sdkwork.link:127.0.0.1:3000".to_owned(),
        ])
        .expect("json output still verifies");

        // Usage errors are usage errors, not silent successes.
        assert!(verify_command(&[]).is_err());
        assert!(verify_command(&["--nope".to_owned()]).is_err());
        let bad_cidr = verify_command(&[
            "--route".to_owned(),
            "web:http:demo.sdkwork.link:127.0.0.1:3000".to_owned(),
            "--allowed-ip".to_owned(),
            "not-a-cidr".to_owned(),
        ])
        .expect_err("a malformed allow-list entry is refused");
        assert!(bad_cidr.contains("CIDR"), "{bad_cidr}");
    }

    #[test]
    fn parse_route_still_accepts_every_documented_shape() {
        let http = parse_route("web:http:demo.tunnel.test:127.0.0.1:3000").expect("http");
        assert_eq!(http.protocol, TunnelProtocolKind::Http);
        assert_eq!(http.domain.as_deref(), Some("demo.tunnel.test"));

        let tcp = parse_route("ssh:tcp:7022:127.0.0.1:22").expect("tcp");
        assert_eq!(tcp.protocol, TunnelProtocolKind::Tcp);
        assert_eq!(tcp.port, Some(7022));

        let udp = parse_route("dns:udp:7053:127.0.0.1:53").expect("udp");
        assert_eq!(udp.protocol, TunnelProtocolKind::Udp);
        assert_eq!(udp.port, Some(7053));

        assert!(parse_route("web:gopher:demo.tunnel.test:127.0.0.1:70").is_err());
    }
}

#[cfg(test)]
mod log_level_tests {
    use super::*;

    #[test]
    fn the_flag_and_the_environment_are_both_honoured() {
        assert_eq!(
            log_level_filter(&[
                "agent".to_owned(),
                "--log-level".to_owned(),
                "debug".to_owned()
            ])
            .expect("flag"),
            "debug"
        );
        assert_eq!(
            log_level_filter(&["--log-level=TRACE".to_owned(), "status".to_owned()])
                .expect("inline flag"),
            "trace"
        );
        // Absent everywhere: the previous default.
        std::env::remove_var("TUNNEL_LOG_LEVEL");
        assert_eq!(
            log_level_filter(&["status".to_owned()]).expect("default"),
            "info"
        );
    }

    #[test]
    fn the_flag_is_removed_from_the_command_line() {
        // The bug the smoke test caught: a parsed-but-left-in-place flag made
        // the dispatcher read `--log-level` as the command name.
        let (level, remaining) = split_log_level(&[
            "--log-level".to_owned(),
            "debug".to_owned(),
            "agent".to_owned(),
            "--endpoint".to_owned(),
            "gw:8443".to_owned(),
        ])
        .expect("parses");
        assert_eq!(level, "debug");
        assert_eq!(remaining, vec!["agent", "--endpoint", "gw:8443"]);

        let (level, remaining) =
            split_log_level(&["status".to_owned(), "--log-level=trace".to_owned()])
                .expect("parses");
        assert_eq!(level, "trace");
        assert_eq!(remaining, vec!["status"]);
    }

    #[test]
    fn a_dns_server_flag_is_validated_while_parsing() {
        let parsed = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "gw.sdkwork.link:8443".to_owned(),
            "--route".to_owned(),
            "web:http:docs.sdkwork.link:127.0.0.1:8080".to_owned(),
            "--dns-server".to_owned(),
            "10.0.0.53:5353".to_owned(),
        ])
        .expect("parses");
        assert_eq!(parsed.dns_server.as_deref(), Some("10.0.0.53:5353"));

        // A name would be resolved by the system resolver, i.e. the resolver
        // the operator just removed, so it is a usage error.
        let error = parse_agent_arguments(&[
            "--endpoint".to_owned(),
            "gw.sdkwork.link:8443".to_owned(),
            "--route".to_owned(),
            "web:http:docs.sdkwork.link:127.0.0.1:8080".to_owned(),
            "--dns-server".to_owned(),
            "resolver.internal".to_owned(),
        ])
        .expect_err("a hostname is refused");
        assert!(error.contains("dnsServer"), "{error}");
        assert!(
            parse_agent_arguments(&[
                "--endpoint".to_owned(),
                "gw.sdkwork.link:8443".to_owned(),
                "--dns-server".to_owned(),
            ])
            .is_err(),
            "a missing value is an error"
        );
    }

    #[test]
    fn an_unusable_level_is_refused_instead_of_silently_matching_nothing() {
        let error = log_level_filter(&["--log-level".to_owned(), "verbose".to_owned()])
            .expect_err("a typo must be an error");
        assert!(error.contains("unknown log level"), "{error}");
        assert!(error.contains("error|warn|info|debug|trace"), "{error}");
        assert!(
            log_level_filter(&["--log-level".to_owned()]).is_err(),
            "a missing value is an error"
        );
        // warning is the spelling people write; accept it and canonicalize.
        assert_eq!(
            log_level_filter(&["--log-level".to_owned(), "Warning".to_owned()]).expect("alias"),
            "warn"
        );
    }
}
