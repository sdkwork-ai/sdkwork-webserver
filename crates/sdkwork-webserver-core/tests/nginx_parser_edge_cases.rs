//! nginx tokenizer edge cases (nginx `ngx_conf_read_token` behavior).
//!
//! These cases pin the lexer against the reference tokenizer: CRLF files,
//! tab whitespace, multi-line directives, comments in every position,
//! quoted strings (per-type closing, escapes, empty values), `${var}`
//! tokens, and error line reporting.

use std::path::Path;

use sdkwork_webserver_core::nginx::{
    expand_includes, parse_nginx_config, NginxDirective, NginxParseError,
};

fn parse(text: &str) -> Result<Vec<NginxDirective>, NginxParseError> {
    parse_nginx_config(text, Path::new("edge.conf"))
}

fn parse_ok(text: &str) -> Vec<NginxDirective> {
    parse(text).expect("parse")
}

/// Extract all (name, args) pairs from a directive tree in order.
fn flatten(directives: &[NginxDirective]) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for directive in directives {
        out.push((directive.name.clone(), directive.args.clone()));
        out.extend(flatten(&directive.children));
    }
    out
}

#[test]
fn crlf_files_tokenize_identically_to_lf() {
    let lf = "server {\n    listen 80;\n    server_name x.example.com;\n    location / { return 200 \"ok\"; }\n}\n";
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(flatten(&parse_ok(lf)), flatten(&parse_ok(&crlf)));
}

#[test]
fn tabs_and_mixed_whitespace_separate_tokens() {
    let directives = parse_ok(
        "server\t{\n\tlisten\t80\t;\n\tserver_name\ttab.example.com;\n\tlocation\t/\t{\treturn\t200\t\"ok\";\t}\n}\n",
    );
    assert_eq!(directives[0].name, "server");
    assert_eq!(directives[0].children[0].args, vec!["80"]);
}

#[test]
fn directives_span_multiple_lines() {
    let directives = parse_ok(
        "server {\n    listen 80\n        default_server;\n    server_name\n        multi.example.com\n        second.example.com;\n    location / {\n        proxy_pass\n            http://127.0.0.1:9001;\n    }\n}\n",
    );
    let children = &directives[0].children;
    assert_eq!(children[0].args, vec!["80", "default_server"]);
    assert_eq!(
        children[1].args,
        vec!["multi.example.com", "second.example.com"]
    );
}

#[test]
fn comments_appear_everywhere_except_mid_token() {
    let directives = parse_ok(
        r#"# leading comment
server { # after block open
    listen 80; # after statement
    server_name comment.example.com; # trailing
    # full-line comment
    location / {
        # inside location
        return 200 "ok"; # after return
    }
}
# trailing comment
"#,
    );
    assert_eq!(directives[0].name, "server");
    assert_eq!(directives[0].children[2].children[0].name, "return");
}

#[test]
fn hash_mid_token_is_part_of_the_token() {
    let directives = parse_ok(
        "server {\n    set $x http://host/path#fragment;\n    set $y token#tag;\n}\n",
    );
    assert_eq!(directives[0].children[0].args, vec!["$x", "http://host/path#fragment"]);
    assert_eq!(directives[0].children[1].args, vec!["$y", "token#tag"]);
}

#[test]
fn quoted_strings_close_only_their_own_quote_type() {
    let directives = parse_ok(
        r#"server {
    set $a "it's fine";
    set $b 'he said "hi"';
    set $c "mix ' single inside double";
    set $d 'mix " double inside single';
    set $e "";
    set $f '';
}
"#,
    );
    let children = &directives[0].children;
    assert_eq!(children[0].args, vec!["$a", "it's fine"]);
    assert_eq!(children[1].args, vec!["$b", "he said \"hi\""]);
    assert_eq!(children[2].args, vec!["$c", "mix ' single inside double"]);
    assert_eq!(children[3].args, vec!["$d", "mix \" double inside single"]);
    assert_eq!(children[4].args, vec!["$e", ""]);
    assert_eq!(children[5].args, vec!["$f", ""]);
}

#[test]
fn escapes_in_quoted_and_unquoted_tokens() {
    let directives = parse_ok(
        r#"server {
    set $a "a\"b";
    set $b 'a\'b';
    set $c "tab\there";
    set $d "nl\nhere";
    set $e "back\\slash";
    set $f a\ b;
    set $g "unknown\qescape";
}
"#,
    );
    let children = &directives[0].children;
    assert_eq!(children[0].args, vec!["$a", "a\"b"]);
    assert_eq!(children[1].args, vec!["$b", "a'b"]);
    assert_eq!(children[2].args, vec!["$c", "tab\there"]);
    assert_eq!(children[3].args, vec!["$d", "nl\nhere"]);
    assert_eq!(children[4].args, vec!["$e", "back\\slash"]);
    // Unquoted escaped space keeps the backslash (nginx copy rule).
    assert_eq!(children[5].args, vec!["$f", "a\\ b"]);
    assert_eq!(children[6].args, vec!["$g", "unknown\\qescape"]);
}

#[test]
fn dollar_brace_variables_and_dollar_sequences() {
    let directives = parse_ok(
        r#"server {
    set $a ${name};
    set $b pre${mid}post;
    set $c "$quoted${brace}";
    set $d $$escaped;
}
"#,
    );
    let children = &directives[0].children;
    assert_eq!(children[0].args, vec!["$a", "${name}"]);
    assert_eq!(children[1].args, vec!["$b", "pre${mid}post"]);
    assert_eq!(children[2].args, vec!["$c", "$quoted${brace}"]);
    assert_eq!(children[3].args, vec!["$d", "$$escaped"]);
}

#[test]
fn errors_report_the_file_and_line() {
    // nginx reports the offending token's line: the `}` that ends the
    // unterminated directive sits on line 3.
    let error = parse("server {\n    listen 80\n}\n").err().expect("error");
    assert!(error.to_string().contains("edge.conf:3"), "{error}");
    let error = parse("server {\n\n\n    location / {\n        proxy_pass http://x\n    }\n}\n")
        .err()
        .expect("error");
    assert!(error.to_string().contains("edge.conf:6"), "{error}");
}

#[test]
fn adjacent_text_after_quotes_is_rejected() {
    for bad in [
        "server { set $x \"abc\"def; }",
        "server { set $x 'abc'def; }",
        "server { set $x \"abc\"#comment; }",
    ] {
        assert!(parse(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn unterminated_constructs_are_rejected() {
    for bad in [
        "server {",
        "server { set $x \"unterminated;",
        "server { set $x 'unterminated;",
        "server { set $x ;",
        "server { } }",
        "}",
        "server { set $x \\; }",
    ] {
        assert!(parse(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn parse_errors_never_panic_on_adversarial_input() {
    let cases = [
        "\u{0}", "\\", "\"", "'", "$", "${", "{}", "{{", "}}", "{{}}",
        "server { location / { location / { } } }",
        "server { location / { if ($x) { return 200; } } }",
        "server { server { server { server { } } } }",
        "server { listen 80; listen 80; listen 80; listen 80; listen 80; }",
        "a; b; c; d; e; f;",
        "\u{feff}",
        "server { # comment with no newline",
        "server { set $x \"a\" \"b\" \"c\" \"d\" \"e\" \"f\"; }",
        "server { set $x $y $z $w $v $u; }",
    ];
    for case in cases {
        let _ = parse(case);
    }
}

#[test]
fn included_files_keep_block_context() {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        root.path().join("frag.conf"),
        "location /from-include { return 200 \"inc\"; }\n",
    )
    .unwrap();
    let text = format!(
        "server {{\n    listen 80;\n    server_name inc.example.com;\n    include {};\n}}\n",
        root.path().join("frag.conf").display()
    );
    let parsed = parse_nginx_config(&text, Path::new("main.conf")).expect("parse");
    let mut budget = 16;
    let mut stack = Vec::new();
    let expanded = expand_includes(parsed, root.path(), &mut budget, &mut stack).expect("expand");
    let location = expanded[0]
        .children
        .iter()
        .find(|directive| directive.name == "location")
        .expect("location from the included file");
    assert_eq!(location.args, vec!["/from-include"]);
    assert_eq!(location.children[0].name, "return");
}
