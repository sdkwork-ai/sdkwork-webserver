#!/usr/bin/env node
/**
 * Classify / draft / emit TOML from an nginx.conf for SDKWork http-core-v1.
 *
 * Usage:
 *   node tools/import-nginx-conf.mjs --conf <nginx.conf> [--module-root <path>]
 *   node tools/import-nginx-conf.mjs --conf <nginx.conf> --write-draft <out.json>
 *   node tools/import-nginx-conf.mjs --conf <nginx.conf> --write-toml <out.toml> [--validate]
 *
 * Activation still requires [nginx].enabled = true and zero
 * unsupported/blocked directives. Emitted TOML is a review draft -- it does
 * not overwrite deployments/webserver/server.*.toml by default.
 */
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { parseTomlSubset } from '../../sdkwork-specs/tools/webserver/toml.mjs';
import { validateWebserverToml } from '../../sdkwork-specs/tools/webserver/validate.mjs';
import { retiredNginxActivationBlock } from '../../sdkwork-specs/tools/webserver/retired-nginx.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..');

const MAPPED = new Map([
  ['listen', 'http.server.listen'],
  ['server_name', 'http.server.serverName'],
  ['http2', 'http.server.http2'],
  ['root', 'http.server.location.root'],
  ['alias', 'http.server.location.alias'],
  ['try_files', 'http.server.location.tryFiles'],
  ['index', 'http.server.location.index'],
  ['proxy_pass', 'http.server.location.proxyPass'],
  ['proxy_set_header', 'http.server.location.proxySetHeader'],
  ['proxy_http_version', 'http.server.location.proxyHttpVersion'],
  ['proxy_buffering', 'http.server.location.proxyBuffering'],
  ['proxy_read_timeout', 'http.server.location.proxyReadTimeout'],
  ['proxy_connect_timeout', 'http.server.location.proxyConnectTimeout'],
  ['proxy_send_timeout', 'http.server.location.proxySendTimeout'],
  ['ssl_certificate', 'http.certificates.certFile'],
  ['ssl_certificate_key', 'http.certificates.certKeyFile'],
  ['ssl_trusted_certificate', 'http.certificates.chainFile'],
  ['ssl_protocols', 'http.server.tls.protocols'],
  ['ssl_prefer_server_ciphers', 'http.server.tls.preferServerCiphers'],
  ['ssl_session_cache', 'http.server.tls.sessionCache'],
  ['ssl_stapling', 'http.server.tls.stapling'],
  ['client_max_body_size', 'http.clientMaxBodySize'],
  ['gzip', 'http.gzip'],
  ['gzip_types', 'http.gzipTypes'],
  ['keepalive_timeout', 'http.keepaliveTimeout'],
  ['sendfile', 'http.sendfile'],
  ['tcp_nopush', 'http.tcpNopush'],
  ['tcp_nodelay', 'http.tcpNodelay'],
  ['server_tokens', 'http.serverTokens'],
  ['worker_processes', 'main.workerProcesses'],
  ['worker_connections', 'main.events.workerConnections'],
  ['user', 'main.user'],
  ['error_log', 'main.errorLog'],
  ['pid', 'main.pid'],
  ['return', 'http.server.location.returnStatus'],
  ['rewrite', 'http.server.location.rewrite'],
  ['add_header', 'http.server.location.addHeader'],
  ['allow', 'http.server.location.allow'],
  ['deny', 'http.server.location.deny'],
  ['limit_req', 'http.server.location.limitReq'],
  ['limit_req_zone', 'http.limitReqZone'],
  ['auth_basic', 'http.server.location.authBasic'],
  ['auth_basic_user_file', 'http.server.location.authBasicUserFile'],
  ['map', 'http.map'],
  ['least_conn', 'http.upstream.loadBalancing'],
  ['ip_hash', 'http.upstream.loadBalancing'],
  ['hash', 'http.upstream.hashKey'],
  ['keepalive', 'http.upstream.keepalive'],
  // upstream member targets (context-sensitive; see classify)
  ['server', 'http.upstream.target'],
  // map body default line
  ['default', 'http.map'],
]);

const BLOCKED = new Set([
  'load_module',
  'perl_set',
  'js_content',
  'lua_shared_dict',
  'content_by_lua_block',
  'access_by_lua_block',
]);

const BLOCK_OPENERS = new Set([
  'http', 'server', 'location', 'upstream', 'stream', 'events', 'mail', 'map',
]);

function parseArgs(argv) {
  const out = {
    conf: null,
    moduleRoot: REPO_ROOT,
    writeDraft: null,
    writeToml: null,
    validate: false,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--conf') out.conf = argv[++i];
    else if (arg === '--module-root') out.moduleRoot = path.resolve(argv[++i]);
    else if (arg === '--write-draft') out.writeDraft = path.resolve(argv[++i]);
    else if (arg === '--write-toml') out.writeToml = path.resolve(argv[++i]);
    else if (arg === '--validate') out.validate = true;
    else if (arg === '--help' || arg === '-h') out.help = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  return out;
}

function stripComments(source) {
  return source
    .split(/\r?\n/u)
    .map((line) => {
      const hash = line.indexOf('#');
      return hash === -1 ? line : line.slice(0, hash);
    })
    .join('\n');
}

function scanDirectives(source) {
  const text = stripComments(source);
  const directives = [];
  const stack = [];
  let i = 0;
  let pendingBlock = null;
  let blockSeq = 0;
  while (i < text.length) {
    while (i < text.length && /\s/u.test(text[i])) i += 1;
    if (i >= text.length) break;
    if (text[i] === '{') {
      if (pendingBlock) {
        stack.push(pendingBlock);
        pendingBlock = null;
      } else {
        stack.push({ kind: 'anonymous', header: 'anonymous', id: ++blockSeq });
      }
      i += 1;
      continue;
    }
    if (text[i] === '}') {
      stack.pop();
      pendingBlock = null;
      i += 1;
      continue;
    }
    const start = i;
    while (i < text.length && text[i] !== ';' && text[i] !== '{' && text[i] !== '}') i += 1;
    const chunk = text.slice(start, i).trim().replace(/\s+/gu, ' ');
    if (i < text.length && text[i] === '{') {
      const name = chunk.split(' ')[0];
      pendingBlock = {
        kind: BLOCK_OPENERS.has(name) ? name : 'anonymous',
        header: chunk,
        id: ++blockSeq,
      };
      continue;
    }
    if (i >= text.length || text[i] !== ';') {
      while (i < text.length && text[i] !== '{' && text[i] !== '}') i += 1;
      continue;
    }
    i += 1;
    if (!chunk) continue;
    const name = chunk.split(' ')[0];
    directives.push({
      name,
      statement: `${chunk};`,
      context: stack.map((frame) => frame.kind),
      contextHeader: stack.map((frame) => frame.header || frame.kind),
      contextIds: stack.map((frame) => frame.id),
      serverBlockId: [...stack].reverse().find((frame) => frame.kind === 'server')?.id ?? null,
      upstreamBlockId: [...stack].reverse().find((frame) => frame.kind === 'upstream')?.id ?? null,
      locationHeader: [...stack].reverse().find((frame) => frame.kind === 'location')?.header ?? null,
      upstreamHeader: [...stack].reverse().find((frame) => frame.kind === 'upstream')?.header ?? null,
      mapHeader: [...stack].reverse().find((frame) => frame.kind === 'map')?.header ?? null,
    });
  }
  return directives;
}

function classify(directives) {
  const mapped = [];
  const unsupported = [];
  const blocked = [];
  const preserveOnly = [];
  for (const item of directives) {
    if (BLOCKED.has(item.name)) {
      blocked.push({ ...item, reason: 'notExecutableByRust' });
      continue;
    }
    if (item.name === 'include') {
      preserveOnly.push({ ...item, reason: 'include-resolution-not-yet-implemented' });
      continue;
    }
    if (item.name === 'server' && !item.context.includes('upstream')) {
      unsupported.push({ ...item, reason: 'server-as-statement-outside-upstream' });
      continue;
    }
    if (item.name === 'default' && !item.context.includes('map')) {
      unsupported.push({ ...item, reason: 'default-outside-map' });
      continue;
    }
    if (MAPPED.has(item.name)) {
      mapped.push({ ...item, tomlPath: MAPPED.get(item.name) });
      continue;
    }
    unsupported.push(item);
  }
  return { mapped, unsupported, blocked, preserveOnly };
}

function parseUpstreamTarget(args) {
  const target = { address: args[0] };
  for (const token of args.slice(1)) {
    if (token === 'backup') target.backup = true;
    else if (token === 'down') target.down = true;
    else if (token.startsWith('weight=')) target.weight = Number(token.slice('weight='.length));
    else if (token.startsWith('max_fails=')) target.maxFails = Number(token.slice('max_fails='.length));
    else if (token.startsWith('fail_timeout=')) target.failTimeout = token.slice('fail_timeout='.length);
  }
  return target;
}

function certFingerprint(cert) {
  return `${cert.certFile || ''}|${cert.certKeyFile || ''}|${cert.chainFile || ''}`;
}

function buildDraft(mapped) {
  const draft = {
    schemaVersion: 1,
    kind: 'sdkwork.webserver.nginx-import.draft',
    profile: 'http-core-v1',
    main: {},
    http: {
      certificates: {},
      upstream: [],
      server: [],
      map: [],
    },
    notes: [
      'Review mapped directives before promoting into deployments/webserver/server.*.toml.',
      'proxySetHeader values with supported $vars execute on the Rust data plane after materialize.',
      '^~ locations materialize as pathType prefix-exclusive; ~ / ~* as regex / regex-ignore-case; rewrite last|break|redirect|permanent executes with bounded internal redirects.',
      'allow/deny, limit_req(+limit_req_zone), and auth_basic(+auth_basic_user_file) map into typed TOML; include stays preserve-only (manual resolution).',
    ],
  };

  const serversById = new Map();
  const upstreamsById = new Map();
  const certByFingerprint = new Map();
  let certCounter = 0;
  let currentLocation = null;

  const ensureServer = (item) => {
    const id = item.serverBlockId;
    if (id == null) {
      throw new Error(`directive ${item.name} requires a server {} context`);
    }
    if (!serversById.has(id)) {
      const server = {
        listen: [],
        serverName: [],
        location: [],
        _blockId: id,
      };
      serversById.set(id, server);
      draft.http.server.push(server);
    }
    return serversById.get(id);
  };

  const ensureUpstream = (item) => {
    const id = item.upstreamBlockId;
    if (id == null) return null;
    if (!upstreamsById.has(id)) {
      const name = (item.upstreamHeader || 'upstream').split(/\s+/u)[1] || `upstream-${id}`;
      const upstream = { name, target: [], _blockId: id };
      upstreamsById.set(id, upstream);
      draft.http.upstream.push(upstream);
    }
    return upstreamsById.get(id);
  };

  const ensureLocation = (server, item) => {
    const header = item.locationHeader || 'location /';
    const match = header.replace(/^location\s+/u, '').trim() || '/';
    let location = server.location.find((loc) => loc.match === match);
    if (!location) {
      location = { match };
      server.location.push(location);
    }
    currentLocation = location;
    return location;
  };

  const ensureCertId = (partial) => {
    const fp = certFingerprint(partial);
    if (certByFingerprint.has(fp) && fp !== '||') {
      return certByFingerprint.get(fp);
    }
    // Prefer merging onto an existing incomplete cert with matching path.
    for (const [existingFp, id] of certByFingerprint) {
      const existing = draft.http.certificates[id];
      if (partial.certFile && existing.certFile === partial.certFile) {
        Object.assign(existing, partial);
        const nextFp = certFingerprint(existing);
        certByFingerprint.delete(existingFp);
        certByFingerprint.set(nextFp, id);
        return id;
      }
    }
    const id = Object.keys(draft.http.certificates).length === 0 && partial.certFile
      ? 'sdkwork'
      : `imported-cert-${++certCounter}`;
    draft.http.certificates[id] = { ...partial };
    certByFingerprint.set(certFingerprint(draft.http.certificates[id]), id);
    return id;
  };

  for (const item of mapped) {
    const args = item.statement.replace(/;$/u, '').split(/\s+/u).slice(1);

    if (item.name === 'user') draft.main.user = args.join(' ');
    else if (item.name === 'worker_processes') draft.main.workerProcesses = args[0];
    else if (item.name === 'pid') draft.main.pid = args[0];
    else if (item.name === 'error_log') draft.main.errorLog = args.join(' ');
    else if (item.name === 'worker_connections') {
      draft.main.events = draft.main.events || {};
      draft.main.events.workerConnections = Number(args[0]) || args[0];
    } else if (item.name === 'sendfile') draft.http.sendfile = args[0] !== 'off';
    else if (item.name === 'tcp_nopush') draft.http.tcpNopush = args[0] !== 'off';
    else if (item.name === 'tcp_nodelay') draft.http.tcpNodelay = args[0] !== 'off';
    else if (item.name === 'keepalive_timeout') draft.http.keepaliveTimeout = Number(args[0]) || args[0];
    else if (item.name === 'client_max_body_size') draft.http.clientMaxBodySize = args[0];
    else if (item.name === 'server_tokens') draft.http.serverTokens = args[0];
    else if (item.name === 'gzip') draft.http.gzip = args[0] !== 'off';
    else if (item.name === 'gzip_types') draft.http.gzipTypes = args;
    else if (item.name === 'limit_req_zone') {
      draft.http.limitReqZone = draft.http.limitReqZone || [];
      draft.http.limitReqZone.push(args.join(' '));
    } else if (item.name === 'map' || (item.name === 'default' && item.mapHeader)) {
      const header = item.mapHeader || item.statement.replace(/;$/u, '');
      const entry = header.startsWith('map ')
        ? `${header.replace(/^map\s+/u, '')} ${args.join(' ')}`.trim()
        : args.join(' ');
      if (entry && !draft.http.map.includes(entry)) draft.http.map.push(entry);
    } else if (item.name === 'least_conn' || item.name === 'ip_hash' || item.name === 'hash') {
      const upstream = ensureUpstream(item);
      if (upstream) {
        if (item.name === 'least_conn') upstream.loadBalancing = 'least-connections';
        else if (item.name === 'ip_hash') upstream.loadBalancing = 'ip-hash';
        else {
          upstream.loadBalancing = 'hash';
          upstream.hashKey = args.join(' ');
        }
      }
    } else if (item.name === 'keepalive' && item.upstreamBlockId != null) {
      const upstream = ensureUpstream(item);
      if (upstream) upstream.keepalive = Number(args[0]) || args[0];
    } else if (item.name === 'server' && item.upstreamBlockId != null) {
      const upstream = ensureUpstream(item);
      if (upstream) upstream.target.push(parseUpstreamTarget(args));
    } else if (item.name === 'listen') {
      ensureServer(item).listen.push(args.join(' '));
    } else if (item.name === 'server_name') {
      ensureServer(item).serverName.push(...args);
    } else if (item.name === 'http2') {
      ensureServer(item).http2 = args[0] !== 'off';
    } else if (
      item.name === 'ssl_certificate'
      || item.name === 'ssl_certificate_key'
      || item.name === 'ssl_trusted_certificate'
      || item.name === 'ssl_protocols'
      || item.name === 'ssl_prefer_server_ciphers'
      || item.name === 'ssl_session_cache'
      || item.name === 'ssl_stapling'
    ) {
      const server = ensureServer(item);
      server.tls = server.tls || {};
      if (item.name === 'ssl_certificate') {
        const id = ensureCertId({ certFile: args[0] });
        server.tls.cert = id;
      } else if (item.name === 'ssl_certificate_key') {
        if (server.tls.cert && draft.http.certificates[server.tls.cert]) {
          draft.http.certificates[server.tls.cert].certKeyFile = args[0];
        } else {
          server.tls.cert = ensureCertId({ certKeyFile: args[0] });
        }
      } else if (item.name === 'ssl_trusted_certificate') {
        if (server.tls.cert && draft.http.certificates[server.tls.cert]) {
          draft.http.certificates[server.tls.cert].chainFile = args[0];
        } else {
          server.tls.cert = ensureCertId({ chainFile: args[0] });
        }
      } else if (item.name === 'ssl_protocols') {
        server.tls.protocols = args;
      } else if (item.name === 'ssl_prefer_server_ciphers') {
        server.tls.preferServerCiphers = args[0] !== 'off';
      } else if (item.name === 'ssl_session_cache') {
        server.tls.sessionCache = args.join(' ');
      } else if (item.name === 'ssl_stapling') {
        server.tls.stapling = args[0] !== 'off';
        if (server.tls.cert && draft.http.certificates[server.tls.cert]) {
          draft.http.certificates[server.tls.cert].ocspStapling = server.tls.stapling;
        }
      }
    } else if (
      item.name === 'proxy_pass'
      || item.name === 'root'
      || item.name === 'alias'
      || item.name === 'return'
      || item.name === 'try_files'
      || item.name === 'index'
      || item.name === 'rewrite'
      || item.name === 'add_header'
      || item.name === 'allow'
      || item.name === 'deny'
      || item.name === 'limit_req'
      || item.name === 'auth_basic'
      || item.name === 'auth_basic_user_file'
    ) {
      const server = ensureServer(item);
      const location = ensureLocation(server, item);
      if (item.name === 'proxy_pass') location.proxyPass = args[0];
      if (item.name === 'root') location.root = args[0];
      if (item.name === 'alias') location.alias = args[0];
      if (item.name === 'try_files') location.tryFiles = args;
      if (item.name === 'index') location.index = args;
      if (item.name === 'rewrite') {
        location.rewrite = location.rewrite || [];
        location.rewrite.push(args.join(' '));
      }
      if (item.name === 'add_header') {
        location.addHeader = location.addHeader || [];
        location.addHeader.push(args.join(' '));
      }
      if (item.name === 'allow') {
        location.allow = location.allow || [];
        location.allow.push(args.join(' '));
      }
      if (item.name === 'deny') {
        location.deny = location.deny || [];
        location.deny.push(args.join(' '));
      }
      if (item.name === 'limit_req') {
        location.limitReq = location.limitReq || [];
        location.limitReq.push(args.join(' '));
      }
      if (item.name === 'auth_basic') {
        location.authBasic = args.join(' ').replace(/^"|"$/g, '');
      }
      if (item.name === 'auth_basic_user_file') {
        location.authBasicUserFile = args[0];
      }
      if (item.name === 'return') {
        location.returnStatus = Number(args[0]);
        if (args.length > 1) location.returnBody = args.slice(1).join(' ');
      }
    } else if (
      item.name === 'proxy_set_header'
      || item.name === 'proxy_http_version'
      || item.name === 'proxy_buffering'
      || item.name === 'proxy_read_timeout'
      || item.name === 'proxy_connect_timeout'
      || item.name === 'proxy_send_timeout'
    ) {
      const server = ensureServer(item);
      const location = ensureLocation(server, item);
      if (item.name === 'proxy_set_header') {
        location.proxySetHeader = location.proxySetHeader || [];
        location.proxySetHeader.push(args.join(' '));
      } else {
        const key = {
          proxy_http_version: 'proxyHttpVersion',
          proxy_buffering: 'proxyBuffering',
          proxy_read_timeout: 'proxyReadTimeout',
          proxy_connect_timeout: 'proxyConnectTimeout',
          proxy_send_timeout: 'proxySendTimeout',
        }[item.name];
        let value = args[0];
        if (item.name === 'proxy_buffering') value = args[0] !== 'off';
        location[key] = value;
      }
    }
  }

  // Drop internal bookkeeping fields from published draft.
  for (const server of draft.http.server) delete server._blockId;
  for (const upstream of draft.http.upstream) delete upstream._blockId;
  if (draft.http.map.length === 0) delete draft.http.map;

  return draft;
}

function tomlQuote(value) {
  return JSON.stringify(String(value));
}

function tomlInlineArray(values) {
  return `[${values.map(tomlQuote).join(', ')}]`;
}

function emitToml(draft, options = {}) {
  const lines = [];
  lines.push('# Generated by tools/import-nginx-conf.mjs -- review before promoting.');
  lines.push('# SDKWORK_WEBSERVER_SPEC.md layout v3 draft from nginx.conf import.');
  lines.push('specVersion = 1');
  lines.push('kind = "sdkwork.webserver.server"');
  lines.push(`id = ${tomlQuote(options.id || 'imported-webserver')}`);
  lines.push('description = "Draft imported from nginx.conf; not an activation snapshot"');
  lines.push('');
  lines.push('[nginx]');
  lines.push('enabled = true');
  lines.push('profile = "http-core-v1"');
  lines.push('unknownDirectivePolicy = "error"');
  lines.push('strict = true');
  lines.push('confFile = "nginx.conf"');
  lines.push('');

  const main = draft.main || {};
  lines.push('[main]');
  if (main.user) lines.push(`user = ${tomlQuote(main.user)}`);
  if (main.workerProcesses) lines.push(`workerProcesses = ${tomlQuote(main.workerProcesses)}`);
  if (main.pid) lines.push(`pid = ${tomlQuote(main.pid)}`);
  if (main.errorLog) lines.push(`errorLog = ${tomlQuote(main.errorLog)}`);
  if (main.events?.workerConnections != null) {
    lines.push('');
    lines.push('[main.events]');
    lines.push(`workerConnections = ${Number(main.events.workerConnections)}`);
  }

  const http = draft.http || {};
  lines.push('');
  lines.push('[http]');
  if (http.sendfile != null) lines.push(`sendfile = ${http.sendfile}`);
  if (http.tcpNopush != null) lines.push(`tcpNopush = ${http.tcpNopush}`);
  if (http.tcpNodelay != null) lines.push(`tcpNodelay = ${http.tcpNodelay}`);
  if (http.keepaliveTimeout != null) lines.push(`keepaliveTimeout = ${Number(http.keepaliveTimeout)}`);
  if (http.clientMaxBodySize) lines.push(`clientMaxBodySize = ${tomlQuote(http.clientMaxBodySize)}`);
  if (http.serverTokens) lines.push(`serverTokens = ${tomlQuote(http.serverTokens)}`);
  if (http.gzip != null) lines.push(`gzip = ${http.gzip}`);
  if (http.gzipTypes?.length) lines.push(`gzipTypes = ${tomlInlineArray(http.gzipTypes)}`);
  if (http.map?.length) lines.push(`map = ${tomlInlineArray(http.map)}`);
  if (http.limitReqZone?.length) lines.push(`limitReqZone = ${tomlInlineArray(http.limitReqZone)}`);

  for (const [certId, cert] of Object.entries(http.certificates || {})) {
    lines.push('');
    lines.push(`[http.certificates.${certId}]`);
    if (cert.certFile) lines.push(`certFile = ${tomlQuote(cert.certFile)}`);
    if (cert.certKeyFile) lines.push(`certKeyFile = ${tomlQuote(cert.certKeyFile)}`);
    if (cert.chainFile) lines.push(`chainFile = ${tomlQuote(cert.chainFile)}`);
    if (cert.ocspStapling != null) lines.push(`ocspStapling = ${cert.ocspStapling}`);
  }

  for (const upstream of http.upstream || []) {
    lines.push('');
    lines.push('[[http.upstream]]');
    lines.push(`name = ${tomlQuote(upstream.name)}`);
    if (upstream.loadBalancing) lines.push(`loadBalancing = ${tomlQuote(upstream.loadBalancing)}`);
    if (upstream.hashKey) lines.push(`hashKey = ${tomlQuote(upstream.hashKey)}`);
    if (upstream.keepalive != null) lines.push(`keepalive = ${Number(upstream.keepalive)}`);
    for (const target of upstream.target || []) {
      lines.push('');
      lines.push('[[http.upstream.target]]');
      lines.push(`address = ${tomlQuote(target.address)}`);
      if (target.weight != null) lines.push(`weight = ${Number(target.weight)}`);
      if (target.maxFails != null) lines.push(`maxFails = ${Number(target.maxFails)}`);
      if (target.failTimeout) lines.push(`failTimeout = ${tomlQuote(target.failTimeout)}`);
      if (target.backup) lines.push('backup = true');
      if (target.down) lines.push('down = true');
    }
  }

  for (const server of http.server || []) {
    lines.push('');
    lines.push('[[http.server]]');
    if (server.listen?.length) lines.push(`listen = ${tomlInlineArray(server.listen)}`);
    if (server.serverName?.length) lines.push(`serverName = ${tomlInlineArray(server.serverName)}`);
    if (server.http2 != null) lines.push(`http2 = ${server.http2}`);
    if (server.tls) {
      lines.push('');
      lines.push('[http.server.tls]');
      if (server.tls.cert) lines.push(`cert = ${tomlQuote(server.tls.cert)}`);
      if (server.tls.protocols?.length) lines.push(`protocols = ${tomlInlineArray(server.tls.protocols)}`);
      if (server.tls.preferServerCiphers != null) {
        lines.push(`preferServerCiphers = ${server.tls.preferServerCiphers}`);
      }
      if (server.tls.sessionCache) lines.push(`sessionCache = ${tomlQuote(server.tls.sessionCache)}`);
      if (server.tls.stapling != null) lines.push(`stapling = ${server.tls.stapling}`);
    }
    for (const location of server.location || []) {
      lines.push('');
      lines.push('[[http.server.location]]');
      lines.push(`match = ${tomlQuote(location.match)}`);
      if (location.proxyPass) lines.push(`proxyPass = ${tomlQuote(location.proxyPass)}`);
      if (location.root) lines.push(`root = ${tomlQuote(location.root)}`);
      if (location.alias) lines.push(`alias = ${tomlQuote(location.alias)}`);
      if (location.tryFiles?.length) lines.push(`tryFiles = ${tomlInlineArray(location.tryFiles)}`);
      if (location.index?.length) lines.push(`index = ${tomlInlineArray(location.index)}`);
      if (location.returnStatus != null) lines.push(`returnStatus = ${Number(location.returnStatus)}`);
      if (location.returnBody) lines.push(`returnBody = ${tomlQuote(location.returnBody)}`);
      if (location.proxySetHeader?.length) {
        lines.push(`proxySetHeader = ${tomlInlineArray(location.proxySetHeader)}`);
      }
      if (location.proxyHttpVersion) lines.push(`proxyHttpVersion = ${tomlQuote(location.proxyHttpVersion)}`);
      if (location.proxyBuffering != null) lines.push(`proxyBuffering = ${location.proxyBuffering}`);
      if (location.proxyReadTimeout) lines.push(`proxyReadTimeout = ${tomlQuote(location.proxyReadTimeout)}`);
      if (location.proxyConnectTimeout) {
        lines.push(`proxyConnectTimeout = ${tomlQuote(location.proxyConnectTimeout)}`);
      }
      if (location.proxySendTimeout) lines.push(`proxySendTimeout = ${tomlQuote(location.proxySendTimeout)}`);
      if (location.rewrite?.length) lines.push(`rewrite = ${tomlInlineArray(location.rewrite)}`);
      if (location.addHeader?.length) lines.push(`addHeader = ${tomlInlineArray(location.addHeader)}`);
      if (location.allow?.length) lines.push(`allow = ${tomlInlineArray(location.allow)}`);
      if (location.deny?.length) lines.push(`deny = ${tomlInlineArray(location.deny)}`);
      if (location.limitReq?.length) lines.push(`limitReq = ${tomlInlineArray(location.limitReq)}`);
      if (location.authBasic) lines.push(`authBasic = ${tomlQuote(location.authBasic)}`);
      if (location.authBasicUserFile) {
        lines.push(`authBasicUserFile = ${tomlQuote(location.authBasicUserFile)}`);
      }
    }
  }

  lines.push('');
  return `${lines.join('\n')}\n`;
}

function readNginxEnabled(moduleRoot) {
  const commonPath = path.join(moduleRoot, 'deployments', 'webserver', 'server.common.toml');
  if (!fs.existsSync(commonPath)) {
    return { enabled: false, reason: 'missing deployments/webserver/server.common.toml' };
  }
  const doc = parseTomlSubset(fs.readFileSync(commonPath, 'utf8'));
  const retired = retiredNginxActivationBlock(doc);
  if (retired.blocked) {
    return { enabled: false, reason: retired.reason };
  }
  const enabled = doc.nginx?.enabled !== false;
  return { enabled, reason: enabled ? 'nginx.enabled=true' : 'nginx.enabled=false' };
}

/**
 * Parse emitted review TOML and run the typed server.toml validator.
 * Sidecar W16 layout checks are out of scope for a draft file.
 */
function validateEmittedToml(tomlText) {
  const doc = parseTomlSubset(tomlText, 'import.server.toml');
  return validateWebserverToml(doc);
}

function main() {
  const args = parseArgs(process.argv);
  if (args.help || !args.conf) {
    process.stdout.write(
      'Usage: node tools/import-nginx-conf.mjs --conf <nginx.conf> [--module-root <path>] [--write-draft <out.json>] [--write-toml <out.toml>] [--validate]\n',
    );
    process.exit(args.help ? 0 : 2);
  }
  const confPath = path.resolve(args.conf);
  if (!fs.existsSync(confPath)) {
    throw new Error(`conf not found: ${confPath}`);
  }
  const nginx = readNginxEnabled(args.moduleRoot);
  const directives = scanDirectives(fs.readFileSync(confPath, 'utf8'));
  const classified = classify(directives);
  const draft = buildDraft(classified.mapped);
  if (args.writeDraft) {
    fs.mkdirSync(path.dirname(args.writeDraft), { recursive: true });
    fs.writeFileSync(args.writeDraft, `${JSON.stringify(draft, null, 2)}\n`);
  }
  let tomlFile = null;
  let tomlText = null;
  if (args.writeToml || args.validate) {
    tomlText = emitToml(draft);
  }
  if (args.writeToml) {
    tomlFile = args.writeToml;
    fs.mkdirSync(path.dirname(tomlFile), { recursive: true });
    fs.writeFileSync(tomlFile, tomlText);
  }
  let validation = null;
  if (args.validate) {
    validation = validateEmittedToml(tomlText);
  }
  const report = {
    schemaVersion: 1,
    kind: 'sdkwork.webserver.nginx-import.classify',
    confFile: confPath,
    moduleRoot: args.moduleRoot,
    nginx: nginx,
    draftFile: args.writeDraft,
    tomlFile,
    validation,
    counts: {
      total: directives.length,
      mapped: classified.mapped.length,
      unsupported: classified.unsupported.length,
      blocked: classified.blocked.length,
      preserveOnly: classified.preserveOnly.length,
      draftServers: draft.http.server.length,
      draftLocations: draft.http.server.reduce((n, s) => n + (s.location?.length || 0), 0),
      draftUpstreams: draft.http.upstream.length,
      draftCertificates: Object.keys(draft.http.certificates || {}).length,
    },
    unsupported: classified.unsupported,
    blocked: classified.blocked,
    preserveOnly: classified.preserveOnly,
    activationAllowed: nginx.enabled
      && classified.blocked.length === 0
      && classified.unsupported.length === 0
      && (!validation || validation.errors.length === 0),
    nextSteps: nginx.enabled
      ? [
          args.writeToml
            ? `Review draft TOML at ${args.writeToml}; promote into deployments/webserver/ only after validation.`
            : 'Re-run with --write-toml <path> to emit a reviewable server.toml draft.',
          'Re-run with --validate to check the emitted draft against SDKWORK_WEBSERVER_SPEC.md.',
          'Run pnpm check:webserver-toml after promoting typed TOML into the layout.',
        ]
      : [
          'Set [nginx].enabled = true before import activation.',
          'Or keep nginx.enabled=false and migrate by hand into typed TOML only.',
        ],
  };
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  if (!nginx.enabled) process.exitCode = 3;
  else if (!report.activationAllowed) process.exitCode = 4;
}

export {
  buildDraft,
  classify,
  emitToml,
  readNginxEnabled,
  scanDirectives,
  validateEmittedToml,
};

const isDirectRun = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isDirectRun) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}
