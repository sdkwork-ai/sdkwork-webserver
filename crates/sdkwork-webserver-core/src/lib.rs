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

pub use nginx::prefer_h5_surface;
pub use config::{
    apply_rewrites, apply_sub_filters, apr1_hash, evaluate_access, evaluate_auth_basic,
    inspect_webserver_config_revision, load_and_compile_webserver_config,
    load_and_compile_webserver_config_json, load_and_compile_webserver_config_revision, normalize_authority_host, normalize_server_name,
    normalize_uri_path, parse_htpasswd, parse_limit_conn, parse_limit_conn_zone, parse_limit_req,
    parse_limit_req_zone, parse_rewrite, expand_proxy_pass_template, validate_proxy_pass_template,
    resolve_nginx_sidecar_path, resolve_webserver_config_path, server_name_covers, sub_filter_content_type_matches,
    hostname_upstream_allowed_cidrs, upstream_ip_is_allowed, validate_md5_template, verify_md5_link, verify_secure_link,
    verify_secret_link, md5_hex, AccessAction,
    AccessDecision, AccessRuleConfig, AppDomainFallbackConfig, AppDomainFallbackLookup,
    AuthBasicConfig, AuthBasicDecision, AuthBasicUserConfig, UsageMeteringChannel,
    UsageMeteringConfig,
    CertificateConfig, CertificateSource, ClientAuthConfig, ClientAuthMode,
    CompiledWebServerApp, CompiledWebServerRevision, ConfigDiagnostic, ConfigFormat,
    ConfigLoadOptions, ConfigProviderType, ConfigSource,
    CustomHeaderConfig, GzipConfig, HtpasswdParseError, JsonConfigSource, LimitConnConfig,
    LimitConnParseError, LimitConnZoneConfig, LimitReqConfig, LimitReqParseError,
    LimitReqZoneConfig, ListenerConfig, ListenerProtocol, ListenerTlsRuntime, LoadedWebServerConfig,
    NginxConfig, NginxConfConfigSource,
    ProviderCachePolicy, ProxyCacheConfig, ProxyProtocolConfig, ProxyProtocolCrc32cPolicy, ProxyProtocolVersion,
    ReloadConfig, ReloadMode, ResolverConfig, ResourceConfig, ResourcePressureConfig,
    ResourceSampleFailurePolicy, RewriteApplyError, RewriteFlag, RewriteOutcome, RewriteParseError,
    RewriteRuleConfig, RouteConfig, RouteMatchConfig, RoutePathType, SecureLinkFailure,
    SecureLinkMode, SecurityHeadersConfig,
    SelectedRoute, StreamProtocol, StreamServerConfig, StreamTargetConfig, StreamTlsMode,
    StrictTransportSecurityConfig, SubFilterConfig, SubFilterRule, TlsPolicyConfig, TlsVersion, TomlConfigSource, TrustedProxyConfig,
    TrustedProxyHeader, UpstreamActiveHealthConfig, UpstreamActiveHealthMethod,
    UpstreamAddressPolicyConfig, UpstreamConfig, UpstreamHashConfig, UpstreamHashKeyVar,
    UpstreamLoadBalancingStrategy, UpstreamPassiveHealthConfig, UpstreamRetryCondition,
    UpstreamRetryConfig, UpstreamTlsConfig,
    UpstreamTlsTrustMode, UriPathNormalizationError, VirtualHostConfig, WebServerAppConfig,
    WebServerConfigError, WebServerConfigFileRevision, WebServerConfigLoader, WebServerLimits, XFrameOptions,
    MAX_CONFIG_BYTES, MAX_REWRITE_INTERNAL_REDIRECTS, MAX_SUB_FILTER_BODY_BYTES,
    WEBSERVER_CONFIG_FILE_ENV,
    WEBSERVER_CONFIG_FILE_NAME, APP_ROOT_ENV, DEPLOYMENT_PROFILE_ENV, ENVIRONMENT_ENV,
    NGINX_CONFIG_FILE_ENV,
};
pub use config_paths::{
    canonical_certificate_domain_directory, canonical_certificate_file,
    canonical_certificate_key_file, canonical_certificates_directory,
    canonical_data_plane_config_path, canonical_runtime_config_path,
    canonical_secrets_directory, canonical_webserver_config_directory,
    runtime_config_override_from_env, APPLICATION_CODE, CERTIFICATES_SUBDIR,
    CERTIFICATE_CHAIN_FILE_NAME, CERTIFICATE_FILE_NAME, CERTS_URI_SCHEME,
    DATA_PLANE_CONFIG_FILE_ENV, DATA_PLANE_CONFIG_FILE_NAME, LINUX_CONFIG_ROOT,
    PRIVATE_KEY_FILE_NAME, RUNTIME_CONFIG_FILE_ENV, RUNTIME_CONFIG_FILE_ENV_LEGACY,
    RUNTIME_CONFIG_FILE_NAME, SECRETS_SUBDIR,
};
pub use module_imports::{
    load_module_import_app_config, merge_import_specs, parse_env_imports, resolve_import_path,
    resolve_import_profile, validate_imports, validate_module_import, ModuleImportError,
    ModuleImportValidation, WebserverImportEntry, WebserverModuleImport, MODULE_IMPORTS_ENV,
};
pub use runtime_config::{
    compile_merged_imports_app, configured_module_imports, expand_webserver_import_includes,
    imported_certificate_names, load_runtime_toml_config, merged_imports_app_config,
    parse_runtime_toml_config, resolve_runtime_config_path, validate_configured_module_imports,
    IMPORT_LISTENER_PORTS_ENV, RuntimeTomlConfig,
};
pub use runtime_env::{
    web_dev_auth_bypass_enabled, web_environment_name, web_is_production_like_environment,
    web_use_dev_inline_auth_resolver,
};
