import io

path = "crates/sdkwork-webserver-core/tests/nginx_config_surface.rs"
text = io.open(path, encoding="utf-8").read()

# 1. stream fail entries: remove from server table
for entry in [
    '    ("stream proxy_protocol v2", "proxy_protocol v2;", "proxy_protocol v2"),\n',
    '    ("stream listen inbound proxy_protocol", "listen 5100 proxy_protocol;", "trusted source CIDRs"),\n',
    '    ("stream listen unsupported parameter", "listen 5100 fancy=1;", "unsupported stream listen parameter"),\n',
]:
    assert text.count(entry) == 1, entry[:50]
    text = text.replace(entry, "")

# 2. add stream fail table + harness after the FAIL_CLOSED_HTTP loop
old = r'''    for (name, body, expected) in FAIL_CLOSED_HTTP {
        let text = format!(
            "http {{
    {body}
    server {{
        listen 80;
        server_name fail-{}.example.com;
        location / {{ return 200 \"ok\"; }}
    }}
}}
",
            name.replace(' ', "-")
        );
        materialize_err(&text, expected);
    }
}'''
new = r'''    for (name, body, expected) in FAIL_CLOSED_HTTP {
        let text = format!(
            "http {{
    {body}
    server {{
        listen 80;
        server_name fail-{}.example.com;
        location / {{ return 200 \"ok\"; }}
    }}
}}
",
            name.replace(' ', "-")
        );
        materialize_err(&text, expected);
    }
}

/// Fail-closed forms that live in the stream context.
const FAIL_CLOSED_STREAM: &[(&str, &str, &str)] = &[
    ("stream proxy_protocol v2", "proxy_protocol v2;", "proxy_protocol v2"),
    ("stream listen inbound proxy_protocol", "listen 5100 proxy_protocol;", "trusted source CIDRs"),
    ("stream listen unsupported parameter", "listen 5100 fancy=1;", "unsupported stream listen parameter"),
];

#[test]
fn every_stream_fail_closed_form_produces_a_precise_diagnostic() {
    for (name, body, expected) in FAIL_CLOSED_STREAM {
        let text = format!(
            "stream {{
    server {{
        listen 5100;
        {body}
        proxy_pass 127.0.0.1:15100;
    }}
}}
",
        );
        materialize_err(&text, expected);
    }
}'''
assert text.count(old) == 1, "http fail harness"
text = text.replace(old, new)

# 3. auth_basic off case: remove from SURFACE, add standalone test
old3 = '''    SurfaceCase {
        name: "auth_basic off disables inherited auth",
        nginx: r#"
server {
    listen 80;
    server_name auth.example.com;
    auth_basic "Realm";
    auth_basic_user_file /etc/nginx/htpasswd;
    location /public {
        auth_basic off;
        return 200 "open";
    }
    location /private { return 200 "closed"; }
}
"#,
        check: |config| {
            let routes = &config.virtual_hosts[0].routes;
            assert!(routes[0].auth_basic.is_none(), "auth_basic off must disable");
            assert!(routes[1].auth_basic.is_some(), "inherited auth_basic applies");
        },
    },
'''
assert text.count(old3) == 1, "auth_basic surface case"
text = text.replace(old3, "")

anchor = '''#[test]
fn every_supported_directive_family_materializes() {'''
new3b = '''#[test]
fn auth_basic_off_disables_inherited_auth_with_real_htpasswd() {
    let directory = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        directory.path().join("htpasswd"),
        "alice:{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ=\\n",
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
    assert!(routes[0].auth_basic.is_none(), "auth_basic off must disable");
    assert!(routes[1].auth_basic.is_some(), "inherited auth_basic applies");
    assert_eq!(routes[1].auth_basic.as_ref().unwrap().users.len(), 1);
}

#[test]
fn every_supported_directive_family_materializes() {'''
assert text.count(anchor) == 1, "surface anchor"
text = text.replace(anchor, new3b)

io.open(path, "w", encoding="utf-8", newline="").write(text)
print("stream harness + auth_basic standalone added")
