//! Installed runtime configuration loaded from a typed TOML file.
//!
//! The native installers write the authoritative runtime configuration to
//! `/etc/sdkwork/webserver/config.toml` (`RUNTIME_DIRECTORY_SPEC.md`
//! section 4.1). Every process binary (gateway, db-migrate, certificate worker)
//! loads this file at startup and materializes it into the process environment
//! variables the runtime components read (`SDKWORK_*`), so downstream crates
//! keep their env-based contract. Secret material is referenced by file path
//! only and is injected in-process, never written to env files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::{WebServerAppConfig, WebServerConfigError};
use crate::config_paths::{
    canonical_runtime_config_path, runtime_config_override_from_env, RUNTIME_CONFIG_FILE_ENV,
};
use crate::module_imports::{
    is_nginx_conf_path, load_module_import_app_config, merge_import_specs, validate_imports,
    WebserverImportEntry, WebserverModuleImport,
};
use crate::nginx::merge_nginx_apps;
use crate::nginx::parse_nginx_config;

/// Resolves the runtime TOML path: explicit override, then the canonical
/// system config directory. `None` when the canonical file does not exist
/// (development/dev-runner environments keep env-based configuration).
pub fn resolve_runtime_config_path() -> Result<Option<PathBuf>, String> {
    if let Some(path) = runtime_config_override_from_env()? {
        if !path.is_file() {
            return Err(format!(
                "{RUNTIME_CONFIG_FILE_ENV} points to a missing file: {}",
                path.display()
            ));
        }
        return Ok(Some(path));
    }
    let canonical = canonical_runtime_config_path()?;
    if canonical.is_file() {
        Ok(Some(canonical))
    } else {
        Ok(None)
    }
}

/// Loads and applies the runtime TOML configuration to the process
/// environment. Missing canonical file is a no-op; a present file must parse
/// and every secret file it references must be readable.
pub fn load_runtime_toml_config() -> Result<(), String> {
    let Some(path) = resolve_runtime_config_path()? else {
        return Ok(());
    };
    let config = parse_runtime_toml_config(&path)?;
    config.apply_to_env()?;
    Ok(())
}

pub fn parse_runtime_toml_config(path: &Path) -> Result<RuntimeTomlConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read runtime config {}: {error}", path.display()))?;
    if text.len() > MAX_RUNTIME_CONFIG_BYTES {
        return Err(format!(
            "runtime config {} exceeds {} bytes",
            path.display(),
            MAX_RUNTIME_CONFIG_BYTES
        ));
    }
    toml::from_str(&text)
        .map_err(|error| format!("parse runtime config {}: {error}", path.display()))
}

const MAX_RUNTIME_CONFIG_BYTES: usize = 256 * 1024;

fn set_env(key: &str, value: &str) {
    // SAFETY-free in edition 2021; only called once at process startup before
    // any runtime component reads the environment.
    std::env::set_var(key, value);
}

fn read_secret_file(path: &str) -> Result<String, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("read secret file {path}: {error}"))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(format!("secret file {path} is empty"));
    }
    Ok(trimmed.to_owned())
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTomlConfig {
    #[serde(default)]
    pub profile: ProfileSection,
    #[serde(default)]
    pub ingress: IngressSection,
    #[serde(default)]
    pub app_roots: AppRootsSection,
    #[serde(default)]
    pub deploy: DeploySection,
    #[serde(default)]
    pub database: DatabaseSection,
    #[serde(default)]
    pub secrets: SecretsSection,
    #[serde(default)]
    pub acme: AcmeSection,
    #[serde(default)]
    pub tls: TlsSection,
    #[serde(default)]
    pub node: NodeSection,
    #[serde(default)]
    pub region: RegionSection,
    #[serde(default)]
    pub webserver: WebserverSection,
}

impl RuntimeTomlConfig {
    pub fn apply_to_env(&self) -> Result<(), String> {
        self.profile.apply_to_env();
        self.ingress.apply_to_env();
        self.app_roots
            .apply_to_env(self.profile.environment.as_deref());
        self.deploy.apply_to_env();
        self.database.apply_to_env()?;
        self.secrets.apply_to_env()?;
        self.acme.apply_to_env();
        self.tls.apply_to_env();
        self.node.apply_to_env();
        self.region.apply_to_env();
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebserverSection {
    /// Inline module imports declared directly in the runtime TOML.
    #[serde(default)]
    pub imports: Vec<WebserverImportEntry>,
    /// nginx-style include patterns (for example
    /// `imports.d/import.conf` or `imports.d/layout-imports.toml`).
    /// `import.conf` is an aggregator of top-level `include` directives
    /// pointing at sibling module nginx sidecars under the space checkout;
    /// each included sidecar becomes one module import entry. Matched
    /// `.toml` files are parsed with the runtime TOML schema and their
    /// `[[webserver.imports]]` entries are appended. Inline entries come
    /// first; included files follow in sorted order. A later entry with the
    /// same `id` replaces the earlier one.
    #[serde(default)]
    pub include: Vec<String>,
}

/// Expand `[webserver] include` patterns into the effective import list.
/// Relative patterns resolve against the runtime config directory.
pub fn expand_webserver_import_includes(
    config_path: &Path,
    section: &WebserverSection,
) -> Result<Vec<WebserverImportEntry>, String> {
    let mut merged = section.imports.clone();
    let base = config_path
        .parent()
        .unwrap_or_else(|| Path::new("/"))
        .to_path_buf();
    for pattern in section
        .include
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
    {
        for path in resolve_include_paths(&base, pattern)? {
            if path.extension().and_then(|value| value.to_str()) == Some("conf") {
                if is_import_aggregator_conf(&path) {
                    for entry in expand_import_aggregator_conf(&path)? {
                        upsert_import_entry(&mut merged, entry)?;
                    }
                    continue;
                }
                let entry = WebserverImportEntry {
                    id: import_id_from_include_path(&path)?,
                    path: path.to_string_lossy().into_owned(),
                    profile: None,
                    enabled: true,
                    required: false,
                    probe_upstreams: false,
                };
                upsert_import_entry(&mut merged, entry)?;
                continue;
            }
            let included = parse_runtime_toml_config(&path)?;
            for entry in included.webserver.imports {
                upsert_import_entry(&mut merged, entry)?;
            }
        }
    }
    Ok(merged)
}

fn upsert_import_entry(
    merged: &mut Vec<WebserverImportEntry>,
    entry: WebserverImportEntry,
) -> Result<(), String> {
    if let Some(existing) = merged.iter().find(|item| item.id == entry.id) {
        let existing_conf = is_nginx_conf_path(Path::new(&existing.path));
        let incoming_conf = is_nginx_conf_path(Path::new(&entry.path));
        if existing_conf != incoming_conf {
            return Err(format!(
                "module import `{}`: nginx `.conf` and layout v3 TOML directory imports are mutually exclusive (existing `{}`, incoming `{}`)",
                entry.id,
                existing.path,
                entry.path
            ));
        }
    }
    if let Some(existing) = merged.iter_mut().find(|item| item.id == entry.id) {
        *existing = entry;
    } else {
        merged.push(entry);
    }
    Ok(())
}

fn import_id_from_include_path(path: &Path) -> Result<String, String> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "webserver include conf `{}` has no import id stem",
                path.display()
            )
        })?;
    Ok(stem.to_owned())
}

fn is_import_aggregator_conf(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("import.conf")
}

fn module_import_id_from_nginx_sidecar(path: &Path) -> Result<String, String> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    // High-cohesion import (SDKWORK_WEBSERVER_SPEC.md §17.3): the aggregator
    // includes the module's own checkout sidecar, so the module id is the
    // path segment before deployments/webserver/.
    for index in 0..components.len().saturating_sub(1) {
        if components[index] == "deployments" && components[index + 1] == "webserver" {
            if index > 0 {
                let module_id = components[index - 1].clone();
                if !module_id.is_empty() {
                    return Ok(module_id);
                }
            }
            break;
        }
    }
    import_id_from_include_path(path)
}

fn expand_import_aggregator_conf(aggregator: &Path) -> Result<Vec<WebserverImportEntry>, String> {
    let text = std::fs::read_to_string(aggregator)
        .map_err(|error| format!("read import aggregator {}: {error}", aggregator.display()))?;
    let parsed = parse_nginx_config(&text, aggregator)
        .map_err(|error| format!("parse import aggregator {}: {error}", aggregator.display()))?;
    let base = aggregator.parent().unwrap_or_else(|| Path::new("/"));
    let mut entries = Vec::new();
    for directive in parsed {
        if directive.name != "include" {
            return Err(format!(
                "import aggregator `{}` accepts only top-level include directives, found `{}`",
                aggregator.display(),
                directive.name
            ));
        }
        let pattern = directive
            .args
            .first()
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "import aggregator `{}:{}`: include requires a path pattern",
                    aggregator.display(),
                    directive.line
                )
            })?;
        for target in resolve_include_paths(base, pattern)? {
            if !is_nginx_conf_path(&target) {
                return Err(format!(
                    "import aggregator `{}` referenced non-nginx path `{}`",
                    aggregator.display(),
                    target.display()
                ));
            }
            let entry = WebserverImportEntry {
                id: module_import_id_from_nginx_sidecar(&target)?,
                path: target.to_string_lossy().into_owned(),
                profile: None,
                enabled: true,
                required: false,
                probe_upstreams: false,
            };
            upsert_import_entry(&mut entries, entry)?;
        }
    }
    Ok(entries)
}

fn resolve_include_paths(base: &Path, pattern: &str) -> Result<Vec<PathBuf>, String> {
    let raw = Path::new(pattern);
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base.join(raw)
    };
    let has_glob = pattern.contains('*') || pattern.contains('?');
    if !has_glob {
        if !absolute.is_file() {
            return Err(format!(
                "webserver include pattern `{pattern}` points to a missing file: {}",
                absolute.display()
            ));
        }
        return Ok(vec![absolute]);
    }
    let file_pattern = absolute
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("webserver include pattern `{pattern}` has no file component"))?;
    let dir = absolute.parent().unwrap_or_else(|| Path::new("/"));
    // nginx semantics: a glob that matches nothing (including a missing
    // include directory) is skipped; only explicit single-file patterns
    // fail closed above.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut matched = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if path.is_file() && glob_match(file_pattern, &name) {
            matched.push(path);
        }
    }
    matched.sort();
    Ok(matched)
}

/// Match a single path component: `*` matches any sequence, `?` matches one
/// character. No character classes or separators are supported.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let mut reachable = vec![false; text.len() + 1];
    reachable[0] = true;
    for &token in &pattern {
        let mut next = vec![false; text.len() + 1];
        match token {
            '*' => {
                let mut seen = false;
                for (index, state) in reachable.iter().enumerate() {
                    if *state {
                        seen = true;
                    }
                    next[index] = seen;
                }
            }
            '?' => {
                for index in 0..text.len() {
                    next[index + 1] = reachable[index];
                }
            }
            literal => {
                for (index, character) in text.iter().enumerate() {
                    next[index + 1] = reachable[index] && *character == literal;
                }
            }
        }
        reachable = next;
    }
    reachable[text.len()]
}

/// Validate imports from runtime TOML `[[webserver.imports]]` and/or
/// `SDKWORK_WEBSERVER_MODULE_IMPORTS`. When no runtime file exists, env-only
/// imports are still validated.
pub fn module_import_resolution_base() -> PathBuf {
    if let Ok(Some(path)) = resolve_runtime_config_path() {
        if let Some(parent) = path.parent() {
            return parent.to_path_buf();
        }
    }
    for key in ["SDKWORK_APP_ROOT", "SDKWORK_WEBSERVER_APP_ROOT"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Validate imports from runtime TOML `[[webserver.imports]]` and/or
/// `SDKWORK_WEBSERVER_MODULE_IMPORTS`. When no runtime file exists, env-only
/// imports are still validated.
pub fn validate_configured_module_imports(
) -> Result<Vec<crate::module_imports::ModuleImportValidation>, String> {
    let imports = configured_module_imports()?;
    if imports.is_empty() {
        return Ok(Vec::new());
    }
    validate_imports(&imports).map_err(|error| error.to_string())
}

/// The effective module import list: runtime TOML inline entries plus
/// `[webserver] include` expansions, overridden by
/// `SDKWORK_WEBSERVER_MODULE_IMPORTS`.
pub fn configured_module_imports() -> Result<Vec<WebserverModuleImport>, String> {
    let base = module_import_resolution_base();
    let from_runtime = if let Ok(Some(path)) = resolve_runtime_config_path() {
        let config = parse_runtime_toml_config(&path)?;
        expand_webserver_import_includes(&path, &config.webserver)?
    } else {
        Vec::new()
    };
    merge_import_specs(&base, &from_runtime).map_err(|error| error.to_string())
}

/// Env var for remapping declared module listener ports to the actual
/// container binds (`"80=8080,443=8430"`). Module `deployments/webserver/`
/// files stay authoritative; the remap only affects the merged data plane.
pub const IMPORT_LISTENER_PORTS_ENV: &str = "SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS";

/// Assemble one data-plane configuration from every configured module
/// import: each module's `effective(<profile>.<environment>)` TOML document
/// is merged (standard layout v3 merge semantics) into a single effective
/// document, `IMPORT_LISTENER_PORTS_ENV` remaps declared listener ports, and
/// the result is materialized. The gateway serves module domains, servers,
/// and resources from this merged configuration. `None` when no enabled
/// module imports are configured.
pub fn merged_imports_app_config() -> Result<Option<WebServerAppConfig>, String> {
    let imports = configured_module_imports()?;
    let enabled: Vec<WebserverModuleImport> = imports
        .into_iter()
        .filter(|import| import.enabled)
        .collect();
    if enabled.is_empty() {
        return Ok(None);
    }
    let mut merged: Option<WebServerAppConfig> = None;
    for import in &enabled {
        let app = load_module_import_app_config(import).map_err(|error| error.to_string())?;
        merged = Some(match merged {
            Some(existing) => merge_nginx_apps(existing, app)
                .map_err(|source| format_webserver_config_error(&source))?,
            None => app,
        });
    }
    let mut merged = merged.expect("enabled imports must produce a merged app");
    apply_import_listener_port_map_to_app(&mut merged)?;
    Ok(Some(merged))
}

/// Format a `WebServerConfigError` together with its diagnostics so merged
/// module-import failures surface the offending paths and rules.
fn format_webserver_config_error(source: &WebServerConfigError) -> String {
    let diagnostics = source.diagnostics();
    let mut detail = format!("{} ({} diagnostic(s))", source, diagnostics.len());
    for diagnostic in diagnostics {
        detail.push_str(&format!("\n  {}: {}", diagnostic.path, diagnostic.message));
    }
    detail
}

/// List the certificate names the merged module configuration references,
/// for bootstrap certificate provisioning (`/etc/sdkwork/certs/...`).
pub fn imported_certificate_names() -> Result<Vec<String>, String> {
    let Some(app) = merged_imports_app_config()? else {
        return Ok(Vec::new());
    };
    Ok(app
        .certificates
        .iter()
        .map(|certificate| certificate.id.clone())
        .collect())
}

/// Semantic validation + compile of the merged module-imports data-plane
/// configuration. `None` when no enabled module imports are configured.
pub fn compile_merged_imports_app() -> Result<Option<crate::CompiledWebServerApp>, String> {
    let Some(app) = merged_imports_app_config()? else {
        return Ok(None);
    };
    let base = module_import_resolution_base();
    ensure_import_bootstrap_certificates(&app, &base)?;
    if let Err(source) = crate::config::validate_webserver_config(&app) {
        return Err(format_webserver_config_error(&source));
    }
    let compiled = crate::CompiledWebServerApp::compile(app, &base)
        .map_err(|source| format_webserver_config_error(&source))?;
    Ok(Some(compiled))
}

/// Bootstrap placeholder certificates: when a certificate referenced by the
/// merged module configuration has no material on disk yet, generate a
/// self-signed placeholder (SAN covers the certificate's server names) so
/// the data plane starts. Operators replace the placeholder with real
/// material (ACME issuance or uploaded files) without a restart. A
/// half-present pair (certificate without key or vice versa) fails closed
/// with a precise diagnostic instead of being overwritten.
pub fn ensure_import_bootstrap_certificates(
    app: &WebServerAppConfig,
    base: &Path,
) -> Result<(), String> {
    use crate::config::CertificateSource;
    for certificate in &app.certificates {
        let CertificateSource::ProtectedFile {
            certificate_file,
            private_key_file,
        } = &certificate.source;
        let certificate_path = resolve_certificate_file_path(base, certificate_file)?;
        let private_key_path = resolve_certificate_file_path(base, private_key_file)?;
        if certificate_path.is_file() && private_key_path.is_file() {
            continue;
        }
        if certificate_path.is_file() != private_key_path.is_file() {
            return Err(format!(
                "certificate `{}`: only one of {} / {} exists; supply both or remove both so a bootstrap placeholder can be generated",
                certificate.id,
                certificate_path.display(),
                private_key_path.display()
            ));
        }
        let mut server_names = if certificate.server_names.is_empty() {
            vec![certificate.id.clone()]
        } else {
            certificate.server_names.clone()
        };
        // Wildcard SAN for the certificate's base name so one bootstrap
        // placeholder stays valid across lifecycle environments
        // (im.sdkwork.com and im-dev.sdkwork.com are both covered by
        // *.sdkwork.com) and survives until the operator replaces it with
        // real material.
        let wildcard = format!("*.{}", certificate.id);
        if !server_names.contains(&wildcard) {
            server_names.push(certificate.id.clone());
            server_names.push(wildcard);
        }
        let (certificate_pem, private_key_pem) = generate_self_signed_placeholder(&server_names)?;
        if let Some(parent) = certificate_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "cannot create certificate directory {} for `{}`: {error}",
                    parent.display(),
                    certificate.id
                )
            })?;
        }
        write_private_certificate_file(&certificate_path, certificate_pem.as_bytes())?;
        write_private_certificate_file(&private_key_path, private_key_pem.as_bytes())?;
        tracing::warn!(
            certificate = %certificate.id,
            certificate_file = %certificate_path.display(),
            "generated self-signed bootstrap placeholder certificate; replace with real material (ACME or operator-uploaded)"
        );
    }
    Ok(())
}

fn resolve_certificate_file_path(base: &Path, configured: &str) -> Result<PathBuf, String> {
    if let Some(rest) = configured.strip_prefix(crate::config_paths::CERTS_URI_SCHEME) {
        let (domain, file) = rest
            .split_once('/')
            .ok_or_else(|| format!("`{configured}` must be certs://<domain>/<file>"))?;
        let directory = crate::config_paths::canonical_certificate_domain_directory(domain)?;
        return Ok(directory.join(file));
    }
    let raw = Path::new(configured);
    if raw.is_absolute() {
        return Ok(raw.to_path_buf());
    }
    Ok(base.join(raw))
}

fn write_private_certificate_file(path: &Path, content: &[u8]) -> Result<(), String> {
    std::fs::write(path, content)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn generate_self_signed_placeholder(hostnames: &[String]) -> Result<(String, String), String> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P256_SHA256};
    let primary = hostnames
        .first()
        .cloned()
        .unwrap_or_else(|| "localhost".to_owned());
    let mut params = CertificateParams::new(hostnames.to_vec())
        .map_err(|error| format!("certificate parameters: {error}"))?;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, &primary);
    let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
        .map_err(|error| format!("generate placeholder key: {error}"))?;
    let certified = params
        .self_signed(&key_pair)
        .map_err(|error| format!("self-sign placeholder certificate: {error}"))?;
    Ok((certified.pem(), key_pair.serialize_pem()))
}

fn apply_import_listener_port_map_to_app(app: &mut WebServerAppConfig) -> Result<(), String> {
    let map = parse_import_listener_port_map()?;
    if map.is_empty() {
        return Ok(());
    }
    for listener in &mut app.listeners {
        let Some(&actual) = map.get(&listener.port) else {
            continue;
        };
        listener.port = actual;
        if listener.bind.contains(':') {
            if let Some((host, _)) = listener.bind.rsplit_once(':') {
                if host.starts_with('[') {
                    listener.bind = format!("[{host}]:{actual}");
                } else {
                    listener.bind = format!("{host}:{actual}");
                }
            }
        } else if listener
            .bind
            .chars()
            .all(|character| character.is_ascii_digit())
        {
            listener.bind = actual.to_string();
        }
    }
    Ok(())
}

fn parse_import_listener_port_map() -> Result<HashMap<u16, u16>, String> {
    let Ok(raw) = std::env::var(IMPORT_LISTENER_PORTS_ENV) else {
        return Ok(HashMap::new());
    };
    let mut map = HashMap::new();
    for pair in raw
        .split(',')
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
    {
        let Some((declared, actual)) = pair.split_once('=') else {
            return Err(format!(
                "{IMPORT_LISTENER_PORTS_ENV} expects `declared=actual` port pairs, got `{pair}`"
            ));
        };
        let declared: u16 = declared.trim().parse().map_err(|_| {
            format!("{IMPORT_LISTENER_PORTS_ENV} has an invalid declared port `{declared}`")
        })?;
        let actual: u16 = actual.trim().parse().map_err(|_| {
            format!("{IMPORT_LISTENER_PORTS_ENV} has an invalid actual port `{actual}`")
        })?;
        map.insert(declared, actual);
    }
    Ok(map)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSection {
    pub deployment_profile: Option<String>,
    pub environment: Option<String>,
    pub profile_id: Option<String>,
    pub node_id: Option<u64>,
}

impl ProfileSection {
    fn apply_to_env(&self) {
        if let Some(value) = &self.deployment_profile {
            set_env("SDKWORK_DEPLOYMENT_PROFILE", value);
            set_env("SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE", value);
        }
        if let Some(value) = &self.environment {
            set_env("SDKWORK_ENVIRONMENT", value);
            set_env("SDKWORK_WEBSERVER_ENVIRONMENT", value);
        }
        if let Some(value) = &self.profile_id {
            set_env("SDKWORK_PROFILE_ID", value);
            set_env("SDKWORK_WEBSERVER_PROFILE_ID", value);
        }
        if let Some(value) = self.node_id {
            set_env("SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID", &value.to_string());
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngressSection {
    pub bind: Option<String>,
    pub management_expose_allowed: Option<bool>,
    pub data_plane_operations_bind: Option<String>,
    pub public_http_url: Option<String>,
    pub app_http_url: Option<String>,
    pub backend_http_url: Option<String>,
    pub cors_allowed_origins: Option<Vec<String>>,
}

impl IngressSection {
    fn apply_to_env(&self) {
        if let Some(value) = &self.bind {
            set_env("SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND", value);
        }
        if let Some(value) = self.management_expose_allowed {
            set_env(
                "SDKWORK_WEBSERVER_MANAGEMENT_EXPOSE_ALLOWED",
                &value.to_string(),
            );
        }
        if let Some(value) = &self.data_plane_operations_bind {
            set_env("SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND", value);
        }
        if let Some(value) = &self.public_http_url {
            set_env("SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL", value);
        }
        if let Some(value) = &self.app_http_url {
            set_env("SDKWORK_WEBSERVER_APPLICATION_APP_HTTP_URL", value);
        }
        if let Some(value) = &self.backend_http_url {
            set_env("SDKWORK_WEBSERVER_APPLICATION_BACKEND_HTTP_URL", value);
        }
        if let Some(origins) = &self.cors_allowed_origins {
            set_env("SDKWORK_CORS_ALLOWED_ORIGINS", &origins.join(", "));
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppRootsSection {
    pub app_root: Option<String>,
    pub iam_app_root: Option<String>,
    pub drive_app_root: Option<String>,
    pub deploy_app_root: Option<String>,
    pub web_store_app_root: Option<String>,
    /// Skills module app root (`SDKWORK_SKILLS_APP_ROOT`).
    pub skills_app_root: Option<String>,
    /// MCP module app root (`SDKWORK_MCP_APP_ROOT`).
    pub mcp_app_root: Option<String>,
    /// Explicit PC SPA root for the active process (wins over by-environment).
    pub pc_static_root: Option<String>,
    /// Explicit H5 SPA root for the active process (wins over by-environment).
    pub h5_static_root: Option<String>,
    /// Ordinary static when neither SPA is available (wins over by-environment).
    pub static_fallback_root: Option<String>,
    /// Optional Adaptive Web tablet preference: `pc` (default) or `h5`.
    pub tablet_surface: Option<String>,
    /// Lifecycle-environment catalog for PC SPA roots (`development`…`production`).
    #[serde(default)]
    pub pc_static_by_environment: HashMap<String, String>,
    /// Lifecycle-environment catalog for H5 SPA roots.
    #[serde(default)]
    pub h5_static_by_environment: HashMap<String, String>,
    /// Lifecycle-environment catalog for ordinary static fallback.
    #[serde(default)]
    pub static_fallback_by_environment: HashMap<String, String>,
}

impl AppRootsSection {
    fn apply_to_env(&self, environment: Option<&str>) {
        if let Some(value) = &self.app_root {
            set_env("SDKWORK_APP_ROOT", value);
            set_env("SDKWORK_WEBSERVER_APP_ROOT", value);
            set_env("SDKWORK_WEBSERVER_SERVER_APP_ROOT", value);
        }
        if let Some(value) = &self.iam_app_root {
            set_env("SDKWORK_IAM_APP_ROOT", value);
        }
        if let Some(value) = &self.drive_app_root {
            set_env("SDKWORK_DRIVE_APP_ROOT", value);
        }
        if let Some(value) = &self.deploy_app_root {
            set_env("SDKWORK_DEPLOY_APP_ROOT", value);
        }
        if let Some(value) = &self.web_store_app_root {
            set_env("SDKWORK_WEB_STORE_APP_ROOT", value);
        }
        if let Some(value) = &self.skills_app_root {
            set_env("SDKWORK_SKILLS_APP_ROOT", value);
        }
        if let Some(value) = &self.mcp_app_root {
            set_env("SDKWORK_MCP_APP_ROOT", value);
        }
        if let Some(value) = resolve_static_root_value(
            &self.pc_static_root,
            &self.pc_static_by_environment,
            environment,
        ) {
            set_env("SDKWORK_WEBSERVER_PC_STATIC_ROOT", &value);
        }
        if let Some(value) = resolve_static_root_value(
            &self.h5_static_root,
            &self.h5_static_by_environment,
            environment,
        ) {
            set_env("SDKWORK_WEBSERVER_H5_STATIC_ROOT", &value);
        }
        if let Some(value) = resolve_static_root_value(
            &self.static_fallback_root,
            &self.static_fallback_by_environment,
            environment,
        ) {
            set_env("SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT", &value);
        }
        if let Some(value) = &self.tablet_surface {
            set_env("SDKWORK_WEBSERVER_TABLET_SURFACE", value);
        }
    }
}

fn resolve_static_root_value(
    explicit: &Option<String>,
    by_environment: &HashMap<String, String>,
    environment: Option<&str>,
) -> Option<String> {
    if let Some(value) = explicit {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    let environment = environment?.trim();
    if environment.is_empty() {
        return None;
    }
    by_environment
        .get(environment)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploySection {
    pub deployment_profile: Option<String>,
    pub environment: Option<String>,
    pub profile_id: Option<String>,
    pub use_memory_drive: Option<bool>,
    pub use_memory_content_provider: Option<bool>,
    pub drive_facade_url: Option<String>,
    pub drive_internal_api_url: Option<String>,
    pub drive_internal_api_ingress_token_file: Option<String>,
    pub knowledgebase_internal_api_url: Option<String>,
    pub knowledgebase_internal_api_ingress_token_file: Option<String>,
    pub web_internal_api_url: Option<String>,
    pub web_internal_api_ingress_token_file: Option<String>,
    pub runtime_assignment_worker_id: Option<String>,
}

impl DeploySection {
    fn apply_to_env(&self) {
        if let Some(value) = &self.deployment_profile {
            set_env("SDKWORK_DEPLOY_DEPLOYMENT_PROFILE", value);
        }
        if let Some(value) = &self.environment {
            set_env("SDKWORK_DEPLOY_ENVIRONMENT", value);
        }
        if let Some(value) = &self.profile_id {
            set_env("SDKWORK_DEPLOY_PROFILE_ID", value);
        }
        if let Some(value) = self.use_memory_drive {
            set_env("SDKWORK_DEPLOY_USE_MEMORY_DRIVE", &value.to_string());
        }
        if let Some(value) = self.use_memory_content_provider {
            set_env(
                "SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER",
                &value.to_string(),
            );
        }
        if let Some(value) = &self.drive_facade_url {
            set_env("SDKWORK_DRIVE_FACADE_URL", value);
        }
        if let Some(value) = &self.drive_internal_api_url {
            set_env("SDKWORK_DEPLOY_DRIVE_INTERNAL_API_URL", value);
        }
        if let Some(value) = &self.drive_internal_api_ingress_token_file {
            set_env(
                "SDKWORK_DEPLOY_DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE",
                value,
            );
        }
        if let Some(value) = &self.knowledgebase_internal_api_url {
            set_env("SDKWORK_DEPLOY_KNOWLEDGEBASE_INTERNAL_API_URL", value);
        }
        if let Some(value) = &self.knowledgebase_internal_api_ingress_token_file {
            set_env(
                "SDKWORK_DEPLOY_KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE",
                value,
            );
        }
        if let Some(value) = &self.web_internal_api_url {
            set_env("SDKWORK_DEPLOY_WEB_INTERNAL_API_URL", value);
        }
        if let Some(value) = &self.web_internal_api_ingress_token_file {
            set_env("SDKWORK_DEPLOY_WEB_INTERNAL_API_INGRESS_TOKEN_FILE", value);
        }
        if let Some(value) = &self.runtime_assignment_worker_id {
            set_env("SDKWORK_DEPLOY_RUNTIME_ASSIGNMENT_WORKER_ID", value);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseSection {
    pub engine: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub name: Option<String>,
    pub schema: Option<String>,
    pub schema_fallback_public: Option<bool>,
    pub username: Option<String>,
    /// Workspace database secret file
    /// (`/etc/sdkwork/webserver/secrets/database.secret` on a
    /// single-application host, ENVIRONMENT_SPEC section 7.3 exception);
    /// its content is injected in-process as `SDKWORK_DATABASE_PASSWORD`
    /// (the Drive dependency requires the direct value).
    pub password_file: Option<String>,
    pub ssl_mode: Option<String>,
    pub max_connections: Option<u32>,
    pub auto_migrate: Option<bool>,
}

impl DatabaseSection {
    fn apply_to_env(&self) -> Result<(), String> {
        if let Some(value) = &self.engine {
            set_env("SDKWORK_DATABASE_ENGINE", value);
        }
        if let Some(value) = &self.host {
            set_env("SDKWORK_DATABASE_HOST", value);
        }
        if let Some(value) = self.port {
            set_env("SDKWORK_DATABASE_PORT", &value.to_string());
        }
        if let Some(value) = &self.name {
            set_env("SDKWORK_DATABASE_NAME", value);
        }
        if let Some(value) = &self.schema {
            set_env("SDKWORK_DATABASE_SCHEMA", value);
        }
        if let Some(value) = self.schema_fallback_public {
            set_env(
                "SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC",
                &value.to_string(),
            );
        }
        if let Some(value) = &self.username {
            set_env("SDKWORK_DATABASE_USERNAME", value);
        }
        if let Some(value) = &self.password_file {
            let password = read_secret_file(value)?;
            set_env("SDKWORK_DATABASE_PASSWORD", &password);
        }
        if let Some(value) = &self.ssl_mode {
            set_env("SDKWORK_DATABASE_SSL_MODE", value);
        }
        if let Some(value) = self.max_connections {
            set_env("SDKWORK_DATABASE_MAX_CONNECTIONS", &value.to_string());
        }
        if let Some(value) = self.auto_migrate {
            set_env("SDKWORK_DATABASE_AUTO_MIGRATE", &value.to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretsSection {
    /// Encryption master key file for production-like environments
    /// (`SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY`).
    pub encryption_key_file: Option<String>,
    /// Deployments domain encryption master key file
    /// (`SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY`).
    pub deploy_encryption_key_file: Option<String>,
    /// Credential-entry bootstrap Access-Token file for the PC login page
    /// (`SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN`). The
    /// gateway injects its content into the served `index.html` so the
    /// identity-service metadata endpoints accept the login renderer.
    pub credential_entry_bootstrap_access_token_file: Option<String>,
}

impl SecretsSection {
    fn apply_to_env(&self) -> Result<(), String> {
        if let Some(value) = &self.encryption_key_file {
            set_env(
                "SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY",
                &read_secret_file(value)?,
            );
        }
        if let Some(value) = &self.deploy_encryption_key_file {
            set_env(
                "SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY",
                &read_secret_file(value)?,
            );
        }
        if let Some(value) = &self.credential_entry_bootstrap_access_token_file {
            // Optional secret: when the file is absent (production without a
            // provisioned credential-entry bootstrap token) the login page
            // simply is not bootstrapped; the gateway fails closed on the
            // identity endpoints instead of failing startup.
            match read_secret_file(value) {
                Ok(token) => set_env(
                    "SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN",
                    &token,
                ),
                Err(_) => set_env(
                    "SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN",
                    "",
                ),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcmeSection {
    pub profile: Option<String>,
    pub directory_url: Option<String>,
    pub contact_email: Option<String>,
    pub webroot: Option<String>,
    pub account_root: Option<String>,
    pub renew_before_days: Option<u64>,
    pub worker_id: Option<String>,
    pub operation_poll_interval_secs: Option<u64>,
    pub renew_scan_interval_secs: Option<u64>,
}

impl AcmeSection {
    fn apply_to_env(&self) {
        if let Some(value) = &self.profile {
            set_env("SDKWORK_WEBSERVER_ACME_PROFILE", value);
        }
        if let Some(value) = &self.directory_url {
            set_env("SDKWORK_WEBSERVER_ACME_DIRECTORY_URL", value);
        }
        if let Some(value) = &self.contact_email {
            set_env("SDKWORK_WEBSERVER_ACME_CONTACT_EMAIL", value);
        }
        if let Some(value) = &self.webroot {
            set_env("SDKWORK_WEBSERVER_ACME_WEBROOT", value);
        }
        if let Some(value) = &self.account_root {
            set_env("SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT", value);
        }
        if let Some(value) = self.renew_before_days {
            set_env(
                "SDKWORK_WEBSERVER_CERT_RENEW_BEFORE_DAYS",
                &value.to_string(),
            );
        }
        if let Some(value) = &self.worker_id {
            set_env("SDKWORK_WEBSERVER_CERT_WORKER_ID", value);
        }
        if let Some(value) = self.operation_poll_interval_secs {
            set_env(
                "SDKWORK_WEBSERVER_CERT_OPERATION_POLL_INTERVAL_SECS",
                &value.to_string(),
            );
        }
        if let Some(value) = self.renew_scan_interval_secs {
            set_env(
                "SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS",
                &value.to_string(),
            );
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsSection {
    pub material_root: Option<String>,
    pub runtime_snapshot_file: Option<String>,
    pub snapshot_alpn: Option<String>,
}

impl TlsSection {
    fn apply_to_env(&self) {
        if let Some(value) = &self.material_root {
            set_env("SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT", value);
        }
        if let Some(value) = &self.runtime_snapshot_file {
            set_env("SDKWORK_WEBSERVER_TLS_RUNTIME_SNAPSHOT_FILE", value);
        }
        if let Some(value) = &self.snapshot_alpn {
            set_env("SDKWORK_WEBSERVER_TLS_SNAPSHOT_ALPN", value);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSection {
    pub uuid: Option<String>,
}

impl NodeSection {
    fn apply_to_env(&self) {
        if let Some(value) = &self.uuid {
            set_env("SDKWORK_WEBSERVER_NODE_UUID", value);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionSection {
    pub region_code: Option<String>,
    pub seed_locale: Option<String>,
}

impl RegionSection {
    fn apply_to_env(&self) {
        if let Some(value) = &self.region_code {
            set_env("SDKWORK_WEBSERVER_REGION_CODE", value);
        }
        if let Some(value) = &self.seed_locale {
            set_env("SDKWORK_DATABASE_SEED_LOCALE", value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_paths::RUNTIME_CONFIG_FILE_ENV_LEGACY;
    use crate::runtime_env::env_test_lock;

    #[test]
    fn retired_runtime_config_env_alias_fails_closed() {
        let _guard = env_test_lock();
        let path = std::env::temp_dir().join("sdkwork-webserver-legacy-env-test.toml");
        std::fs::write(&path, "[profile]\nenvironment = \"test\"\n").unwrap();
        std::env::set_var(RUNTIME_CONFIG_FILE_ENV_LEGACY, &path);
        let error = resolve_runtime_config_path().expect_err("legacy env must fail closed");
        assert!(
            error.contains(RUNTIME_CONFIG_FILE_ENV_LEGACY)
                && error.contains(RUNTIME_CONFIG_FILE_ENV),
            "unexpected diagnostic: {error}"
        );
        std::env::remove_var(RUNTIME_CONFIG_FILE_ENV_LEGACY);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parses_and_applies_typed_config() {
        let _guard = env_test_lock();
        let path = std::env::temp_dir().join("sdkwork-webserver-runtime-config-test.toml");
        let secret_path = std::env::temp_dir().join("sdkwork-webserver-runtime-config-test.secret");
        std::fs::write(&secret_path, "test-db-password\n").unwrap();
        let secret_path_text = secret_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &path,
            format!(
                r#"
[profile]
deployment_profile = "standalone"
environment = "test"
profile_id = "standalone.test"
node_id = 0

[ingress]
bind = "0.0.0.0:8888"
management_expose_allowed = true
data_plane_operations_bind = "127.0.0.1:3901"
public_http_url = "http://server-test.sdkwork.com:8888"
cors_allowed_origins = ["http://server-test.sdkwork.com:8888"]

[app_roots]
app_root = "/usr/lib/sdkwork/webserver"
iam_app_root = "/var/lib/sdkwork/webserver/iam"
drive_app_root = "/var/lib/sdkwork/webserver/drive"
pc_static_root = "/usr/share/sdkwork/webserver/web/pc"
h5_static_root = "/usr/share/sdkwork/webserver/web/h5"
static_fallback_root = "/usr/share/sdkwork/webserver/web/static"

[deploy]
deployment_profile = "standalone"
environment = "test"
profile_id = "standalone.test"
use_memory_drive = false
use_memory_content_provider = false
drive_facade_url = "http://server-test.sdkwork.com:8888"
web_internal_api_url = "http://server-test.sdkwork.com:8888"
runtime_assignment_worker_id = "deploy-worker-0"

[database]
engine = "postgresql"
host = "127.0.0.1"
port = 5432
name = "sdkwork_ai_test"
schema = "sdkwork_ai_test"
schema_fallback_public = false
username = "sdkwork_ai_test"
password_file = "{secret_path_text}"
ssl_mode = "disable"
max_connections = 10
auto_migrate = true

[acme]
profile = "staging"
directory_url = "https://acme-staging-v02.api.letsencrypt.org/directory"
contact_email = "admin@localhost"
webroot = "/var/lib/sdkwork/webserver/acme-webroot"
account_root = "/var/lib/sdkwork/webserver/acme-accounts"
worker_id = "certificate-worker-0"

[tls]
material_root = "/var/lib/sdkwork/webserver/tls-materials"
runtime_snapshot_file = "/var/lib/sdkwork/webserver/tls-materials/tls-runtime.json"
snapshot_alpn = "h2,http/1.1"

[node]
uuid = "standalone-test-node"

[region]
region_code = "cn"
seed_locale = "zh-CN"
"#
            ),
        )
        .unwrap();
        let config = parse_runtime_toml_config(&path).expect("config must parse");
        config.apply_to_env().expect("config must apply");
        assert_eq!(
            std::env::var("SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND").unwrap(),
            "0.0.0.0:8888"
        );
        assert_eq!(
            std::env::var("SDKWORK_DATABASE_NAME").unwrap(),
            "sdkwork_ai_test"
        );
        assert_eq!(
            std::env::var("SDKWORK_CORS_ALLOWED_ORIGINS").unwrap(),
            "http://server-test.sdkwork.com:8888"
        );
        assert_eq!(
            std::env::var("SDKWORK_DEPLOY_USE_MEMORY_DRIVE").unwrap(),
            "false"
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&secret_path);
    }

    #[test]
    fn selects_static_roots_from_environment_catalog() {
        let _guard = env_test_lock();
        for key in [
            "SDKWORK_WEBSERVER_PC_STATIC_ROOT",
            "SDKWORK_WEBSERVER_H5_STATIC_ROOT",
            "SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT",
        ] {
            std::env::remove_var(key);
        }
        let path = std::env::temp_dir().join("sdkwork-webserver-runtime-config-by-env.toml");
        std::fs::write(
            &path,
            r#"
[profile]
environment = "development"

[app_roots]
tablet_surface = "pc"

[app_roots.pc_static_by_environment]
development = "apps/sdkwork-webserver-pc/dist/standalone/dev"
test = "apps/sdkwork-webserver-pc/dist/standalone/test"
staging = "apps/sdkwork-webserver-pc/dist/standalone/staging"
production = "apps/sdkwork-webserver-pc/dist/standalone/prod"

[app_roots.h5_static_by_environment]
development = "apps/sdkwork-webserver-h5/dist/standalone/dev"
test = "apps/sdkwork-webserver-h5/dist/standalone/test"
staging = "apps/sdkwork-webserver-h5/dist/standalone/staging"
production = "apps/sdkwork-webserver-h5/dist/standalone/prod"

[app_roots.static_fallback_by_environment]
development = "deployments/webserver/static"
test = "deployments/webserver/static"
staging = "deployments/webserver/static"
production = "deployments/webserver/static"
"#,
        )
        .unwrap();
        let config = parse_runtime_toml_config(&path).expect("config must parse");
        config.apply_to_env().expect("config must apply");
        assert_eq!(
            std::env::var("SDKWORK_WEBSERVER_PC_STATIC_ROOT").unwrap(),
            "apps/sdkwork-webserver-pc/dist/standalone/dev"
        );
        assert_eq!(
            std::env::var("SDKWORK_WEBSERVER_H5_STATIC_ROOT").unwrap(),
            "apps/sdkwork-webserver-h5/dist/standalone/dev"
        );
        assert_eq!(
            std::env::var("SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT").unwrap(),
            "deployments/webserver/static"
        );
        // Every lifecycle environment resolves its own roots from the catalog;
        // the active environment is the config's `[profile] environment` and
        // dist directories use the standard aliases (dev/test/staging/prod)
        // under the standalone profile segment.
        for environment in ["development", "test", "staging", "production"] {
            let dist_alias = match environment {
                "development" => "dev",
                "production" => "prod",
                other => other,
            };
            assert_eq!(
                resolve_static_root_value(
                    &None,
                    &config.app_roots.pc_static_by_environment,
                    Some(environment),
                ),
                Some(format!(
                    "apps/sdkwork-webserver-pc/dist/standalone/{dist_alias}"
                ))
            );
            assert_eq!(
                resolve_static_root_value(
                    &None,
                    &config.app_roots.h5_static_by_environment,
                    Some(environment),
                ),
                Some(format!(
                    "apps/sdkwork-webserver-h5/dist/standalone/{dist_alias}"
                ))
            );
        }
        // An explicit root wins over the environment catalog.
        assert_eq!(
            resolve_static_root_value(
                &Some("/usr/share/sdkwork/webserver/web/pc".to_owned()),
                &config.app_roots.pc_static_by_environment,
                Some("production"),
            ),
            Some("/usr/share/sdkwork/webserver/web/pc".to_owned())
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_unknown_fields() {
        let path = std::env::temp_dir().join("sdkwork-webserver-runtime-config-bad.toml");
        std::fs::write(&path, "[profile]\nunknown_key = true\n").unwrap();
        let error = parse_runtime_toml_config(&path).expect_err("unknown fields must be rejected");
        assert!(error.contains("unknown_key"));
        let _ = std::fs::remove_file(&path);
    }

    fn write_import_file(
        dir: &std::path::Path,
        name: &str,
        id: &str,
        path: &str,
    ) -> std::path::PathBuf {
        let file = dir.join(name);
        std::fs::write(
            &file,
            format!(
                "[[webserver.imports]]\nid = \"{id}\"\npath = \"{path}\"\nenabled = true\nrequired = false\nprobe_upstreams = false\n"
            ),
        )
        .unwrap();
        file
    }

    #[test]
    fn expands_webserver_include_conf_files() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-include-conf-{}",
            std::process::id()
        ));
        let imports_dir = temp.join("imports.d");
        std::fs::create_dir_all(&imports_dir).unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/*.conf\"]\n",
        )
        .unwrap();
        let conf_a = imports_dir.join("sdkwork-a.conf");
        let conf_b = imports_dir.join("sdkwork-b.conf");
        std::fs::write(&conf_a, "user sdkwork;\nevents {}\nhttp {}\n").unwrap();
        std::fs::write(&conf_b, "user sdkwork;\nevents {}\nhttp {}\n").unwrap();
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let expanded =
            expand_webserver_import_includes(&config_path, &config.webserver).expect("expand");
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].id, "sdkwork-a");
        assert_eq!(expanded[0].path, conf_a.to_string_lossy());
        assert_eq!(expanded[1].id, "sdkwork-b");
        assert_eq!(expanded[1].path, conf_b.to_string_lossy());
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn expands_webserver_include_glob_after_inline_imports() {
        let temp =
            std::env::temp_dir().join(format!("sdkwork-webserver-include-{}", std::process::id()));
        let imports_dir = temp.join("imports.d");
        std::fs::create_dir_all(&imports_dir).unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/*.toml\"]\n\n[[webserver.imports]]\nid = \"inline\"\npath = \"/srv/inline\"\n",
        )
        .unwrap();
        write_import_file(&imports_dir, "b-module.toml", "sdkwork-b", "/srv/b");
        write_import_file(&imports_dir, "a-module.toml", "sdkwork-a", "/srv/a");
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let expanded =
            expand_webserver_import_includes(&config_path, &config.webserver).expect("expand");
        let ids: Vec<&str> = expanded.iter().map(|entry| entry.id.as_str()).collect();
        // Inline entries first, then included files in sorted order.
        assert_eq!(ids, vec!["inline", "sdkwork-a", "sdkwork-b"]);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn included_import_overrides_inline_entry_with_same_id() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-include-override-{}",
            std::process::id()
        ));
        let imports_dir = temp.join("imports.d");
        std::fs::create_dir_all(&imports_dir).unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/*.toml\"]\n\n[[webserver.imports]]\nid = \"sdkwork-im\"\npath = \"/srv/inline\"\n",
        )
        .unwrap();
        write_import_file(&imports_dir, "im.toml", "sdkwork-im", "/srv/included");
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let expanded =
            expand_webserver_import_includes(&config_path, &config.webserver).expect("expand");
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].path.as_str(), "/srv/included");
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn expands_import_conf_aggregator_into_module_sidecars() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-import-aggregator-{}",
            std::process::id()
        ));
        let imports_dir = temp.join("imports.d");
        let checkout = temp.join("sdkwork-space");
        let im_ws = checkout.join("sdkwork-im/deployments/webserver");
        std::fs::create_dir_all(&imports_dir).unwrap();
        std::fs::create_dir_all(&im_ws).unwrap();
        let im_conf = im_ws.join("nginx.standalone.development.conf");
        std::fs::write(
            &im_conf,
            "user sdkwork;\nevents {}\nhttp { server { listen 80; server_name im-dev.example.com; location / { return 200; } } }\n",
        )
        .unwrap();
        // nginx applies the `\t`/`\r`/`\n` escape table to unquoted tokens
        // (ngx_conf_read_token), so a raw Windows path like `...\nginx.conf`
        // is invalid conf content: generators must emit forward slashes.
        let im_conf_pattern = im_conf.to_string_lossy().replace('\\', "/");
        std::fs::write(
            imports_dir.join("import.conf"),
            format!("include {im_conf_pattern};\n"),
        )
        .unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/import.conf\"]\n",
        )
        .unwrap();
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let expanded =
            expand_webserver_import_includes(&config_path, &config.webserver).expect("expand");
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].id, "sdkwork-im");
        assert_eq!(expanded[0].path, im_conf_pattern);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn expands_import_conf_aggregator_for_checkout_sidecar() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-import-checkout-{}",
            std::process::id()
        ));
        let imports_dir = temp.join("imports.d");
        let checkout = temp.join("sdkwork-space");
        let im_ws = checkout.join("sdkwork-im/deployments/webserver");
        std::fs::create_dir_all(&imports_dir).unwrap();
        std::fs::create_dir_all(&im_ws).unwrap();
        let im_conf = im_ws.join("nginx.standalone.development.conf");
        std::fs::write(
            &im_conf,
            "user sdkwork;\nevents {}\nhttp { server { listen 80; server_name im-dev.example.com; location / { return 200; } } }\n",
        )
        .unwrap();
        // Forward slashes only: see the note in
        // expands_import_conf_aggregator_into_module_sidecars.
        let im_conf_pattern = im_conf.to_string_lossy().replace('\\', "/");
        std::fs::write(
            imports_dir.join("import.conf"),
            format!("include {im_conf_pattern};\n"),
        )
        .unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/import.conf\"]\n",
        )
        .unwrap();
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let expanded =
            expand_webserver_import_includes(&config_path, &config.webserver).expect("expand");
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].id, "sdkwork-im");
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn missing_explicit_include_file_fails_closed() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-include-missing-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/modules.toml\"]\n",
        )
        .unwrap();
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let error = expand_webserver_import_includes(&config_path, &config.webserver)
            .expect_err("missing explicit include must fail closed");
        assert!(
            error.contains("missing file"),
            "unexpected diagnostic: {error}"
        );
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn conf_and_toml_imports_for_same_id_are_mutually_exclusive() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-include-exclusive-{}",
            std::process::id()
        ));
        let imports_dir = temp.join("imports.d");
        std::fs::create_dir_all(&imports_dir).unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/*.conf\", \"imports.d/*.toml\"]\n",
        )
        .unwrap();
        std::fs::write(
            imports_dir.join("sdkwork-im.conf"),
            "user sdkwork;\nevents {}\nhttp { server { listen 80; server_name im.example.com; location / { return 200; } } }\n",
        )
        .unwrap();
        write_import_file(
            &imports_dir,
            "sdkwork-im.toml",
            "sdkwork-im",
            "/srv/layout-v3",
        );
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let error = expand_webserver_import_includes(&config_path, &config.webserver)
            .expect_err("conf and toml imports for the same id must fail closed");
        assert!(
            error.contains("mutually exclusive"),
            "unexpected diagnostic: {error}"
        );
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn empty_include_glob_is_skipped() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-include-empty-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let config_path = temp.join("config.toml");
        std::fs::write(
            &config_path,
            "[webserver]\ninclude = [\"imports.d/*.toml\"]\n",
        )
        .unwrap();
        let config = parse_runtime_toml_config(&config_path).expect("config must parse");
        let expanded =
            expand_webserver_import_includes(&config_path, &config.webserver).expect("expand");
        assert!(expanded.is_empty());
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn glob_match_supports_star_and_question_mark() {
        assert!(glob_match("*.toml", "a.toml"));
        assert!(glob_match("*.toml", ".toml"));
        assert!(!glob_match("*.toml", "a.toml.bak"));
        assert!(glob_match("module-?.toml", "module-1.toml"));
        assert!(!glob_match("module-?.toml", "module-12.toml"));
        assert!(glob_match("*", "anything"));
        assert!(!glob_match("a*b", "ac"));
    }

    fn certificate_fixture(
        id: &str,
        server_names: Vec<String>,
        base: &std::path::Path,
    ) -> crate::config::CertificateConfig {
        crate::config::CertificateConfig {
            id: id.to_owned(),
            server_names,
            source: crate::config::CertificateSource::ProtectedFile {
                certificate_file: format!("certs/{id}/fullchain.pem"),
                private_key_file: format!("certs/{id}/privkey.pem"),
            },
        }
    }

    #[test]
    fn bootstrap_certificates_are_generated_and_preserved() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-bootstrap-cert-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let app = crate::config::WebServerAppConfig {
            schema_version: 1,
            kind: "test".to_owned(),
            app_key: "test".to_owned(),
            nginx: Default::default(),
            gzip: Default::default(),
            limit_req_zones: Vec::new(),
            limit_conn_zones: Vec::new(),
            resolution_cache: None,
            limits: Default::default(),
            listeners: Vec::new(),
            certificates: vec![certificate_fixture(
                "sdkwork.com",
                vec!["im.sdkwork.com".to_owned(), "im-dev.sdkwork.com".to_owned()],
                &temp,
            )],
            tls_policies: Vec::new(),
            resolvers: Vec::new(),
            resources: Vec::new(),
            upstreams: Vec::new(),
            virtual_hosts: Vec::new(),
            streams: Vec::new(),
            proxy_cache: Default::default(),
            app_domain_fallback: None,
            usage_metering: None,
            observability: Default::default(),
            deployment: Default::default(),
            metadata: Default::default(),
        };
        ensure_import_bootstrap_certificates(&app, &temp).expect("generate");
        let cert_path = temp.join("certs/sdkwork.com/fullchain.pem");
        let key_path = temp.join("certs/sdkwork.com/privkey.pem");
        assert!(cert_path.is_file());
        assert!(key_path.is_file());
        let first = std::fs::read_to_string(&cert_path).unwrap();
        // Idempotent: a second pass must not overwrite existing material.
        ensure_import_bootstrap_certificates(&app, &temp).expect("second pass");
        assert_eq!(std::fs::read_to_string(&cert_path).unwrap(), first);
        // The material is a valid PEM pair (the leaf SAN covers the declared
        // server names; end-to-end SNI coverage is exercised by the data
        // plane integration).
        let pem = std::fs::read_to_string(&cert_path).unwrap();
        assert!(pem.contains("BEGIN CERTIFICATE"));
        assert!(std::fs::read_to_string(&key_path)
            .unwrap()
            .contains("BEGIN PRIVATE KEY"));
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn half_present_certificate_pair_fails_closed() {
        let temp = std::env::temp_dir().join(format!(
            "sdkwork-webserver-bootstrap-half-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp.join("certs/example.com")).unwrap();
        std::fs::write(temp.join("certs/example.com/fullchain.pem"), "stale").unwrap();
        let app = crate::config::WebServerAppConfig {
            schema_version: 1,
            kind: "test".to_owned(),
            app_key: "test".to_owned(),
            nginx: Default::default(),
            gzip: Default::default(),
            limit_req_zones: Vec::new(),
            limit_conn_zones: Vec::new(),
            resolution_cache: None,
            limits: Default::default(),
            listeners: Vec::new(),
            certificates: vec![certificate_fixture(
                "example.com",
                vec!["im.example.com".to_owned()],
                &temp,
            )],
            tls_policies: Vec::new(),
            resolvers: Vec::new(),
            resources: Vec::new(),
            upstreams: Vec::new(),
            virtual_hosts: Vec::new(),
            streams: Vec::new(),
            proxy_cache: Default::default(),
            app_domain_fallback: None,
            usage_metering: None,
            observability: Default::default(),
            deployment: Default::default(),
            metadata: Default::default(),
        };
        let error = ensure_import_bootstrap_certificates(&app, &temp)
            .expect_err("half-present pair must fail closed");
        assert!(error.contains("only one of"), "unexpected: {error}");
        assert_eq!(
            std::fs::read_to_string(temp.join("certs/example.com/fullchain.pem")).unwrap(),
            "stale"
        );
        let _ = std::fs::remove_dir_all(&temp);
    }
}
