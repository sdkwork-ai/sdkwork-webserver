//! Command-line surface of the tunnel binary (PRD §37, §83).
//!
//! These run the **real** binary rather than a function: the flags an operator
//! types, the exit code a script branches on, and the JSON a console parses are
//! all part of the contract, and none of them is exercised by a unit test that
//! calls the parser directly.
//!
//! Bounded by construction: `verify` is offline, and the agent runs with
//! `--once` against a port nobody listens on, so nothing here waits on a
//! network.

use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sdkwork-webserver-tunnel"))
}

/// Runs the binary with a clean environment.
///
/// The harness exports deployment variables (`SDKWORK_ENVIRONMENT`,
/// `SDKWORK_DEPLOYMENT_PROFILE`); letting them through would test the host, not
/// the CLI, and the topology resolution of this workspace is driven by exactly
/// those names.
fn run(arguments: &[&str]) -> (bool, String, String) {
    let output = Command::new(binary())
        .args(arguments)
        .env_remove("SDKWORK_ENVIRONMENT")
        .env_remove("SDKWORK_DEPLOYMENT_PROFILE")
        .env_remove("SDKWORK_PROFILE_ID")
        .env_remove("SDKWORK_ACCESS_TOKEN")
        .env_remove("SDKWORK_TUNNEL_TOKEN")
        .env_remove("SDKWORK_TUNNEL_ENDPOINT")
        .output()
        .expect("the tunnel binary runs");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn verify_reports_the_connection_side_of_a_configuration() {
    let (ok, stdout, stderr) = run(&[
        "verify",
        "--route",
        "web:http:docs.sdkwork.link:127.0.0.1:8080",
        "--endpoint",
        "gw1.sdkwork.link:8443",
        "--endpoint",
        "gw2.sdkwork.link:8443",
        "--dns-server",
        "10.0.0.53",
        "--dial-server-local-ip",
        "10.0.0.5",
        "--user",
        "team-a",
        "--login-fail-exit",
        "--json",
    ]);
    assert!(ok, "verify failed: {stderr}");
    let payload: serde_json::Value =
        serde_json::from_str(&stdout).expect("verify --json emits JSON");
    assert_eq!(payload["ok"], serde_json::json!(true));
    let connection = &payload["connection"];
    assert_eq!(
        connection["endpoints"],
        serde_json::json!(["gw1.sdkwork.link:8443", "gw2.sdkwork.link:8443"]),
        "the address list keeps its order: it is the failover order"
    );
    assert_eq!(connection["dnsServer"], serde_json::json!("10.0.0.53"));
    assert_eq!(
        connection["dialServerLocalIP"],
        serde_json::json!("10.0.0.5")
    );
    assert_eq!(connection["user"], serde_json::json!("team-a"));
    assert_eq!(connection["loginFailExit"], serde_json::json!(true));
}

#[test]
fn verify_reports_the_connection_side_in_text_too() {
    let (ok, stdout, _stderr) = run(&[
        "verify",
        "--route",
        "web:http:docs.sdkwork.link:127.0.0.1:8080",
        "--endpoint",
        "gw1.sdkwork.link:8443",
        "--dns-server",
        "10.0.0.53",
        "--user",
        "team-a",
    ]);
    assert!(ok);
    for expected in [
        "connection.endpoints",
        "gw1.sdkwork.link:8443",
        "connection.dnsServer",
        "10.0.0.53",
        "connection.user",
        "team-a",
    ] {
        assert!(
            stdout.contains(expected),
            "`{expected}` missing from:\n{stdout}"
        );
    }
}

#[test]
fn an_unusable_connection_flag_is_refused_before_anything_else() {
    // A hostname is not a source address, and a name the gateway would refuse
    // is not a user claim: both are usage errors, reported with the flag name.
    let (ok, _stdout, stderr) = run(&[
        "verify",
        "--route",
        "web:http:docs.sdkwork.link:127.0.0.1:8080",
        "--dial-server-local-ip",
        "eth0",
    ]);
    assert!(!ok, "a hostname source address must fail");
    assert!(stderr.contains("--dial-server-local-ip"), "{stderr}");

    let (ok, _stdout, stderr) = run(&[
        "verify",
        "--route",
        "web:http:docs.sdkwork.link:127.0.0.1:8080",
        "--dns-server",
        "resolver.internal",
    ]);
    assert!(!ok, "resolving the resolver must fail");
    assert!(stderr.contains("dnsServer"), "{stderr}");

    let (ok, _stdout, stderr) = run(&[
        "verify",
        "--route",
        "web:http:docs.sdkwork.link:127.0.0.1:8080",
        "--user",
        "team a",
    ]);
    assert!(!ok, "a user name with a space must fail");
    assert!(stderr.contains("may only contain"), "{stderr}");
}

#[test]
fn the_agent_prints_the_connection_settings_it_will_use() {
    // `--once` bounds the run: one attempt against a closed port, then exit.
    let (_ok, stdout, _stderr) = run(&[
        "--log-level",
        "error",
        "agent",
        "--endpoint",
        "127.0.0.1:1",
        "--route",
        "web:http:docs.sdkwork.link:127.0.0.1:8080",
        "--user",
        "team-a",
        "--login-fail-exit",
        "--dns-server",
        "10.0.0.53",
        "--dial-server-local-ip",
        "127.0.0.2",
        "--once",
    ]);
    for expected in [
        "gateway addresses: 127.0.0.1:1",
        "gateway DNS server: 10.0.0.53",
        "user claim: team-a",
        "loginFailExit: enabled",
        "target dial source address: 127.0.0.2",
    ] {
        assert!(
            stdout.contains(expected),
            "`{expected}` missing from:\n{stdout}"
        );
    }
}

#[test]
fn the_gateway_refuses_a_bind_address_that_is_not_a_literal_ip() {
    // Both forms would bind somewhere the operator did not ask for: a hostname
    // resolves per registration, and a `host:port` port is overwritten by each
    // route's own port.
    let (ok, _stdout, stderr) = run(&[
        "gateway",
        "--listen",
        "127.0.0.1:0",
        "--proxy-bind-addr",
        "gateway.internal",
    ]);
    assert!(!ok, "a hostname bind address must fail");
    assert!(stderr.contains("literal IP address"), "{stderr}");

    let (ok, _stdout, stderr) = run(&[
        "gateway",
        "--listen",
        "127.0.0.1:0",
        "--proxy-bind-addr",
        "127.0.0.1:8080",
    ]);
    assert!(!ok, "a bind address with a port must fail");
    assert!(stderr.contains("without a port"), "{stderr}");
}

#[test]
fn a_bad_log_level_fails_before_the_command_runs() {
    let (ok, _stdout, stderr) = run(&["--log-level", "verbose", "status"]);
    assert!(!ok, "an unknown level must not start anything");
    assert!(stderr.contains("unknown log level"), "{stderr}");
    assert!(
        stderr.contains("error|warn|info|debug|trace"),
        "the message lists the valid levels: {stderr}"
    );
}
