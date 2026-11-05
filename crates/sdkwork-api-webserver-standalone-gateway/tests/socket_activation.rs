//! Socket-activation adoption (`ops.executable-upgrade.fd-inheritance`, PRD
//! production-operations §8): the gateway adopts `LISTEN_FDS` descriptors in
//! place of binding, refuses a descriptor set that does not match the declared
//! sockets, and serves normally through an adopted socket.

#![cfg(unix)]

use std::{
    io::Write,
    net::TcpListener,
    os::unix::io::AsRawFd,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::json;

fn response_config(port: u16, body: &str) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-socket-activation-test",
        "limits": { "requestTimeoutMs": 5000, "drainTimeoutMs": 1000, "maxConnections": 32 },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "resources": [{
            "id": "response",
            "type": "respond",
            "status": 200,
            "contentType": "text/plain; charset=utf-8",
            "body": body
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["activation.localhost"],
            "routes": [{
                "id": "response-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "response"
            }]
        }]
    }))
    .expect("serialize config")
}

/// Spawns the gateway through a shell that re-marks the inherited descriptor
/// as `LISTEN_PID=$$` (the exec keeps the shell's pid) and dup2's the listener
/// into descriptor 3, mirroring systemd socket activation.
#[allow(unsafe_code)]
fn spawn_with_inherited_socket(
    binary: &str,
    config_path: &std::path::Path,
    listener: &TcpListener,
) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let raw_fd = listener.as_raw_fd();
    // SAFETY: `pre_exec` runs in the forked child before exec; dup2 places the
    // parent's pre-bound listener at descriptor 3 (clearing CLOEXEC) exactly
    // as sd_listen_fds(3) expects. No other thread exists in the child yet.
    unsafe {
        Command::new("/bin/sh")
            .arg("-c")
            .arg("export LISTEN_PID=$$; export LISTEN_FDS=1; exec \"$0\" data-plane \"$1\"")
            .arg(binary)
            .arg(config_path)
            .pre_exec(move || {
                if libc::dup2(raw_fd, 3) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            })
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn socket-activated gateway")
    }
}

fn wait_for_body(port: u16, expected: &str, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "{message}: timed out waiting for {expected}"
        );
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            if stream
                .write_all(
                    b"GET / HTTP/1.1\r\nhost: activation.localhost\r\nconnection: close\r\n\r\n",
                )
                .is_ok()
            {
                let mut response = Vec::new();
                if std::io::Read::read_to_end(&mut stream, &mut response).is_ok()
                    && String::from_utf8_lossy(&response).contains(expected)
                {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn gateway_adopts_the_inherited_listening_socket_and_serves() {
    let directory = tempfile::TempDir::new().expect("create temp directory");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind inherited socket");
    let port = listener.local_addr().expect("local addr").port();
    let config_path = directory.path().join("config.json");
    std::fs::write(&config_path, response_config(port, "activation-ok")).expect("write config");

    let binary = env!("CARGO_BIN_EXE_sdkwork-api-webserver-standalone-gateway");
    let mut child = spawn_with_inherited_socket(binary, &config_path, &listener);
    // The inherited socket is consumed by the child; the parent's copy must
    // not answer on it anymore. Dropping it closes only the parent's view.
    drop(listener);

    wait_for_body(port, "activation-ok", "the adopted socket serves");
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn gateway_fails_closed_when_the_inherited_socket_does_not_match_the_configuration() {
    let directory = tempfile::TempDir::new().expect("create temp directory");
    // The descriptor is bound to a port the configuration does NOT declare.
    let stray = TcpListener::bind("127.0.0.1:0").expect("bind stray socket");
    let stray_port = stray.local_addr().expect("local addr").port();
    let declared_port = TcpListener::bind("127.0.0.1:0")
        .expect("probe a free port")
        .local_addr()
        .expect("local addr")
        .port();
    let config_path = directory.path().join("config.json");
    std::fs::write(&config_path, response_config(declared_port, "never-served"))
        .expect("write config");

    let binary = env!("CARGO_BIN_EXE_sdkwork-api-webserver-standalone-gateway");
    let mut child = spawn_with_inherited_socket(binary, &config_path, &stray);
    // The stray descriptor is not claimable by the declared socket: the
    // process must fail closed at bootstrap, leaving nothing listening.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "the gateway must fail closed on a mismatched inherited descriptor"
        );
        match child.try_wait().expect("poll child") {
            Some(status) => {
                assert!(
                    !status.success(),
                    "a leftover inherited descriptor must fail the start"
                );
                break;
            }
            None => {
                // The child may still be validating; confirm it never serves.
                if std::net::TcpStream::connect(("127.0.0.1", declared_port)).is_ok() {
                    panic!("the gateway served despite a mismatched inherited descriptor");
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = stray_port;
}
