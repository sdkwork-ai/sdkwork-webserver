//! Framework-independent Web Server configuration and runtime helpers.

mod canonical_json;

pub mod config;
pub mod config_paths;
pub mod module_imports;
pub mod nginx;
pub mod runtime_config;
pub mod runtime_env;
pub mod tls_runtime;
pub mod website_runtime;

pub use config::{
    apply_rewrites, apr1_hash, evaluate_access, evaluate_auth_basic,
    inspect_webserver_config_revision, load_and_compile_webserver_config,
    load_and_compile_webserver_config_json, load_and_compile_webserver_config_revision, normalize_authority_host, normalize_server_name,
    normalize_uri_path, parse_htpasswd, parse_limit_req, parse_limit_req_zone, parse_rewrite,
    resolve_webserver_config_path, server_name_covers, upstream_ip_is_allowed, AccessAction,
    AccessDecision, AccessRuleConfig, AuthBasicConfig, AuthBasicDecision, AuthBasicUserConfig,
    CertificateConfig, CertificateSource, ClientAuthConfig, ClientAuthMode,
    CompiledWebServerApp, CompiledWebServerRevision, ConfigDiagnostic, ConfigProviderType,
    CustomHeaderConfig, GzipConfig, HtpasswdParseError, LimitReqConfig, LimitReqParseError,
    LimitReqZoneConfig, ListenerConfig, ListenerProtocol, ListenerTlsRuntime, NginxConfig,
    ProviderCachePolicy, ProxyCacheConfig, ProxyProtocolConfig, ProxyProtocolCrc32cPolicy, ProxyProtocolVersion,
    ReloadConfig, ReloadMode, ResolverConfig, ResourceConfig, ResourcePressureConfig,
    ResourceSampleFailurePolicy, RewriteApplyError, RewriteFlag, RewriteOutcome, RewriteParseError,
    RewriteRuleConfig, RouteConfig, RouteMatchConfig, RoutePathType, SecurityHeadersConfig,
    SelectedRoute, StreamServerConfig, StreamTargetConfig, StreamTlsMode,
    StrictTransportSecurityConfig, TlsPolicyConfig, TlsVersion, TrustedProxyConfig,
    TrustedProxyHeader, UpstreamActiveHealthConfig, UpstreamActiveHealthMethod,
    UpstreamAddressPolicyConfig, UpstreamConfig, UpstreamHashConfig, UpstreamHashKeyVar,
    UpstreamLoadBalancingStrategy, UpstreamPassiveHealthConfig, UpstreamRetryCondition,
    UpstreamRetryConfig, UpstreamTlsConfig,
    UpstreamTlsTrustMode, UriPathNormalizationError, VirtualHostConfig, WebServerAppConfig,
    WebServerConfigError, WebServerConfigFileRevision, WebServerLimits, XFrameOptions,
    MAX_CONFIG_BYTES, MAX_REWRITE_INTERNAL_REDIRECTS, WEBSERVER_CONFIG_FILE_ENV,
    WEBSERVER_CONFIG_FILE_NAME,
};
pub use config_paths::{
    canonical_data_plane_config_path, canonical_runtime_config_path,
    canonical_secrets_directory, canonical_webserver_config_directory,
    runtime_config_override_from_env, APPLICATION_CODE, DATA_PLANE_CONFIG_FILE_ENV,
    DATA_PLANE_CONFIG_FILE_NAME, LINUX_CONFIG_ROOT, RUNTIME_CONFIG_FILE_ENV,
    RUNTIME_CONFIG_FILE_ENV_LEGACY, RUNTIME_CONFIG_FILE_NAME, SECRETS_SUBDIR,
};
pub use module_imports::{
    merge_import_specs, parse_env_imports, resolve_import_path, resolve_import_profile,
    validate_imports, validate_module_import, ModuleImportError, ModuleImportValidation,
    WebserverImportEntry, WebserverModuleImport, MODULE_IMPORTS_ENV,
};
pub use runtime_config::{
    load_runtime_toml_config, parse_runtime_toml_config, resolve_runtime_config_path,
    validate_configured_module_imports, RuntimeTomlConfig,
};
pub use runtime_env::{
    web_dev_auth_bypass_enabled, web_environment_name, web_is_production_like_environment,
    web_use_dev_inline_auth_resolver,
};
