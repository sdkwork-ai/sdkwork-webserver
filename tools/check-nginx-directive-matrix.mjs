#!/usr/bin/env node
/**
 * Drift gate for the directive-level nginx support catalog and the generated
 * operator compatibility matrix.
 *
 *   node tools/check-nginx-directive-matrix.mjs           # check (default)
 *   node tools/check-nginx-directive-matrix.mjs --write   # regenerate the doc
 *   node --test tools/check-nginx-directive-matrix.mjs    # same checks under node:test
 *
 * The gate is offline and deterministic: it reads only committed files. It
 * fails when:
 *
 *   (a) an inventory directive is missing from the support catalog, or the
 *       catalog disagrees with the inventory about name/group/module/contexts;
 *   (b) a catalog directive is absent from the inventory and is not declared
 *       as an sdkwork-only extension;
 *   (c) the classification disagrees with the implementation sources
 *       (`crates/sdkwork-webserver-core/src/nginx/mapping.rs` match arms,
 *       `ACCEPTED_IGNORED`, and the context functions), including contexts the
 *       catalog claims are supported that the source does not recognize;
 *   (d) an `executable` entry has no existing evidence reference (test file +
 *       test/case), or an executable entry is not backed by a source handler;
 *   (e) a committed nginx fixture under `tests/nginx/` or
 *       `deployments/webserver/` uses a directive that has no catalog entry;
 *   (f) the generated operator matrix is out of date relative to the catalog;
 *   (g) the migration CLI (`tools/import-nginx-conf.mjs`) classifies
 *       directives differently from the catalog, or keeps a private
 *       MAPPED/BLOCKED set instead of consuming the committed catalog.
 *
 * Run with `--write` to regenerate `docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { inventoryDigest, serializeInventory } from './gen-nginx-directive-inventory.mjs';
import * as migrationCli from './import-nginx-conf.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(__dirname, '..');
export const INVENTORY_PATH = path.join(REPO_ROOT, 'specs', 'nginx-directive-inventory.json');
export const CATALOG_PATH = path.join(REPO_ROOT, 'specs', 'nginx-directive-support.catalog.json');
export const MATRIX_PATH = path.join(
  REPO_ROOT,
  'docs',
  'guides',
  'operator',
  'NGINX-COMPATIBILITY-MATRIX.md',
);
export const MAPPING_SOURCE_PATH = path.join(
  REPO_ROOT,
  'crates',
  'sdkwork-webserver-core',
  'src',
  'nginx',
  'mapping.rs',
);
export const CLI_PATH = path.join(REPO_ROOT, 'tools', 'import-nginx-conf.mjs');
export const SERVER_TOML_SOURCE_PATH = path.join(
  REPO_ROOT,
  'crates',
  'sdkwork-webserver-core',
  'src',
  'config',
  'server_toml.rs',
);
export const ACTIVATION_SOURCE_PATH = path.join(
  REPO_ROOT,
  'crates',
  'sdkwork-webserver-core',
  'src',
  'nginx_activation.rs',
);

export const CLASSIFICATIONS = ['executable', 'accepted-unenforced', 'unsupported-fail-closed', 'excluded'];
export const RISKS = ['high', 'medium', 'low'];

/** Present in the nginx OSS source but deliberately outside the http-core-v1 product profile (PRD section 4). */
export const EXCLUDED_MODULES = new Set([
  'ngx_http_perl_module',
  'ngx_http_geoip_module',
  'ngx_stream_geoip_module',
]);

/**
 * Directives whose nginx module is excluded by PRD section 4 even when the
 * module name alone is not in {@link EXCLUDED_MODULES}.
 */
export const EXCLUDED_DIRECTIVE_NAMES = new Set([
  'perl',
  'perl_set',
  'perl_modules',
  'perl_require',
  'js_content',
  'js_set',
  'lua_shared_dict',
  'content_by_lua',
  'content_by_lua_block',
  'access_by_lua_block',
  'load_module',
]);

/** Context functions in `mapping.rs` and the profile context label each governs. */
export const CONTEXT_FUNCTIONS = [
  { fn: 'materialize_nginx_app', context: 'http' },
  { fn: 'materialize_server', context: 'http-server' },
  { fn: 'materialize_location', context: 'http-location' },
  { fn: 'materialize_upstream', context: 'http-upstream', also: 'stream-upstream' },
  { fn: 'materialize_stream_context', context: 'stream' },
  { fn: 'materialize_stream_server', context: 'stream-server' },
  { fn: 'materialize_mail_context', context: 'mail' },
  { fn: 'materialize_mail_server', context: 'mail-server' },
];

/** Top-level (main) context directives accepted by `extract_http_context`. */
export const MAIN_STRUCTURAL = new Set(['http', 'stream', 'mail']);

/**
 * Directives the parser executes before materialization instead of a
 * `mapping.rs` match arm: `include` is expanded by
 * `crates/sdkwork-webserver-core/src/nginx/parser.rs::expand_includes`.
 */
export const PARSER_EXECUTED = new Map([
  ['include', 'crates/sdkwork-webserver-core/src/nginx/parser.rs::expand_includes'],
  // `map` is materialized by the context pre-scan in `materialize_nginx_app`
  // (parse_map + Mapper::materialize_map) before the directive loop runs, so
  // the loop's `"map" => {}` arm is not evidence that the value is dropped.
  // Keyed by catalog id, not name: the stream `map` directive is still
  // accepted-unenforced because the stream plane evaluates no variables.
  ['http:map', 'crates/sdkwork-webserver-core/src/nginx/mapping.rs::materialize_nginx_app map pre-scan'],
  // `geo` / `split_clients` are materialized by the same context pre-scan in
  // `materialize_nginx_app` (parse_geo / parse_split_clients), before the
  // directive loop runs.
  ['http:geo', 'crates/sdkwork-webserver-core/src/nginx/mapping.rs::materialize_nginx_app geo pre-scan'],
  ['http:split_clients', 'crates/sdkwork-webserver-core/src/nginx/mapping.rs::materialize_nginx_app split_clients pre-scan'],
  ['http:fastcgi_pass', 'mapping materialize_location fastcgi arm'],
  ['http:fastcgi_index', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_param', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_split_path_info', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_keep_conn', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_buffering', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_request_buffering', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_pass_request_headers', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_connect_timeout', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_send_timeout', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_read_timeout', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_hide_header', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_pass_header', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_intercept_errors', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_next_upstream', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_next_upstream_tries', 'mapping fastcgi settings pipeline'],
  ['http:fastcgi_next_upstream_timeout', 'mapping fastcgi settings pipeline'],
  ['http:uwsgi_pass', 'mapping materialize_location uwsgi arm'],
  ['http:uwsgi_param', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_pass_request_headers', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_buffering', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_request_buffering', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_connect_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_send_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_read_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_hide_header', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_pass_header', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_intercept_errors', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_next_upstream', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_next_upstream_tries', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_next_upstream_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_modifier1', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_modifier2', 'mapping cgi-upstream settings pipeline'],
  ['http:uwsgi_string', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_pass', 'mapping materialize_location scgi arm'],
  ['http:scgi_param', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_pass_request_headers', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_buffering', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_request_buffering', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_connect_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_send_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_read_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_hide_header', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_pass_header', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_intercept_errors', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_next_upstream', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_next_upstream_tries', 'mapping cgi-upstream settings pipeline'],
  ['http:scgi_next_upstream_timeout', 'mapping cgi-upstream settings pipeline'],
  ['http:memcached_pass', 'mapping materialize_location memcached arm'],
  ['http:memcached_connect_timeout', 'mapping memcached settings pipeline'],
  ['http:memcached_send_timeout', 'mapping memcached settings pipeline'],
  ['http:memcached_read_timeout', 'mapping memcached settings pipeline'],
  ['http:memcached_buffering', 'mapping memcached settings pipeline'],
  ['http:memcached_next_upstream', 'mapping memcached settings pipeline'],
  ['http:memcached_next_upstream_tries', 'mapping memcached settings pipeline'],
  ['http:memcached_next_upstream_timeout', 'mapping memcached settings pipeline'],
  ['http:set', 'mapping materialize_location bounded set arm'],
  ['mail:mail', 'extract_mail_context mail block wrapper'],
  ['mail:server', 'materialize_mail_context server blocks'],
  ['mail:listen', 'mapping mail settings pipeline'],
  ['mail:protocol', 'mapping mail settings pipeline'],
  ['mail:server_name', 'mapping mail settings pipeline'],
  ['mail:timeout', 'mapping mail settings pipeline'],
  ['mail:auth_http', 'mapping mail settings pipeline'],
  ['mail:auth_http_header', 'mapping mail settings pipeline'],
  ['mail:auth_http_timeout', 'mapping mail settings pipeline'],
  ['mail:starttls', 'mapping mail settings pipeline'],
  ['mail:xclient', 'mapping mail settings pipeline'],
  ['mail:proxy', 'mapping mail settings pipeline'],
  ['mail:proxy_buffer', 'mapping mail settings pipeline'],
  ['mail:proxy_timeout', 'mapping mail settings pipeline'],
  ['mail:proxy_pass_error_message', 'mapping mail settings pipeline'],
  ['mail:proxy_smtp_auth', 'mapping mail settings pipeline'],
  ['mail:proxy_protocol', 'mapping mail settings pipeline'],
  ['mail:max_errors', 'mapping mail settings pipeline'],
  ['mail:smtp_auth', 'mapping mail settings pipeline'],
  ['mail:smtp_greeting_delay', 'mapping mail settings pipeline'],
  ['mail:smtp_client_buffer', 'mapping mail settings pipeline'],
  ['mail:imap_auth', 'mapping mail settings pipeline'],
  ['mail:imap_client_buffer', 'mapping mail settings pipeline'],
  ['mail:pop3_auth', 'mapping mail settings pipeline'],
  ['mail:ssl_certificate', 'mapping mail settings pipeline'],
  ['mail:ssl_certificate_key', 'mapping mail settings pipeline'],
  ['mail:ssl_protocols', 'mapping mail settings pipeline'],
  ['http:fastcgi_bind', 'mapping cgi settings transport fields'],
  ['http:fastcgi_socket_keepalive', 'mapping cgi settings transport fields'],
  ['http:uwsgi_bind', 'mapping cgi settings transport fields'],
  ['http:uwsgi_socket_keepalive', 'mapping cgi settings transport fields'],
  ['http:scgi_bind', 'mapping cgi settings transport fields'],
  ['http:scgi_socket_keepalive', 'mapping cgi settings transport fields'],
  ['http:memcached_bind', 'mapping cgi settings transport fields'],
  ['http:memcached_socket_keepalive', 'mapping cgi settings transport fields'],
  ['http:grpc_pass', 'mapping materialize_location grpc arm'],
  ['http:grpc_set_header', 'mapping grpc settings pipeline'],
  ['http:grpc_connect_timeout', 'mapping grpc settings pipeline'],
  ['http:grpc_send_timeout', 'mapping grpc settings pipeline'],
  ['http:grpc_read_timeout', 'mapping grpc settings pipeline'],
  ['http:grpc_buffer_size', 'mapping grpc settings pipeline'],
  ['http:grpc_hide_header', 'mapping grpc settings pipeline'],
  ['http:grpc_pass_header', 'mapping grpc settings pipeline'],
  ['http:grpc_intercept_errors', 'mapping grpc settings pipeline'],
  ['http:grpc_next_upstream', 'mapping grpc settings pipeline'],
  ['http:grpc_next_upstream_tries', 'mapping grpc settings pipeline'],
  ['http:grpc_next_upstream_timeout', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_verify', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_verify_depth', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_server_name', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_name', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_certificate', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_certificate_key', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_trusted_certificate', 'mapping grpc settings pipeline'],
  ['http:grpc_ssl_protocols', 'mapping grpc settings pipeline'],
  // Wave 7B: `ngx_http_uwsgi_module` upstream TLS. `CgiSslSettings::apply`
  // parses the family by suffix, so the per-context match arm only carries a
  // prefix guard; the implemented forms are listed by catalog id. The remaining
  // uwsgi_ssl_* forms (ciphers/crl/session_reuse/conf_command/password_file/
  // certificate_cache/verify_depth) are fail-closed and stay unlisted.
  ['http:uwsgi_ssl_verify', 'mapping CgiSslSettings pipeline (uWSGI upstream TLS)'],
  ['http:uwsgi_ssl_trusted_certificate', 'mapping CgiSslSettings pipeline (uWSGI upstream TLS)'],
  ['http:uwsgi_ssl_certificate', 'mapping CgiSslSettings pipeline (uWSGI upstream TLS)'],
  ['http:uwsgi_ssl_certificate_key', 'mapping CgiSslSettings pipeline (uWSGI upstream TLS)'],
  ['http:uwsgi_ssl_server_name', 'mapping CgiSslSettings pipeline (uWSGI upstream TLS)'],
  ['http:uwsgi_ssl_name', 'mapping CgiSslSettings pipeline (uWSGI upstream TLS)'],
  ['http:uwsgi_ssl_protocols', 'mapping CgiSslSettings pipeline (uWSGI upstream TLS)'],
]);

/**
 * These directives are recognized by a source match arm but the arm does not
 * carry the operator's value into the runtime model. The gate's source
 * derivation reports them as "executed" (a state change happens); the catalog
 * downgrades them to `accepted-unenforced` with an explicit reason. A missing
 * override is a gate failure, so the downgrade set cannot silently grow.
 */
export const PARTIAL_ENFORCEMENT = {
  'http:proxy_cache_key': 'the directive only enables the proxy cache; the configured key expression is not applied, the runtime uses its own cache key',
  'http:keepalive': 'the nginx upstream keepalive pool size is not carried into the nginx materializer (the typed TOML path owns the upstream pool)',
  'http:keepalive_timeout': 'the nginx upstream keepalive idle timeout is not carried into the nginx materializer (the typed TOML path owns the upstream pool)',
  'http:keepalive_requests': 'the nginx upstream keepalive request ceiling is not carried into the nginx materializer (the typed TOML path owns the upstream pool)',
  'http:keepalive_time': 'the nginx upstream keepalive connection lifetime is not carried into the nginx materializer (the typed TOML path owns the upstream pool)',
};

function stripRustComments(text) {
  // Only `//` line comments and `/* */` blocks; this file has no raw strings
  // inside the parsed regions, but skip string literals defensively.
  let out = '';
  let i = 0;
  let state = 'code';
  while (i < text.length) {
    const c = text[i];
    const n = text[i + 1];
    if (state === 'code') {
      if (c === '/' && n === '/') {
        while (i < text.length && text[i] !== '\n') i += 1;
        continue;
      }
      if (c === '/' && n === '*') {
        state = 'block';
        i += 2;
        continue;
      }
      if (c === '"') {
        state = 'string';
        out += c;
        i += 1;
        continue;
      }
      out += c;
      i += 1;
      continue;
    }
    if (state === 'block') {
      if (c === '*' && n === '/') {
        state = 'code';
        i += 2;
        continue;
      }
      if (c === '\n') out += '\n';
      i += 1;
      continue;
    }
    out += c;
    if (c === '\\') {
      out += n ?? '';
      i += 2;
      continue;
    }
    if (c === '"') state = 'code';
    i += 1;
  }
  return out;
}

function matchingBrace(text, openIndex) {
  let depth = 0;
  let index = openIndex;
  while (index < text.length) {
    const c = text[index];
    if (c === '{') depth += 1;
    else if (c === '}') {
      depth -= 1;
      if (depth === 0) return index;
    }
    index += 1;
  }
  throw new Error('unbalanced braces while parsing implementation source');
}

function readRustConstArray(text, constName) {
  const match = new RegExp(`const ${constName}: &\\[&str\\] = &\\[([\\s\\S]*?)\\];`, 'u').exec(text);
  if (!match) throw new Error(`unable to locate ${constName} in mapping.rs`);
  return new Set([...match[1].matchAll(/"([^"]+)"/gu)].map((item) => item[1]));
}

/**
 * Parse the arms of `match <expr>.name.as_str() { ... }` inside a function
 * body. Returns one record per arm:
 *   { names, catchall: 'accepted'|'unsupported'|null, kind: 'execute'|'noop'|'error' }
 */
export function parseMatchArms(body) {
  const matchStart = /match\s+[\w.&]*\.name\.as_str\(\)\s*\{/u.exec(body);
  if (!matchStart) return [];
  const braceIndex = body.indexOf('{', matchStart.index + matchStart[0].length - 1);
  const matchEnd = matchingBrace(body, braceIndex);
  const matchBody = body.slice(braceIndex + 1, matchEnd);
  const lineStart = body.lastIndexOf('\n', matchStart.index) + 1;
  const matchIndent = matchStart.index - lineStart;
  const lines = matchBody.split('\n');
  const armIndent = matchIndent + 4;
  const arms = [];
  let current = null;
  const finalize = () => {
    if (!current) return;
    const pattern = current.patternLines.join('\n');
    const bodyText = current.bodyLines.join('\n');
    const names = [...pattern.matchAll(/"([^"]+)"/gu)].map((item) => item[1]);
    let catchall = null;
    if (/ACCEPTED_IGNORED/u.test(pattern)) catchall = 'accepted';
    else if (/UNSUPPORTED_SECURITY/u.test(pattern)) catchall = 'unsupported';
    let kind = 'execute';
    const compact = bodyText.replace(/\s+/gu, ' ').trim();
    const startsWithErr = /^\{? ?return Err\(/u.test(compact) || /^\{\s*return Err\(/u.test(bodyText.trim());
    if (/^\{\s*\}$/u.test(bodyText.trim())) kind = 'noop';
    else if (startsWithErr) kind = 'error';
    arms.push({ names, catchall, kind, indent: current.indent });
    current = null;
  };
  for (const line of lines) {
    const indent = line.length - line.trimStart().length;
    const trimmed = line.trim();
    // A `| "name"` continuation belongs to the in-flight pattern; only start a
    // new arm once the previous pattern has reached its `=>`.
    const inPattern = current?.inPattern === true;
    const startsArm = !inPattern && indent === armIndent
      && (trimmed.startsWith('"') || trimmed.startsWith('|') || trimmed.startsWith('name if ')
        || /^\w+ if /u.test(trimmed) || /^[A-Za-z_][A-Za-z0-9_]*\s*=>/u.test(trimmed)
        || trimmed.startsWith('_ =>'));
    if (startsArm) {
      finalize();
      current = { indent, patternLines: [line], bodyLines: [], inPattern: true };
    }
    if (!current) continue;
    if (current.inPattern) {
      if (current.patternLines[current.patternLines.length - 1] !== line
        || current.patternLines.length > 1) {
        current.patternLines.push(line);
      }
      if (line.includes('=>')) {
        current.inPattern = false;
        const after = line.slice(line.indexOf('=>') + 2);
        if (after.trim()) current.bodyLines.push(after);
      }
    } else {
      current.bodyLines.push(line);
    }
  }
  finalize();
  return arms;
}

/** Locate a function body in `mapping.rs` by name. */
export function functionBody(text, name) {
  const pattern = new RegExp(`(?:pub(?:\\([a-z()]+\\))?\\s+)?fn\\s+${name}\\s*\\(`, 'u');
  const match = pattern.exec(text);
  if (!match) throw new Error(`unable to locate fn ${name} in mapping.rs`);
  const brace = text.indexOf('{', match.index + match[0].length);
  const end = matchingBrace(text, brace);
  return text.slice(brace + 1, end);
}

/**
 * Derive the shared-edge activation policy from `nginx_activation.rs`.
 *
 * The migration importer supports `include`/`alias`, but the staging gate for
 * tenant-supplied fragments on a shared edge rejects them (`scan_nginx_directive_risks`
 * is also what `sdkwork-webserver-edge-runtime` calls before activating a
 * staged nginx site file). This is a third classifier that must not drift from
 * the catalog metadata.
 */
export function deriveActivationPolicy() {
  const text = stripRustComments(fs.readFileSync(ACTIVATION_SOURCE_PATH, 'utf8'));
  const proxyMatch = /const PROXY_DIRECTIVES: &\[&str\] = &\[([\s\S]*?)\];/u.exec(text);
  if (!proxyMatch) throw new Error('unable to locate PROXY_DIRECTIVES in nginx_activation.rs');
  const proxyDirectives = [...proxyMatch[1].matchAll(/"([^"]+)"/gu)].map((item) => item[1]).sort();
  const forbidden = new Set();
  for (const match of text.matchAll(/"([a-z_]+)"\s*=>/gu)) {
    if (['include', 'alias', 'set', 'map'].includes(match[1])) forbidden.add(match[1]);
  }
  if (/"set" \| "map"/u.test(text)) {
    forbidden.add('set');
    forbidden.add('map');
  }
  return { proxyDirectives, stagingForbidden: [...forbidden].sort() };
}

/**
 * Derive the typed TOML keys that the TOML plane accepts but does not enforce
 * (`server_toml.rs::UNENFORCED_BEHAVIOR_TOML_KEYS`). The catalog mirrors this
 * set so a migration action never points an operator at a second silent no-op.
 */
export function deriveUnenforcedTomlKeys() {
  const text = stripRustComments(fs.readFileSync(SERVER_TOML_SOURCE_PATH, 'utf8'));
  const match = /const UNENFORCED_BEHAVIOR_TOML_KEYS: &\[\(&str, &str\)\] = &\[([\s\S]*?)\n\];/u.exec(text);
  if (!match) throw new Error('unable to locate UNENFORCED_BEHAVIOR_TOML_KEYS');
  const entries = [];
  const tupleRe = /\(\s*"([^"]+)",\s*"((?:[^"\\]|\\.)*)"\s*,?\s*\)/gu;
  let tuple;
  while ((tuple = tupleRe.exec(match[1]))) {
    entries.push({ key: tuple[1], consequence: tuple[2].replace(/\\"/gu, '"') });
  }
  return entries.sort((a, b) => a.key.localeCompare(b.key));
}

/**
 * Derive the authoritative typed-TOML enforcement facts from `server_toml.rs`:
 *
 *  - `acceptedIgnored`: the full `ACCEPTED_IGNORED` allow-list (never read by a
 *    materializer consumer; the const exists so stock configs load);
 *  - `behaviorKeys`: the curated `UNENFORCED_BEHAVIOR_TOML_KEYS` behaviour
 *    subset with the operator consequence;
 *  - `supportedKeys`: keys listed in a `check_supported_keys` context list or a
 *    `*_KEYS` const (validation surface, not proof of consumption);
 *  - `consumedKeys`: keys actually read by the typed-TOML materializer in
 *    non-test code (`get`/`get_mut`/`remove`, `as_*` helper reads, bracket
 *    indexing, and dynamic key-pair loops such as allow/deny).
 *
 * A typed key is enforced only when it is not in the accepted-ignored or
 * behaviour sets and the materializer actually consumes it. A key that only
 * appears in a supported-context list is NOT enforced (that was the root cause
 * of the migration-action over-claims).
 */
export function deriveTypedTomlFacts() {
  const source = fs.readFileSync(SERVER_TOML_SOURCE_PATH, 'utf8');
  const testStart = source.indexOf('#[cfg(test)]');
  const production = testStart >= 0 ? source.slice(0, testStart) : source;
  const acceptedMatch = /const ACCEPTED_IGNORED: &\[&str\] = &\[([\s\S]*?)\];/u.exec(production);
  if (!acceptedMatch) throw new Error('unable to locate ACCEPTED_IGNORED in server_toml.rs');
  const acceptedIgnored = new Set(
    [...acceptedMatch[1].matchAll(/"([^"]+)"/gu)].map((item) => item[1]),
  );
  const behaviorKeys = deriveUnenforcedTomlKeys();
  const supportedKeys = new Set();
  for (const match of source.matchAll(/const ([A-Z_]+_KEYS): &\[&str\] = &\[([\s\S]*?)\];/gu)) {
    for (const item of match[2].matchAll(/"([^"]+)"/gu)) supportedKeys.add(item[1]);
  }
  for (const match of source.matchAll(/check_supported_keys\([^;]*?&\[([\s\S]*?)\]\)/gu)) {
    for (const item of match[1].matchAll(/"([^"]+)"/gu)) supportedKeys.add(item[1]);
  }
  const consumedKeys = new Set();
  for (const match of production.matchAll(/\.(?:get|get_mut|remove)\("([^"]+)"/gu)) consumedKeys.add(match[1]);
  for (const match of production.matchAll(/as_(?:array|object|str|u64|bool)\([^;\n]*?"([^"]+)"\s*\)/gu)) {
    consumedKeys.add(match[1]);
  }
  for (const match of production.matchAll(/\["([^"]+)"\]/gu)) consumedKeys.add(match[1]);
  // Dynamic key loops, e.g. `for (action, key) in [("allow", "allow"), ("deny", "deny")]`.
  for (const match of production.matchAll(/for\s*\([^)]*\)\s*in\s*\[([^\]]*)\]/gu)) {
    for (const item of match[1].matchAll(/"([^"]+)"/gu)) consumedKeys.add(item[1]);
  }
  return { acceptedIgnored, behaviorKeys, supportedKeys, consumedKeys };
}

/** Authoritative enforcement verdict for one typed TOML key. */
export function typedKeyVerdict(key, facts) {
  if (facts.behaviorKeys.some((entry) => entry.key === key)) {
    return { enforced: false, reason: 'documented unenforced behavior key' };
  }
  if (facts.acceptedIgnored.has(key)) {
    return { enforced: false, reason: 'accepted-but-ignored by the typed TOML materializer' };
  }
  if (!facts.consumedKeys.has(key)) {
    return { enforced: false, reason: 'not consumed by the typed TOML materializer' };
  }
  return { enforced: true, reason: 'consumed by the typed TOML materializer' };
}

/**
 * The complete unenforced typed-key set the catalog must publish: the curated
 * behaviour subset plus every importer-mapped key the authoritative verdict
 * rejects. Sorted, de-duplicated by key.
 */
export function deriveUnenforcedTypedKeys(catalog, facts) {
  const byKey = new Map();
  for (const entry of facts.behaviorKeys) {
    byKey.set(entry.key, { key: entry.key, reason: 'documented unenforced behavior key', consequence: entry.consequence });
  }
  for (const entry of catalog.directives) {
    if (!entry.importer?.mapped) continue;
    const key = entry.importer.tomlPath?.split('.').pop();
    if (!key) continue;
    const verdict = typedKeyVerdict(key, facts);
    if (verdict.enforced || byKey.has(key)) continue;
    byKey.set(key, { key, reason: verdict.reason });
  }
  return [...byKey.values()].sort((a, b) => a.key.localeCompare(b.key));
}

/**
 * Match arms may guard on a family prefix (`name if name.starts_with("fastcgi_")`)
 * rather than naming a directive; those prefix markers are not directives and
 * must not become catalog extensions or classification evidence.
 */
function directiveNames(names) {
  return names.filter((name) => !name.endsWith('_'));
}

/**
 * Derive the directive recognition facts from the implementation sources.
 *
 * `contexts[context]` contains four sets:
 *   execute  - arms that carry the declaration into the runtime model
 *   noop     - arms that accept the directive and drop its value
 *   error    - arms that always reject the directive
 *   accepted - directives accepted by the ACCEPTED_IGNORED catch-all arm
 *
 * `main` is the top-level nginx context handled by `extract_http_context`.
 */
export function deriveImplementation() {
  const text = stripRustComments(fs.readFileSync(MAPPING_SOURCE_PATH, 'utf8'));
  const acceptedIgnored = readRustConstArray(text, 'ACCEPTED_IGNORED');
  const unsupportedSecurity = readRustConstArray(text, 'UNSUPPORTED_SECURITY');
  const contexts = {};
  const ensure = (context) => {
    contexts[context] ??= { execute: new Set(), noop: new Set(), error: new Set(), accepted: new Set() };
    return contexts[context];
  };
  for (const { fn, context, also } of CONTEXT_FUNCTIONS) {
    const arms = parseMatchArms(functionBody(text, fn));
    const record = ensure(context);
    const targets = also ? [record, ensure(also)] : [record];
    for (const arm of arms) {
      for (const target of targets) {
        if (arm.catchall === 'accepted') {
          for (const name of acceptedIgnored) target.accepted.add(name);
        } else if (arm.catchall === 'unsupported') {
          for (const name of unsupportedSecurity) target.error.add(name);
        } else if (arm.kind === 'noop') {
          for (const name of directiveNames(arm.names)) target.noop.add(name);
        } else if (arm.kind === 'error') {
          for (const name of directiveNames(arm.names)) target.error.add(name);
        } else {
          for (const name of directiveNames(arm.names)) target.execute.add(name);
        }
      }
    }
  }
  // The two block-wrapper directives are unwrapped structurally before the
  // http match runs, and top-level accepted tuning comes from
  // `extract_http_context` (ACCEPTED_IGNORED + the http/stream wrappers).
  const main = ensure('main');
  for (const name of acceptedIgnored) main.accepted.add(name);
  for (const name of MAIN_STRUCTURAL) main.execute.add(name);
  const http = ensure('http');
  for (const name of MAIN_STRUCTURAL) http.execute.add(name);
  // `events` stays accepted via the ACCEPTED_IGNORED catch-all arm.
  return {
    acceptedIgnored,
    unsupportedSecurity,
    contexts,
    activationPolicy: deriveActivationPolicy(),
  };
}

/**
 * Recommend a catalog classification/supported contexts for one inventory
 * entry from the implementation facts. This is the drift baseline the gate
 * compares the committed catalog against.
 */
export function recommendClassification(entry, implementation) {
  const executableContexts = new Set();
  const acceptedContexts = new Set();
  const nginxContexts = new Set(entry.contexts ?? []);
  for (const [context, record] of Object.entries(implementation.contexts)) {
    // Only contexts that are valid for this directive in nginx count; the
    // materializer's permissive catch-all arms accept stock tuning anywhere a
    // block is parsed, but the catalog documents the nginx-declared contexts.
    if (!nginxContexts.has(context)) continue;
    if (record.execute.has(entry.name)) executableContexts.add(context);
    if (record.noop.has(entry.name) || record.accepted.has(entry.name)) acceptedContexts.add(context);
  }
  const groupExcluded = EXCLUDED_MODULES.has(entry.name)
    || entry.modules?.some((module) => EXCLUDED_MODULES.has(module));
  const nameExcluded = EXCLUDED_DIRECTIVE_NAMES.has(entry.name);
  // `mapping.rs` accepts an entire `events {}` block through the
  // ACCEPTED_IGNORED catch-all and never descends into it, so every
  // events-context directive is accepted without being enforced.
  if (nginxContexts.has('events')) acceptedContexts.add('events');
  let classification;
  if (PARSER_EXECUTED.has(entry.name) || PARSER_EXECUTED.has(entry.id)) {
    return {
      classification: 'executable',
      supportedContexts: [...nginxContexts].sort(),
      enforcedContexts: [...nginxContexts].sort(),
      unenforcedContexts: [],
    };
  }
  if (groupExcluded || nameExcluded) classification = 'excluded';
  else if (executableContexts.size > 0) classification = 'executable';
  else if (acceptedContexts.size > 0) classification = 'accepted-unenforced';
  else classification = 'unsupported-fail-closed';
  if (classification === 'executable' && PARTIAL_ENFORCEMENT[entry.id]) {
    classification = 'accepted-unenforced';
  }
  const supportedContexts = [...new Set([...executableContexts, ...acceptedContexts])].sort();
  const enforcedContexts = [...executableContexts].sort();
  const unenforcedContexts = [...acceptedContexts].sort();
  return { classification, supportedContexts, enforcedContexts, unenforcedContexts };
}

export function loadJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

export function loadInventory() {
  return loadJson(INVENTORY_PATH);
}

export function loadCatalog() {
  return loadJson(CATALOG_PATH);
}

/** Read every evidence-bearing test file once. */
function loadEvidenceSources() {
  const cache = new Map();
  return (relative) => {
    if (!cache.has(relative)) {
      const absolute = path.join(REPO_ROOT, relative);
      cache.set(relative, fs.existsSync(absolute) ? fs.readFileSync(absolute, 'utf8') : null);
    }
    return cache.get(relative);
  };
}

export function evidenceProblems(catalog, readSource = loadEvidenceSources()) {
  const problems = [];
  for (const entry of catalog.directives) {
    if (entry.classification !== 'executable') continue;
    if (!Array.isArray(entry.evidence) || entry.evidence.length === 0) {
      problems.push(`${entry.id}: executable entry has no evidence reference`);
      continue;
    }
    let hasProof = false;
    for (const reference of entry.evidence) {
      const source = readSource(reference.file);
      if (source == null) {
        problems.push(`${entry.id}: evidence file does not exist: ${reference.file}`);
        continue;
      }
      if (reference.test) {
        if (!new RegExp(`(?:fn|def)\\s+${reference.test}\\b`, 'u').test(source)
          && !source.includes(reference.test)) {
          problems.push(`${entry.id}: evidence test not found in ${reference.file}: ${reference.test}`);
          continue;
        }
        hasProof = true;
      }
      if (reference.case) {
        if (!source.includes(reference.case)) {
          problems.push(`${entry.id}: evidence case not found in ${reference.file}: ${reference.case}`);
          continue;
        }
        hasProof = true;
      }
    }
    if (!hasProof) problems.push(`${entry.id}: no verifiable evidence reference`);
  }
  return problems;
}

/** Parse the directive names `buildDraft` handles in the migration CLI. */
export function cliHandledDirectiveNames(cliSource) {
  const start = cliSource.indexOf('function buildDraft(');
  assert.ok(start >= 0, 'import-nginx-conf.mjs must export buildDraft');
  const body = cliSource.slice(start);
  const names = new Set();
  for (const match of body.matchAll(/(?:item\.name ===|item\.name !==)\s*'([^']+)'/gu)) names.add(match[1]);
  for (const match of body.matchAll(/case\s+'([^']+)'\s*:/gu)) names.add(match[1]);
  return names;
}

/**
 * Walk an nginx configuration tree and return the directive names it uses,
 * including block directives but excluding the bodies of data blocks whose
 * children are not directives (`types`, `charset_map`, `map`, `geo`,
 * `split_clients`).
 */
export function collectConfigDirectiveNames(tree, options = {}) {
  // `geo` / `split_clients` block bodies are address/percentage rows, not
  // directives (same as `map`); the fixture scan must not require a catalog
  // entry for every row.
  const dataBlocks = new Set(
    options.dataBlocks ?? ['types', 'charset_map', 'map', 'geo', 'split_clients'],
  );
  const names = [];
  const visit = (node) => {
    names.push(node.name);
    if (dataBlocks.has(node.name)) return;
    for (const child of node.children) visit(child);
  };
  for (const node of tree) visit(node);
  return names;
}

/** Discover committed nginx fixtures that must classify cleanly. */
export function fixtureFiles() {
  const roots = [
    ['tests/nginx', ['.conf']],
    ['deployments/webserver', ['.conf']],
  ];
  const files = [];
  const walk = (relative) => {
    const absolute = path.join(REPO_ROOT, relative);
    if (!fs.existsSync(absolute)) return;
    for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
      const child = path.join(relative, entry.name);
      if (entry.isDirectory()) walk(child);
      else if (entry.name.endsWith('.conf')) files.push(child);
    }
  };
  for (const [root] of roots) walk(root);
  return files.sort();
}

const STATUS_LABELS = {
  executable: 'executable',
  'accepted-unenforced': 'accepted, NOT enforced',
  'unsupported-fail-closed': 'rejected (fail closed)',
  excluded: 'excluded from profile',
};

const STATUS_ORDER = [
  'executable',
  'accepted-unenforced',
  'unsupported-fail-closed',
  'excluded',
];

/** High-risk rationale rendered in the migration guide. */
function riskSummary(catalog) {
  return catalog.directives
    .filter((entry) => entry.classification === 'accepted-unenforced' && entry.risk === 'high')
    .sort((a, b) => a.id.localeCompare(b.id));
}

function escapeCell(value) {
  return String(value ?? '')
    .replace(/\|/gu, '\\|')
    .replace(/\n/gu, ' ')
    .replace(/\s+/gu, ' ')
    .trim();
}

function groupHeading(group) {
  return {
    core: 'Core (`core`)',
    events: 'Events (`events`)',
    http: 'HTTP (`http`)',
    stream: 'Stream (`stream`)',
    mail: 'Mail (`mail`)',
  }[group] ?? group;
}

function formatContexts(contexts) {
  return (contexts ?? []).join(', ') || '—';
}

function formatEvidence(entry) {
  if (!Array.isArray(entry.evidence) || entry.evidence.length === 0) return '—';
  return entry.evidence
    .map((reference) => {
      const suffix = reference.test ?? reference.case ?? '';
      return `${reference.file}${suffix ? ` (${suffix})` : ''}`;
    })
    .join('<br>');
}

function renderDirectiveRow(entry) {
  const notes = [];
  if (entry.runtimeField) notes.push(`runtime: \`${entry.runtimeField}\``);
  if (entry.migrationImpact) notes.push(entry.migrationImpact);
  if (entry.knownDifferences?.length) notes.push(entry.knownDifferences.join('; '));
  return `| \`${entry.name}\` | ${escapeCell(entry.grammar ?? '—')} | `
    + `${escapeCell(formatContexts(entry.contexts))} | ${STATUS_LABELS[entry.classification]} | `
    + `${escapeCell(notes.join('. '))} |`;
}

export function serializeCatalog(catalog) {
  return `${JSON.stringify(catalog, null, 2)}\n`;
}

/**
 * The migration action shown for an accepted-but-unenforced directive. It may
 * only name a typed TOML key whose runtime classification is enforced; when the
 * typed equivalent is itself accepted-but-unenforced the action names the
 * catalog capability to track instead.
 */
export function migrationActionFor(entry) {
  if (entry.classification !== 'accepted-unenforced') return undefined;
  const importer = entry.importer;
  if (importer?.mapped && importer.tomlEnforced) {
    if (importer.tomlSemantics) {
      return `Import renders typed key \`${importer.tomlPath}\`, but the runtime consumes it as `
        + `${importer.tomlSemantics}; treat it as an intentional semantic difference, not an equivalent.`;
    }
    return `Import renders the enforced typed key \`${importer.tomlPath}\`; verify it with \`--write-toml\`.`;
  }
  if (importer?.mapped) {
    const detail = importer.tomlSemantics
      ? `is consumed as ${importer.tomlSemantics}, not as this directive`
      : 'is accepted-but-ignored by the runtime';
    return `No enforced typed equivalent: typed TOML key \`${importer.tomlPath}\` ${detail}; `
      + `see the migration risk list, remove the directive, or track capability `
      + `\`${entry.id ?? `${entry.group}:${entry.name}`}\` (specs/nginx-gap.catalog.json).`;
  }
  return `No enforced equivalent is catalogued for this directive; remove it or track capability `
    + `\`${entry.id ?? `${entry.group}:${entry.name}`}\` (specs/nginx-gap.catalog.json).`;
}

/**
 * Reconcile the committed catalog with the implementation sources.
 *
 * This is the maintainer path for a mapping.rs change: it rewrites the derived
 * fields (`classification`, `supportedContexts`, inventory metadata) and flags
 * every entry that needs human review (evidence, risk, migration text) instead
 * of guessing prose. `--write` then regenerates the operator matrix.
 *
 * Extensions are recomputed from the recognized-but-not-nginx directives and
 * the migration CLI's rendered names.
 */
export function syncCatalog(catalog, inventory, implementation, cliHandled) {
  const byId = new Map(catalog.directives.map((entry) => [entry.id, entry]));
  const expected = new Map();
  const problems = [];
  for (const source of inventory.directives) {
    const recommended = recommendClassification(source, implementation);
    const current = byId.get(source.id);
    if (!current) {
      const created = {
        id: source.id,
        name: source.name,
        group: source.group,
        module: source.modules[0] ?? `${source.group}-module`,
        contexts: source.contexts,
        grammar: source.grammar,
        supportedContexts: recommended.supportedContexts,
        classification: recommended.classification,
        risk: 'medium',
        reviewRequired: true,
      };
      problems.push(`${source.id}: new inventory directive needs review (evidence/runtime field/risk)`);
      expected.set(source.id, created);
      continue;
    }
    if (current.classification !== recommended.classification
      || JSON.stringify(current.supportedContexts ?? []) !== JSON.stringify(recommended.supportedContexts)) {
      current.classification = recommended.classification;
      current.supportedContexts = recommended.supportedContexts;
      current.reviewRequired = true;
      problems.push(`${source.id}: classification changed to ${recommended.classification}; review required`);
    }
    current.name = source.name;
    current.group = source.group;
    current.module = source.modules[0] ?? `${source.group}-module`;
    current.contexts = source.contexts;
    current.grammar = source.grammar;
    if (current.classification === 'executable' && !current.runtimeField) {
      current.reviewRequired = true;
      problems.push(`${source.id}: executable but runtimeField is missing`);
    }
    if (current.classification === 'accepted-unenforced'
      && (!current.migrationImpact || !current.diagnostics)) {
      current.reviewRequired = true;
      problems.push(`${source.id}: accepted-unenforced without migrationImpact/diagnostics`);
    }
    expected.set(source.id, current);
  }
  const inventoryNames = new Set(inventory.directives.map((entry) => entry.name));
  const extensionNames = new Set(catalog.extensions.map((entry) => entry.name));
  const recognized = new Set();
  for (const record of Object.values(implementation.contexts)) {
    for (const set of [record.execute, record.accepted, record.noop]) {
      for (const name of set) recognized.add(name);
    }
  }
  for (const name of recognized) {
    if (inventoryNames.has(name) || extensionNames.has(name)) continue;
    const entry = {
      name,
      group: 'sdkwork',
      classification: implementation.acceptedIgnored.has(name) ? 'accepted-unenforced' : 'executable',
      contexts: ['any'],
      supportedContexts: ['any'],
      risk: 'low',
      reason: 'recognized by the SDKWork nginx materializer but not an nginx 1.29.6 directive',
      reviewRequired: true,
    };
    catalog.extensions.push(entry);
    extensionNames.add(name);
    problems.push(`extension ${name}: new sdkwork-only directive needs review`);
  }
  for (const name of cliHandled) {
    if (inventoryNames.has(name) || extensionNames.has(name)) continue;
    catalog.extensions.push({
      name,
      group: 'sdkwork',
      classification: 'executable',
      contexts: ['any'],
      supportedContexts: ['any'],
      risk: 'low',
      reason: 'handled by the migration CLI draft renderer; context-specific pseudo-directive',
      reviewRequired: true,
    });
    extensionNames.add(name);
    problems.push(`extension ${name}: new CLI-handled directive needs review`);
  }
  catalog.directives = inventory.directives
    .map((source) => expected.get(source.id))
    .filter(Boolean)
    .sort((a, b) => a.id.localeCompare(b.id));
  catalog.extensions = catalog.extensions.slice().sort((a, b) => (a.group + a.name).localeCompare(b.group + b.name));
  catalog.inventory = {
    ...catalog.inventory,
    path: 'specs/nginx-directive-inventory.json',
    version: inventory.source.version,
    digest: inventory.digest,
    directiveCount: inventory.directiveCount,
  };
  const facts = deriveTypedTomlFacts();
  const derivedBehavior = deriveUnenforcedTomlKeys();
  const derivedUnenforced = deriveUnenforcedTypedKeys(catalog, facts);
  catalog.typedToml = {
    ...catalog.typedToml,
    source: 'crates/sdkwork-webserver-core/src/config/server_toml.rs::UNENFORCED_BEHAVIOR_TOML_KEYS '
      + '+ ACCEPTED_IGNORED + materializer consumption',
    unenforcedBehaviorKeys: derivedBehavior,
    unenforcedKeys: derivedUnenforced,
  };
  for (const entry of [...catalog.directives, ...catalog.extensions]) {
    if (!entry.importer?.mapped) continue;
    const key = entry.importer.tomlPath ? entry.importer.tomlPath.split('.').pop() : null;
    const verdict = key ? typedKeyVerdict(key, facts) : { enforced: false, reason: 'missing tomlPath key' };
    if (entry.importer.tomlKey !== key || entry.importer.tomlEnforced !== verdict.enforced) {
      entry.importer.tomlKey = key;
      entry.importer.tomlEnforced = verdict.enforced;
    }
  }
  for (const entry of catalog.directives) {
    if (entry.classification !== 'accepted-unenforced') continue;
    const action = migrationActionFor(entry);
    if (action && entry.migrationAction !== action) entry.migrationAction = action;
  }
  return problems;
}

/**
 * Render the operator compatibility matrix from the catalog. Deterministic:
 * the same catalog bytes always produce the same document bytes.
 */
export function renderMatrix(catalog, inventory) {
  const lines = [];
  const counts = Object.fromEntries(STATUS_ORDER.map((status) => [status, 0]));
  for (const entry of catalog.directives) counts[entry.classification] += 1;
  const groups = [...new Set(catalog.directives.map((entry) => entry.group))].sort();
  lines.push('# Nginx Directive Compatibility Matrix (`http-core-v1`)');
  lines.push('');
  lines.push('<!-- Generated by tools/check-nginx-directive-matrix.mjs --write. Do not edit by hand. -->');
  lines.push('');
  lines.push('This page is the operator-facing, generated view of the machine-readable directive support');
  lines.push('catalog required by PRD-nginx-compatibility §3 and §13. It answers one question per nginx');
  lines.push('directive: **if I keep this line in my nginx.conf, what happens on SDKWork Web Server?**');
  lines.push('');
  lines.push('| Field | Value |');
  lines.push('| --- | --- |');
  lines.push(`| Compatibility profile | \`${catalog.profile}\` |`);
  lines.push(`| Nginx reference | \`${inventory.source.vendor} ${inventory.source.version}\` |`);
  lines.push(`| Reference build | ${inventory.referenceBuild.name} |`);
  lines.push(`| Directive inventory | \`specs/nginx-directive-inventory.json\` (${inventory.directiveCount} directives, sha256 \`${inventory.digest}\`) |`);
  lines.push(`| Support catalog | \`specs/nginx-directive-support.catalog.json\` |`);
  lines.push(`| Evidence revision | ${catalog.evidenceRevision} |`);
  lines.push(`| Catalog revision | schema ${catalog.schemaVersion}, profile version ${catalog.profileVersion} |`);
  lines.push(`| Renderer paths | ${escapeCell(Object.values(catalog.renderers ?? {}).join('; '))} |`);
  lines.push('');
  lines.push('## How To Read This Matrix');
  lines.push('');
  lines.push('| Status | Meaning on SDKWork Web Server |');
  lines.push('| --- | --- |');
  lines.push('| **executable** | The Rust data plane enforces the directive and a committed test proves it. The `runtime` note names the normalized field the value reaches. |');
  lines.push('| **accepted, NOT enforced** | The directive is accepted so a stock configuration can be loaded (one diagnostic per occurrence), but the nginx value has no effect. The note says what an operator loses. Treat every such line as a migration task. |');
  lines.push('| **rejected (fail closed)** | An active occurrence is an error in the Rust engine; nothing is silently ignored. Remove the line or keep serving it from stock nginx during migration. It is not represented as supported. |');
  lines.push('| **excluded from profile** | The nginx feature is explicitly out of the `http-core-v1` product profile (PRD §4) and is rejected in an active Rust context. |');
  lines.push('');
  lines.push('`Contexts` are the nginx contexts the directive is valid in (inventory). `runtime` names the');
  lines.push('normalized field reached on the executable path. Directives marked **accepted, NOT enforced**');
  lines.push('are also reported by `node tools/import-nginx-conf.mjs` so a migration review can decide the');
  lines.push('equivalent explicit SDKWork setting.');
  lines.push('');
  lines.push('## Summary');
  lines.push('');
  lines.push('| Classification | Directives |');
  lines.push('| --- | ---: |');
  for (const status of STATUS_ORDER) {
    lines.push(`| ${STATUS_LABELS[status]} | ${counts[status]} |`);
  }
  lines.push(`| **total** | **${catalog.directives.length}** |`);
  lines.push('');

  const risks = riskSummary(catalog);
  lines.push('## Differences From Nginx And Migration Guide');
  lines.push('');
  lines.push('### Highest-risk accepted-but-unenforced directives');
  lines.push('');
  lines.push('These are the directives an operator is most likely to migrate **silently wrong**: the line');
  lines.push('loads without an error, yet the behavior the operator configured does not happen. Each row');
  lines.push('states what is lost and what to configure instead.');
  lines.push('');
  lines.push('| Directive | Contexts | What an operator loses | Migration action |');
  lines.push('| --- | --- | --- | --- |');
  for (const entry of risks) {
    lines.push(
      `| \`${entry.name}\` | ${escapeCell(formatContexts(entry.contexts))} | `
        + `${escapeCell(entry.migrationImpact)} | ${escapeCell(entry.migrationAction ?? 'Configure the equivalent typed TOML key explicitly.')} |`,
    );
  }
  lines.push('');
  lines.push('### Surface that is rejected (fail closed)');
  lines.push('');
  const groupByModule = (entries) => {
    const modules = new Map();
    for (const entry of [...entries].sort((a, b) => a.id.localeCompare(b.id))) {
      const list = modules.get(entry.module) ?? [];
      list.push(entry);
      modules.set(entry.module, list);
    }
    return [...modules.entries()].sort(([a], [b]) => a.localeCompare(b));
  };
  const rejected = catalog.directives
    .filter((entry) => entry.classification === 'unsupported-fail-closed');
  if (rejected.length === 0) {
    lines.push('_No directives are in this class._');
  } else {
    lines.push('An active occurrence of any directive below is an error, never a warning. There is no');
    lines.push('silent approximation. The exhaustive per-directive rows are in the module tables below.');
    lines.push('');
    for (const [module, entries] of groupByModule(rejected)) {
      const names = entries.map((entry) => `\`${entry.name}\``).join(', ');
      lines.push(`- \`${module}\` (${entries.length}) — ${names}`);
    }
  }
  lines.push('');
  lines.push('### Excluded from the product profile');
  lines.push('');
  const excluded = catalog.directives
    .filter((entry) => entry.classification === 'excluded');
  if (excluded.length === 0) {
    lines.push('_No directives are in this class._');
  } else {
    lines.push('These features are out of the `http-core-v1` profile (PRD §4). They are rejected when an');
    lines.push('active Rust context uses them; use typed TOML upstreams instead of CGI/uWSGI/SCGI/');
    lines.push('memcached, and do not import mail or third-party module blocks.');
    lines.push('');
    for (const [module, entries] of groupByModule(excluded)) {
      const names = entries.map((entry) => `\`${entry.name}\``).join(', ');
      lines.push(`- \`${module}\` (${entries.length}) — ${names}`);
    }
  }
  lines.push('');
  lines.push('### Typed TOML keys that are accepted but NOT enforced');
  lines.push('');
  lines.push('These typed TOML keys are accepted so a stock configuration can be loaded, but the TOML');
  lines.push('materializer does not apply them. A migration that renders one of these keys would turn one');
  lines.push('silent no-op into another, so `tools/import-nginx-conf.mjs` does not emit them and reports the');
  lines.push('directive under `accepted` instead. Derived from `server_toml.rs` (ACCEPTED_IGNORED +');
  lines.push('UNENFORCED_BEHAVIOR_TOML_KEYS + actual materializer reads).');
  lines.push('');
  lines.push('| Typed TOML key | Why it has no effect |');
  lines.push('| --- | --- |');
  for (const entry of catalog.typedToml?.unenforcedKeys ?? []) {
    const consequence = entry.consequence ? `${entry.reason} — ${entry.consequence}` : entry.reason;
    lines.push(`| \`${entry.key}\` | ${escapeCell(consequence)} |`);
  }
  lines.push('');
  const semantic = catalog.directives.filter((entry) => entry.importer?.tomlSemantics);
  if (semantic.length > 0) {
    lines.push('### Typed TOML mappings with a different semantic');
    lines.push('');
    for (const entry of semantic) {
      lines.push(`- \`${entry.name}\` → \`${entry.importer.tomlPath}\`: ${escapeCell(entry.importer.tomlSemantics)}.`);
    }
    lines.push('');
  }
  lines.push('### Extraction, rewrite, buffer, and retry limits');
  lines.push('');
  lines.push('Nginx tuning directives listed as accepted-but-unenforced do not change runtime limits: the');
  lines.push('Rust engine owns bounded buffering, timeouts, retry budgets, and pool sizes. A configuration');
  lines.push('that depends on a specific nginx buffer or retry value must be reviewed as an intentional');
  lines.push('behavioral difference rather than assumed equivalent.');
  lines.push('');
  lines.push('## Directive Support By Module');
  lines.push('');
  lines.push('Tables are one row per directive, grouped by nginx module. Use your editor or browser');
  lines.push('search on a directive name (`proxy_pass`, `expires`, ...) to jump to its row.');
  lines.push('');
  for (const group of groups) {
    const groupEntries = catalog.directives
      .filter((entry) => entry.group === group)
      .sort((a, b) => a.name.localeCompare(b.name));
    const modules = [...new Set(groupEntries.map((entry) => entry.module))].sort();
    lines.push(`### ${groupHeading(group)}`);
    lines.push('');
    for (const module of modules) {
      const moduleEntries = groupEntries.filter((entry) => entry.module === module);
      lines.push(`#### \`${module}\` (${moduleEntries.length} directives)`);
      lines.push('');
      lines.push('| Directive | Args | Contexts | Status | Runtime field / difference |');
      lines.push('| --- | --- | --- | --- | --- |');
      for (const entry of moduleEntries) lines.push(renderDirectiveRow(entry));
      lines.push('');
    }
  }
  lines.push('## Version Policy');
  lines.push('');
  lines.push(`- The profile is \`${catalog.profile}\` version ${catalog.profileVersion}; a change to a directive classification is a profile change that must bump the catalog revision and update this page, the catalog, and the evidence together.`);
  lines.push(`- The reference nginx version is \`${inventory.source.version}\`; a different nginx release requires re-running \`node tools/gen-nginx-directive-inventory.mjs --write --source <dir>\` and re-triaging the catalog diff, never a silent upgrade.`);
  lines.push('- Deprecation is explicit: moving a directive from executable to accepted-unenforced is a documented downgrade with a migration note, and moving it to rejected is a breaking profile change.');
  lines.push('');
  lines.push('## Evidence And Reproduce');
  lines.push('');
  lines.push('```sh');
  lines.push('node tools/gen-nginx-directive-inventory.mjs --check --source <unpacked-nginx-1.29.6>');
  lines.push('node tools/check-nginx-directive-matrix.mjs');
  lines.push('node tools/import-nginx-conf.mjs --conf <nginx.conf>');
  lines.push('```');
  lines.push('');
  lines.push('The inventory is parsed from the real nginx source (`ngx_command_t` tables) and the support');
  lines.push('catalog is reconciled against the Rust implementation on every gate run. A change in either');
  lines.push('implementation or catalog fails the gate until both agree; regenerate this page with');
  lines.push('`--write`.');
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function parseArgs(argv) {
  const args = { write: false, sync: false };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--write') args.write = true;
    else if (arg === '--sync') args.sync = true;
    else if (arg === '--check') args.write = false;
    else if (arg === '--help' || arg === '-h') args.help = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

// ---------------------------------------------------------------------------
// node:test checks
// ---------------------------------------------------------------------------

test('inventory is canonical, digest-valid, and derived from nginx 1.29.6', () => {
  const text = fs.readFileSync(INVENTORY_PATH, 'utf8');
  const inventory = JSON.parse(text);
  assert.equal(inventory.schemaVersion, 1);
  assert.equal(inventory.kind, 'sdkwork.webserver.nginx-directive-inventory');
  assert.equal(inventory.source.version, '1.29.6');
  assert.equal(inventory.source.url, 'https://nginx.org/download/nginx-1.29.6.tar.gz');
  assert.equal(inventory.directiveCount, inventory.directives.length);
  assert.ok(inventory.directives.length >= 700, 'inventory must cover the whole OSS directive table');
  assert.equal(inventory.digest, inventoryDigest(inventory), 'inventory digest mismatch');
  assert.equal(text, serializeInventory(inventory), 'inventory file must be canonical JSON');
  assert.ok(inventory.directives.some((entry) => !entry.referenceBuild),
    'inventory records directives outside the reference build too');
  assert.ok(inventory.directives.every((entry) => entry.contexts.length > 0),
    'every directive must declare at least one context');
});

test('support catalog covers the inventory exactly (checks a and b)', () => {
  const inventory = loadInventory();
  const catalog = loadCatalog();
  assert.equal(catalog.schemaVersion, 1);
  assert.equal(catalog.kind, 'sdkwork.webserver.nginx-directive-support.catalog');
  assert.equal(catalog.profile, 'http-core-v1');
  assert.equal(catalog.inventory.digest, inventory.digest, 'catalog must pin the current inventory digest');
  assert.ok(catalog.evidenceRevision, 'catalog must declare an evidence revision');
  assert.equal(fs.readFileSync(CATALOG_PATH, 'utf8'), serializeCatalog(catalog),
    'catalog must be canonical JSON (run --sync --write)');
  for (const entry of catalog.directives) {
    assert.ok(!entry.reviewRequired,
      `${entry.id}: flagged reviewRequired; run --sync and complete the review, then remove the flag`);
  }
  for (const extension of catalog.extensions) {
    assert.ok(!extension.reviewRequired,
      `extension ${extension.name}: flagged reviewRequired; complete the review and remove the flag`);
  }
  assert.ok(Array.isArray(catalog.extensions), 'catalog must declare sdkwork-only extensions explicitly');
  const catalogIds = new Set();
  const problems = [];
  for (const entry of catalog.directives) {
    assert.ok(entry.id && !catalogIds.has(entry.id), `duplicate or missing catalog id: ${entry.id}`);
    catalogIds.add(entry.id);
    assert.ok(CLASSIFICATIONS.includes(entry.classification), `${entry.id}: bad classification`);
    assert.ok(RISKS.includes(entry.risk), `${entry.id}: bad risk`);
    assert.ok(entry.module && entry.group, `${entry.id}: missing module/group`);
    assert.ok(Array.isArray(entry.contexts), `${entry.id}: missing contexts`);
    if (entry.classification === 'accepted-unenforced') {
      assert.ok(entry.migrationImpact, `${entry.id}: accepted-unenforced needs migrationImpact`);
      assert.match(entry.diagnostics ?? '', /accepted|not enforced/u,
        `${entry.id}: accepted-unenforced needs an acceptance diagnostic`);
    }
  }
  const inventoryIds = new Set(inventory.directives.map((entry) => entry.id));
  for (const entry of inventory.directives) {
    if (!catalogIds.has(entry.id)) problems.push(`(a) inventory directive missing from catalog: ${entry.id}`);
  }
  const extensionNames = new Set(catalog.extensions.map((entry) => `${entry.group}:${entry.name}`));
  for (const id of catalogIds) {
    if (!inventoryIds.has(id) && !extensionNames.has(id)) {
      problems.push(`(b) catalog directive is not in the inventory and not declared as an extension: ${id}`);
    }
  }
  for (const extension of catalog.extensions) {
    assert.ok(extension.name && extension.group && extension.classification,
      `extension entry is incomplete: ${JSON.stringify(extension)}`);
    assert.ok(extension.reason, `extension entry ${extension.group}:${extension.name} needs a reason`);
  }
  assert.deepEqual(problems, []);
});

test('catalog agrees with the inventory metadata', () => {
  const inventory = loadInventory();
  const catalog = loadCatalog();
  const byId = new Map(inventory.directives.map((entry) => [entry.id, entry]));
  const problems = [];
  for (const entry of catalog.directives) {
    const source = byId.get(entry.id);
    if (!source) continue;
    if (entry.name !== source.name) problems.push(`${entry.id}: name mismatch`);
    if (entry.group !== source.group) problems.push(`${entry.id}: group mismatch`);
    if (entry.grammar !== source.grammar) {
      problems.push(`${entry.id}: argument grammar drift from inventory (${entry.grammar} vs ${source.grammar})`);
    }
    if (JSON.stringify(entry.contexts) !== JSON.stringify(source.contexts)) {
      problems.push(`${entry.id}: contexts drift from inventory (${JSON.stringify(entry.contexts)} vs ${JSON.stringify(source.contexts)})`);
    }
    if (source.modules.length > 0 && entry.module !== source.modules[0]) {
      problems.push(`${entry.id}: module drift (${entry.module} vs ${source.modules[0]})`);
    }
  }
  assert.deepEqual(problems, []);
});

test('classification is reconciled against mapping.rs sources (check c)', () => {
  const catalog = loadCatalog();
  const inventory = loadInventory();
  const implementation = deriveImplementation();
  const byId = new Map(inventory.directives.map((entry) => [entry.id, entry]));
  const problems = [];
  for (const entry of catalog.directives) {
    if (!byId.has(entry.id)) continue;
    const recommended = recommendClassification(byId.get(entry.id), implementation);
    if (entry.classification !== recommended.classification) {
      const override = PARTIAL_ENFORCEMENT[entry.id];
      if (!override) {
        problems.push(
          `${entry.id}: catalog says ${entry.classification}, implementation implies `
            + `${recommended.classification} (${JSON.stringify(recommended.supportedContexts)})`,
        );
      } else if (entry.classification !== 'accepted-unenforced') {
        problems.push(`${entry.id}: partial-enforcement override must classify accepted-unenforced`);
      }
    }
    const declared = [...(entry.supportedContexts ?? [])].sort();
    if (JSON.stringify(declared) !== JSON.stringify(recommended.supportedContexts)) {
      if (!PARTIAL_ENFORCEMENT[entry.id]) {
        problems.push(
          `${entry.id}: supportedContexts drift: catalog ${JSON.stringify(declared)} vs `
            + `source ${JSON.stringify(recommended.supportedContexts)}`,
        );
      }
    }
  }
  // Reverse direction: every source-recognized directive must be in the
  // catalog under a classification that admits it. The same directive name can
  // exist in two module namespaces (http and stream), so match by name +
  // declared context rather than name alone.
  const catalogByName = new Map();
  for (const entry of [...catalog.directives, ...catalog.extensions]) {
    const list = catalogByName.get(entry.name) ?? [];
    list.push(entry);
    catalogByName.set(entry.name, list);
  }
  const inventoryContexts = new Map();
  for (const entry of inventory.directives) {
    const set = inventoryContexts.get(entry.name) ?? new Set();
    for (const context of entry.contexts) set.add(context);
    inventoryContexts.set(entry.name, set);
  }
  for (const [context, record] of Object.entries(implementation.contexts)) {
    for (const name of record.execute) {
      // The materializer's nested arms are permissive about contexts nginx
      // itself forbids (for example `ip_hash` inside a stream upstream, or a
      // nested `http` block). Only reconcile contexts the directive actually
      // declares in the nginx inventory; extensions are checked separately.
      const declared = inventoryContexts.get(name);
      if (declared && !declared.has(context)) continue;
      const entries = catalogByName.get(name) ?? [];
      if (entries.length === 0) {
        problems.push(`(c) mapping.rs recognizes \`${name}\` in ${context} but the catalog has no entry`);
        continue;
      }
      const admitting = entries.filter((entry) => (entry.contexts ?? ['any']).includes(context)
        || (entry.contexts ?? ['any']).includes('any'))
        .filter((entry) => entry.classification !== 'unsupported-fail-closed'
          && entry.classification !== 'excluded');
      if (admitting.length === 0) {
        const classifications = [...new Set(entries.map((entry) => entry.classification))].join(', ');
        problems.push(
          `(c) mapping.rs executes \`${name}\` in ${context} but the catalog calls it ${classifications}`,
        );
      }
    }
  }
  assert.deepEqual(problems, []);
});

test('executable entries have verifiable evidence (check d)', () => {
  const catalog = loadCatalog();
  const problems = evidenceProblems(catalog);
  assert.deepEqual(problems, []);
});

test('committed nginx fixtures only use catalogued directives (check e)', () => {
  const catalog = loadCatalog();
  const known = new Set(catalog.directives.map((entry) => entry.name));
  for (const extension of catalog.extensions) known.add(extension.name);
  const problems = [];
  for (const file of fixtureFiles()) {
    const source = fs.readFileSync(path.join(REPO_ROOT, file), 'utf8');
    let names;
    try {
      names = collectConfigDirectiveNames(migrationCli.parseConfigTree(source, path.join(REPO_ROOT, file)));
    } catch (error) {
      problems.push(`${file}: cannot parse fixture: ${error.message}`);
      continue;
    }
    for (const name of names) {
      if (!known.has(name)) problems.push(`${file}: directive \`${name}\` is not in the support catalog`);
    }
  }
  assert.deepEqual(problems, []);
});

test('migration CLI consumes the catalog classification (check g)', () => {
  const source = fs.readFileSync(CLI_PATH, 'utf8');
  assert.ok(source.includes('nginx-directive-support.catalog.json'),
    'import-nginx-conf.mjs must load the committed support catalog');
  assert.ok(!/const MAPPED = new Map\(/u.test(source),
    'import-nginx-conf.mjs must not keep a private MAPPED directive map');
  assert.ok(!/const BLOCKED = new Set\(/u.test(source),
    'import-nginx-conf.mjs must not keep a private BLOCKED directive set');
  const catalog = loadCatalog();
  const importerMapped = new Set(
    catalog.directives.filter((entry) => entry.importer?.mapped).map((entry) => entry.name),
  );
  const handled = cliHandledDirectiveNames(source);
  const cli = migrationCli;
  const problems = [];
  for (const name of handled) {
    if (!importerMapped.has(name)) {
      problems.push(`CLI buildDraft handles \`${name}\` but no catalog entry marks it importer.mapped`);
    }
  }
  for (const name of importerMapped) {
    if (!handled.has(name)) {
      problems.push(`catalog importer.mapped lists \`${name}\` but the CLI buildDraft cannot render it`);
    }
  }
  // Behavioural drift: the two Lead-reproduced symptoms plus the now-supported
  // FastCGI family, which must be reported (accepted or mapped) rather than
  // silently dropped or blocked as out-of-profile.
  const fixture = `
http {
  types { text/html html htm; }
  default_type application/octet-stream;
  server {
    listen 80;
    server_name drift.example.com;
    location / { expires 1d; }
    location ~ \\.php$ { fastcgi_pass 127.0.0.1:9000; }
  }
}
`;
  const classified = cli.classify(cli.scanDirectives(fixture));
  const unsupportedNames = classified.unsupported.map((item) => item.name);
  for (const name of ['text/html', 'default_type', 'expires']) {
    if (unsupportedNames.includes(name)) {
      problems.push(`CLI still reports catalogued data/executable directive \`${name}\` as unsupported`);
    }
  }
  if (classified.blocked.some((item) => item.name === 'fastcgi_pass')) {
    problems.push('CLI still reports fastcgi_pass as out-of-profile/blocked although the module now executes');
  }
  // A directive whose typed TOML key is itself accepted-but-unenforced must not
  // be rendered into the draft as if the migration were enforced.
  const unenforcedNames = catalog.directives
    .filter((entry) => entry.importer?.mapped && entry.importer.tomlEnforced === false)
    .map((entry) => entry.name);
  const unenforcedKeys = new Set(
    catalog.directives
      .filter((entry) => entry.importer?.mapped && entry.importer.tomlEnforced === false)
      .map((entry) => entry.importer.tomlPath.split('.').pop()),
  );
  if (unenforcedNames.length > 0) {
    const sampleConfig = [
      'http {',
      '  server {',
      '    listen 80;',
      '    server_name unenforced-check.local;',
      '    location / {',
      ...[...new Set(unenforcedNames)].slice(0, 12).map((name) => `      ${name} on;`),
      '      return 200 ok;',
      '    }',
      '  }',
      '}',
    ].join('\n');
    const cliClassified = cli.classify(cli.scanDirectives(sampleConfig));
    const draft = cli.buildDraft(cliClassified.mapped);
    const toml = cli.emitToml(draft, { id: 'gate-unenforced' });
    for (const entry of catalog.directives) {
      if (entry.classification === 'excluded') continue;
      if (entry.importer?.tomlEnforced === false) {
        const key = entry.importer.tomlPath.split('.').pop();
        const rendered = new RegExp(`^${key}\\s*=|\\.${key}\\s*=`, 'mu').test(toml);
        if (rendered) problems.push(`CLI rendered unenforced typed key \`${entry.importer.tomlPath}\``);
      }
    }
    if (unenforcedKeys.size === 0) problems.push('catalog declares no unenforced importer keys');
  }
  assert.deepEqual(problems, []);
});

test('catalog typed-TOML enforcement is authoritative (check i)', () => {
  const catalog = loadCatalog();
  assert.ok(catalog.typedToml, 'catalog must document typed TOML enforcement');
  const facts = deriveTypedTomlFacts();
  const derivedBehavior = deriveUnenforcedTomlKeys();
  assert.deepEqual(catalog.typedToml.unenforcedBehaviorKeys, derivedBehavior,
    'catalog.typedToml.unenforcedBehaviorKeys drift from server_toml.rs::UNENFORCED_BEHAVIOR_TOML_KEYS');
  const derivedUnenforced = deriveUnenforcedTypedKeys(catalog, facts);
  assert.deepEqual(catalog.typedToml.unenforcedKeys, derivedUnenforced,
    'catalog.typedToml.unenforcedKeys drift from server_toml.rs enforcement facts '
      + '(ACCEPTED_IGNORED + supported lists + actual materializer reads)');
  const unenforced = new Map(derivedUnenforced.map((entry) => [entry.key, entry]));
  for (const entry of catalog.directives) {
    if (!entry.importer?.mapped) continue;
    const key = entry.importer.tomlPath?.split('.').pop();
    assert.ok(key, `${entry.id}: importer.mapped needs a tomlPath`);
    const verdict = typedKeyVerdict(key, facts);
    assert.equal(entry.importer.tomlEnforced, verdict.enforced,
      `${entry.id}: importer.tomlEnforced must equal the authoritative verdict for \`${key}\` (${verdict.reason})`);
    if (!verdict.enforced) {
      assert.ok(!/enforced typed key/u.test(entry.migrationAction ?? ''),
        `${entry.id}: migrationAction still claims an enforced typed key for unenforced \`${key}\``);
    }
    if (entry.classification === 'executable') {
      assert.equal(entry.importer.tomlEnforced, true,
        `${entry.id}: an executable nginx directive may not map to an unenforced typed key`);
    }
    if (entry.importer.tomlEnforced && unenforced.has(key)) {
      assert.fail(`${entry.id}: \`${key}\` is published as unenforced but importer.tomlEnforced is true`);
    }
  }
});

test('executable evidence citations resolve against the audit contract (check j)', () => {
  const catalog = loadCatalog();
  const problems = evidenceProblems(catalog);
  assert.deepEqual(problems, [], 'every executable evidence citation must resolve');
  const facts = deriveTypedTomlFacts();
  for (const entry of catalog.directives) {
    if (entry.classification === 'executable') {
      assert.ok(Array.isArray(entry.evidence) && entry.evidence.length > 0,
        `${entry.id}: executable entry needs evidence`);
      assert.ok(entry.evidence.some((reference) => reference.test || reference.case),
        `${entry.id}: evidence must cite a test function or fixture case id`);
      for (const reference of entry.evidence) {
        assert.ok(reference.file.startsWith('crates/') || reference.file.startsWith('tests/'),
          `${entry.id}: evidence must be a committed test/fixture path, got ${reference.file}`);
        assert.ok(!reference.file.startsWith('specs/') && !reference.file.startsWith('docs/'),
          `${entry.id}: evidence must not cite generated contract/documentation files`);
      }
    }
    if (entry.classification === 'accepted-unenforced'
      && /enforced typed key/u.test(entry.migrationAction ?? '')) {
      const key = entry.importer?.tomlPath?.split('.').pop();
      assert.ok(key, `${entry.id}: an "enforced typed key" action needs an importer.tomlPath`);
      assert.equal(typedKeyVerdict(key, facts).enforced, true,
        `${entry.id}: migrationAction names typed key \`${key}\`, which the materializer does not consume`);
    }
  }
});

test('catalog activation policy matches the shared-edge scanner (check h)', () => {
  const catalog = loadCatalog();
  assert.ok(catalog.activationPolicy, 'catalog must document the shared-edge activation policy');
  const derived = deriveActivationPolicy();
  assert.deepEqual(catalog.activationPolicy.proxyDirectives, derived.proxyDirectives,
    'catalog activationPolicy.proxyDirectives drift from nginx_activation.rs');
  assert.deepEqual(catalog.activationPolicy.stagingForbidden, derived.stagingForbidden,
    'catalog activationPolicy.stagingForbidden drift from nginx_activation.rs');
});

test('generated operator matrix is current (check f)', () => {
  const catalog = loadCatalog();
  const inventory = loadInventory();
  const expected = renderMatrix(catalog, inventory);
  const actual = fs.readFileSync(MATRIX_PATH, 'utf8');
  assert.equal(actual, expected,
    'docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md is stale; run '
      + '`node tools/check-nginx-directive-matrix.mjs --write`');
});

test('catalog summary counts are non-trivial and honest', () => {
  const catalog = loadCatalog();
  const counts = {};
  for (const entry of catalog.directives) counts[entry.classification] = (counts[entry.classification] ?? 0) + 1;
  assert.ok(counts.executable >= 40, 'expected a broad executable surface');
  assert.ok(counts['accepted-unenforced'] >= 50, 'expected the accepted-ignored surface to be catalogued');
  assert.ok(counts['unsupported-fail-closed'] > 0, 'expected fail-closed entries');
  assert.ok(counts.excluded > 0, 'expected profile exclusions');
  for (const entry of catalog.directives.filter((item) => item.classification === 'executable')) {
    assert.ok(entry.runtimeField, `${entry.id}: executable entry must record its normalized runtime field`);
  }
});

// ---------------------------------------------------------------------------
// CLI entrypoint
// ---------------------------------------------------------------------------

const isDirectRun = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isDirectRun) {
  const args = parseArgs(process.argv);
  if (args.help) {
    process.stdout.write('Usage: node tools/check-nginx-directive-matrix.mjs [--check|--write|--sync [--write]]\n');
    process.exit(0);
  }
  if (args.sync) {
    const catalog = loadCatalog();
    const inventory = loadInventory();
    const implementation = deriveImplementation();
    const cliHandled = cliHandledDirectiveNames(fs.readFileSync(CLI_PATH, 'utf8'));
    const problems = syncCatalog(catalog, inventory, implementation, cliHandled);
    fs.writeFileSync(CATALOG_PATH, serializeCatalog(catalog));
    if (args.write) {
      fs.mkdirSync(path.dirname(MATRIX_PATH), { recursive: true });
      fs.writeFileSync(MATRIX_PATH, renderMatrix(catalog, inventory));
      process.stdout.write(`wrote ${path.relative(REPO_ROOT, MATRIX_PATH)}\n`);
    }
    process.stdout.write(`synced ${path.relative(REPO_ROOT, CATALOG_PATH)}\n`);
    for (const problem of problems) process.stdout.write(`review: ${problem}\n`);
    process.exit(0);
  }
  if (args.write) {
    const catalog = loadCatalog();
    const inventory = loadInventory();
    fs.mkdirSync(path.dirname(MATRIX_PATH), { recursive: true });
    fs.writeFileSync(MATRIX_PATH, renderMatrix(catalog, inventory));
    process.stdout.write(`wrote ${path.relative(REPO_ROOT, MATRIX_PATH)}\n`);
    process.exit(0);
  }
}
