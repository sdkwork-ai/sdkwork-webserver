mod access;
mod auth_basic;
mod compiled;
mod discovery;
mod error;
mod limit_req;
mod loader;
mod model;
mod network;
mod proxy_headers;
mod rewrite;
mod server_toml;
mod source;
mod uri;
mod validate;

pub use access::{evaluate_access, AccessDecision};
pub use auth_basic::{
    apr1_hash, evaluate_auth_basic, parse_htpasswd, AuthBasicDecision, HtpasswdParseError,
};
pub use compiled::{normalize_authority_host, CompiledWebServerApp, SelectedRoute};
pub use discovery::{
    canonical_webserver_config_directory, resolve_webserver_config_path, WEBSERVER_CONFIG_FILE_ENV,
    WEBSERVER_CONFIG_FILE_NAME,
};
pub use error::{ConfigDiagnostic, WebServerConfigError};
pub use limit_req::{parse_limit_req, parse_limit_req_zone, LimitReqParseError};
pub use loader::{
    inspect_webserver_config_revision, load_and_compile_webserver_config,
    load_and_compile_webserver_config_json, load_and_compile_webserver_config_revision, CompiledWebServerRevision,
    WebServerConfigFileRevision, MAX_CONFIG_BYTES,
};
pub use proxy_headers::{
    format_proxy_set_header_entry, merge_proxy_set_headers, validate_proxy_set_header_entry,
    SUPPORTED_PROXY_HEADER_VARS,
};
pub use rewrite::{
    apply_rewrites, parse_rewrite, RewriteApplyError, RewriteOutcome, RewriteParseError,
    MAX_REWRITE_INTERNAL_REDIRECTS,
};
pub use server_toml::{
    load_server_toml_app, load_server_toml_file, materialize_app, merge_common_profile,
};
pub use source::{
    ConfigFormat, ConfigLoadOptions, ConfigSource, JsonConfigSource, LoadedWebServerConfig,
    NginxConfConfigSource, TomlConfigSource, WebServerConfigLoader, DEFAULT_APP_KEY,
    DEFAULT_TOML_PROFILE,
};
pub use model::{
    AccessAction, AccessRuleConfig, AcmeHttp01Config, AuthBasicConfig, AuthBasicUserConfig,
    CertificateConfig, CertificateSource, ClientAuthConfig, ClientAuthMode, GzipConfig,
    LimitReqConfig, LimitReqZoneConfig, NginxConfig, ConfigProviderType,
    CustomHeaderConfig, DeploymentConfig, ListenerConfig, ListenerProtocol, ListenerTlsRuntime,
    ObservabilityConfig, ProviderCachePolicy, ProxyCacheConfig, ProxyProtocolConfig, ProxyProtocolCrc32cPolicy,
    ProxyProtocolVersion, ReloadConfig, ReloadMode, ResolverConfig, ResourceConfig,
    ResourcePressureConfig, ResourceSampleFailurePolicy, RewriteFlag, RewriteRuleConfig,
    RouteConfig, RouteMatchConfig, RoutePathType, SecurityHeadersConfig, StreamServerConfig,
    StreamTargetConfig, StreamTlsMode, StrictTransportSecurityConfig, TlsPolicyConfig, TlsVersion,
    TrustedProxyConfig, TrustedProxyHeader, UpstreamActiveHealthConfig, UpstreamActiveHealthMethod,
    UpstreamAddressPolicyConfig, UpstreamConfig, UpstreamHashConfig, UpstreamHashKeyVar,
    UpstreamLoadBalancingStrategy,
    UpstreamPassiveHealthConfig, UpstreamRetryCondition, UpstreamRetryConfig,
    UpstreamTargetConfig, UpstreamTlsConfig, UpstreamTlsTrustMode, VirtualHostConfig,
    WebServerAppConfig, WebServerLimits, XFrameOptions,
};
pub use network::{is_supported_upstream_allowed_cidr, upstream_ip_is_allowed};
pub use uri::{normalize_uri_path, UriPathNormalizationError};
pub use validate::{normalize_server_name, server_name_covers};

pub(crate) use validate::validate_webserver_config;
