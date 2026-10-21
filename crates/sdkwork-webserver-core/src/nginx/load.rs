//! Load stock nginx configuration (file, `sites-enabled` directory, and
//! companion `stream-conf.d`) into the runtime app model.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    expand_includes, materialize_nginx_app, merge_nginx_apps, parse_nginx_config, NginxConfigError,
};
use crate::config::ConfigDiagnostic;

/// Result of loading one nginx config path. `skipped` lists site files that
/// could not be materialized (progressive compatibility); `ignored` carries one
/// explicit diagnostic for every directive that was accepted for compatibility
/// but is not enforced by the runtime (PRD §3/§9: no silent acceptance).
pub struct NginxLoadReport {
    pub app: crate::config::WebServerAppConfig,
    pub skipped: Vec<(PathBuf, String)>,
    pub ignored: Vec<ConfigDiagnostic>,
}

/// Load a stock nginx config file, a directory of `*.conf` site files, or a
/// mixed tree. When `path` is a site directory, companion `stream-conf.d`
/// directories walking up from the path are also loaded so TCP/UDP `stream`
/// servers start alongside HTTP virtual hosts.
pub fn load_nginx_compat(path: &Path, app_key: &str) -> Result<NginxLoadReport, NginxConfigError> {
    let mut skipped = Vec::new();
    let mut ignored = Vec::new();
    let materialized = if path.is_file() {
        let (app, diagnostics) = load_one_file(path, app_key)?;
        ignored.extend(diagnostics);
        Some(app)
    } else if path.is_dir() {
        let mut materialized = load_directory(path, app_key, &mut skipped, &mut ignored)?;
        for stream_dir in companion_stream_directories(path) {
            if let Some(stream_app) =
                load_directory(&stream_dir, app_key, &mut skipped, &mut ignored)?
            {
                materialized = Some(match materialized {
                    Some(existing) => {
                        merge_nginx_apps(existing, stream_app).map_err(NginxConfigError::from)?
                    }
                    None => stream_app,
                });
            }
        }
        materialized
    } else {
        return Err(NginxConfigError::unsupported_path(
            path,
            format!("nginx config path {} does not exist", path.display()),
        ));
    };
    let Some(app) = materialized else {
        return Err(NginxConfigError::unsupported_path(
            path,
            format!(
                "no loadable nginx configuration at {}; skipped {} files",
                path.display(),
                skipped.len()
            ),
        ));
    };
    Ok(NginxLoadReport {
        app,
        skipped,
        ignored,
    })
}

fn load_directory(
    path: &Path,
    app_key: &str,
    skipped: &mut Vec<(PathBuf, String)>,
    ignored: &mut Vec<ConfigDiagnostic>,
) -> Result<Option<crate::config::WebServerAppConfig>, NginxConfigError> {
    let mut files = fs::read_dir(path)
        .map_err(|source| io_error(path, source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error(path, source))?;
    files.sort_by_key(|entry| entry.file_name());
    let mut materialized = None;
    for entry in files {
        let file = entry.path();
        if file.extension().and_then(|value| value.to_str()) != Some("conf") {
            continue;
        }
        match load_one_file(&file, app_key) {
            Ok((app, diagnostics)) => {
                ignored.extend(diagnostics);
                materialized = Some(match materialized {
                    Some(existing) => {
                        merge_nginx_apps(existing, app).map_err(NginxConfigError::from)?
                    }
                    None => app,
                });
            }
            Err(error) => skipped.push((file, error.to_string())),
        }
    }
    Ok(materialized)
}

fn load_one_file(
    path: &Path,
    app_key: &str,
) -> Result<(crate::config::WebServerAppConfig, Vec<ConfigDiagnostic>), NginxConfigError> {
    let text = crate::fs_bounds::read_text_capped(path, crate::fs_bounds::MAX_IMPORT_SOURCE_BYTES)
        .map_err(|source| io_error(path, source))?;
    let text = wrap_bare_stream_file(path, text);
    let parsed = parse_nginx_config(&text, path).map_err(NginxConfigError::from)?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut budget = 256;
    let mut stack = vec![path.to_path_buf()];
    let expanded = expand_includes(parsed, base_dir, &mut budget, &mut stack)
        .map_err(NginxConfigError::from)?;
    let ignored = super::mapping::ignored_directive_diagnostics(&expanded);
    for diagnostic in &ignored {
        tracing::warn!(
            path = %diagnostic.path,
            message = %diagnostic.message,
            "nginx compatibility directive is accepted but not enforced"
        );
    }
    let app = materialize_nginx_app(&expanded, base_dir, app_key)?;
    Ok((app, ignored))
}

/// Bare `stream-conf.d` fragments are `server { listen; proxy_pass; }` without
/// a wrapping `stream { }` block. Wrap them so the mapper treats them as
/// stream servers instead of HTTP virtual hosts.
fn wrap_bare_stream_file(path: &Path, text: String) -> String {
    if !is_stream_config_path(path) {
        return text;
    }
    if text
        .lines()
        .any(|line| line.trim_start().starts_with("stream"))
    {
        return text;
    }
    format!("stream {{\n{text}\n}}\n")
}

fn is_stream_config_path(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if file_name.contains(".stream.conf") || file_name.ends_with(".stream") {
        return true;
    }
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        == Some("stream-conf.d")
}

fn companion_stream_directories(start: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut current = start;
    for _ in 0..6 {
        let Some(parent) = current.parent() else {
            break;
        };
        let candidate = parent.join("stream-conf.d");
        if candidate.is_dir() && !found.contains(&candidate) {
            found.push(candidate);
        }
        current = parent;
    }
    found
}

fn io_error(path: &Path, source: io::Error) -> NginxConfigError {
    NginxConfigError::unsupported_path(path, format!("cannot read {}: {source}", path.display()))
}

impl NginxConfigError {
    pub(crate) fn unsupported_path(path: &Path, message: impl std::fmt::Display) -> Self {
        Self::Unsupported {
            path: path.to_path_buf(),
            line: 0,
            message: message.to_string(),
        }
    }
}

impl From<super::parser::NginxParseError> for NginxConfigError {
    fn from(error: super::parser::NginxParseError) -> Self {
        match error {
            super::parser::NginxParseError::Syntax {
                path,
                line,
                message,
            } => Self::Unsupported {
                path,
                line,
                message,
            },
            other => Self::unsupported_path(Path::new("."), other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_sites_and_companion_stream_directory() {
        let root = tempfile::tempdir().expect("temp");
        let sites = root.path().join("sites-enabled");
        let stream_dir = root.path().join("stream-conf.d");
        fs::create_dir_all(&sites).unwrap();
        fs::create_dir_all(&stream_dir).unwrap();
        fs::write(
            sites.join("web.conf"),
            r#"
server {
    listen 80;
    server_name web.example.com;
    location /healthz { return 200 "ok"; }
    location / { proxy_pass http://127.0.0.1:18080; }
}
"#,
        )
        .unwrap();
        fs::write(
            stream_dir.join("im.stream.conf"),
            r#"
server {
    listen 5100;
    proxy_pass 127.0.0.1:15100;
    proxy_timeout 1h;
}
"#,
        )
        .unwrap();
        let report = load_nginx_compat(&sites, "nginx-compat").expect("load");
        assert_eq!(report.app.virtual_hosts.len(), 1);
        assert_eq!(report.app.streams.len(), 1);
        assert_eq!(report.app.streams[0].port, 5100);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }

    /// PRD §3/§9: a directive the runtime accepts but does not enforce must
    /// produce an explicit diagnostic with its source location, never a
    /// silent drop.
    #[test]
    fn accepted_but_unenforced_directives_are_reported() {
        let root = tempfile::tempdir().expect("temp");
        let sites = root.path().join("sites-enabled");
        fs::create_dir_all(&sites).unwrap();
        fs::write(
            sites.join("web.conf"),
            r#"
server {
    listen 80;
    server_name web.example.com;
    proxy_read_timeout 300s;
    proxy_buffering off;
    gzip_static on;
    location / { proxy_pass http://127.0.0.1:18080; }
}
"#,
        )
        .unwrap();
        let report = load_nginx_compat(&sites, "nginx-compat").expect("load");
        assert_eq!(
            report.ignored.len(),
            3,
            "every accepted-but-unenforced directive needs one diagnostic: {:?}",
            report.ignored
        );
        for expected in ["proxy_read_timeout", "proxy_buffering", "gzip_static"] {
            assert!(
                report.ignored.iter().any(|diagnostic| {
                    diagnostic.message.contains(expected) && diagnostic.path.contains("web.conf:")
                }),
                "missing diagnostic for {expected}: {:?}",
                report.ignored
            );
        }
        assert!(
            report
                .ignored
                .iter()
                .all(|diagnostic| diagnostic.message.contains("no effect")),
            "diagnostics must state the value has no effect: {:?}",
            report.ignored
        );
    }
}
