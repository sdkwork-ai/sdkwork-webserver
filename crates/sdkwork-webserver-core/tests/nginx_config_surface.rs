//! Systematic nginx configuration surface tests (capability matrix).
//!
//! Every supported nginx directive is exercised through
//! parse → materialize → validate with an assertion on the materialized
//! runtime model; every fail-closed form is asserted to produce a precise
//! diagnostic; stock `nginx.conf` tuning directives are asserted to load;
//! and `include` / `load_nginx_compat` / `merge_nginx_apps` round trips
//! are covered end to end.
//!
//! The matrix mirrors the http-core-v1 profile of
//! `specs/nginx-gap.catalog.json` and the directive surface documented in
//! `crates/sdkwork-webserver-core/src/nginx/mapping.rs`.

use std::path::Path;

use sdkwork_webserver_core::config::{
    ListenerProtocol, ResourceConfig, RoutePathType, StreamTlsMode, UpstreamLoadBalancingStrategy,
    WebServerAppConfig,
};
use sdkwork_webserver_core::nginx::{
    expand_includes, load_nginx_compat, materialize_nginx_app, merge_nginx_apps,
    parse_nginx_config, NginxConfigError,
};

/// Parse + materialize + validate one configuration text.
fn materialize(text: &str) -> Result<WebServerAppConfig, NginxConfigError> {
    let parsed = parse_nginx_config(text, Path::new("site.conf")).expect("parse");
    materialize_nginx_app(&parsed, Path::new("/etc/nginx/sites-enabled"), "surface")
}

fn materialize_ok(text: &str) -> WebServerAppConfig {
    materialize(text).expect("materialize + validate")
}

/// Parse + materialize, expecting a fail-closed diagnostic containing
/// `expected`.
fn materialize_err(text: &str, expected: &str) {
    let error = materialize(text).err().expect("must fail closed");
    let message = error.to_string();
    assert!(
        message.contains(expected),
        "expected diagnostic containing `{expected}`, got: {message}"
    );
}

struct SurfaceCase {
    name: &'static str,
    nginx: &'static str,
    check: fn(&WebServerAppConfig),
}

/// Every supported directive family, materialized and asserted on the
/// runtime model.
const SURFACE: &[SurfaceCase] = &[
    SurfaceCase {
        name: "upstream weighted round-robin with keepalive",
        nginx: r#"
upstream api {
    server 127.0.0.1:9001 weight=2 max_fails=3 fail_timeout=10s;
    server 127.0.0.1:9002 backup;
    keepalive 32;
    keepalive_timeout 60s;
    keepalive_requests 1000;
}
server {
    listen 80;
    server_name api.example.com;
    location / { proxy_pass http://api; }
}
"#,
        check: |config| {
            let upstream = &config.upstreams[0];
            assert_eq!(upstream.id, "api");
            assert_eq!(
                upstream.load_balancing,
                UpstreamLoadBalancingStrategy::RoundRobin
            );
            assert_eq!(upstream.targets.len(), 2);
            assert_eq!(upstream.targets[0].weight, 2);
            assert!(upstream.targets[1].backup);
        },
    },
    SurfaceCase {
        name: "hostname upstream authorizes private Docker CIDRs",
        nginx: r#"
upstream gateway {
    least_conn;
    server gateway:3900;
    keepalive 32;
}
server {
    listen 80;
    server_name api-dev.example.com;
    location / { proxy_pass http://gateway; }
}
"#,
        check: |config| {
            let upstream = &config.upstreams[0];
            assert_eq!(upstream.id, "gateway");
            assert_eq!(upstream.targets[0].url, "http://gateway:3900");
            let cidrs = &upstream.address_policy.allowed_cidrs;
            assert!(
                cidrs.iter().any(|c| c.to_string() == "172.16.0.0/12"),
                "hostname upstream must authorize Docker bridge ranges: {cidrs:?}"
            );
            assert!(
                cidrs.iter().any(|c| c.to_string() == "192.168.0.0/16"),
                "hostname upstream must authorize 192.168.0.0/16: {cidrs:?}"
            );
        },
    },
    SurfaceCase {
        name: "upstream least_conn and random two least_conn",
        nginx: r#"
upstream lc { least_conn; server 127.0.0.1:9001; }
upstream rnd { random two least_conn; server 127.0.0.1:9002; }
server {
    listen 80;
    server_name lb.example.com;
    location /a/ { proxy_pass http://lc; }
    location /b/ { proxy_pass http://rnd; }
}
"#,
        check: |config| {
            let lc = config.upstreams.iter().find(|u| u.id == "lc").unwrap();
            assert_eq!(
                lc.load_balancing,
                UpstreamLoadBalancingStrategy::LeastConnections
            );
            let rnd = config.upstreams.iter().find(|u| u.id == "rnd").unwrap();
            assert_eq!(
                rnd.load_balancing,
                UpstreamLoadBalancingStrategy::RandomTwoLeastConnections
            );
        },
    },
    SurfaceCase {
        name: "upstream ip_hash and hash consistent",
        nginx: r#"
upstream sticky { ip_hash; server 127.0.0.1:8001; }
upstream by_uri { hash $request_uri consistent; server 127.0.0.1:8002; }
server {
    listen 80;
    server_name hash.example.com;
    location /a/ { proxy_pass http://sticky; }
    location /b/ { proxy_pass http://by_uri; }
}
"#,
        check: |config| {
            let sticky = config.upstreams.iter().find(|u| u.id == "sticky").unwrap();
            assert_eq!(sticky.load_balancing, UpstreamLoadBalancingStrategy::IpHash);
            let by_uri = config.upstreams.iter().find(|u| u.id == "by_uri").unwrap();
            assert_eq!(by_uri.load_balancing, UpstreamLoadBalancingStrategy::Hash);
            assert!(by_uri.hash.as_ref().unwrap().consistent);
        },
    },
    SurfaceCase {
        name: "upstream down and max_conns and slow_start",
        nginx: r#"
upstream api {
    server 127.0.0.1:9001 max_conns=64 slow_start=30s;
    server 127.0.0.1:9002 down;
}
server {
    listen 80;
    server_name api.example.com;
    location / { proxy_pass http://api; }
}
"#,
        check: |config| {
            let upstream = &config.upstreams[0];
            assert_eq!(upstream.targets.len(), 1, "down target is filtered out");
            assert_eq!(upstream.targets[0].max_connections, Some(64));
            assert_eq!(upstream.targets[0].slow_start_ms, Some(30_000));
        },
    },
    SurfaceCase {
        name: "listen forms: port, addr:port, [v6]:port, bare address",
        nginx: r#"
server {
    listen 80;
    listen 127.0.0.1:8080;
    listen [::]:80;
    listen [::1]:8081;
    listen 10.0.0.1;
    server_name forms.example.com;
    location / { return 200 "ok"; }
}
"#,
        check: |config| {
            let ports = config
                .listeners
                .iter()
                .map(|l| (l.bind.as_str(), l.port))
                .collect::<Vec<_>>();
            assert!(ports.contains(&("0.0.0.0", 80)));
            assert!(ports.contains(&("127.0.0.1", 8080)));
            assert!(ports.contains(&("::", 80)));
            assert!(ports.contains(&("::1", 8081)));
            assert!(
                ports.contains(&("10.0.0.1", 80)),
                "bare address defaults to port 80: {ports:?}"
            );
        },
    },
    SurfaceCase {
        name: "default server: first server then explicit default_server",
        nginx: r#"
server {
    listen 8080;
    server_name first.example.com;
    location / { return 200 "first"; }
}
server {
    listen 8080 default_server;
    server_name second.example.com;
    location / { return 200 "second"; }
}
"#,
        check: |config| {
            let listener = config.listeners.iter().find(|l| l.port == 8080).unwrap();
            assert_eq!(
                listener.default_virtual_host_ref.as_deref(),
                Some("second-example-com-8080")
            );
        },
    },
    SurfaceCase {
        name: "wildcard server_name materializes and validates",
        nginx: r#"
server {
    listen 80;
    server_name example.com *.example.com;
    location / { return 200 "ok"; }
}
"#,
        check: |config| {
            assert_eq!(
                config.virtual_hosts[0].server_names,
                vec!["example.com", "*.example.com"]
            );
        },
    },
    SurfaceCase {
        name: "ssl listener with SNI-shared 443 across servers",
        nginx: r#"
server {
    listen 443 ssl;
    server_name api.example.com;
    ssl_certificate /etc/ssl/api.pem;
    ssl_certificate_key /etc/ssl/api.key;
    location / { return 200 "api"; }
}
server {
    listen 443 ssl;
    server_name web.example.com;
    ssl_certificate /etc/ssl/web.pem;
    ssl_certificate_key /etc/ssl/web.key;
    location / { return 200 "web"; }
}
"#,
        check: |config| {
            let ssl_listeners = config.listeners.iter().filter(|l| l.port == 443).count();
            assert_eq!(ssl_listeners, 1, "both servers share one 443 listener");
            let policy_id = config
                .listeners
                .iter()
                .find(|l| l.port == 443)
                .unwrap()
                .tls_policy_ref
                .as_deref()
                .unwrap();
            let policy = config
                .tls_policies
                .iter()
                .find(|p| p.id == policy_id)
                .unwrap();
            let certs = policy.certificate_refs().count();
            assert!(certs >= 2, "SNI policy carries both certificates");
            assert_eq!(config.certificates.len(), 2);
        },
    },
    SurfaceCase {
        name: "ssl_verify_client with ssl_client_certificate",
        nginx: r#"
server {
    listen 443 ssl;
    server_name mtls.example.com;
    ssl_certificate /etc/ssl/mtls.pem;
    ssl_certificate_key /etc/ssl/mtls.key;
    ssl_verify_client optional;
    ssl_client_certificate /etc/ssl/ca.pem;
    location / { return 200 "ok"; }
}
"#,
        check: |config| {
            let policy = &config.tls_policies[0];
            let client_auth = policy.client_auth.as_ref().expect("client auth");
            assert_eq!(
                client_auth.mode,
                sdkwork_webserver_core::config::ClientAuthMode::Optional
            );
            assert_eq!(client_auth.ca_certificate_files.len(), 1);
        },
    },
    SurfaceCase {
        name: "http2 via listen flag and via http2 on;",
        nginx: r#"
server {
    listen 443 ssl http2;
    server_name h2flag.example.com;
    ssl_certificate /etc/ssl/a.pem;
    ssl_certificate_key /etc/ssl/a.key;
    location / { return 200 "ok"; }
}
server {
    listen 443 ssl;
    http2 on;
    server_name h2dir.example.com;
    ssl_certificate /etc/ssl/b.pem;
    ssl_certificate_key /etc/ssl/b.key;
    location / { return 200 "ok"; }
}
"#,
        check: |config| {
            let listeners = config
                .listeners
                .iter()
                .filter(|l| l.port == 443)
                .collect::<Vec<_>>();
            assert!(!listeners.is_empty());
            for listener in listeners {
                assert!(
                    listener.protocols.contains(&ListenerProtocol::Http2),
                    "ssl listeners must negotiate h2: {:?}",
                    listener.protocols
                );
            }
        },
    },
    SurfaceCase {
        name: "http-level proxy_set_header inheritance with server override",
        nginx: r#"
http {
    proxy_set_header X-Http $scheme;
    upstream api { server 127.0.0.1:9001; }
    server {
        listen 80;
        server_name headers.example.com;
        location / {
            proxy_set_header X-Server $host;
            proxy_pass http://api;
        }
    }
}
"#,
        check: |config| {
            let proxy = config
                .resources
                .iter()
                .find_map(|r| match r {
                    ResourceConfig::Proxy {
                        request_set_headers,
                        ..
                    } => Some(request_set_headers.clone()),
                    _ => None,
                })
                .unwrap();
            assert!(proxy.contains(&"X-Http $scheme".to_owned()), "{proxy:?}");
            assert!(proxy.contains(&"X-Server $host".to_owned()), "{proxy:?}");
        },
    },
    SurfaceCase {
        name: "proxy_pass URI replacement: strip and targetUri",
        nginx: r#"
upstream api { server 127.0.0.1:9001; }
server {
    listen 80;
    server_name uri.example.com;
    location /api/ { proxy_pass http://api/; }
    location /v1/ { proxy_pass http://127.0.0.1:9002/api; }
    location /legacy/ { proxy_pass http://127.0.0.1:9003; }
}
"#,
        check: |config| {
            let entries = config
                .resources
                .iter()
                .filter_map(|r| match r {
                    ResourceConfig::Proxy {
                        upstream_ref,
                        target_uri,
                        ..
                    } => Some((upstream_ref.clone(), target_uri.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(entries.contains(&("api".to_owned(), Some("/".to_owned()))));
            assert!(
                entries.contains(&("literal-127-0-0-1-9002".to_owned(), Some("/api".to_owned())))
            );
            assert!(entries.contains(&("literal-127-0-0-1-9003".to_owned(), None)));
            let literal = config
                .upstreams
                .iter()
                .find(|u| u.id == "literal-127-0-0-1-9002")
                .unwrap();
            assert_eq!(literal.targets[0].url, "http://127.0.0.1:9002");
        },
    },
    SurfaceCase {
        name: "variable proxy_pass templates materialize as dynamicTarget",
        nginx: r#"
server {
    listen 80;
    server_name dyn.example.com;
    location / { proxy_pass http://$host:8080; }
}
"#,
        check: |config| {
            let proxy = config
                .resources
                .iter()
                .find_map(|r| match r {
                    ResourceConfig::Proxy { dynamic_target, .. } => dynamic_target.clone(),
                    _ => None,
                })
                .expect("dynamic target");
            assert_eq!(proxy, "http://$host:8080");
        },
    },
    SurfaceCase {
        name: "proxy_pass_request_headers off",
        nginx: r#"
server {
    listen 80;
    server_name hdrs.example.com;
    location / {
        proxy_pass_request_headers off;
        proxy_pass http://127.0.0.1:9001;
    }
}
"#,
        check: |config| {
            let proxy = config
                .resources
                .iter()
                .find_map(|r| match r {
                    ResourceConfig::Proxy {
                        proxy_pass_request_headers,
                        ..
                    } => Some(*proxy_pass_request_headers),
                    _ => None,
                })
                .unwrap();
            assert!(!proxy);
        },
    },
    SurfaceCase {
        name: "return forms: code, code+body, 3xx+url, bare URL (302)",
        nginx: r#"
server {
    listen 80;
    server_name ret.example.com;
    location = /a { return 204; }
    location = /b { return 200 "ok"; }
    location = /c { return 301 https://$host$request_uri; }
    location = /d { return /relative-target; }
    location = /e { return 404 "not here"; }
}
"#,
        check: |config| {
            let kinds = config
                .resources
                .iter()
                .map(|r| match r {
                    ResourceConfig::Respond { status, .. } => format!("respond-{status}"),
                    ResourceConfig::Redirect { status, .. } => format!("redirect-{status}"),
                    other => panic!("unexpected {other:?}"),
                })
                .collect::<Vec<_>>();
            assert!(kinds.contains(&"respond-204".to_owned()));
            assert!(kinds.contains(&"respond-200".to_owned()));
            assert!(kinds.contains(&"redirect-301".to_owned()));
            assert!(
                kinds.contains(&"redirect-302".to_owned()),
                "bare URL is a 302: {kinds:?}"
            );
            assert!(kinds.contains(&"respond-404".to_owned()));
        },
    },
    SurfaceCase {
        name: "root appends full path; alias replaces prefix; try_files SPA fallback",
        nginx: r#"
server {
    listen 80;
    server_name static.example.com;
    location /assets/ {
        root /srv/www;
        try_files $uri $uri/ /index.html;
    }
    location /files/ {
        alias /srv/data/;
        index index.html;
    }
}
"#,
        check: |config| {
            let statics = config
                .resources
                .iter()
                .filter_map(|r| match r {
                    ResourceConfig::Static {
                        root,
                        strip_prefix,
                        spa_fallback,
                        ..
                    } => Some((root.clone(), *strip_prefix, spa_fallback.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(statics.contains(&(
                "/srv/www".to_owned(),
                false,
                Some("index.html".to_owned())
            )));
            assert!(statics.contains(&("/srv/data/".to_owned(), true, None)));
        },
    },
    SurfaceCase {
        name: "location matchers: exact, ^~, prefix, ~, ~*",
        nginx: r#"
server {
    listen 80;
    server_name routes.example.com;
    location = /exact { return 200 "e"; }
    location ^~ /exclusive/ { return 200 "x"; }
    location /prefix/ { return 200 "p"; }
    location ~ ^/re/(.*)$ { return 200 "r"; }
    location ~* \.jpg$ { return 200 "j"; }
}
"#,
        check: |config| {
            let matches = config.virtual_hosts[0]
                .routes
                .iter()
                .map(|r| (r.route_match.path_type, r.route_match.path.clone()))
                .collect::<Vec<_>>();
            assert!(matches.contains(&(RoutePathType::Exact, "/exact".to_owned())));
            assert!(matches.contains(&(RoutePathType::PrefixExclusive, "/exclusive/".to_owned())));
            assert!(matches.contains(&(RoutePathType::Prefix, "/prefix/".to_owned())));
            assert!(matches.contains(&(RoutePathType::Regex, "^/re/(.*)$".to_owned())));
            assert!(matches.contains(&(RoutePathType::RegexIgnoreCase, "\\.jpg$".to_owned())));
        },
    },
    SurfaceCase {
        name: "rewrite last/break/redirect/permanent materialize",
        nginx: r#"
server {
    listen 80;
    server_name rw.example.com;
    location /old/ {
        rewrite ^/old/?(.*)$ /$1 last;
        rewrite ^/a$ /b break;
        rewrite ^/c$ /d redirect;
        rewrite ^/e$ /f permanent;
        proxy_pass http://127.0.0.1:9001;
    }
}
"#,
        check: |config| {
            let route = &config.virtual_hosts[0].routes[0];
            assert_eq!(route.rewrite.len(), 4);
            assert_eq!(route.rewrite[0].pattern, "^/old/?(.*)$");
            assert_eq!(route.rewrite[0].replacement, "/$1");
            assert_eq!(
                route.rewrite[0].flag,
                sdkwork_webserver_core::config::RewriteFlag::Last
            );
            assert_eq!(
                route.rewrite[1].flag,
                sdkwork_webserver_core::config::RewriteFlag::Break
            );
            assert_eq!(
                route.rewrite[2].flag,
                sdkwork_webserver_core::config::RewriteFlag::Redirect
            );
            assert_eq!(
                route.rewrite[3].flag,
                sdkwork_webserver_core::config::RewriteFlag::Permanent
            );
        },
    },
    SurfaceCase {
        name: "access allow/deny and limit_req and limit_conn and auth_basic",
        nginx: r#"
limit_req_zone $binary_remote_addr zone=one:10m rate=10r/s;
limit_conn_zone $binary_remote_addr zone=conn:10m;
server {
    listen 80;
    server_name guard.example.com;
    location /admin {
        allow 10.0.0.0/8;
        deny all;
        limit_req zone=one burst=20 nodelay;
        limit_conn conn 10;
        proxy_pass http://127.0.0.1:9001;
    }
}
"#,
        check: |config| {
            assert_eq!(config.limit_req_zones.len(), 1);
            assert_eq!(config.limit_conn_zones.len(), 1);
            let route = &config.virtual_hosts[0].routes[0];
            assert_eq!(route.access.len(), 2);
            assert_eq!(
                route.access[0].action,
                sdkwork_webserver_core::config::AccessAction::Allow
            );
            assert_eq!(route.access[0].network, "10.0.0.0/8");
            assert_eq!(route.limit_req.len(), 1);
            assert_eq!(route.limit_req[0].burst, 20);
            assert!(route.limit_req[0].nodelay);
            assert_eq!(route.limit_conn.len(), 1);
            assert_eq!(route.limit_conn[0].max_connections, 10);
        },
    },
    SurfaceCase {
        name: "sub_filter family and secure_link family materialize",
        nginx: r#"
server {
    listen 80;
    server_name filters.example.com;
    location / {
        sub_filter_once off;
        sub_filter_types text/html application/xhtml+xml;
        sub_filter "old" "new";
        sub_filter_last_modified on;
        proxy_pass http://127.0.0.1:9001;
    }
    location /links {
        secure_link $arg_st;
        secure_link_md5 "$secure_link_expires$uri$remote_addr secret";
        secure_link_expires $arg_e;
        proxy_pass http://127.0.0.1:9002;
    }
}
"#,
        check: |config| {
            let routes = &config.virtual_hosts[0].routes;
            let sub = routes[0].sub_filter.as_ref().expect("sub filter");
            assert!(!sub.once);
            assert!(sub.last_modified);
            assert_eq!(sub.rules.len(), 1);
            assert_eq!(sub.rules[0].from, "old");
            assert_eq!(sub.rules[0].to, "new");
            let link = routes[1].secure_link.as_ref().expect("secure link");
            match link {
                sdkwork_webserver_core::config::SecureLinkMode::Md5 {
                    argument,
                    expires_argument,
                    ..
                } => {
                    assert_eq!(argument, "st");
                    assert_eq!(expires_argument.as_deref(), Some("e"));
                }
                other => panic!("expected md5 mode, got {other:?}"),
            }
        },
    },
    SurfaceCase {
        name: "secure_link_secret materializes",
        nginx: r#"
server {
    listen 80;
    server_name secret.example.com;
    location /s {
        secure_link_secret s3cret;
        proxy_pass http://127.0.0.1:9001;
    }
}
"#,
        check: |config| {
            let link = config.virtual_hosts[0].routes[0]
                .secure_link
                .as_ref()
                .unwrap();
            match link {
                sdkwork_webserver_core::config::SecureLinkMode::Secret { secret } => {
                    assert_eq!(secret, "s3cret");
                }
                other => panic!("expected secret mode, got {other:?}"),
            }
        },
    },
    SurfaceCase {
        name: "gzip + limit_req_zone + proxy_cache_path at http level",
        nginx: r#"
http {
    gzip on;
    gzip_types text/plain application/json;
    gzip_min_length 1k;
    limit_req_zone $binary_remote_addr zone=one:10m rate=1r/s;
    proxy_cache_path /var/cache/nginx levels=1:2 keys_zone=cache:10m inactive=5m max_size=1g;
    server {
        listen 80;
        server_name http.example.com;
        location / {
            proxy_cache cache;
            proxy_cache_valid 200 5m;
            proxy_cache_use_stale error timeout;
            proxy_pass http://127.0.0.1:9001;
        }
    }
}
"#,
        check: |config| {
            assert!(config.gzip.enabled);
            assert_eq!(config.gzip.types, vec!["text/plain", "application/json"]);
            assert_eq!(config.gzip.min_length, 1024);
            assert_eq!(config.limit_req_zones[0].name, "one");
            assert!(config.proxy_cache.enabled);
            assert!(config
                .proxy_cache
                .disk_path
                .as_deref()
                .unwrap()
                .contains("var/cache/nginx"));
            assert_eq!(config.proxy_cache.default_ttl_seconds, 300);
        },
    },
    SurfaceCase {
        name: "client_max_body_size materializes limits",
        nginx: r#"
server {
    listen 80;
    server_name body.example.com;
    client_max_body_size 10m;
    location / { return 200 "ok"; }
}
"#,
        check: |config| {
            assert_eq!(config.limits.max_request_body_bytes, 10 * 1024 * 1024);
        },
    },
    SurfaceCase {
        name: "proxy_ssl upstream TLS materializes",
        nginx: r#"
upstream secure { server https://127.0.0.1:9443; }
server {
    listen 80;
    server_name upstream-tls.example.com;
    location / {
        proxy_ssl_verify on;
        proxy_ssl_trusted_certificate ca.pem;
        proxy_ssl_certificate client.pem;
        proxy_ssl_certificate_key client.key;
        proxy_ssl_server_name on;
        proxy_pass https://secure;
    }
}
"#,
        check: |config| {
            let upstream = config.upstreams.iter().find(|u| u.id == "secure").unwrap();
            let tls = upstream.tls.as_ref().expect("upstream tls");
            assert_eq!(
                tls.trust_mode,
                sdkwork_webserver_core::config::UpstreamTlsTrustMode::Custom
            );
            assert_eq!(tls.ca_certificate_files, vec!["ca.pem"]);
            assert_eq!(tls.client_certificate_file.as_deref(), Some("client.pem"));
        },
    },
    SurfaceCase {
        name: "set_real_ip_from materializes listener trustedProxy",
        nginx: r#"
server {
    listen 80;
    server_name realip.example.com;
    set_real_ip_from 10.0.0.0/8;
    set_real_ip_from 192.168.1.1;
    real_ip_header X-Forwarded-For;
    real_ip_recursive on;
    location / { return 200 "ok"; }
}
"#,
        check: |config| {
            let trusted = config.listeners[0]
                .trusted_proxy
                .as_ref()
                .expect("trusted proxy");
            assert_eq!(trusted.trusted_cidrs.len(), 2);
            assert!(trusted.recursive);
        },
    },
    SurfaceCase {
        name: "add_header maps security headers",
        nginx: r#"
server {
    listen 80;
    server_name headers.example.com;
    add_header X-Frame-Options SAMEORIGIN always;
    add_header X-Content-Type-Options nosniff;
    add_header Strict-Transport-Security "max-age=63072000";
    add_header X-Custom "custom-value";
    location / { return 200 "ok"; }
}
"#,
        check: |config| {
            let security = config.virtual_hosts[0]
                .security_headers
                .as_ref()
                .expect("security headers");
            assert!(security.x_frame_options.is_some());
            assert!(security.x_content_type_options);
            assert_eq!(
                security
                    .strict_transport_security
                    .as_ref()
                    .unwrap()
                    .max_age_seconds,
                63_072_000
            );
            assert!(security.custom_headers.iter().any(|h| h.name == "X-Custom"));
        },
    },
    SurfaceCase {
        name: "stream: literal tcp, upstream tcp, ssl_preread, udp, proxy_protocol",
        nginx: r#"
stream {
    upstream tcp_backend {
        server 127.0.0.1:19000;
        least_conn;
    }
    server {
        listen 5100;
        proxy_pass 127.0.0.1:15100;
        proxy_timeout 30s;
        proxy_protocol on;
    }
    server {
        listen 5101;
        proxy_pass tcp_backend;
    }
    server {
        listen 5102;
        ssl_preread on;
        proxy_pass 127.0.0.1:15102;
    }
    server {
        listen 5103 udp reuseport;
        proxy_pass 127.0.0.1:15103;
    }
}
"#,
        check: |config| {
            assert_eq!(config.streams.len(), 4);
            let plain = config.streams.iter().find(|s| s.port == 5100).unwrap();
            assert!(plain.proxy_protocol);
            assert_eq!(plain.proxy_timeout_ms, 30_000);
            let upstream_target = config.streams.iter().find(|s| s.port == 5101).unwrap();
            assert!(matches!(
                upstream_target.target,
                sdkwork_webserver_core::config::StreamTargetConfig::Upstream { .. }
            ));
            let preread = config.streams.iter().find(|s| s.port == 5102).unwrap();
            assert_eq!(preread.tls, Some(StreamTlsMode::Preread));
            let udp = config.streams.iter().find(|s| s.port == 5103).unwrap();
            assert_eq!(
                udp.protocol,
                sdkwork_webserver_core::config::StreamProtocol::Udp
            );
        },
    },
    SurfaceCase {
        name: "stream ssl terminate with client auth",
        nginx: r#"
stream {
    server {
        listen 5110 ssl;
        proxy_pass 127.0.0.1:15110;
        ssl_certificate /etc/ssl/s.pem;
        ssl_certificate_key /etc/ssl/s.key;
        ssl_verify_client on;
        ssl_client_certificate /etc/ssl/ca.pem;
    }
}
"#,
        check: |config| {
            let stream = &config.streams[0];
            let StreamTlsMode::Terminate {
                certificate_ref,
                client_auth,
            } = stream.tls.as_ref().unwrap()
            else {
                panic!("expected terminate mode");
            };
            assert!(!certificate_ref.is_empty());
            let client_auth = client_auth.as_ref().expect("stream client auth");
            assert_eq!(
                client_auth.mode,
                sdkwork_webserver_core::config::ClientAuthMode::Required
            );
            assert_eq!(client_auth.ca_certificate_files.len(), 1);
        },
    },
    SurfaceCase {
        name: "multiple stream listen materializes one stream per listen",
        nginx: r#"
stream {
    server {
        listen 5120;
        listen 5121;
        proxy_pass 127.0.0.1:15120;
    }
}
"#,
        check: |config| {
            let ports = config.streams.iter().map(|s| s.port).collect::<Vec<_>>();
            assert_eq!(ports, vec![5120, 5121]);
        },
    },
];

/// Every fail-closed form with the diagnostic fragment it must produce.
const FAIL_CLOSED: &[(&str, &str, &str)] = &[
    ("named location", "location @fallback { root /srv; }", "named location"),
    ("regex server_name", "server_name ~^www\\d+\\.example\\.com$;", "regex server name"),
    ("trailing wildcard server_name", "server_name www.example.*;", "wildcard server name"),
    ("empty server_name", "server_name \"\";", "empty `server_name"),
    ("return 444", "location / { return 444; }", "return 444"),
    ("variable return body", "location / { return 200 \"$host\"; }", "contains variables"),
    // `=code` fallbacks are accepted since the runtime maps them to a static
    // resource without a SPA fallback (with `root`); without any root the
    // location fails closed on the serving-behavior rule.
    ("try_files =code fallback without a root", "location / { try_files $uri =404; }", "exactly one of"),
    ("try_files @named with serving", "location / { try_files $uri @x; proxy_pass http://127.0.0.1:9001; }", "cannot be combined"),
    // Literal intermediate probes are nginx-standard and accepted; the
    // rejection path covers non-`$uri`/non-literal probes (unit-tested). The
    // fixture below fails closed on the serving-behavior rule (no root).
    ("try_files intermediate literal without a root", "location / { try_files /fixed $uri /index.html; }", "exactly one of"),
    ("unix proxy_pass", "location / { proxy_pass http://unix:/tmp/sock; }", "unix:"),
    ("proxy_pass non-http scheme", "location / { proxy_pass ftp://127.0.0.1:21; }", "must be http(s)://"),
    ("proxy_pass URI query", "location / { proxy_pass http://127.0.0.1:9001/?a=1; }", "query strings"),
    ("proxy_pass undefined upstream", "location / { proxy_pass http://missing; }", "undefined upstream"),
    ("proxy_ssl verify off", "location / { proxy_ssl_verify off; proxy_pass https://127.0.0.1:9443; }", "proxy_ssl_verify off"),
    ("proxy_ssl server_name off", "location / { proxy_ssl_server_name off; proxy_pass https://127.0.0.1:9443; }", "proxy_ssl_server_name off"),
    ("proxy_ssl custom name", "location / { proxy_ssl_name api.internal; proxy_pass https://127.0.0.1:9443; }", "custom upstream SNI name"),
    ("proxy_ssl absolute path", "location / { proxy_ssl_verify on; proxy_ssl_trusted_certificate /etc/ssl/ca.pem; proxy_pass https://127.0.0.1:9443; }", "must be relative"),
    ("proxy_ssl cert without key", "location / { proxy_ssl_certificate c.pem; proxy_pass https://127.0.0.1:9443; }", "requires `proxy_ssl_certificate_key`"),
    ("proxy_ssl on http target", "location / { proxy_ssl_verify on; proxy_pass http://127.0.0.1:9001; }", "require an `https://`"),
    ("real_ip unsupported header", "real_ip_header X-Real-IP;", "X-Forwarded-For"),
    ("invalid client_max_body_size", "client_max_body_size huge;", "invalid client_max_body_size"),
    ("conflicting client_max_body_size", "client_max_body_size 1m; location /a/ { client_max_body_size 10m; return 200 \"a\"; }", "conflicting `client_max_body_size`"),
    ("proxy_cache undefined zone", "proxy_cache nope;", "undefined cache zone"),
    ("limit_req undefined zone", "location / { limit_req zone=nope; proxy_pass http://127.0.0.1:9001; }", "undefined zone"),
    ("limit_conn undefined zone", "location / { limit_conn nope 10; proxy_pass http://127.0.0.1:9001; }", "undefined zone"),
    ("auth_basic_user_file without auth_basic", "location / { auth_basic_user_file /etc/nginx/htpasswd; return 200 \"ok\"; }", "requires `auth_basic`"),
    ("ssl_verify_client without CA", "listen 443 ssl; ssl_certificate /etc/ssl/x.pem; ssl_certificate_key /etc/ssl/x.key; ssl_verify_client on; location / { return 200 \"ok\"; }", "requires ssl_client_certificate"),
    ("ssl_client_certificate without verify", "listen 443 ssl; ssl_certificate /etc/ssl/x.pem; ssl_certificate_key /etc/ssl/x.key; ssl_client_certificate /etc/ssl/ca.pem; location / { return 200 \"ok\"; }", "requires ssl_verify_client"),
    ("ssl listen without certificate", "listen 443 ssl;", "declares no ssl_certificate"),
    ("alias regex location", "location ~ ^/x { alias /srv/data/; }", "regex location match"),
    ("alias gluing", "location /static/ { alias /srv/data; }", "trailing slash"),
    ("multiple serving behaviors", "location / { root /srv/www; proxy_pass http://127.0.0.1:9001; }", "exactly one of"),
    ("unknown rewrite flag", "location / { rewrite ^/x$ /y if_not_supported; proxy_pass http://127.0.0.1:9001; }", "rewrite flag"),

    ("http listen proxy_protocol", "listen 80 proxy_protocol;", "trusted source CIDRs"),
    ("location without match", "location { return 200 \"ok\"; }", "requires a match path"),
    ("proxy_pass_request_headers bad value", "location / { proxy_pass_request_headers maybe; proxy_pass http://127.0.0.1:9001; }", "accepts on|off"),
    ("sub_filter_once bad value", "location / { sub_filter_once maybe; return 200 \"ok\"; }", "accepts on|off"),
    ("ssl_verify_client bad value", "ssl_verify_client maybe;", "accepts on|optional|off"),
    ("http2 bad value", "http2 maybe;", "accepts on|off"),
    ("listen unsupported parameter", "listen 80 fancy=1;", "unsupported listen parameter"),
];

/// Stock `nginx.conf` tuning directives that must be accepted (runtime owns
/// the corresponding behavior).
const STOCK_ACCEPTED: &[(&str, &str, &str)] = &[
    ("main process", "main", "user nginx;\nworker_processes auto;\npid /run/nginx.pid;\nworker_rlimit_nofile 65535;\ndaemon off;\nmaster_process on;\nworker_priority -5;\nworker_cpu_affinity auto;\nworker_shutdown_timeout 10s;\nenv FOO;\npcre_jit on;\nssl_engine dynamic;\ntimer_resolution 100ms;\nlock_file /var/lock/nginx.lock;\nworker_rlimit_core 1g;\nworking_directory /var/cache/nginx;"),
    ("events", "main", "events { worker_connections 1024; use epoll; multi_accept on; accept_mutex off; accept_mutex_delay 500ms; }"),
    ("http tuning", "http", "server_names_hash_bucket_size 64;\ntypes_hash_max_size 2048;\ntypes_hash_bucket_size 64;\nvariables_hash_max_size 1024;\nmap_hash_max_size 2048;\nproxy_headers_hash_max_size 512;\nclient_header_buffer_size 1k;\nlarge_client_header_buffers 4 8k;\nclient_body_buffer_size 16k;\nclient_body_timeout 60s;\nclient_header_timeout 60s;\nsend_timeout 60s;\nkeepalive_timeout 75;\nkeepalive_requests 1000;\nkeepalive_disable msie6;\nlingering_timeout 5s;\nlingering_time 30s;\nlingering_close off;\nconnection_pool_size 320;\nrequest_pool_size 4k;\noutput_buffers 2 32k;\npostpone_output 1460;\nread_ahead 0;\nsendfile on;\nsendfile_max_chunk 1m;\ntcp_nopush on;\ntcp_nodelay on;\nserver_tokens off;\nmerge_slashes on;\nunderscores_in_headers on;\nignore_invalid_headers on;\nlog_not_found off;\nreset_timedout_connection on;\nopen_file_cache max=1000 inactive=20s;\nopen_file_cache_valid 30s;\nopen_file_cache_min_uses 2;\nmsie_padding off;\nchunked_transfer_encoding on;\nmax_ranges 1;\nabsolute_redirect off;\nport_in_redirect off;\nserver_name_in_redirect off;\nrecursive_error_pages on;\nlog_subrequest on;\ndirectio 4m;\ndirectio_alignment 512;\nerror_page 500 502 503 504 /50x.html;\nexpires 1d;\netag off;\nif_modified_since before;\ncharset utf-8;\ncharset_types text/html text/xml;\nsource_charset utf-8;\ndefault_type application/octet-stream;\nmap $http_user_agent $mobile { default 0; ~*iphone 1; }\nlog_format main escape=json '$remote_addr $request';\naccess_log /var/log/nginx/access.log main buffer=32k flush=5m;\nerror_log /var/log/nginx/error.log warn;"),
    ("http proxy tuning", "http", "proxy_http_version 1.1;\nproxy_buffering off;\nproxy_request_buffering off;\nproxy_buffer_size 4k;\nproxy_buffers 8 4k;\nproxy_connect_timeout 5s;\nproxy_read_timeout 60s;\nproxy_send_timeout 60s;\nproxy_intercept_errors on;\nproxy_next_upstream error timeout;\nproxy_redirect default;\nproxy_hide_header X-Foo;\nproxy_pass_header X-Bar;\nproxy_temp_path /var/cache/nginx/temp;\nproxy_max_temp_file_size 1g;\nproxy_temp_file_write_size 8k;\nproxy_send_lowat 0;\nproxy_ssl_protocols TLSv1.2 TLSv1.3;\nproxy_ssl_ciphers HIGH;\nproxy_ssl_session_reuse on;\nproxy_ssl_verify_depth 2;"),
    ("gzip tuning", "http", "gzip_comp_level 6;\ngzip_vary on;\ngzip_proxied any;\ngzip_disable \"msie6\";\ngzip_static on;\ngzip_http_version 1.1;\ngzip_buffers 16 8k;\ngzip_window 512k;"),
    ("server tuning", "server", "keepalive_timeout 65;\nclient_max_body_size 1m;\nssl_protocols TLSv1.2 TLSv1.3;\nssl_prefer_server_ciphers on;\nssl_session_cache shared:SSL:10m;\nssl_session_timeout 10m;\nssl_session_tickets on;\nssl_ciphers HIGH:!aNULL;\nssl_verify_depth 3;\nssl_dhparam /etc/ssl/dhparam.pem;\nssl_ecdh_curve auto;\nssl_trusted_certificate /etc/ssl/chain.pem;\nssl_stapling on;\nssl_stapling_verify on;\nssl_conf_command Options PrioritizeChaCha;\nssl_buffer_size 4k;\nautoindex off;\nadd_header X-Extra \"1\";\nlimit_conn_status 429;\nlimit_conn_log_level warn;\nlimit_req_status 429;\nlimit_req_log_level warn;"),
    ("location tuning", "location", "proxy_read_timeout 30s;\nproxy_send_timeout 30s;\nproxy_connect_timeout 3s;\nproxy_buffering off;\nproxy_request_buffering off;\nproxy_redirect default;\nproxy_http_version 1.1;\nclient_body_timeout 30s;\nclient_header_timeout 30s;\nkeepalive_timeout 30s;\nsend_timeout 30s;\nlog_not_found off;\naccess_log off;\nopen_file_cache max=100;\nexpires 7d;\netag on;\nautoindex off;"),
];

#[test]
fn auth_basic_off_disables_inherited_auth_with_real_htpasswd() {
    let directory = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        directory.path().join("htpasswd"),
        "alice:{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ=\n",
    )
    .unwrap();
    let parsed = parse_nginx_config(
        &format!(
            r#"
server {{
    listen 80;
    server_name auth.example.com;
    auth_basic "Realm";
    auth_basic_user_file {};
    location /public {{
        auth_basic off;
        return 200 "open";
    }}
    location /private {{ return 200 "closed"; }}
}}
"#,
            directory.path().join("htpasswd").display()
        ),
        Path::new("site.conf"),
    )
    .expect("parse");
    let config = materialize_nginx_app(&parsed, directory.path(), "auth").expect("materialize");
    let routes = &config.virtual_hosts[0].routes;
    assert!(
        routes[0].auth_basic.is_none(),
        "auth_basic off must disable"
    );
    assert!(
        routes[1].auth_basic.is_some(),
        "inherited auth_basic applies"
    );
    assert_eq!(routes[1].auth_basic.as_ref().unwrap().users.len(), 1);
}

#[test]
fn every_supported_directive_family_materializes() {
    for case in SURFACE {
        let config = materialize_ok(case.nginx);
        (case.check)(&config);
    }
}

/// Fail-closed forms that live at the http level (upstream blocks).
const FAIL_CLOSED_HTTP: &[(&str, &str, &str)] = &[
    (
        "unix upstream target",
        "upstream u { server unix:/tmp/sock; }",
        "unix:",
    ),
    (
        "upstream unknown parameter",
        "upstream u { server 127.0.0.1:9001 resolve; }",
        "unsupported upstream server parameter",
    ),
    (
        "upstream without targets",
        "upstream empty { keepalive 4; }",
        "no server targets",
    ),
    (
        "upstream hash unsupported key",
        "upstream u { hash $http_user_agent; server 127.0.0.1:9001; }",
        "unsupported hash key",
    ),
    (
        "gzip invalid value",
        "gzip maybe;",
        "unsupported gzip value",
    ),
];

#[test]
fn every_fail_closed_form_produces_a_precise_diagnostic() {
    for (name, body, expected) in FAIL_CLOSED {
        let text = format!(
            "server {{\n    listen 80;\n    server_name fail-{}.example.com;\n    {body}\n}}\n",
            name.replace(' ', "-")
        );
        materialize_err(&text, expected);
    }
    for (name, body, expected) in FAIL_CLOSED_HTTP {
        let text = format!(
            "http {{\n    {body}\n    server {{\n        listen 80;\n        server_name fail-{}.example.com;\n        location / {{ return 200 \"ok\"; }}\n    }}\n}}\n",
            name.replace(' ', "-")
        );
        materialize_err(&text, expected);
    }
}

/// Fail-closed forms that live in the stream context.
const FAIL_CLOSED_STREAM: &[(&str, &str, &str)] = &[
    (
        "stream proxy_protocol v2",
        "proxy_protocol v2;",
        "proxy_protocol v2",
    ),
    (
        "stream listen inbound proxy_protocol",
        "listen 5100 proxy_protocol;",
        "trusted source CIDRs",
    ),
    (
        "stream listen unsupported parameter",
        "listen 5100 fancy=1;",
        "unsupported stream listen parameter",
    ),
];

#[test]
fn every_stream_fail_closed_form_produces_a_precise_diagnostic() {
    for (name, body, expected) in FAIL_CLOSED_STREAM {
        let text = format!(
            "stream {{\n    server {{\n        listen 5100;\n        {body}\n        proxy_pass 127.0.0.1:15100;\n    }}\n}}\n",
        );
        materialize_err(&text, expected);
    }
}

/// Fail-closed forms that are whole top-level configurations.
const FAIL_CLOSED_TOP: &[(&str, &str, &str)] = &[
    (
        "multiple http blocks",
        "http { } http { }",
        "multiple `http` blocks",
    ),
    (
        "multiple stream blocks",
        "stream { } stream { }",
        "multiple `stream` blocks",
    ),
    (
        "top-level unknown directive",
        "mail { }",
        "unsupported directive `mail`",
    ),
    (
        "no server blocks",
        "http { limit_req_zone $binary_remote_addr zone=z:1m rate=1r/s; }",
        "no `server` or `stream` blocks",
    ),
    (
        "server without location",
        "server { listen 80; server_name x.example.com; }",
        "at least one location",
    ),
    (
        "server without server_name",
        "server { listen 80; location / { return 200 \"ok\"; } }",
        "requires server_name",
    ),
];

#[test]
fn every_top_level_fail_closed_form_produces_a_precise_diagnostic() {
    for (name, body, expected) in FAIL_CLOSED_TOP {
        materialize_err(body, expected);
    }
}

#[test]
fn stock_nginx_conf_tuning_is_accepted_end_to_end() {
    for &(name, context, tuning) in STOCK_ACCEPTED {
        let host = format!("stock-{}.example.com", name.replace(' ', "-"));
        let text = match context {
            "main" => format!(
                "{tuning}\nhttp {{\n    server {{\n        listen 80;\n        server_name {host};\n        location / {{ return 200 \"ok\"; }}\n    }}\n}}\n"
            ),
            "http" => format!(
                "http {{\n    {tuning}\n    server {{\n        listen 80;\n        server_name {host};\n        location / {{ return 200 \"ok\"; }}\n    }}\n}}\n"
            ),
            "server" => format!(
                "http {{\n    server {{\n        listen 80;\n        server_name {host};\n        {tuning}\n        location / {{ return 200 \"ok\"; }}\n    }}\n}}\n"
            ),
            "location" => format!(
                "http {{\n    server {{\n        listen 80;\n        server_name {host};\n        location / {{\n            {tuning}\n            return 200 \"ok\";\n        }}\n    }}\n}}\n"
            ),
            other => panic!("unknown stock context {other}"),
        };
        let config = materialize_ok(&text);
        assert_eq!(config.virtual_hosts.len(), 1, "{name}");
    }
}

#[test]
fn full_nginx_conf_round_trip_combines_the_whole_surface() {
    let directory = tempfile::tempdir().expect("temp dir");
    std::fs::write(directory.path().join("ca.pem"), "ca").unwrap();
    std::fs::write(directory.path().join("client.pem"), "cert").unwrap();
    std::fs::write(directory.path().join("client.key"), "key").unwrap();
    std::fs::write(
        directory.path().join("htpasswd"),
        "alice:{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ=\n",
    )
    .unwrap();
    let cache = directory.path().join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let text = format!(
        r#"
user nginx;
worker_processes auto;
events {{ worker_connections 1024; }}
http {{
    include {mime};
    gzip on;
    gzip_types text/plain application/json;
    limit_req_zone $binary_remote_addr zone=one:10m rate=10r/s;
    proxy_cache_path {cache} levels=1:2 keys_zone=cache:10m inactive=5m;
    proxy_set_header X-Http-Level $scheme;

    upstream api {{
        least_conn;
        server https://127.0.0.1:9001 weight=2;
        server https://127.0.0.1:9002 backup;
        keepalive 32;
    }}

    server {{
        listen 80;
        listen [::]:80;
        server_name full.example.com *.example.com;
        client_max_body_size 20m;
        location /api/ {{
            limit_req zone=one burst=10 nodelay;
            auth_basic "Restricted";
            auth_basic_user_file {htpasswd};
            proxy_set_header Host $host;
            proxy_pass http://api/;
        }}
        location = /healthz {{ return 200 "ok"; }}
        location /old/ {{ return 301 https://$host$request_uri; }}
        location /static/ {{
            root /srv/www;
            try_files $uri $uri/ /index.html;
        }}
        location /files/ {{
            alias /srv/data/;
        }}
        location /up/ {{
            proxy_ssl_verify on;
            proxy_ssl_trusted_certificate ca.pem;
            proxy_ssl_certificate client.pem;
            proxy_ssl_certificate_key client.key;
            proxy_pass https://api;
        }}
    }}
}}

stream {{
    upstream tcp_backend {{
        server 127.0.0.1:19000;
    }}
    server {{
        listen 5100;
        proxy_pass tcp_backend;
        proxy_timeout 30s;
        proxy_protocol on;
    }}
    server {{
        listen 5101 udp;
        proxy_pass 127.0.0.1:15101;
    }}
}}
"#,
        mime = directory
            .path()
            .join("mime.types")
            .display()
            .to_string()
            .replace('\\', "/"),
        cache = cache.display().to_string().replace('\\', "/"),
        htpasswd = directory
            .path()
            .join("htpasswd")
            .display()
            .to_string()
            .replace('\\', "/"),
    );
    // The `include mime.types` target must exist for the loader path.
    std::fs::write(
        directory.path().join("mime.types"),
        "types { text/html html; application/json json; }\n",
    )
    .unwrap();
    let parsed = parse_nginx_config(&text, Path::new("nginx.conf")).expect("parse");
    let mut budget = 256;
    let mut stack = vec![directory.path().join("nginx.conf")];
    let expanded =
        expand_includes(parsed, directory.path(), &mut budget, &mut stack).expect("expand");
    let config = materialize_nginx_app(&expanded, directory.path(), "full").expect("materialize");
    assert_eq!(config.virtual_hosts.len(), 1);
    assert_eq!(config.listeners.len(), 2, "80 + [::]:80");
    assert_eq!(config.upstreams.len(), 2, "api + tcp_backend");
    assert_eq!(config.streams.len(), 2);
    assert!(config.gzip.enabled);
    assert_eq!(config.limit_req_zones.len(), 1);
    assert!(config.proxy_cache.enabled);
    let api = config.upstreams.iter().find(|u| u.id == "api").unwrap();
    assert_eq!(
        api.load_balancing,
        UpstreamLoadBalancingStrategy::LeastConnections
    );
    let routes = &config.virtual_hosts[0].routes;
    assert_eq!(routes.len(), 6, "api, healthz, old, static, files, up");
    assert!(routes[0].limit_req.len() == 1 && routes[0].auth_basic.is_some());
    let upstream_tls = config.upstreams.iter().find(|u| u.id == "api").unwrap();
    assert!(
        upstream_tls.tls.is_some(),
        "proxy_ssl attaches to the shared upstream"
    );
}

#[test]
fn include_surface_expands_files_globs_and_nested_includes() {
    let root = tempfile::tempdir().expect("temp dir");
    let snippets = root.path().join("snippets");
    std::fs::create_dir_all(&snippets).unwrap();
    std::fs::write(snippets.join("frag.conf"), "return 200 \"frag\";\n").unwrap();
    std::fs::write(root.path().join("a.conf"), "server { listen 1; }\n").unwrap();
    std::fs::write(root.path().join("b.conf"), "server { listen 2; }\n").unwrap();
    std::fs::write(root.path().join("c.txt"), "ignore\n").unwrap();
    std::fs::write(
        root.path().join("nested.conf"),
        "include snippets/frag.conf;\n",
    )
    .unwrap();

    let pattern_glob = root
        .path()
        .join("*.conf")
        .display()
        .to_string()
        .replace('\\', "/");
    let pattern_nested = root
        .path()
        .join("nested.conf")
        .display()
        .to_string()
        .replace('\\', "/");
    let text = format!("include {pattern_glob};\ninclude {pattern_nested};\n");
    let parsed = parse_nginx_config(&text, Path::new("main.conf")).expect("parse");
    let mut budget = 64;
    let mut stack = Vec::new();
    let expanded = expand_includes(parsed, root.path(), &mut budget, &mut stack).expect("expand");
    // a.conf, b.conf, nested.conf → nested.conf expands to its fragment.
    let names = expanded.iter().map(|d| d.name.as_str()).collect::<Vec<_>>();
    // Glob matched a.conf, b.conf, nested.conf; nested.conf expands to the
    // fragment, and the second literal include of nested.conf expands again.
    assert_eq!(names, vec!["server", "server", "return", "return"]);
    assert_eq!(expanded[2].args, vec!["200", "frag"]);
}

#[test]
fn include_missing_and_cycle_fail_closed() {
    let root = tempfile::tempdir().expect("temp dir");
    let parsed =
        parse_nginx_config("include missing.conf;\n", Path::new("main.conf")).expect("parse");
    let mut budget = 16;
    let mut stack = Vec::new();
    let error = expand_includes(parsed, root.path(), &mut budget, &mut stack)
        .err()
        .expect("missing include must fail");
    assert!(error.to_string().contains("matches no files"), "{error}");

    std::fs::write(root.path().join("loop.conf"), "include loop.conf;\n").unwrap();
    let parsed = parse_nginx_config(
        &format!("include {};\n", root.path().join("loop.conf").display()),
        Path::new("main.conf"),
    )
    .expect("parse");
    let mut budget = 16;
    let mut stack = Vec::new();
    let error = expand_includes(parsed, root.path(), &mut budget, &mut stack)
        .err()
        .expect("include cycle must fail");
    assert!(error.to_string().contains("cycle"), "{error}");
}

#[test]
fn load_nginx_compat_round_trips_files_and_directories() {
    let root = tempfile::tempdir().expect("temp dir");
    let sites = root.path().join("sites-enabled");
    let stream_dir = root.path().join("stream-conf.d");
    std::fs::create_dir_all(&sites).unwrap();
    std::fs::create_dir_all(&stream_dir).unwrap();
    std::fs::write(
        sites.join("web.conf"),
        r#"
server {
    listen 80;
    server_name web.example.com;
    location / { proxy_pass http://127.0.0.1:18080; }
}
"#,
    )
    .unwrap();
    std::fs::write(
        stream_dir.join("im.stream.conf"),
        r#"
server {
    listen 5100;
    proxy_pass 127.0.0.1:15100;
}
"#,
    )
    .unwrap();

    let report = load_nginx_compat(&sites, "compat").expect("load directory");
    assert_eq!(report.app.virtual_hosts.len(), 1);
    assert_eq!(report.app.streams.len(), 1);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let file_report = load_nginx_compat(&sites.join("web.conf"), "compat-file").expect("load file");
    assert_eq!(file_report.app.virtual_hosts.len(), 1);

    let missing = load_nginx_compat(&root.path().join("absent"), "compat")
        .err()
        .expect("missing path must fail");
    assert!(missing.to_string().contains("does not exist"), "{missing}");
}

#[test]
fn load_nginx_compat_loads_a_full_nginx_conf_file_with_includes() {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        root.path().join("mime.types"),
        "types { text/html html; application/json json; }
",
    )
    .unwrap();
    let sites = root.path().join("sites-enabled");
    std::fs::create_dir_all(&sites).unwrap();
    std::fs::write(
        sites.join("site.conf"),
        "server {
    listen 8081;
    server_name full.example.com;
    location / { return 200 \"ok\"; }
}
",
    )
    .unwrap();
    std::fs::write(
        root.path().join("nginx.conf"),
        format!(
            "user nginx;
worker_processes auto;
events {{ worker_connections 1024; }}
http {{
    include {};
    server {{
        listen 8080;
        server_name main.example.com;
        location / {{ return 200 \"main\"; }}
    }}
}}
stream {{
    server {{
        listen 5200;
        proxy_pass 127.0.0.1:15200;
    }}
}}
",
            root.path()
                .join("mime.types")
                .display()
                .to_string()
                .replace('\\', "/")
        ),
    )
    .unwrap();
    let report = load_nginx_compat(&root.path().join("nginx.conf"), "full-file").expect("load");
    assert_eq!(
        report.app.virtual_hosts.len(),
        1,
        "the full conf has one server"
    );
    assert_eq!(report.app.listeners.len(), 1);
    assert_eq!(report.app.listeners[0].port, 8080);
    assert_eq!(report.app.streams.len(), 1);
    assert_eq!(report.app.streams[0].port, 5200);
}

#[test]
fn merge_nginx_apps_keeps_both_files_vhosts_listeners_and_streams() {
    let left = materialize_ok(
        r#"
server {
    listen 443 ssl;
    server_name left.example.com;
    ssl_certificate /etc/ssl/left.pem;
    ssl_certificate_key /etc/ssl/left.key;
    location / { return 200 "left"; }
}
stream {
    server { listen 5100; proxy_pass 127.0.0.1:15100; }
}
"#,
    );
    let right = materialize_ok(
        r#"
server {
    listen 443 ssl;
    server_name right.example.com;
    ssl_certificate /etc/ssl/right.pem;
    ssl_certificate_key /etc/ssl/right.key;
    location / { return 200 "right"; }
}
"#,
    );
    let merged = merge_nginx_apps(left, right).expect("merge");
    assert_eq!(merged.virtual_hosts.len(), 2);
    assert_eq!(merged.listeners.iter().filter(|l| l.port == 443).count(), 1);
    assert_eq!(merged.streams.len(), 1);
    let policy = merged
        .tls_policies
        .iter()
        .find(|p| {
            p.id == merged
                .listeners
                .iter()
                .find(|l| l.port == 443)
                .unwrap()
                .tls_policy_ref
                .as_deref()
                .unwrap()
        })
        .unwrap();
    assert!(policy.certificate_refs().count() >= 2);
    // Resource ids from the second file are rewritten to avoid collisions.
    let ids = merged
        .resources
        .iter()
        .map(|r| r.id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn load_report_skips_unmaterializable_site_files_with_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let sites = root.path().join("sites-enabled");
    std::fs::create_dir_all(&sites).unwrap();
    std::fs::write(
        sites.join("ok.conf"),
        "server { listen 80; server_name ok.example.com; location / { return 200 \"ok\"; } }\n",
    )
    .unwrap();
    std::fs::write(
        sites.join("bad.conf"),
        "server { listen 80; server_name bad.example.com; location @named { root /srv; } }\n",
    )
    .unwrap();
    let report = load_nginx_compat(&sites, "compat").expect("load");
    assert_eq!(report.app.virtual_hosts.len(), 1);
    assert_eq!(report.skipped.len(), 1);
    assert!(
        report.skipped[0].1.contains("named location"),
        "{:?}",
        report.skipped
    );
}
