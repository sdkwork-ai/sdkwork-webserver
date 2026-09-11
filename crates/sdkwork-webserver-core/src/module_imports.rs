//! Imported sibling-module `deployments/webserver/` validation.
//!
//! Operators declare other SDKWork modules whose layout v3 web server
//! configuration must be present and materializable before the gateway starts.
//! Paths may be **absolute** or **relative**:
//!
//! - **Layout v3 TOML directory** (`/etc/sdkwork/iam/deployments/webserver`,
//!   module root, or relative path): loaded through the standard
//!   `server.common.toml` merge pipeline.
//! - **Stock nginx `.conf` file** (`nginx.standalone.development.conf` or an
//!   entry listed in `imports.d/import.conf`): loaded through the nginx
//!   compatibility loader and merged into the module-imports data plane.

use std::{
    collections::HashSet,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;

use crate::config::{
    load_server_toml_app, ConfigFormat, ConfigLoadOptions, ResourceConfig, WebServerAppConfig,
    WebServerConfigError, WebServerConfigLoader,
};

/// Comma-separated `id=path` pairs override/supplement runtime TOML imports.
pub const MODULE_IMPORTS_ENV: &str = "SDKWORK_WEBSERVER_MODULE_IMPORTS";

/// Standard subdirectory under a module root (`SDKWORK_WEBSERVER_SPEC.md`).
pub const WEBSERVER_DEPLOY_SUBDIR: &str = "deployments/webserver";

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// One imported module webserver directory (`deployments/webserver/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebserverModuleImport {
    pub id: String,
    pub path: PathBuf,
    pub profile: Option<String>,
    pub enabled: bool,
    pub required: bool,
    pub probe_upstreams: bool,
}

impl WebserverModuleImport {
    pub fn webserver_dir(&self) -> &Path {
        &self.path
    }
}

/// Outcome of validating one import.
#[derive(Debug, Clone)]
pub struct ModuleImportValidation {
    pub import: WebserverModuleImport,
    pub profile: String,
    pub app_key: String,
    pub virtual_host_count: usize,
    pub upstream_count: usize,
    pub probed_upstreams: Vec<String>,
    pub unreachable_upstreams: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleImportError {
    #[error("module import `{id}`: {message}")]
    InvalidSpec { id: String, message: String },

    #[error("module import `{id}` at {path}: {source}")]
    Validation {
        id: String,
        path: PathBuf,
        #[source]
        source: WebServerConfigError,
    },

    #[error(
        "module import `{id}` at {path}: required upstream `{upstream}` is unreachable ({detail})"
    )]
    UnreachableUpstream {
        id: String,
        path: PathBuf,
        upstream: String,
        detail: String,
    },
}

/// JSON/env import entry (runtime TOML uses the same shape).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebserverImportEntry {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default = "default_true")]
    pub probe_upstreams: bool,
}

fn default_true() -> bool {
    true
}

impl WebserverImportEntry {
    fn into_import(self, base: &Path) -> Result<WebserverModuleImport, ModuleImportError> {
        let id = self.id.trim().to_owned();
        if id.is_empty() {
            return Err(ModuleImportError::InvalidSpec {
                id: self.id,
                message: "`id` must not be empty".to_owned(),
            });
        }
        if !id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_'))
        {
            return Err(ModuleImportError::InvalidSpec {
                id: id.clone(),
                message: "`id` must match ^[a-z0-9_-]+$".to_owned(),
            });
        }
        let path = resolve_import_path_for(base, self.path.trim(), &id)?;
        Ok(WebserverModuleImport {
            id,
            path,
            profile: self.profile,
            enabled: self.enabled,
            required: self.required,
            probe_upstreams: self.probe_upstreams,
        })
    }
}

/// Resolve a configured import path against runtime anchors.
pub fn resolve_import_path(base: &Path, configured: &str) -> Result<PathBuf, ModuleImportError> {
    resolve_import_path_for(base, configured, "")
}

fn layout_v2_webserver_dir(path: &Path) -> bool {
    path.join("server.common.toml").is_file()
}

pub(crate) fn is_nginx_conf_path(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("conf")
}

/// Module id for a `<module>/deployments/webserver/…` sidecar.
///
/// High-cohesion import (SDKWORK_WEBSERVER_SPEC.md §17.3): the import
/// aggregator includes the module's own checkout sidecar, so the module id is
/// the path segment before `deployments/webserver/`.
pub(crate) fn module_dir_id_from_nginx_sidecar(path: &Path) -> Option<String> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for index in 0..components.len().saturating_sub(1) {
        if components[index] == "deployments" && components[index + 1] == "webserver" && index > 0 {
            let module_id = components[index - 1].clone();
            if !module_id.is_empty() {
                return Some(module_id);
            }
            break;
        }
    }
    None
}

/// Split a `nginx.<profile>.<environment>.conf` sidecar name into its profile
/// and lifecycle environment. `None` for any other naming, so legacy
/// single-file sidecars keep the bare module id.
pub(crate) fn sidecar_profile_environment(path: &Path) -> Option<(&str, &str)> {
    let stem = path.file_stem().and_then(|value| value.to_str())?.trim();
    let mut parts = stem.strip_prefix("nginx.")?.split('.');
    let profile = parts.next()?;
    let environment = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if !matches!(profile, "standalone" | "cloud") {
        return None;
    }
    if !matches!(
        environment,
        "development" | "test" | "staging" | "demo" | "production"
    ) {
        return None;
    }
    Some((profile, environment))
}

/// Adaptive Web dist alias for a lifecycle environment. Mirrors the
/// entrypoint's `environment_dist_alias` (SDKWORK_WEBSERVER_SPEC.md §13.6).
fn lifecycle_environment_dist_alias(environment: &str) -> Option<&'static str> {
    Some(match environment {
        "development" => "dev",
        "test" => "test",
        "staging" => "staging",
        "demo" => "demo",
        "production" => "prod",
        _ => return None,
    })
}

/// Environment-scoped sibling of a declared Adaptive Web package root:
/// `<base>/<module>/web/<surface>` -> `<base>/<module>/web/<alias>/<surface>`.
///
/// The `<module>` segment is **never interpreted**: the plan is derived from
/// the declared root alone, so it stays correct for whatever path segment a
/// module uses under `<base>` — the Adaptive Web runtime code (`im`), not the
/// repository directory id (`sdkwork-im`). Kept in step with the entrypoint's
/// `module_env_web_static_root` (`bin/container/entrypoint-standalone.sh`),
/// which inserts `<alias>/` before the last segment in exactly the same way.
///
/// `None` when the declared root is not an Adaptive Web package root, or when
/// the environment-scoped root has not been materialized — a
/// single-environment deployment therefore keeps serving the declared root
/// exactly as before.
fn environment_scoped_adaptive_root(declared: &str, alias: &str) -> Option<String> {
    let trimmed = declared.trim().trim_end_matches('/');
    let (parent, surface) = trimmed.rsplit_once('/')?;
    // Adaptive Web package roots only (SDKWORK_DEPLOY_SPEC.md §8.1):
    // `<base>/<module>/web/{pc,h5}`. Everything else keeps its declared root.
    if !matches!(surface, "pc" | "h5") || !parent.ends_with("/web") {
        return None;
    }
    let candidate = format!("{parent}/{alias}/{surface}");
    Path::new(&candidate).exists().then_some(candidate)
}

/// Resolve a module sidecar's Adaptive Web static resources to the routed
/// environment's dist tree (SDKWORK_WEBSERVER_SPEC.md §17.3.2 /
/// ENVIRONMENT_SPEC.md §6.2.2 universal import plane).
///
/// A deployed instance is the shared edge for every commissioned lifecycle
/// environment, but §8.1 of `SDKWORK_DEPLOY_SPEC.md` fixes every sidecar's
/// `@pc`/`@h5` roots to one package path — so one process would serve one
/// environment's PC/H5 bundle under every environment's domain. The
/// entrypoint materializes an environment-scoped root next to each declared
/// root for every commissioned environment; here the data plane binds each
/// imported sidecar to its own environment's tree. Module configuration is
/// never rewritten: this is resolution, not editing (§17.3).
fn apply_environment_scoped_adaptive_roots(app: &mut WebServerAppConfig, sidecar: &Path) {
    let Some((_profile, environment)) = sidecar_profile_environment(sidecar) else {
        return;
    };
    let Some(alias) = lifecycle_environment_dist_alias(environment) else {
        return;
    };
    for resource in &mut app.resources {
        let ResourceConfig::Static { root, h5_root, .. } = resource else {
            continue;
        };
        if let Some(scoped) = environment_scoped_adaptive_root(root, alias) {
            *root = scoped;
        }
        let declared_h5 = h5_root.clone();
        if let Some(scoped) =
            declared_h5.and_then(|value| environment_scoped_adaptive_root(&value, alias))
        {
            *h5_root = Some(scoped);
        }
    }
}

fn resolve_conf_import_path(
    base: &Path,
    configured: &str,
    id: &str,
) -> Result<PathBuf, ModuleImportError> {
    let label = if id.is_empty() { "import" } else { id };
    let trimmed = configured.trim();
    let raw = Path::new(trimmed);
    let candidates = if raw.is_absolute() {
        vec![PathBuf::from(trimmed)]
    } else {
        import_path_anchors(base)
            .into_iter()
            .map(|anchor| anchor.join(trimmed))
            .collect()
    };
    for candidate in candidates {
        if candidate.is_file() && is_nginx_conf_path(&candidate) {
            return candidate
                .canonicalize()
                .map_err(|error| ModuleImportError::InvalidSpec {
                    id: id.to_owned(),
                    message: format!(
                        "cannot canonicalize nginx conf import `{}` for `{label}`: {error}",
                        candidate.display()
                    ),
                });
        }
    }
    Err(ModuleImportError::InvalidSpec {
        id: id.to_owned(),
        message: format!(
            "nginx conf import `{trimmed}` for `{label}` was not found under {}",
            base.display()
        ),
    })
}

fn webserver_dir_candidates(path: PathBuf) -> Vec<PathBuf> {
    let mut candidates = vec![path.clone()];
    let nested = path.join("deployments").join("webserver");
    if nested != path {
        candidates.push(nested);
    }
    candidates
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for path in paths {
        if seen.insert(path.clone()) {
            unique.push(path);
        }
    }
    unique
}

fn resolve_import_path_for(
    base: &Path,
    configured: &str,
    id: &str,
) -> Result<PathBuf, ModuleImportError> {
    let label = if id.is_empty() { "import" } else { id };
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return Err(ModuleImportError::InvalidSpec {
            id: id.to_owned(),
            message: "`path` must not be empty".to_owned(),
        });
    }

    if trimmed.ends_with(".conf") {
        return resolve_conf_import_path(base, trimmed, id);
    }

    let raw = Path::new(trimmed);
    let candidates = if raw.is_absolute() {
        webserver_dir_candidates(PathBuf::from(trimmed))
    } else {
        let mut expanded = Vec::new();
        for anchor in import_path_anchors(base) {
            for candidate in webserver_dir_candidates(anchor.join(trimmed)) {
                expanded.push(candidate);
            }
        }
        dedupe_paths(expanded)
    };

    if candidates.is_empty() {
        return Err(ModuleImportError::InvalidSpec {
            id: id.to_owned(),
            message: format!("no candidate paths for `{trimmed}`"),
        });
    }

    let mut last_error = String::new();
    for candidate in &candidates {
        if !layout_v2_webserver_dir(candidate) {
            continue;
        }
        match candidate.canonicalize() {
            Ok(canonical) if canonical.is_dir() => return Ok(canonical),
            Ok(_) => {
                last_error = format!(
                    "resolved import path `{}` for `{label}` is not a directory",
                    candidate.display()
                );
            }
            Err(error) => {
                last_error = format!(
                    "cannot canonicalize import path `{}` for `{label}`: {error}",
                    candidate.display()
                );
            }
        }
    }

    // Fail closed with the most specific layout hint when nothing matched.
    let primary = &candidates[0];
    if let Ok(canonical) = primary.canonicalize() {
        if canonical.is_dir() {
            return Err(ModuleImportError::InvalidSpec {
                id: id.to_owned(),
                message: format!(
                    "import path `{}` for `{label}` exists but is not a layout v3 webserver directory (missing server.common.toml); expected that directory or a module root containing `{WEBSERVER_DEPLOY_SUBDIR}/`",
                    canonical.display(),
                ),
            });
        }
    }

    Err(ModuleImportError::InvalidSpec {
        id: id.to_owned(),
        message: if last_error.is_empty() {
            format!(
                "cannot resolve import path `{trimmed}` for `{label}` from base {}; tried {} candidate(s) including absolute/relative module roots and `{WEBSERVER_DEPLOY_SUBDIR}/`",
                base.display(),
                candidates.len()
            )
        } else {
            last_error
        },
    })
}

fn import_path_anchors(base: &Path) -> Vec<PathBuf> {
    let mut anchors = Vec::new();
    anchors.push(base.to_path_buf());
    if let Some(parent) = base.parent() {
        anchors.push(parent.to_path_buf());
    }
    for key in ["SDKWORK_APP_ROOT", "SDKWORK_WEBSERVER_APP_ROOT"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                anchors.push(PathBuf::from(trimmed));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        anchors.push(cwd);
    }
    anchors
}

/// Parse `SDKWORK_WEBSERVER_MODULE_IMPORTS` as comma-separated `id=path` pairs.
pub fn parse_env_imports(base: &Path) -> Result<Vec<WebserverModuleImport>, ModuleImportError> {
    let Some(raw) = std::env::var(MODULE_IMPORTS_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };
    if raw.starts_with('[') {
        let entries: Vec<WebserverImportEntry> =
            serde_json::from_str(&raw).map_err(|error| ModuleImportError::InvalidSpec {
                id: MODULE_IMPORTS_ENV.to_owned(),
                message: format!("invalid JSON array: {error}"),
            })?;
        return entries
            .into_iter()
            .map(|entry| entry.into_import(base))
            .collect();
    }
    let mut imports = Vec::new();
    for segment in raw
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
    {
        let Some((id, path)) = segment.split_once('=') else {
            return Err(ModuleImportError::InvalidSpec {
                id: MODULE_IMPORTS_ENV.to_owned(),
                message: format!(
                    "expected `id=path` pairs in {MODULE_IMPORTS_ENV}, got `{segment}`"
                ),
            });
        };
        imports.push(
            WebserverImportEntry {
                id: id.trim().to_owned(),
                path: path.trim().to_owned(),
                profile: None,
                enabled: true,
                required: true,
                probe_upstreams: true,
            }
            .into_import(base)?,
        );
    }
    Ok(imports)
}

/// Merge runtime-file imports with env imports (env entries override same `id`).
pub fn merge_import_specs(
    base: &Path,
    from_runtime: &[WebserverImportEntry],
) -> Result<Vec<WebserverModuleImport>, ModuleImportError> {
    let mut merged: Vec<WebserverModuleImport> = from_runtime
        .iter()
        .cloned()
        .map(|entry| entry.into_import(base))
        .collect::<Result<_, _>>()?;
    for env_import in parse_env_imports(base)? {
        if let Some(existing) = merged.iter_mut().find(|item| item.id == env_import.id) {
            *existing = env_import;
        } else {
            merged.push(env_import);
        }
    }
    Ok(merged)
}

/// Effective deployment profile for an import (`standalone` or `cloud`).
pub fn resolve_import_profile(import: &WebserverModuleImport) -> Result<String, ModuleImportError> {
    if let Some(profile) = import.profile.as_deref() {
        if profile == "standalone" || profile == "cloud" {
            return Ok(profile.to_owned());
        }
        return Err(ModuleImportError::InvalidSpec {
            id: import.id.clone(),
            message: format!("profile must be `standalone` or `cloud`, got `{profile}`"),
        });
    }
    let deployment_profile = std::env::var("SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE")
        .or_else(|_| std::env::var("SDKWORK_DEPLOYMENT_PROFILE"))
        .unwrap_or_else(|_| "standalone".to_owned());
    if deployment_profile == "cloud" {
        Ok("cloud".to_owned())
    } else {
        Ok("standalone".to_owned())
    }
}

/// Effective lifecycle environment for an import.
pub fn resolve_import_environment(
    import: &WebserverModuleImport,
) -> Result<String, ModuleImportError> {
    let environment = std::env::var("SDKWORK_WEBSERVER_ENVIRONMENT")
        .or_else(|_| std::env::var("SDKWORK_ENVIRONMENT"))
        .unwrap_or_else(|_| "production".to_owned());
    if matches!(
        environment.as_str(),
        "development" | "test" | "staging" | "production" | "demo"
    ) {
        Ok(environment)
    } else {
        Err(ModuleImportError::InvalidSpec {
            id: import.id.clone(),
            message: format!(
                "environment must be `development`, `test`, `staging`, `production`, or `demo`, got `{environment}`"
            ),
        })
    }
}

/// Load one module import as a materialized app model. Layout v3 TOML
/// directories and stock nginx `.conf` files are supported.
pub fn load_module_import_app_config(
    import: &WebserverModuleImport,
) -> Result<WebServerAppConfig, ModuleImportError> {
    let app_key = format!("imported-{}", import.id);
    if is_nginx_conf_path(&import.path) {
        let loader = WebServerConfigLoader::new();
        let options = ConfigLoadOptions {
            format: Some(ConfigFormat::NginxConf),
            app_key: Some(app_key),
            ..ConfigLoadOptions::default()
        };
        let loaded = loader
            .load(&import.path, &options)
            .map_err(|source| ModuleImportError::Validation {
                id: import.id.clone(),
                path: import.path.clone(),
                source,
            })?;
        let mut app = loaded.app;
        apply_environment_scoped_adaptive_roots(&mut app, &import.path);
        return Ok(app);
    }
    let profile = resolve_import_profile(import)?;
    let environment = resolve_import_environment(import)?;
    load_server_toml_app(import.webserver_dir(), &profile, &environment, &app_key).map_err(
        |source| ModuleImportError::Validation {
            id: import.id.clone(),
            path: import.path.clone(),
            source,
        },
    )
}

/// Validate one import: layout v3 TOML or nginx `.conf`, materialization,
/// optional upstream probe.
pub fn validate_module_import(
    import: &WebserverModuleImport,
) -> Result<ModuleImportValidation, ModuleImportError> {
    if !import.enabled {
        return Ok(ModuleImportValidation {
            import: import.clone(),
            profile: String::new(),
            app_key: String::new(),
            virtual_host_count: 0,
            upstream_count: 0,
            probed_upstreams: Vec::new(),
            unreachable_upstreams: Vec::new(),
        });
    }
    let profile = resolve_import_profile(import)?;
    let app_key = format!("imported-{}", import.id);
    let config = load_module_import_app_config(import)?;
    let mut probed_upstreams = Vec::new();
    let mut unreachable_upstreams = Vec::new();
    if import.probe_upstreams {
        for target in collect_upstream_targets(&config) {
            probed_upstreams.push(target.clone());
            if !probe_upstream_target(&target) {
                unreachable_upstreams.push(target);
            }
        }
    }
    if import.required && !unreachable_upstreams.is_empty() {
        let upstream = unreachable_upstreams[0].clone();
        return Err(ModuleImportError::UnreachableUpstream {
            id: import.id.clone(),
            path: import.path.clone(),
            upstream,
            detail: "TCP connect failed within startup probe budget".to_owned(),
        });
    }
    Ok(ModuleImportValidation {
        virtual_host_count: config.virtual_hosts.len(),
        upstream_count: config.upstreams.len(),
        import: import.clone(),
        profile,
        app_key,
        probed_upstreams,
        unreachable_upstreams,
    })
}

/// Validate every configured import. Disabled imports are skipped.
pub fn validate_imports(
    imports: &[WebserverModuleImport],
) -> Result<Vec<ModuleImportValidation>, ModuleImportError> {
    let mut results = Vec::new();
    for import in imports {
        if !import.enabled {
            continue;
        }
        results.push(validate_module_import(import)?);
    }
    Ok(results)
}

fn collect_upstream_targets(config: &WebServerAppConfig) -> Vec<String> {
    config
        .upstreams
        .iter()
        .flat_map(|upstream| upstream.targets.iter().map(|target| target.url.clone()))
        .collect()
}

fn probe_upstream_target(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let port = parsed.port_or_known_default().unwrap_or(80);
    let authority = format!("{host}:{port}");
    let Ok(mut addrs) = authority.to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| tcp_reachable(addr))
}

fn tcp_reachable(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, DEFAULT_PROBE_TIMEOUT).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn examples_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .expect("crates")
            .parent()
            .expect("sdkwork-webserver")
            .join("..")
            .join("sdkwork-specs")
            .join("examples")
            .join("webserver")
    }

    fn webserver_repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates")
            .parent()
            .expect("sdkwork-webserver")
            .to_path_buf()
    }

    #[test]
    fn resolves_relative_import_against_base() {
        let examples = examples_dir();
        let base = examples
            .parent()
            .expect("examples")
            .parent()
            .expect("specs");
        let resolved = resolve_import_path(base, "examples/webserver").expect("resolve");
        assert!(resolved.join("server.common.toml").is_file());
    }

    #[test]
    fn resolves_relative_module_root() {
        let repo = webserver_repo_root();
        let base = repo.join("etc");
        let resolved = resolve_import_path(&base, "..").expect("resolve module root");
        assert!(resolved.join("server.common.toml").is_file());
    }

    #[test]
    fn resolves_absolute_webserver_dir() {
        let webserver_dir = webserver_repo_root().join("deployments/webserver");
        let configured = webserver_dir.to_string_lossy().into_owned();
        let resolved = resolve_import_path(Path::new("."), &configured).expect("resolve absolute");
        assert!(resolved.join("server.common.toml").is_file());
    }

    #[test]
    fn resolves_absolute_module_root() {
        let repo = webserver_repo_root();
        let configured = repo.to_string_lossy().into_owned();
        let resolved =
            resolve_import_path(Path::new("."), &configured).expect("resolve absolute root");
        assert!(resolved.join("server.common.toml").is_file());
    }

    #[test]
    fn validates_example_layout_v2_import() {
        let import = WebserverModuleImport {
            id: "im-example".to_owned(),
            path: examples_dir(),
            profile: Some("cloud".to_owned()),
            enabled: true,
            required: true,
            probe_upstreams: false,
        };
        let report = validate_module_import(&import).expect("example import must validate");
        assert_eq!(report.profile, "cloud");
        assert!(report.virtual_host_count >= 1);
        assert!(report.upstream_count >= 1);
    }

    #[test]
    fn parses_env_import_pairs() {
        std::env::set_var(MODULE_IMPORTS_ENV, "im-example=examples/webserver");
        let examples = examples_dir();
        let base = examples
            .parent()
            .expect("examples")
            .parent()
            .expect("specs");
        let imports = parse_env_imports(base).expect("parse env");
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].id, "im-example");
        std::env::remove_var(MODULE_IMPORTS_ENV);
    }

    /// The environment-scoped root plan must be derived from the declared root
    /// alone: the path segment under the web base is the module's Adaptive Web
    /// runtime code (`im`), never the repository directory id (`sdkwork-im`).
    /// Re-introducing a module-id prefix silently disables per-environment
    /// static resolution, so pin the plan and the refusals here.
    #[test]
    fn environment_scoped_adaptive_root_follows_the_declared_root_segment() {
        let base = std::env::temp_dir().join(format!(
            "sdkwork-adaptive-root-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let staging_pc = base.join("im/web/staging/pc");
        std::fs::create_dir_all(&staging_pc).expect("materialize staging pc root");

        let declared = format!("{}/im/web/pc", base.display());
        let scoped = environment_scoped_adaptive_root(&declared, "staging");
        assert_eq!(scoped.as_deref(), Some(staging_pc.to_string_lossy().as_ref()));

        // Not materialized for this environment -> keep the declared root.
        assert!(environment_scoped_adaptive_root(&declared, "production").is_none());
        // Trailing slashes are tolerated.
        assert_eq!(
            environment_scoped_adaptive_root(&format!("{declared}/"), "staging").as_deref(),
            Some(staging_pc.to_string_lossy().as_ref())
        );
        // Non-Adaptive-Web roots are never rewritten.
        assert!(
            environment_scoped_adaptive_root(&format!("{}/im/static", base.display()), "staging")
                .is_none()
        );
        assert!(
            environment_scoped_adaptive_root(&format!("{}/im/web/pc/app", base.display()), "staging")
                .is_none()
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
