//! nginx virtual-host selection parity (PRD-nginx-compatibility §5).
//!
//! The expected results were captured from the pinned nginx 1.29.6 oracle
//! (`tests/nginx/differential-host`), not inferred from documentation:
//! exact wins over every wildcard; the longest leading wildcard wins; the
//! longest trailing wildcard wins; leading wildcards match subdomains of any
//! depth; a dot-prefixed name matches the bare name and any subdomain;
//! Wildcards outrank regexes; regexes are evaluated in authored order; and the
//! lower-cased host is what a regex sees.

use sdkwork_webserver_core::{
    load_and_compile_webserver_config_json, ResourceConfig, WebServerAppConfig,
    WebServerConfigError,
};
use serde_json::{json, Value};

fn respond(id: &str, body: &str) -> Value {
    json!({"id": id, "type": "respond", "status": 200, "body": body})
}

fn host(id: &str, server_names: &[&str], resource: &str) -> Value {
    json!({
        "id": id,
        "listenerRefs": ["http"],
        "serverNames": server_names,
        "routes": [{
            "id": format!("{id}-route"),
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": resource,
        }],
    })
}

/// Compile a two-listener configuration (`http` and `tls`) whose default
/// server is the first declared host.
fn compile(
    hosts: Vec<Value>,
) -> Result<sdkwork_webserver_core::CompiledWebServerApp, WebServerConfigError> {
    let mut resources = vec![respond("default-response", "default")];
    let mut virtual_hosts = Vec::new();
    for (index, host) in hosts.into_iter().enumerate() {
        let id = host["id"].as_str().expect("host id").to_owned();
        let resource = format!("{id}-resource");
        resources.push(respond(&resource, &id));
        let mut host = host;
        host["routes"][0]["resourceRef"] = json!(resource);
        if index == 0 {
            host["listenerRefs"] = json!(["http"]);
        }
        virtual_hosts.push(host);
    }
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-test-web",
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": 18080,
            "protocols": ["http1"],
            "defaultVirtualHostRef": virtual_hosts[0]["id"],
        }],
        "resources": resources,
        "virtualHosts": virtual_hosts,
    });
    let config: WebServerAppConfig =
        serde_json::from_value(config).expect("deserialize selection config");
    load_and_compile_webserver_config_json(&config)
}

fn selected_host(app: &sdkwork_webserver_core::CompiledWebServerApp, authority: &str) -> String {
    app.select_route("http", authority, "/", "GET")
        .unwrap_or_else(|| panic!("no host selected for {authority:?}"))
        .virtual_host
        .id
        .clone()
}

#[test]
fn exact_host_wins_over_every_wildcard_and_is_case_insensitive() {
    let app = compile(vec![
        host("default", &["unmatched.invalid"], "default"),
        host("exact", &["exact.example.com"], "exact"),
        host("lead", &["*.example.com"], "lead"),
        host("trail", &["exact.example.*"], "trail"),
        host("regex", &[r"~^exact\.example\.com$"], "regex"),
    ])
    .expect("compile");
    assert_eq!(selected_host(&app, "exact.example.com"), "exact");
    assert_eq!(selected_host(&app, "EXACT.EXAMPLE.COM"), "exact");
    assert_eq!(selected_host(&app, "exact.example.com:8443"), "exact");
    assert_eq!(selected_host(&app, "exact.example.com."), "exact");
}

#[test]
fn longest_leading_wildcard_matches_subdomains_of_any_depth() {
    let app = compile(vec![
        host("default", &["unmatched.invalid"], "default"),
        host("short", &["*.example.com"], "short"),
        host("long", &["*.sub.example.com"], "long"),
        host("deeper", &["*.deep.sub.example.com"], "deeper"),
    ])
    .expect("compile");
    assert_eq!(selected_host(&app, "x.example.com"), "short");
    assert_eq!(selected_host(&app, "x.sub.example.com"), "long");
    // Both `*.example.com` and `*.sub.example.com` match; the longest wins.
    assert_eq!(selected_host(&app, "x.y.sub.example.com"), "long");
    assert_eq!(selected_host(&app, "x.deep.sub.example.com"), "deeper");
    // The bare suffix domain is not a subdomain of the wildcard.
    assert_eq!(selected_host(&app, "example.com"), "default");
}

#[test]
fn longest_trailing_wildcard_matches_any_suffix_below_the_prefix() {
    let app = compile(vec![
        host("default", &["unmatched.invalid"], "default"),
        host("short", &["www.*"], "short"),
        host("long", &["www.example.*"], "long"),
    ])
    .expect("compile");
    assert_eq!(selected_host(&app, "www.example.com"), "long");
    assert_eq!(selected_host(&app, "www.example.co.uk"), "long");
    assert_eq!(selected_host(&app, "www.other.org"), "short");
    assert_eq!(selected_host(&app, "xwww.example.com"), "default");
    assert_eq!(selected_host(&app, "example.com"), "default");
}

#[test]
fn dot_prefix_matches_the_bare_name_and_any_subdomain() {
    let app = compile(vec![
        host("default", &["unmatched.invalid"], "default"),
        host("dot", &[".example.com"], "dot"),
    ])
    .expect("compile");
    assert_eq!(selected_host(&app, "example.com"), "dot");
    assert_eq!(selected_host(&app, "a.example.com"), "dot");
    assert_eq!(selected_host(&app, "a.b.c.example.com"), "dot");
    assert_eq!(selected_host(&app, "xexample.com"), "default");
}

#[test]
fn wildcards_outrank_regexes_and_regexes_follow_authored_order() {
    let app = compile(vec![
        host("default", &["unmatched.invalid"], "default"),
        host("lead", &["*.mix.example"], "lead"),
        host("trail", &["trail.mix.*"], "trail"),
        host(
            "regex-first",
            &[r"~^order\.example\.com$", r"~^order\."],
            "regex-first",
        ),
        host(
            "regex-second",
            &[r"~*^ORDER\.EXAMPLE\.COM$"],
            "regex-second",
        ),
    ])
    .expect("compile");
    // A leading wildcard beats a matching regex.
    assert_eq!(selected_host(&app, "a.mix.example"), "lead");
    // `trail.mix.example` matches both `*.mix.example` and `trail.mix.*`; the
    // leading-wildcard table is searched first (PRD §5 order).
    assert_eq!(selected_host(&app, "trail.mix.example"), "lead");
    // A trailing wildcard beats a matching regex when no leading wildcard
    // also matches.
    assert_eq!(selected_host(&app, "trail.mix.other"), "trail");
    // The first authored regex wins even though the second also matches.
    assert_eq!(selected_host(&app, "order.example.com"), "regex-first");
    // The request host is lower-cased before the regex is evaluated; nginx
    // also compiles a pattern containing ASCII uppercase case-insensitively.
    assert_eq!(selected_host(&app, "ORDER.EXAMPLE.COM"), "regex-first");
}

#[test]
fn unmatched_hosts_fall_back_to_the_listener_default_server() {
    let app = compile(vec![
        host("default", &["unmatched.invalid"], "default"),
        host("exact", &["exact.example.com"], "exact"),
    ])
    .expect("compile");
    assert_eq!(selected_host(&app, "nobody.invalid"), "default");
    assert_eq!(selected_host(&app, ""), "default");
}

#[test]
fn named_locations_are_resolved_by_name_and_never_by_request_path() {
    let config: WebServerAppConfig = serde_json::from_value(json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-test-web",
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": 18080,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "main",
        }],
        "resources": [
            respond("main-resource", "main"),
            respond("fallback-resource", "fallback"),
            {
                "id": "try-files-root",
                "type": "static",
                "root": "/tmp",
                "indexFiles": ["index.html"],
                "spaFallback": "@fallback"
            },
        ],
        "virtualHosts": [{
            "id": "main",
            "listenerRefs": ["http"],
            "serverNames": ["named.example.com"],
            "errorPages": [{"codes": [404], "uri": "@fallback", "fromTarget": true}],
            "routes": [
                {
                    "id": "main-route",
                    "match": {"pathType": "prefix", "path": "/"},
                    "resourceRef": "try-files-root",
                },
                {
                    "id": "named-fallback",
                    "match": {"pathType": "exact", "path": "@fallback"},
                    "resourceRef": "fallback-resource",
                },
            ],
        }],
    }))
    .expect("deserialize named-location config");
    let app = load_and_compile_webserver_config_json(&config).expect("compile");

    // Request-path selection never resolves a named route.
    let selected = app
        .select_route("http", "named.example.com", "/", "GET")
        .expect("path route");
    assert_eq!(selected.route.id, "main-route");
    let ResourceConfig::Static { spa_fallback, .. } = selected.resource else {
        panic!("expected static resource");
    };
    assert_eq!(spa_fallback.as_deref(), Some("@fallback"));
    assert!(
        app.select_route("http", "named.example.com", "@fallback", "GET")
            .is_none(),
        "a named location is not a request path"
    );

    // Only the named-target lookup resolves it, inside the same virtual host.
    let named = app
        .select_named_route("http", "named.example.com", "@fallback", "GET")
        .expect("named location");
    assert_eq!(named.route.id, "named-fallback");
    assert_eq!(named.virtual_host.id, "main");
    assert!(app
        .select_named_route("http", "named.example.com", "@missing", "GET")
        .is_none());
}

#[test]
fn error_page_to_a_named_proxy_target_fails_closed() {
    let config: WebServerAppConfig = serde_json::from_value(json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-test-web",
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": 18080,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "main",
        }],
        "upstreams": [{
            "id": "backend",
            "targets": [{"url": "http://127.0.0.1:9001"}],
        }],
        "resources": [
            respond("main-resource", "main"),
            {
                "id": "proxy-resource",
                "type": "proxy",
                "upstreamRef": "backend",
            },
        ],
        "virtualHosts": [{
            "id": "main",
            "listenerRefs": ["http"],
            "serverNames": ["named.example.com"],
            "errorPages": [{"codes": [404], "uri": "@backend"}],
            "routes": [
                {
                    "id": "main-route",
                    "match": {"pathType": "prefix", "path": "/"},
                    "resourceRef": "main-resource",
                },
                {
                    "id": "named-backend",
                    "match": {"pathType": "exact", "path": "@backend"},
                    "resourceRef": "proxy-resource",
                },
            ],
        }],
    }))
    .expect("deserialize config");
    let diagnostics = validation_error(config);
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("cannot target the named proxy location")),
        "{diagnostics:?}"
    );
}

fn config_with_hosts(hosts: Vec<Value>) -> WebServerAppConfig {
    let mut resources = vec![respond("default-response", "default")];
    let mut virtual_hosts = Vec::new();
    for host in hosts {
        let id = host["id"].as_str().expect("host id").to_owned();
        let resource = format!("{id}-resource");
        resources.push(respond(&resource, &id));
        let mut host = host;
        host["routes"][0]["resourceRef"] = json!(resource);
        virtual_hosts.push(host);
    }
    serde_json::from_value(json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-test-web",
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": 18080,
            "protocols": ["http1"],
            "defaultVirtualHostRef": virtual_hosts[0]["id"],
        }],
        "resources": resources,
        "virtualHosts": virtual_hosts,
    }))
    .expect("deserialize config")
}

fn validation_error(config: WebServerAppConfig) -> Vec<String> {
    let error = load_and_compile_webserver_config_json(&config).expect_err("must fail closed");
    error
        .diagnostics()
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.path, diagnostic.message))
        .collect()
}

#[test]
fn ambiguous_server_names_are_rejected_instead_of_silently_dropped() {
    // `.example.com` claims the exact name AND the leading wildcard; nginx
    // warns and keeps the first owner, which silently changes selection.
    let diagnostics = validation_error(config_with_hosts(vec![
        host("dot", &[".example.com"], "dot"),
        host("lead", &["*.example.com"], "lead"),
    ]));
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("already owned")),
        "{diagnostics:?}"
    );

    let diagnostics = validation_error(config_with_hosts(vec![
        host("exact", &["example.com"], "exact"),
        host("dot", &[".example.com"], "dot"),
    ]));
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("already owned")),
        "{diagnostics:?}"
    );
}

#[test]
fn invalid_server_name_forms_are_rejected_with_a_diagnostic() {
    for invalid in [
        "www.*.example.com",
        "*lead.example.com",
        "www.example*",
        "~",
        "@fallback",
    ] {
        let diagnostics = validation_error(config_with_hosts(vec![host(
            "invalid",
            &[invalid],
            "invalid",
        )]));
        assert!(
            diagnostics
                .iter()
                .any(|message| message.contains("invalid")),
            "{invalid:?}: {diagnostics:?}"
        );
    }
}

#[test]
fn named_location_references_are_validated_and_chains_fail_closed() {
    let base = |route_resource: &str, extra_routes: Vec<Value>| {
        let mut routes = vec![json!({
            "id": "main-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": route_resource,
        })];
        routes.extend(extra_routes);
        json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": "sdkwork-test-web",
            "listeners": [{
                "id": "http",
                "bind": "127.0.0.1",
                "port": 18080,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main",
            }],
            "resources": [
                respond("main-resource", "main"),
                {
                    "id": "try-files-root",
                    "type": "static",
                    "root": "/tmp",
                    "indexFiles": ["index.html"],
                    "spaFallback": "@fallback"
                },
                respond("fallback-resource", "fallback"),
            ],
            "virtualHosts": [{
                "id": "main",
                "listenerRefs": ["http"],
                "serverNames": ["named.example.com"],
                "routes": routes,
            }],
        })
        .to_string()
    };

    // Unknown named target.
    let config: WebServerAppConfig =
        serde_json::from_str(&base("try-files-root", vec![])).expect("deserialize");
    let diagnostics = validation_error(config);
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("not a declared named location")),
        "{diagnostics:?}"
    );

    // Named location chaining into another named location.
    let config: WebServerAppConfig = serde_json::from_str(&base(
        "try-files-root",
        vec![json!({
            "id": "named-fallback",
            "match": {"pathType": "exact", "path": "@fallback"},
            "resourceRef": "try-files-root",
        })],
    ))
    .expect("deserialize");
    let diagnostics = validation_error(config);
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("internal redirect depth bound")),
        "{diagnostics:?}"
    );
}
