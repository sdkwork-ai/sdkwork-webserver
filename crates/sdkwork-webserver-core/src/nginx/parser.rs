//! nginx configuration lexer/parser and `include` expansion.
//!
//! The tokenizer mirrors `ngx_conf_read_token()` (nginx 1.26.2): directive
//! names and arguments separated by whitespace, `;` terminators, `{ }`
//! blocks, `#` comments at token boundaries, single- and double-quoted
//! strings (each quote type closes only itself), and backslash escapes
//! (`\"` `\'` `\\` collapse, `\t` `\r` `\n` become control characters, any
//! other `\c` is preserved verbatim). Mid-token `#`, `}`, and quotes are
//! ordinary characters exactly as nginx tokenizes them, and a `{` directly
//! after `$` stays inside the token (`${name}` variables).
//!
//! `include` follows nginx: patterns containing `*`, `?`, or `[` are globs
//! matched with libc-glob semantics (`*` any sequence, `?` one character,
//! `[a-z]`/`[!a-z]` classes) and expanded in sorted order; a glob matching
//! no files is a no-op; a literal include path must exist. Relative include
//! paths resolve against the top-level loaded configuration directory
//! (nginx resolves them against the main `nginx.conf` directory), not the
//! including file's own directory.

use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

/// One nginx directive (a name, arguments, and an optional block body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NginxDirective {
    pub name: String,
    pub args: Vec<String>,
    pub children: Vec<NginxDirective>,
    pub line: usize,
    pub source: PathBuf,
}

#[derive(Debug, Error)]
pub enum NginxParseError {
    #[error("{path}:{line}: {message}")]
    Syntax {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("include path {path} matches no files")]
    IncludeMissing { path: String },
    #[error("include {path} exceeds the {maximum} file expansion budget")]
    IncludeBudget { path: String, maximum: usize },
    #[error("include cycle detected at {path}")]
    IncludeCycle { path: PathBuf },
}

/// Maximum `{` nesting depth. nginx has no limit, but the recursive
/// tokenizer must never overflow the stack on pathological input; deeper
/// configurations fail closed with a precise diagnostic.
const MAXIMUM_BLOCK_DEPTH: usize = 256;

/// Parse one nginx configuration text into top-level directives.
pub fn parse_nginx_config(
    text: &str,
    source: &Path,
) -> Result<Vec<NginxDirective>, NginxParseError> {
    let mut lexer = Lexer::new(text, source);
    lexer.parse_directives(None, 0)
}

/// Expand `include` directives in a directive list, resolving paths relative
/// to `base_dir` — the top-level loaded configuration directory (nginx
/// `conf_prefix` semantics; relative paths never resolve against the
/// including file's own directory). Globs like `sites-enabled/*.conf` are
/// supported; a glob that matches nothing expands to nothing, and a literal
/// missing path is an error, both matching nginx. `map`-style includes of
/// fragments keep their block context because expansion happens per level.
pub fn expand_includes(
    directives: Vec<NginxDirective>,
    base_dir: &Path,
    budget: &mut usize,
    stack: &mut Vec<PathBuf>,
) -> Result<Vec<NginxDirective>, NginxParseError> {
    const MAXIMUM_INCLUDE_FILES: usize = 256;
    let mut expanded = Vec::new();
    for directive in directives {
        if directive.name == "include" {
            let Some(pattern) = directive.args.first() else {
                return Err(NginxParseError::Syntax {
                    path: directive.source.clone(),
                    line: directive.line,
                    message: "include requires a path pattern".to_owned(),
                });
            };
            if *budget == 0 {
                return Err(NginxParseError::IncludeBudget {
                    path: pattern.clone(),
                    maximum: MAXIMUM_INCLUDE_FILES,
                });
            }
            *budget -= 1;
            let pattern_path = if Path::new(pattern).is_absolute() {
                PathBuf::from(pattern)
            } else {
                base_dir.join(pattern)
            };
            let mut matches = Vec::new();
            if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
                // nginx globs the full pattern path (directory segments can
                // carry glob characters too); matches are sorted because
                // nginx globs without GLOB_NOSORT.
                matches = expand_glob_pattern(&pattern_path)?;
            } else if pattern_path.is_file() {
                matches.push(pattern_path);
            } else {
                return Err(NginxParseError::IncludeMissing {
                    path: pattern.clone(),
                });
            }
            for include_path in matches {
                if stack.contains(&include_path) {
                    return Err(NginxParseError::IncludeCycle { path: include_path });
                }
                let text =
                    fs::read_to_string(&include_path).map_err(|source| NginxParseError::Read {
                        path: include_path.clone(),
                        source,
                    })?;
                stack.push(include_path.clone());
                let parsed = parse_nginx_config(&text, &include_path)?;
                // Relative includes inside the included file resolve against
                // the root `base_dir` (nginx `conf_prefix`), not this file's
                // own directory.
                let nested = expand_includes(parsed, base_dir, budget, stack)?;
                stack.pop();
                expanded.extend(nested);
            }
        } else {
            let children = expand_includes(directive.children, base_dir, budget, stack)?;
            expanded.push(NginxDirective {
                children,
                ..directive
            });
        }
    }
    Ok(expanded)
}

/// Expand a glob `include` pattern across the whole path, walking each
/// segment left to right (libc `glob()` semantics: `*`, `?`, `[a-z]`
/// classes on every segment, literal segments joined directly). Only
/// regular files are returned, in sorted order; a pattern matching no files
/// yields an empty list (nginx include tolerates empty glob matches).
fn expand_glob_pattern(pattern: &Path) -> Result<Vec<PathBuf>, NginxParseError> {
    let text = pattern.to_string_lossy().replace('\\', "/");
    let absolute = text.starts_with('/');
    // Windows drive prefix (`C:/...`): start the walk at the drive root so
    // joined paths stay absolute instead of drive-relative.
    let drive_prefix =
        (text.len() >= 3 && text.as_bytes()[1] == b':' && text.as_bytes()[2] == b'/')
            .then(|| &text[..3]);
    let mut segments = text
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if drive_prefix.is_some() {
        segments.remove(0);
    }
    let mut current = if let Some(prefix) = drive_prefix {
        vec![PathBuf::from(prefix)]
    } else if absolute {
        vec![PathBuf::from("/")]
    } else {
        vec![PathBuf::new()]
    };
    for (index, segment) in segments.iter().enumerate() {
        let is_last = index + 1 == segments.len();
        let mut next = Vec::new();
        if segment.contains('*') || segment.contains('?') || segment.contains('[') {
            for base in &current {
                let entries = fs::read_dir(base).map_err(|source| NginxParseError::Read {
                    path: base.clone(),
                    source,
                })?;
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !glob_match(segment, &name) {
                        continue;
                    }
                    let candidate = base.join(&name);
                    if is_last {
                        if candidate.is_file() {
                            next.push(candidate);
                        }
                    } else if candidate.is_dir() {
                        next.push(candidate);
                    }
                }
            }
        } else {
            for base in &current {
                let candidate = base.join(segment);
                if is_last {
                    if candidate.is_file() {
                        next.push(candidate);
                    }
                } else if candidate.is_dir() {
                    next.push(candidate);
                }
            }
        }
        current = next;
    }
    current.sort();
    Ok(current)
}

/// libc-glob-style file pattern matching used by nginx `include`:
/// `*` matches any (possibly empty) sequence, `?` exactly one character,
/// and `[a-z]` / `[!a-z]` / `[^a-z]` character classes with ranges. An
/// unterminated `[` is a literal character (libc glob behavior).
fn glob_match(pattern: &str, name: &str) -> bool {
    glob_match_bytes(pattern.as_bytes(), name.as_bytes())
}

fn glob_match_bytes(pattern: &[u8], name: &[u8]) -> bool {
    if let Some(rest) = pattern.strip_prefix(b"*") {
        // `*` greedily consumes; backtrack when the remainder cannot match.
        for split in 0..=name.len() {
            if glob_match_bytes(rest, &name[split..]) {
                return true;
            }
        }
        return false;
    }
    let Some((&first, name_rest)) = name.split_first() else {
        return pattern.is_empty();
    };
    let Some((&p, pattern_rest)) = pattern.split_first() else {
        return false;
    };
    match p {
        b'?' => glob_match_bytes(pattern_rest, name_rest),
        b'[' => {
            // Character class: `[!...]` / `[^...]` negate, `a-z` ranges apply.
            let Some(end) = pattern_rest.iter().position(|&byte| byte == b']') else {
                // Unterminated class is a literal `[`.
                return first == b'[' && glob_match_bytes(pattern_rest, name_rest);
            };
            let class = &pattern_rest[..end];
            let (negated, class) = match class.split_first() {
                Some((&b'!' | &b'^', rest)) => (true, rest),
                _ => (false, class),
            };
            let matched = class_matches(class, first);
            if matched != negated {
                glob_match_bytes(&pattern_rest[end + 1..], name_rest)
            } else {
                false
            }
        }
        _ => first == p && glob_match_bytes(pattern_rest, name_rest),
    }
}

fn class_matches(class: &[u8], byte: u8) -> bool {
    let mut index = 0;
    while index < class.len() {
        if index + 2 < class.len() && class[index + 1] == b'-' {
            let (start, end) = (class[index], class[index + 2]);
            if start <= byte && byte <= end {
                return true;
            }
            index += 3;
        } else {
            if class[index] == byte {
                return true;
            }
            index += 1;
        }
    }
    false
}

struct Lexer<'a> {
    text: &'a str,
    source: &'a Path,
    position: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str, source: &'a Path) -> Self {
        Self {
            text,
            source,
            position: 0,
            line: 1,
        }
    }

    fn parse_directives(
        &mut self,
        closing: Option<char>,
        depth: usize,
    ) -> Result<Vec<NginxDirective>, NginxParseError> {
        if depth > MAXIMUM_BLOCK_DEPTH {
            return Err(self.syntax(format!(
                "block nesting exceeds the {MAXIMUM_BLOCK_DEPTH} level limit"
            )));
        }
        let mut directives = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let Some(next) = self.peek() else {
                if closing.is_some() {
                    return Err(self.syntax("unexpected end of file inside a block"));
                }
                return Ok(directives);
            };
            if next == '}' {
                if closing != Some('}') {
                    return Err(self.syntax("unexpected '}'"));
                }
                self.position += 1;
                return Ok(directives);
            }
            directives.push(self.parse_directive(depth)?);
        }
    }

    fn parse_directive(&mut self, depth: usize) -> Result<NginxDirective, NginxParseError> {
        let name = self.parse_token()?;
        if name.is_empty() {
            return Err(self.syntax("expected a directive name"));
        }
        let line = self.line;
        let mut args = Vec::new();
        let mut children = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let Some(next) = self.peek() else {
                return Err(NginxParseError::Syntax {
                    path: self.source.to_path_buf(),
                    line,
                    message: format!("directive `{name}` is not terminated"),
                });
            };
            match next {
                ';' => {
                    self.position += 1;
                    break;
                }
                '{' => {
                    self.position += 1;
                    children = self.parse_directives(Some('}'), depth + 1)?;
                    break;
                }
                '}' => {
                    return Err(self.syntax(format!("directive `{name}` is missing ';' or '{{'")));
                }
                _ => args.push(self.parse_token()?),
            }
        }
        Ok(NginxDirective {
            name,
            args,
            children,
            line,
            source: self.source.to_path_buf(),
        })
    }

    /// Read one argument token with nginx tokenizer semantics.
    ///
    /// Mirrors `ngx_conf_read_token()`: `"` / `'` start a quoted string at a
    /// word boundary (each quote type closes only itself; a quote character
    /// mid-word is an ordinary character); `\` escapes the next character;
    /// a `{` directly after `$` is part of the token (`${name}`); `#` and
    /// `}` mid-word are ordinary characters. After a closing quote the next
    /// character must be whitespace, `;`, `{`, `}`, or `)` (nginx
    /// `need_space` state), otherwise the configuration is rejected.
    fn parse_token(&mut self) -> Result<String, NginxParseError> {
        self.skip_whitespace_and_comments();
        if self.peek().is_none() {
            return Ok(String::new());
        }
        let mut raw = String::new();
        let mut double_quoted = false;
        let mut single_quoted = false;
        let mut escaped = false;
        let mut variable = false;
        let mut at_word_start = true;
        let mut word_complete = false;
        loop {
            let Some(ch) = self.peek() else {
                if double_quoted || single_quoted {
                    return Err(self.syntax("unterminated quoted string"));
                }
                break;
            };
            if escaped {
                // The escaped character is consumed raw: it cannot terminate
                // the word, open/close a quote, or start a comment.
                escaped = false;
                self.position += ch.len_utf8();
                if ch == '\n' {
                    self.line += 1;
                }
                raw.push(ch);
                at_word_start = false;
                continue;
            }
            if double_quoted {
                self.position += ch.len_utf8();
                if ch == '\n' {
                    self.line += 1;
                }
                if ch == '\\' {
                    // nginx: inside a quoted string `\` escapes the next
                    // character (including the closing quote).
                    raw.push('\\');
                    escaped = true;
                } else if ch == '"' {
                    double_quoted = false;
                    word_complete = true;
                } else {
                    raw.push(ch);
                }
                continue;
            }
            if single_quoted {
                self.position += ch.len_utf8();
                if ch == '\n' {
                    self.line += 1;
                }
                if ch == '\\' {
                    raw.push('\\');
                    escaped = true;
                } else if ch == '\'' {
                    single_quoted = false;
                    word_complete = true;
                } else {
                    raw.push(ch);
                }
                continue;
            }
            if word_complete {
                // nginx `need_space`: after a closing quote the token is
                // complete and the next character must be whitespace, `;`,
                // `{`, `}`, or `)`.
                match ch {
                    ' ' | '\t' | '\r' | '\n' => {
                        self.position += ch.len_utf8();
                        if ch == '\n' {
                            self.line += 1;
                        }
                    }
                    ')' => {
                        self.position += 1;
                    }
                    ';' | '{' | '}' => break,
                    _ => {
                        return Err(self
                            .syntax(format!("unexpected character {ch:?} after a quoted string")));
                    }
                }
                return Ok(unescape_word(&raw));
            }
            if at_word_start {
                match ch {
                    ' ' | '\t' | '\r' | '\n' => {
                        self.position += ch.len_utf8();
                        if ch == '\n' {
                            self.line += 1;
                        }
                    }
                    // `#` at a token boundary starts a comment; the token
                    // ends here and the caller skips the comment.
                    '#' | ';' | '{' => break,
                    '\\' => {
                        self.position += 1;
                        raw.push('\\');
                        escaped = true;
                        at_word_start = false;
                    }
                    '"' => {
                        self.position += 1;
                        double_quoted = true;
                        at_word_start = false;
                    }
                    '\'' => {
                        self.position += 1;
                        single_quoted = true;
                        at_word_start = false;
                    }
                    '$' => {
                        self.position += ch.len_utf8();
                        raw.push('$');
                        variable = true;
                        at_word_start = false;
                    }
                    _ => {
                        self.position += ch.len_utf8();
                        raw.push(ch);
                        at_word_start = false;
                    }
                }
                continue;
            }
            // Mid-word: `{` directly after `$` belongs to the token
            // (`${name}`); `}` and `#` are ordinary characters.
            if ch == '{' && variable {
                self.position += ch.len_utf8();
                raw.push('{');
                variable = false;
                continue;
            }
            variable = false;
            if ch == '\\' {
                self.position += 1;
                raw.push('\\');
                escaped = true;
                continue;
            }
            if ch == '$' {
                self.position += ch.len_utf8();
                raw.push('$');
                variable = true;
                continue;
            }
            if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' || ch == ';' || ch == '{' {
                break;
            }
            self.position += ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
            }
            raw.push(ch);
        }
        Ok(unescape_word(&raw))
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while let Some(ch) = self.peek() {
                if ch == '\n' {
                    self.line += 1;
                    self.position += ch.len_utf8();
                } else if ch.is_whitespace() {
                    self.position += ch.len_utf8();
                } else {
                    break;
                }
            }
            if self.peek() == Some('#') {
                while let Some(ch) = self.peek() {
                    self.position += ch.len_utf8();
                    if ch == '\n' {
                        self.line += 1;
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.position..].chars().next()
    }

    fn syntax(&self, message: impl std::fmt::Display) -> NginxParseError {
        NginxParseError::Syntax {
            path: self.source.to_path_buf(),
            line: self.line,
            message: message.to_string(),
        }
    }
}

/// Apply the nginx word-copy escape table (`ngx_conf_read_token`): `\"`,
/// `\'`, and `\\` collapse to the escaped character, `\t` `\r` `\n` become
/// control characters, and every other `\c` pair is preserved verbatim.
fn unescape_word(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            // The backslash collapses; the escaped character is kept.
            Some(escaped @ ('"' | '\'' | '\\')) => output.push(escaped),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            Some('n') => output.push('\n'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Vec<NginxDirective> {
        parse_nginx_config(text, Path::new("test.conf")).expect("parse")
    }

    #[test]
    fn parses_server_upstream_and_location_blocks() {
        let directives = parse(
            r#"
# comment
upstream backend {
    server 127.0.0.1:8080 weight=2 max_fails=3;
    keepalive 16;
}

server {
    listen 80;
    listen [::]:80;
    server_name example.com www.example.com;
    location /api/ {
        proxy_pass http://backend;
        proxy_set_header Host $host;
    }
    location = /healthz {
        return 200 "ok";
    }
}
"#,
        );
        assert_eq!(directives.len(), 2);
        let upstream = &directives[0];
        assert_eq!(upstream.name, "upstream");
        assert_eq!(upstream.args, vec!["backend"]);
        assert_eq!(upstream.children[0].name, "server");
        assert_eq!(
            upstream.children[0].args,
            vec!["127.0.0.1:8080", "weight=2", "max_fails=3"]
        );
        let server = &directives[1];
        assert_eq!(server.name, "server");
        let locations = server
            .children
            .iter()
            .filter(|directive| directive.name == "location")
            .collect::<Vec<_>>();
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].args, vec!["/api/"]);
        assert_eq!(locations[0].children[0].name, "proxy_pass");
        assert_eq!(locations[0].children[0].args, vec!["http://backend"]);
        assert_eq!(locations[1].args, vec!["=", "/healthz"]);
        assert_eq!(locations[1].children[0].args, vec!["200", "ok"]);
    }

    #[test]
    fn quoted_and_variable_arguments_are_preserved() {
        let directives = parse(
            r#"server {
    location / {
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Host "$host:$server_port";
        return 301 https://$host$request_uri;
    }
}"#,
        );
        let location = &directives[0].children[0];
        assert_eq!(
            location.children[0].args,
            vec!["X-Forwarded-Proto", "$scheme"]
        );
        assert_eq!(
            location.children[1].args,
            vec!["Host", "$host:$server_port"]
        );
        assert_eq!(
            location.children[2].args,
            vec!["301", "https://$host$request_uri"]
        );
    }

    #[test]
    fn multibyte_characters_do_not_break_tokenization() {
        let directives = parse(
            "server {
    server_name 中文.example.com;
    location /中/ {
        return 200 \"ok§\";
    }
}
",
        );
        let server = &directives[0];
        assert_eq!(server.children[0].args, vec!["中文.example.com"]);
        assert_eq!(server.children[1].args, vec!["/中/"]);
    }

    #[test]
    fn syntax_errors_carry_file_and_line() {
        let error = parse_nginx_config("server {\n    listen 80\n", Path::new("bad.conf"))
            .expect_err("unterminated block must fail");
        let message = error.to_string();
        assert!(message.contains("bad.conf:2"), "{message}");
        assert!(message.contains("not terminated"), "{message}");
    }

    #[test]
    fn include_globs_expand_in_directory_order() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("a.conf"), "server { listen 1; }\n").unwrap();
        std::fs::write(directory.path().join("b.conf"), "server { listen 2; }\n").unwrap();
        let text = format!("include {};\n", directory.path().join("*.conf").display());
        let parsed = parse_nginx_config(&text, Path::new("main.conf")).expect("parse");
        let mut budget = 16;
        let mut stack = Vec::new();
        let expanded =
            expand_includes(parsed, directory.path(), &mut budget, &mut stack).expect("expand");
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].children[0].args, vec!["1"]);
        assert_eq!(expanded[1].children[0].args, vec!["2"]);
    }

    #[test]
    fn quote_escapes_follow_nginx_word_copy_rules() {
        let directives = parse(
            r#"server {
    set $escaped "a\"b";
    set $tab "a\tb";
    set $literal "a\qb";
    set $mixed 'it"s fine';
}"#,
        );
        let children = &directives[0].children;
        assert_eq!(children[0].args, vec!["$escaped", "a\"b"]);
        assert_eq!(children[1].args, vec!["$tab", "a\tb"]);
        assert_eq!(children[2].args, vec!["$literal", "a\\qb"]);
        assert_eq!(children[3].args, vec!["$mixed", "it\"s fine"]);
    }

    #[test]
    fn hash_and_close_brace_are_ordinary_mid_token() {
        let directives = parse(
            r#"server {
    proxy_pass http://127.0.0.1:8080/faq#section;
    server_name example.com#internal;
}"#,
        );
        let children = &directives[0].children;
        assert_eq!(children[0].args, vec!["http://127.0.0.1:8080/faq#section"]);
        assert_eq!(children[1].args, vec!["example.com#internal"]);
    }

    #[test]
    fn dollar_brace_variables_stay_in_one_token() {
        let directives = parse(
            r#"server {
    set $combined "pre${suffix}post";
    proxy_set_header X-Test ${header_name};
}"#,
        );
        let children = &directives[0].children;
        assert_eq!(children[0].args, vec!["$combined", "pre${suffix}post"]);
        assert_eq!(children[1].args, vec!["X-Test", "${header_name}"]);
    }

    #[test]
    fn escaped_terminators_do_not_end_the_token() {
        let directives = parse(
            r#"server {
    set $semi "a\;b";
    set $brace a\{b;
    set $slash "a\\b";
}"#,
        );
        let children = &directives[0].children;
        assert_eq!(children[0].args, vec!["$semi", "a\\;b"]);
        assert_eq!(children[1].args, vec!["$brace", "a\\{b"]);
        assert_eq!(children[2].args, vec!["$slash", "a\\b"]);
    }

    #[test]
    fn adjacent_text_after_quoted_string_is_rejected() {
        let error = parse_nginx_config(
            "server {\n    set $x \"abc\"def;\n}\n",
            Path::new("bad.conf"),
        )
        .expect_err("nginx need_space rule must reject adjacent text");
        assert!(
            error.to_string().contains("after a quoted string"),
            "{error}"
        );
    }

    #[test]
    fn unterminated_quoted_string_is_rejected() {
        let error = parse_nginx_config("set $x \"abc;\n", Path::new("bad.conf"))
            .expect_err("unterminated quote must fail");
        assert!(
            error.to_string().contains("unterminated quoted string"),
            "{error}"
        );
    }

    #[test]
    fn question_mark_and_character_class_globs_expand() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            directory.path().join("site-a.conf"),
            "server { listen 1; }\n",
        )
        .unwrap();
        std::fs::write(
            directory.path().join("site-b.conf"),
            "server { listen 2; }\n",
        )
        .unwrap();
        std::fs::write(directory.path().join("other.txt"), "ignore\n").unwrap();
        let pattern = directory.path().join("site-?.conf");
        let parsed = parse_nginx_config(
            &format!("include {};\n", pattern.display()),
            Path::new("main.conf"),
        )
        .expect("parse");
        let mut budget = 16;
        let mut stack = Vec::new();
        let expanded =
            expand_includes(parsed, directory.path(), &mut budget, &mut stack).expect("expand");
        assert_eq!(expanded.len(), 2);
        let pattern = directory.path().join("site-[ab].conf");
        let parsed = parse_nginx_config(
            &format!("include {};\n", pattern.display()),
            Path::new("main.conf"),
        )
        .expect("parse");
        let expanded =
            expand_includes(parsed, directory.path(), &mut budget, &mut stack).expect("expand");
        assert_eq!(expanded.len(), 2);
    }

    #[test]
    fn empty_glob_include_is_a_noop_like_nginx() {
        let directory = tempfile::tempdir().expect("temp dir");
        let parsed = parse_nginx_config(
            &format!(
                "include {};\n",
                directory.path().join("missing-*.conf").display()
            ),
            Path::new("main.conf"),
        )
        .expect("parse");
        let mut budget = 16;
        let mut stack = Vec::new();
        let expanded =
            expand_includes(parsed, directory.path(), &mut budget, &mut stack).expect("expand");
        assert!(expanded.is_empty());
    }

    #[test]
    fn globs_in_directory_segments_expand_like_libc_glob() {
        let root = tempfile::tempdir().expect("temp dir");
        let a = root.path().join("sites-a");
        let b = root.path().join("sites-b");
        let other = root.path().join("not-sites");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(
            a.join("one.conf"),
            "server { listen 1; }
",
        )
        .unwrap();
        std::fs::write(
            b.join("two.conf"),
            "server { listen 2; }
",
        )
        .unwrap();
        std::fs::write(
            other.join("x.conf"),
            "server { listen 9; }
",
        )
        .unwrap();
        let parsed = parse_nginx_config(
            &format!(
                "include {};
",
                root.path().join("sites-*/*.conf").display()
            ),
            Path::new("main.conf"),
        )
        .expect("parse");
        let mut budget = 16;
        let mut stack = Vec::new();
        let expanded =
            expand_includes(parsed, root.path(), &mut budget, &mut stack).expect("expand");
        assert_eq!(expanded.len(), 2);
        let listens = expanded
            .iter()
            .map(|directive| directive.children[0].args[0].clone())
            .collect::<Vec<_>>();
        assert_eq!(listens, vec!["1", "2"], "sorted across directory matches");
    }

    #[test]
    fn nested_includes_resolve_against_the_root_directory() {
        let root = tempfile::tempdir().expect("temp dir");
        let snippets = root.path().join("snippets");
        std::fs::create_dir_all(&snippets).unwrap();
        std::fs::write(
            root.path().join("fragments"),
            "include snippets/frag.conf;\n",
        )
        .unwrap();
        std::fs::write(snippets.join("frag.conf"), "server { listen 7; }\n").unwrap();
        let parsed =
            parse_nginx_config("include fragments;\n", Path::new("main.conf")).expect("parse");
        let mut budget = 16;
        let mut stack = Vec::new();
        let expanded =
            expand_includes(parsed, root.path(), &mut budget, &mut stack).expect("expand");
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].children[0].args, vec!["7"]);
    }
}
