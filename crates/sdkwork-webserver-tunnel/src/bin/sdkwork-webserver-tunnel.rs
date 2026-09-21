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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
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
         Commands:\n\
         \x20 gateway [--listen <host:port>] [--domain-suffix <suffix>]...\n\
         \x20   Run a tunnel gateway (QUIC). Agent tokens come from\n\
         \x20   SDKWORK_TUNNEL_GATEWAY_TOKEN (comma-separated).\n\
         \x20 agent --endpoint <host:port> [--name <device-name>] [--route name=http,domain,target]\n\
         \x20   [--route name=tcp,port,target] [--ca <pem>] [--pin <sha256>] [--insecure]\n\
         \x20   Run a tunnel agent exposing configured routes. Token from SDKWORK_TUNNEL_TOKEN.\n\
         \x20 expose --endpoint <host:port> [--domain <name>] [--name <n>] <local-port>\n\
         \x20   One-shot: expose a local HTTP port and print the public URL.\n\
         \x20 list    [--url <operations-base-url>]   List gateway routes.\n\
         \x20 remove  [--url <base>] <route-id>      Remove a gateway route.\n\
         \x20 status  [--url <base>]                 Gateway tunnel status.\n\
         \x20 doctor  --endpoint <host:port>         Diagnose gateway reachability (PRD §83).\n\
         \n\
         Endpoints default to SDKWORK_TUNNEL_ENDPOINT. Routes registered by an\n\
         agent always dial targets from the agent's own configuration."
    );
}
