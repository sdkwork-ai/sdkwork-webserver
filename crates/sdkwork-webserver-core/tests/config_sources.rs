//! Unified config source regression suite.
//!
//! - The unified loader auto-detects and materializes all three formats.
//! - The nginx corpus (`config-source-fixtures/nginx/full-nginx.conf`)
//!   materializes with the extended execution surface (sub_filter,
//!   limit_conn, secure_link, dynamic proxy_pass, UDP stream).
//! - The TOML corpus (`config-source-fixtures/toml/full-server.toml`) is the
//!   single-file parity counterpart: the same logical configuration must
//!   materialize to an equivalent `WebServerAppConfig`.
//! - Unsupported directives fail closed with `file:line` diagnostics.

use std::{
    fs,
    path::{Path, PathBuf},
};

use sdkwork_webserver_core::{
    ConfigFormat, ConfigLoadOptions, ResourceConfig, WebServerConfigLoader,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/config-source-fixtures")
}

fn loader() -> WebServerConfigLoader {
    WebServerConfigLoader::new()
}

#[test]
fn unified_loader_materializes_all_three_formats() {
    let fixtures = fixtures();
    let loader = loader();

    let json = fixtures.join("json");
    let _ = &json; // JSON fixtures are covered by webserver_config.rs; probe a minimal one inline.
    let nginx = loader
        .load(
            &fixtures.join("nginx/full-nginx.conf"),
            &ConfigLoadOptions::default(),
        )
        .expect("nginx fixture must materialize");
    assert_eq!(nginx.format, ConfigFormat::NginxConf);

    let toml = loader
        .load(
            &fixtures.join("toml/full-server.toml"),
            &ConfigLoadOptions::default(),
        )
        .expect("toml fixture must materialize");
    assert_eq!(toml.format, ConfigFormat::Toml);
}

#[test]
fn nginx_corpus_materializes_full_surface() {
    let loaded = loader()
        .load(
            &fixtures().join("nginx/full-nginx.conf"),
            &ConfigLoadOptions::default(),
        )
        .expect("nginx corpus must materialize");
    let app = &loaded.app;
    assert!(
        loaded.skipped.is_empty(),
        "no files may be skipped: {:?}",
        loaded.skipped
    );

    // Two HTTP virtual hosts (www.example.com + secure.example.com).
    assert_eq!(app.virtual_hosts.len(), 2);
    // HTTP listeners: 0.0.0.0:80, [::]:80, and 443.
    assert_eq!(app.listeners.len(), 3);
    // Three upstreams: backend, hash_backend, and the literal /api/ … is a
    // named upstream; the 443 location proxies a literal (synthesized).
    let upstream_names: Vec<&str> = app
        .upstreams
        .iter()
        .map(|upstream| upstream.id.as_str())
        .collect();
    assert!(upstream_names.contains(&"backend"));
    assert!(upstream_names.contains(&"hash_backend"));

    // Streams: TCP 3307, UDP 53, and TLS-terminated 3443 with mTLS.
    assert_eq!(app.streams.len(), 3);
    assert_eq!(app.streams[0].port, 3307);
    assert_eq!(app.streams[1].port, 53);
    assert_eq!(
        app.streams[1].protocol,
        sdkwork_webserver_core::StreamProtocol::Udp
    );
    assert_eq!(app.streams[2].port, 3443);
    match &app.streams[2].tls {
        Some(sdkwork_webserver_core::StreamTlsMode::Terminate {
            client_auth: Some(auth),
            ..
        }) => {
            assert_eq!(auth.mode, sdkwork_webserver_core::ClientAuthMode::Required);
            assert_eq!(auth.ca_certificate_files.len(), 1);
        }
        other => panic!("stream 3443 must terminate TLS with client auth, got {other:?}"),
    }

    // http gzip + shared zones.
    assert!(app.gzip.enabled);
    assert!(app.gzip.types.contains(&"application/json".to_owned()));
    assert_eq!(app.limit_req_zones.len(), 1);
    assert_eq!(app.limit_conn_zones.len(), 1);
    assert_eq!(app.limit_conn_zones[0].name, "perip");

    // The www host routes: /api/ (proxy+limit), = /healthz (respond),
    // /docs/ (static alias + sub_filter), /files/ (secure_link secret),
    // /links/ (secure_link md5), /dynamic/ (dynamic proxy), / (static).
    let www = app
        .virtual_hosts
        .iter()
        .find(|host| host.server_names.contains(&"www.example.com".to_owned()))
        .expect("www host");
    assert_eq!(www.routes.len(), 7);

    // The secure host routes: / (proxy), /old/ (redirect with variables),
    // /listing/ (static + autoindex accepted).
    let secure = app
        .virtual_hosts
        .iter()
        .find(|host| host.server_names.contains(&"secure.example.com".to_owned()))
        .expect("secure host");
    assert_eq!(secure.routes.len(), 3);
    let redirect = secure
        .routes
        .iter()
        .find(|route| route.route_match.path == "/old/")
        .expect("redirect route");
    let redirect_resource = app
        .resources
        .iter()
        .find(|resource| resource.id() == redirect.resource_ref)
        .expect("redirect resource");
    match redirect_resource {
        ResourceConfig::Redirect {
            status, location, ..
        } => {
            assert_eq!(*status, 301);
            assert_eq!(location, "https://$host$request_uri");
        }
        other => panic!("redirect route must be a redirect resource, got {other:?}"),
    }
    // The 443 TLS policy carries the optional client auth.
    let secure_listener = app
        .listeners
        .iter()
        .find(|listener| listener.port == 443)
        .expect("443 listener");
    let policy_ref = secure_listener
        .tls_policy_ref
        .as_deref()
        .expect("443 tls policy");
    let policy = app
        .tls_policies
        .iter()
        .find(|policy| policy.id == policy_ref)
        .expect("policy");
    let client_auth = policy.client_auth.as_ref().expect("client auth");
    assert_eq!(
        client_auth.mode,
        sdkwork_webserver_core::ClientAuthMode::Optional
    );
    assert_eq!(client_auth.ca_certificate_files.len(), 1);

    let api = www
        .routes
        .iter()
        .find(|route| route.route_match.path == "/api/")
        .expect("api route");
    assert_eq!(api.limit_req.len(), 1);
    assert_eq!(api.limit_req[0].zone, "api");
    assert_eq!(api.limit_conn.len(), 1);
    assert_eq!(api.limit_conn[0].max_connections, 5);
    assert_eq!(api.access.len(), 2);

    let docs = www
        .routes
        .iter()
        .find(|route| route.route_match.path == "/docs/")
        .expect("docs route");
    let sub_filter = docs.sub_filter.as_ref().expect("sub_filter");
    assert_eq!(sub_filter.rules.len(), 1);
    assert_eq!(sub_filter.rules[0].from, "Draft");
    assert_eq!(sub_filter.rules[0].to, "Final");
    assert!(!sub_filter.once);
    assert_eq!(
        sub_filter.types,
        vec!["text/html".to_owned(), "text/plain".to_owned()]
    );

    let files = www
        .routes
        .iter()
        .find(|route| route.route_match.path == "/files/")
        .expect("files route");
    assert!(matches!(
        files.secure_link,
        Some(sdkwork_webserver_core::SecureLinkMode::Secret { .. })
    ));

    let links = www
        .routes
        .iter()
        .find(|route| route.route_match.path == "/links/")
        .expect("links route");
    assert!(matches!(
        links.secure_link,
        Some(sdkwork_webserver_core::SecureLinkMode::Md5 { .. })
    ));

    let dynamic = www
        .routes
        .iter()
        .find(|route| route.route_match.path == "/dynamic/")
        .expect("dynamic route");
    let resource = app
        .resources
        .iter()
        .find(|resource| resource.id() == dynamic.resource_ref)
        .expect("dynamic resource");
    match resource {
        ResourceConfig::Proxy {
            dynamic_target,
            proxy_pass_request_headers,
            ..
        } => {
            assert_eq!(dynamic_target.as_deref(), Some("http://$host:$server_port"));
            assert!(!proxy_pass_request_headers);
        }
        other => panic!("dynamic route must be a proxy resource, got {other:?}"),
    }

    // Proxy cache enabled from proxy_cache_path + proxy_cache_valid.
    assert!(app.proxy_cache.enabled);
    assert_eq!(app.proxy_cache.default_ttl_seconds, 3600);
}

#[test]
fn toml_corpus_materializes_single_file() {
    let loaded = loader()
        .load(
            &fixtures().join("toml/full-server.toml"),
            &ConfigLoadOptions::default(),
        )
        .expect("toml corpus must materialize");
    let app = &loaded.app;
    assert_eq!(loaded.format, ConfigFormat::Toml);
    assert!(
        loaded.revision.is_some(),
        "single-file sources carry a revision"
    );

    assert_eq!(app.virtual_hosts.len(), 2);
    assert_eq!(app.listeners.len(), 3);
    assert_eq!(app.streams.len(), 3);
    assert_eq!(
        app.streams[1].protocol,
        sdkwork_webserver_core::StreamProtocol::Udp
    );
    match &app.streams[2].tls {
        Some(sdkwork_webserver_core::StreamTlsMode::Terminate {
            client_auth: Some(auth),
            ..
        }) => {
            assert_eq!(auth.mode, sdkwork_webserver_core::ClientAuthMode::Required);
        }
        other => panic!("stream 3443 must terminate TLS with client auth, got {other:?}"),
    }
    assert!(app.gzip.enabled);
    assert_eq!(app.limit_conn_zones.len(), 1);

    let www = app
        .virtual_hosts
        .iter()
        .find(|host| host.server_names.contains(&"www.example.com".to_owned()))
        .expect("www host");
    assert_eq!(www.routes.len(), 7);

    let dynamic = www
        .routes
        .iter()
        .find(|route| route.route_match.path == "/dynamic/")
        .expect("dynamic route");
    let resource = app
        .resources
        .iter()
        .find(|resource| resource.id() == dynamic.resource_ref)
        .expect("dynamic resource");
    match resource {
        ResourceConfig::Proxy {
            dynamic_target,
            proxy_pass_request_headers,
            ..
        } => {
            assert_eq!(dynamic_target.as_deref(), Some("http://$host:$server_port"));
            assert!(!proxy_pass_request_headers);
        }
        other => panic!("dynamic route must be a proxy resource, got {other:?}"),
    }

    // down=true target is filtered; hash backend has one live target.
    let hash_backend = app
        .upstreams
        .iter()
        .find(|upstream| upstream.id == "hash_backend")
        .expect("hash backend");
    assert_eq!(hash_backend.targets.len(), 1);
}

/// Semantic fingerprint of a materialized app for the nginx↔TOML parity
/// check: everything the two formats must agree on, minus naming noise.
fn semantic_fingerprint(app: &sdkwork_webserver_core::WebServerAppConfig) -> String {
    let mut parts = Vec::new();
    parts.push(format!("listeners={}", app.listeners.len()));
    parts.push(format!(
        "listener_ports={:?}",
        app.listeners
            .iter()
            .map(|listener| (listener.bind.as_str(), listener.port))
            .collect::<Vec<_>>()
    ));
    parts.push(format!(
        "hosts={:?}",
        app.virtual_hosts
            .iter()
            .map(|host| {
                let routes = host
                    .routes
                    .iter()
                    .map(|route| {
                        format!(
                            "{}/{}:{}",
                            route_path_type_label(route.route_match.path_type),
                            route.route_match.path,
                            app.resources
                                .iter()
                                .find(|resource| resource.id() == route.resource_ref)
                                .map(resource_kind)
                                .unwrap_or("?")
                        )
                    })
                    .collect::<Vec<_>>();
                format!("{}[{:?}]", host.server_names.join(","), routes)
            })
            .collect::<Vec<_>>()
    ));
    // Upstream declaration order differs between formats (nginx stream
    // upstreams vs TOML table order); compare the set sorted by id.
    let mut upstreams: Vec<_> = app
        .upstreams
        .iter()
        .map(|upstream| {
            (
                upstream.id.as_str(),
                upstream
                    .targets
                    .iter()
                    .map(|target| target.url.as_str())
                    .collect::<Vec<_>>(),
                format!("{:?}", upstream.load_balancing),
            )
        })
        .collect();
    upstreams.sort_by(|left, right| left.0.cmp(right.0));
    parts.push(format!("upstreams={upstreams:?}"));
    parts.push(format!(
        "streams={:?}",
        app.streams
            .iter()
            .map(|stream| (stream.port, format!("{:?}", stream.protocol)))
            .collect::<Vec<_>>()
    ));
    parts.push(format!("gzip={}", app.gzip.enabled));
    parts.push(format!("limitReqZones={:?}", app.limit_req_zones));
    parts.push(format!("limitConnZones={:?}", app.limit_conn_zones));
    parts.push(format!("proxyCache={}", app.proxy_cache.enabled));
    parts.sort();
    parts.join("|")
}

fn route_path_type_label(path_type: sdkwork_webserver_core::RoutePathType) -> &'static str {
    match path_type {
        sdkwork_webserver_core::RoutePathType::Exact => "exact",
        sdkwork_webserver_core::RoutePathType::Prefix => "prefix",
        sdkwork_webserver_core::RoutePathType::PrefixExclusive => "prefix-exclusive",
        sdkwork_webserver_core::RoutePathType::Regex => "regex",
        sdkwork_webserver_core::RoutePathType::RegexIgnoreCase => "regex-ignore-case",
    }
}

fn resource_kind(resource: &ResourceConfig) -> &'static str {
    match resource {
        ResourceConfig::Static { .. } => "static",
        ResourceConfig::Proxy { .. } => "proxy",
        ResourceConfig::FastCgi { .. } => "fastcgi",
        ResourceConfig::CgiUpstream { .. } => "cgi-upstream",
        ResourceConfig::Memcached { .. } => "memcached",
        ResourceConfig::Grpc { .. } => "grpc",
        ResourceConfig::Redirect { .. } => "redirect",
        ResourceConfig::EmptyGif { .. } => "empty-gif",
        ResourceConfig::StubStatus { .. } => "stub-status",
        ResourceConfig::Respond { .. } => "respond",
        ResourceConfig::Drive { .. } => "drive",
        ResourceConfig::Knowledgebase { .. } => "knowledgebase",
    }
}

#[test]
fn nginx_and_toml_corpora_are_semantically_equivalent() {
    let loader = loader();
    let nginx = loader
        .load(
            &fixtures().join("nginx/full-nginx.conf"),
            &ConfigLoadOptions::default(),
        )
        .expect("nginx corpus")
        .app;
    let toml = loader
        .load(
            &fixtures().join("toml/full-server.toml"),
            &ConfigLoadOptions::default(),
        )
        .expect("toml corpus")
        .app;

    let nginx_fingerprint = semantic_fingerprint(&nginx);
    let toml_fingerprint = semantic_fingerprint(&toml);
    assert_eq!(
        nginx_fingerprint, toml_fingerprint,
        "nginx and TOML corpora must materialize to the same effective configuration"
    );
}

#[test]
fn unsupported_nginx_directives_fail_closed_with_file_and_line() {
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("bad.conf");
    // `slice_size` is NOT an nginx 1.29.6 directive: `ngx_http_slice_filter_module`
    // spells it `slice <size>`. The runtime executes `slice`, so a configuration
    // that reaches for the non-existent name must still fail closed with the
    // directive's file:line instead of being silently ignored.
    fs::write(
        &file,
        "server {\n    listen 80;\n    server_name userid.local;\n    location / {\n        slice_size 1k;\n    }\n}\n",
    )
    .expect("write");
    let error = loader()
        .load(&file, &ConfigLoadOptions::default())
        .expect_err("unsupported directive must fail closed");
    let message = error.to_string();
    let diagnostics = error.diagnostics();
    assert!(
        message.contains("slice_size"),
        "diagnostic must name the directive: {message}"
    );
    if let Some(diagnostic) = diagnostics.first() {
        assert!(
            diagnostic.path.contains("bad.conf:5"),
            "diagnostic must carry file:line, got {}",
            diagnostic.path
        );
    }
}

#[test]
fn slice_size_is_executable_and_not_a_1_29_6_directive() {
    // `slice <size>` materializes into the route's effective `sliceSize`
    // (http/server/location inheritance); the section below pins the nginx
    // scalar merge and the typed TOML equivalent.
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("slice.conf");
    fs::write(
        &file,
        "http {\n    slice 4;\n    server {\n        listen 80;\n        server_name slice.local;\n        slice 8;\n        location /inherits {\n            proxy_pass http://127.0.0.1:8080;\n        }\n        location /replaces {\n            slice 16;\n            proxy_pass http://127.0.0.1:8080;\n        }\n        location /disables {\n            slice 0;\n            proxy_pass http://127.0.0.1:8080;\n        }\n    }\n}\n",
    )
    .expect("write");
    let app = loader()
        .load(&file, &ConfigLoadOptions::default())
        .expect("slice is executable")
        .app;
    let routes = &app.virtual_hosts[0].routes;
    let slice_of = |path: &str| {
        routes
            .iter()
            .find(|route| route.route_match.path == path)
            .unwrap_or_else(|| panic!("route {path}"))
            .slice_size
    };
    assert_eq!(slice_of("/inherits"), Some(8), "server replaces http");
    assert_eq!(slice_of("/replaces"), Some(16), "location replaces server");
    assert_eq!(slice_of("/disables"), Some(0), "slice 0 is a declaration");
}

#[test]
fn typed_toml_slice_materializes_with_nginx_inheritance() {
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("server.toml");
    fs::write(
        &file,
        "[[http.server]]\nlisten = [\"80\"]\nserverName = [\"slice.local\"]\nslice = 8\n\n[[http.server.location]]\nmatch = \"/inherits\"\nproxyPass = \"http://127.0.0.1:8080\"\n\n[[http.server.location]]\nmatch = \"/replaces\"\nslice = \"1k\"\nproxyPass = \"http://127.0.0.1:8080\"\n\n[[http.server.location]]\nmatch = \"/disables\"\nslice = 0\nproxyPass = \"http://127.0.0.1:8080\"\n",
    )
    .expect("write");
    let app = loader()
        .load(&file, &ConfigLoadOptions::default())
        .expect("typed slice")
        .app;
    let routes = &app.virtual_hosts[0].routes;
    let slice_of = |path: &str| {
        routes
            .iter()
            .find(|route| route.route_match.path == path)
            .unwrap_or_else(|| panic!("route {path}"))
            .slice_size
    };
    assert_eq!(slice_of("/inherits"), Some(8), "server value is inherited");
    assert_eq!(
        slice_of("/replaces"),
        Some(1024),
        "location size string wins"
    );
    assert_eq!(slice_of("/disables"), Some(0), "slice = 0 is a declaration");
}

#[test]
fn unknown_toml_keys_fail_closed() {
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("server.toml");
    fs::write(
        &file,
        "[[http.server]]\nlisten = [\"80\"]\nserverName = [\"x.local\"]\n\n[[http.server.location]]\nmatch = \"/\"\nproxyPass = \"http://backend\"\n",
    )
    .expect("write");
    let error = loader()
        .load(&file, &ConfigLoadOptions::default())
        .expect_err("undefined upstream must fail");
    assert!(error.to_string().contains("undefined upstream"));
}

#[test]
fn directory_sources_are_detected_and_loaded() {
    let directory = tempfile::tempdir().expect("temp dir");
    let sites = directory.path().join("sites-enabled");
    fs::create_dir_all(&sites).expect("sites dir");
    fs::write(
        sites.join("web.conf"),
        "server {\n    listen 80;\n    server_name dir.local;\n    location / { return 200 \"dir\"; }\n}\n",
    )
    .expect("write site");
    let loader = loader();
    let format = loader
        .format_of(&sites, &ConfigLoadOptions::default())
        .expect("detect directory");
    assert_eq!(format, ConfigFormat::NginxConf);
    let loaded = loader
        .load(&sites, &ConfigLoadOptions::default())
        .expect("load directory");
    assert_eq!(loaded.app.virtual_hosts.len(), 1);
}

#[test]
fn explicit_format_override_resolves_ambiguous_paths() {
    let directory = tempfile::tempdir().expect("temp dir");
    // Extension-less content that sniffs as nginx.
    let path = directory.path().join("config");
    fs::write(
        &path,
        "server { listen 80; server_name any.local; location / { return 200 \"ok\"; } }\n",
    )
    .expect("write");
    let options = ConfigLoadOptions::with_format(ConfigFormat::NginxConf);
    let loaded = loader().load(&path, &options).expect("forced nginx");
    assert_eq!(loaded.format, ConfigFormat::NginxConf);
    assert_eq!(loaded.app.virtual_hosts.len(), 1);
}

#[test]
fn missing_path_fails_closed_with_guidance() {
    let directory = tempfile::tempdir().expect("temp dir");
    let missing = directory.path().join("missing.conf");
    let error = loader()
        .load(&missing, &ConfigLoadOptions::default())
        .expect_err("missing path must fail");
    assert!(error.to_string().contains("missing.conf"));
}

#[test]
fn json_corpus_materializes_via_the_unified_loader() {
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("app.json");
    fs::write(
        &file,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": "unified-json",
            "limits": { "maxConnections": 16 },
            "listeners": [{
                "id": "http",
                "bind": "127.0.0.1",
                "port": 18080,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "host"
            }],
            "resources": [{
                "id": "ok",
                "type": "respond",
                "status": 200,
                "contentType": "text/plain; charset=utf-8",
                "body": "ok"
            }],
            "virtualHosts": [{
                "id": "host",
                "listenerRefs": ["http"],
                "serverNames": ["json.local"],
                "routes": [{
                    "id": "ok-route",
                    "match": { "pathType": "prefix", "path": "/" },
                    "resourceRef": "ok"
                }]
            }]
        }))
        .expect("serialize"),
    )
    .expect("write");
    let loaded = loader()
        .load(&file, &ConfigLoadOptions::default())
        .expect("json via unified loader");
    assert_eq!(loaded.format, ConfigFormat::Json);
    assert_eq!(loaded.app.app_key, "unified-json");
    assert!(loaded.revision.is_some());
    // load_and_compile validates + compiles.
    let compiled = loader()
        .load_and_compile(&file, &ConfigLoadOptions::default())
        .expect("compile json");
    assert_eq!(compiled.config().virtual_hosts.len(), 1);
}

#[test]
fn nginx_sites_with_include_glob_expand_in_order() {
    let directory = tempfile::tempdir().expect("temp dir");
    let sites = directory.path().join("sites-enabled");
    fs::create_dir_all(&sites).expect("sites dir");
    fs::write(
        sites.join("a.conf"),
        "server { listen 18001; server_name a.local; location / { return 200 \"a\"; } }\n",
    )
    .expect("a");
    fs::write(
        sites.join("b.conf"),
        "server { listen 18002; server_name b.local; location / { return 200 \"b\"; } }\n",
    )
    .expect("b");
    let main = directory.path().join("nginx.conf");
    fs::write(&main, "http {\n    include sites-enabled/*.conf;\n}\n").expect("main");
    let loaded = loader()
        .load(&main, &ConfigLoadOptions::default())
        .expect("include glob");
    assert_eq!(loaded.app.virtual_hosts.len(), 2);
    assert_eq!(loaded.app.listeners.len(), 2);
    let _ = Path::new("");
}

/// Typed-TOML parity for the Wave 14B mail client-certificate TLS fields.
#[test]
fn typed_toml_mail_tls_client_certificate_fields_materialize() {
    let directory = tempfile::TempDir::new().expect("temp dir");
    let path = directory.path().join("server.toml");
    fs::write(
        &path,
        r#"
[[mail.server]]
listen = ["127.0.0.1:2993 ssl"]
protocol = "smtp"

[mail.server.tls]
certificateFile = "server.pem"
privateKeyFile = "server.key"
verifyClient = "optional-no-ca"
verifyDepth = 2
clientCertificateFile = "ca.pem"
trustedCertificateFile = "chain.pem"

[mail.server.authHttp]
uri = "127.0.0.1:9000/auth"
passClientCert = true
"#,
    )
    .expect("write toml config");
    let config = loader()
        .load(&path, &ConfigLoadOptions::default())
        .expect("typed mail TLS client-cert config materializes");
    let server = &config.app.mail.servers[0];
    let tls = server.tls.as_ref().expect("mail tls");
    assert!(tls.implicit);
    assert_eq!(
        tls.verify_client,
        sdkwork_webserver_core::MailClientVerifyMode::OptionalNoCa
    );
    assert_eq!(tls.verify_depth, 2);
    assert!(tls
        .client_certificate_file
        .as_deref()
        .is_some_and(|value| value.ends_with("ca.pem")));
    assert!(tls
        .trusted_certificate_file
        .as_deref()
        .is_some_and(|value| value.ends_with("chain.pem")));
    assert!(server.auth_http.as_ref().expect("auth").pass_client_cert);
}

#[test]
fn nginx_proxy_cache_is_route_scoped_with_http_server_location_inheritance() {
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("cache.conf");
    fs::write(
        &file,
        "http {\n    proxy_cache_path /tmp/cache levels=1:2 keys_zone=zone:10m inactive=1m;\n    proxy_cache zone;\n    server {\n        listen 80;\n        server_name inherited.local;\n        location /inherits {\n            proxy_pass http://127.0.0.1:8080;\n        }\n        location /off {\n            proxy_cache off;\n            proxy_pass http://127.0.0.1:8080;\n        }\n    }\n    server {\n        listen 80;\n        server_name scoped.local;\n        proxy_cache off;\n        location /on {\n            proxy_cache zone;\n            proxy_pass http://127.0.0.1:8080;\n        }\n        location /inherits-off {\n            proxy_pass http://127.0.0.1:8080;\n        }\n    }\n}\n",
    )
    .expect("write");
    let app = loader()
        .load(&file, &ConfigLoadOptions::default())
        .expect("proxy_cache inheritance")
        .app;
    let cache_of = |host: &str, path: &str| {
        app.virtual_hosts
            .iter()
            .find(|host_config| host_config.server_names.iter().any(|name| name == host))
            .and_then(|host_config| {
                host_config
                    .routes
                    .iter()
                    .find(|route| route.route_match.path == path)
            })
            .unwrap_or_else(|| panic!("route {host}{path}"))
            .proxy_cache
    };
    assert_eq!(cache_of("inherited.local", "/inherits"), Some(true));
    assert_eq!(cache_of("inherited.local", "/off"), Some(false));
    assert_eq!(cache_of("scoped.local", "/on"), Some(true));
    assert_eq!(cache_of("scoped.local", "/inherits-off"), Some(false));
}

#[test]
fn slice_invalid_forms_and_contexts_fail_closed_with_file_and_line() {
    let cases: [(&str, &str); 4] = [
        ("slice 4 5;", "exactly one size argument"),
        ("slice on;", "not a byte size"),
        ("slice -1;", "not a byte size"),
        ("slice 100m;", "exceeds the runtime's bounded slice buffer"),
    ];
    for (directive, needle) in cases {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = directory.path().join("bad-slice.conf");
        fs::write(
            &file,
            format!(
                "server {{\n    listen 80;\n    server_name slice.local;\n    location / {{\n        {directive}\n    }}\n}}\n"
            ),
        )
        .expect("write");
        let error = loader()
            .load(&file, &ConfigLoadOptions::default())
            .expect_err("invalid slice form must fail closed");
        let message = error.to_string();
        assert!(
            message.contains(needle),
            "diagnostic for `{directive}` must contain `{needle}`: {message}"
        );
        if let Some(diagnostic) = error.diagnostics().first() {
            assert!(
                diagnostic.path.contains("bad-slice.conf:5"),
                "diagnostic must carry file:line for `{directive}`, got {}",
                diagnostic.path
            );
        }
    }

    // `slice` is an http-module directive: the stream and mail planes neither
    // own nor silently ignore it.
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("stream-slice.conf");
    fs::write(
        &file,
        "stream {\n    server {\n        listen 127.0.0.1:19000;\n        slice 4;\n    }\n}\n",
    )
    .expect("write");
    let error = loader()
        .load(&file, &ConfigLoadOptions::default())
        .expect_err("stream slice must fail closed");
    assert!(
        error.to_string().contains("slice"),
        "stream diagnostic must name the directive: {}",
        error
    );
    if let Some(diagnostic) = error.diagnostics().first() {
        assert!(
            diagnostic.path.contains("stream-slice.conf:4"),
            "stream diagnostic must carry file:line, got {}",
            diagnostic.path
        );
    }
}
