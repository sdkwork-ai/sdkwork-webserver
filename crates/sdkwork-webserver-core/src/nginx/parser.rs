//! nginx configuration lexer/parser and `include` expansion.
//!
//! The parser accepts the nginx configuration language subset used by
//! `server`/`upstream`/`location` blocks: directive names, quoted or
//! unquoted arguments (including `$variables`), `{ }` blocks, `;`
//! terminators, and `#` comments. It is intentionally strict: unknown
//! syntax fails with the file and line so operators can see exactly what
//! the runtime cannot consume.

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

/// Parse one nginx configuration text into top-level directives.
pub fn parse_nginx_config(text: &str, source: &Path) -> Result<Vec<NginxDirective>, NginxParseError> {
    let mut lexer = Lexer::new(text, source);
    lexer.parse_directives(None)
}

/// Expand `include` directives in a directive list, resolving paths relative
/// to `base_dir` (globs like `sites-enabled/*.conf` supported). The expanded
/// list replaces the including directive in place; `map`-style includes of
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
            if pattern.contains('*') {
                let pattern_text = pattern_path.to_string_lossy().replace('\\', "/");
                let (directory, file_pattern) = match pattern_text.rsplit_once('/') {
                    Some((directory, file_pattern)) => (directory.to_owned(), file_pattern.to_owned()),
                    None => (String::from("."), pattern_text),
                };
                let entries = fs::read_dir(&directory).map_err(|source| NginxParseError::Read {
                    path: PathBuf::from(&directory),
                    source,
                })?;
                let mut collected = Vec::new();
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if glob_match(&file_pattern, &name) {
                        collected.push(entry.path());
                    }
                }
                collected.sort();
                matches = collected;
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
                let text = fs::read_to_string(&include_path).map_err(|source| {
                    NginxParseError::Read {
                        path: include_path.clone(),
                        source,
                    }
                })?;
                let include_dir = include_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                stack.push(include_path.clone());
                let parsed = parse_nginx_config(&text, &include_path)?;
                let nested = expand_includes(parsed, &include_dir, budget, stack)?;
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

fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    pattern == name
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
    ) -> Result<Vec<NginxDirective>, NginxParseError> {
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
            directives.push(self.parse_directive()?);
        }
    }

    fn parse_directive(&mut self) -> Result<NginxDirective, NginxParseError> {
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
                    children = self.parse_directives(Some('}'))?;
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

    fn parse_token(&mut self) -> Result<String, NginxParseError> {
        self.skip_whitespace_and_comments();
        let Some(first) = self.peek() else {
            return Ok(String::new());
        };
        if first == '"' || first == '\'' {
            let quote = first;
            self.position += 1;
            let mut token = String::new();
            loop {
                let Some(ch) = self.peek() else {
                    return Err(self.syntax("unterminated quoted string"));
                };
                self.position += ch.len_utf8();
                if ch == quote {
                    break;
                }
                if ch == '\n' {
                    self.line += 1;
                }
                token.push(ch);
            }
            return Ok(token);
        }
        let start = self.position;
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || matches!(ch, ';' | '{' | '}' | '#' | '"' | '\'') {
                break;
            }
            if ch == '\n' {
                break;
            }
            self.position += ch.len_utf8();
        }
        Ok(self.text[start..self.position].to_owned())
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
        let text = format!(
            "include {};\n",
            directory.path().join("*.conf").display()
        );
        let parsed = parse_nginx_config(&text, Path::new("main.conf")).expect("parse");
        let mut budget = 16;
        let mut stack = Vec::new();
        let expanded =
            expand_includes(parsed, directory.path(), &mut budget, &mut stack).expect("expand");
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].children[0].args, vec!["1"]);
        assert_eq!(expanded[1].children[0].args, vec!["2"]);
    }
}
