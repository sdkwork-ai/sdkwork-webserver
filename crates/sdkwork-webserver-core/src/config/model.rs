use std::{collections::BTreeMap, net::IpAddr};

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

/// Default ceiling for a proxied request body.
///
/// 16 MiB, not 10 MiB: the drive admin storage API carries object content as
/// **base64 inside JSON** (`storageProviders.objects.content.update`), and its
/// documented object ceiling is 8 MiB. Base64 inflates by 4/3, so the wire body
/// for a legal 8 MiB object is ≈11.2 MB plus the JSON envelope. A 10 MiB limit
/// silently rejected everything above ≈7.8 MB with a bare 413 — the client's own
/// guard allowed 8 MiB, so the last megabyte always failed. 16 MiB also matches
/// the web framework's own default body limit, so edge and module agree.
fn default_max_request_body_bytes() -> u64 {
    16 * 1024 * 1024
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

fn default_tls_handshake_timeout_ms() -> u64 {
    60_000
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
    /// nginx `map` variables declared at http context, in declaration order.
    ///
    /// A `map` declares a result variable whose value is selected from an
    /// ordered key/value table by one request variable. The result is
    /// expandable wherever the runtime already expands nginx variables
    /// (`add_header`, `proxy_set_header`, `return` URL/body, `rewrite`
    /// replacement, and variable `proxy_pass` templates); an empty result is
    /// the empty string and therefore omits a header or redirect target just
    /// like nginx. See `config::map_variables` for the bounded source, key and
    /// value subset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maps: Vec<MapConfig>,
    /// nginx `geo` address-variable blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub geo: Vec<GeoConfig>,
    /// nginx `split_clients` buckets.
    #[serde(
        rename = "splitClients",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub split_clients: Vec<SplitClientsConfig>,
    /// nginx stream `map` blocks (evaluated against the variables the stream
    /// plane knows at accept/preread time).
    #[serde(rename = "streamMaps", default, skip_serializing_if = "Vec::is_empty")]
    pub stream_maps: Vec<MapConfig>,
    /// Names of nginx HTTP cache zones declared by `proxy_cache_path` or a
    /// CGI `*_cache_path` `keys_zone=` parameter. All zones share the single
    /// process cache store (bounded difference: nginx sizes each zone
    /// separately).
    #[serde(
        rename = "cacheZoneNames",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cache_zone_names: Vec<String>,
    /// nginx **stream** `limit_conn_zone` declarations (the stream module owns
    /// a separate zone table from the HTTP one).
    #[serde(
        rename = "streamLimitConnZones",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub stream_limit_conn_zones: Vec<LimitConnZoneConfig>,
    /// nginx http-level `types { <mime> <ext…>; }` table, in authored order.
    /// `None` keeps the runtime's built-in MIME table; `Some` (including an
    /// explicitly empty `types {}` block) *replaces* it for this application,
    /// exactly as nginx replaces the inherited table at the declaring level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_types: Option<Vec<MimeTypeConfig>>,
    /// nginx `default_type`: the content type of a static representation whose
    /// extension is absent from the resolved table. `None` keeps the runtime's
    /// safe `application/octet-stream` fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_type: Option<String>,
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
    /// Multi-layer resolution cache (local file → memory → Redis →
    /// database). Absent means the system resolver runs uncached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_cache: Option<sdkwork_webserver_resolver_cache::ResolutionCacheConfig>,
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
    /// Mail proxy plane (`nginx mail {}`): POP3/IMAP/SMTP frontend listeners.
    /// Absent or empty keeps the mail plane idle.
    #[serde(default, skip_serializing_if = "MailConfig::is_empty")]
    pub mail: MailConfig,
    /// Shared HTTP response cache for proxied surfaces.
    #[serde(default)]
    pub proxy_cache: ProxyCacheConfig,
    /// App publishing domain fallback: when no local virtual host or website
    /// runtime binding matches a request host, resolve the server through the
    /// sdkwork-deployments control plane (default app domains
    /// `<slug>.app[-<env>].<suffix>` and user custom domains).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_domain_fallback: Option<AppDomainFallbackConfig>,
    /// SaaS traffic usage metering: per-domain / per-server-IP traffic facts
    /// attributed to the serving tenant and app, ingested into the
    /// sdkwork-deployments billing tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_metering: Option<UsageMeteringConfig>,
    /// Built-in network tunnel capability (reverse connections / NAT
    /// traversal, PRD SDKWORK WebServer Tunnel §38). Absent or `enabled =
    /// false` leaves the runtime byte-for-byte unchanged (§77).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<sdkwork_webserver_tunnel_core::TunnelConfig>,
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
    /// Typed-TOML `[http] unixSocketRoot`: the operator-configured directory
    /// that `unix:` upstream socket targets must live under.
    ///
    /// Unix-socket upstreams are deliberately typed-plane only. An
    /// nginx-configuration `*_pass unix:/path` fails closed at import with a
    /// diagnostic pointing here, because a stock nginx file cannot state the
    /// confinement root the Rust data plane requires. When set, upstream target
    /// URLs of the form `unix:/absolute/socket` are accepted only when the
    /// resolved path is a strict descendant of this absolute directory (no
    /// `..`, no backslashes, no NUL); everything else fails semantic
    /// validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix_socket_root: Option<String>,
    /// Typed `[http] documentRootAllowlist`: absolute directory prefixes a
    /// request-time `root $variable;` / `alias $variable;` may resolve to.
    ///
    /// A configuration that declares a variable document root fails closed at
    /// materialization unless this allowlist is configured and every evaluated
    /// path stays inside one of its prefixes. Empty means "no variable
    /// document roots are executable".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_root_allowlist: Vec<String>,
    /// Set by the nginx-configuration materializer only. Stock nginx does not
    /// stat document roots or certificate material at configuration load, so
    /// a file relocated without its file tree still validates; typed
    /// configurations keep the strict existence checks.
    #[serde(default)]
    pub native_compatibility: bool,
}

/// nginx `map` variable (`map $source $result { … }`), executed in the
/// bounded subset defined by `config::map_variables`.
///
/// nginx matches a map **case-insensitively** for every key (the map hash is
/// lowercased at startup; differential-confirmed against nginx 1.29.6: keys
/// `foo` and `FOO` are a duplicate declaration and both source values select
/// the same entry). With `hostnames` the keys may additionally be host
/// wildcards of the `*.suffix` / `prefix.*` / `.suffix` forms; wildcard
/// characters are literal text otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapConfig {
    /// Source variable, written with its `$` (`$host`, `$http_x_tenant`, …).
    pub source: String,
    /// Result variable name, written with its `$` (`$tenant_zone`).
    pub result: String,
    /// `hostnames;` — wildcard key matching for host-like sources.
    #[serde(default)]
    pub hostnames: bool,
    /// `default <value>;` — used when no key matches. An absent default is the
    /// empty string, exactly like nginx.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Authored key/value entries in declaration order.
    pub entries: Vec<MapEntryConfig>,
}

/// One nginx stream `set $variable value;` assignment. The value is a literal
/// template over the stream variables known at accept/preread time
/// (`$remote_addr`, `$remote_port`, `$server_addr`, `$server_port`,
/// `$ssl_preread_server_name`, stream `map` results and earlier `set` values).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamSetVariableConfig {
    /// Variable name without the `$`.
    pub name: String,
    pub value: String,
}

/// One entry of a nginx `geo` block: a CIDR/IP (or an `a-b` range with
/// `ranges;`) mapped to a literal value; `key == "default"` is the fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeoEntryConfig {
    pub key: String,
    pub value: String,
}

/// nginx `geo [$address] $variable { … }`: address-based variable assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeoConfig {
    /// Source address variable, written with its `$` (default `$remote_addr`).
    pub source: String,
    pub result: String,
    /// `ranges;` — keys are `first-last` address ranges instead of CIDRs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ranges: bool,
    pub entries: Vec<GeoEntryConfig>,
}

/// One `split_clients` bucket: `percent` is the authored percentage in
/// hundredths (`30%` → 3000); `None` is the `*` catch-all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitClientsPartConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u32>,
    pub value: String,
}

/// nginx `split_clients $source $result { … }`: deterministic MurmurHash2
/// bucketing of an expanded source value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitClientsConfig {
    pub source: String,
    pub result: String,
    pub parts: Vec<SplitClientsPartConfig>,
}

/// One `<key> <value>;` entry of a `map` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapEntryConfig {
    pub key: String,
    pub value: String,
}

/// One nginx `types { <mime> <ext…>; }` row: a media type and the file
/// extensions that resolve to it. Rows keep authored order; a later row that
/// declares an already-mapped extension wins (nginx logs a duplicate-extension
/// warning and uses the last declaration, differential-confirmed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MimeTypeConfig {
    /// Media type, for example `text/html`.
    pub mime: String,
    /// Extensions without the leading dot, stored lowercased.
    pub extensions: Vec<String>,
}

/// HTTP response gzip policy (`gzip`, `gzip_types`, `gzip_min_length`,
/// `gzip_vary`, `gzip_static`).
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
    /// nginx `gzip_vary`: declare `Vary: Accept-Encoding` whenever the gzip
    /// policy selects the representation, even when the negotiated encoding is
    /// identity. The nginx import plane defaults to `false` (nginx's default);
    /// the typed plane keeps the historical runtime behaviour (`true`) so an
    /// existing config does not silently change its cache-key advertisement.
    #[serde(default = "default_true")]
    pub vary: bool,
    /// nginx `gzip_static`: serve a precompressed `<path>.gz` sidecar for
    /// static content.
    #[serde(default)]
    pub static_files: GzipStaticMode,
    /// nginx `gzip_comp_level` (1-9; nginx default `1`). The runtime's
    /// compression layer owns its deflate level, so the declared value is
    /// recorded for import completeness but does not change negotiation or the
    /// decoded representation (a boundary documented on the nginx mapping).
    #[serde(default = "default_gzip_comp_level")]
    pub comp_level: u8,
    /// nginx `gzip_disable` PCRE patterns; nginx compiles them case-insensitively
    /// and disables gzip when any matches the request `User-Agent`. The runtime
    /// compiles them with the bounded Rust regex engine (an unsupported pattern
    /// fails closed at materialization).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disable: Vec<String>,
    /// The predefined `gzip_disable "msie6"` selector, tracked independently
    /// of the regex list (nginx merges the flag and the array through separate
    /// inheritance chains). A matching `MSIE 4/5/6` user agent disables gzip.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disable_msie6: bool,
    /// nginx `gzip_proxied` conditions for a request that carries `Via`.
    /// `None` is nginx's default `off` policy (no gzip for proxied requests).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxied: Option<GzipProxiedConfig>,
}

/// nginx `gzip_proxied` bitmask as one boolean per accepted token.
///
/// A request is "proxied" when it carries a `Via` header. With `off` (nginx's
/// default) or no matching condition, a proxied request is served
/// uncompressed; `any` accepts every proxied request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GzipProxiedConfig {
    #[serde(default)]
    pub off: bool,
    #[serde(default)]
    pub any: bool,
    #[serde(default)]
    pub expired: bool,
    #[serde(default)]
    pub no_cache: bool,
    #[serde(default)]
    pub no_store: bool,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub no_last_modified: bool,
    #[serde(default)]
    pub no_etag: bool,
    #[serde(default)]
    pub auth: bool,
}

impl GzipProxiedConfig {
    /// Whether the configuration declares any token at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !(self.off
            || self.any
            || self.expired
            || self.no_cache
            || self.no_store
            || self.private
            || self.no_last_modified
            || self.no_etag
            || self.auth)
    }
}

fn default_gzip_comp_level() -> u8 {
    1
}

/// nginx `gzip_static` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GzipStaticMode {
    /// Serve the sidecar only to clients that accept gzip.
    #[default]
    Off,
    /// Serve the sidecar when the client accepts gzip, else the identity file.
    On,
    /// Serve the sidecar regardless of `Accept-Encoding`.
    Always,
}

impl Default for GzipConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            types: Vec::new(),
            min_length: default_gzip_min_length(),
            vary: true,
            static_files: GzipStaticMode::Off,
            comp_level: default_gzip_comp_level(),
            disable: Vec::new(),
            disable_msie6: false,
            proxied: None,
        }
    }
}

impl Default for NginxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            profile: default_profile(),
            unknown_directive_policy: default_unknown_directive_policy(),
            unix_socket_root: None,
            document_root_allowlist: Vec::new(),
            native_compatibility: false,
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
    #[serde(default = "default_tls_handshake_timeout_ms")]
    pub tls_handshake_timeout_ms: u64,
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
            tls_handshake_timeout_ms: default_tls_handshake_timeout_ms(),
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
    /// Combines the configured certificate files with the dynamically assigned
    /// certificate set. When absent the resolution is derived from the declared
    /// sources, which preserves the historical one-of semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_certificate_resolution: Option<TlsCertificateResolution>,
    pub default_virtual_host_ref: Option<String>,
    pub max_connections: Option<usize>,
    pub trusted_proxy: Option<TrustedProxyConfig>,
    pub proxy_protocol: Option<ProxyProtocolConfig>,
    pub acme_http_01: Option<AcmeHttp01Config>,
    #[serde(default)]
    pub allow_plaintext_http: bool,
}

/// The reserved ACME HTTP-01 challenge namespace (RFC 8555 §8.3).
///
/// This is a protocol constant, not a routing preference: the certificate
/// worker writes `<webroot>/.well-known/acme-challenge/<token>` and the data
/// plane serves exactly that path with narrow precedence. It is defined once
/// here so the nginx-compatible materializer, the compiled model, and the data
/// plane cannot drift apart on a string that silently breaks issuance when it
/// changes.
pub const ACME_HTTP_01_CHALLENGE_PREFIX: &str = "/.well-known/acme-challenge/";

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

/// How a listener combines its configured certificate files with the
/// dynamically assigned (database-backed) certificate set.
///
/// `policy-first` is the layered strategy: a configured file wins for a server
/// name when it exists and is usable, and the assigned set covers every server
/// name the files do not. It is the only resolution that declares both a
/// `tlsPolicyRef` and a `tlsRuntime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TlsCertificateResolution {
    /// Serve only the certificate files referenced by `tlsPolicyRef`.
    PolicyOnly,
    /// Serve only the assigned certificate set.
    AssignmentOnly,
    /// Prefer a configured certificate file for each server name, falling back
    /// to the assigned certificate set per server name.
    PolicyFirst,
}

impl TlsCertificateResolution {
    /// Derives the resolution implied by the declared sources. Declaring both
    /// sources has no implied value: it requires an explicit `policy-first`.
    pub fn from_sources(has_policy_ref: bool, has_tls_runtime: bool) -> Option<Self> {
        match (has_policy_ref, has_tls_runtime) {
            (true, false) => Some(Self::PolicyOnly),
            (false, true) => Some(Self::AssignmentOnly),
            _ => None,
        }
    }

    /// Resolves the effective strategy: an explicit declaration wins, otherwise
    /// the declared sources imply one.
    pub fn effective(
        declared: Option<Self>,
        has_policy_ref: bool,
        has_tls_runtime: bool,
    ) -> Option<Self> {
        declared.or_else(|| Self::from_sources(has_policy_ref, has_tls_runtime))
    }

    /// Whether this listener serves configured certificate files.
    pub fn serves_policy_files(self) -> bool {
        matches!(self, Self::PolicyOnly | Self::PolicyFirst)
    }

    /// Whether this listener serves the dynamically assigned certificate set.
    pub fn serves_assignments(self) -> bool {
        matches!(self, Self::AssignmentOnly | Self::PolicyFirst)
    }
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
    /// nginx `server_name ~pattern` values attached to this certificate.
    /// Patterns are stored without the `~` marker; SNI regex selection
    /// evaluates them in authored order after exact and wildcard names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub server_name_regexes: Vec<String>,
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
        /// Optional nginx `ssl_trusted_certificate` / typed-TOML `chainFile`
        /// PEM chain used for downstream client-certificate verification. The
        /// certificate worker exposes it to client-auth trust stores; it is
        /// never silently dropped.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chain_file: Option<String>,
        /// Optional static OCSP response (nginx `ssl_stapling_file`, DER).
        /// Served as the stapled response for this certificate; the file is
        /// read once when the TLS material is built (nginx refreshes it, the
        /// runtime requires a reload — a documented limitation).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ocsp_stapling_file: Option<String>,
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

impl TlsVersion {
    /// Serialized spelling used by `TlsPolicyConfig` (`minimumVersion` /
    /// `maximumVersion`) and by the nginx `ssl_protocols` materializer.
    ///
    /// Derived from the serde rename so a future variant cannot be added
    /// without this arm failing to compile.
    pub fn wire(self) -> &'static str {
        match self {
            TlsVersion::Tls12 => "tls1.2",
            TlsVersion::Tls13 => "tls1.3",
        }
    }

    /// The platform TLS floor (`PRD-https-and-certificates.md` §2: TLS 1.0/1.1
    /// are forbidden, TLS 1.2 is required).
    pub const PLATFORM_FLOOR: TlsVersion = TlsVersion::Tls12;

    /// The platform TLS ceiling: the newest version the rustls core speaks.
    pub const PLATFORM_CEILING: TlsVersion = TlsVersion::Tls13;

    /// The platform default window: TLS 1.2 through 1.3, matching
    /// `SDKWORK_WEBSERVER_SPEC.md` §10 (`protocols` default) and
    /// `NGINX_SPEC.md` §3 (generated configs enable TLSv1.2 and TLSv1.3).
    pub const PLATFORM_DEFAULT_WINDOW: (TlsVersion, TlsVersion) =
        (TlsVersion::Tls12, TlsVersion::Tls13);
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

/// One `fastcgi_param NAME VALUE [if_not_empty];` entry of a FastCGI resource.
///
/// `name` must be a FastCGI parameter name token (ASCII alphanumerics and `_`)
/// and `value` is a template over the supported upstream variable vocabulary;
/// the materializer validates both. `if_not_empty` mirrors nginx's
/// `fastcgi_param ... if_not_empty`: when the expanded value is empty the
/// parameter is omitted instead of sent with an empty value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamParamConfig {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub if_not_empty: bool,
}

fn default_fastcgi_connect_timeout_ms() -> u64 {
    // nginx `fastcgi_connect_timeout` default.
    60_000
}

fn default_fastcgi_io_timeout_ms() -> u64 {
    // nginx `fastcgi_send_timeout` / `fastcgi_read_timeout` default.
    60_000
}

fn default_upstream_pass_request_headers() -> bool {
    true
}

fn default_grpc_buffer_size() -> u64 {
    // nginx `grpc_buffer_size` default.
    4096
}

/// Upstream CGI-family protocol of a [`ResourceConfig::CgiUpstream`] resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CgiUpstreamProtocol {
    Uwsgi,
    Scgi,
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
        /// Adaptive Web H5 document root (`location @h5`). When absent the
        /// PC `root` is used for every client.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        h5_root: Option<String>,
        #[serde(default = "default_index_files")]
        index_files: Vec<String>,
        /// nginx `try_files` fallback.
        ///
        /// A relative `/path`-style document fallback (SPA fallback) is served
        /// from this resource's root. The reserved `@name` form is an nginx
        /// named-location target: on a probe miss the data plane performs one
        /// bounded internal redirect to the virtual host's `@name` route
        /// instead of opening a file. It must reference a declared named
        /// location; `config::validate` rejects an unknown, chained, or
        /// duplicated target.
        spa_fallback: Option<String>,
        #[serde(default)]
        follow_symlinks: bool,
        /// nginx `alias` semantics: the route's matched path prefix is
        /// replaced by `root` (the alias value) before opening the file.
        /// `false` (nginx `root`) appends the full request path to `root`.
        #[serde(default)]
        strip_prefix: bool,
        /// nginx `root $variable;` / `alias $variable;`: a per-request document
        /// root template. When present, `root` is the empty placeholder and the
        /// data plane resolves the template with the rewrite-phase variables;
        /// the resolved path must be absolute, free of `..`, and inside
        /// `nginx.documentRootAllowlist` (fail closed otherwise).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dynamic_root: Option<String>,
    },
    Proxy {
        id: String,
        #[serde(default)]
        upstream_ref: String,
        #[serde(default)]
        strip_prefix: bool,
        /// nginx `proxy_pass` URI part (`http://backend/api/` → `/api/`).
        /// When present the route's matched path prefix is replaced by this
        /// URI before forwarding (nginx replacement semantics); a missing
        /// value forwards the full request URI like `proxy_pass` without a
        /// URI part.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_uri: Option<String>,
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
        /// nginx `proxy_method <method>`: forward the request with this method
        /// instead of the client's (for example a `GET` sent upstream as
        /// `POST`). Inherited http → server → location when undeclared.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proxy_method: Option<String>,
        /// nginx `proxy_intercept_errors on`: proxied responses whose status
        /// has an `error_page` mapping are replaced by the mapped page.
        #[serde(default)]
        proxy_intercept_errors: bool,
        /// nginx proxy response-header policy (`proxy_hide_header`,
        /// `proxy_pass_header`, `proxy_redirect`, `proxy_cookie_path`,
        /// `proxy_cookie_domain`) resolved for this resource. The nginx
        /// materializer always emits it (nginx defaults included), so `None`
        /// only appears in typed TOML/JSON configs that declare no policy and
        /// keeps the historical pass-through behavior (every upstream header
        /// forwarded except hop-by-hop fields).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proxy_response_policy: Option<ProxyResponsePolicyConfig>,
    },
    /// nginx `fastcgi_pass` location handler: the FastCGI 1.0 upstream adapter
    /// executes only the bounded directive subset below; unsupported FastCGI
    /// directives fail closed at materialization with a located diagnostic.
    FastCgi {
        id: String,
        /// Upstream id resolved by the materializer (a literal `host:port`
        /// target or a declared `upstream` name).
        upstream_ref: String,
        /// `fastcgi_param` entries in effective order (a level that declares any
        /// `fastcgi_param` replaces the inherited list, nginx inheritance).
        #[serde(default)]
        params: Vec<UpstreamParamConfig>,
        /// `fastcgi_index`: appended to `$fastcgi_script_name` for requests
        /// whose URI ends with `/`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<String>,
        /// `fastcgi_split_path_info`: a regex with exactly two capture groups.
        /// Captures 1/2 become `$fastcgi_script_name` / `$fastcgi_path_info`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        split_path_info: Option<String>,
        /// Effective nginx `root` for the location (from `root`, inherited from
        /// the server, or empty), used by `$document_root` and
        /// `$request_filename`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document_root: Option<String>,
        /// `fastcgi_pass_request_headers off`: suppress the automatic
        /// `HTTP_<name>` request-header parameters.
        #[serde(default = "default_upstream_pass_request_headers")]
        pass_request_headers: bool,
        /// `fastcgi_keep_conn`: sent in `BEGIN_REQUEST`. When on, a connection
        /// that answered a request with a clean `END_REQUEST` and no trailing
        /// bytes is returned to the bounded per-generation protocol pool
        /// instead of being closed.
        #[serde(default)]
        keep_conn: bool,
        /// nginx `fastcgi_bind <address>`: bind the upstream connection's
        /// source address before connect. Only an IP literal is executable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bind_address: Option<IpAddr>,
        /// nginx `fastcgi_socket_keepalive on`: enable `SO_KEEPALIVE` on the
        /// upstream socket.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        socket_keepalive: Option<bool>,
        /// `fastcgi_buffering` (materialized; the adapter always buffers the
        /// bounded response, see crate docs for the bounded-difference note).
        #[serde(default = "default_true")]
        buffering: bool,
        /// `fastcgi_request_buffering` (materialized; request bodies are
        /// buffered bounded before the upstream exchange).
        #[serde(default = "default_true")]
        request_buffering: bool,
        #[serde(default = "default_fastcgi_connect_timeout_ms")]
        connect_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        send_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        read_timeout_ms: u64,
        /// `fastcgi_hide_header` / `fastcgi_pass_header` / `fastcgi_ignore_headers`
        /// resolved policy for the upstream response.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response_policy: Option<ProxyResponsePolicyConfig>,
        /// `fastcgi_intercept_errors on`: an upstream error status is replaced
        /// by the matched `error_page` target.
        #[serde(default)]
        intercept_errors: bool,
        /// `fastcgi_next_upstream` / `fastcgi_next_upstream_tries` /
        /// `fastcgi_next_upstream_timeout` as the runtime retry policy.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry: Option<UpstreamRetryConfig>,
        /// Resolved `fastcgi_cache*` policy; `None` disables response caching.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache: Option<CgiCacheConfig>,
    },
    /// nginx `memcached_pass` location handler: bounded text-protocol GET/HEAD
    /// against one memcached upstream. The request key is the expansion of the
    /// location's `set $memcached_key <value>` template (the only `set` form the
    /// bounded materializer executes).
    Memcached {
        id: String,
        /// Upstream id resolved by the materializer (literal `host:port` or a
        /// declared `upstream`).
        upstream_ref: String,
        /// `set $memcached_key <value>` template, expanded per request.
        key_template: String,
        /// nginx `memcached_bind <address>`: bind the upstream connection's
        /// source address before connect. An IP literal only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bind_address: Option<IpAddr>,
        /// nginx `memcached_socket_keepalive on`: enable `SO_KEEPALIVE` on the
        /// upstream socket.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        socket_keepalive: Option<bool>,
        #[serde(default = "default_true")]
        buffering: bool,
        #[serde(default = "default_fastcgi_connect_timeout_ms")]
        connect_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        send_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        read_timeout_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry: Option<UpstreamRetryConfig>,
    },
    /// nginx `uwsgi_pass` / `scgi_pass` location handler: the uWSGI and SCGI
    /// CGI-family adapters share the FastCGI parameter pipeline and a raw HTTP
    /// response (nginx parses both modules' responses with the upstream header
    /// machinery).
    CgiUpstream {
        id: String,
        protocol: CgiUpstreamProtocol,
        /// Upstream id resolved by the materializer (literal `host:port` or a
        /// declared `upstream`).
        upstream_ref: String,
        /// `uwsgi_param` / `scgi_param` entries in effective order.
        #[serde(default)]
        params: Vec<UpstreamParamConfig>,
        /// Effective nginx `root` for the location.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document_root: Option<String>,
        /// `*_pass_request_headers off`: suppress automatic `HTTP_<name>` params.
        #[serde(default = "default_upstream_pass_request_headers")]
        pass_request_headers: bool,
        /// nginx `uwsgi_bind` / `scgi_bind <address>`: bind the upstream
        /// connection's source address before connect. An IP literal only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bind_address: Option<IpAddr>,
        /// nginx `uwsgi_socket_keepalive` / `scgi_socket_keepalive on`:
        /// enable `SO_KEEPALIVE` on the upstream socket.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        socket_keepalive: Option<bool>,
        /// `*_buffering` (materialized; the adapter always buffers bounded).
        #[serde(default = "default_true")]
        buffering: bool,
        /// `*_request_buffering` (materialized; request bodies are buffered bounded).
        #[serde(default = "default_true")]
        request_buffering: bool,
        #[serde(default = "default_fastcgi_connect_timeout_ms")]
        connect_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        send_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        read_timeout_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response_policy: Option<ProxyResponsePolicyConfig>,
        #[serde(default)]
        intercept_errors: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry: Option<UpstreamRetryConfig>,
        /// nginx `uwsgi_modifier1` (uWSGI only; default 0 = WSGI request).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modifier1: Option<u8>,
        /// nginx `uwsgi_modifier2` (uWSGI only; default 0).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modifier2: Option<u8>,
        /// nginx `uwsgi_string`: extra variable bytes appended to the uWSGI
        /// packet (uWSGI only).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extra_string: Option<String>,
        /// Resolved `uwsgi_cache*` / `scgi_cache*` policy; `None` disables
        /// response caching.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache: Option<CgiCacheConfig>,
    },
    /// nginx `grpc_pass` location handler: gRPC over HTTP/2. The transport is
    /// cleartext h2c for `grpc://` targets and TLS for `grpcs://` targets (the
    /// per-upstream TLS policy carries the certificate/trust settings); the
    /// bounded directive subset below is executable, unsupported `grpc_*` forms
    /// fail closed at materialization with a located diagnostic.
    Grpc {
        id: String,
        /// Upstream id resolved by the materializer (a literal `host:port`
        /// target or a declared `upstream` name). The upstream URL scheme is
        /// `http` for h2c and `https` for TLS.
        upstream_ref: String,
        /// nginx `grpc_set_header` entries as `"Name value"` strings. Supported
        /// value tokens are the shared proxy vocabulary
        /// (`$host`, `$scheme`, `$remote_addr`,
        /// `$proxy_add_x_forwarded_for`, `$http_upgrade`, `$server_port`) plus
        /// literals; any other variable fails closed at materialization.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        request_set_headers: Vec<String>,
        #[serde(default = "default_fastcgi_connect_timeout_ms")]
        connect_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        send_timeout_ms: u64,
        #[serde(default = "default_fastcgi_io_timeout_ms")]
        read_timeout_ms: u64,
        /// nginx `grpc_buffer_size`: the adapter's response stream chunk bound.
        #[serde(default = "default_grpc_buffer_size")]
        buffer_size: u64,
        /// `grpc_hide_header` / `grpc_pass_header` resolved policy for the
        /// upstream response (headers and trailing metadata).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response_policy: Option<ProxyResponsePolicyConfig>,
        /// `grpc_intercept_errors on`: an upstream HTTP error status (not a gRPC
        /// status) is replaced by the matched `error_page` target.
        #[serde(default)]
        intercept_errors: bool,
        /// `grpc_next_upstream` / `grpc_next_upstream_tries` /
        /// `grpc_next_upstream_timeout` as the runtime retry policy.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry: Option<UpstreamRetryConfig>,
    },
    Redirect {
        id: String,
        status: u16,
        location: String,
    },
    /// nginx `empty_gif`: the location handler serves the canonical minimal
    /// 1x1 transparent GIF (`ngx_http_empty_gif_module`, 43 bytes,
    /// `Content-Type: image/gif`). GET/HEAD only; any other method is 405.
    EmptyGif { id: String },
    /// nginx `stub_status`: a bounded status page with nginx's field shape
    /// (`ngx_http_stub_status_module`). Served to loopback clients only.
    StubStatus { id: String },
    Respond {
        id: String,
        status: u16,
        #[serde(default = "default_content_type")]
        content_type: String,
        /// nginx `return <status> "<body>"` derives the response content type
        /// from the request URI extension through the resolved `types` table
        /// (`default_type` when the extension is unknown). The typed TOML/JSON
        /// plane keeps declaring `contentType` explicitly, so this is opt-in.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        content_type_from_uri: bool,
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

/// One nginx `*_cache_valid [code...] <time>` rule: the listed statuses are
/// fresh for `seconds`; an empty status list means the directive's default
/// status list (`200`, `302`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CgiCacheValidRule {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<u16>,
    pub seconds: u64,
}

/// Resolved nginx CGI-family response cache policy (`fastcgi_cache`,
/// `uwsgi_cache`, `scgi_cache` and their companion directives) for one
/// location. The runtime reuses the shared HTTP response cache store
/// (`proxy_cache_path` capacity/disk tier); `zone` selects the nginx zone name
/// the location was declared against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CgiCacheConfig {
    pub zone: String,
    /// Cache-key template (nginx default
    /// `$scheme$request_method$host$request_uri`).
    pub key: String,
    /// Freshness rules by status (`*_cache_valid`). nginx has no default:
    /// when empty, a response is cached only if it carries an explicit upstream
    /// freshness header (`Cache-Control`/`Expires`).
    #[serde(default)]
    pub valid: Vec<CgiCacheValidRule>,
    /// `*_cache_bypass`: if any listed variable is non-empty and not `"0"`,
    /// the cache is not consulted for this request (the response may still be
    /// stored unless `no_cache` matches).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bypass: Vec<String>,
    /// `*_no_cache`: if any listed variable is non-empty and not `"0"`, the
    /// response is not stored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_cache: Vec<String>,
    /// `*_cache_revalidate on`: an expired entry is revalidated with its
    /// stored validators (`If-None-Match` / `If-Modified-Since`).
    #[serde(default)]
    pub revalidate: bool,
    /// `*_cache_use_stale` conditions (nginx tokens; the runtime supports
    /// `error`, `timeout`, `invalid_header`, `updating`, `http_403`,
    /// `http_404`, `http_500`, `http_502`, `http_503`, `http_504`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub use_stale: Vec<String>,
    /// `*_cache_methods`: request methods eligible for caching (default GET,
    /// HEAD).
    pub methods: Vec<String>,
    /// `*_cache_min_uses`: requests for the same key before the response is
    /// stored (default 1).
    pub min_uses: u32,
    /// `*_cache_lock on|off`: collapse concurrent fills of the same key into
    /// one upstream request (default on).
    pub lock: bool,
    /// `*_cache_lock_timeout`: how long a coalesced waiter waits for the
    /// filling request before doing its own upstream call (default 5 s).
    pub lock_timeout_ms: u64,
    /// `*_cache_lock_age`: accepted nginx-compatibility value; the shared
    /// store cannot preempt an in-progress fill (bounded difference).
    #[serde(default)]
    pub lock_age_ms: u64,
    /// `*_cache_background_update on`: when a fill for the same key is already
    /// in progress, an expired entry is served stale instead of waiting.
    #[serde(default)]
    pub background_update: bool,
    /// `*_cache_convert_head on`: a HEAD cache miss is filled with a GET (and
    /// HEAD lookups use the GET key). Default on.
    pub convert_head: bool,
    /// `*_cache_max_range_offset`: accepted; the runtime never caches Range
    /// requests (it always bypasses the cache for them), which is the
    /// conservative form of this limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_range_offset: Option<u64>,
}

impl ResourceConfig {
    pub fn id(&self) -> &str {
        match self {
            Self::Static { id, .. }
            | Self::Proxy { id, .. }
            | Self::FastCgi { id, .. }
            | Self::CgiUpstream { id, .. }
            | Self::Memcached { id, .. }
            | Self::Grpc { id, .. }
            | Self::Redirect { id, .. }
            | Self::EmptyGif { id }
            | Self::StubStatus { id }
            | Self::Respond { id, .. }
            | Self::Drive { id, .. }
            | Self::Knowledgebase { id, .. } => id,
        }
    }

    pub fn set_id(&mut self, id: String) {
        match self {
            Self::Static { id: current, .. }
            | Self::Proxy { id: current, .. }
            | Self::FastCgi { id: current, .. }
            | Self::CgiUpstream { id: current, .. }
            | Self::Memcached { id: current, .. }
            | Self::Grpc { id: current, .. }
            | Self::Redirect { id: current, .. }
            | Self::EmptyGif { id: current }
            | Self::StubStatus { id: current }
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
            | Self::FastCgi { .. }
            | Self::CgiUpstream { .. }
            | Self::Memcached { .. }
            | Self::Grpc { .. }
            | Self::Redirect { .. }
            | Self::EmptyGif { .. }
            | Self::StubStatus { .. }
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
            | Self::FastCgi { .. }
            | Self::CgiUpstream { .. }
            | Self::Memcached { .. }
            | Self::Grpc { .. }
            | Self::Redirect { .. }
            | Self::EmptyGif { .. }
            | Self::StubStatus { .. }
            | Self::Respond { .. } => None,
        }
    }

    /// Optional resolution-cache policy for provider-backed resources.
    pub fn provider_cache_policy(&self) -> Option<ProviderCachePolicy> {
        match self {
            Self::Drive { cache, .. } | Self::Knowledgebase { cache, .. } => *cache,
            Self::Static { .. }
            | Self::Proxy { .. }
            | Self::FastCgi { .. }
            | Self::CgiUpstream { .. }
            | Self::Memcached { .. }
            | Self::Grpc { .. }
            | Self::Redirect { .. }
            | Self::EmptyGif { .. }
            | Self::StubStatus { .. }
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
    /// Destination for proxied connections. `None` only for nginx stream
    /// `return` servers, which answer without an upstream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<StreamTargetConfig>,
    /// Ordered nginx stream `allow`/`deny` rules. nginx's stream access phase
    /// uses first-match-wins and allows a peer that matches no rule; an empty
    /// list allows every peer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access: Vec<AccessRuleConfig>,
    /// nginx stream `return <value>`: write the literal value to the
    /// connection and close it instead of proxying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_value: Option<String>,
    /// nginx stream `limit_conn <zone> <n>` rules (checked at accept time;
    /// a denied connection is closed silently, like nginx).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limit_conn: Vec<LimitConnConfig>,
    /// nginx stream `limit_conn_dry_run on`: count concurrent connections but
    /// do not deny the over-limit ones.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub limit_conn_dry_run: bool,
    /// nginx stream `set $variable value;` assignments, in authored order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub set_variables: Vec<StreamSetVariableConfig>,
    /// nginx stream `preread_timeout`: the deadline for reading the TLS
    /// ClientHello during `ssl_preread on`.
    #[serde(default = "default_stream_preread_timeout_ms")]
    pub preread_timeout_ms: u64,
    /// nginx stream `preread_buffer_size`: the ClientHello peek buffer bound.
    #[serde(default = "default_stream_preread_buffer_bytes")]
    pub preread_buffer_bytes: usize,
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

fn default_stream_preread_timeout_ms() -> u64 {
    30 * 1000
}

fn default_stream_preread_buffer_bytes() -> usize {
    16 * 1024
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

/// The nginx `mail {}` proxy plane (POP3/IMAP/SMTP frontend listeners).
///
/// Wave 9A models the listener plane and the plaintext greeting/auth dialog.
/// `auth_http`, mail TLS (`ssl_*`, STARTTLS/implicit-TLS listeners), `xclient`
/// and the backend relay are Wave 9B: they are absent from this model and the
/// nginx materializer fails closed with a located diagnostic instead of
/// accepting them silently.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<MailServerConfig>,
}

impl MailConfig {
    /// Whether no mail listener is configured (the mail plane stays idle).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

/// One `mail { server { … } }` listener.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailServerConfig {
    pub id: String,
    pub bind: String,
    pub port: u16,
    /// nginx mail `protocol pop3|imap|smtp` (nginx defaults to `imap`).
    #[serde(default)]
    pub protocol: MailServerProtocol,
    /// nginx mail `server_name`: the name used in the SMTP greeting. Absent
    /// means the runtime resolves the host name (`HOSTNAME`/`COMPUTERNAME`,
    /// then `localhost`), matching nginx's `gethostname()` default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    /// nginx mail `timeout`: idle timeout for the whole session.
    #[serde(default = "default_mail_timeout_ms")]
    pub timeout_ms: u64,
    /// nginx mail `so_keepalive on|off` (listener socket keepalive).
    #[serde(default)]
    pub so_keepalive: bool,
    /// nginx mail `proxy_protocol on|off`: accept a PROXY protocol v1 header
    /// from the connecting mail client (outbound PROXY emission to a backend
    /// is Wave 9B's relay concern).
    #[serde(default)]
    pub proxy_protocol: bool,
    /// nginx `smtp_client_buffer`: the SMTP client command line bound.
    #[serde(default = "default_mail_client_buffer_bytes")]
    pub smtp_client_buffer: usize,
    /// nginx `imap_client_buffer`: the IMAP client command line bound.
    #[serde(default = "default_mail_client_buffer_bytes")]
    pub imap_client_buffer: usize,
    /// nginx `pop3_auth` mechanisms enabled for this POP3 listener.
    #[serde(default = "default_pop3_auth")]
    pub pop3_auth: Vec<MailAuthMechanism>,
    /// nginx `imap_auth` mechanisms enabled for this IMAP listener.
    #[serde(default = "default_imap_auth")]
    pub imap_auth: Vec<MailAuthMechanism>,
    /// nginx `smtp_auth` mechanisms enabled for this SMTP listener.
    #[serde(default = "default_smtp_auth")]
    pub smtp_auth: Vec<MailAuthMechanism>,
    /// nginx `smtp_greeting_delay`: delay before the SMTP `220` greeting.
    #[serde(default)]
    pub smtp_greeting_delay_ms: u64,
    /// nginx `auth_http` authentication service (Wave 9B). Absent keeps the
    /// Wave 9A behavior: the authorization attempt is answered with nginx's
    /// auth-service-unavailable reply and the session closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_http: Option<MailAuthHttpConfig>,
    /// nginx mail frontend TLS (`ssl_certificate`, `ssl_certificate_key`,
    /// `ssl_protocols`) and STARTTLS mode (Wave 9B). Absent means the listener
    /// is plaintext with no STARTTLS, exactly like a nginx mail server with no
    /// `ssl_*` directives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<MailTlsConfig>,
    /// nginx mail backend relay policy (`proxy_*`, `xclient`, `proxy_protocol`,
    /// `proxy_smtp_auth`) resolved for this listener (Wave 9B).
    #[serde(default)]
    pub proxy: MailProxyConfig,
    /// nginx mail `max_errors` (default 5): the number of client protocol
    /// errors tolerated before the session is closed.
    #[serde(default = "default_mail_max_errors")]
    pub max_errors: u32,
}

/// nginx mail `auth_http` authentication service (Wave 9B).
///
/// nginx issues a bounded `GET <uri> HTTP/1.0` request to this service for
/// every authorization attempt, then uses the response `Auth-*` headers to
/// decide the client reply and the backend target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailAuthHttpConfig {
    /// `auth_http <host[:port][/path]>` (an optional `http://` prefix is
    /// accepted, exactly like nginx).
    pub uri: String,
    /// `Host` header sent to the auth service (nginx defaults it to the
    /// configured host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_header: Option<String>,
    /// `auth_http_header <name> <value>` entries, in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<String>,
    /// `auth_http_timeout` (nginx default 60s).
    #[serde(default = "default_mail_auth_http_timeout_ms")]
    pub timeout_ms: u64,
    /// `auth_http_pass_client_cert on`: forward the client certificate in the
    /// exchange when the listener terminates TLS with client verification.
    #[serde(default)]
    pub pass_client_cert: bool,
}

impl MailAuthHttpConfig {
    /// The bounded `host:port` peer and request target parsed from `uri`.
    #[must_use]
    pub fn endpoint(&self) -> Option<String> {
        let uri = self
            .uri
            .strip_prefix("http://")
            .unwrap_or(self.uri.as_str())
            .trim();
        let uri = uri.trim_end_matches('/');
        if uri.is_empty() {
            return None;
        }
        Some(uri.to_owned())
    }
}

/// nginx mail frontend TLS material (Wave 9B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailTlsConfig {
    pub certificate_file: String,
    pub private_key_file: String,
    /// `ssl_protocols`: the TLS 1.2/1.3 window (the runtime never negotiates
    /// older versions, matching the platform floor).
    #[serde(default = "default_mail_tls_protocols")]
    pub protocols: Vec<MailTlsProtocol>,
    /// `starttls on|off|only` (nginx default off).
    #[serde(default)]
    pub starttls: MailStarttlsMode,
    /// Listen with implicit TLS (`listen … ssl`).
    #[serde(default)]
    pub implicit: bool,
}

/// One allowed TLS protocol token from mail `ssl_protocols`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailTlsProtocol {
    #[serde(rename = "TLSv1.2")]
    Tls12,
    #[serde(rename = "TLSv1.3")]
    Tls13,
}

/// nginx mail `starttls` mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailStarttlsMode {
    #[default]
    Off,
    On,
    Only,
}

/// Resolved nginx mail backend relay policy (Wave 9B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailProxyConfig {
    /// nginx mail `proxy on|off`: accepted for compatibility (nginx 1.29.6
    /// merges the flag but never reads it; the relay always runs).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// nginx `xclient on|off`: send an `XCLIENT` line to the SMTP backend
    /// (nginx default on).
    #[serde(default = "default_true")]
    pub xclient: bool,
    /// nginx `proxy_smtp_auth on|off`: re-authenticate to the SMTP backend
    /// instead of passing the XCLIENT LOGIN attribute.
    #[serde(default)]
    pub smtp_auth: bool,
    /// nginx `proxy_protocol on|off`: send a PROXY protocol v1 header to the
    /// backend before the backend greeting.
    #[serde(default)]
    pub proxy_protocol: bool,
    /// nginx `proxy_pass_error_message on|off`: relay the backend's error
    /// response to the client instead of the generic internal error.
    #[serde(default)]
    pub pass_error_message: bool,
    /// nginx `proxy_buffer`: the backend response line buffer bound.
    #[serde(default = "default_mail_proxy_buffer_bytes")]
    pub buffer_bytes: usize,
    /// nginx `proxy_timeout` (nginx default 24h).
    #[serde(default = "default_mail_proxy_timeout_ms")]
    pub timeout_ms: u64,
    /// Typed-plane mail backend TLS (nginx has no `proxy_ssl` in the mail
    /// module; this is the SDKWork typed-config extension). Absent keeps the
    /// backend leg plaintext.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<MailProxyTlsConfig>,
}

impl Default for MailProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            xclient: true,
            smtp_auth: false,
            proxy_protocol: false,
            pass_error_message: false,
            buffer_bytes: default_mail_proxy_buffer_bytes(),
            timeout_ms: default_mail_proxy_timeout_ms(),
            tls: None,
        }
    }
}

/// SDKWork typed-config extension: TLS for the mail backend leg.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailProxyTlsConfig {
    /// Verify the backend certificate chain (default on; the runtime is
    /// stricter than nginx's HTTP `proxy_ssl_verify` default).
    #[serde(default = "default_true")]
    pub verify: bool,
    /// PEM trust anchors for `verify on`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_certificate_file: Option<String>,
    /// Client certificate presented to the backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_file: Option<String>,
    /// Client private key paired with `certificate_file`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_key_file: Option<String>,
    /// SNI name sent to the backend (defaults to the auth service's
    /// `Auth-Server` host when it is a name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

/// Default `max_errors` (nginx `ngx_mail_core_module` merge: 5).
fn default_mail_max_errors() -> u32 {
    5
}

fn default_mail_auth_http_timeout_ms() -> u64 {
    60 * 1000
}

fn default_mail_tls_protocols() -> Vec<MailTlsProtocol> {
    vec![MailTlsProtocol::Tls12, MailTlsProtocol::Tls13]
}

fn default_mail_proxy_buffer_bytes() -> usize {
    4096
}

fn default_mail_proxy_timeout_ms() -> u64 {
    24 * 60 * 60 * 1000
}

impl MailServerConfig {
    /// `bind:port` identity shared with the HTTP listener and stream planes.
    #[must_use]
    pub fn socket_key(&self) -> String {
        format!("{}:{}", self.bind.to_ascii_lowercase(), self.port)
    }

    /// The `SASL`/`AUTH` mechanism subset enabled for this listener's
    /// protocol, in configured order.
    #[must_use]
    pub fn auth_mechanisms(&self) -> &[MailAuthMechanism] {
        match self.protocol {
            MailServerProtocol::Pop3 => &self.pop3_auth,
            MailServerProtocol::Imap => &self.imap_auth,
            MailServerProtocol::Smtp => &self.smtp_auth,
        }
    }

    /// nginx `smtp_client_buffer` / `imap_client_buffer` for this listener's
    /// protocol (POP3 has no configurable client buffer in nginx).
    #[must_use]
    pub fn client_buffer_bytes(&self) -> usize {
        match self.protocol {
            MailServerProtocol::Pop3 => default_mail_client_buffer_bytes(),
            MailServerProtocol::Imap => self.imap_client_buffer,
            MailServerProtocol::Smtp => self.smtp_client_buffer,
        }
    }

    /// The resolved greeting/diagnostic server name. nginx uses the declared
    /// `server_name`, otherwise the process host name.
    #[must_use]
    pub fn effective_server_name(&self) -> String {
        self.server_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(default_mail_server_name)
    }
}

/// nginx mail `protocol` (`pop3` | `imap` | `smtp`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailServerProtocol {
    Pop3,
    #[default]
    Imap,
    Smtp,
}

impl MailServerProtocol {
    /// Lowercase protocol label used in diagnostics and logs.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pop3 => "pop3",
            Self::Imap => "imap",
            Self::Smtp => "smtp",
        }
    }
}

/// Auth mechanism the mail frontend can parse in Wave 9A.
///
/// nginx also accepts `apop`, `cram-md5`, and `external`; those need the
/// `auth_http` credential exchange (Wave 9B) and are refused by the
/// materializer with a located diagnostic rather than silently downgraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MailAuthMechanism {
    Plain,
    Login,
}

impl MailAuthMechanism {
    /// Uppercase SASL token (`PLAIN` / `LOGIN`).
    #[must_use]
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Plain => "PLAIN",
            Self::Login => "LOGIN",
        }
    }
}

fn default_mail_timeout_ms() -> u64 {
    60 * 1000
}

/// nginx `smtp_client_buffer` / `imap_client_buffer` default (`4k|8k`, the
/// page-size dependent default; 4 KiB on the supported Linux page size).
fn default_mail_client_buffer_bytes() -> usize {
    4096
}

fn default_pop3_auth() -> Vec<MailAuthMechanism> {
    vec![MailAuthMechanism::Plain, MailAuthMechanism::Login]
}

fn default_imap_auth() -> Vec<MailAuthMechanism> {
    vec![MailAuthMechanism::Plain]
}

fn default_smtp_auth() -> Vec<MailAuthMechanism> {
    vec![MailAuthMechanism::Plain, MailAuthMechanism::Login]
}

/// Host name used when `server_name` is absent (nginx calls `gethostname()` at
/// startup; the runtime reads the standard environment variable first and
/// falls back to the loopback name).
fn default_mail_server_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| {
            !name.is_empty()
                && !name
                    .contains(|character: char| character.is_whitespace() || character.is_control())
        })
        .unwrap_or_else(|| "localhost".to_owned())
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
    /// Upstream SNI / certificate-verification name override. Consumed by the
    /// gRPC adapter when nginx `grpc_ssl_name` declares a name different from
    /// the target host; the HTTP proxy path verifies the target host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
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
    /// Whether the TLS handshake sends a TLS SNI extension. nginx
    /// `uwsgi_ssl_server_name off` disables it; the default keeps the runtime's
    /// established behavior (SNI enabled) for every other upstream protocol.
    #[serde(default = "default_true")]
    pub send_sni: bool,
    /// Explicit, documented "dangerous accept any server certificate" mode.
    ///
    /// nginx `uwsgi_ssl_verify off` (and the typed-plane equivalent) means the
    /// handshake must succeed without validating the peer chain. The runtime
    /// never weakens verification implicitly: this flag is only set when the
    /// operator writes an explicit `off`, and the nginx loader emits a warning
    /// diagnostic for it. It must not be combined with custom trust material.
    #[serde(default)]
    pub dangerous_accept_any_verify: bool,
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

/// nginx `if` condition operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IfOperatorConfig {
    /// `$var`: non-empty.
    Truthy,
    /// `!$var`: empty.
    NegatedTruthy,
    Equals,
    NotEquals,
    Regex,
    NotRegex,
    RegexIgnoreCase,
    NotRegexIgnoreCase,
}

/// One nginx `if (<condition>)` test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IfConditionConfig {
    /// Left-hand variable without its `$` (`host`, `http_user_agent`,
    /// `is_mobile`, …).
    pub variable: String,
    pub operator: IfOperatorConfig,
    /// Right-hand operand: literal or variable template (expanded before the
    /// comparison). `None` for the truthiness operators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// One ordered nginx rewrite-phase instruction (`set` / `if` / the body of an
/// `if`).
///
/// Server-level instructions execute before location selection; location-level
/// instructions execute after it. `if` bodies are limited to the instruction
/// kinds the runtime executes inside the rewrite phase (`set`, `add_header`,
/// `return`, `rewrite`); anything else fails closed at materialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RewritePhaseOpConfig {
    Set {
        /// Target variable without its `$`.
        variable: String,
        /// Value template (`$var` / `${var}` / `$1..$9` / literal text).
        value: String,
    },
    If {
        condition: IfConditionConfig,
        body: Vec<RewritePhaseOpConfig>,
    },
    /// nginx `return <code> [body|url]` in a rewrite phase.
    Return {
        status: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<String>,
    },
    /// nginx `rewrite <pattern> <replacement> <flag>` in an `if` body.
    Rewrite {
        pattern: String,
        replacement: String,
        flag: RewriteFlag,
    },
    /// nginx `add_header` in an `if` body (expanded in the rewrite phase and
    /// emitted with the final response).
    AddHeader {
        name: String,
        value: String,
        #[serde(default)]
        always: bool,
    },
}

/// Maximum rewrite-phase instructions in one context (server or location).
pub const MAX_REWRITE_PHASE_OPS: usize = 256;
/// Maximum variables a rewrite phase may assign.
pub const MAX_REWRITE_PHASE_VARIABLES: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VirtualHostConfig {
    pub id: String,
    pub listener_refs: Vec<String>,
    /// nginx `server_name` values in authored order.
    ///
    /// Supported forms are an exact DNS name, `*.suffix`, `prefix.*`,
    /// `.suffix`, an IP literal, and the `~regex` form (a `~` entry is a
    /// bounded regular expression, not a DNS name). Host selection follows
    /// nginx exactly: exact, longest leading wildcard, longest trailing
    /// wildcard, regexes in this authored order, then the listener's default
    /// server. See `config::server_name`.
    pub server_names: Vec<String>,
    /// Path-selected routes plus named locations.
    ///
    /// An nginx named location (`location @fallback { … }`) is carried as a
    /// route whose exact match path is `@fallback`. Named routes are indexed
    /// by name and can never be selected by a request path (request paths
    /// start with `/`); they are reachable only through a bounded internal
    /// redirect from a static `tryFiles` fallback (`spaFallback = "@fallback"`)
    /// or from an `errorPages` entry whose `uri` is `@fallback`. The internal
    /// redirect depth is one and reference/cycle validation rejects anything
    /// longer; see `config::validate`.
    pub routes: Vec<RouteConfig>,
    /// Ordered rewrite-phase instructions (`set` / `if`) for this virtual
    /// host, executed before location selection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewrite_phase: Vec<RewritePhaseOpConfig>,
    #[serde(default)]
    pub security_headers: Option<SecurityHeadersConfig>,
    /// Server-level nginx `gzip` policy resolved for this host; `None`
    /// inherits the http-level (app-wide) policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<GzipConfig>,
    /// Server-level nginx `types` block resolved for this host; `None`
    /// inherits the http-level (app-wide) table. A declared block replaces the
    /// inherited table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_types: Option<Vec<MimeTypeConfig>>,
    /// Server-level nginx `default_type`; `None` inherits the http-level value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_type: Option<String>,
    /// nginx `error_page` mappings declared at http/server level for this
    /// host, in declaration order (later entries override earlier codes).
    /// A `uri` of `@name` targets a named location in this virtual host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub error_pages: Vec<ErrorPageConfig>,
    /// nginx `recursive_error_pages on`: an error produced while serving an
    /// error page may itself be mapped again (bounded hops).
    #[serde(default)]
    pub recursive_error_pages: bool,
    /// Server-level nginx `expires` / `etag` / `if_modified_since` resolved
    /// for this host; `None` inherits the http-level (app-wide) policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_policy: Option<CachePolicyConfig>,
    /// Redirect construction policy resolved for this host from the
    /// http/server `absolute_redirect` / `port_in_redirect` /
    /// `server_name_in_redirect` declarations; `None` keeps the nginx
    /// defaults (absolute Location, port included only off the scheme
    /// default, host taken from the request `Host`). A route-level
    /// declaration overrides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_policy: Option<RedirectPolicyConfig>,
    /// Server-level nginx `limit_req_status`; `None` inherits the http-level
    /// value (nginx default 503).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_req_status: Option<u16>,
    /// Server-level nginx `limit_conn_status`; `None` inherits the http-level
    /// value (nginx default 503).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_conn_status: Option<u16>,
    /// Server-level nginx `max_ranges`; `None` inherits the http-level value
    /// (nginx default: an effectively unbounded `NGX_MAX_INT32_VALUE`). `0`
    /// disables byte-range processing entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ranges: Option<u64>,
    /// nginx `merge_slashes` (default `on`) resolved for this host. When
    /// `off`, repeated adjacent slashes stay in the canonical request path:
    /// exact-location matching, static path identity, `$uri`, and proxied
    /// request targets keep them instead of collapsing to one slash. Bounded
    /// difference from nginx: nginx parses the request URI before virtual-host
    /// selection, so its effective value is the listener's default server's;
    /// the runtime resolves the directive per selected host (the same result
    /// when the directive is declared at the http level, which is how the
    /// differential fixture exercises it).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub merge_slashes: bool,
    /// nginx `underscores_in_headers` resolved for this host.
    ///
    /// nginx's default is `off`: a request header whose name contains `_` is
    /// dropped at the request boundary. The nginx materializer always emits the
    /// resolved value, so an imported configuration gets the nginx default. The
    /// serde default is `true` (forward the header) so the typed TOML/JSON
    /// plane keeps its historical permissive behavior: an existing API
    /// integration that uses an underscore header is not silently broken by a
    /// directive it never declared.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub underscores_in_headers: bool,
    /// nginx `ignore_invalid_headers` (default `on`) resolved for this host.
    /// The runtime always rejects a request whose header name is not an RFC
    /// 9110 token at its HTTP boundary with 400 (intentional, stricter
    /// divergence: nginx silently drops such a field when this is on and
    /// forwards it when off). The value is materialized so the nginx import
    /// surface is complete and typed configs can state it explicitly.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub ignore_invalid_headers: bool,
    /// Server-level nginx `charset` / `charset_types` / `source_charset` /
    /// `override_charset` resolved for this host; `None` means no level
    /// declared the family (no charset header rewrite).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<CharsetConfig>,
    /// Server-level nginx `autoindex` family resolved for this host; `None`
    /// means no level declared it (directory listings off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoindex: Option<AutoindexConfig>,
    /// Server-level nginx `valid_referers` policy; `None` means no level
    /// declared it (`$invalid_referer` is empty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<RefererConfig>,
}

/// `true` is the serde default for the nginx `merge_slashes` /
/// `ignore_invalid_headers` directives; serialize only overrides.
fn is_true(value: &bool) -> bool {
    *value
}

/// nginx `max_ranges` default: `ngx_http_core_module` merges
/// `NGX_MAX_INT32_VALUE`, i.e. a range-count ceiling that is effectively
/// unbounded for any real request.
pub const DEFAULT_MAX_RANGES: u64 = i32::MAX as u64;

/// nginx proxy response-header policy (`proxy_hide_header`,
/// `proxy_pass_header`, `proxy_redirect`, `proxy_cookie_path`,
/// `proxy_cookie_domain`).
///
/// nginx keeps a fixed default-hidden upstream response header set
/// (`ngx_http_proxy_hide_headers`: `Date`, `Server`, `X-Pad`,
/// `X-Accel-Expires`, `X-Accel-Redirect`, `X-Accel-Limit-Rate`,
/// `X-Accel-Buffering`, `X-Accel-Charset`) and never forwards those fields;
/// `proxy_hide_header` adds names to it and `proxy_pass_header` removes names
/// from it (pass wins when a name is in both lists — differential-confirmed
/// against nginx 1.29.6). The remaining fields select the upstream
/// `Location`/`Refresh` rewrite rules and the `Set-Cookie` attribute rewrites.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyResponsePolicyConfig {
    /// `proxy_hide_header <name>` entries, added to the default-hidden set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide_headers: Vec<String>,
    /// `proxy_pass_header <name>` entries, removed from the hidden set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pass_headers: Vec<String>,
    /// `proxy_redirect` policy. `None` means the directive was not declared at
    /// any level, which for a proxy resource is nginx's `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect: Option<ProxyRedirectPolicyConfig>,
    /// `proxy_cookie_path` rules; `None` means no rewriting (nginx default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_path: Option<CookieRewritePolicyConfig>,
    /// `proxy_cookie_domain` rules; `None` means no rewriting (nginx default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_domain: Option<CookieRewritePolicyConfig>,
}

/// nginx `proxy_redirect` policy for one proxy resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyRedirectPolicyConfig {
    /// `proxy_redirect off`: pass the upstream `Location`/`Refresh` through.
    Off,
    /// `proxy_redirect default`: replace the `proxy_pass` URL base with the
    /// matched location prefix (or the upstream base with `/` when
    /// `proxy_pass` has no URI part). The replacement is relative whenever the
    /// location prefix is, so nginx's header filter may still make it absolute
    /// per `absolute_redirect`.
    Default,
    /// Ordered `proxy_redirect <from> <to>` rules; the first rule whose
    /// literal `from` is a prefix of the upstream target wins. Matching is
    /// case-insensitive (nginx `ngx_rstrncmp`) and the replacement is emitted
    /// verbatim.
    Rules(Vec<ProxyRewriteRuleConfig>),
}

/// One literal `proxy_redirect <from> <to>` / `proxy_cookie_path|domain
/// <from> <to>` pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyRewriteRuleConfig {
    pub from: String,
    pub to: String,
}

/// nginx `proxy_cookie_path` / `proxy_cookie_domain` policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CookieRewritePolicyConfig {
    /// The directive form `off`: attribute rewriting is disabled.
    Off,
    /// Ordered literal `<from> <to>` rules. A path rule replaces the
    /// attribute-value prefix literally (case-sensitive, like nginx); a domain
    /// rule compares the value case-insensitively after ignoring one leading
    /// dot and re-applies the matched value's leading dot.
    Rules(Vec<ProxyRewriteRuleConfig>),
    /// Ordered `proxy_cookie_path ~[~*]<pattern> <replacement>` rules.
    RegexRules(Vec<ProxyRewriteRegexRuleConfig>),
}

/// One regex `proxy_cookie_path` / `proxy_cookie_domain` pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyRewriteRegexRuleConfig {
    /// Pattern without the `~` marker.
    pub pattern: String,
    /// Replacement template (`$1..$9` capture references are expanded).
    pub replacement: String,
    #[serde(default)]
    pub case_insensitive: bool,
}

/// nginx `autoindex` file-listing policy (`autoindex`,
/// `autoindex_exact_size`, `autoindex_localtime`, `autoindex_format`).
///
/// A directory request without a matching `index` file is answered 403 when
/// this policy is off (nginx's default); when on, the data plane renders
/// nginx-compatible listings (`html`, `json`, `jsonp`, `xml`) bounded in entry
/// count and safely escaped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoindexConfig {
    #[serde(default)]
    pub enabled: bool,
    /// nginx `autoindex_exact_size` (default `on`): exact byte counts; off
    /// renders nginx's human-readable K/M/G sizes.
    #[serde(default = "default_true")]
    pub exact_size: bool,
    /// nginx `autoindex_localtime` (default `off`): render mtimes in the
    /// process's local time zone instead of GMT.
    #[serde(default)]
    pub localtime: bool,
    /// nginx `autoindex_format` (default `html`).
    #[serde(default)]
    pub format: AutoindexFormat,
}

/// nginx `autoindex_format` values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoindexFormat {
    #[default]
    Html,
    Json,
    Jsonp,
    Xml,
}

/// HTTP response charset policy (`charset`, `charset_types`,
/// `source_charset`, `override_charset`).
///
/// nginx's charset header filter appends `; charset=<name>` to the
/// `Content-Type` of a response whose media type matches `charset_types`
/// (default list: `text/html`, `text/xml`, `text/plain`,
/// `text/vnd.wap.wml`, `application/javascript`, `application/rss+xml`). It
/// never sets the field on a 301/302 (nginx skips redirects so NN 4.x does not
/// apply the target page's charset to the redirect). Body transcoding
/// (`charset_map` + a `source_charset` that differs from `charset`) is not
/// implemented: such a location is accepted-unenforced and its `Content-Type`
/// is left untouched rather than claiming an encoding the bytes do not have.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharsetConfig {
    /// Effective `charset` value; `None` means `charset off` (or that no level
    /// declared the directive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    /// Effective `charset_types` media types. `None` keeps nginx's default
    /// list. A declared list always starts with `text/html` (nginx seeds the
    /// directive array with the first default type, so a declared list is
    /// "text/html plus the declared types"), and `["*"]` matches every media
    /// type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    /// Effective `source_charset`; `None` means off. A non-`None` value that
    /// differs from `charset` requires byte transcoding (out of scope; the
    /// filter is accepted-unenforced for that pair).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_charset: Option<String>,
    /// nginx `override_charset` (default off): when on, a charset already
    /// carried by the response (an upstream `Content-Type; charset=...`) is
    /// normalized against the configured one instead of being kept verbatim.
    #[serde(default)]
    pub override_charset: bool,
}

/// nginx redirect construction policy (`absolute_redirect`,
/// `port_in_redirect`, `server_name_in_redirect`).
///
/// nginx builds the client-visible `Location` in
/// `ngx_http_header_filter`/`ngx_http_header_filter_module`: only a
/// `Location` value starting with `/` is rewritten; an absolute target is
/// passed through unchanged. With `absolute_redirect on` (the nginx default)
/// the value becomes `<scheme>://<host>[:<port>]<path>`, where the host is the
/// first `server_name` when `server_name_in_redirect on` and the request's
/// `Host` header otherwise, and the port is the listener port, omitted when it
/// is the scheme default (80/http, 443/https) and always omitted when
/// `port_in_redirect off`. With `absolute_redirect off` the value stays
/// relative. Differential-confirmed against nginx 1.29.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RedirectPolicyConfig {
    /// nginx `absolute_redirect` (default `on`).
    #[serde(default = "default_true")]
    pub absolute_redirect: bool,
    /// nginx `port_in_redirect` (default `on`): include the listener port in
    /// an absolute `Location` unless it is the scheme default port.
    #[serde(default = "default_true")]
    pub port_in_redirect: bool,
    /// nginx `server_name_in_redirect` (default `off`): build the absolute
    /// `Location` host from the first `server_name` instead of the request
    /// `Host` header.
    #[serde(default)]
    pub server_name_in_redirect: bool,
}

impl RedirectPolicyConfig {
    /// nginx's compiled defaults (all levels absent).
    pub const NGINX_DEFAULT: Self = Self {
        absolute_redirect: true,
        port_in_redirect: true,
        server_name_in_redirect: false,
    };
}

/// One nginx `error_page <codes…> [=response] uri;` mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorPageConfig {
    /// Status codes this mapping answers for.
    pub codes: Vec<u16>,
    /// Internal-redirect target: an absolute `/path`, or `@name` for a named
    /// location in the same virtual host.
    pub uri: String,
    /// Optional `=code` response-code replacement; `None` keeps the
    /// original error status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_code: Option<u16>,
    /// nginx `error_page <code> = <target>;`: the bare `=` form (no explicit
    /// `=code`) makes the internally redirected target's own response status
    /// the final status. Without it the original error status is kept unless
    /// `response_code` overrides it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub from_target: bool,
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
    /// nginx `add_header … always`: emit the field on every response status.
    /// Without it nginx's header filter only adds the field to the statuses in
    /// [`CustomHeaderConfig::NGINX_DEFAULT_STATUSES`] (200, 201, 204, 206, 301,
    /// 302, 303, 304, 307, 308). Differential-confirmed against nginx 1.29.6,
    /// including 403/404/500 responses that omit a non-`always` field.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub always: bool,
}

impl CustomHeaderConfig {
    /// Response statuses an `add_header` entry without `always` applies to
    /// (`ngx_http_headers_filter` `safe_status`).
    pub const NGINX_DEFAULT_STATUSES: [u16; 10] =
        [200, 201, 204, 206, 301, 302, 303, 304, 307, 308];

    /// Variables the response-header layer expands in an `add_header` value.
    /// Every other `$name` fails configuration validation, so a typo can never
    /// silently reach clients as a literal `$…` string.
    pub const SUPPORTED_VARIABLES: &'static [&'static str] = &[
        "host",
        "scheme",
        "remote_addr",
        "server_port",
        "request_uri",
        "uri",
        "document_uri",
        "args",
        "is_args",
        "request_method",
        "server_name",
        // nginx `ngx_http_referer_module`: "1" when `valid_referers` rejects
        // the request's Referer, empty otherwise.
        "invalid_referer",
    ];

    /// Maximum header value length. nginx places no comparable bound; this is
    /// large enough for a realistic Content-Security-Policy while keeping a
    /// configuration (or an operator typo) from materializing an unbounded
    /// response header.
    pub const MAX_VALUE_BYTES: usize = 4096;

    /// Validate one response header declaration (`add_header Name value
    /// [always]` in nginx, or a JSON/TOML custom header): a valid HTTP field
    /// name, a bounded value, and only the variables the runtime expands.
    pub fn validate_declaration(name: &str, value: &str) -> Result<(), String> {
        Self::validate_declaration_with_variables(name, value, &[])
    }

    /// [`Self::validate_declaration`] plus the nginx `map` result variables
    /// declared by the same application config. A map result is a legal
    /// `$variable` in a response header value; every other unlisted variable
    /// still fails closed.
    pub fn validate_declaration_with_variables(
        name: &str,
        value: &str,
        map_variables: &[&str],
    ) -> Result<(), String> {
        if name.is_empty() {
            return Err("response header name must not be empty".to_owned());
        }
        if name.len() > 128 {
            return Err(format!(
                "response header name `{name}` exceeds the 128-byte bound"
            ));
        }
        if !name.bytes().all(|byte| {
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
        }) {
            return Err(format!(
                "response header name `{name}` is not a valid HTTP field name"
            ));
        }
        if value.len() > Self::MAX_VALUE_BYTES {
            return Err(format!(
                "response header `{name}` value is {} bytes; maximum is {}",
                value.len(),
                Self::MAX_VALUE_BYTES
            ));
        }
        // Fail closed instead of silently dropping an unrepresentable value at
        // response time: HTTP field values are visible ASCII plus SP/HTAB.
        if !value
            .bytes()
            .all(|byte| byte == b'\t' || (0x20..=0x7e).contains(&byte))
        {
            return Err(format!(
                "response header `{name}` value must be printable ASCII (HTTP field-value characters)"
            ));
        }
        let mut rest = value;
        while let Some(start) = rest.find('$') {
            let after = &rest[start + 1..];
            if let Some(escaped) = after.strip_prefix('$') {
                rest = escaped;
                continue;
            }
            let end = after
                .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .unwrap_or(after.len());
            let variable = &after[..end];
            if variable.is_empty() {
                return Err(format!("response header `{name}` contains a bare `$`"));
            }
            if !Self::SUPPORTED_VARIABLES.contains(&variable) && !map_variables.contains(&variable)
            {
                return Err(format!(
                    "response header `{name}` uses `${variable}`, which the runtime does not expand; supported: {}",
                    Self::SUPPORTED_VARIABLES
                        .iter()
                        .map(|name| format!("${name}"))
                        .chain(map_variables.iter().map(|name| format!("${name}")))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            rest = &after[end..];
        }
        Ok(())
    }
}

fn default_x_content_type_options() -> bool {
    true
}

fn default_hsts_max_age_seconds() -> u32 {
    31_536_000
}

/// nginx response freshness policy: `expires`, `etag`, `if_modified_since`.
///
/// Each directive carries its own nginx inheritance chain (http → server →
/// location, replace-on-declare), so materialization resolves the three
/// directives *separately* and stores the complete effective triple. A
/// `None` on the route/virtual host therefore means "nothing declared at this
/// level or above", not "expires off".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CachePolicyConfig {
    /// `expires` mode (`ngx_http_parse_expires`).
    #[serde(default)]
    pub expires: ExpiresMode,
    /// Signed `expires` time argument in seconds. For [`ExpiresMode::Daily`]
    /// this is the time of day as seconds after midnight (`0..=86400`).
    #[serde(default)]
    pub expires_seconds: i64,
    /// nginx `etag on|off`; `true` (the nginx default) emits an entity tag
    /// for static files, `false` suppresses it.
    #[serde(default = "default_true")]
    pub etag: bool,
    /// nginx `if_modified_since exact|before|off`.
    #[serde(default)]
    pub if_modified_since: IfModifiedSinceMode,
}

impl Default for CachePolicyConfig {
    fn default() -> Self {
        Self {
            expires: ExpiresMode::Off,
            expires_seconds: 0,
            etag: true,
            if_modified_since: IfModifiedSinceMode::Exact,
        }
    }
}

/// nginx `expires` modes as parsed by `ngx_http_parse_expires`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExpiresMode {
    /// `expires off` (nginx default): leave `Expires`/`Cache-Control` alone.
    #[default]
    Off,
    /// `expires epoch`: `Expires: Thu, 01 Jan 1970 00:00:01 GMT` + `no-cache`.
    Epoch,
    /// `expires max`: `Expires: Thu, 31 Dec 2037 23:55:55 GMT` + 10-year max-age.
    Max,
    /// `expires @<time>`: the next occurrence of a wall-clock time of day.
    Daily,
    /// `expires <time>`: relative to the time the response is produced.
    Access,
    /// `expires modified <time>`: relative to the response `Last-Modified`.
    Modified,
}

/// nginx `if_modified_since` modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IfModifiedSinceMode {
    /// `exact` (nginx default): `If-Modified-Since` must equal `Last-Modified`.
    #[default]
    Exact,
    /// `before`: a `Last-Modified` not newer than the condition is unchanged.
    Before,
    /// `off`: ignore the request `If-Modified-Since` entirely.
    Off,
}

impl ExpiresMode {
    /// Stable wire name, shared by every configuration surface and by the
    /// serialized `CachePolicyConfig`. Single-sourcing the spelling means a
    /// new mode cannot be added without a compile error here.
    pub fn wire(self) -> &'static str {
        match self {
            ExpiresMode::Off => "off",
            ExpiresMode::Epoch => "epoch",
            ExpiresMode::Max => "max",
            ExpiresMode::Daily => "daily",
            ExpiresMode::Access => "access",
            ExpiresMode::Modified => "modified",
        }
    }

    /// Whether the mode rewrites `Expires`/`Cache-Control` at all. `off` is
    /// nginx's default and leaves the deployment-level headers untouched.
    pub fn is_declared(self) -> bool {
        !matches!(self, ExpiresMode::Off)
    }
}

impl IfModifiedSinceMode {
    /// Stable wire name, shared by every configuration surface.
    pub fn wire(self) -> &'static str {
        match self {
            IfModifiedSinceMode::Exact => "exact",
            IfModifiedSinceMode::Before => "before",
            IfModifiedSinceMode::Off => "off",
        }
    }
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
    /// Location-level nginx `gzip` policy resolved for this route; `None`
    /// inherits the host/app policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<GzipConfig>,
    /// Location-level nginx `types` block; `None` inherits the host table and
    /// a declared block replaces it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_types: Option<Vec<MimeTypeConfig>>,
    /// Location-level nginx `default_type`; `None` inherits the host value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_type: Option<String>,
    /// Location-level nginx `error_page` mappings; when non-empty they
    /// replace the host-level set for this route (nginx inheritance).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub error_pages: Vec<ErrorPageConfig>,
    /// Location-level nginx `add_header` entries in declaration order. nginx
    /// replaces the inherited array when a level declares any `add_header`, so
    /// a non-empty list here *replaces* the host's `securityHeaders.customHeaders`
    /// for this route; an empty list inherits them. Duplicate names are
    /// preserved as separate fields, and `always` controls status gating.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_headers: Vec<CustomHeaderConfig>,
    /// Location-level nginx `expires` / `etag` / `if_modified_since`
    /// resolved for this route; `None` inherits the host/app policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_policy: Option<CachePolicyConfig>,
    /// Ordered rewrite-phase instructions (`set` / `if`) for this location,
    /// executed after location selection and before the content handler.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewrite_phase: Vec<RewritePhaseOpConfig>,
    /// Effective nginx `client_max_body_size` for this location (inherited from
    /// the server when undeclared); `None` means the app-level limit applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_request_body_bytes: Option<u64>,
    /// Location-level redirect construction policy; `None` inherits the
    /// virtual host's value. See [`RedirectPolicyConfig`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_policy: Option<RedirectPolicyConfig>,
    /// Location-level nginx `limit_req_status`; `None` inherits the host
    /// value (nginx default 503).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_req_status: Option<u16>,
    /// Location-level nginx `limit_conn_status`; `None` inherits the host
    /// value (nginx default 503).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_conn_status: Option<u16>,
    /// Location-level nginx `max_ranges`; `None` inherits the host value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ranges: Option<u64>,
    /// nginx `internal`: the location is reachable only through an internal
    /// redirect (`rewrite … last`, `error_page`, a `try_files` named target,
    /// or `X-Accel-Redirect`). An external request that matches it is answered
    /// with nginx's generated 404, and the location's own `add_header` /
    /// `error_page` policy still applies to that 404 (differential-confirmed
    /// against nginx 1.29.6).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub internal: bool,
    /// Location-level nginx charset policy resolved over the host value;
    /// `None` inherits the host (a location that declares any directive in the
    /// family gets the fully resolved config here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<CharsetConfig>,
    /// Location-level nginx `autoindex` family resolved over the host value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoindex: Option<AutoindexConfig>,
    /// Location-level nginx `valid_referers` policy resolved over the host
    /// value (a location declaration replaces the inherited list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<RefererConfig>,
}

/// One `valid_referers` host entry: a lowercased host (optionally a leading
/// `*.` or `.` wildcard) plus the optional URI-prefix part after the first
/// `/` (nginx splits the configured value there).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefererHostEntry {
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// nginx `valid_referers` policy (ngx_http_referer_module): the ordered
/// matchers that decide `$invalid_referer`. A declared directive replaces the
/// inherited list entirely (nginx merge semantics).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefererConfig {
    /// `none`: a missing `Referer` header is valid.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub none: bool,
    /// `blocked`: a `Referer` without an `http://`/`https://` scheme is valid.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
    /// `server_names`: the host must match one of this virtual host's
    /// `server_name` entries.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub server_names: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<RefererHostEntry>,
    /// `~regex` entries, matched case-insensitively against the referer value
    /// with the scheme stripped (nginx `ngx_http_add_regex_referer`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regexes: Vec<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Maximum total body bytes resident in the memory tier (LRU evicts
    /// beyond this even when `max_entries` is not reached; nginx
    /// `proxy_cache_path max_size` analog for the memory L1).
    #[serde(default = "default_proxy_cache_max_memory_bytes")]
    pub max_memory_bytes: u64,
    /// Maximum total object bytes on the disk tier; the oldest objects are
    /// evicted when the tier outgrows it (nginx `proxy_cache_path max_size`
    /// for the L2). Unique-URL floods can otherwise fill the disk without
    /// bound.
    #[serde(default = "default_proxy_cache_max_disk_bytes")]
    pub max_disk_bytes: u64,
    /// Maximum object count on the disk tier, enforced alongside
    /// `max_disk_bytes`.
    #[serde(default = "default_proxy_cache_max_disk_entries")]
    pub max_disk_entries: u64,
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
            max_memory_bytes: default_proxy_cache_max_memory_bytes(),
            max_disk_bytes: default_proxy_cache_max_disk_bytes(),
            max_disk_entries: default_proxy_cache_max_disk_entries(),
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
    1024 * 1024
}

fn default_proxy_cache_max_memory_bytes() -> u64 {
    128 * 1024 * 1024
}

fn default_proxy_cache_max_disk_bytes() -> u64 {
    1024 * 1024 * 1024
}

fn default_proxy_cache_max_disk_entries() -> u64 {
    100_000
}

fn default_proxy_cache_ttl_seconds() -> u64 {
    60
}

fn default_proxy_cache_stale_ttl_seconds() -> u64 {
    60
}

/// App publishing domain fallback (`appDomainFallback`). When no local
/// virtual host or website runtime binding matches a request, the data plane
/// resolves the host through the sdkwork-deployments control plane and
/// serves the site's latest compiled runtime descriptor. Default app domains
/// (`<slug>.app[-<env>].<suffix>` over the configured suffixes) and user
/// custom domains both resolve there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppDomainFallbackConfig {
    #[serde(default = "default_app_domain_fallback_enabled")]
    pub enabled: bool,
    /// Platform app-domain suffixes. Defaults to the control plane's catalog
    /// (14 domains: sdkwork.com/.cn, birdcoder.com/.cn, dtupay.com/.cn,
    /// skubc.com/.cn, zowalk.com/.cn, offer86.com/.cn, 86offer.com/.cn) and
    /// must stay a subset of it: the Deploy control plane only ever
    /// provisions hostnames under those suffixes. Requests for
    /// `<appLabel>.app[-<env>].<suffix>` are classified as default app
    /// domains; every other hostname is treated as a user custom domain.
    #[serde(default = "default_app_domain_suffixes")]
    pub suffixes: Vec<String>,
    /// Lookup channel. The Web Server is a standalone-only product
    /// (`SDKWORK_WEBSERVER_SPEC.md` §17.4): its data plane shares the process
    /// with the Deploy control plane, so the app-domain lookup is always
    /// resolved through the shared Deploy database. There is deliberately no
    /// HTTP control-plane channel — a mode that silently degraded to "no
    /// fallback at all" on an edge without a shared pool was worse than the
    /// configuration surface it pretended to offer.
    #[serde(default)]
    pub lookup: AppDomainFallbackLookup,
    /// Upper bound for a control-plane resolution call.
    #[serde(default = "default_app_domain_timeout_ms")]
    pub timeout_ms: u64,
    /// How long a resolved server stays cached before re-lookup.
    #[serde(default = "default_app_domain_cache_ttl_ms")]
    pub cache_ttl_ms: u64,
    /// How long a resolution miss stays cached (negative cache).
    #[serde(default = "default_app_domain_negative_cache_ttl_ms")]
    pub negative_cache_ttl_ms: u64,
}

/// Fallback lookup channel configuration.
///
/// Only the embedded channel exists: the standalone Web Server resolves
/// unmatched hosts against the shared Deploy database in the same process.
/// `deny_unknown_fields` keeps the typed model exactly as strict as the
/// published schema (`lookup` has `additionalProperties: false`), so the
/// in-memory compile path cannot accept a section the file path rejects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
pub enum AppDomainFallbackLookup {
    /// Resolve through the shared Deploy database (`deploy_app_binding` /
    /// `deploy_app_revision`). Requires the process database connection.
    #[default]
    Embedded,
}

impl Default for AppDomainFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: default_app_domain_fallback_enabled(),
            suffixes: default_app_domain_suffixes(),
            lookup: AppDomainFallbackLookup::Embedded,
            timeout_ms: default_app_domain_timeout_ms(),
            cache_ttl_ms: default_app_domain_cache_ttl_ms(),
            negative_cache_ttl_ms: default_app_domain_negative_cache_ttl_ms(),
        }
    }
}

fn default_app_domain_fallback_enabled() -> bool {
    true
}

/// Default platform app-domain suffixes. Delegates to the control plane that
/// provisions them (`sdkwork-deployments` `sdkwork-deploy-core`), so the
/// catalog has exactly one definition: a suffix the control plane does not
/// provision can never be classified as a default app domain here, which
/// would produce a permanent 404 documented as a "default app" host.
pub fn default_app_domain_suffixes() -> Vec<String> {
    sdkwork_deploy_core::platform_app_domain_suffixes()
}

fn default_app_domain_timeout_ms() -> u64 {
    2_000
}

fn default_app_domain_cache_ttl_ms() -> u64 {
    60_000
}

fn default_app_domain_negative_cache_ttl_ms() -> u64 {
    5_000
}

/// SaaS traffic usage metering (`usageMetering`). Every served request is
/// counted per domain (hostname) and per server IP with tenant/app
/// attribution when known, aggregated in memory windows and ingested into
/// the sdkwork-deployments billing tables (`deploy_usage_event` +
/// daily rollups) through the configured channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageMeteringConfig {
    #[serde(default = "default_usage_metering_enabled")]
    pub enabled: bool,
    /// Aggregation window in seconds (events are aggregated per window and
    /// deduplicated on the window key).
    #[serde(default = "default_usage_metering_window_seconds")]
    pub window_seconds: u64,
    /// How often aggregated windows are flushed to the ingest channel.
    #[serde(default = "default_usage_metering_flush_interval_ms")]
    pub flush_interval_ms: u64,
    /// Ingest channel: `embedded` writes through the shared Deploy database
    /// (standalone deployment); `http` posts to the Deploy control-plane
    /// ingest endpoint (cloud deployment).
    #[serde(default)]
    pub channel: UsageMeteringChannel,
    /// Hard cap on live aggregation buckets. Hostname is client influenceable
    /// input, so an adversarial traffic pattern must hit bounded rejection
    /// (facts for new bucket keys are dropped and counted) instead of
    /// unbounded memory growth (PRD §8.1).
    #[serde(default = "default_usage_metering_max_buckets")]
    pub max_buckets: usize,
}

/// Usage ingest channel configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
#[derive(Default)]
pub enum UsageMeteringChannel {
    /// Write through the shared Deploy database (`deploy_usage_event`).
    /// Requires the process database connection.
    #[default]
    Embedded,
    /// POST batches to the Deploy control-plane ingest endpoint
    /// (`/backend/v3/api/usage/ingest`).
    Http {
        /// Absolute control-plane ingest endpoint URL.
        endpoint: String,
        /// Optional file whose trimmed content is the API access token.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth_token_file: Option<String>,
    },
}

impl Default for UsageMeteringConfig {
    fn default() -> Self {
        Self {
            enabled: default_usage_metering_enabled(),
            window_seconds: default_usage_metering_window_seconds(),
            flush_interval_ms: default_usage_metering_flush_interval_ms(),
            channel: UsageMeteringChannel::default(),
            max_buckets: default_usage_metering_max_buckets(),
        }
    }
}

fn default_usage_metering_max_buckets() -> usize {
    65_536
}

fn default_usage_metering_enabled() -> bool {
    true
}

fn default_usage_metering_window_seconds() -> u64 {
    60
}

fn default_usage_metering_flush_interval_ms() -> u64 {
    30_000
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
