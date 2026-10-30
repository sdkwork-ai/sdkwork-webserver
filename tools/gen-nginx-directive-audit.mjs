#!/usr/bin/env node
/**
 * Generator for the directive-by-directive nginx compatibility audit ledger.
 *
 *   node tools/gen-nginx-directive-audit.mjs                  # write the report
 *   node tools/gen-nginx-directive-audit.mjs --check          # exit 1 when the report is stale
 *   node tools/gen-nginx-directive-audit.mjs --verify-evidence # re-check citations and over-claim status
 *
 * The ledger is derived programmatically from the two committed contract files:
 *
 *   specs/nginx-directive-inventory.json        (744 nginx 1.29.6 OSS directives)
 *   specs/nginx-directive-support.catalog.json  (classification + evidence + migration data)
 *
 * Every one of the 744 inventory directives gets exactly one audit decision and
 * one generated rationale. The decision rules live in this file; the wave,
 * security-difference and remaining-backlog sets are explicit so a reviewer can
 * see why a row is `runtime-owned` rather than `behavior-gap` instead of
 * re-deriving it from prose. The generator never reads the network, never embeds
 * timestamps or a live git HEAD, and sorts every collection before rendering, so
 * a rerun on the same catalog is byte-identical.
 *
 * `--verify-evidence` re-checks the audited claims against the real repository:
 * (a) every evidence file/test/case cited by an `executable` entry still exists;
 * (b) an accepted-unenforced entry never claims an "enforced typed key" that the
 * typed-TOML materializer does not consume; (c) an executable entry never renders
 * an accepted-but-ignored typed key; (d) the previously reported weak citations
 * really were replaced; (e) every directive recorded as completed in waves
 * 1/2/2b is really classified executable. It is read-only and offline as well.
 */
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..');
const INVENTORY_PATH = path.join(REPO_ROOT, 'specs', 'nginx-directive-inventory.json');
const CATALOG_PATH = path.join(REPO_ROOT, 'specs', 'nginx-directive-support.catalog.json');
const SERVER_TOML_PATH = path.join(
  REPO_ROOT,
  'crates',
  'sdkwork-webserver-core',
  'src',
  'config',
  'server_toml.rs',
);
const REPORT_PATH = path.join(
  REPO_ROOT,
  'docs',
  'reports',
  '2026-10-30-nginx-directive-by-directive-audit.md',
);

const AUDIT_DATE = '2026-10-30';
const AUDIT_HEAD = 'ed74ed90';
const AUDIT_BRANCH = 'main';
const PROFILE = 'http-core-v1';
const SNAPSHOT_STATE =
  'uncommitted worktree refresh after Waves 1, 2, 2b, 3a, 3a-2, 3b, 4a, the upstream-protocol wave (wave 5), the gRPC wave (wave 6), the stream/OCSP wave, the CGI-transport wave (wave 7A), the rewrite/variable-engine wave (wave 10), the mail waves (9A/9B), the uWSGI upstream-TLS wave (wave 7B), the CGI cache-family wave (wave 7C) and the protocol response-policy wave (wave 7D) (code plus the re-synced directive catalog), on top of HEAD ' +
  AUDIT_HEAD;
const REMAINING_WAVE = 'remaining backlog';

/** Ordered decision vocabulary used by the ledger. */
const DECISION_ORDER = [
  'enforced',
  'runtime-owned',
  'security-difference',
  'behavior-gap',
  'out-of-profile',
  'fail-closed',
];

/** Canonical one-line meaning of each decision, rendered in the legend. */
const DECISION_MEANING = {
  enforced:
    'the Rust data plane executes the directive and committed evidence asserts the behavior it reaches',
  'runtime-owned':
    'the nginx value has no client-visible effect because the Rust runtime owns that subsystem by design',
  'security-difference':
    'intentional stricter/safer behavior than nginx, justified by the declared security posture',
  'behavior-gap':
    'the value is client-visible behavior the runtime does not produce yet; it is scheduled in the remaining wave-3 backlog',
  'out-of-profile':
    'excluded from the declared http-core-v1 product profile (PRD section 4) and rejected in an active context',
  'fail-closed':
    'rejected at materialization and not planned, because the feature is outside the declared required surface or unsafe without separate requirements',
};

/**
 * Executed waves. Members are now `executable` in the catalog; the generator
 * fails closed if any of them regresses. Wave 3a is the charset/gzip-refinement
 * wave and wave 3a-2 is the autoindex/empty_gif/stub_status wave.
 */
const COMPLETED_WAVES = [
  {
    key: 'wave 1',
    summary: 'map, MIME types/default_type, gzip_static, gzip_vary',
    ids: [
      'http:map',
      'http:types',
      'http:default_type',
      'http:gzip_static',
      'http:gzip_vary',
    ],
  },
  {
    key: 'wave 2',
    summary:
      'redirect construction policies, limit status codes, max_ranges, chain-file consumption, typed TOML maps',
    ids: [
      'http:absolute_redirect',
      'http:port_in_redirect',
      'http:server_name_in_redirect',
      'http:limit_req_status',
      'http:limit_conn_status',
      'http:max_ranges',
    ],
  },
  {
    key: 'wave 2b',
    summary:
      'proxy response-header/cookie policy, internal locations, merge_slashes, request-header acceptance',
    ids: [
      'http:proxy_hide_header',
      'http:proxy_pass_header',
      'http:proxy_redirect',
      'http:proxy_cookie_path',
      'http:proxy_cookie_domain',
      'http:internal',
      'http:merge_slashes',
      'http:ignore_invalid_headers',
      'http:underscores_in_headers',
    ],
  },
  {
    key: 'wave 3a',
    summary:
      'charset family (header-level behaviour) and gzip refinements (gzip_disable, gzip_proxied)',
    ids: [
      'http:charset',
      'http:charset_types',
      'http:source_charset',
      'http:override_charset',
      'http:gzip_disable',
      'http:gzip_proxied',
    ],
  },
  {
    key: 'wave 3a-2',
    summary:
      'autoindex family (on/exact_size/localtime/format), empty_gif and stub_status location handlers',
    ids: [
      'http:autoindex',
      'http:autoindex_exact_size',
      'http:autoindex_localtime',
      'http:autoindex_format',
      'http:empty_gif',
      'http:stub_status',
    ],
  },
  {
    key: 'wave 3b',
    summary:
      'HTTP/2 stream ceiling, proxy_method, stream allow/deny and literal stream return',
    ids: [
      'http:http2_max_concurrent_streams',
      'http:proxy_method',
      'stream:allow',
      'stream:deny',
      'stream:return',
    ],
  },
  {
    key: 'wave 4a',
    summary:
      'geo and split_clients variable blocks plus the referer module (valid_referers / $invalid_referer)',
    ids: ['http:geo', 'http:split_clients', 'http:valid_referers'],
  },
  {
    key: 'wave 5 (upstream-protocol)',
    summary:
      'FastCGI/uWSGI/SCGI/memcached end-to-end implemented forms plus the bounded memcached-backed `set` variable form',
    ids: [
      'http:fastcgi_buffering',
      'http:fastcgi_connect_timeout',
      'http:fastcgi_hide_header',
      'http:fastcgi_index',
      'http:fastcgi_intercept_errors',
      'http:fastcgi_keep_conn',
      'http:fastcgi_next_upstream',
      'http:fastcgi_next_upstream_timeout',
      'http:fastcgi_next_upstream_tries',
      'http:fastcgi_param',
      'http:fastcgi_pass',
      'http:fastcgi_pass_header',
      'http:fastcgi_pass_request_headers',
      'http:fastcgi_read_timeout',
      'http:fastcgi_request_buffering',
      'http:fastcgi_send_timeout',
      'http:fastcgi_split_path_info',
      'http:memcached_connect_timeout',
      'http:memcached_next_upstream',
      'http:memcached_next_upstream_timeout',
      'http:memcached_next_upstream_tries',
      'http:memcached_pass',
      'http:memcached_read_timeout',
      'http:memcached_send_timeout',
      'http:scgi_buffering',
      'http:scgi_connect_timeout',
      'http:scgi_hide_header',
      'http:scgi_intercept_errors',
      'http:scgi_next_upstream',
      'http:scgi_next_upstream_timeout',
      'http:scgi_next_upstream_tries',
      'http:scgi_param',
      'http:scgi_pass',
      'http:scgi_pass_header',
      'http:scgi_pass_request_headers',
      'http:scgi_read_timeout',
      'http:scgi_request_buffering',
      'http:scgi_send_timeout',
      'http:set',
      'http:uwsgi_buffering',
      'http:uwsgi_connect_timeout',
      'http:uwsgi_hide_header',
      'http:uwsgi_intercept_errors',
      'http:uwsgi_modifier1',
      'http:uwsgi_modifier2',
      'http:uwsgi_next_upstream',
      'http:uwsgi_next_upstream_timeout',
      'http:uwsgi_next_upstream_tries',
      'http:uwsgi_param',
      'http:uwsgi_pass',
      'http:uwsgi_pass_header',
      'http:uwsgi_pass_request_headers',
      'http:uwsgi_read_timeout',
      'http:uwsgi_request_buffering',
      'http:uwsgi_send_timeout',
      'http:uwsgi_string',
    ],
  },
  {
    key: 'wave 6 (gRPC)',
    summary:
      'gRPC upstream execution: grpc_pass/h2 client, headers, timeouts, buffer, next-upstream, intercept and always-verified grpc_ssl_* forms',
    ids: [
      'http:grpc_buffer_size',
      'http:grpc_connect_timeout',
      'http:grpc_hide_header',
      'http:grpc_intercept_errors',
      'http:grpc_next_upstream',
      'http:grpc_next_upstream_timeout',
      'http:grpc_next_upstream_tries',
      'http:grpc_pass',
      'http:grpc_pass_header',
      'http:grpc_read_timeout',
      'http:grpc_send_timeout',
      'http:grpc_set_header',
      'http:grpc_ssl_certificate',
      'http:grpc_ssl_certificate_key',
      'http:grpc_ssl_name',
      'http:grpc_ssl_protocols',
      'http:grpc_ssl_server_name',
      'http:grpc_ssl_trusted_certificate',
      'http:grpc_ssl_verify',
      'http:grpc_ssl_verify_depth',
    ],
  },
  {
    key: 'stream/OCSP wave',
    summary:
      'stream limit_conn/zone/dry_run, bounded stream map/set, preread timeout and buffer sizing, and static OCSP stapling (ssl_stapling_file)',
    ids: [
      'http:ssl_stapling_file',
      'stream:limit_conn',
      'stream:limit_conn_zone',
      'stream:limit_conn_dry_run',
      'stream:map',
      'stream:set',
      'stream:preread_timeout',
      'stream:preread_buffer_size',
    ],
  },
  {
    key: 'wave 7A (CGI transport)',
    summary:
      'IP-literal *_bind and *_socket_keepalive for the FastCGI/uWSGI/SCGI/memcached transports (http/server/location inheritance)',
    ids: [
      'http:fastcgi_bind',
      'http:fastcgi_socket_keepalive',
      'http:uwsgi_bind',
      'http:uwsgi_socket_keepalive',
      'http:scgi_bind',
      'http:scgi_socket_keepalive',
      'http:memcached_bind',
      'http:memcached_socket_keepalive',
    ],
  },
  {
    key: 'wave 10 (rewrite/variable engine)',
    summary:
      'server/location `if` conditions (truthy, =, !=, ~, !~, ~*, !~*) over the bounded variable engine, with true bodies executing set/add_header/return/rewrite',
    ids: ['http:if'],
  },
  {
    key: 'mail waves (9A/9B)',
    summary:
      'POP3/IMAP/SMTP mail proxy end-to-end (auth_http, login/relay, STARTTLS/implicit TLS) after the PRD section 4 mail-module amendment',
    ids: [
      'mail:auth_http',
      'mail:auth_http_header',
      'mail:auth_http_timeout',
      'mail:imap_auth',
      'mail:imap_client_buffer',
      'mail:listen',
      'mail:mail',
      'mail:max_errors',
      'mail:pop3_auth',
      'mail:protocol',
      'mail:proxy',
      'mail:proxy_buffer',
      'mail:proxy_pass_error_message',
      'mail:proxy_protocol',
      'mail:proxy_smtp_auth',
      'mail:proxy_timeout',
      'mail:server',
      'mail:server_name',
      'mail:smtp_auth',
      'mail:smtp_client_buffer',
      'mail:smtp_greeting_delay',
      'mail:ssl_certificate',
      'mail:ssl_certificate_key',
      'mail:ssl_protocols',
      'mail:starttls',
      'mail:timeout',
      'mail:xclient',
    ],
  },
  {
    key: 'wave 7B (uWSGI upstream TLS)',
    summary:
      'uwsgi_ssl_* upstream TLS identity/verification at http/server/location through the CGI transport seam (tokio-rustls, always-verified unless ssl_verify is explicitly off)',
    ids: [
      'http:uwsgi_ssl_verify',
      'http:uwsgi_ssl_trusted_certificate',
      'http:uwsgi_ssl_certificate',
      'http:uwsgi_ssl_certificate_key',
      'http:uwsgi_ssl_server_name',
      'http:uwsgi_ssl_name',
      'http:uwsgi_ssl_protocols',
    ],
  },
  {
    key: 'wave 7C (CGI cache family)',
    summary:
      'fastcgi/uwsgi/scgi response-cache family at http/server/location on the shared HTTP cache store (Vary-aware refinement, single-flight lock, bounded min_uses counters)',
    ids: [
      'http:fastcgi_cache',
      'http:fastcgi_cache_background_update',
      'http:fastcgi_cache_bypass',
      'http:fastcgi_cache_key',
      'http:fastcgi_cache_lock',
      'http:fastcgi_cache_lock_age',
      'http:fastcgi_cache_lock_timeout',
      'http:fastcgi_cache_max_range_offset',
      'http:fastcgi_cache_methods',
      'http:fastcgi_cache_min_uses',
      'http:fastcgi_cache_path',
      'http:fastcgi_cache_revalidate',
      'http:fastcgi_cache_use_stale',
      'http:fastcgi_cache_valid',
      'http:fastcgi_no_cache',
      'http:scgi_cache',
      'http:scgi_cache_background_update',
      'http:scgi_cache_bypass',
      'http:scgi_cache_key',
      'http:scgi_cache_lock',
      'http:scgi_cache_lock_age',
      'http:scgi_cache_lock_timeout',
      'http:scgi_cache_max_range_offset',
      'http:scgi_cache_methods',
      'http:scgi_cache_min_uses',
      'http:scgi_cache_path',
      'http:scgi_cache_revalidate',
      'http:scgi_cache_use_stale',
      'http:scgi_cache_valid',
      'http:scgi_no_cache',
      'http:uwsgi_cache',
      'http:uwsgi_cache_background_update',
      'http:uwsgi_cache_bypass',
      'http:uwsgi_cache_key',
      'http:uwsgi_cache_lock',
      'http:uwsgi_cache_lock_age',
      'http:uwsgi_cache_lock_timeout',
      'http:uwsgi_cache_max_range_offset',
      'http:uwsgi_cache_methods',
      'http:uwsgi_cache_min_uses',
      'http:uwsgi_cache_path',
      'http:uwsgi_cache_revalidate',
      'http:uwsgi_cache_use_stale',
      'http:uwsgi_cache_valid',
      'http:uwsgi_no_cache',
    ],
  },
  {
    key: 'wave 7D (protocol response policy)',
    summary:
      'CGI buffer/store/limit_rate/force_ranges/ignore_headers/pass_request_body plus memcached buffer/gzip-flag and gRPC ignore_headers',
    ids: [
      'http:fastcgi_buffer_size',
      'http:fastcgi_buffers',
      'http:fastcgi_busy_buffers_size',
      'http:fastcgi_force_ranges',
      'http:fastcgi_ignore_headers',
      'http:fastcgi_limit_rate',
      'http:fastcgi_pass_request_body',
      'http:fastcgi_store',
      'http:fastcgi_store_access',
      'http:grpc_ignore_headers',
      'http:memcached_buffer_size',
      'http:memcached_gzip_flag',
      'http:scgi_buffer_size',
      'http:scgi_buffers',
      'http:scgi_busy_buffers_size',
      'http:scgi_force_ranges',
      'http:scgi_ignore_headers',
      'http:scgi_limit_rate',
      'http:scgi_pass_request_body',
      'http:scgi_store',
      'http:scgi_store_access',
      'http:uwsgi_buffer_size',
      'http:uwsgi_buffers',
      'http:uwsgi_busy_buffers_size',
      'http:uwsgi_force_ranges',
      'http:uwsgi_ignore_headers',
      'http:uwsgi_limit_rate',
      'http:uwsgi_pass_request_body',
      'http:uwsgi_store',
      'http:uwsgi_store_access',
    ],
  },
];

const COMPLETED_WAVE_IDS = new Set(COMPLETED_WAVES.flatMap((wave) => wave.ids));

/**
 * Remaining behavior-gap backlog after waves 1/2/2b/3a/3a-2/3b (the categories
 * named by specs/nginx-gap.catalog.json as remaining after wave 4a).
 *
 * Each id is either accepted-unenforced today (silently ignored) or
 * unsupported-fail-closed (rejected). Rejected entries are planned work here:
 * they block a stock nginx.conf from activating, so they are real compatibility
 * gaps, not "outside the profile".
 */
const REMAINING_GAP_CATEGORIES = [
  {
    key: 'chunked-framing',
    label: 'HTTP/1 framing policy',
    priority: 'medium (encoder blocker)',
    ids: ['http:chunked_transfer_encoding'],
    rejectedLoss:
      'the directive is accepted-but-ignored; the response keeps the runtime encoder framing.',
    why: 'the pinned hyper HTTP/1.1 encoder cannot emit a close-delimited HTTP/1.1 response (nginx probe recorded), so `chunked_transfer_encoding off` has no implementation path in the current encoder; the truthful diagnostic says so.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/http1_wire.rs (response framing), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'framing differential (chunked/identity/HEAD) once the encoder exposes close-delimited 1.1 responses; blocked until then',
  },
  {
    key: 'subrequest-engine',
    label: 're-entrant subrequest engine',
    priority: 'high (largest remaining item)',
    ids: [
      'http:add_before_body',
      'http:add_after_body',
      'http:addition_types',
      'http:auth_request',
      'http:auth_request_set',
      'http:mirror',
      'http:mirror_request_body',
    ],
    rejectedLoss:
      'a stock nginx.conf that declares a subrequest-based filter, auth check or mirror fails to activate.',
    why: 'these directives need a bounded re-entrant subrequest engine; the current engine only performs internal redirects and error-page hops, so a nested subrequest that itself proxies/serves cannot be bounded the way PRD section 6 requires.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/handler.rs (bounded subrequest engine + addition/auth/mirror filters), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'subrequest-depth/cycle unit tests plus differential addition, auth_request and mirror cases',
  },
  {
    key: 'module-sized',
    label: 'module-sized functional modules',
    priority: 'medium',
    ids: [
      'http:userid',
      'http:userid_domain',
      'http:userid_expires',
      'http:userid_flags',
      'http:userid_mark',
      'http:userid_name',
      'http:userid_p3p',
      'http:userid_path',
      'http:userid_service',
      'http:ancient_browser',
      'http:ancient_browser_value',
      'http:modern_browser',
      'http:modern_browser_value',
      'http:random_index',
      'http:degradation',
      'http:degrade',
      'http:slice',
    ],
    rejectedLoss:
      'a stock nginx.conf that uses this module fails to activate.',
    why: 'each is a self-contained module-sized feature (userid cookies, browser classification variables, random index selection, degradation limits, slice range caching); none needs a new engine, but each needs its own state/variable surface and differential coverage.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/handler.rs + static_file_response.rs (new module handlers), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'one differential slice per module against nginx 1.29.6 plus expiry/encoding unit tests for userid and slice',
  },
  {
    key: 'http2-chunking',
    label: 'HTTP/2 DATA chunking',
    priority: 'medium (blocked upstream)',
    ids: ['http:http2_chunk_size'],
    rejectedLoss:
      'a stock nginx.conf that tunes the HTTP/2 DATA-frame chunk size fails to activate.',
    why: 'the pinned hyper/h2 ingress exposes no DATA-frame chunk-size knob, so there is no runtime field to map it onto; every other HTTP/2 tuning directive is now executable, a nginx-obsolete accepted no-op, or runtime-owned ingress buffer sizing.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/server.rs (Hyper h2 builder) + data_plane/runtime.rs (limits struct), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'h2 DATA-chunking differential once the pinned hyper/h2 exposes the knob; blocked upstream until then',
  },
  {
    key: 'static-open-file-cache',
    label: 'static open-file cache',
    priority: 'medium (performance-only)',
    ids: [
      'http:open_file_cache',
      'http:open_file_cache_min_uses',
      'http:open_file_cache_valid',
    ],
    rejectedLoss:
      'the static-file descriptor cache policy is not applied; the runtime uses its own open policy.',
    why: 'the family is a performance cache whose only semantic contract is staleness bounds (serving stale data is allowed), so response correctness is unaffected and the gap is tuning parity rather than behaviour; the truthful catalog diagnostic says so.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/static_path.rs (fd cache), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'cache invalidation/TTL/threshold unit tests',
  },
  {
    key: 'ocsp',
    label: 'OCSP dynamic fetch / refresh / verification',
    priority: 'medium (architecture blocker, commercial impact bounded)',
    ids: [
      'http:ssl_stapling',
      'http:ssl_stapling_verify',
      'http:ssl_trusted_certificate',
      'stream:ssl_stapling',
      'stream:ssl_stapling_verify',
      'stream:ssl_trusted_certificate',
    ],
    rejectedLoss:
      'the directive is accepted-but-ignored; only the static ssl_stapling_file form is stapled, so no dynamically fetched/refreshed OCSP response or OCSP verification runs.',
    why: 'static OCSP stapling now executes via ssl_stapling_file (rustls CertifiedKey.ocsp, wire-proven); the dynamic fetch/refresh/verification path remains unimplemented and the pinned rustls stack exposes no fetch surface, so these directives stay accepted-unenforced with the blocker diagnostic.',
    surface:
      'crates/sdkwork-webserver-core/src/tls_runtime/, crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/tls.rs, certificate material distribution',
    tests: 'the static-file form is covered by tests/tls_ocsp_stapling.rs wire proof; dynamic fetch/refresh/verification needs a fetch owner and remains out of scope',
  },
  {
    key: 'stream',
    label: 'stream advanced gaps',
    priority: 'high',
    ids: [
      'stream:proxy_protocol_timeout',
      'stream:pass',
      'stream:server_name',
      'stream:proxy_bind',
      'stream:proxy_half_close',
      'stream:proxy_next_upstream_timeout',
      'stream:proxy_next_upstream_tries',
      'stream:proxy_requests',
      'stream:proxy_responses',
      'stream:proxy_download_rate',
      'stream:proxy_upload_rate',
      'stream:proxy_ssl',
      'stream:proxy_ssl_certificate',
      'stream:proxy_ssl_certificate_key',
      'stream:proxy_ssl_name',
      'stream:proxy_ssl_server_name',
      'stream:proxy_ssl_verify',
      'stream:proxy_ssl_trusted_certificate',
    ],
    rejectedLoss:
      'a stock stream sidecar that uses this stream-plane feature fails to activate.',
    why: 'the stream plane now executes access, limit_conn, variable map/set/return, limit and preread tuning, but still lacks an upstream TLS client (proxy_ssl identity/verification), half-close/rate/counting/retry controls, proxy_bind/new-connection selection, pass/SNI routing (pass, server_name) and the PROXY-protocol timeout; the HTTP plane executes the analogous policies.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/stream_proxy.rs, crates/sdkwork-webserver-core/src/config/model.rs (streams), src/nginx/mapping.rs',
    tests: 'stream_proxy.rs integration tests for upstream TLS identity, half-close, rates/counters/retry, bind and pass/SNI routing plus extended differential-stream cases against nginx 1.29.6',
  },
  {
    key: 'grpc-residual',
    label: 'gRPC residual forms',
    priority: 'low',
    ids: [
      'http:grpc_bind',
      'http:grpc_socket_keepalive',
      'http:grpc_ssl_certificate_cache',
      'http:grpc_ssl_ciphers',
      'http:grpc_ssl_conf_command',
      'http:grpc_ssl_crl',
      'http:grpc_ssl_password_file',
      'http:grpc_ssl_session_reuse',
    ],
    rejectedLoss:
      'the directive is rejected at load; a stock nginx.conf that uses this gRPC form fails to activate.',
    why: 'the gRPC h2 client now executes the request/response, header (including ignore_headers), timeout, buffer, retry, intercept and TLS-identity forms end-to-end against nginx 1.29.6; the residual bind and socket_keepalive blockers plus the rustls-unsupported TLS knobs (certificate_cache, ciphers, conf_command, crl, password_file, session_reuse) are unimplemented backlog.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cgi_upstream.rs (gRPC adapter), crates/sdkwork-webserver-upstream-protocol/src/grpc.rs (codec/limits), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'extend tests/nginx/differential-protocols with the residual bind/socket/TLS-library forms once the h2 client exposes them',
  },
  {
    key: 'fastcgi-residual',
    label: 'FastCGI residual forms',
    priority: 'medium',
    ids: [
      'http:fastcgi_catch_stderr',
      'http:fastcgi_ignore_client_abort',
      'http:fastcgi_max_temp_file_size',
      'http:fastcgi_send_lowat',
      'http:fastcgi_temp_file_write_size',
      'http:fastcgi_temp_path',
    ],
    rejectedLoss:
      'a stock nginx.conf that uses this FastCGI sub-family fails to activate.',
    why: 'the implemented FastCGI request/response, transport, cache and response-policy forms now execute end-to-end; the residual fail-closed forms are the temp-spool family (no bounded temp-spool subsystem), ignore_client_abort, catch_stderr and send_lowat.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cgi_upstream.rs, crates/sdkwork-webserver-upstream-protocol/src/fastcgi.rs + limits.rs, crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'extend tests/nginx/differential-protocols with cache/buffer/store/SSL and unix/variable-target cases',
  },
  {
    key: 'uwsgi-residual',
    label: 'uWSGI residual forms',
    priority: 'medium',
    ids: [
      'http:uwsgi_ignore_client_abort',
      'http:uwsgi_max_temp_file_size',
      'http:uwsgi_ssl_certificate_cache',
      'http:uwsgi_ssl_ciphers',
      'http:uwsgi_ssl_conf_command',
      'http:uwsgi_ssl_crl',
      'http:uwsgi_ssl_password_file',
      'http:uwsgi_ssl_session_reuse',
      'http:uwsgi_ssl_verify_depth',
      'http:uwsgi_temp_file_write_size',
      'http:uwsgi_temp_path',
    ],
    rejectedLoss:
      'a stock nginx.conf that uses this uWSGI sub-family fails to activate.',
    why: 'the implemented uWSGI request/response, transport, cache, response-policy and upstream TLS identity/verification forms now execute end-to-end; the residual fail-closed forms are the temp-spool family, ignore_client_abort and the rustls-unsupported SSL knobs (ciphers, CRL, session reuse, conf_command, password_file, certificate_cache, verify_depth).',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cgi_upstream.rs, crates/sdkwork-webserver-upstream-protocol/src/uwsgi.rs + limits.rs, crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'extend tests/nginx/differential-protocols with cache/buffer/store/SSL and unix/variable-target cases',
  },
  {
    key: 'scgi-residual',
    label: 'SCGI residual forms',
    priority: 'medium',
    ids: [
      'http:scgi_ignore_client_abort',
      'http:scgi_max_temp_file_size',
      'http:scgi_temp_file_write_size',
      'http:scgi_temp_path',
    ],
    rejectedLoss:
      'a stock nginx.conf that uses this SCGI sub-family fails to activate.',
    why: 'the implemented SCGI request/response, transport, cache and response-policy forms now execute end-to-end; the residual fail-closed forms are the temp-spool family and ignore_client_abort.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cgi_upstream.rs, crates/sdkwork-webserver-upstream-protocol/src/scgi.rs + limits.rs, crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'extend tests/nginx/differential-protocols with cache/buffer/store/SSL and unix/variable-target cases',
  },
  {
    key: 'mail-residual',
    label: 'mail module residuals',
    priority: 'medium',
    ids: [
      'mail:auth_http_pass_client_cert',
      'mail:imap_capabilities',
      'mail:pop3_capabilities',
      'mail:resolver',
      'mail:resolver_timeout',
      'mail:set_real_ip_from',
      'mail:smtp_capabilities',
      'mail:ssl_certificate_compression',
      'mail:ssl_ciphers',
      'mail:ssl_client_certificate',
      'mail:ssl_conf_command',
      'mail:ssl_crl',
      'mail:ssl_dhparam',
      'mail:ssl_ecdh_curve',
      'mail:ssl_password_file',
      'mail:ssl_prefer_server_ciphers',
      'mail:ssl_session_cache',
      'mail:ssl_session_ticket_key',
      'mail:ssl_session_tickets',
      'mail:ssl_session_timeout',
      'mail:ssl_trusted_certificate',
      'mail:ssl_verify_client',
      'mail:ssl_verify_depth',
    ],
    rejectedLoss:
      'the directive is rejected at load; a stock nginx `mail {}` sidecar that uses this TLS/client-cert/capability form fails to activate.',
    why: 'the mail module is now in profile and the implemented POP3/IMAP/SMTP planes execute end-to-end against nginx 1.29.6; the residual rustls-unsupported ssl_* policy knobs, client-certificate verification flows, capability lists, resolver/real-IP forms and auth_http client-cert pass-through are unimplemented backlog.',
    surface:
      'crates/sdkwork-webserver-mail-protocol, crates/sdkwork-api-webserver-standalone-gateway (mail listener), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'extend tests/nginx/differential-mail with client-certificate, capability-list, resolver and STARTTLS-policy cases once rustls/backend support lands',
  },

];

const REMAINING_GAP_BY_ID = new Map();
for (const category of REMAINING_GAP_CATEGORIES) {
  for (const id of category.ids) {
    if (REMAINING_GAP_BY_ID.has(id)) {
      throw new Error(`directive ${id} appears in two remaining-gap categories`);
    }
    REMAINING_GAP_BY_ID.set(id, category);
  }
}

/**
 * accepted-unenforced directives whose behavior is intentionally different for
 * security reasons. Every rationale is anchored in the TLS policy contract
 * (NGINX_SPEC.md section 3) or the documented hardening posture
 * (PRD-nginx-compatibility.md section 8.1).
 */
const SECURITY_DIFFERENCES = new Map([
  [
    'http:server_tokens',
    'intentional: the runtime emits no nginx-version Server banner (hyper adds none), so `server_tokens off` is always effectively in effect - stricter than the nginx default; PRD section 8.1 hardening.',
  ],
  [
    'http:disable_symlinks',
    'intentional: static path resolution opens every component with follow-symlinks disabled (data_plane/static_path.rs), so the runtime is always at least as strict as `disable_symlinks on`.',
  ],
  [
    'http:ssl_ciphers',
    'intentional: rustls policy-safe TLS defaults replace nginx cipher lists; legacy/broad cipher tokens are refused by design (NGINX_SPEC section 3).',
  ],
  [
    'http:ssl_prefer_server_ciphers',
    'intentional: cipher preference is owned by the policy-safe rustls configuration, not by the nginx flag (NGINX_SPEC section 3).',
  ],
  [
    'http:ssl_conf_command',
    'intentional: raw OpenSSL configuration commands are not applied; the runtime TLS stack exposes no OpenSSL escape hatch.',
  ],
  [
    'http:ssl_dhparam',
    'intentional: custom DH parameters are not applied; rustls does not use finite-field DH parameter files.',
  ],
  [
    'http:ssl_ecdh_curve',
    'intentional: the key-exchange group list is owned by the runtime TLS policy, not by the nginx curve list.',
  ],
  [
    'stream:ssl_ciphers',
    'intentional: rustls policy-safe TLS defaults replace nginx cipher lists on stream listeners (NGINX_SPEC section 3).',
  ],
  [
    'stream:ssl_prefer_server_ciphers',
    'intentional: cipher preference is owned by the policy-safe rustls configuration on stream listeners.',
  ],
  [
    'stream:ssl_conf_command',
    'intentional: raw OpenSSL configuration commands are not applied on stream listeners.',
  ],
  [
    'stream:ssl_dhparam',
    'intentional: custom DH parameters are not applied; rustls does not use them.',
  ],
  [
    'stream:ssl_ecdh_curve',
    'intentional: the key-exchange group list is owned by the runtime TLS policy.',
  ],
  [
    'stream:ssl_protocols',
    'intentional: the runtime TLS stack fixes the TLSv1.2/TLSv1.3 window on stream listeners and refuses legacy protocol tokens (NGINX_SPEC section 3).',
  ],
  [
    'http:proxy_ssl_ciphers',
    'intentional: upstream TLS cipher policy is owned by the typed TOML/rustls configuration; nginx cipher tokens are not applied (policy-safe defaults).',
  ],
  [
    'http:proxy_ssl_protocols',
    'intentional: upstream TLS protocol window is owned by the typed TOML/rustls configuration; nginx protocol tokens are not applied.',
  ],
  [
    'stream:proxy_ssl_ciphers',
    'intentional: upstream TLS cipher policy is owned by the typed TOML/rustls configuration (stream namespace).',
  ],
  [
    'stream:proxy_ssl_protocols',
    'intentional: upstream TLS protocol window is owned by the typed TOML/rustls configuration (stream namespace).',
  ],
]);

/**
 * Accepted-unenforced rows where the runtime owns the subsystem but the nginx
 * value is not a pure tuning knob; the rationale needs the explicit boundary.
 */
const RUNTIME_OWNED_NOTES = new Map([
  [
    'http:charset_map',
    'runtime-owned: charset/Content-Type translation is owned by the runtime at header level; byte-level transcoding through charset_map is deliberately out of scope, so the directive stays accepted-unenforced with a truthful diagnostic.',
  ],
]);

/** Out-of-profile groups: excluded by PRD-nginx-compatibility section 4. */
const OUT_OF_PROFILE_REASONS = [
  [
    /^ngx_http_perl_module$/,
    'embedded Perl execution is excluded by PRD section 4.',
  ],
  [
    /^ngx_http_geoip_module$/,
    'GeoIP database lookup is excluded by PRD section 4 (OS/database-specific surface; MaxMind database dependency).',
  ],
  [
    /^ngx_stream_geoip_module$/,
    'GeoIP database lookup is excluded by PRD section 4 (OS/database-specific surface; MaxMind database dependency).',
  ],
  [
    /^ngx_core_module$/,
    'dynamic third-party module loading is excluded by PRD section 4; the reference build is a static nginx OSS binary.',
  ],
];

/** Fail-closed groups: rejected at materialization and not planned. */
const FAIL_CLOSED_REASONS = [
  [
    /^ngx_http_proxy_module$/,
    'outside the covered proxy subset (cache controls, body rewriting, upstream trust knobs); rejected rather than approximated under PRD section 8 bounded policy.',
  ],
  [
    /^ngx_stream_proxy_module$/,
    'stream proxy buffer sizing, TLS library caches/commands/CRL/password files and pacing/counting knobs are not planned; upstream TLS identity is in the wave-3 backlog.',
  ],
  [
    /^ngx_http_grpc_module$/,
    'the residual gRPC bind/socket/TLS-library forms are tracked as remaining backlog, not as a profile exclusion.',
  ],
  [
    /^ngx_http_v2_module$/,
    'http2_chunk_size is tracked as wave-3 backlog (the pinned hyper/h2 ingress has no DATA-chunk knob); the module\'s other directives are executable, nginx-obsolete accepted no-ops, or runtime-owned ingress buffer sizing.',
  ],
  [
    /^ngx_http_ssl_module$/,
    'TLS library feature not implemented behind rustls (ECH, certificate compression, CRL/password-file, session-ticket-file, reject-handshake, cache); rejected at materialization.',
  ],
  [
    /^ngx_stream_ssl_module$/,
    'stream TLS library feature not implemented behind rustls (ECH, ALPN override, CRL/OCSP responder, password-file, session-ticket-file, reject-handshake, cache); rejected at materialization.',
  ],
  [
    /^ngx_http_v3_module$/,
    'HTTP/3 and QUIC are excluded by PRD section 4; a QUIC stack would be a separate transport product.',
  ],
  [
    /^ngx_http_userid_filter_module$/,
    'the userid cookie filter is tracked as wave-4 backlog (module-sized work).',
  ],
  [
    /^ngx_http_image_filter_module$/,
    'the image transformation filter is outside the profile (libgd dependency and image codecs).',
  ],
  [
    /^ngx_http_ssi_filter_module$/,
    'SSI is an embedded template language and is outside the profile (PRD section 4 embedded executable configuration).',
  ],
  [
    /^ngx_http_xslt_filter_module$/,
    'XSLT transformation is outside the profile (libxslt dependency).',
  ],
  [
    /^ngx_http_rewrite_module$/,
    '`break`/`rewrite_log`/`uninitialized_variable_warn` are outside the bounded rewrite state machine; `if`, `set`, `return`, `rewrite` and `try_files` execute.',
  ],
  [
    /^ngx_http_browser_module$/,
    'the browser/UA classification module is tracked as wave-4 backlog (module-sized work).',
  ],
  [
    /^ngx_http_dav_module$/,
    'WebDAV methods are outside the profile.',
  ],
  [
    /^ngx_http_mp4_module$/,
    'the mp4 pseudo-streaming filter is outside the profile.',
  ],
  [
    /^ngx_stream_core_module$/,
    'preread_timeout and proxy_protocol_timeout are in the wave-3 backlog; preread_buffer_size and stream server_name are not planned.',
  ],
  [
    /^ngx_http_addition_filter_module$/,
    'the body addition filter is tracked as wave-4 backlog (it needs a bounded re-entrant subrequest engine); any remaining form is not planned.',
  ],
  [
    /^ngx_http_headers_filter_module$/,
    'add_header executes with nginx inheritance; trailer and inheritance-mode controls are not in the covered response surface.',
  ],
  [
    /^ngx_http_gzip_filter_module$/,
    'internal gzip filter tuning; the runtime compression layer owns buffering and scheduling.',
  ],
  [
    /^ngx_http_referer_module$/,
    'valid_referers now executes; the referer hash-table sizing directives are not planned.',
  ],
  [
    /^ngx_stream_limit_conn_module$/,
    'the stream limit_conn family is tracked as wave-3 stream backlog.',
  ],
  [
    /^ngx_http_auth_request_module$/,
    'subrequest-based external auth is tracked as wave-4 backlog (bounded re-entrant subrequest engine).',
  ],
  [
    /^ngx_http_degradation_module$/,
    'the degradation module is tracked as wave-4 backlog (module-sized work).',
  ],
  [
    /^ngx_http_gunzip_filter_module$/,
    'on-the-fly gunzip is not implemented.',
  ],
  [
    /^ngx_http_mirror_module$/,
    'request mirroring is tracked as wave-4 backlog (bounded re-entrant subrequest engine).',
  ],
  [
    /^ngx_stream_access_module$/,
    'stream access ACLs are tracked as wave-3 stream backlog; the remaining rejected directives in this module are not separately planned.',
  ],
  [
    /^ngx_http_limit_conn_module$/,
    'the limit_conn dry-run mode is not implemented; the executable limit_conn behavior is covered separately.',
  ],
  [
    /^ngx_http_limit_req_module$/,
    'the limit_req dry-run mode is not implemented; the executable limit_req behavior is covered separately.',
  ],
  [
    /^ngx_http_log_module$/,
    'log targets and formats are owned by runtime observability.',
  ],
  [
    /^ngx_stream_log_module$/,
    'log targets and formats are owned by runtime observability.',
  ],
  [
    /^ngx_http_geo_module$/,
    'the geo variable table is a map-style feature; the remaining backlog covers stream maps, not this cluster.',
  ],
  [
    /^ngx_stream_geo_module$/,
    'the stream geo variable table is a map-style feature; the remaining backlog covers stream maps, not this cluster.',
  ],
  [
    /^ngx_http_random_index_module$/,
    'random index selection is tracked as wave-4 backlog (module-sized work).',
  ],
  [
    /^ngx_http_slice_filter_module$/,
    'slice range caching is tracked as wave-4 backlog (module-sized work).',
  ],
  [
    /^ngx_http_split_clients_module$/,
    'split_clients is a map-style variable table; the remaining backlog covers stream maps, not this cluster.',
  ],
  [
    /^ngx_stream_split_clients_module$/,
    'stream split_clients is a map-style variable feature that is not implemented; the planned stream variable work is the stream map.',
  ],
  [
    /^ngx_http_upstream_sticky_module$/,
    'cookie-sticky affinity is not implemented; hash/ip_hash/least_conn are the supported affinity policies.',
  ],
  [
    /^ngx_http_stub_status_module$/,
    'the nginx stub_status page is not implemented; the runtime exposes its own metrics surface.',
  ],
  [
    /^ngx_http_upstream_zone_module$/,
    'shared upstream memory zones are runtime-owned data structures.',
  ],
  [
    /^ngx_stream_upstream_zone_module$/,
    'shared stream upstream memory zones are runtime-owned data structures.',
  ],
  [
    /^ngx_stream_pass_module$/,
    'stream `pass` to a named target is not implemented and is not part of the wave-3 backlog.',
  ],
  [
    /^ngx_stream_return_module$/,
    'stream `return` is tracked as wave-3 stream backlog; other forms are not planned.',
  ],
  [
    /^ngx_stream_set_module$/,
    'stream variable assignment is tracked as wave-3 stream backlog; other forms are not planned.',
  ],
  [
    /^ngx_stream_realip_module$/,
    'stream PROXY-protocol realip rewriting is not implemented and is not part of the wave-3 backlog.',
  ],
  [
    /^ngx_core_module$/,
    'a debug-only nginx internal; not part of the runtime model.',
  ],
  [
    /^ngx_google_perftools_module$/,
    'gperftools profiling is a build-time option of the reference nginx binary.',
  ],
  [
    /^ngx_thread_pool_module$/,
    'nginx thread-pool configuration belongs to the nginx event model, which the runtime replaces.',
  ],
  [
    /^ngx_quic_bpf_module$/,
    'QUIC BPF routing is part of the excluded HTTP/3 transport.',
  ],
  [
    /^ngx_openssl_cache_module$/,
    'OpenSSL object-cache tuning belongs to the TLS library the runtime does not embed.',
  ],
  [
    /^ngx_http_empty_gif_module$/,
    'generated empty-GIF content is outside the profile.',
  ],
  [
    /^ngx_http_flv_module$/,
    'the FLV pseudo-streaming filter is outside the profile.',
  ],
];

/**
 * Extra sentence appended to the ledger rationale for gaps whose blocker is an
 * architecture limit rather than missing implementation effort.
 */
const GAP_RATIONALE_SUFFIX = new Map([
  [
    'http:ssl_stapling',
    ' Architecture blocker: static stapling executes via ssl_stapling_file; dynamic OCSP fetch/refresh/verification has no rustls surface.',
  ],
  [
    'http:ssl_stapling_verify',
    ' Architecture blocker: static stapling executes via ssl_stapling_file; dynamic OCSP fetch/refresh/verification has no rustls surface.',
  ],
  [
    'http:ssl_trusted_certificate',
    ' Architecture blocker: static stapling executes via ssl_stapling_file; dynamic OCSP fetch/refresh/verification has no rustls surface.',
  ],
  [
    'stream:ssl_stapling',
    ' Architecture blocker: static stapling executes via ssl_stapling_file; dynamic OCSP fetch/refresh/verification has no rustls surface.',
  ],
  [
    'stream:ssl_stapling_verify',
    ' Architecture blocker: static stapling executes via ssl_stapling_file; dynamic OCSP fetch/refresh/verification has no rustls surface.',
  ],
  [
    'stream:ssl_trusted_certificate',
    ' Architecture blocker: static stapling executes via ssl_stapling_file; dynamic OCSP fetch/refresh/verification has no rustls surface.',
  ],
  [
    'http:chunked_transfer_encoding',
    ' Architecture blocker: the pinned hyper HTTP/1.1 encoder cannot emit a close-delimited 1.1 response (nginx probe recorded).',
  ],
  [
    'http:open_file_cache',
    ' Contract note: this is a performance cache whose only semantic contract is staleness bounds; response correctness is unaffected.',
  ],
  [
    'http:open_file_cache_min_uses',
    ' Contract note: this is a performance cache whose only semantic contract is staleness bounds; response correctness is unaffected.',
  ],
  [
    'http:open_file_cache_valid',
    ' Contract note: this is a performance cache whose only semantic contract is staleness bounds; response correctness is unaffected.',
  ],
]);

/** Per-directive overrides for the fail-closed rationale where the module group is too broad. */
const FAIL_CLOSED_BY_ID = new Map([
  ['http:aio', 'OS asynchronous file I/O is not exposed by the runtime static path (Tokio + bounded buffering); rejected.'],
  ['http:aio_write', 'OS asynchronous file I/O is not exposed by the runtime static path; rejected.'],
  ['http:auth_delay', 'auth_delay timing hardening is not implemented; rejected rather than silently ignored.'],
  ['http:client_body_in_file_only', 'request bodies are streamed; temp-file-only body handling is rejected.'],
  ['http:client_body_in_single_buffer', 'request bodies are streamed with bounded buffering; single-buffer mode is rejected.'],
  ['http:client_body_temp_path', 'the runtime owns bounded body buffering and temp-path policy; rejected.'],
  ['http:early_hints', '103 Early Hints is not implemented in the covered HTTP surface.'],
  ['http:keepalive_min_timeout', 'connection lifetime is owned by the runtime keep-alive policy; the nginx minimum is rejected.'],
  ['http:limit_except', 'method-scoped ACL is outside the required V1 access-control surface (allow/deny + auth_basic execute); rejected.'],
  ['http:limit_rate', 'per-connection bandwidth throttling is outside the declared rate-limit surface (limit_req/limit_conn execute).'],
  ['http:limit_rate_after', 'per-connection bandwidth throttling is outside the declared rate-limit surface.'],
  ['http:open_file_cache_errors', 'the runtime owns static-file cache/open policy; rejected.'],
  ['http:open_file_cache_events', 'the runtime owns static-file cache/open policy; rejected.'],
  ['http:post_action', 'post-request subrequest hooks are outside the bounded rewrite/route state machine.'],
  ['http:satisfy', 'access-control composition (satisfy) is outside the covered access-control surface (allow/deny + auth_basic execute).'],
  ['http:subrequest_output_buffer_size', 'subrequest output buffering is outside the bounded response pipeline.'],
]);

/**
 * Verified spot-checks. Every row was read against the cited file: the cited
 * test/case exists and its assertions actually cover the claimed behavior.
 * Routing, static, proxy, cache, limits, TLS and stream are all represented.
 */
const VERIFIED_SPOT_CHECKS = [
  {
    id: 'core:include',
    evidence:
      'crates/sdkwork-webserver-core/tests/nginx_config_surface.rs::include_surface_expands_files_globs_and_nested_includes',
    claim: 'include expands files, globs and nested includes before materialization',
    proves:
      'asserts a glob match plus two nested expansions produce the expected directive sequence and args',
  },
  {
    id: 'http:alias',
    evidence: 'tests/nginx/full-surface/probe.py::case_alias_substitution',
    claim: 'alias performs prefix replacement for static lookup',
    proves: 'requests /docs/readme.md and asserts the body served from the aliased directory',
  },
  {
    id: 'http:allow',
    evidence: 'tests/nginx/full-surface/probe.py::case_allow_deny_allowed',
    claim: 'allow rules admit the matching client',
    proves: 'asserts HTTP 200 for the allowed location',
  },
  {
    id: 'http:deny',
    evidence: 'tests/nginx/full-surface/probe.py::case_deny_all',
    claim: 'deny rules reject the client',
    proves: 'asserts HTTP 403 for the blocked location',
  },
  {
    id: 'http:auth_basic',
    evidence: 'tests/nginx/full-surface/probe.py::case_auth_basic',
    claim: 'auth_basic challenges and validates credentials',
    proves:
      'asserts 401 without credentials, 401 on a wrong password, 200 with the htpasswd secret',
  },
  {
    id: 'http:error_page',
    evidence: 'tests/nginx/differential-host/battery.py::named.error-page-static-body',
    claim: 'error_page maps a status to a declared static body while preserving the status',
    proves:
      'real-nginx differential case compares status/headers/body for the error-page mapping',
  },
  {
    id: 'http:etag',
    evidence: 'tests/nginx/full-surface/probe.py::case_etag_off_suppresses_generation',
    claim: 'etag off suppresses ETag without touching Last-Modified',
    proves:
      'asserts no ETag, Last-Modified still present, concrete If-None-Match does not 304, `*` still does',
  },
  {
    id: 'http:expires',
    evidence: 'tests/nginx/full-surface/probe.py::case_expires_relative',
    claim: 'expires writes Cache-Control max-age and a matching Expires date',
    proves: 'asserts max-age=3600 and an Expires value ~3600s in the future',
  },
  {
    id: 'http:gzip',
    evidence: 'tests/nginx/full-surface/probe.py::case_gzip_large_text',
    claim: 'gzip compresses an eligible response and stays gzip-decodable',
    proves:
      'asserts Content-Encoding: gzip, body gzip-decodes, Content-Length matches',
  },
  {
    id: 'http:gzip_min_length',
    evidence: 'tests/nginx/full-surface/probe.py::case_gzip_below_min_length',
    claim: 'gzip_min_length suppresses compression below the threshold',
    proves: 'asserts no Content-Encoding for the small body',
  },
  {
    id: 'http:http2',
    evidence: 'tests/nginx/full-surface/probe.py::case_http2_alpn_settings',
    claim: 'http2 on an ssl listener negotiates ALPN h2',
    proves: 'completes an h2 SETTINGS exchange over the TLS listener',
  },
  {
    id: 'http:if_modified_since',
    evidence:
      'tests/nginx/full-surface/probe.py::case_if_modified_since_off_ignores_the_condition',
    claim: 'if_modified_since off ignores If-Modified-Since',
    proves:
      'asserts 200 with a future conditional date while the default `exact` path still evaluates it',
  },
  {
    id: 'http:index',
    evidence: 'tests/nginx/full-surface/probe.py::case_index_directive',
    claim: 'index serves the configured directory index',
    proves: 'asserts 200 and the index-file body for the directory URL',
  },
  {
    id: 'http:limit_conn',
    evidence: 'tests/nginx/full-surface/probe.py::case_limit_conn',
    claim: 'limit_conn rejects the excess concurrent connection',
    proves:
      'holds one request open and asserts the second gets 503 while the first completes 200',
  },
  {
    id: 'http:limit_req',
    evidence: 'tests/nginx/full-surface/probe.py::case_limit_req_burst',
    claim: 'limit_req with burst/nodelay passes the burst and rejects the rest',
    proves:
      'asserts a bounded pass count and at least two 503 rejections over 12 rapid requests',
  },
  {
    id: 'http:proxy_cache',
    evidence: 'tests/nginx/full-surface/probe.py::case_proxy_cache',
    claim: 'proxy_cache reuses the cached response for the same key',
    proves:
      'asserts upstream hit count increases by two for one cached hit plus one keyed miss',
  },
  {
    id: 'http:proxy_pass',
    evidence: 'tests/nginx/full-surface/probe.py::case_proxy_path_and_query_preserved',
    claim: 'proxy_pass without a URI part preserves the request path and query',
    proves: 'asserts the upstream-observed path and raw query equal the client request',
  },
  {
    id: 'http:proxy_pass_request_headers',
    evidence: 'tests/nginx/full-surface/probe.py::case_proxy_pass_request_headers_off',
    claim: 'proxy_pass_request_headers off strips client headers upstream',
    proves: 'asserts the client header is absent at the upstream',
  },
  {
    id: 'http:proxy_set_header',
    evidence: 'tests/nginx/full-surface/probe.py::case_proxy_set_header_server_level',
    claim: 'proxy_set_header at server level reaches the upstream',
    proves:
      'asserts the configured header value and the variable-expanded value upstream',
  },
  {
    id: 'http:proxy_intercept_errors',
    evidence: 'tests/nginx/full-surface/probe.py::case_proxy_intercept_errors',
    claim: 'proxy_intercept_errors replaces a mapped upstream error with the error page',
    proves:
      'asserts the mapped body is served with the upstream status while a control path passes through',
  },
  {
    id: 'http:rewrite',
    evidence: 'tests/nginx/full-surface/probe.py::case_rewrite_last_with_captures',
    claim: 'rewrite with captures rewrites the upstream path',
    proves: 'asserts the upstream-observed path equals the captured rewrite result',
  },
  {
    id: 'http:secure_link_secret',
    evidence: 'tests/nginx/full-surface/probe.py::case_secure_link_secret',
    claim: 'secure_link_secret validates the MD5 URI suffix',
    proves: 'asserts 200 for a valid digest and 403 for an invalid one',
  },
  {
    id: 'http:ssl_certificate',
    evidence: 'tests/nginx/full-surface/probe.py::case_tls_sni_certificate_selection',
    claim: 'ssl_certificate selects the SNI certificate',
    proves:
      'asserts distinct DER certificates and distinct bodies for two SNI hosts on one TLS listener',
  },
  {
    id: 'http:sub_filter',
    evidence: 'tests/nginx/full-surface/probe.py::case_sub_filter',
    claim: 'sub_filter rewrites the response body',
    proves: 'asserts the exact substituted body',
  },
  {
    id: 'http:try_files',
    evidence: 'tests/nginx/full-surface/probe.py::case_try_files_spa_fallback',
    claim: 'try_files falls back to the SPA shell while serving real files and directory indexes',
    proves: 'asserts fallback body, real-file body and directory index body',
  },
  {
    id: 'http:add_header',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/handler.rs::add_header_status_gating_matches_nginx',
    claim: 'add_header applies the nginx default status list and `always`',
    proves:
      'asserts header presence for the default status list and absence outside it unless `always`',
  },
  {
    id: 'http:server_name',
    evidence:
      'crates/sdkwork-webserver-core/tests/server_name_selection.rs::longest_leading_wildcard_matches_subdomains_of_any_depth',
    claim: 'server_name leading wildcards match at any depth and longest wins',
    proves: 'asserts the selected virtual host for competing wildcard names',
  },
  {
    id: 'http:client_max_body_size',
    evidence:
      'crates/sdkwork-webserver-core/tests/nginx_config_surface.rs::every_supported_directive_family_materializes (case "client_max_body_size materializes limits")',
    claim: 'client_max_body_size reaches the request-body limit',
    proves: 'asserts limits.max_request_body_bytes == 10 MiB',
  },
  {
    id: 'http:ssl_protocols',
    evidence:
      'crates/sdkwork-webserver-core/src/nginx/mapping.rs::ssl_protocols_declaration_selects_the_tls_window',
    claim: 'ssl_protocols narrows the TLS policy window',
    proves:
      'asserts both min and max TLS versions for TLSv1.3-only and TLSv1.2-only declarations',
  },
  {
    id: 'stream:proxy_pass',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/stream_proxy.rs::forwards_bytes_to_a_literal_tcp_target',
    claim: 'a stream listener forwards bytes to a literal TCP target',
    proves:
      'round-trips bytes through a real TCP echo upstream and asserts upstream acceptance count',
  },
  {
    id: 'stream:ssl_preread',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/stream_proxy.rs::ssl_preread_forwards_client_hello_preface_to_upstream',
    claim: 'ssl_preread forwards the ClientHello preface upstream',
    proves: 'asserts the upstream receives the unmodified ClientHello bytes',
  },
  {
    id: 'stream:ssl_certificate',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/stream_proxy.rs::terminates_stream_tls_and_forwards_plaintext_to_upstream',
    claim: 'stream ssl_certificate terminates TLS and forwards plaintext',
    proves:
      'performs a real TLS handshake against the stream listener and round-trips bytes to the plaintext upstream',
  },
  {
    id: 'http:map',
    evidence:
      'crates/sdkwork-webserver-core/src/config/map_variables.rs::hostname_precedence_is_exact_then_longest_head_then_longest_tail',
    claim: 'map hostname keys resolve exact, then longest leading wildcard, then longest trailing wildcard',
    proves:
      'asserts the precedence order over five competing hostname keys plus map value variable expansion',
  },
  {
    id: 'http:types',
    evidence: 'tests/nginx/differential-http/battery.py::mime.host-table',
    claim: 'types replaces the inherited MIME table at the declaring level',
    proves:
      'real-nginx differential: `.txt` resolves through the host table, not the inherited http table',
  },
  {
    id: 'http:default_type',
    evidence: 'tests/nginx/differential-http/battery.py::mime.location-inherits-default',
    claim: 'default_type applies to unknown extensions and inherits independently of `types`',
    proves:
      'differential: a location table without its own default_type inherits the host default for an unknown extension',
  },
  {
    id: 'http:gzip_static',
    evidence: 'tests/nginx/differential-http/battery.py::gz.static-on-gzip',
    claim: 'gzip_static serves the precompressed sidecar when gzip is accepted',
    proves:
      'differential: Content-Length/ETag/body are the sidecar representation and Content-Encoding is gzip',
  },
  {
    id: 'http:gzip_vary',
    evidence: 'tests/nginx/differential-http/battery.py::gz.dynamic-vary-off',
    claim: 'gzip_vary controls the Vary: Accept-Encoding negotiation field',
    proves:
      'differential: `vary off` strips the field even though the representation is compressed; the on cases assert it on both representations',
  },
  {
    id: 'http:absolute_redirect',
    evidence: 'tests/nginx/differential-http/battery.py::redirect.relative.dir',
    claim: 'absolute_redirect off emits a relative Location',
    proves:
      'differential: the directory redirect Location is relative under off and absolute under the default',
  },
  {
    id: 'http:port_in_redirect',
    evidence: 'tests/nginx/differential-http/battery.py::redirect.noport.dir',
    claim: 'port_in_redirect off omits the listener port from Location',
    proves: 'differential: the directory redirect Location carries no non-default port',
  },
  {
    id: 'http:server_name_in_redirect',
    evidence: 'tests/nginx/differential-http/battery.py::redirect.srvname.dir',
    claim: 'server_name_in_redirect on uses the first server_name as the Location host',
    proves:
      'differential: the Location host is the first server_name while the request Host is an alias',
  },
  {
    id: 'http:limit_req_status',
    evidence: 'tests/nginx/differential-http/battery.py::limit-req-status.first',
    claim: 'limit_req_status replaces the 503 rejection status',
    proves:
      'differential: the rate-limited response uses the configured 429 (with a handler unit test for location-then-host resolution)',
  },
  {
    id: 'http:limit_conn_status',
    evidence: 'tests/nginx/differential-http/battery.py::limit-conn-status',
    claim: 'limit_conn_status replaces the 503 rejection status',
    proves:
      'differential: the excess concurrent connection is rejected with the configured status under server-level inheritance',
  },
  {
    id: 'http:max_ranges',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/static_file_response.rs::max_ranges_caps_or_disables_range_processing',
    claim: 'max_ranges disables or caps range processing',
    proves:
      'unit test asserts 0 disables ranges and an over-limit multi-range request declines to a full 200; differential cases cover both',
  },
  {
    id: 'http:proxy_hide_header',
    evidence: 'tests/nginx/differential-http/battery.py::proxy.hide-header',
    claim: 'proxy_hide_header hides listed upstream response headers',
    proves:
      'differential: the hidden header is absent while the non-hidden set passes through, against nginx default-hidden semantics',
  },
  {
    id: 'http:proxy_pass_header',
    evidence: 'tests/nginx/differential-http/battery.py::proxy.pass-header',
    claim: 'proxy_pass_header removes a name from the hidden set',
    proves:
      'differential: the pass-listed header is forwarded although the default-hidden set would drop it',
  },
  {
    id: 'http:proxy_redirect',
    evidence: 'tests/nginx/differential-http/battery.py::proxy.redirect-default',
    claim: 'proxy_redirect rewrites upstream Location targets',
    proves:
      'differential: default and explicit forms rewrite Location and Refresh targets while `off` passes them through',
  },
  {
    id: 'http:proxy_cookie_path',
    evidence: 'tests/nginx/differential-http/battery.py::proxy.cookie-path',
    claim: 'proxy_cookie_path rewrites the Set-Cookie Path attribute',
    proves:
      'differential: the Path prefix is rewritten with the rest of the cookie attributes preserved',
  },
  {
    id: 'http:proxy_cookie_domain',
    evidence: 'tests/nginx/differential-http/battery.py::proxy.cookie-domain',
    claim: 'proxy_cookie_domain rewrites the Set-Cookie Domain attribute',
    proves:
      'differential: Domain rewrite is case-insensitive and preserves the leading dot',
  },
  {
    id: 'http:internal',
    evidence: 'tests/nginx/differential-http/battery.py::internal.external',
    claim: 'internal locations are unreachable from external requests',
    proves:
      'differential: an external hit answers the generated 404 (with the host error_page mapping) while rewrite last reaches the same location',
  },
  {
    id: 'http:merge_slashes',
    evidence: 'tests/nginx/differential-http/battery.py::merge.double-slash-does-not-match-exact',
    claim: 'merge_slashes off keeps repeated slashes in exact-location identity',
    proves:
      'differential: `/merge//exact` does not match the exact location while `/merge/exact` does',
  },
  {
    id: 'http:ignore_invalid_headers',
    evidence:
      'crates/sdkwork-webserver-core/src/nginx/mapping.rs::materializes_request_boundary_policies_with_inheritance',
    claim: 'ignore_invalid_headers materializes with nginx inheritance and defaults',
    proves:
      'asserts http-level inheritance, server-level override, and nginx default on for a host that declares none',
  },
  {
    id: 'http:underscores_in_headers',
    evidence: 'tests/nginx/differential-http/battery.py::headers.underscore-on-forwarded',
    claim: 'underscores_in_headers on forwards underscore header names',
    proves:
      'differential: the underscore header reaches the upstream under on, and is dropped under the nginx default off',
  },
  {
    id: 'http:charset',
    evidence: 'tests/nginx/differential-http/battery.py::charset.default-html',
    claim: 'charset appends the configured charset to eligible response types',
    proves:
      'differential: the built-in text types receive the configured charset while application/json, text/css and images are untouched; `charset off` at a location disables it',
  },
  {
    id: 'http:charset_types',
    evidence: 'tests/nginx/differential-http/battery.py::charset.declared-types-css',
    claim: 'charset_types replaces the built-in eligibility list and is seeded with text/html',
    proves:
      'differential: a declared list applies the charset to CSS and still to text/html, while JSON stays untouched; a location list replaces the server list',
  },
  {
    id: 'http:source_charset',
    evidence:
      'tests/nginx/differential-http/battery.py::charset.proxy-override-params-normalized',
    claim: 'source_charset participates in header-level charset normalisation',
    proves:
      'differential: an upstream Content-Type whose charset matches the configured source is normalised in place; a differing source needs transcoding and stays accepted-unenforced',
  },
  {
    id: 'http:override_charset',
    evidence:
      'tests/nginx/differential-http/battery.py::charset.proxy-override-normalizes',
    claim: 'override_charset normalises an equal upstream charset and leaves a differing one',
    proves:
      'differential: override on rewrites the upstream charset spelling when it equals the configured charset; a differing upstream charset (which needs charset_map transcoding) is kept',
  },
  {
    id: 'http:gzip_disable',
    evidence: 'tests/nginx/differential-http/battery.py::gzip.disable-ua-match',
    claim: 'gzip_disable disables compression for matching user agents',
    proves:
      'differential: a case-insensitive UA regex match suppresses gzip while a non-matching UA is compressed; the predefined msie6 selector and its Opera carve-out also match nginx',
  },
  {
    id: 'http:gzip_proxied',
    evidence: 'tests/nginx/differential-http/battery.py::gzip.proxied-any',
    claim: 'gzip_proxied gates compression for Via-marked proxied requests',
    proves:
      'differential: the nginx default off suppresses compression for proxied traffic, `any` accepts it, and the no-cache/no_last_modified/auth/expired flags match nginx',
  },
  {
    id: 'http:autoindex',
    evidence: 'tests/nginx/differential-http/battery.py::autoindex.html',
    claim: 'autoindex generates the nginx HTML directory listing',
    proves:
      'differential: byte-exact listing (directories first, hidden skipped, escaping, pinned sizes/mtimes), 301 for a missing trailing slash, HEAD without a body, and 403 when off',
  },
  {
    id: 'http:autoindex_exact_size',
    evidence: 'tests/nginx/differential-http/battery.py::autoindex.html-human-size',
    claim: 'autoindex_exact_size off renders nginx rounded K/M/G sizes',
    proves: 'differential: the listing column matches nginx human-size rounding',
  },
  {
    id: 'http:autoindex_localtime',
    evidence: 'tests/nginx/differential-http/battery.py::autoindex.html-localtime',
    claim: 'autoindex_localtime on renders the listing dates in local time',
    proves:
      'differential: the listing date column matches nginx with both sides pinned to TZ=UTC',
  },
  {
    id: 'http:autoindex_format',
    evidence: 'tests/nginx/differential-http/battery.py::autoindex.json',
    claim: 'autoindex_format renders html, json, jsonp and xml listings',
    proves:
      'differential: the JSON shape matches; jsonp validates the callback and downgrades to JSON without one; an invalid callback is 400; XML matches',
  },
  {
    id: 'http:empty_gif',
    evidence: 'tests/nginx/differential-http/battery.py::empty-gif.get',
    claim: 'empty_gif serves the canonical 1x1 transparent GIF',
    proves:
      'differential: the exact 43-byte image/gif body and fixed Last-Modified; HEAD keeps headers without a body and POST is 405',
  },
  {
    id: 'http:stub_status',
    evidence: 'tests/nginx/differential-http/battery.py::stub-status.get',
    claim: 'stub_status serves the nginx status page from runtime metrics',
    proves:
      'differential: field shape and plain-text headers match nginx (counters canonicalised as process-local); HEAD and 405 behaviour match; location-only and loopback-gated',
  },
  {
    id: 'http:http2_max_concurrent_streams',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/http2_tuning.rs::http2_settings_publish_the_configured_stream_ceiling',
    claim: 'http2_max_concurrent_streams reaches the advertised h2 SETTINGS ceiling',
    proves:
      'performs a real TLS/ALPN h2 handshake and parses the server SETTINGS frame, asserting SETTINGS_MAX_CONCURRENT_STREAMS equals the configured value (and that an oversized header list is rejected)',
  },
  {
    id: 'http:proxy_method',
    evidence: 'tests/nginx/differential-http/battery.py::proxy.method-override-post',
    claim: 'proxy_method overrides the upstream request method',
    proves:
      'real-nginx differential: the client GET is forwarded upstream as POST against nginx 1.29.6',
  },
  {
    id: 'stream:allow',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/stream_access.rs::stream_allow_first_match_proxies_and_deny_first_match_closes',
    claim: 'stream allow is an ordered first-match peer ACL',
    proves:
      'an allow-first rule proxies the loopback connection while a deny-first rule on the same peer closes it with no bytes',
  },
  {
    id: 'stream:deny',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/stream_access.rs::stream_allow_first_match_proxies_and_deny_first_match_closes',
    claim: 'stream deny closes the denied peer',
    proves:
      'the deny-first stream listener accepts and closes the connection without proxying, matching nginx stream access semantics',
  },
  {
    id: 'stream:return',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/stream_access.rs::stream_return_writes_the_literal_value_and_closes',
    claim: 'stream return writes the literal value and closes',
    proves:
      'a configured literal `return` value is written to the client and the connection closes (1..4096 bytes; variable values fail closed)',
  },
  {
    id: 'http:geo',
    evidence:
      'crates/sdkwork-webserver-core/src/config/geo.rs::geo_matches_longest_prefix_and_default',
    claim: 'geo resolves the longest matching CIDR/range entry and the default',
    proves:
      'unit test asserts longest-prefix selection, inclusive range bounds and the default fallback; differential cases cover loopback and X-Forwarded-For sources',
  },
  {
    id: 'http:split_clients',
    evidence:
      'crates/sdkwork-webserver-core/src/config/geo.rs::split_clients_uses_nginx_murmur_hash_and_buckets',
    claim: 'split_clients selects the nginx murmur-hash bucket',
    proves:
      'unit test pins ngx_murmur_hash2 bucket selection and fixed-point percentages against nginx 1.29.6 fixture keys',
  },
  {
    id: 'http:valid_referers',
    evidence:
      'crates/sdkwork-webserver-core/src/config/referer.rs::wildcards_paths_and_regexes',
    claim: 'valid_referers evaluates server names, wildcards, path prefixes and regexes for $invalid_referer',
    proves:
      'unit test asserts wildcard/path/regex matching and missing-header validity; differential cases compare the resolved $invalid_referer against nginx',
  },
  {
    id: 'http:fastcgi_pass',
    evidence: 'tests/nginx/differential-protocols/battery.py::basic-get',
    claim: 'fastcgi_pass proxies to a FastCGI responder with nginx-exact params and response',
    proves:
      'the real-nginx oracle and the Rust data plane return byte-identical status/headers/body for GET, POST upload, split-path-info, intercept, retry, 503 and HEAD cases with a deterministic mock FastCGI responder',
  },
  {
    id: 'http:uwsgi_pass',
    evidence: 'tests/nginx/differential-protocols/battery.py::uwsgi-basic',
    claim: 'uwsgi_pass proxies to a uWSGI responder',
    proves:
      'differential cases compare basic GET, POST upload, auto-generated CGI headers and intercept-404 against nginx 1.29.6',
  },
  {
    id: 'http:scgi_pass',
    evidence: 'tests/nginx/differential-protocols/battery.py::scgi-basic',
    claim: 'scgi_pass proxies to an SCGI responder',
    proves:
      'differential cases compare basic GET, POST upload, auto-generated CGI headers and intercept-404 against nginx 1.29.6',
  },
  {
    id: 'http:memcached_pass',
    evidence: 'tests/nginx/differential-protocols/battery.py::memcached-hit',
    claim: 'memcached_pass proxies a key lookup to a memcached responder',
    proves:
      'differential cases compare hit, HEAD, large value, miss, raw-error 502 and POST against nginx 1.29.6 with a deterministic mock memcached responder',
  },
  {
    id: 'http:set',
    evidence: 'tests/nginx/differential-protocols/battery.py::memcached-hit',
    claim: 'the bounded `set $memcached_key` form feeds the memcached adapter',
    proves:
      'the location-scoped set variable is consumed by memcached_pass in the hit/miss differential cases; other set variables/sources fail closed',
  },
  {
    id: 'http:grpc_pass',
    evidence: 'tests/nginx/differential-protocols/battery.py::grpc-unary',
    claim: 'grpc_pass proxies to a gRPC upstream over h2 with nginx-exact trailers and status mapping',
    proves:
      '13 differential cases against nginx 1.29.6 with a test-only Rust HTTP/2 mock: unary body+trailers byte-identical, trailers-only non-zero grpc-status kept as 200 headers (not 502), server streaming, set/hide/pass headers, read-timeout 504, retry, positive/negative verified TLS, intercept and missing-trailers cases',
  },
  {
    id: 'http:grpc_ssl_verify',
    evidence: 'tests/nginx/differential-protocols/battery.py::grpc-tls-untrusted',
    claim: 'grpc_ssl_verify runs against an always-verified upstream TLS client',
    proves:
      'the grpcs positive case succeeds with a trusted CA and the negative case fails with an unrelated CA, matching nginx; the runtime enforces full-chain verification even where nginx accepts a depth setting',
  },
  {
    id: 'http:ssl_stapling_file',
    evidence:
      'crates/sdkwork-api-webserver-standalone-gateway/tests/tls_ocsp_stapling.rs::ssl_stapling_file_is_served_and_absent_without_it',
    claim: 'ssl_stapling_file feeds a static OCSP response into the served certificate',
    proves:
      'a wire-level TLS handshake test serves the configured DER OCSP response in the CertificateStatus message and asserts it is absent when no file is configured (rustls CertifiedKey.ocsp)',
  },
  {
    id: 'stream:limit_conn',
    evidence: 'tests/nginx/differential-stream/battery.py::limit_conn.enforce',
    claim: 'stream limit_conn closes over-limit connections silently and releases the slot',
    proves:
      'the differential-stream slice compares per-connection outcomes against nginx 1.29.6: over-limit connections are closed with no bytes, the holder is proxied, and the slot is released after disconnect; dry-run is covered by a sibling case',
  },
  {
    id: 'stream:map',
    evidence: 'tests/nginx/differential-stream/battery.py::map.return-remote-addr',
    claim: 'stream map evaluates bounded accept-time facts into a variable consumed by return',
    proves:
      'differential-stream cases compare `map` over an accept-time fact and `map $ssl_preread_server_name` from a synthetic ClientHello feeding `return` byte-for-byte against nginx 1.29.6',
  },
  {
    id: 'http:fastcgi_bind',
    evidence: 'tests/nginx/differential-protocols/battery.py::fastcgi-bind',
    claim: 'fastcgi_bind pins the outbound transport to the configured IP literal',
    proves:
      'the differential-protocols binder case compares the peer address echoed by the mock responder against nginx 1.29.6; hostname and transparent binds fail closed with a located diagnostic',
  },
  {
    id: 'http:scgi_bind',
    evidence: 'tests/nginx/differential-protocols/battery.py::scgi-bind',
    claim: 'scgi_bind pins the outbound SCGI transport to the configured IP literal',
    proves:
      'the SCGI binder differential case compares the peer address echoed by the mock responder against nginx 1.29.6',
  },
  {
    id: 'http:memcached_bind',
    evidence: 'tests/nginx/differential-protocols/battery.py::bind',
    claim: 'memcached_bind pins the outbound memcached transport to the configured IP literal',
    proves:
      'the memcached binder differential case compares the peer address observed by the mock responder against nginx 1.29.6',
  },
  {
    id: 'http:if',
    evidence: 'tests/nginx/differential-http/battery.py::rewrite.if-truthy-header',
    claim: '`if` executes a truthy-header condition and runs the true-body directives',
    proves:
      'real-nginx differential cases compare the truthy, false and regex header conditions; a true body executes set/add_header/return/rewrite while a false body does not run',
  },
  {
    id: 'http:if',
    evidence: 'tests/nginx/differential-http/battery.py::rewrite.if-regex-header',
    claim: '`if` supports regex conditions and captures over the bounded variable engine',
    proves:
      'the regex-header differential case asserts the same response/header outcome as nginx 1.29.6 for a `~` condition (with the known if-body proxy_pass/root/error_page bounded differences documented)',
  },
  {
    id: 'mail:auth_http',
    evidence: 'tests/nginx/differential-mail/battery.py::pop3.login.relay',
    claim: 'mail auth_http drives the POP3/IMAP/SMTP auth exchange against a backend',
    proves:
      'the 9-case differential-mail slice compares the client transcript, backend authentication sequence and every auth_http request header against real nginx 1.29.6 for login/relay, auth failures, SMTP AUTH+XCLIENT, implicit TLS and STARTTLS',
  },
  {
    id: 'mail:proxy',
    evidence: 'tests/nginx/differential-mail/battery.py::smtp.auth-xclient.relay',
    claim: 'the mail proxy relays the client session to the backend after auth',
    proves:
      'the SMTP AUTH+XCLIENT case compares the relayed commands, XCLIENT client host and backend authentication sequence against nginx 1.29.6',
  },
  {
    id: 'mail:starttls',
    evidence: 'tests/nginx/differential-mail/battery.py::smtp_starttls',
    claim: 'mail starttls upgrades the session without re-greeting and continues AUTH',
    proves:
      'the STARTTLS case compares greeting, EHLO capabilities, the 220 upgrade, decrypted application bytes and the post-upgrade AUTH exchange against nginx 1.29.6',
  },
  {
    id: 'http:uwsgi_ssl_verify',
    evidence: 'tests/nginx/differential-protocols/battery.py::uwsgi-tls-ok',
    claim: 'uwsgi_ssl_verify verifies the upstream TLS peer against the trusted CA',
    proves:
      'the uWSGI TLS differential cases authenticate the mock responder with a trusted CA (success), an unrelated CA (fail closed with a status-only 502), and mTLS client identity, against nginx 1.29.6',
  },
  {
    id: 'http:uwsgi_ssl_certificate',
    evidence: 'tests/nginx/differential-protocols/battery.py::uwsgi-tls-mtls',
    claim: 'uwsgi_ssl_certificate/_key present the client identity to the upstream',
    proves:
      'the mTLS differential case completes the upstream handshake with the configured client certificate/key and matches nginx 1.29.6 behaviour',
  },
  {
    id: 'http:fastcgi_cache',
    evidence: 'tests/nginx/differential-protocols/battery.py::cache-hit',
    claim: 'fastcgi_cache serves a hit from the shared HTTP cache store',
    proves:
      'the cache differential compares the FastCGI cache hit/miss body (`gen=<n>`) against nginx 1.29.6; the sibling `cache-default` case proves nginx has no default caching time until valid/Cache-Control/Expires allows it',
  },
  {
    id: 'http:fastcgi_cache_valid',
    evidence: 'tests/nginx/differential-protocols/battery.py::cache-valid',
    claim: 'fastcgi_cache_valid sets the per-status caching time',
    proves:
      'the cache-valid case compares status-based expiry (and the no-default case) against nginx 1.29.6',
  },
  {
    id: 'http:fastcgi_cache_key',
    evidence: 'tests/nginx/differential-protocols/battery.py::cache-key',
    claim: 'fastcgi_cache_key selects the cache entry key',
    proves:
      'the custom-key case proves two requests with different keys are cached separately and match nginx 1.29.6',
  },
  {
    id: 'http:fastcgi_store',
    evidence: 'tests/nginx/differential-protocols/battery.py::d7-store',
    claim: 'fastcgi_store atomically writes the upstream response to the nginx path template',
    proves:
      'the d7-store case compares the stored path, content and file mode (644) byte-for-byte against nginx 1.29.6, with the store root confined by the typed storeRoot allowlist',
  },
  {
    id: 'http:fastcgi_limit_rate',
    evidence: 'tests/nginx/differential-protocols/battery.py::d7-rate',
    claim: 'fastcgi_limit_rate applies a bounded token-bucket read rate',
    proves:
      'the d7-rate case compares the throttled transfer profile (20KB body at a 20k window) against nginx 1.29.6',
  },
  {
    id: 'http:fastcgi_force_ranges',
    evidence: 'tests/nginx/differential-protocols/battery.py::d7-range-multipart',
    claim: 'fastcgi_force_ranges enables single/suffix/multipart range handling with nginx decline semantics',
    proves:
      'the d7-range-single/suffix/multipart/invalid cases compare 206/416 and multi-range decline decisions against nginx 1.29.6',
  },
];

/**
 * The evidence citations reported as over-claims in the previous audit snapshot
 * and the stronger committed evidence that replaced them. `--verify-evidence`
 * fails if a catalog entry regresses back to the old citation.
 */
const RESOLVED_EVIDENCE_OVERCLAIMS = [
  {
    id: 'http:location',
    oldCitation:
      'crates/sdkwork-webserver-core/tests/nginx_config_surface.rs::every_supported_directive_family_materializes (case "upstream weighted round-robin with keepalive")',
    oldFragment: 'upstream weighted round-robin with keepalive',
    replacement:
      'the same file now cites case "location matchers: exact, ^~, prefix, ~, ~*", which asserts route matchers.',
  },
  {
    id: 'http:ssl_protocols',
    oldCitation:
      'crates/sdkwork-webserver-core/src/nginx/mapping.rs::materializes_ssl_listener_with_certificates',
    oldFragment: 'materializes_ssl_listener_with_certificates',
    replacement:
      'now cites ssl_protocols_declaration_selects_the_tls_window, which asserts both min and max TLS versions.',
  },
  {
    id: 'stream:ssl_certificate',
    oldCitation: 'tests/nginx/full-surface/probe.py::case_tls_sni_certificate_selection',
    oldFragment: 'case_tls_sni_certificate_selection',
    replacement:
      'now cites stream_proxy.rs::terminates_stream_tls_and_forwards_plaintext_to_upstream, a real stream TLS handshake.',
  },
  {
    id: 'stream:ssl_certificate_key',
    oldCitation: 'tests/nginx/full-surface/probe.py::case_tls_sni_certificate_selection',
    oldFragment: 'case_tls_sni_certificate_selection',
    replacement:
      'now cites stream_proxy.rs::terminates_stream_tls_and_forwards_plaintext_to_upstream.',
  },
  {
    id: 'stream:ssl_client_certificate',
    oldCitation:
      'crates/sdkwork-webserver-core/tests/nginx_config_surface.rs::every_supported_directive_family_materializes (case "ssl_verify_client with ssl_client_certificate")',
    oldFragment: 'ssl_verify_client with ssl_client_certificate',
    replacement:
      'now cites case "stream ssl terminate with client auth", which asserts stream ClientAuthMode::Required plus the CA list.',
  },
  {
    id: 'stream:ssl_verify_client',
    oldCitation:
      'crates/sdkwork-webserver-core/tests/nginx_config_surface.rs::every_supported_directive_family_materializes (case "ssl_verify_client with ssl_client_certificate")',
    oldFragment: 'ssl_verify_client with ssl_client_certificate',
    replacement: 'now cites case "stream ssl terminate with client auth".',
  },
];

/**
 * The migration-action over-claims reported previously and how they were
 * resolved. The catalog now derives `importer.tomlEnforced` from actual
 * materializer consumption, so these are tracked as resolved expectations in
 * `--verify-evidence` rather than as current findings.
 */
const RESOLVED_MIGRATION_OVERCLAIMS = [
  { id: 'core:error_log', key: 'errorLog', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'core:pid', key: 'pid', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'core:user', key: 'user', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'core:worker_processes', key: 'workerProcesses', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'events:worker_connections', key: 'workerConnections', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'http:error_log', key: 'errorLog', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'http:keepalive', key: 'keepalive', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'http:sendfile', key: 'sendfile', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'http:tcp_nodelay', key: 'tcpNodelay', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  { id: 'http:tcp_nopush', key: 'tcpNopush', resolution: 'tomlEnforced is now false (accepted-but-ignored diagnostic).' },
  {
    id: 'http:ssl_trusted_certificate',
    key: 'chainFile',
    resolution:
      'chainFile is now consumed end-to-end by the certificate materializer (wave 2), so the "enforced typed key" action is truthful.',
  },
];

/** Typed keys whose consumption `--verify-evidence` re-checks directly. */
const CONSUMED_TYPED_KEYS = new Map([
  ['chainFile', 'crates/sdkwork-webserver-core/src/config/server_toml.rs'],
]);

const REQUIRED_SECTIONS = [
  'Executive Summary',
  'Decision Ledger By Module',
  'Wave Completion And Remaining Behavior-Gap Backlog',
  'Enforced Spot-Check',
  'Why Full Nginx Parity Is Not A Coherent Target',
  'Reproduction And Change Control',
];

function fail(message) {
  throw new Error(message);
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function sha256(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function escapeCell(value) {
  return String(value).replace(/\r?\n/g, ' ').replace(/\|/g, '\\|');
}

function shortEvidence(entry) {
  const first = entry.evidence?.[0];
  if (!first) return 'no evidence reference';
  return first.test ? `${first.file}::${first.test}` : `${first.file}::${first.case}`;
}

function completedWaveFor(id) {
  return COMPLETED_WAVES.find((wave) => wave.ids.includes(id)) ?? null;
}

function failClosedReason(entry) {
  const byId = FAIL_CLOSED_BY_ID.get(entry.id);
  if (byId) return byId;
  const group = FAIL_CLOSED_REASONS.find(([matcher]) => matcher.test(entry.module));
  if (group) return group[1];
  return 'rejected at materialization; not part of the declared http-core-v1 required surface and no compatibility work is planned.';
}

function outOfProfileReason(entry) {
  const group = OUT_OF_PROFILE_REASONS.find(([matcher]) => matcher.test(entry.module));
  if (group) return group[1];
  return 'excluded from the http-core-v1 product profile (PRD section 4) and rejected in an active context.';
}

function gapLoss(entry) {
  const category = REMAINING_GAP_BY_ID.get(entry.id);
  if (entry.migrationImpact) return entry.migrationImpact;
  return category?.rejectedLoss ?? 'the directive is currently rejected at load.';
}

function decisionFor(entry) {
  if (entry.classification === 'executable') return 'enforced';
  if (entry.classification === 'excluded') return 'out-of-profile';
  if (entry.classification === 'unsupported-fail-closed') {
    return REMAINING_GAP_BY_ID.has(entry.id) ? 'behavior-gap' : 'fail-closed';
  }
  if (entry.classification === 'accepted-unenforced') {
    if (REMAINING_GAP_BY_ID.has(entry.id)) return 'behavior-gap';
    if (SECURITY_DIFFERENCES.has(entry.id)) return 'security-difference';
    return 'runtime-owned';
  }
  return fail(`unknown catalog classification for ${entry.id}: ${entry.classification}`);
}

function rationaleFor(entry, decision) {
  const impact = entry.migrationImpact ?? entry.diagnostics ?? '';
  switch (decision) {
    case 'enforced': {
      const runtimeField = String(entry.runtimeField ?? 'no runtime field').replaceAll('`', "'");
      let text = `enforced: the value reaches \`${runtimeField}\`; evidence ${shortEvidence(entry)}.`;
      if (entry.knownDifferences?.length) {
        text += ` Known difference: ${entry.knownDifferences.join('; ')}.`;
      }
      return text;
    }
    case 'runtime-owned': {
      const note = RUNTIME_OWNED_NOTES.get(entry.id);
      if (note) return note;
      const normalized = impact.replace(/^none:\s*/, '').replace(/\.$/, '');
      return `runtime-owned: ${normalized || 'the Rust runtime owns this subsystem by design'}.`;
    }
    case 'security-difference':
      return SECURITY_DIFFERENCES.get(entry.id) ?? fail(`missing security-difference rationale for ${entry.id}`);
    case 'behavior-gap': {
      const category = REMAINING_GAP_BY_ID.get(entry.id);
      if (!category) fail(`missing remaining-gap category for ${entry.id}`);
      const state =
        entry.classification === 'unsupported-fail-closed'
          ? 'currently rejected at load; '
          : '';
      const loss = gapLoss(entry).replace(/\.$/, '');
      const suffix = GAP_RATIONALE_SUFFIX.get(entry.id) ?? '';
      return `behavior-gap (${REMAINING_WAVE}, ${category.label}): ${state}${loss}. Not runtime-owned: no runtime policy substitutes the client-visible effect.${suffix}`;
    }
    case 'out-of-profile':
      return `out-of-profile: ${outOfProfileReason(entry)}`;
    case 'fail-closed':
      return `fail-closed: ${failClosedReason(entry)}`;
    default:
      return fail(`unknown decision ${decision}`);
  }
}

function buildRows(inventory, catalog) {
  if (catalog.directives.length !== inventory.directiveCount) {
    fail(`catalog has ${catalog.directives.length} directives, inventory declares ${inventory.directiveCount}`);
  }
  const inventoryById = new Map(inventory.directives.map((entry) => [entry.id, entry]));
  const rows = catalog.directives.map((entry) => {
    const source = inventoryById.get(entry.id);
    if (!source) fail(`catalog entry ${entry.id} is absent from the inventory`);
    if (source.name !== entry.name) fail(`name mismatch for ${entry.id}`);
    if (!source.contexts || source.contexts.length === 0) fail(`no contexts for ${entry.id}`);
    const decision = decisionFor(entry);
    return {
      id: entry.id,
      name: entry.name,
      group: entry.group,
      module: entry.module,
      contexts: source.contexts,
      classification: entry.classification,
      risk: entry.risk,
      runtimeField: entry.runtimeField ?? '',
      evidence: entry.evidence ?? [],
      migrationImpact: entry.migrationImpact ?? '',
      decision,
      gapCategory: decision === 'behavior-gap' ? REMAINING_GAP_BY_ID.get(entry.id).label : '',
      rationale: rationaleFor(entry, decision),
    };
  });
  if (rows.length !== 744) fail(`ledger has ${rows.length} rows, expected 744`);
  const seen = new Set();
  const moduleNames = new Set();
  for (const row of rows) {
    if (seen.has(row.id)) fail(`duplicate ${row.id}`);
    seen.add(row.id);
    moduleNames.add(`${row.module}::${row.name}`);
  }
  if (moduleNames.size !== rows.length) fail('module-qualified directive names are not unique');
  for (const wave of COMPLETED_WAVES) {
    for (const id of wave.ids) {
      const row = rows.find((candidate) => candidate.id === id);
      if (!row) fail(`${wave.key} references unknown directive ${id}`);
      if (row.decision !== 'enforced') fail(`${id} is recorded in ${wave.key} but decision is ${row.decision}`);
      if (row.classification !== 'executable') fail(`${id} is recorded in ${wave.key} but classification is ${row.classification}`);
    }
  }
  for (const id of REMAINING_GAP_BY_ID.keys()) {
    const row = rows.find((candidate) => candidate.id === id);
    if (!row) fail(`remaining-gap set references unknown directive ${id}`);
    if (row.decision !== 'behavior-gap') fail(`${id} is in the remaining-gap set but decision is ${row.decision}`);
  }
  for (const id of SECURITY_DIFFERENCES.keys()) {
    const row = rows.find((candidate) => candidate.id === id);
    if (!row) fail(`security-difference set references unknown directive ${id}`);
    if (row.decision !== 'security-difference') fail(`${id} is in security-difference set but decision is ${row.decision}`);
  }
  const sum = DECISION_ORDER.reduce((total, decision) => total + countRows(rows, decision), 0);
  if (sum !== rows.length) fail(`decision counts sum to ${sum}, expected ${rows.length}`);
  return rows;
}

function countRows(rows, decision) {
  return rows.filter((row) => row.decision === decision).length;
}

function countClassification(rows, classification) {
  return rows.filter((row) => row.classification === classification).length;
}

function renderHeader(inventory, catalog) {
  const inventorySha = catalog.inventory?.digest ?? sha256(INVENTORY_PATH);
  const catalogSha = sha256(CATALOG_PATH);
  return `# Nginx Directive-by-Directive Audit (\`${PROFILE}\`) - ${AUDIT_DATE}

<!-- Generated by tools/gen-nginx-directive-audit.mjs. Do not edit by hand. -->
<!-- Regenerate with: node tools/gen-nginx-directive-audit.mjs -->

| Field | Value |
| --- | --- |
| Audit date | ${AUDIT_DATE} |
| Repository | \`sdkwork-webserver\`, branch \`${AUDIT_BRANCH}\`, HEAD \`${AUDIT_HEAD}\` |
| Snapshot state | ${SNAPSHOT_STATE} |
| Compatibility profile | \`${PROFILE}\` |
| Nginx reference | \`nginx ${inventory.source.version}\` (${inventory.referenceBuild.name}) |
| Directive inventory | \`specs/nginx-directive-inventory.json\` - ${inventory.directiveCount} directives, sha256 \`${inventorySha}\` |
| Support catalog | \`specs/nginx-directive-support.catalog.json\` - sha256 \`${catalogSha}\` |
| Catalog evidence revision | ${catalog.evidenceRevision} |
| Decision model | 6 decisions (see legend); every inventory row appears exactly once |
| Rows | ${inventory.directiveCount} |

## Status And Authority

- The committed catalog is the authority for this audit. It classifies each directive
  \`executable\`, \`accepted-unenforced\`, \`unsupported-fail-closed\`, or \`excluded\`; this
  audit adds the operator-facing **decision** for every row. No directive is claimed
  implemented unless the catalog says \`executable\` and the evidence reference was
  verified. Behavior-gap rows that the catalog still rejects are marked as such: they
  are planned work, not current support.
- **This snapshot is the uncommitted worktree refresh after Waves 1, 2, 2b, 3a,
  3a-2, 3b, 4a, the upstream-protocol wave (wave 5), the gRPC wave (wave 6), the
  stream/OCSP wave, the CGI-transport wave (wave 7A), the rewrite/variable-engine
  wave (wave 10), the mail waves (9A/9B), the uWSGI upstream-TLS wave (wave 7B) and the
  CGI cache-family wave (wave 7C) and the protocol response-policy wave (wave 7D).** The code, tests, generated operator matrix and the re-synced directive
  catalog were produced by those waves and are not committed at HEAD
  \`${AUDIT_HEAD}\`; the audit is regenerated against that post-wave catalog (evidence
  revision ${catalog.evidenceRevision}). Wave 1 (map, MIME types/default_type,
  gzip_static, gzip_vary), Wave 2 (redirect policies, limit status codes, max_ranges,
  chain-file consumption, typed TOML maps), Wave 2b (proxy response-header/cookie
  policy, internal locations, merge_slashes, request-header acceptance), Wave 3a
  (charset family header-level behaviour, gzip_disable/gzip_proxied) and Wave 3a-2
  (autoindex family, empty_gif, stub_status), Wave 3b (h2 stream ceiling,
  proxy_method, stream allow/deny and literal return), Wave 4a (geo and
  split_clients variable blocks plus valid_referers), the upstream-protocol wave
  (FastCGI/uWSGI/SCGI/memcached implemented forms plus the bounded memcached-backed
  \`set\` form), the gRPC wave (grpc_pass/h2 client with headers, timeouts, buffer,
  retry, intercept and always-verified grpc_ssl_* forms), the stream/OCSP wave
  (stream limit_conn/map/set/preread tuning plus static ssl_stapling_file), the
  CGI-transport wave (IP-literal *_bind and *_socket_keepalive) and the
  rewrite/variable-engine wave (\`if\` conditions over the bounded variable engine) and
  the mail waves (POP3/IMAP/SMTP proxy end-to-end after the PRD section 4 mail-module
  amendment) and the uWSGI upstream-TLS wave (uwsgi_ssl_* identity/verification through
  the CGI transport seam), the CGI cache-family wave (shared HTTP cache store with a
  CGI key/policy layer) and the protocol response-policy wave (buffers, store, limit_rate,
  force_ranges, ignore_headers, pass_request_body) are recorded as completed in section 3,
  and every one of them is asserted executable by \`--verify-evidence\`.
- **The ledger, the counts, and the operator matrix must be regenerated whenever the
  catalog changes.** The catalog and
  \`docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md\` are generated/checked by
  \`tools/check-nginx-directive-matrix.mjs\`; this audit embeds the catalog sha256, so
  \`--check\` fails when the catalog moves. The remaining backlog in section 3.2
  is the only behavior-gap set that still moves to \`enforced\`; when it does, rerun
  \`node tools/check-nginx-directive-matrix.mjs --write\` and then this generator, and
  commit the regenerated report with the catalog change.
- Method: the 744-row ledger is derived programmatically from
  \`specs/nginx-directive-inventory.json\` + \`specs/nginx-directive-support.catalog.json\`.
  The audit decisions for the accepted-unenforced and rejected groups are explicit,
  reviewable sets in \`tools/gen-nginx-directive-audit.mjs\` (completed-wave sets,
  remaining-backlog categories, security-difference set, runtime-owned fallback
  rationale from \`migrationImpact\`). Evidence was spot-checked by reading the cited
  test files; \`node tools/gen-nginx-directive-audit.mjs --verify-evidence\` re-checks
  citation existence, the typed-TOML enforcement claims, the replacement of the
  previously reported weak citations, and the completed-wave classifications offline.
`;
}

function renderExecutiveSummary(rows) {
  const total = rows.length;
  const decisionCounts = DECISION_ORDER.map((decision) => [decision, countRows(rows, decision)]);
  const arithmetic = decisionCounts.map(([, count]) => count).join(' + ');
  const completedCount = COMPLETED_WAVE_IDS.size;
  const remainingCount = countRows(rows, 'behavior-gap');
  return `### 1.1 Counts Per Decision Category

| Decision | Count | Share | Meaning |
| --- | ---: | ---: | --- |
${decisionCounts
  .map(
    ([decision, count]) =>
      `| \`${decision}\` | ${count} | ${((count / total) * 100).toFixed(1)}% | ${DECISION_MEANING[decision]} |`,
  )
  .join('\n')}
| **total** | **${total}** | **100%** | every inventory row appears exactly once |

**Sum proof:** ${arithmetic} = **${total}**. The generator fails closed if the sum is not
${total}, if any row is duplicated, or if a (\`module\`, \`name\`) pair is repeated.

### 1.2 Catalog Classification x Audit Decision

The audit decision is derived from, but not identical to, the catalog classification:
\`accepted-unenforced\` is split into \`runtime-owned\`, \`security-difference\`, and
\`behavior-gap\`; \`unsupported-fail-closed\` is split into \`fail-closed\` and the
wave-3 backlog entries that are currently rejected.

| Catalog classification | Rows | ${DECISION_ORDER.map((decision) => `\`${decision}\``).join(' | ')} |
| --- | ---: | ${DECISION_ORDER.map(() => '---:').join(' | ')} |
${['executable', 'accepted-unenforced', 'unsupported-fail-closed', 'excluded']
  .map((classification) => {
    const rowCount = countClassification(rows, classification);
    const cells = DECISION_ORDER.map(
      (decision) =>
        rows.filter((row) => row.classification === classification && row.decision === decision)
          .length,
    );
    return `| \`${classification}\` | ${rowCount} | ${cells.join(' | ')} |`;
  })
  .join('\n')}
| **total** | **${total}** | ${DECISION_ORDER.map(
    (decision) => countRows(rows, decision),
  ).join(' | ')} |

### 1.3 Wave Completion Status

| Wave | Directives | State |
| --- | ---: | --- |
${COMPLETED_WAVES.map(
  (wave) => `| \`${wave.key}\` | ${wave.ids.length} | done - ${wave.summary} (catalog: executable) |`,
).join('\n')}
| ${REMAINING_WAVE} | ${remainingCount} | open - ${REMAINING_GAP_CATEGORIES.length} categories, see section 3.2 |
| **total planned work** | **${completedCount + remainingCount}** | ${completedCount} completed, ${remainingCount} remaining |

Every completed-wave directive is asserted \`executable\` by
\`node tools/gen-nginx-directive-audit.mjs --verify-evidence\`; a regression in any of
them fails the gate.

### 1.4 Ledger By Nginx Group

| Group | Directives | enforced | runtime-owned | security-difference | behavior-gap | out-of-profile | fail-closed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
${['core', 'events', 'http', 'mail', 'stream']
  .map((group) => {
    const groupRows = rows.filter((row) => row.group === group);
    const cells = DECISION_ORDER.map((decision) =>
      String(groupRows.filter((row) => row.decision === decision).length),
    );
    return `| \`${group}\` | ${groupRows.length} | ${cells.join(' | ')} |`;
  })
  .join('\n')}
| **total** | **${total}** | ${DECISION_ORDER.map(
    (decision) => countRows(rows, decision),
  ).join(' | ')} |

### 1.5 Headline Findings

1. **${countRows(
    rows,
    'enforced',
  )} of ${total} directives (${((countRows(rows, 'enforced') / total) * 100).toFixed(
    1,
  )}%) are executed by the Rust data plane today**, up from 79 at the first audit snapshot:
   Waves 1, 2, 2b, 3a, 3a-2, 3b, 4a, the upstream-protocol wave, the gRPC wave, the stream/OCSP wave, the CGI-transport wave, the rewrite/variable-engine wave, the mail waves, the uWSGI upstream-TLS wave, the CGI cache-family wave and the protocol response-policy wave landed ${completedCount} directives. ${countRows(
     rows,
     'runtime-owned',
   )} more are deliberately runtime-owned (${(
     (countRows(rows, 'runtime-owned') / total) *
     100
   ).toFixed(1)}%) and ${countRows(
     rows,
     'security-difference',
   )} are intentional, spec-justified security differences.
2. **${remainingCount} behavior-gap directives remain in the remaining backlog** (${(
    (remainingCount / total) *
    100
  ).toFixed(1)}% of the surface): ${REMAINING_GAP_CATEGORIES.map(
    (category) => `${category.label} (${category.ids.length})`,
  ).join(', ')}.
3. **${countRows(rows, 'out-of-profile') + countRows(
    rows,
    'fail-closed',
  )} of ${total} directives (${(
    ((countRows(rows, 'out-of-profile') + countRows(rows, 'fail-closed')) / total) *
    100
  ).toFixed(1)}%) are excluded or rejected, not silently emulated.** ${countRows(
    rows,
    'out-of-profile',
  )} are explicit PRD section 4 profile exclusions and ${countRows(
    rows,
    'fail-closed',
  )} are fail-closed rejections outside the planned backlog.
4. **The catalog's previously reported over-claims are resolved.** All ${RESOLVED_EVIDENCE_OVERCLAIMS.length}
   evidence-citation mismatches were replaced with the stronger committed tests and
   all ${RESOLVED_MIGRATION_OVERCLAIMS.length} "enforced typed key" migration actions now
   reflect actual materializer consumption; the generator gates both classes so they
   cannot silently regress (section 4.3).
5. **Regenerating this report is mandatory when the catalog moves.** It is a generated
   snapshot of the post-protocol-response-policy-wave worktree, not a hand-maintained table; section 6 states
   the exact commands and the remaining-backlog landing procedure.
`;
}

function renderDecisionLedger(rows) {
  const groupOrder = ['core', 'events', 'http', 'mail', 'stream'];
  const sections = [];
  for (const group of groupOrder) {
    const groupRows = rows.filter((row) => row.group === group);
    if (groupRows.length === 0) continue;
    const title =
      group === 'http'
        ? 'HTTP (`http`)'
        : group === 'core'
          ? 'Core (`core`)'
          : group === 'events'
            ? 'Events (`events`)'
            : group === 'mail'
              ? 'Mail (`mail`)'
              : 'Stream (`stream`)';
    sections.push(`### ${title} - ${groupRows.length} directives`);
    const modules = [...new Set(groupRows.map((row) => row.module))].sort();
    for (const module of modules) {
      const moduleRows = groupRows.filter((row) => row.module === module);
      sections.push('');
      const moduleLabel = moduleRows.length === 1 ? 'directive' : 'directives';
      sections.push(`#### \`${module}\` (${moduleRows.length} ${moduleLabel})`);
      sections.push('');
      sections.push('| Directive | Contexts | Catalog | Decision | Rationale |');
      sections.push('| --- | --- | --- | --- | --- |');
      for (const row of moduleRows) {
        sections.push(
          `| \`${row.name}\` | ${row.contexts.join(', ')} | ${row.classification} | **${row.decision}** | ${escapeCell(row.rationale)} |`,
        );
      }
    }
    sections.push('');
  }
  return sections.join('\n');
}

function renderWaveAndBacklog(rows) {
  const completedTable =
    '| Wave | Directive | Runtime field reached | Evidence |\n| --- | --- | --- | --- |';
  const completedRows = COMPLETED_WAVES.flatMap((wave) =>
    wave.ids.map((id) => {
      const row = rows.find((candidate) => candidate.id === id);
      const evidence = shortEvidence({ evidence: row.evidence });
      const runtimeField = escapeCell(String(row.runtimeField || 'n/a').replaceAll('`', "'"));
      return `| \`${wave.key}\` | \`${row.name}\` (\`${id}\`) | \`${runtimeField}\` | \`${escapeCell(evidence)}\` |`;
    }),
  ).join('\n');
  const backlogHeader =
    '| Directive | Priority | Catalog state | Operator loss | Why it is a real gap (not runtime-owned) | Estimated implementation surface | Tests to add |';
  const backlogRule = '| --- | --- | --- | --- | --- | --- | --- |';
  const backlogSections = REMAINING_GAP_CATEGORIES.map((category) => {
    const categoryRows = category.ids.map((id) => rows.find((row) => row.id === id));
    return `### 3.2.${REMAINING_GAP_CATEGORIES.indexOf(category) + 1} ${category.label} - ${
      category.ids.length
    } directive(s)

${backlogHeader}
${backlogRule}
${categoryRows
  .map(
    (row) =>
      `| \`${row.name}\` | ${category.priority} | ${row.classification} | ${escapeCell(gapLoss(row))} | ${category.why} | ${category.surface} | ${category.tests} |`,
  )
  .join('\n')}
`;
  }).join('\n');
  return `## 3. Wave Completion And Remaining Behavior-Gap Backlog

### 3.1 Completed Waves (1, 2, 2b, 3a, 3a-2, 3b, 4a, upstream-protocol, gRPC, stream/OCSP, CGI-transport, rewrite/variable-engine, mail 9A/9B, wave 7B, wave 7C, wave 7D)

The catalog now classifies every directive below as \`executable\` with committed
evidence. Wave 1 was the routing/static correctness set; Wave 2 added redirect
construction, limit status codes, range ceilings and typed-TOML map/chain materialization;
Wave 2b closed the proxy response-header policy, internal locations, slash merging and
request-header acceptance; Wave 3a added the header-level charset family and the gzip
user-agent/proxied refinements; Wave 3a-2 added the autoindex listing family plus the
empty_gif and stub_status location handlers; Wave 3b added the advertised HTTP/2
stream ceiling, proxy_method, stream allow/deny ACLs and the literal stream return;
Wave 4a added the geo and split_clients variable blocks and the referer module
(valid_referers / $invalid_referer); the upstream-protocol wave moved FastCGI, uWSGI,
SCGI and memcached from out-of-profile to executable for their implemented
request/response forms and added the bounded memcached-backed \`set\` variable form;
the gRPC wave added the h2 upstream client with header/timeout/buffer/retry/intercept
and always-verified grpc_ssl_* forms (trailers forwarded and non-zero grpc-status
following nginx rather than mapping to 502); the stream/OCSP wave added stream
limit_conn/zone/dry_run, the bounded stream map/set variable engine, preread timeout and
buffer sizing, and static OCSP stapling via ssl_stapling_file (rustls CertifiedKey.ocsp,
wire-proven); the CGI-transport wave added IP-literal *_bind and *_socket_keepalive for
the FastCGI/uWSGI/SCGI/memcached adapters (http/server/location inheritance); and the
rewrite/variable-engine wave made server/location \`if\` conditions executable; and the
mail waves brought the POP3/IMAP/SMTP mail proxy in profile (PRD section 4 amendment) with
the 27 implemented mail-module directives executing end-to-end; and the uWSGI
upstream-TLS wave made the seven uwsgi_ssl_* identity/verification directives executable;
and the CGI cache-family wave made 45 fastcgi/uwsgi/scgi response-cache directives
executable on the shared HTTP cache store; and the protocol response-policy wave added the
CGI buffer/store/limit_rate/force_ranges/ignore_headers/pass_request_body forms, memcached
buffer/gzip-flag and gRPC ignore_headers.

${completedTable}
${completedRows}

Wave 10 also landed engine capabilities that are not directive rows but are part of the
claim: a bounded template grammar (\`$name\`, \`\${name}\`, \`$1..$9\`), server/location \`set\`
with nginx phase ordering, regex captures from location/if matches, request-time
variable \`proxy_pass\` resolving to declared upstreams, and variable \`root\`/\`alias\`
confined by the typed \`[http] documentRootAllowlist\`. Acceptance evidence: the real
production nginx snapshot committed at
\`tests/nginx/real-world/production-api-birdcoder\` (api.birdcoder.com, nginx/1.28.0,
33 files) now materializes cleanly (virtualHosts=41, listeners=4, resources=150,
upstreams=88, skipped=0), enforced by the aggregate-gate step
\`tools/check-nginx-real-world.mjs\`; the synthetic differential-http slice grew to
265/265 against nginx 1.29.6 (6 rewrite-engine cases).

Mail waves acceptance evidence: the new \`tests/nginx/differential-mail\` slice (9 cases,
the sixth aggregate differential slice) compares the mail proxy against real nginx 1.29.6
for POP3/IMAP/SMTP login and relay, auth failures, SMTP AUTH+XCLIENT, implicit TLS and
STARTTLS, including the auth_http request headers and backend command transcripts. One
scope caveat is explicit: frontend implicit/STARTTLS TLS executes, and backend TLS is a
typed-plane extension (nginx mail has no proxy_ssl), proven by integration tests only -
the audit does not claim nginx parity for backend TLS, and client-certificate verification
is absent.

Wave 7B acceptance evidence: uWSGI upstream TLS is dialed through the committed CGI
transport seam with tokio-rustls and proven by three new real-nginx differential cases
(trusted CA, mTLS client identity, untrusted CA -> 502 status-only), taking
differential-protocols from 46 to 49 (it remains the second aggregate differential
slice). TLS is always verified unless uwsgi_ssl_verify is explicitly off (a documented
dangerous verifier). Rustls-unsupported OpenSSL forms stay fail-closed. Factual note:
nginx 1.29.6 has **no** scgi_ssl_* and **no** fastcgi_ssl_* directives (inventory: 14
uwsgi_ssl, 0 scgi_ssl), so those names are rejected as non-nginx extensions and are not
counted as nginx gaps.

Wave 7C acceptance evidence: the 45 fastcgi/uwsgi/scgi response-cache directives execute on
the shared HTTP cache store with a CGI-specific key/policy layer (Vary-aware refinement,
single-flight lock, bounded min_uses counters), proven by 16 new cache cases in
differential-protocols (49 -> 65; it remains the second aggregate differential slice):
hit, no-default-caching, bypass, no_cache, valid-by-status, custom key, min_uses, lock
coalescing, use_stale http_500, conditional revalidate, HEAD->GET convert_head, three
range/max_range_offset decisions, and uWSGI/SCGI cache hits. **nginx has no default caching
time**: the task brief's "10m/1h defaults" are not nginx defaults, and the
differential \`cache-default\` case proves nginx caches nothing until
\`*_cache_valid\`/\`Cache-Control\`/\`Expires\` says so.

Wave 7D acceptance evidence: 30 protocol response-policy directives execute, proven by 12
new differential-protocols cases (65 -> 77; floor/manifest 77, still the second aggregate
slice): atomic store with nginx path templates and \`[http] storeRoot\` confinement (stored
path/content/mode 644 compared byte-for-byte), token-bucket \`limit_rate\` (20KB at a 20k
window), \`force_ranges\` 206/416/multi-range-decline semantics, the recognised
\`ignore_headers\` set (X-Accel-Redirect), \`pass_request_body off\` suppressing STDIN/body
while preserving CONTENT_LENGTH, memcached gzip-flag bitwise Content-Encoding, gRPC
ignore_headers, and oversized-header-block -> 502 (bounded adapter buffer like nginx).

The wave notes also record bounded remaining differences (for example map evaluation
timing, orphan \`.gz\` sidecars, regex proxy_redirect/cookie forms, and the nginx
pre-vhost-selection \`merge_slashes\` quirk); those are documented in each catalog entry's
\`knownDifferences\` and are not counted as backlog rows.

### 3.2 Remaining Behavior-Gap Backlog (${countRows(
    rows,
    'behavior-gap',
  )} directives)

These are the categories \`specs/nginx-gap.catalog.json\` lists as remaining after
the protocol response-policy wave (7D). An accepted-unenforced row is silently ignored today; a rejected row fails
the configuration load. Both are real migration gaps, so every row carries the
operator-visible loss, the reason it is **not** runtime-owned, the files that would
have to change, and the tests to add.

${backlogSections}
### 3.3 Backlog Sequencing Note

Within the backlog, the largest surface is still the residual sub-families of the four
landed protocol modules (102 directives: cache/buffer/store/SSL, unix/variable targets
and connection-pool knobs), followed by the bounded **re-entrant subrequest engine**
(addition filter, \`auth_request\`, mirror). The stream advanced gaps remain the next
transport item even after this wave: an upstream TLS client, half-close, rate limits,
request/response counting, retry budget, proxy_bind and pass/SNI routing are still
missing. The module-sized userid/browser/random_index/degradation/slice modules and the
nine gRPC residual bind/socket/TLS-library knobs come next. \`chunked_transfer_encoding\`
and the static open-file cache are precisely bounded (hyper HTTP/1.1 close-delimited
framing; staleness-only performance cache), \`http2_chunk_size\` is blocked on hyper/h2,
and the OCSP rows are now limited to dynamic fetch/refresh/verification (static
ssl_stapling_file executes) because rustls exposes no fetch surface.

Wave 7A also landed two non-directive transport capabilities recorded here for operators:
(a) typed \`[http] unixSocketRoot\` with strict-descendant confinement for
\`unix:/absolute\` upstream targets - nginx-conf \`unix:\` pass targets intentionally
fail closed with a located diagnostic (typed-TOML only by design); and (b) a bounded
per-generation FastCGI keep_conn connection pool (one in-flight request, idle caps and
deadline, dead-socket probe) with a framing fix that keeps pooled sockets aligned.
Remaining 7A blockers: gRPC bind/keepalive, hostname/transparent binds, uWSGI/SCGI/
memcached connection reuse, TLS-over-unix (wave 7B) and nginx-plane unix targets.

Wave 10 bounded gaps (documented known differences, not backlog rows): if-body
\`proxy_pass\`/\`root\`/\`error_page\` fail closed; multiple matched if blocks that set
\`add_header\` are approximated by the last applied set; dynamic-proxy retries rebuild a
static target (the expanded path is not preserved); per-route \`client_max_body_size\` is
enforced on \`Content-Length\` only (chunked bodies are bounded by the app max); regex SNI
selection still needs its own differential slice; and regexes are compiled per request
(bounded compile budget).

Mail waves residual note: the mail module is in profile and its implemented planes
execute, but 23 mail directives remain backlog - rustls-unsupported ssl_* policy knobs
(ciphers, conf_command, crl, dhparam, ecdh_curve, password_file, prefer_server_ciphers,
session cache/timeout/tickets/ticket_key, certificate_compression), client-certificate
flows (ssl_verify_client/ssl_verify_depth/ssl_client_certificate/ssl_trusted_certificate/
auth_http_pass_client_cert), capability lists (smtp/imap/pop3_capabilities),
resolver/resolver_timeout and set_real_ip_from. Client-certificate verification remains
absent, and backend TLS stays a typed-plane extension rather than a claimed nginx parity
surface.

Wave 7B note: uWSGI upstream TLS identity/verification now executes (trusted CA, client
certificate/key, SNI/name override, TLS1.2/1.3 window) with inheritance at
http/server/location; the seven rustls-unsupported uWSGI SSL forms (ciphers, CRL, session
reuse, conf_command, password_file, certificate_cache, verify_depth) stay fail-closed
backlog. nginx 1.29.6 defines no scgi_ssl_* or fastcgi_ssl_* directives, so those names
are non-nginx extensions rather than missing nginx features and are deliberately not
listed as gaps.

Wave 7C bounded differences (documented known differences, not backlog rows): (a)
\`*_cache_lock_age\` cannot preempt an in-flight fill (the store waits or serves as its
policy allows); (b) \`*_cache_background_update\` serves stale instead of spawning a
detached task; (c) \`X-Accel-Expires\` is not consulted - only \`Cache-Control\` and \`Expires\`
drive freshness; (d) \`inactive=\` is accepted but eviction is LRU/object-cap based rather
than a strict inactive timer; (e) one process cache store backs every declared zone, and
only the first zone provides a disk tier; (f) a CGI cache may reference a
\`proxy_cache_path\` zone, a documented superset of nginx. nginx has no default caching
time, and the runtime matches that (no implicit TTL).

Wave 7D note: the 30 response-policy directives now execute and the remaining fail-closed
protocol residuals are reported as behavior-gap with concrete blockers: the temp-spool
family (\`*_max_temp_file_size\`/\`*_temp_file_write_size\`/\`*_temp_path\`, no bounded
temp-spool subsystem), \`*_ignore_client_abort\`, \`fastcgi_catch_stderr\`,
\`fastcgi_send_lowat\`, the \`*_store on\` value form, the seven uWSGI TLS leftovers and the
gRPC bind/keepalive/TLS leftovers. Carried bounded difference: \`*_buffering off\` /
\`*_request_buffering off\` still buffer (a body-streaming seam is required), documented in
the catalog knownDifferences rather than as a directive-level exclusion.

Two former backlog rows are **not** remaining gaps and stay accepted-unenforced with
truthful diagnostics: \`charset_map\` (byte-level transcoding is deliberately out of
scope; the runtime owns charset translation at header level) and \`gzip_comp_level\` (the
compression layer owns the level).
`;
}

function renderSpotChecks(rows) {
  const enforced = rows.filter((row) => row.decision === 'enforced');
  const materializationOnly = enforced.filter(
    (row) =>
      row.evidence.length > 0 &&
      row.evidence.every(
        (item) =>
          item.file ===
            'crates/sdkwork-webserver-core/tests/nginx_config_surface.rs' &&
          item.test === 'every_supported_directive_family_materializes',
      ),
  );
  const verifiedRows = VERIFIED_SPOT_CHECKS.map((check) => {
    const row = rows.find((candidate) => candidate.id === check.id);
    if (!row) fail(`spot-check references unknown directive ${check.id}`);
    const claim = escapeCell(check.claim.replaceAll('`', "'"));
    return `| \`${row.id}\` | \`${claim}\` | \`${escapeCell(check.evidence)}\` | ${check.proves} | verified |`;
  }).join('\n');
  const decisionProof =
    rows
      .filter((row) => row.decision === 'enforced')
      .every((row) => row.classification === 'executable') &&
    rows
      .filter((row) => row.classification === 'executable')
      .every((row) => row.decision === 'enforced');
  const resolvedEvidence = RESOLVED_EVIDENCE_OVERCLAIMS.map(
    (item) =>
      `| \`${item.id}\` | \`${item.oldCitation}\` | ${item.replacement} | resolved |`,
  ).join('\n');
  const resolvedMigration = RESOLVED_MIGRATION_OVERCLAIMS.map(
    (item) =>
      `| \`${item.id}\` | \`${item.key}\` | ${item.resolution} | resolved |`,
  ).join('\n');
  return `## 4. Enforced Spot-Check

### 4.1 Method

- A sample of ${VERIFIED_SPOT_CHECKS.length} \`executable\` directive rows was drawn,
  covering the original surface (routing, static content, proxying, cache, limits, TLS,
  stream) **and every directive completed by Waves 1/2/2b/3a/3a-2/3b/4a, the upstream-protocol wave, the gRPC wave, the stream/OCSP wave, the CGI-transport wave, the rewrite/variable-engine wave, the mail waves, the uWSGI upstream-TLS wave, the CGI cache-family wave and the protocol response-policy wave**.
- For each one the audit **read the cited file** and confirmed the cited
  function/case exists and its assertions cover the behavior the catalog attaches to
  the directive. Mechanical existence is re-checkable offline with
  \`node tools/gen-nginx-directive-audit.mjs --verify-evidence\`.
- Verdicts: \`verified\` means the reference exists and the assertions prove the claimed
  behavior. Section 4.3 records the previously reported over-claims and their current
  state.

### 4.2 Verified Sample (${VERIFIED_SPOT_CHECKS.length} directive rows)

| Directive | Claim checked | Evidence reference | What the assertion actually proves | Verdict |
| --- | --- | --- | --- | --- |
${verifiedRows}

### 4.3 Over-Claim Status

**No unresolved over-claim remains.** The two classes reported in the previous snapshot
were fixed and are now gated by \`--verify-evidence\`:

**A. Evidence-citation mismatches (${RESOLVED_EVIDENCE_OVERCLAIMS.length}, resolved).**
The weak citations were replaced with the stronger committed tests shown below; the
gate fails if any entry regresses back to the old citation.

| Directive | Previously cited (did not prove the claim) | Replacement evidence | Status |
| --- | --- | --- | --- |
${resolvedEvidence}

**B. Migration-action over-claims (${RESOLVED_MIGRATION_OVERCLAIMS.length}, resolved).**
The matrix gate now derives \`importer.tomlEnforced\` from actual materializer
consumption, so the migration actions no longer promise a typed key that is silently
dropped. The gate asserts that no accepted-unenforced entry claims an unconsumed
"enforced typed key", and that no executable entry renders an accepted-but-ignored key.

| Directive | Typed key previously mis-claimed | Current state | Status |
| --- | --- | --- | --- |
${resolvedMigration}

One semantic caveat remains explicit rather than silently resolved:
\`http:keepalive_timeout\` maps to a consumed key, but as \`limits.requestTimeoutMs\`
(a request timeout, not the keep-alive idle timeout). The catalog records this in
\`importer.tomlSemantics\` and the migration action says so.

**C. Evidence grade (not an over-claim).** ${materializationOnly.length} of the
${countRows(rows, 'enforced')} executable rows cite only the generic
\`every_supported_directive_family_materializes\` case. That case does assert the
normalized fields per surface case, so it is mapping evidence, not end-to-end behavioral
evidence. Behavioral proof for those directives rests on the
\`tests/nginx/differential-*\` batteries and \`tests/nginx/full-surface\`, which is a
stronger contract than the per-row citation suggests. The catalog could distinguish an
evidence **grade** (\`materialized\` vs \`behavioral\`) instead of implying every
executable row has a behavioral test.

### 4.4 Decision Consistency Check

${decisionProof ? `Consistent: \`executable\` <=> \`enforced\` in both directions (${countRows(
    rows,
    'enforced',
  )} <=> ${countClassification(rows, 'executable')}). Every completed-wave directive is executable and every executable directive is enforced.` : 'INCONSISTENT - see generator failure.'}
`;
}

function renderParity(rows) {
  const outOfProfile = rows.filter((row) => row.decision === 'out-of-profile');
  const failClosed = rows.filter((row) => row.decision === 'fail-closed');
  const excludedModules = [...new Set(outOfProfile.map((row) => row.module))].sort();
  const excludedByModule = excludedModules.map((module) => [
    module,
    outOfProfile.filter((row) => row.module === module).length,
  ]);
  const failClosedByModule = [...new Set(failClosed.map((row) => row.module))]
    .map((module) => [module, failClosed.filter((row) => row.module === module).length])
    .sort((left, right) => right[1] - left[1])
    .slice(0, 18);
  const notNginxBehavior =
    countRows(rows, 'out-of-profile') +
    countRows(rows, 'fail-closed') +
    countRows(rows, 'runtime-owned');
  return `## 5. Why Full Nginx Parity Is Not A Coherent Target

### 5.1 The Audit Data Says So

| Bucket | Directives | Share of 744 |
| --- | ---: | ---: |
| \`enforced\` (actually nginx-compatible behavior) | ${countRows(rows, 'enforced')} | ${((countRows(rows, 'enforced') / 744) * 100).toFixed(1)}% |
| \`security-difference\` (intentionally not nginx) | ${countRows(rows, 'security-difference')} | ${((countRows(rows, 'security-difference') / 744) * 100).toFixed(1)}% |
| \`behavior-gap\` (planned toward compatibility) | ${countRows(rows, 'behavior-gap')} | ${((countRows(rows, 'behavior-gap') / 744) * 100).toFixed(1)}% |
| \`runtime-owned\` (deliberately not nginx value) | ${countRows(rows, 'runtime-owned')} | ${((countRows(rows, 'runtime-owned') / 744) * 100).toFixed(1)}% |
| \`out-of-profile\` (PRD section 4 exclusion) | ${countRows(rows, 'out-of-profile')} | ${((countRows(rows, 'out-of-profile') / 744) * 100).toFixed(1)}% |
| \`fail-closed\` (rejected, not planned) | ${countRows(rows, 'fail-closed')} | ${((countRows(rows, 'fail-closed') / 744) * 100).toFixed(1)}% |

${notNginxBehavior} of 744 directives (${((notNginxBehavior / 744) * 100).toFixed(1)}%) are
**not** nginx-behavior compatibility surface by design: the Rust runtime owns the value
(process model, event loop, buffers, timeouts, TLS policy, logging, upstream policy) or the
feature is explicitly rejected. The compatibility program is the ${countRows(
    rows,
    'enforced',
  )} executed directives plus the ${countRows(
    rows,
    'behavior-gap',
  )} planned backlog; full parity would still have to add every rejected and excluded
feature below.

### 5.2 Out-Of-Profile Groups (${outOfProfile.length} directives)

${excludedByModule.map(([module, count]) => `- \`${module}\` - ${count} directive(s)`).join('\n')}

Only embedded Perl, GeoIP and dynamic module loading remain outside the declared
profile. The four embedded application protocol modules and (after the PRD section 4
amendment) the mail proxy moved **into** the profile and are audited in the ledger.

What implementing each remaining group would actually require:

| Group | Directives | What full parity would require |
| --- | ---: | --- |
| Perl | ${outOfProfile.filter((row) => row.module === 'ngx_http_perl_module').length} | an embedded Perl interpreter - executable configuration, explicitly excluded. |
| GeoIP | ${outOfProfile.filter((row) => row.module.includes('geoip')).length} | a MaxMind database dependency plus an OS/database-specific lookup contract. |
| Dynamic module loading | ${outOfProfile.filter((row) => row.module === 'ngx_core_module').length} | an ABI/loading contract that the static nginx OSS reference binary does not even provide; conflicts with the standalone-only deployment profile. |

Substrate note: the landed \`crates/sdkwork-webserver-upstream-protocol\` substrate
(FastCGI/uWSGI/SCGI/memcached/gRPC codecs, 89 tests) is now wired end-to-end for all five
protocols through \`crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cgi_upstream.rs\`
and \`tests/nginx/differential-protocols\` (39 cases, byte-identical against nginx 1.29.6;
13 of them gRPC against a test-only Rust HTTP/2 mock, plaintext h2c and verified TLS).
Implemented forms execute, including Wave 7A's typed \`[http] unixSocketRoot\` with
strict-descendant confinement for \`unix:/absolute\` upstream targets (nginx-conf
\`unix:\` pass targets intentionally fail closed) and the bounded per-generation FastCGI
keep_conn pool with the pooled-socket framing fix. Wave 7B added uWSGI upstream TLS
identity/verification through the same transport seam (always verified unless
uwsgi_ssl_verify is off), and nginx 1.29.6 defines no scgi_ssl_* or fastcgi_ssl_*
directives. Wave 7C executes the fastcgi/uwsgi/scgi response-cache family on the shared
HTTP cache store (nginx has no default caching time). Wave 7D executes the protocol
response policy (header-block/read-chunk buffers, store with path templates, limit_rate,
force_ranges, ignore_headers, pass_request_body, memcached gzip-flag and gRPC
ignore_headers). The residual temp-spool, ignore_client_abort, catch_stderr, send_lowat,
store-on, uWSGI TLS and gRPC bind/keepalive/TLS-library forms are backlog rows, not
profile exclusions.

Mail note (PRD section 4 amendment): the mail proxy is no longer out of profile. The
\`crates/sdkwork-webserver-mail-protocol\` crate (POP3/IMAP/SMTP state machines) and the
mail listener plane (wave 9A: config plane, nginx mail{} materialization, plaintext
dialog and PROXY v1) are now wired end-to-end (wave 9B): 27 mail-module directives are
executable, proven by the new 9-case \`tests/nginx/differential-mail\` slice against real
nginx 1.29.6 (the sixth aggregate differential slice). Two scope caveats stay explicit:
backend TLS is a typed-plane extension (nginx mail has no proxy_ssl), so nginx parity is
not claimed for it; client-certificate verification is absent and those flows remain
fail-closed backlog (section 3.2 mail module residuals).

### 5.3 Fail-Closed Groups (${failClosed.length} directives, top modules)

${failClosedByModule.map(([module, count]) => `- \`${module}\` - ${count} directive(s)`).join('\n')}

Representative implementation cost:

| Group | Directives | What parity would require |
| --- | ---: | --- |
| Rejected proxy knobs (\`ngx_http_proxy_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_proxy_module').length} | cache locking/background update/bypass/method sets, body substitution, upstream-trust knobs: each is a bounded policy feature with its own security review. |
| Rejected stream plumbing (\`ngx_stream_proxy_module\`) | ${failClosed.filter((row) => row.module === 'ngx_stream_proxy_module').length} | buffer sizing, TLS library caches/commands/CRL/password-file knobs; the routing/identity subset is instead in the remaining backlog. |
| TLS extras (CRL/OCSP-response/ECH/ticket files) | ${failClosed.filter((row) => row.module === 'ngx_http_ssl_module' || row.module === 'ngx_stream_ssl_module').length} | rustls feature gaps (OCSP response handling, raw public keys, certificate compression, session ticket key files) plus a certificate-material pipeline. |
| HTTP/3 + QUIC (\`ngx_http_v3_module\`, \`ngx_quic_bpf_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_v3_module' || row.module === 'ngx_quic_bpf_module').length} | a QUIC transport (UDP, TLS 1.3 QUIC keys, connection migration, congestion control) - a separate transport product, explicitly excluded by PRD section 4. |
| Embedded filters (SSI/XSLT/image/mp4/FLV/empty-gif) | ${failClosed.filter((row) => ['ngx_http_ssi_filter_module', 'ngx_http_xslt_filter_module', 'ngx_http_image_filter_module', 'ngx_http_mp4_module', 'ngx_http_flv_module', 'ngx_http_empty_gif_module'].includes(row.module)).length} | embedded template/language/codec dependencies (libxslt, libgd, mp4/FLV parsers) that the standalone data plane does not ship. |
| \`break\`/\`rewrite_log\` rewrite control (\`ngx_http_rewrite_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_rewrite_module').length} | unconditional control directives outside the bounded state machine; \`if\`/\`set\`/\`return\`/\`rewrite\`/\`try_files\` now execute. |

### 5.4 Lua, njs, And Third-Party Modules

The 744-row inventory is the pristine nginx **OSS tarball** directive table
(\`inventory.source.thirdPartyModules\` = "excluded (a pristine nginx OSS tarball contains
none)"), so Lua/njs/third-party directives have **zero** rows here even before the profile
decision. They are excluded twice: absent from the inventory, and excluded by PRD section 4
as embedded executable configuration. The reference build's \`configureArgs\` list
(\`--with-http_v2_module\`, \`--with-http_v3_module\`, \`--with-mail\`, \`--with-stream\`, and so
on) describes what the differential oracle contains, not what http-core-v1 promises.

### 5.5 Conclusion

A "parity" target would still require: embedded Perl, GeoIP, a QUIC transport, embedded
template/language filters and dynamic module loading, plus the residual protocol/gRPC/
mail sub-families that are now backlog rather than profile exclusions. Six application
protocols (FastCGI/uWSGI/SCGI/memcached/gRPC and now the POP3/IMAP/SMTP mail proxy) are
no longer in that excluded set: their implemented forms execute end-to-end on the Rust
data plane against nginx 1.29.6 (mail backend TLS is a typed-plane extension, not a
claimed nginx parity surface). The coherent target remains the declared
\`${PROFILE}\` profile with the four PRD section 2 grades (parse, semantic, behavioral,
operational), a generated directive catalog, and a versioned evidence revision. That is
what this ledger audits.
`;
}

function renderReproduction(rows) {
  const completedRows = COMPLETED_WAVES.flatMap((wave) =>
    wave.ids.map((id) => {
      const row = rows.find((candidate) => candidate.id === id);
      return `| \`${row.name}\` (\`${id}\`) | \`${wave.key}\` | completed - \`enforced\` |`;
    }),
  ).join('\n');
  return `## 6. Reproduction And Change Control

### 6.1 Commands

\`\`\`sh
# 1. Regenerate this audit (deterministic; reads only committed inputs).
node tools/gen-nginx-directive-audit.mjs

# 2. CI freshness gate (exits 1 when the report is stale). Safe to wire in.
node tools/gen-nginx-directive-audit.mjs --check

# 3. Re-verify the audited claims against the repository sources (offline, read-only).
node tools/gen-nginx-directive-audit.mjs --verify-evidence

# 4. Real-world acceptance evidence for the rewrite/variable engine (aggregate-gate step).
node tools/check-nginx-real-world.mjs
\`\`\`

The generator reads \`specs/nginx-directive-inventory.json\` and
\`specs/nginx-directive-support.catalog.json\`; it makes no network calls, embeds no
timestamps or git HEAD, and sorts every collection before rendering, so a rerun on the
same catalog is byte-identical. \`--check\` regenerates in memory and compares bytes.

### 6.2 What Changed In This Refresh

This refresh was regenerated after the protocol response-policy wave (7D) landed and the
Lead re-synced the catalog. The table lists the cumulative completed-wave rows and their
current state:

| Directive | Wave | Current state |
| --- | --- | --- |
${completedRows}

All ${COMPLETED_WAVE_IDS.size} rows above were re-spot-checked (section 4.2) and are
asserted \`executable\` by \`--verify-evidence\`.

### 6.3 Which Claims Change When Wave 3 Lands

The ${countRows(rows, 'behavior-gap')} wave-3 backlog rows are the only behavior-gap
entries that can move to \`enforced\`. When a wave-3 slice lands, the workstream (or the
Lead) must:

1. Update \`crates/sdkwork-webserver-core\`, the gateway data plane, tests, and the wave
   notes in \`specs/nginx-gap.catalog.json\`.
2. Regenerate the catalog + operator matrix via
   \`node tools/check-nginx-directive-matrix.mjs --write\` (the catalog is generated from
   the implementation by the matrix gate reconciliation).
3. Rerun \`node tools/gen-nginx-directive-audit.mjs\` and commit the regenerated report
   with the catalog change; \`--check\` fails otherwise.

Count effects per moved slice: \`behavior-gap\` decreases by the slice size,
\`enforced\` increases by the same size, and the category disappears from section 3.2
once all its rows are executable.

### 6.4 Wiring Suggestion For The Lead

\`package.json\` is outside this audit's write scope. The Lead can add the two
non-mutating invocations to the release verification chain, for example next to
\`check:nginx-matrix\`:

\`\`\`json
{
  "scripts": {
    "check:nginx-directive-audit": "node tools/gen-nginx-directive-audit.mjs --check && node tools/gen-nginx-directive-audit.mjs --verify-evidence"
  }
}
\`\`\`

and include \`check:nginx-directive-audit\` in the \`_sdkwork:verify\` aggregate after the
catalog/matrix gate. The generator fails closed on a count mismatch, a duplicate row,
an unknown directive in a decision set, a completed-wave classification regression, a
consumed-key claim that no longer holds, a reintroduced weak citation, a missing evidence
file in \`--verify-evidence\`, or a stale report in \`--check\`.
`;
}

function renderReport(inventory, catalog, rows) {
  const parts = [
    renderHeader(inventory, catalog),
    '## 1. Executive Summary\n\n### 1.0 How To Read The Ledger\n\nEvery row has exactly one **decision**: ' +
      DECISION_ORDER.map((decision) => `\`${decision}\``).join(', ') +
      '. `Catalog` is the committed machine classification; `Decision` is this audit\'s operator-facing judgment. `Rationale` is generated from the catalog evidence/migration data plus the explicit rule sets in the generator, so a reviewer can audit the rule, not the prose.\n',
    renderExecutiveSummary(rows),
    '## 2. Decision Ledger By Module\n\nOne row per inventory directive, grouped by nginx group and module (the same grouping the operator matrix uses). Every one of the 744 `(module, name)` pairs appears exactly once; bare names can repeat across modules (for example `listen`, `map`, `proxy_pass`), so the module heading is part of the identity.\n\n### Decision Legend\n\n| Decision | Meaning |\n| --- | --- |\n' +
      DECISION_ORDER.map((decision) => `| \`${decision}\` | ${DECISION_MEANING[decision]} |`).join('\n') +
      '\n',
    renderDecisionLedger(rows),
    renderWaveAndBacklog(rows),
    renderSpotChecks(rows),
    renderParity(rows),
    renderReproduction(rows),
  ];
  const document = parts.join('\n');
  for (const section of REQUIRED_SECTIONS) {
    if (!document.includes(section)) fail(`rendered report is missing section "${section}"`);
  }
  if (!document.endsWith('\n')) fail('rendered report must end with a newline');
  return document;
}

function sourceConsumesTypedKey(source, key) {
  return new RegExp(`\\.get\\("${key}"\\)`).test(source);
}

function verifyEvidence(inventory) {
  const catalog = readJson(CATALOG_PATH);
  const rows = buildRows(inventory, catalog);
  const problems = [];
  const executable = catalog.directives.filter((entry) => entry.classification === 'executable');
  for (const entry of executable) {
    for (const evidence of entry.evidence ?? []) {
      const absolute = path.join(REPO_ROOT, evidence.file);
      if (!fs.existsSync(absolute)) {
        problems.push(`${entry.id}: evidence file does not exist: ${evidence.file}`);
        continue;
      }
      const source = fs.readFileSync(absolute, 'utf8');
      if (evidence.test && !new RegExp(`(?:fn|def)\\s+${evidence.test}\\b`).test(source)) {
        problems.push(`${entry.id}: evidence test not found in ${evidence.file}: ${evidence.test}`);
      }
      if (evidence.case && !source.includes(evidence.case)) {
        problems.push(`${entry.id}: evidence case not found in ${evidence.file}: ${evidence.case}`);
      }
    }
  }
  for (const wave of COMPLETED_WAVES) {
    for (const id of wave.ids) {
      const entry = catalog.directives.find((candidate) => candidate.id === id);
      if (!entry) {
        problems.push(`${wave.key}: unknown completed directive ${id}`);
      } else if (entry.classification !== 'executable') {
        problems.push(
          `${wave.key}: ${id} regressed to ${entry.classification}; regenerate the audit and fix the catalog`,
        );
      }
    }
  }
  const serverToml = fs.readFileSync(SERVER_TOML_PATH, 'utf8');
  for (const entry of catalog.directives) {
    const importer = entry.importer;
    if (!importer?.mapped) continue;
    const key = importer.tomlPath ? importer.tomlPath.split('.').pop() : '';
    if (entry.classification === 'executable' && importer.tomlEnforced === false) {
      problems.push(
        `${entry.id}: executable row renders an accepted-but-ignored typed key (${importer.tomlPath})`,
      );
    }
    if (entry.classification === 'accepted-unenforced' && importer.tomlEnforced === true) {
      if (!sourceConsumesTypedKey(serverToml, key)) {
        problems.push(
          `${entry.id}: claims enforced typed key "${key}" but the materializer does not consume it`,
        );
      }
    }
  }
  for (const [key] of CONSUMED_TYPED_KEYS) {
    if (!sourceConsumesTypedKey(serverToml, key)) {
      problems.push(`typed key "${key}" is no longer consumed by the materializer`);
    }
  }
  for (const item of RESOLVED_EVIDENCE_OVERCLAIMS) {
    const entry = catalog.directives.find((candidate) => candidate.id === item.id);
    if (!entry) {
      problems.push(`resolved over-claim references unknown directive ${item.id}`);
      continue;
    }
    if ((entry.evidence ?? []).some((evidence) => JSON.stringify(evidence).includes(item.oldFragment))) {
      problems.push(`${item.id}: the previously reported weak citation is present again`);
    }
  }
  for (const item of RESOLVED_MIGRATION_OVERCLAIMS) {
    const entry = catalog.directives.find((candidate) => candidate.id === item.id);
    if (!entry) {
      problems.push(`resolved migration over-claim references unknown directive ${item.id}`);
      continue;
    }
    // Resolved when the typed key is either no longer claimed enforced, or is
    // genuinely consumed by the materializer (chainFile after wave 2).
    if (
      entry.classification === 'accepted-unenforced' &&
      entry.importer?.tomlEnforced === true &&
      !sourceConsumesTypedKey(serverToml, item.key)
    ) {
      problems.push(
        `${item.id}: accepted-unenforced entry claims an unconsumed enforced typed key again (${item.key})`,
      );
    }
  }
  for (const check of VERIFIED_SPOT_CHECKS) {
    const row = rows.find((candidate) => candidate.id === check.id);
    if (!row || row.decision !== 'enforced') {
      problems.push(`spot-check ${check.id}: directive is no longer enforced; regenerate the audit`);
    }
  }
  const enforcedCount = countRows(rows, 'enforced');
  const executableCount = catalog.directives.filter(
    (row) => row.classification === 'executable',
  ).length;
  if (enforcedCount !== executableCount) {
    problems.push(`enforced (${enforcedCount}) != executable (${executableCount})`);
  }
  return problems;
}

function main() {
  const args = new Set(process.argv.slice(2));
  const known = new Set(['--check', '--verify-evidence', '--help']);
  for (const arg of args) {
    if (!known.has(arg)) {
      process.stderr.write(`unknown argument: ${arg}\n`);
      process.exitCode = 2;
      return;
    }
  }
  if (args.has('--help')) {
    process.stdout.write(
      'usage: node tools/gen-nginx-directive-audit.mjs [--check|--verify-evidence]\n',
    );
    return;
  }
  const inventory = readJson(INVENTORY_PATH);
  const catalog = readJson(CATALOG_PATH);
  const rows = buildRows(inventory, catalog);
  if (args.has('--verify-evidence')) {
    const problems = verifyEvidence(inventory);
    if (problems.length > 0) {
      process.stderr.write('audit evidence verification FAILED:\n');
      for (const problem of problems) process.stderr.write(`  - ${problem}\n`);
      process.exitCode = 1;
      return;
    }
    process.stdout.write(
      `audit evidence verification passed: ${rows.length} rows, ${countRows(
        rows,
        'enforced',
      )} executable evidence references, ${COMPLETED_WAVE_IDS.size} completed-wave directives, ${RESOLVED_EVIDENCE_OVERCLAIMS.length} resolved evidence citations, ${RESOLVED_MIGRATION_OVERCLAIMS.length} resolved migration over-claims\n`,
    );
    return;
  }
  const document = renderReport(inventory, catalog, rows);
  if (args.has('--check')) {
    if (!fs.existsSync(REPORT_PATH)) {
      process.stderr.write(`report is missing: ${path.relative(REPO_ROOT, REPORT_PATH)}\n`);
      process.exitCode = 1;
      return;
    }
    const current = fs.readFileSync(REPORT_PATH, 'utf8');
    if (current !== document) {
      process.stderr.write(
        `report is stale: rerun node tools/gen-nginx-directive-audit.mjs (${path.relative(
          REPO_ROOT,
          REPORT_PATH,
        )})\n`,
      );
      process.exitCode = 1;
      return;
    }
    process.stdout.write(`report is current: ${path.relative(REPO_ROOT, REPORT_PATH)}\n`);
    return;
  }
  fs.mkdirSync(path.dirname(REPORT_PATH), { recursive: true });
  fs.writeFileSync(REPORT_PATH, document, 'utf8');
  const counts = DECISION_ORDER.map((decision) => `${decision}=${countRows(rows, decision)}`).join(
    ' ',
  );
  process.stdout.write(
    `wrote ${path.relative(REPO_ROOT, REPORT_PATH)} (${rows.length} rows; ${counts})\n`,
  );
}

main();
