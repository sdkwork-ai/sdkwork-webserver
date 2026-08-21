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

use crate::config_paths::{
    canonical_runtime_config_path, runtime_config_override_from_env, RUNTIME_CONFIG_FILE_ENV,
};
use crate::module_imports::{merge_import_specs, validate_imports, WebserverImportEntry};

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
    #[serde(default)]
    pub imports: Vec<WebserverImportEntry>,
}

/// Validate imported sibling-module `deployments/webserver/` directories
/// declared in the runtime TOML and/or `SDKWORK_WEBSERVER_MODULE_IMPORTS`.
///
/// Base directory for resolving relative import paths.
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
    let base = module_import_resolution_base();
    let from_runtime = if let Ok(Some(path)) = resolve_runtime_config_path() {
        parse_runtime_toml_config(&path)?.webserver.imports
    } else {
        Vec::new()
    };
    let imports = merge_import_specs(&base, &from_runtime).map_err(|error| error.to_string())?;
    if imports.is_empty() {
        return Ok(Vec::new());
    }
    validate_imports(&imports).map_err(|error| error.to_string())
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
        if let Some(value) =
            resolve_static_root_value(&self.pc_static_root, &self.pc_static_by_environment, environment)
        {
            set_env("SDKWORK_WEBSERVER_PC_STATIC_ROOT", &value);
        }
        if let Some(value) =
            resolve_static_root_value(&self.h5_static_root, &self.h5_static_by_environment, environment)
        {
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
            error.contains(RUNTIME_CONFIG_FILE_ENV_LEGACY) && error.contains(RUNTIME_CONFIG_FILE_ENV),
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
development = "apps/sdkwork-webserver-pc/dist/dev"
test = "apps/sdkwork-webserver-pc/dist/test"
staging = "apps/sdkwork-webserver-pc/dist/staging"
production = "apps/sdkwork-webserver-pc/dist/prod"

[app_roots.h5_static_by_environment]
development = "apps/sdkwork-webserver-h5/dist/dev"
test = "apps/sdkwork-webserver-h5/dist/test"
staging = "apps/sdkwork-webserver-h5/dist/staging"
production = "apps/sdkwork-webserver-h5/dist/prod"

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
            "apps/sdkwork-webserver-pc/dist/dev"
        );
        assert_eq!(
            std::env::var("SDKWORK_WEBSERVER_H5_STATIC_ROOT").unwrap(),
            "apps/sdkwork-webserver-h5/dist/dev"
        );
        assert_eq!(
            std::env::var("SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT").unwrap(),
            "deployments/webserver/static"
        );
        // Every lifecycle environment resolves its own roots from the catalog;
        // the active environment is the config's `[profile] environment` and
        // dist directories use the standard aliases (dev/test/staging/prod).
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
                Some(format!("apps/sdkwork-webserver-pc/dist/{dist_alias}"))
            );
            assert_eq!(
                resolve_static_root_value(
                    &None,
                    &config.app_roots.h5_static_by_environment,
                    Some(environment),
                ),
                Some(format!("apps/sdkwork-webserver-h5/dist/{dist_alias}"))
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
}
