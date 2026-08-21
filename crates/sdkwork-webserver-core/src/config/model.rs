use std::collections::BTreeMap;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn default_profile() -> String {
    "http-core-v1".to_owned()
}

fn default_true() -> bool {
    true
}

fn default_proxy_pass_request_headers() -> bool {
    true
}

fn default_unknown_directive_policy() -> String {
    "error".to_owned()
}

fn default_gzip_min_length() -> u64 {
    // nginx default for gzip_min_length
    20
}

fn default_provider_timeout_ms() -> u64 {
    30_000
}

fn default_max_request_body_bytes() -> u64 {
    10 * 1024 * 1024
}

/// Hard ceiling on a proxied upstream response body. Streaming is preserved,
/// but an upstream that pushes beyond the configured budget fails as a
/// bounded 502 target failure instead of occupying connections indefinitely.
fn default_max_response_body_bytes() -> u64 {
    256 * 1024 * 1024
}

fn default_request_timeout_ms() -> u64 {
    30_000
}

fn default_request_body_start_timeout_ms() -> u64 {
    30_000
}

fn default_request_body_idle_timeout_ms() -> u64 {
    30_000
}

fn default_response_body_idle_timeout_ms() -> u64 {
    30_000
}

fn default_connection_write_timeout_ms() -> u64 {
    30_000
}

fn default_http1_keep_alive_idle_timeout_ms() -> u64 {
    75_000
}

fn default_http1_max_pipeline_depth() -> usize {
    16
}

fn default_drain_timeout_ms() -> u64 {
    30_000
}

fn default_max_connections() -> usize {
    10_000
}

fn default_max_concurrent_requests() -> usize {
    4_096
}

fn default_max_request_header_bytes() -> usize {
    64 * 1024
}

fn default_max_request_line_bytes() -> usize {
    8 * 1024
}

fn default_max_request_method_bytes() -> usize {
    32
}

fn default_max_request_target_bytes() -> usize {
    8 * 1024
}

fn default_max_uri_path_bytes() -> usize {
    8 * 1024
}

fn default_max_decoded_path_bytes() -> usize {
    8 * 1024
}

fn default_max_path_segments() -> usize {
    256
}

fn default_max_query_string_bytes() -> usize {
    4 * 1024
}

fn default_max_query_parameters() -> usize {
    256
}

fn default_max_query_component_bytes() -> usize {
    1024
}

fn default_max_header_name_bytes() -> usize {
    256
}

fn default_max_header_value_bytes() -> usize {
    8 * 1024
}

fn default_max_request_headers() -> usize {
    100
}

fn default_request_header_timeout_ms() -> u64 {
    10_000
}

fn default_max_chunk_line_bytes() -> usize {
    1_024
}

fn default_max_trailer_bytes() -> usize {
    8 * 1024
}

fn default_max_trailers() -> usize {
    32
}

fn default_http2_max_concurrent_streams() -> u32 {
    100
}

fn default_max_connection_age_ms() -> u64 {
    3_600_000
}

fn default_http2_keep_alive_interval_ms() -> u64 {
    60_000
}

fn default_http2_keep_alive_timeout_ms() -> u64 {
    20_000
}

fn default_http2_max_pending_accept_reset_streams() -> usize {
    20
}

fn default_http2_max_local_error_reset_streams() -> usize {
    128
}

fn default_http2_max_send_buffer_bytes() -> usize {
    64 * 1024
}

fn default_http2_max_header_list_bytes() -> u32 {
    64 * 1024
}

fn default_http2_max_frame_bytes() -> u32 {
    16 * 1024
}

fn default_http2_abuse_window_ms() -> u64 {
    1_000
}

fn default_http2_max_frames_per_window() -> usize {
    10_000
}

fn default_http2_max_new_streams_per_window() -> usize {
    1_000
}

fn default_http2_max_reset_frames_per_window() -> usize {
    100
}

fn default_http2_max_continuation_frames() -> usize {
    16
}

fn default_http2_max_encoded_header_block_bytes() -> usize {
    64 * 1024
}

fn default_trusted_proxy_max_hops() -> usize {
    16
}

fn default_trusted_proxy_max_header_bytes() -> usize {
    4 * 1024
}

fn default_proxy_protocol_versions() -> Vec<ProxyProtocolVersion> {
    vec![ProxyProtocolVersion::V1, ProxyProtocolVersion::V2]
}

fn default_proxy_protocol_timeout_ms() -> u64 {
    3_000
}

fn default_proxy_protocol_max_header_bytes() -> usize {
    536
}

fn default_proxy_protocol_crc32c_policy() -> ProxyProtocolCrc32cPolicy {
    ProxyProtocolCrc32cPolicy::Ignore
}

fn default_index_files() -> Vec<String> {
    vec!["index.html".to_owned()]
}

fn default_content_type() -> String {
    "text/plain; charset=utf-8".to_owned()
}

fn default_connect_timeout_ms() -> u64 {
    5_000
}

fn default_max_idle_connections() -> usize {
    128
}

fn default_upstream_max_connections() -> usize {
    256
}

fn default_upstream_max_response_header_bytes() -> usize {
    64 * 1024
}

fn default_upstream_max_response_headers() -> usize {
    100
}

fn default_weight() -> u16 {
    1
}

fn default_resolver_timeout_ms() -> u64 {
    2_000
}

fn default_maximum_answers() -> usize {
    16
}

fn default_max_concurrent_queries() -> usize {
    64
}

fn default_idle_connection_timeout_ms() -> u64 {
    30_000
}

fn default_upstream_max_in_flight_requests() -> usize {
    1_024
}

fn default_upstream_retry_max_attempts() -> u8 {
    2
}

fn default_upstream_retry_timeout_ms() -> u64 {
    30_000
}

fn default_upstream_retry_on() -> Vec<UpstreamRetryCondition> {
    vec![
        UpstreamRetryCondition::TransportFailure,
        UpstreamRetryCondition::Timeout,
    ]
}

fn default_max_concurrent_health_checks() -> usize {
    64
}

fn default_passive_failure_threshold() -> u32 {
    3
}

fn default_passive_ejection_time_ms() -> u64 {
    30_000
}

fn default_passive_failure_statuses() -> Vec<u16> {
    vec![502, 503, 504]
}

fn default_active_health_uri() -> String {
    "/".to_owned()
}

fn default_active_health_interval_ms() -> u64 {
    10_000
}

fn default_active_health_timeout_ms() -> u64 {
    2_000
}

fn default_active_unhealthy_threshold() -> u32 {
    3
}

fn default_active_healthy_threshold() -> u32 {
    2
}

fn default_active_success_status_min() -> u16 {
    200
}

fn default_active_success_status_max() -> u16 {
    399
}

fn default_active_max_response_body_bytes() -> u64 {
    65_536
}

fn default_access_log() -> bool {
    true
}

fn default_reload_poll_interval_ms() -> u64 {
    1_000
}

fn default_resource_sample_interval_ms() -> u64 {
    250
}

fn default_maximum_process_memory_bytes() -> u64 {
    1_073_741_824
}

fn default_memory_reserve_bytes() -> u64 {
    67_108_864
}

fn default_memory_admission_percent() -> u8 {
    90
}

fn default_memory_recovery_percent() -> u8 {
    80
}

fn default_maximum_open_handles() -> u64 {
    16_384
}

fn default_open_handle_reserve() -> u64 {
    128
}

fn default_open_handle_admission_percent() -> u8 {
    90
}

fn default_open_handle_recovery_percent() -> u8 {
    80
}

fn default_event_loop_lag_admission_ms() -> u64 {
    250
}

fn default_event_loop_lag_recovery_ms() -> u64 {
    50
}

fn default_consecutive_pressure_samples() -> u32 {
    2
}

fn default_consecutive_recovery_samples() -> u32 {
    4
}

fn default_operations_reserve_requests() -> usize {
    16
}

fn default_tls_minimum() -> TlsVersion {
    TlsVersion::Tls12
}

fn default_tls_maximum() -> TlsVersion {
    TlsVersion::Tls13
}

fn default_alpn() -> Vec<String> {
    vec!["h2".to_owned(), "http/1.1".to_owned()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebServerAppConfig {
    pub schema_version: u32,
    pub kind: String,
    pub app_key: String,
    #[serde(default)]
    pub nginx: NginxConfig,
    /// Response gzip settings materialized from `[http] gzip` / `gzipTypes` /
    /// `gzipMinLength` (SDKWORK_WEBSERVER_SPEC.md §6).
    #[serde(default)]
    pub gzip: GzipConfig,
    /// Shared `limit_req_zone` definitions materialized from `[http] limitReqZone`.
    #[serde(default)]
    pub limit_req_zones: Vec<LimitReqZoneConfig>,
    /// Shared `limit_conn_zone` definitions materialized from
    /// `[http] limitConnZone` (nginx `limit_conn_zone`).
    #[serde(default)]
    pub limit_conn_zones: Vec<LimitConnZoneConfig>,
    #[serde(default)]
    pub limits: WebServerLimits,
    pub listeners: Vec<ListenerConfig>,
    #[serde(default)]
    pub certificates: Vec<CertificateConfig>,
    #[serde(default)]
    pub tls_policies: Vec<TlsPolicyConfig>,
    #[serde(default)]
    pub resolvers: Vec<ResolverConfig>,
    pub resources: Vec<ResourceConfig>,
    #[serde(default)]
    pub upstreams: Vec<UpstreamConfig>,
    pub virtual_hosts: Vec<VirtualHostConfig>,
    /// Raw TCP proxying (`nginx stream` equivalent, SDKWORK_WEBSERVER_SPEC
    /// section 12): one listener per entry that forwards bytes to a literal
    /// `host:port` or a declared upstream.
    #[serde(default)]
    pub streams: Vec<StreamServerConfig>,
    /// Shared HTTP response cache for proxied surfaces.
    #[serde(default)]
    pub proxy_cache: ProxyCacheConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub deployment: DeploymentConfig,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

/// nginx `limit_req_zone` shared zone (SDKWORK_WEBSERVER_SPEC.md §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LimitReqZoneConfig {
    pub name: String,
    /// Only `$binary_remote_addr` / `$remote_addr` are executable today.
    pub key: String,
    /// Approximate key capacity derived from the zone size argument.
    pub max_keys: u32,
    /// Sustained rate as requests per second (e.g. `10r/m` → `10/60`).
    pub rate_per_second: f64,
}

/// One `limit_req` directive on a location (SDKWORK_WEBSERVER_SPEC.md §11.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LimitReqConfig {
    pub zone: String,
    #[serde(default)]
    pub burst: u32,
    #[serde(default)]
    pub nodelay: bool,
}

/// nginx `limit_conn_zone` shared zone: per-key connection budget with an
/// approximate tracked-key capacity derived from the zone size argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LimitConnZoneConfig {
    pub name: String,
    /// Only `$binary_remote_addr` / `$remote_addr` are executable today.
    pub key: String,
    pub max_keys: u32,
}

/// One `limit_conn` directive on a location (nginx `limit_conn <zone> <n>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LimitConnConfig {
    pub zone: String,
    /// Concurrent connections permitted per key.
    pub max_connections: u32,
}

/// Ordered access-module rule (`allow` / `deny`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessRuleConfig {
    pub action: AccessAction,
    /// `"all"` or an IP / CIDR string.
    pub network: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessAction {
    Allow,
    Deny,
}

/// One htpasswd user entry materialized from `auth_basic_user_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthBasicUserConfig {
    pub username: String,
    pub password_hash: String,
}

/// Location `auth_basic` challenge after the user file is loaded at materialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthBasicConfig {
    pub realm: String,
    pub users: Vec<AuthBasicUserConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NginxConfig {
    /// Deploy-TOML `nginx.enabled` master switch; default true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default = "default_unknown_directive_policy")]
    pub unknown_directive_policy: String,
}

/// HTTP response gzip policy (`gzip`, `gzip_types`, `gzip_min_length`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GzipConfig {
    #[serde(default)]
    pub enabled: bool,
    /// MIME types eligible for gzip. `text/html` is always included when
    /// `enabled` (nginx semantics).
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default = "default_gzip_min_length")]
    pub min_length: u64,
}

impl Default for GzipConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            types: Vec::new(),
            min_length: default_gzip_min_length(),
        }
    }
}

impl Default for NginxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            profile: default_profile(),
            unknown_directive_policy: default_unknown_directive_policy(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebServerLimits {
    #[serde(default = "default_provider_timeout_ms")]
    pub provider_timeout_ms: u64,
    #[serde(default = "default_max_request_body_bytes")]
    pub max_request_body_bytes: u64,
    #[serde(default = "default_max_response_body_bytes")]
    pub max_response_body_bytes: u64,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_request_body_start_timeout_ms")]
    pub request_body_start_timeout_ms: u64,
    #[serde(default = "default_request_body_idle_timeout_ms")]
    pub request_body_idle_timeout_ms: u64,
    #[serde(default = "default_response_body_idle_timeout_ms")]
    pub response_body_idle_timeout_ms: u64,
    #[serde(default = "default_connection_write_timeout_ms")]
    pub connection_write_timeout_ms: u64,
    #[serde(default = "default_http1_keep_alive_idle_timeout_ms")]
    pub http1_keep_alive_idle_timeout_ms: u64,
    #[serde(default = "default_http1_max_pipeline_depth")]
    pub http1_max_pipeline_depth: usize,
    #[serde(default = "default_drain_timeout_ms")]
    pub drain_timeout_ms: u64,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_max_concurrent_requests")]
    pub max_concurrent_requests: usize,
    #[serde(default = "default_max_concurrent_health_checks")]
    pub max_concurrent_health_checks: usize,
    #[serde(default = "default_max_request_header_bytes")]
    pub max_request_header_bytes: usize,
    #[serde(default = "default_max_request_line_bytes")]
    pub max_request_line_bytes: usize,
    #[serde(default = "default_max_request_method_bytes")]
    pub max_request_method_bytes: usize,
    #[serde(default = "default_max_request_target_bytes")]
    pub max_request_target_bytes: usize,
    #[serde(default = "default_max_uri_path_bytes")]
    pub max_uri_path_bytes: usize,
    #[serde(default = "default_max_decoded_path_bytes")]
    pub max_decoded_path_bytes: usize,
    #[serde(default = "default_max_path_segments")]
    pub max_path_segments: usize,
    #[serde(default = "default_max_query_string_bytes")]
    pub max_query_string_bytes: usize,
    #[serde(default = "default_max_query_parameters")]
    pub max_query_parameters: usize,
    #[serde(default = "default_max_query_component_bytes")]
    pub max_query_component_bytes: usize,
    #[serde(default = "default_max_header_name_bytes")]
    pub max_header_name_bytes: usize,
    #[serde(default = "default_max_header_value_bytes")]
    pub max_header_value_bytes: usize,
    #[serde(default = "default_max_request_headers")]
    pub max_request_headers: usize,
    #[serde(default = "default_request_header_timeout_ms")]
    pub request_header_timeout_ms: u64,
    #[serde(default = "default_max_chunk_line_bytes")]
    pub max_chunk_line_bytes: usize,
    #[serde(default = "default_max_trailer_bytes")]
    pub max_trailer_bytes: usize,
    #[serde(default = "default_max_trailers")]
    pub max_trailers: usize,
    #[serde(default = "default_http2_max_concurrent_streams")]
    pub http2_max_concurrent_streams: u32,
    #[serde(default = "default_max_connection_age_ms")]
    pub max_connection_age_ms: u64,
    #[serde(default = "default_http2_keep_alive_interval_ms")]
    pub http2_keep_alive_interval_ms: u64,
    #[serde(default = "default_http2_keep_alive_timeout_ms")]
    pub http2_keep_alive_timeout_ms: u64,
    #[serde(default = "default_http2_max_pending_accept_reset_streams")]
    pub http2_max_pending_accept_reset_streams: usize,
    #[serde(default = "default_http2_max_local_error_reset_streams")]
    pub http2_max_local_error_reset_streams: usize,
    #[serde(default = "default_http2_max_send_buffer_bytes")]
    pub http2_max_send_buffer_bytes: usize,
    #[serde(default = "default_http2_max_header_list_bytes")]
    pub http2_max_header_list_bytes: u32,
    #[serde(default = "default_http2_max_frame_bytes")]
    pub http2_max_frame_bytes: u32,
    #[serde(default = "default_http2_abuse_window_ms")]
    pub http2_abuse_window_ms: u64,
    #[serde(default = "default_http2_max_frames_per_window")]
    pub http2_max_frames_per_window: usize,
    #[serde(default = "default_http2_max_new_streams_per_window")]
    pub http2_max_new_streams_per_window: usize,
    #[serde(default = "default_http2_max_reset_frames_per_window")]
    pub http2_max_reset_frames_per_window: usize,
    #[serde(default = "default_http2_max_continuation_frames")]
    pub http2_max_continuation_frames: usize,
    #[serde(default = "default_http2_max_encoded_header_block_bytes")]
    pub http2_max_encoded_header_block_bytes: usize,
}

impl Default for WebServerLimits {
    fn default() -> Self {
        Self {
            provider_timeout_ms: default_provider_timeout_ms(),
            max_request_body_bytes: default_max_request_body_bytes(),
            max_response_body_bytes: default_max_response_body_bytes(),
            request_timeout_ms: default_request_timeout_ms(),
            request_body_start_timeout_ms: default_request_body_start_timeout_ms(),
            request_body_idle_timeout_ms: default_request_body_idle_timeout_ms(),
            response_body_idle_timeout_ms: default_response_body_idle_timeout_ms(),
            connection_write_timeout_ms: default_connection_write_timeout_ms(),
            http1_keep_alive_idle_timeout_ms: default_http1_keep_alive_idle_timeout_ms(),
            http1_max_pipeline_depth: default_http1_max_pipeline_depth(),
            drain_timeout_ms: default_drain_timeout_ms(),
            max_connections: default_max_connections(),
            max_concurrent_requests: default_max_concurrent_requests(),
            max_concurrent_health_checks: default_max_concurrent_health_checks(),
            max_request_header_bytes: default_max_request_header_bytes(),
            max_request_line_bytes: default_max_request_line_bytes(),
            max_request_method_bytes: default_max_request_method_bytes(),
            max_request_target_bytes: default_max_request_target_bytes(),
            max_uri_path_bytes: default_max_uri_path_bytes(),
            max_decoded_path_bytes: default_max_decoded_path_bytes(),
            max_path_segments: default_max_path_segments(),
            max_query_string_bytes: default_max_query_string_bytes(),
            max_query_parameters: default_max_query_parameters(),
            max_query_component_bytes: default_max_query_component_bytes(),
            max_header_name_bytes: default_max_header_name_bytes(),
            max_header_value_bytes: default_max_header_value_bytes(),
            max_request_headers: default_max_request_headers(),
            request_header_timeout_ms: default_request_header_timeout_ms(),
            max_chunk_line_bytes: default_max_chunk_line_bytes(),
            max_trailer_bytes: default_max_trailer_bytes(),
            max_trailers: default_max_trailers(),
            http2_max_concurrent_streams: default_http2_max_concurrent_streams(),
            max_connection_age_ms: default_max_connection_age_ms(),
            http2_keep_alive_interval_ms: default_http2_keep_alive_interval_ms(),
            http2_keep_alive_timeout_ms: default_http2_keep_alive_timeout_ms(),
            http2_max_pending_accept_reset_streams: default_http2_max_pending_accept_reset_streams(
            ),
            http2_max_local_error_reset_streams: default_http2_max_local_error_reset_streams(),
            http2_max_send_buffer_bytes: default_http2_max_send_buffer_bytes(),
            http2_max_header_list_bytes: default_http2_max_header_list_bytes(),
            http2_max_frame_bytes: default_http2_max_frame_bytes(),
            http2_abuse_window_ms: default_http2_abuse_window_ms(),
            http2_max_frames_per_window: default_http2_max_frames_per_window(),
            http2_max_new_streams_per_window: default_http2_max_new_streams_per_window(),
            http2_max_reset_frames_per_window: default_http2_max_reset_frames_per_window(),
            http2_max_continuation_frames: default_http2_max_continuation_frames(),
            http2_max_encoded_header_block_bytes: default_http2_max_encoded_header_block_bytes(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListenerConfig {
    pub id: String,
    pub bind: String,
    pub port: u16,
    pub protocols: Vec<ListenerProtocol>,
    pub tls_policy_ref: Option<String>,
    pub tls_runtime: Option<ListenerTlsRuntime>,
    pub default_virtual_host_ref: Option<String>,
    pub max_connections: Option<usize>,
    pub trusted_proxy: Option<TrustedProxyConfig>,
    pub proxy_protocol: Option<ProxyProtocolConfig>,
    pub acme_http_01: Option<AcmeHttp01Config>,
    #[serde(default)]
    pub allow_plaintext_http: bool,
}

/// Narrow-precedence ACME HTTP-01 challenge serving for a listener.
///
/// When configured, the listener serves only the exact
/// `/.well-known/acme-challenge/<token>` path from a single bounded regular
/// file under `webroot`; no directory listing or unrelated route is exposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcmeHttp01Config {
    pub webroot: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListenerProtocol {
    Http1,
    Http2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListenerTlsRuntime {
    Assignment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustedProxyConfig {
    pub trusted_cidrs: Vec<IpNet>,
    #[serde(default)]
    pub header: TrustedProxyHeader,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default = "default_trusted_proxy_max_hops")]
    pub max_hops: usize,
    #[serde(default = "default_trusted_proxy_max_header_bytes")]
    pub max_header_bytes: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustedProxyHeader {
    #[default]
    XForwardedFor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyProtocolConfig {
    pub trusted_source_cidrs: Vec<IpNet>,
    #[serde(default = "default_proxy_protocol_versions")]
    pub versions: Vec<ProxyProtocolVersion>,
    #[serde(default = "default_proxy_protocol_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_proxy_protocol_max_header_bytes")]
    pub max_header_bytes: usize,
    #[serde(default = "default_proxy_protocol_crc32c_policy")]
    pub crc32c_policy: ProxyProtocolCrc32cPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyProtocolVersion {
    V1,
    V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyProtocolCrc32cPolicy {
    Ignore,
    ValidateIfPresent,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CertificateConfig {
    pub id: String,
    pub server_names: Vec<String>,
    pub source: CertificateSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CertificateSource {
    ProtectedFile {
        certificate_file: String,
        private_key_file: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsPolicyConfig {
    pub id: String,
    pub certificate_ref: Option<String>,
    #[serde(default)]
    pub certificate_refs: Vec<String>,
    #[serde(default = "default_tls_minimum")]
    pub minimum_version: TlsVersion,
    #[serde(default = "default_tls_maximum")]
    pub maximum_version: TlsVersion,
    #[serde(default = "default_alpn")]
    pub alpn: Vec<String>,
    /// Downstream client certificate policy (`ssl_verify_client`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_auth: Option<ClientAuthConfig>,
}

/// Listener/server TLS client authentication (`ssl_verify_client`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientAuthConfig {
    pub mode: ClientAuthMode,
    /// Absolute paths to PEM trust anchors (`ssl_client_certificate`).
    pub ca_certificate_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClientAuthMode {
    /// `ssl_verify_client off`
    Off,
    /// `ssl_verify_client optional`
    Optional,
    /// `ssl_verify_client on`
    Required,
}

impl TlsPolicyConfig {
    pub fn certificate_refs(&self) -> impl Iterator<Item = &str> {
        self.certificate_ref
            .iter()
            .map(String::as_str)
            .chain(self.certificate_refs.iter().map(String::as_str))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TlsVersion {
    #[serde(rename = "tls1.2")]
    Tls12,
    #[serde(rename = "tls1.3")]
    Tls13,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolverConfig {
    pub id: String,
    #[serde(default)]
    pub servers: Vec<String>,
    #[serde(default = "default_resolver_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_maximum_answers")]
    pub maximum_answers: usize,
    #[serde(default = "default_max_concurrent_queries")]
    pub max_concurrent_queries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ResourceConfig {
    Static {
        id: String,
        root: String,
        #[serde(default = "default_index_files")]
        index_files: Vec<String>,
        spa_fallback: Option<String>,
        #[serde(default)]
        follow_symlinks: bool,
    },
    Proxy {
        id: String,
        #[serde(default)]
        upstream_ref: String,
        #[serde(default)]
        strip_prefix: bool,
        /// nginx `proxy_set_header` entries as `"Name value"` strings.
        /// Supported value tokens: literals plus `$host`, `$scheme`,
        /// `$remote_addr`, `$proxy_add_x_forwarded_for`, `$http_upgrade`,
        /// `$server_port`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        request_set_headers: Vec<String>,
        /// Variable `proxy_pass` template (`http://$host` …). When present,
        /// the target URL is evaluated per request instead of using
        /// `upstream_ref` (nginx dynamic `proxy_pass`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dynamic_target: Option<String>,
        /// nginx `proxy_pass_request_headers off`: do not forward the client
        /// request headers to the upstream (fixed safe defaults still apply).
        #[serde(default = "default_proxy_pass_request_headers")]
        proxy_pass_request_headers: bool,
    },
    Redirect {
        id: String,
        status: u16,
        location: String,
    },
    Respond {
        id: String,
        status: u16,
        #[serde(default = "default_content_type")]
        content_type: String,
        #[serde(default)]
        body: String,
    },
    Drive {
        id: String,
        provider_resource_uuid: String,
        resource_subpath: Option<String>,
        #[serde(default = "default_index_files")]
        index_files: Vec<String>,
        spa_fallback: Option<String>,
        #[serde(default)]
        cache: Option<ProviderCachePolicy>,
    },
    Knowledgebase {
        id: String,
        provider_resource_uuid: String,
        locale: Option<String>,
        #[serde(default)]
        cache: Option<ProviderCachePolicy>,
    },
}

impl ResourceConfig {
    pub fn id(&self) -> &str {
        match self {
            Self::Static { id, .. }
            | Self::Proxy { id, .. }
            | Self::Redirect { id, .. }
            | Self::Respond { id, .. }
            | Self::Drive { id, .. }
            | Self::Knowledgebase { id, .. } => id,
        }
    }

    pub fn set_id(&mut self, id: String) {
        match self {
            Self::Static { id: current, .. }
            | Self::Proxy { id: current, .. }
            | Self::Redirect { id: current, .. }
            | Self::Respond { id: current, .. }
            | Self::Drive { id: current, .. }
            | Self::Knowledgebase { id: current, .. } => *current = id,
        }
    }

    pub fn set_proxy_upstream_ref(&mut self, upstream_ref: String) {
        if let Self::Proxy {
            upstream_ref: current,
            ..
        } = self
        {
            *current = upstream_ref;
        }
    }

    /// Provider-backed resource type, when this resource is served from a
    /// Drive WebsiteRoot or a Knowledgebase WikiPublication instead of the
    /// local filesystem. The config module keeps this provider vocabulary
    /// local; callers map it to the website-runtime provider registry.
    pub fn provider_type(&self) -> Option<ConfigProviderType> {
        match self {
            Self::Drive { .. } => Some(ConfigProviderType::Drive),
            Self::Knowledgebase { .. } => Some(ConfigProviderType::Knowledgebase),
            Self::Static { .. }
            | Self::Proxy { .. }
            | Self::Redirect { .. }
            | Self::Respond { .. } => None,
        }
    }

    /// Provider resource identifier (WebsiteRoot or WikiPublication uuid),
    /// when this resource is provider-backed.
    pub fn provider_resource_uuid(&self) -> Option<&str> {
        match self {
            Self::Drive {
                provider_resource_uuid,
                ..
            }
            | Self::Knowledgebase {
                provider_resource_uuid,
                ..
            } => Some(provider_resource_uuid),
            Self::Static { .. }
            | Self::Proxy { .. }
            | Self::Redirect { .. }
            | Self::Respond { .. } => None,
        }
    }

    /// Optional resolution-cache policy for provider-backed resources.
    pub fn provider_cache_policy(&self) -> Option<ProviderCachePolicy> {
        match self {
            Self::Drive { cache, .. } | Self::Knowledgebase { cache, .. } => *cache,
            Self::Static { .. }
            | Self::Proxy { .. }
            | Self::Redirect { .. }
            | Self::Respond { .. } => None,
        }
    }
}

/// Provider vocabulary for application-config resources backed by the
/// website-runtime provider registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigProviderType {
    Drive,
    Knowledgebase,
}

/// Optional resolution-cache policy for Drive and Knowledgebase resources.
///
/// Mirrors the website-runtime descriptor `deliveryPolicy` cache TTL semantics:
/// metadata resolutions are cached for `metadata_ttl_seconds`, negative
/// resolutions (not-found/revoked) for `negative_ttl_seconds`, and stale
/// entries may be served while a background revalidation runs for
/// `stale_while_revalidate_seconds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderCachePolicy {
    #[serde(default = "default_provider_metadata_cache_ttl_seconds")]
    pub metadata_ttl_seconds: u32,
    #[serde(default = "default_provider_negative_cache_ttl_seconds")]
    pub negative_ttl_seconds: u32,
    #[serde(default = "default_provider_stale_while_revalidate_seconds")]
    pub stale_while_revalidate_seconds: u32,
}

impl Default for ProviderCachePolicy {
    fn default() -> Self {
        Self {
            metadata_ttl_seconds: default_provider_metadata_cache_ttl_seconds(),
            negative_ttl_seconds: default_provider_negative_cache_ttl_seconds(),
            stale_while_revalidate_seconds: default_provider_stale_while_revalidate_seconds(),
        }
    }
}

fn default_provider_metadata_cache_ttl_seconds() -> u32 {
    30
}

fn default_provider_negative_cache_ttl_seconds() -> u32 {
    5
}

fn default_provider_stale_while_revalidate_seconds() -> u32 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamConfig {
    pub id: String,
    pub targets: Vec<UpstreamTargetConfig>,
    #[serde(default)]
    pub load_balancing: UpstreamLoadBalancingStrategy,
    /// Present when `load_balancing` is `Hash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<UpstreamHashConfig>,
    pub resolver_ref: Option<String>,
    #[serde(default)]
    pub address_policy: UpstreamAddressPolicyConfig,
    pub tls: Option<UpstreamTlsConfig>,
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_max_idle_connections")]
    pub max_idle_connections: usize,
    #[serde(default = "default_upstream_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_upstream_max_response_header_bytes")]
    pub max_response_header_bytes: usize,
    #[serde(default = "default_upstream_max_response_headers")]
    pub max_response_headers: usize,
    #[serde(default = "default_idle_connection_timeout_ms")]
    pub idle_connection_timeout_ms: u64,
    #[serde(default = "default_upstream_max_in_flight_requests")]
    pub max_in_flight_requests: usize,
    pub retry: Option<UpstreamRetryConfig>,
    #[serde(default)]
    pub passive_health: UpstreamPassiveHealthConfig,
    pub active_health: Option<UpstreamActiveHealthConfig>,
}

/// One raw TCP proxy listener (`[[stream.server]]`,
/// SDKWORK_WEBSERVER_SPEC section 12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamServerConfig {
    pub id: String,
    pub bind: String,
    pub port: u16,
    /// `tcp` (default) or `udp` (nginx `listen … udp`).
    #[serde(default)]
    pub protocol: StreamProtocol,
    pub target: StreamTargetConfig,
    /// Idle timeout on both directions of the proxied connection
    /// (`proxyTimeout`; nginx `proxy_timeout`).
    #[serde(default = "default_stream_proxy_timeout_ms")]
    pub proxy_timeout_ms: u64,
    /// Send a PROXY protocol v1 header to the upstream
    /// (`proxyProtocol`; nginx `proxy_protocol`).
    #[serde(default)]
    pub proxy_protocol: bool,
    /// Optional stream TLS mode (`listen … ssl` terminate or `sslPreread`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<StreamTlsMode>,
}

/// Stream TLS execution mode (SDKWORK_WEBSERVER_SPEC §12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "mode",
    deny_unknown_fields
)]
pub enum StreamTlsMode {
    /// Terminate TLS with a named certificate from `certificates[]`.
    Terminate {
        certificate_ref: String,
        /// Downstream client certificate verification (nginx stream
        /// `ssl_verify_client` + `ssl_client_certificate`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_auth: Option<ClientAuthConfig>,
    },
    /// Peek ClientHello then pass encrypted bytes to the upstream.
    Preread,
}

fn default_stream_proxy_timeout_ms() -> u64 {
    60 * 1000
}

/// Stream proxy destination: a declared upstream (load-balanced, resolves its
/// targets like HTTP proxy) or a literal `host:port`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, tag = "type")]
pub enum StreamTargetConfig {
    Upstream { name: String },
    Literal { host: String, port: u16 },
}

/// Stream transport protocol (nginx `listen … udp` flag).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamProtocol {
    #[default]
    Tcp,
    Udp,
}

impl StreamServerConfig {
    pub fn socket_key(&self) -> String {
        format!("{}:{}", self.bind.to_ascii_lowercase(), self.port)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpstreamLoadBalancingStrategy {
    #[default]
    RoundRobin,
    LeastConnections,
    RandomTwoLeastConnections,
    IpHash,
    /// nginx `hash <key> [consistent]`.
    Hash,
}

/// Executable hash-key variable subset for `loadBalancing = "hash"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamHashKeyVar {
    #[serde(rename = "$request_uri")]
    RequestUri,
    #[serde(rename = "$uri")]
    Uri,
    #[serde(rename = "$remote_addr")]
    RemoteAddr,
    #[serde(rename = "$host")]
    Host,
}

/// nginx `hash` / `hash … consistent` policy materialized onto an upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamHashConfig {
    pub key: UpstreamHashKeyVar,
    #[serde(default)]
    pub consistent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamRetryConfig {
    #[serde(default = "default_upstream_retry_max_attempts")]
    pub max_attempts: u8,
    #[serde(default = "default_upstream_retry_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_upstream_retry_on")]
    pub retry_on: Vec<UpstreamRetryCondition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpstreamRetryCondition {
    #[serde(rename = "error")]
    TransportFailure,
    Timeout,
    #[serde(rename = "http_502")]
    Http502,
    #[serde(rename = "http_503")]
    Http503,
    #[serde(rename = "http_504")]
    Http504,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamAddressPolicyConfig {
    #[serde(default)]
    pub allowed_cidrs: Vec<IpNet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamTlsConfig {
    #[serde(default)]
    pub trust_mode: UpstreamTlsTrustMode,
    #[serde(default)]
    pub ca_certificate_files: Vec<String>,
    pub client_certificate_file: Option<String>,
    pub client_private_key_file: Option<String>,
    #[serde(default = "default_tls_minimum")]
    pub minimum_version: TlsVersion,
    #[serde(default = "default_tls_maximum")]
    pub maximum_version: TlsVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamPassiveHealthConfig {
    #[serde(default = "default_passive_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_passive_ejection_time_ms")]
    pub ejection_time_ms: u64,
    #[serde(default = "default_passive_failure_statuses")]
    pub failure_statuses: Vec<u16>,
}

impl Default for UpstreamPassiveHealthConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_passive_failure_threshold(),
            ejection_time_ms: default_passive_ejection_time_ms(),
            failure_statuses: default_passive_failure_statuses(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamActiveHealthConfig {
    #[serde(default)]
    pub method: UpstreamActiveHealthMethod,
    #[serde(default = "default_active_health_uri")]
    pub uri: String,
    #[serde(default = "default_active_health_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_active_health_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_active_unhealthy_threshold")]
    pub unhealthy_threshold: u32,
    #[serde(default = "default_active_healthy_threshold")]
    pub healthy_threshold: u32,
    #[serde(default = "default_active_success_status_min")]
    pub success_status_min: u16,
    #[serde(default = "default_active_success_status_max")]
    pub success_status_max: u16,
    #[serde(default = "default_active_max_response_body_bytes")]
    pub max_response_body_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum UpstreamActiveHealthMethod {
    #[default]
    Get,
    Head,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpstreamTlsTrustMode {
    #[default]
    System,
    Custom,
    SystemAndCustom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamTargetConfig {
    pub url: String,
    #[serde(default = "default_weight")]
    pub weight: u16,
    #[serde(default)]
    pub backup: bool,
    pub slow_start_ms: Option<u64>,
    pub max_connections: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VirtualHostConfig {
    pub id: String,
    pub listener_refs: Vec<String>,
    pub server_names: Vec<String>,
    pub routes: Vec<RouteConfig>,
    #[serde(default)]
    pub security_headers: Option<SecurityHeadersConfig>,
}

/// Per-virtual-host security response headers, applied to every response
/// selected by that host. `x_content_type_options` defaults to `true`
/// (`nosniff`); HSTS is emitted only over HTTPS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecurityHeadersConfig {
    #[serde(default)]
    pub strict_transport_security: Option<StrictTransportSecurityConfig>,
    pub x_frame_options: Option<XFrameOptions>,
    pub content_security_policy: Option<String>,
    pub referrer_policy: Option<String>,
    #[serde(default = "default_x_content_type_options")]
    pub x_content_type_options: bool,
    #[serde(default)]
    pub custom_headers: Vec<CustomHeaderConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrictTransportSecurityConfig {
    #[serde(default = "default_hsts_max_age_seconds")]
    pub max_age_seconds: u32,
    #[serde(default = "default_true")]
    pub include_sub_domains: bool,
    #[serde(default)]
    pub preload: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum XFrameOptions {
    #[serde(rename = "DENY")]
    Deny,
    #[serde(rename = "SAMEORIGIN")]
    SameOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomHeaderConfig {
    pub name: String,
    pub value: String,
}

fn default_x_content_type_options() -> bool {
    true
}

fn default_hsts_max_age_seconds() -> u32 {
    31_536_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteConfig {
    pub id: String,
    #[serde(rename = "match")]
    pub route_match: RouteMatchConfig,
    pub resource_ref: String,
    /// Location `allow` then `deny` entries in that order (common
    /// `allow …; deny all;` pattern). Empty means no access-module check.
    #[serde(default)]
    pub access: Vec<AccessRuleConfig>,
    /// Location `limitReq` entries referencing `limit_req_zones`.
    #[serde(default)]
    pub limit_req: Vec<LimitReqConfig>,
    /// Location `limitConn` entries referencing `limit_conn_zones`.
    #[serde(default)]
    pub limit_conn: Vec<LimitConnConfig>,
    /// Location `rewrite` directives (ordered; see `RewriteFlag`).
    #[serde(default)]
    pub rewrite: Vec<RewriteRuleConfig>,
    /// Location `auth_basic` + loaded `auth_basic_user_file` entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_basic: Option<AuthBasicConfig>,
    /// Location `sub_filter` family (nginx response body substitution).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_filter: Option<SubFilterConfig>,
    /// Location `secure_link` family (nginx http secure link module).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure_link: Option<SecureLinkMode>,
}

/// nginx `secure_link` module modes (ngx_http_secure_link_module).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields,
    tag = "mode"
)]
pub enum SecureLinkMode {
    /// `secure_link_secret <word>`: URIs are `/prefix/<md5(secret+rest)>/<rest>`;
    /// on success the data plane serves `/prefix/<rest>`.
    Secret {
        /// The secret word appended to the URI in `md5(secret + rest)`.
        secret: String,
    },
    /// `secure_link $arg_<argument>` + `secure_link_md5 "<template>"` with
    /// optional `secure_link_expires $arg_<expiresArgument>`.
    Md5 {
        /// Query argument carrying the MD5 digest (`secure_link $arg_…`).
        argument: String,
        /// MD5 template; supported variables: `$uri`, `$remote_addr`,
        /// `$secure_link_expires`.
        template: String,
        /// Optional query argument with the link expiry unix timestamp.
        expires_argument: Option<String>,
    },
}



/// Location response body substitution (`sub_filter` family). The rules
/// apply in declaration order; `once` (nginx `sub_filter_once`, default on)
/// replaces only the first occurrence of each rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubFilterConfig {
    #[serde(default)]
    pub rules: Vec<SubFilterRule>,
    #[serde(default = "default_sub_filter_once")]
    pub once: bool,
    /// MIME types eligible for substitution (nginx `sub_filter_types`;
    /// default `text/html`). The comparison ignores parameters such as
    /// `; charset=utf-8` and is case-insensitive.
    #[serde(default = "default_sub_filter_types")]
    pub types: Vec<String>,
    /// nginx `sub_filter_last_modified`; when `false` (default) the
    /// `Last-Modified` header is dropped from substituted responses.
    #[serde(default)]
    pub last_modified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubFilterRule {
    pub from: String,
    pub to: String,
}

fn default_sub_filter_once() -> bool {
    true
}

fn default_sub_filter_types() -> Vec<String> {
    vec!["text/html".to_owned()]
}

/// One nginx `rewrite` directive on a location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RewriteRuleConfig {
    pub pattern: String,
    pub replacement: String,
    pub flag: RewriteFlag,
}

/// nginx rewrite flag subset executed by the Rust data plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RewriteFlag {
    /// Re-run location selection with the rewritten URI.
    Last,
    /// Stop rewrite processing; continue with the current location.
    Break,
    /// External 302 redirect.
    Redirect,
    /// External 301 redirect.
    Permanent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteMatchConfig {
    pub path_type: RoutePathType,
    pub path: String,
    pub methods: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutePathType {
    Exact,
    Prefix,
    /// nginx `^~` — longest exclusive prefix wins and suppresses regex locations.
    PrefixExclusive,
    /// nginx `~` (case-sensitive regex).
    Regex,
    /// nginx `~*` (case-insensitive regex).
    RegexIgnoreCase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservabilityConfig {
    #[serde(default = "default_access_log")]
    pub access_log: bool,
}

/// Shared HTTP response cache policy for proxied surfaces (nginx
/// `proxy_cache` / `proxy_cache_path` equivalent). Memory is always the L1
/// index; an optional `diskPath` enables durable object spill (component
/// boundary: `CacheBackend`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Maximum cached object count (LRU eviction).
    #[serde(default = "default_proxy_cache_max_entries")]
    pub max_entries: usize,
    /// Maximum cached response body bytes per entry.
    #[serde(default = "default_proxy_cache_max_object_bytes")]
    pub max_object_bytes: u64,
    /// Freshness used when the response declares no Cache-Control/Expires.
    #[serde(default = "default_proxy_cache_ttl_seconds")]
    pub default_ttl_seconds: u64,
    /// How long a stale entry may be served on upstream 5xx
    /// (`proxy_cache_use_stale` window). Zero disables stale serving.
    #[serde(default = "default_proxy_cache_stale_ttl_seconds")]
    pub stale_ttl_seconds: u64,
    /// Optional on-disk cache directory (`proxy_cache_path`). When set, the
    /// memory store remains the hot index and object bodies spill to disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_path: Option<String>,
}

impl Default for ProxyCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_entries: default_proxy_cache_max_entries(),
            max_object_bytes: default_proxy_cache_max_object_bytes(),
            default_ttl_seconds: default_proxy_cache_ttl_seconds(),
            stale_ttl_seconds: default_proxy_cache_stale_ttl_seconds(),
            disk_path: None,
        }
    }
}

fn default_proxy_cache_max_entries() -> usize {
    4_096
}

fn default_proxy_cache_max_object_bytes() -> u64 {
    1 * 1024 * 1024
}

fn default_proxy_cache_ttl_seconds() -> u64 {
    60
}

fn default_proxy_cache_stale_ttl_seconds() -> u64 {
    60
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            access_log: default_access_log(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentConfig {
    pub drain_timeout_ms: Option<u64>,
    #[serde(default)]
    pub reload: ReloadConfig,
    pub resource_pressure: Option<ResourcePressureConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePressureConfig {
    #[serde(default = "default_resource_sample_interval_ms")]
    pub sample_interval_ms: u64,
    #[serde(default = "default_maximum_process_memory_bytes")]
    pub maximum_process_memory_bytes: u64,
    #[serde(default = "default_memory_reserve_bytes")]
    pub memory_reserve_bytes: u64,
    #[serde(default = "default_memory_admission_percent")]
    pub memory_admission_percent: u8,
    #[serde(default = "default_memory_recovery_percent")]
    pub memory_recovery_percent: u8,
    #[serde(default = "default_maximum_open_handles")]
    pub maximum_open_handles: u64,
    #[serde(default = "default_open_handle_reserve")]
    pub open_handle_reserve: u64,
    #[serde(default = "default_open_handle_admission_percent")]
    pub open_handle_admission_percent: u8,
    #[serde(default = "default_open_handle_recovery_percent")]
    pub open_handle_recovery_percent: u8,
    #[serde(default = "default_event_loop_lag_admission_ms")]
    pub event_loop_lag_admission_ms: u64,
    #[serde(default = "default_event_loop_lag_recovery_ms")]
    pub event_loop_lag_recovery_ms: u64,
    #[serde(default = "default_consecutive_pressure_samples")]
    pub consecutive_pressure_samples: u32,
    #[serde(default = "default_consecutive_recovery_samples")]
    pub consecutive_recovery_samples: u32,
    #[serde(default = "default_operations_reserve_requests")]
    pub operations_reserve_requests: usize,
    #[serde(default)]
    pub sample_failure_policy: ResourceSampleFailurePolicy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceSampleFailurePolicy {
    FailOpen,
    #[default]
    FailClosed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReloadConfig {
    #[serde(default)]
    pub mode: ReloadMode,
    #[serde(default = "default_reload_poll_interval_ms")]
    pub poll_interval_ms: u64,
}

impl Default for ReloadConfig {
    fn default() -> Self {
        Self {
            mode: ReloadMode::Disabled,
            poll_interval_ms: default_reload_poll_interval_ms(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReloadMode {
    #[default]
    Disabled,
    Watch,
}
