import io

path = "crates/sdkwork-webserver-core/tests/nginx_config_surface.rs"
text = io.open(path, encoding="utf-8").read()

# 1. wildcard server_name case: drop the invalid trailing-wildcard-ish name
old = "    server_name example.com *.example.com www.example.*-invalid;"
new = "    server_name example.com *.example.com;"
assert text.count(old) == 1, "wildcard case"
text = text.replace(old, new)

# 2. include surface: nested.conf appears twice (glob + literal include)
old2 = r"""    let names = expanded.iter().map(|d| d.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, vec!["server", "server", "return"]);"""
new2 = r"""    let names = expanded.iter().map(|d| d.name.as_str()).collect::<Vec<_>>();
    // Glob matched a.conf, b.conf, nested.conf; nested.conf expands to the
    // fragment, and the second literal include of nested.conf expands again.
    assert_eq!(names, vec!["server", "server", "return", "return"]);"""
assert text.count(old2) == 1, "include expectations"
text = text.replace(old2, new2)

# 3. full round trip: api upstream targets must be https when proxy_ssl attaches
old3 = r"""    upstream api {{
        least_conn;
        server 127.0.0.1:9001 weight=2;
        server 127.0.0.1:9002 backup;
        keepalive 32;
    }}"""
new3 = r"""    upstream api {{
        least_conn;
        server https://127.0.0.1:9001 weight=2;
        server https://127.0.0.1:9002 backup;
        keepalive 32;
    }}"""
assert text.count(old3) == 1, "round trip upstream"
text = text.replace(old3, new3)

# 4. move upstream-level fail entries out of the server-level table
for entry in [
    '    ("unix upstream target", "server unix:/tmp/sock;", "unix:"),\n',
    '    ("upstream unknown parameter", "server 127.0.0.1:9001 resolve;", "unsupported upstream server parameter"),\n',
    '    ("upstream without targets", "upstream empty { keepalive 4; }", "no server targets"),\n',
    '    ("upstream hash unsupported key", "hash $http_user_agent;", "unsupported hash key"),\n',
]:
    assert text.count(entry) == 1, entry[:50]
    text = text.replace(entry, "")

# 5. add the http-level fail table + harness (raw strings keep \n literal)
old8 = r'''#[test]
fn every_fail_closed_form_produces_a_precise_diagnostic() {
    for (name, body, expected) in FAIL_CLOSED {
        let text = format!(
            "server {{
    listen 80;
    server_name fail-{}.example.com;
    {body}
}}
",
            name.replace(' ', "-")
        );
        materialize_err(&text, expected);
    }
}'''
new8 = r'''/// Fail-closed forms that live at the http level (upstream blocks).
const FAIL_CLOSED_HTTP: &[(&str, &str, &str)] = &[
    ("unix upstream target", "upstream u { server unix:/tmp/sock; }", "unix:"),
    ("upstream unknown parameter", "upstream u { server 127.0.0.1:9001 resolve; }", "unsupported upstream server parameter"),
    ("upstream without targets", "upstream empty { keepalive 4; }", "no server targets"),
    ("upstream hash unsupported key", "upstream u { hash $http_user_agent; server 127.0.0.1:9001; }", "unsupported hash key"),
];

#[test]
fn every_fail_closed_form_produces_a_precise_diagnostic() {
    for (name, body, expected) in FAIL_CLOSED {
        let text = format!(
            "server {{
    listen 80;
    server_name fail-{}.example.com;
    {body}
}}
",
            name.replace(' ', "-")
        );
        materialize_err(&text, expected);
    }
    for (name, body, expected) in FAIL_CLOSED_HTTP {
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
assert text.count(old8) == 1, "fail harness"
text = text.replace(old8, new8)

io.open(path, "w", encoding="utf-8", newline="").write(text)
print("all fixes applied")
