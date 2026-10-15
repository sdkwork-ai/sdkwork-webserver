//! `sdkwork-webserver-tunnel` CLI (PRD §37, §83, §85, §122).
//!
//! Thin control surface over the tunnel library: the gateway and agent
//! roles run from this binary, `expose` launches a temporary agent for a
//! single local service, and `list` / `remove` / `status` / `doctor` talk
//! to a running gateway through its operations REST surface. All argument
//! parsing is hand-rolled, matching the standalone-gateway binary style.

use std::process::ExitCode;

use sdkwork_webserver_tunnel::cli;

fn main() -> ExitCode {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // FRP `log.level`: the level is decided before the subscriber is installed,
    // and an unusable value is a startup error rather than a filter that
    // silently matches nothing. The flag is stripped from the command line so
    // the dispatcher below never sees it as a command name.
    let (filter, arguments) = match cli::split_log_level(&arguments) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&filter)),
        )
        .init();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<(), String> {
    let Some(command) = arguments.first() else {
        print_help();
        return Ok(());
    };
    let rest = &arguments[1..];
    match command.as_str() {
        "gateway" => cli::gateway_command(rest),
        "agent" => cli::agent_command(rest),
        "visitor" => cli::visitor_command(rest),
        "verify" => cli::verify_command(rest),
        "expose" => cli::expose_command(rest),
        "list" => cli::list_command(rest),
        "remove" => cli::remove_command(rest),
        "status" => cli::status_command(rest),
        "doctor" => cli::doctor_command(rest),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!(
            "unknown command `{other}`; run `sdkwork-webserver-tunnel help`"
        )),
    }
}

fn print_help() {
    println!(
        "sdkwork-webserver-tunnel\n\
         \n\
         Global: [--log-level error|warn|info|debug|trace]  (or TUNNEL_LOG_LEVEL)\n\
         \n\
         Commands:\n\
         \x20 gateway [--listen <host:port>] [--domain-suffix <suffix>]...\n\
         \x20   Run a tunnel gateway (QUIC). Agent tokens come from\n\
         \x20   SDKWORK_TUNNEL_GATEWAY_TOKEN (comma-separated).\n\
         \x20 agent --endpoint <host:port> [--name <device-name>] [--route name=http,domain,target]\n\
         \x20   [--route name=tcp,port,target] [--ca <pem>] [--pin <sha256>] [--insecure]\n\
         \x20   Run a tunnel agent exposing configured routes. Token from SDKWORK_TUNNEL_TOKEN.\n\
         \x20 expose --endpoint <host:port> [--domain <name>] [--name <n>] <local-port>\n\
         \x20   One-shot: expose a local HTTP port and print the public URL.\n\
         \x20 visitor --endpoint <host:port> --visitor <name>:<local-port>[:<bind-addr>]...\n\
         \x20   [--secret-key-env <VAR>] [--pin <sha256>] [--insecure]\n\
         \x20   Consume a secret (stcp) route on a local port. The shared key comes\n\
         \x20   from SDKWORK_TUNNEL_SECRET_KEY unless --secret-key-env names another\n\
         \x20   variable; it is never accepted on the command line.\n\
         \x20 verify  --route <name>:<proto>:<domain-or-port>:<target>... [--json]\n\
         \x20   [--private] [--allowed-ip <cidr>] [--visitor-token <token>]\n\
         \x20   [--secret-key-env <VAR>] [--no-environment]\n\
         \x20   Offline preflight: validates routes, credentials, surfaces and\n\
         \x20   policies without connecting to a gateway. Exit code 1 on errors.\n\
         \x20 list    [--url <operations-base-url>]   List gateway routes.\n\
         \x20 remove  [--url <base>] <route-id>      Remove a gateway route.\n\
         \x20 status  [--url <base>]                 Gateway tunnel status.\n\
         \x20 doctor  --endpoint <host:port>         Diagnose gateway reachability (PRD §83).\n\
         \n\
         Endpoints default to SDKWORK_TUNNEL_ENDPOINT. Routes registered by an\n\
         agent always dial targets from the agent's own configuration."
    );
}
