mod access;
mod acme_webroot;
mod auth_basic;
pub mod cache_policy;
mod compiled;
mod discovery;
mod error;
pub mod geo;
mod gzip_disable;
mod limit_conn;
mod limit_req;
mod loader;
pub mod map_variables;
mod model;
mod network;
mod proxy_headers;
mod proxy_pass;
mod referer;
mod rewrite;
mod secure_link;
mod server_name;
mod server_toml;
mod source;
pub mod stream_variables;
mod sub_filter;
mod upstream_params;
mod uri;
mod validate;
mod variables;

pub use access::{evaluate_access, evaluate_access_first_match_allow, AccessDecision};
pub use acme_webroot::{AcmeWebrootReport, AcmeWebrootVerdict};
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
pub use geo::{
    evaluate_geo, evaluate_split_clients, murmur_hash2, parse_split_percent, validate_geo_config,
    validate_split_clients_config,
};
pub use gzip_disable::{
    gzip_disabled_for_user_agent, user_agent_is_msie6, user_agent_matches_disable_patterns,
};
pub use limit_conn::{parse_limit_conn, parse_limit_conn_zone, LimitConnParseError};
pub use limit_req::{parse_limit_req, parse_limit_req_zone, LimitReqParseError};
pub use loader::{
    inspect_webserver_config_revision, load_and_compile_webserver_config,
    load_and_compile_webserver_config_json, load_and_compile_webserver_config_revision,
    CompiledWebServerRevision, WebServerConfigFileRevision, MAX_CONFIG_BYTES,
};
pub use map_variables::{
    evaluate_map, expand_map_template, is_supported_map_variable, map_result_name,
    resolve_map_value, validate_map_definitions, validate_map_source, validate_map_value_template,
    MapVariables, MAP_SOURCE_PREFIXES, MAP_SOURCE_VARIABLES,
};
pub use model::{
    AccessAction, AccessRuleConfig, AcmeHttp01Config, AdditionConfig, AppDomainFallbackConfig,
    AppDomainFallbackLookup, AuthBasicConfig, AuthBasicUserConfig, AuthRequestConfig,
    AuthRequestSetConfig, AutoindexConfig, AutoindexFormat, CachePolicyConfig, CertificateConfig,
    CertificateSource, CgiCacheConfig, CgiCacheValidRule, CgiUpstreamProtocol, CharsetConfig,
    ClientAuthConfig, ClientAuthMode, ConfigProviderType, CookieRewritePolicyConfig,
    CustomHeaderConfig, DeploymentConfig, ErrorPageConfig, ExpiresMode, GeoConfig, GeoEntryConfig,
    GzipConfig, GzipProxiedConfig, GzipStaticMode, IfConditionConfig, IfModifiedSinceMode,
    IfOperatorConfig, LimitConnConfig, LimitConnZoneConfig, LimitReqConfig, LimitReqZoneConfig,
    ListenerConfig, ListenerProtocol, ListenerTlsRuntime, MailAuthHttpConfig, MailAuthMechanism,
    MailConfig, MailProxyConfig, MailProxyTlsConfig, MailServerConfig, MailServerProtocol,
    MailStarttlsMode, MailTlsConfig, MailTlsProtocol, MapConfig, MapEntryConfig, MimeTypeConfig,
    MirrorConfig, NginxConfig, ObservabilityConfig, ProviderCachePolicy, ProxyCacheConfig,
    ProxyProtocolConfig, ProxyProtocolCrc32cPolicy, ProxyProtocolVersion,
    ProxyRedirectPolicyConfig, ProxyResponsePolicyConfig, ProxyRewriteRegexRuleConfig,
    ProxyRewriteRuleConfig, RedirectPolicyConfig, RefererConfig, RefererHostEntry, ReloadConfig,
    ReloadMode, ResolverConfig, ResourceConfig, ResourcePressureConfig,
    ResourceSampleFailurePolicy, RewriteFlag, RewritePhaseOpConfig, RewriteRuleConfig, RouteConfig,
    RouteMatchConfig, RoutePathType, SecureLinkMode, SecurityHeadersConfig, SplitClientsConfig,
    SplitClientsPartConfig, StreamProtocol, StreamServerConfig, StreamSetVariableConfig,
    StreamTargetConfig, StreamTlsMode, StrictTransportSecurityConfig, SubFilterConfig,
    SubFilterRule, TlsCertificateResolution, TlsPolicyConfig, TlsVersion, TrustedProxyConfig,
    TrustedProxyHeader, UpstreamActiveHealthConfig, UpstreamActiveHealthMethod,
    UpstreamAddressPolicyConfig, UpstreamBufferingConfig, UpstreamConfig, UpstreamHashConfig,
    UpstreamHashKeyVar, UpstreamLoadBalancingStrategy, UpstreamParamConfig,
    UpstreamPassiveHealthConfig, UpstreamRetryCondition, UpstreamRetryConfig, UpstreamStoreConfig,
    UpstreamTargetConfig, UpstreamTlsConfig, UpstreamTlsTrustMode, UsageMeteringChannel,
    UsageMeteringConfig, VirtualHostConfig, WebServerAppConfig, WebServerLimits, XFrameOptions,
    ACME_HTTP_01_CHALLENGE_PREFIX, DEFAULT_MAX_RANGES, MAX_REWRITE_PHASE_OPS,
    MAX_REWRITE_PHASE_VARIABLES,
};
pub use network::{
    hostname_upstream_allowed_cidrs, is_supported_upstream_allowed_cidr, upstream_ip_is_allowed,
};
pub use proxy_headers::{
    format_proxy_set_header_entry, format_proxy_set_header_entry_with_maps,
    merge_proxy_set_headers, validate_proxy_set_header_entry,
    validate_proxy_set_header_entry_with_maps, SUPPORTED_PROXY_HEADER_VARS,
};
pub use proxy_pass::{
    expand_proxy_pass_template, validate_proxy_pass_template,
    validate_proxy_pass_template_with_maps,
};
pub use referer::{compile_referer_regex, invalid_referer};
pub use rewrite::{
    apply_rewrites, apply_rewrites_with, parse_rewrite, RewriteApplyError, RewriteOutcome,
    RewriteParseError, MAX_REWRITE_INTERNAL_REDIRECTS,
};
pub use sdkwork_webserver_resolver_cache::{RedisCacheConfig, ResolutionCacheConfig};
pub use secure_link::{
    md5_hex, validate_md5_template, verify_md5_link, verify_secret_link, verify_secure_link,
    SecureLinkFailure,
};
pub use server_name::{
    normalize_virtual_host_server_name, parse_server_name, server_name_is_regex, ServerNameClaim,
    ServerNameForm, ServerNameRegex,
};
pub use server_toml::{
    accepted_unenforced_toml_diagnostics, load_server_toml_app, load_server_toml_app_effective,
    load_server_toml_app_with_compatibility, load_server_toml_file,
    load_server_toml_file_with_compatibility, materialize_app, merge_common_profile,
    merge_effective, merge_overlay,
};
pub use source::{
    ConfigFormat, ConfigLoadOptions, ConfigSource, JsonConfigSource, LoadedWebServerConfig,
    NginxConfConfigSource, TomlConfigSource, WebServerConfigLoader, DEFAULT_APP_KEY,
    DEFAULT_TOML_PROFILE,
};
pub use stream_variables::{
    evaluate_stream_sets, expand_stream_template, stream_template_variables,
    validate_stream_return, validate_stream_set, StreamVariableFacts, STREAM_MAP_SOURCES,
    STREAM_TEMPLATE_VARIABLES,
};
pub use sub_filter::{
    apply_sub_filters, sub_filter_content_type_matches, MAX_SUB_FILTER_BODY_BYTES,
};
pub use upstream_params::{
    validate_param_name, validate_param_value, UPSTREAM_PARAM_PREFIXES, UPSTREAM_PARAM_VARIABLES,
};
pub use uri::{normalize_uri_path, normalize_uri_path_with_options, UriPathNormalizationError};
pub use validate::{
    normalize_server_name, normalize_tls_server_name, server_name_covers, unix_path_is_confined,
    unix_target_path, wildcard_server_name_covers,
};
pub use variables::{
    expand_template, is_capture_variable, template_variables, TemplateVariable, MAX_TEMPLATE_BYTES,
};

pub(crate) use validate::validate_webserver_config;

/// Whether `name` is an RFC 9110 header-field token (used by configuration
/// validators that accept upstream header policies).
pub(crate) fn header_block_token(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}
