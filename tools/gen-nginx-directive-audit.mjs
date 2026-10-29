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
const AUDIT_HEAD = '8f96a59e';
const AUDIT_BRANCH = 'main';
const PROFILE = 'http-core-v1';
const SNAPSHOT_STATE =
  'uncommitted worktree refresh after Waves 1, 2, 2b, 3a and 3a-2 (code plus the re-synced directive catalog), on top of HEAD ' +
  AUDIT_HEAD;
const REMAINING_WAVE = 'wave 3 backlog';

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
];

const COMPLETED_WAVE_IDS = new Set(COMPLETED_WAVES.flatMap((wave) => wave.ids));

/**
 * Remaining behavior-gap backlog after waves 1/2/2b/3a/3a-2 (the categories
 * named by specs/nginx-gap.catalog.json as still open after wave 3a-2).
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
    priority: 'medium',
    ids: ['http:chunked_transfer_encoding'],
    rejectedLoss:
      'a stock nginx.conf that tunes response framing fails to activate.',
    why: 'response framing (chunked vs content-length) is directly client-visible and can change caching and proxy behavior.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/http1_wire.rs (response framing), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'response-framing unit tests plus a framed differential case (chunked/identity and HEAD)',
  },
  {
    key: 'addition-filter',
    label: 'body addition filter',
    priority: 'medium',
    ids: ['http:add_before_body', 'http:add_after_body', 'http:addition_types'],
    rejectedLoss:
      'a stock nginx.conf that injects additional bodies around a response fails to activate.',
    why: 'the injected body is part of the client-visible response and needs the bounded subrequest engine the gap catalog records as the missing piece.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/handler.rs (response body pipeline + bounded subrequests), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'addition unit tests (before/after ordering, eligible MIME types, missing target) plus differential addition cases',
  },
  {
    key: 'referer-module',
    label: 'referer module',
    priority: 'medium',
    ids: ['http:valid_referers'],
    rejectedLoss:
      'a stock nginx.conf that allow-lists Referer values fails to activate.',
    why: 'the request allow-list decision and the `$invalid_referer` variable are request-visible; nginx normally consumes them through `if`, so this also depends on threading the variable through the supported variable surface.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/request_gate.rs + data_plane/handler.rs ($invalid_referer variable), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'referer allow-list matrix plus differential valid_referers cases',
  },
  {
    key: 'http2-tuning',
    label: 'HTTP/2 tuning',
    priority: 'medium',
    ids: [
      'http:http2_body_preread_size',
      'http:http2_chunk_size',
      'http:http2_idle_timeout',
      'http:http2_max_concurrent_pushes',
      'http:http2_max_concurrent_streams',
      'http:http2_max_field_size',
      'http:http2_max_header_size',
      'http:http2_max_requests',
      'http:http2_pool_size',
      'http:http2_push',
      'http:http2_push_preload',
      'http:http2_recv_buffer_size',
      'http:http2_recv_timeout',
      'http:http2_streams_index_size',
    ],
    rejectedLoss:
      'a stock HTTP/2 edge that tunes flow control, header size, or request limits fails to activate.',
    why: 'http-core-v1 already terminates HTTP/2 (the `http2` directive is executable); the nginx tuning knobs of the same protocol are in-profile settings, and today they reject the configuration rather than map onto the runtime H2 limits.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/server.rs (Hyper h2 builder) + data_plane/runtime.rs (limits struct), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'limit mapping unit tests plus an HTTP/2 SETTINGS/size differential (header list, concurrent streams, request count)',
  },
  {
    key: 'static-open-file-cache',
    label: 'static open-file cache',
    priority: 'medium',
    ids: [
      'http:open_file_cache',
      'http:open_file_cache_min_uses',
      'http:open_file_cache_valid',
    ],
    rejectedLoss:
      'a stock nginx.conf that tunes the static-file cache fails to activate.',
    why: 'the runtime has its own open policy and exposes no parity knob; the response bytes are unchanged, but the operator-declared cache contract is not honored.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/static_path.rs (fd cache), crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'cache invalidation/TTL/threshold unit tests',
  },
  {
    key: 'proxy-method',
    label: 'proxied method override',
    priority: 'high',
    ids: ['http:proxy_method'],
    rejectedLoss:
      'the directive is accepted-but-ignored; the client method is forwarded instead of the configured override.',
    why: 'the upstream request method is protocol-visible and changes upstream semantics.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/upstream_client.rs, crates/sdkwork-webserver-core/src/config/model.rs, src/nginx/mapping.rs',
    tests: 'method-override unit test plus a differential upstream-method case',
  },
  {
    key: 'ocsp-http',
    label: 'OCSP stapling / trust chain (HTTP)',
    priority: 'medium (commercial impact bounded)',
    ids: [
      'http:ssl_stapling',
      'http:ssl_stapling_verify',
      'http:ssl_trusted_certificate',
    ],
    rejectedLoss:
      'the directive is accepted-but-ignored; no OCSP response is stapled to the handshake.',
    why: 'stapling is a client-visible TLS handshake property and the gap catalog records `tls.ocsp-stapling` as missing.',
    surface:
      'crates/sdkwork-webserver-core/src/tls_runtime/, crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/tls.rs, certificate material distribution',
    tests: 'stapling unit test plus a TLS handshake differential with a stapled response',
  },
  {
    key: 'stream',
    label: 'stream module gaps',
    priority: 'high',
    ids: [
      'stream:map',
      'stream:ssl_stapling',
      'stream:ssl_stapling_verify',
      'stream:ssl_trusted_certificate',
      'stream:allow',
      'stream:deny',
      'stream:limit_conn',
      'stream:limit_conn_zone',
      'stream:limit_conn_dry_run',
      'stream:set_real_ip_from',
      'stream:pass',
      'stream:return',
      'stream:set',
      'stream:split_clients',
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
    why: 'generic stream proxying is in profile and the HTTP plane already executes the analogous feature (access ACL, connection limits, realip, routing primitives, upstream TLS identity); the stream plane does not, and variable maps/OCSP affect stream routing and handshakes.',
    surface:
      'crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/stream_proxy.rs, crates/sdkwork-webserver-core/src/config/model.rs (streams), src/nginx/mapping.rs, src/config/uri.rs (stream map variables)',
    tests: 'stream_proxy.rs unit/integration tests for each feature plus a stream differential slice against nginx 1.29.6',
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
    /^ngx_http_fastcgi_module$/,
    'FastCGI execution is excluded by PRD section 4 (embedded CGI family); it would need a FastCGI record-protocol client plus the CGI-variable security model.',
  ],
  [
    /^ngx_http_uwsgi_module$/,
    'uWSGI execution is excluded by PRD section 4; it would need the uWSGI packet protocol and argument marshalling.',
  ],
  [
    /^ngx_http_scgi_module$/,
    'SCGI execution is excluded by PRD section 4 (embedded CGI family); it would need SCGI framing and a CGI environment model.',
  ],
  [
    /^ngx_http_memcached_module$/,
    'memcached execution is excluded by PRD section 4; it would need a memcached protocol client plus cache-status mapping.',
  ],
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
    /^ngx_mail/,
    'the mail proxy (SMTP/IMAP/POP3 plus auth backends) is excluded by PRD section 4; it would need protocol state machines and mail auth backends.',
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
    'stream proxy plumbing that is not part of the wave-3 stream backlog (buffer sizing, TLS library caches/commands/CRL/password files); rejected under the PRD section 4 stream profile.',
  ],
  [
    /^ngx_http_grpc_module$/,
    'gRPC upstream execution is not in http-core-v1; it would need an HTTP/2 gRPC framing/trailer surface and is rejected.',
  ],
  [
    /^ngx_http_v2_module$/,
    'HTTP/2 tuning is tracked as wave-3 backlog (see the behavior-gap register), so a remaining http2_* directive outside that set is not planned.',
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
    'the userid cookie filter is outside the http-core-v1 response surface.',
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
    '`if`/`set`/`break`/`rewrite_log` are outside the bounded rewrite state machine; only `return`, `rewrite` and `try_files` execute.',
  ],
  [
    /^ngx_http_browser_module$/,
    'the browser/UA classification module is outside the profile.',
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
    'stream preread buffer/timeout and stream server_name tuning is owned by the runtime stream plane; rejected.',
  ],
  [
    /^ngx_http_addition_filter_module$/,
    'the body addition filter is tracked as wave-3 backlog (it needs a bounded subrequest engine); any remaining form is not planned.',
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
    'valid_referers is tracked as wave-3 backlog; the referer hash-table sizing directives are not planned.',
  ],
  [
    /^ngx_stream_limit_conn_module$/,
    'the remaining stream limit_conn directives are tracked as wave-3 stream backlog; dry-run mode outside that list is not planned.',
  ],
  [
    /^ngx_http_auth_request_module$/,
    'subrequest-based external auth is not implemented and is outside the covered access-control surface (allow/deny + auth_basic execute).',
  ],
  [
    /^ngx_http_degradation_module$/,
    'the degradation module is outside the profile.',
  ],
  [
    /^ngx_http_gunzip_filter_module$/,
    'on-the-fly gunzip is not implemented.',
  ],
  [
    /^ngx_http_mirror_module$/,
    'request mirroring is outside the profile.',
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
    'random index selection is not implemented.',
  ],
  [
    /^ngx_http_slice_filter_module$/,
    'slice range caching is not implemented.',
  ],
  [
    /^ngx_http_split_clients_module$/,
    'split_clients is a map-style variable table; the remaining backlog covers stream maps, not this cluster.',
  ],
  [
    /^ngx_stream_split_clients_module$/,
    'stream split_clients is tracked as wave-3 stream backlog; any remaining form outside that list is not planned.',
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
    'stream `pass` is tracked as wave-3 stream backlog; other forms are not planned.',
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
    'stream PROXY-protocol realip rewriting is tracked as wave-3 stream backlog; other forms are not planned.',
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
      return `runtime-owned: ${impact || 'the Rust runtime owns this subsystem by design'}.`;
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
      return `behavior-gap (${REMAINING_WAVE}, ${category.label}): ${state}${loss}. Not runtime-owned: no runtime policy substitutes the client-visible effect.`;
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
- **This snapshot is the uncommitted worktree refresh after Waves 1, 2, 2b, 3a and
  3a-2.** The code, tests, generated operator matrix and the re-synced directive
  catalog were produced by those waves and are not committed at HEAD
  \`${AUDIT_HEAD}\`; the audit is regenerated against that post-wave catalog (evidence
  revision ${catalog.evidenceRevision}). Wave 1 (map, MIME types/default_type,
  gzip_static, gzip_vary), Wave 2 (redirect policies, limit status codes, max_ranges,
  chain-file consumption, typed TOML maps), Wave 2b (proxy response-header/cookie
  policy, internal locations, merge_slashes, request-header acceptance), Wave 3a
  (charset family header-level behaviour, gzip_disable/gzip_proxied) and Wave 3a-2
  (autoindex family, empty_gif, stub_status) are recorded as completed in section 3,
  and every one of them is asserted executable by \`--verify-evidence\`.
- **The ledger, the counts, and the operator matrix must be regenerated whenever the
  catalog changes.** The catalog and
  \`docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md\` are generated/checked by
  \`tools/check-nginx-directive-matrix.mjs\`; this audit embeds the catalog sha256, so
  \`--check\` fails when the catalog moves. The Wave 3 remaining backlog in section 3.2
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
   Waves 1, 2, 2b, 3a and 3a-2 landed ${completedCount} directives. ${countRows(
     rows,
     'runtime-owned',
   )} more are deliberately runtime-owned (${(
     (countRows(rows, 'runtime-owned') / total) *
     100
   ).toFixed(1)}%) and ${countRows(
     rows,
     'security-difference',
   )} are intentional, spec-justified security differences.
2. **${remainingCount} behavior-gap directives remain in the wave-3 backlog** (${(
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
   snapshot of the post-Wave-2b worktree, not a hand-maintained table; section 6 states
   the exact commands and the wave-3 landing procedure.
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

### 3.1 Completed Waves (1, 2, 2b)

The catalog now classifies every directive below as \`executable\` with committed
evidence. Wave 1 was the routing/static correctness set; Wave 2 added redirect
construction, limit status codes, range ceilings and typed-TOML map/chain materialization;
Wave 2b closed the proxy response-header policy, internal locations, slash merging and
request-header acceptance; Wave 3a added the header-level charset family and the gzip
user-agent/proxied refinements; Wave 3a-2 added the autoindex listing family plus the
empty_gif and stub_status location handlers.

${completedTable}
${completedRows}

The wave notes also record bounded remaining differences (for example map evaluation
timing, orphan \`.gz\` sidecars, regex proxy_redirect/cookie forms, and the nginx
pre-vhost-selection \`merge_slashes\` quirk); those are documented in each catalog entry's
\`knownDifferences\` and are not counted as backlog rows.

### 3.2 Remaining Behavior-Gap Backlog (${REMAINING_WAVE}, ${countRows(
    rows,
    'behavior-gap',
  )} directives)

These are the categories \`specs/nginx-gap.catalog.json\` lists as still open after
Wave 3a-2 (plus the addition filter and referer module the same wave note names as
remaining in this area). An accepted-unenforced row is silently ignored today; a
rejected row fails the configuration load. Both are real migration gaps, so every row carries the
operator-visible loss, the reason it is **not** runtime-owned, the files that would
have to change, and the tests to add.

${backlogSections}
### 3.3 Backlog Sequencing Note

Within the backlog, the largest operator payoff is the stream module gaps (a whole
transport plane), followed by the HTTP/2 tuning set (configurations that declare HTTP/2
limits are rejected today even though HTTP/2 itself executes). The body addition filter
and the referer module are medium-sized features that pull in a bounded subrequest engine
and \`$invalid_referer\` variable threading respectively. \`proxy_method\`,
\`chunked_transfer_encoding\` and the static open-file cache are smaller, bounded
changes. The OCSP/trust-chain rows are limited by the rustls capability gap recorded as
\`tls.ocsp-stapling: missing\` in the gap catalog.

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
  stream) **and every directive completed by Waves 1/2/2b/3a/3a-2**.
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

What implementing each group would actually require:

| Group | Directives | What full parity would require |
| --- | ---: | --- |
| FastCGI | ${outOfProfile.filter((row) => row.module === 'ngx_http_fastcgi_module').length} | a FastCGI record-protocol client, request/response stream framing, CGI-variable marshalling, and a security model for app-supplied params; a separate protocol product surface with its own conformance corpus. |
| uWSGI | ${outOfProfile.filter((row) => row.module === 'ngx_http_uwsgi_module').length} | the uWSGI packet protocol (modifier1/modifier2), params/body marshalling, and its own upstream TLS/cache policy surface. |
| SCGI | ${outOfProfile.filter((row) => row.module === 'ngx_http_scgi_module').length} | SCGI netstring framing plus a CGI environment model; same security review as FastCGI. |
| memcached | ${outOfProfile.filter((row) => row.module === 'ngx_http_memcached_module').length} | a memcached text/binary protocol client and cache-status mapping into the response pipeline. |
| Mail proxy (SMTP/IMAP/POP3 + auth) | ${outOfProfile.filter((row) => row.module.startsWith('ngx_mail')).length} | three protocol state machines, STARTTLS, auth backends (HTTP/IMAP/POP3), and a mail-specific security review. |
| Perl | ${outOfProfile.filter((row) => row.module === 'ngx_http_perl_module').length} | an embedded Perl interpreter - executable configuration, explicitly excluded. |
| GeoIP | ${outOfProfile.filter((row) => row.module.includes('geoip')).length} | a MaxMind database dependency plus an OS/database-specific lookup contract. |
| Dynamic module loading | ${outOfProfile.filter((row) => row.module === 'ngx_core_module').length} | an ABI/loading contract that the static nginx OSS reference binary does not even provide; conflicts with the standalone-only deployment profile. |

### 5.3 Fail-Closed Groups (${failClosed.length} directives, top modules)

${failClosedByModule.map(([module, count]) => `- \`${module}\` - ${count} directive(s)`).join('\n')}

Representative implementation cost:

| Group | Directives | What parity would require |
| --- | ---: | --- |
| gRPC upstream (\`ngx_http_grpc_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_grpc_module').length} | an HTTP/2 gRPC transport with trailers, status mapping, :authority handling, upstream TLS/SNI policy, retry policy - a second proxy protocol family. |
| Rejected proxy knobs (\`ngx_http_proxy_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_proxy_module').length} | cache locking/background update/bypass/method sets, body substitution, upstream-trust knobs: each is a bounded policy feature with its own security review. |
| Rejected stream plumbing (\`ngx_stream_proxy_module\`) | ${failClosed.filter((row) => row.module === 'ngx_stream_proxy_module').length} | buffer sizing, TLS library caches/commands/CRL/password-file knobs; the routing/identity subset is instead in the wave-3 backlog. |
| HTTP/2 tuning (\`ngx_http_v2_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_v2_module').length} | the http2_* set is tracked as wave-3 backlog, so this module is expected to move rather than stay fail-closed. |
| TLS extras (CRL/OCSP-response/ECH/ticket files) | ${failClosed.filter((row) => row.module === 'ngx_http_ssl_module' || row.module === 'ngx_stream_ssl_module').length} | rustls feature gaps (OCSP response handling, raw public keys, certificate compression, session ticket key files) plus a certificate-material pipeline. |
| HTTP/3 + QUIC (\`ngx_http_v3_module\`, \`ngx_quic_bpf_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_v3_module' || row.module === 'ngx_quic_bpf_module').length} | a QUIC transport (UDP, TLS 1.3 QUIC keys, connection migration, congestion control) - a separate transport product, explicitly excluded by PRD section 4. |
| Embedded filters (SSI/XSLT/image/mp4/FLV/empty-gif) | ${failClosed.filter((row) => ['ngx_http_ssi_filter_module', 'ngx_http_xslt_filter_module', 'ngx_http_image_filter_module', 'ngx_http_mp4_module', 'ngx_http_flv_module', 'ngx_http_empty_gif_module'].includes(row.module)).length} | embedded template/language/codec dependencies (libxslt, libgd, mp4/FLV parsers) that the standalone data plane does not ship. |
| \`if\`/\`set\` rewrite control (\`ngx_http_rewrite_module\`) | ${failClosed.filter((row) => row.module === 'ngx_http_rewrite_module').length} | a general conditional rewrite interpreter - contradicts the bounded rewrite state machine PRD section 6 requires. |

### 5.4 Lua, njs, And Third-Party Modules

The 744-row inventory is the pristine nginx **OSS tarball** directive table
(\`inventory.source.thirdPartyModules\` = "excluded (a pristine nginx OSS tarball contains
none)"), so Lua/njs/third-party directives have **zero** rows here even before the profile
decision. They are excluded twice: absent from the inventory, and excluded by PRD section 4
as embedded executable configuration. The reference build's \`configureArgs\` list
(\`--with-http_v2_module\`, \`--with-http_v3_module\`, \`--with-mail\`, \`--with-stream\`, and so
on) describes what the differential oracle contains, not what http-core-v1 promises.

### 5.5 Conclusion

A "parity" target would require: five extra application protocols (FastCGI/uWSGI/SCGI/
memcached/gRPC), three mail protocols, a QUIC transport, three embedded languages/filters,
and operator-tunable internals that the Rust runtime replaces by design - while the
deployment profile forbids the stock nginx process those features exist to serve. The
coherent target is the declared \`${PROFILE}\` profile with the four PRD section 2 grades
(parse, semantic, behavioral, operational), a generated directive catalog, and a
versioned evidence revision. That is what this ledger audits.
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
\`\`\`

The generator reads \`specs/nginx-directive-inventory.json\` and
\`specs/nginx-directive-support.catalog.json\`; it makes no network calls, embeds no
timestamps or git HEAD, and sorts every collection before rendering, so a rerun on the
same catalog is byte-identical. \`--check\` regenerates in memory and compares bytes.

### 6.2 What Changed In This Refresh

This refresh was regenerated after Waves 3a/3a-2 landed and the Lead re-synced the
catalog. The table lists the cumulative completed-wave rows and their current state:

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
