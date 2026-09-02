mod access;
mod auth_basic;
mod compiled;
mod discovery;
mod error;
mod limit_conn;
mod limit_req;
mod loader;
mod model;
mod network;
mod proxy_headers;
mod proxy_pass;
mod rewrite;
mod secure_link;
mod server_toml;
mod source;
mod sub_filter;
mod uri;
mod validate;

pub use access::{evaluate_access, AccessDecision};
pub use auth_basic::{
    apr1_hash, evaluate_auth_basic, parse_htpasswd, AuthBasicDecision, HtpasswdParseError,
};
pub use compiled::{normalize_authority_host, CompiledWebServerApp, SelectedRoute};
pub use discovery::{
    canonical_webserver_config_directory, resolve_nginx_sidecar_path,
    resolve_webserver_config_path, APP_ROOT_ENV, DEPLOYMENT_PROFILE_ENV, ENVIRONMENT_ENV,
    NGINX_CONFIG_FILE_ENV, WEBSERVER_CONFIG_FILE_ENV, WEBSERVER_CONFIG_FILE_NAME,
};
pub use error::{ConfigDiagnostic, WebServerConfigError};
pub use limit_conn::{parse_limit_conn, parse_limit_conn_zone, LimitConnParseError};
pub use limit_req::{parse_limit_req, parse_limit_req_zone, LimitReqParseError};
pub use loader::{
    inspect_webserver_config_revision, load_and_compile_webserver_config,
    load_and_compile_webserver_config_json, load_and_compile_webserver_config_revision,
    CompiledWebServerRevision, WebServerConfigFileRevision, MAX_CONFIG_BYTES,
};
pub use model::{
    AccessAction, AccessRuleConfig, AcmeHttp01Config, AppDomainFallbackConfig,
    AppDomainFallbackLookup, AuthBasicConfig, AuthBasicUserConfig, CertificateConfig,
    CertificateSource, ClientAuthConfig, ClientAuthMode, ConfigProviderType, CustomHeaderConfig,
    DeploymentConfig, GzipConfig, LimitConnConfig, LimitConnZoneConfig, LimitReqConfig,
    LimitReqZoneConfig, ListenerConfig, ListenerProtocol, ListenerTlsRuntime, NginxConfig,
    ObservabilityConfig, ProviderCachePolicy, ProxyCacheConfig, ProxyProtocolConfig,
    ProxyProtocolCrc32cPolicy, ProxyProtocolVersion, ReloadConfig, ReloadMode, ResolverConfig,
    ResourceConfig, ResourcePressureConfig, ResourceSampleFailurePolicy, RewriteFlag,
    RewriteRuleConfig, RouteConfig, RouteMatchConfig, RoutePathType, SecureLinkMode,
    SecurityHeadersConfig, StreamProtocol, StreamServerConfig, StreamTargetConfig, StreamTlsMode,
    StrictTransportSecurityConfig, SubFilterConfig, SubFilterRule, TlsPolicyConfig, TlsVersion,
    TrustedProxyConfig, TrustedProxyHeader, UpstreamActiveHealthConfig, UpstreamActiveHealthMethod,
    UpstreamAddressPolicyConfig, UpstreamConfig, UpstreamHashConfig, UpstreamHashKeyVar,
    UpstreamLoadBalancingStrategy, UpstreamPassiveHealthConfig, UpstreamRetryCondition,
    UpstreamRetryConfig, UpstreamTargetConfig, UpstreamTlsConfig, UpstreamTlsTrustMode,
    UsageMeteringChannel, UsageMeteringConfig, VirtualHostConfig, WebServerAppConfig,
    WebServerLimits, XFrameOptions,
};
pub use network::{
    hostname_upstream_allowed_cidrs, is_supported_upstream_allowed_cidr, upstream_ip_is_allowed,
};
pub use proxy_headers::{
    format_proxy_set_header_entry, merge_proxy_set_headers, validate_proxy_set_header_entry,
    SUPPORTED_PROXY_HEADER_VARS,
};
pub use proxy_pass::{expand_proxy_pass_template, validate_proxy_pass_template};
pub use rewrite::{
    apply_rewrites, parse_rewrite, RewriteApplyError, RewriteOutcome, RewriteParseError,
    MAX_REWRITE_INTERNAL_REDIRECTS,
};
pub use sdkwork_webserver_resolver_cache::{RedisCacheConfig, ResolutionCacheConfig};
pub use secure_link::{
    md5_hex, validate_md5_template, verify_md5_link, verify_secret_link, verify_secure_link,
    SecureLinkFailure,
};
pub use server_toml::{
    load_server_toml_app, load_server_toml_app_effective, load_server_toml_file, materialize_app,
    merge_common_profile, merge_effective, merge_overlay,
};
pub use source::{
    ConfigFormat, ConfigLoadOptions, ConfigSource, JsonConfigSource, LoadedWebServerConfig,
    NginxConfConfigSource, TomlConfigSource, WebServerConfigLoader, DEFAULT_APP_KEY,
    DEFAULT_TOML_PROFILE,
};
pub use sub_filter::{
    apply_sub_filters, sub_filter_content_type_matches, MAX_SUB_FILTER_BODY_BYTES,
};
pub use uri::{normalize_uri_path, UriPathNormalizationError};
pub use validate::{normalize_server_name, server_name_covers};

pub(crate) use validate::validate_webserver_config;
