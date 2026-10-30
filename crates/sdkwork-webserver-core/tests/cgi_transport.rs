//! Typed-plane transport configuration for the CGI family (Wave 7A):
//! `*_bind`, `*_socket_keepalive`, and unix-socket targets confined to
//! `[http] unixSocketRoot`.

use sdkwork_webserver_core::{
    load_and_compile_webserver_config, load_and_compile_webserver_config_json, ResourceConfig,
    WebServerAppConfig, WebServerConfigError,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn base(nginx: Value, resources: Value, upstreams: Value) -> Value {
    let resources = if resources.is_array() {
        resources
    } else {
        json!([resources])
    };
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-test-web",
        "nginx": nginx,
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": 18080,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "main",
        }],
        "upstreams": upstreams,
        "resources": resources,
        "virtualHosts": [{
            "id": "main",
            "listenerRefs": ["http"],
            "serverNames": ["transport.example.com"],
            "routes": [{
                "id": "main-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "cgi",
            }],
        }],
    })
}

fn fastcgi_resource(extra: Value) -> Value {
    let mut resource = json!({
        "id": "cgi",
        "type": "fast-cgi",
        "upstreamRef": "backend",
    });
    if let (Some(target), Some(extra)) = (resource.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }
    resource
}

fn unix_upstream() -> Value {
    json!([{
        "id": "backend",
        "targets": [{"url": "unix:/var/run/sdkwork/php.sock"}],
    }])
}

fn compile_json(
    config: Value,
) -> Result<sdkwork_webserver_core::CompiledWebServerApp, WebServerConfigError> {
    let config: WebServerAppConfig =
        serde_json::from_value(config).expect("deserialize typed transport config");
    load_and_compile_webserver_config_json(&config)
}

fn diagnostics(error: WebServerConfigError) -> Vec<String> {
    error
        .diagnostics()
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.path, diagnostic.message))
        .collect()
}

#[test]
fn unix_socket_targets_require_the_typed_confinement_root() {
    let diagnostics = diagnostics(
        compile_json(base(
            json!({"enabled": true}),
            fastcgi_resource(json!({})),
            unix_upstream(),
        ))
        .expect_err("unix target without unixSocketRoot must fail closed"),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("unixSocketRoot")),
        "{diagnostics:?}"
    );
}

#[test]
fn unix_socket_targets_must_stay_inside_the_root() {
    for (root, expected) in [
        ("/var/run/sdkwork", None),
        ("/var/run", None), // parent: the same socket is still inside /var/run
        ("/var/run/other", Some("strict descendant")),
        // The root itself is not a socket path: a strict descendant is required.
        ("/var/run/sdkwork/php.sock", Some("strict descendant")),
    ] {
        let result = compile_json(base(
            json!({"enabled": true, "unixSocketRoot": root}),
            fastcgi_resource(json!({})),
            unix_upstream(),
        ));
        match expected {
            None => {
                result.unwrap_or_else(|error| {
                    panic!(
                        "root {root} should confine the socket: {:?}",
                        diagnostics(error)
                    )
                });
            }
            Some(expected) => {
                let diagnostics = diagnostics(result.expect_err("must fail closed"));
                assert!(
                    diagnostics.iter().any(|message| message.contains(expected)),
                    "root {root}: {diagnostics:?}"
                );
            }
        }
    }
}

#[test]
fn unix_socket_paths_reject_traversal_relative_and_root_forms() {
    for (target, root) in [
        ("unix:/var/run/sdkwork/../escape.sock", "/var/run/sdkwork"),
        ("unix:relative.sock", "/var/run/sdkwork"),
        ("unix://host/var/run/sdkwork/php.sock", "/var/run/sdkwork"),
        ("unix:/var/run/sdkwork/php.sock?x=1", "/var/run/sdkwork"),
    ] {
        let diagnostics = diagnostics(
            compile_json(base(
                json!({"enabled": true, "unixSocketRoot": root}),
                fastcgi_resource(json!({})),
                json!([{"id": "backend", "targets": [{"url": target}]}]),
            ))
            .expect_err("malformed unix target must fail closed"),
        );
        assert!(
            diagnostics
                .iter()
                .any(|message| message.contains("unix upstream")),
            "{target}: {diagnostics:?}"
        );
    }

    for root in ["relative/dir", "/", "/var/run/../etc", "/var/run/sdkwork/"] {
        let diagnostics = diagnostics(
            compile_json(base(
                json!({"enabled": true, "unixSocketRoot": root}),
                fastcgi_resource(json!({})),
                unix_upstream(),
            ))
            .expect_err("invalid unixSocketRoot must fail closed"),
        );
        assert!(
            diagnostics
                .iter()
                .any(|message| message.contains("unixSocketRoot must be")),
            "{root}: {diagnostics:?}"
        );
    }
}

#[test]
fn unix_socket_targets_are_limited_to_the_cgi_family() {
    let diagnostics = diagnostics(
        compile_json(base(
            json!({"enabled": true, "unixSocketRoot": "/var/run/sdkwork"}),
            json!([{
                "id": "web",
                "type": "proxy",
                "upstreamRef": "backend",
            }]),
            unix_upstream(),
        ))
        .expect_err("proxy over a unix upstream must fail closed"),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("only fastcgi/uwsgi/scgi/memcached")),
        "{diagnostics:?}"
    );
}

#[test]
fn a_source_bind_does_not_apply_to_a_unix_socket_target() {
    let diagnostics = diagnostics(
        compile_json(base(
            json!({"enabled": true, "unixSocketRoot": "/var/run/sdkwork"}),
            fastcgi_resource(json!({"bindAddress": "127.0.0.2"})),
            unix_upstream(),
        ))
        .expect_err("bindAddress with a unix target must fail closed"),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("does not apply to a unix-socket")),
        "{diagnostics:?}"
    );
}

#[test]
fn typed_bind_and_socket_keepalive_reach_the_compiled_resource() {
    let app = compile_json(base(
        json!({"enabled": true}),
        fastcgi_resource(json!({"bindAddress": "127.0.0.2", "socketKeepalive": true})),
        json!([{
            "id": "backend",
            "targets": [{"url": "http://127.0.0.1:9001"}],
            "addressPolicy": {"allowedCidrs": ["127.0.0.1/32"]},
        }]),
    ))
    .expect("compile bind/keepalive config");
    let ResourceConfig::FastCgi {
        bind_address,
        socket_keepalive,
        ..
    } = &app.config().resources[0]
    else {
        panic!("fastcgi resource");
    };
    assert_eq!(
        *bind_address,
        Some("127.0.0.2".parse().expect("loopback bind"))
    );
    assert_eq!(*socket_keepalive, Some(true));
}

#[test]
fn the_checked_in_schema_accepts_the_new_transport_fields() {
    let directory = TempDir::new().expect("temp dir");
    let path = directory.path().join("sdkwork.webserver.config.json");
    let config = base(
        json!({"enabled": true, "unixSocketRoot": "/var/run/sdkwork"}),
        fastcgi_resource(json!({
            "bindAddress": "127.0.0.2",
            "socketKeepalive": true
        })),
        // A TCP target keeps the bind legal; the unix target itself is covered
        // by the typed-config tests above.
        json!([{
            "id": "backend",
            "targets": [{"url": "http://127.0.0.1:9001"}],
            "addressPolicy": {"allowedCidrs": ["127.0.0.1/32"]},
        }]),
    );
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write typed config");
    let app = load_and_compile_webserver_config(&path)
        .expect("JSON Schema accepts unixSocketRoot/bindAddress/socketKeepalive");
    assert_eq!(
        app.config().nginx.unix_socket_root.as_deref(),
        Some("/var/run/sdkwork")
    );
}
