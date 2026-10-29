#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { parseTomlSubset } from '../../sdkwork-specs/tools/webserver/toml.mjs';
import { buildDraft, classify, emitToml, loadDirectiveCatalog, readNginxEnabled, scanDirectives, validateEmittedToml } from './import-nginx-conf.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..');
const STANDALONE_CONF = path.join(REPO_ROOT, 'deployments', 'webserver', 'nginx.standalone.production.conf');

test('import classifies product standalone conf with multiple servers', () => {
  const source = fs.readFileSync(STANDALONE_CONF, 'utf8');
  const directives = scanDirectives(source, { sourcePath: STANDALONE_CONF });
  const classified = classify(directives);
  assert.equal(classified.blocked.length, 0);
  assert.equal(classified.unsupported.length, 0, JSON.stringify(classified.unsupported, null, 2));
  const draft = buildDraft(classified.mapped);
  assert.ok(draft.http.server.length >= 2, 'expected multiple virtual hosts');
  assert.ok(draft.http.upstream.some((u) => u.name === 'gateway'));
  const gateway = draft.http.upstream.find((u) => u.name === 'gateway');
  assert.equal(gateway.loadBalancing, 'least-connections');
  assert.ok(gateway.target.length >= 1);
  const api = draft.http.server
    .flatMap((s) => s.location.map((loc) => ({ server: s, loc })))
    .find(({ loc }) => loc.match === '/api/');
  assert.ok(api);
  assert.equal(api.loc.proxyPass, 'http://gateway');
  assert.ok(api.loc.proxySetHeader?.includes('Host $host'));
});

test('emitToml produces parseable layout v3 draft', () => {
  const source = fs.readFileSync(STANDALONE_CONF, 'utf8');
  const draft = buildDraft(classify(scanDirectives(source)).mapped);
  const toml = emitToml(draft, { id: 'import-test' });
  const doc = parseTomlSubset(toml);
  assert.equal(doc.kind, 'sdkwork.webserver.server');
  assert.equal(doc.id, 'import-test');
  assert.ok(Array.isArray(doc.http.server));
  assert.ok(doc.http.server.length >= 2);
  assert.ok(doc.http.server.some((s) => (s.serverName || []).includes('server.sdkwork.com')));
  assert.ok(doc.http.upstream.some((u) => u.name === 'gateway'));
});

test('write-toml path can be written for operator review', () => {
  const source = fs.readFileSync(STANDALONE_CONF, 'utf8');
  const draft = buildDraft(classify(scanDirectives(source)).mapped);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-nginx-import-'));
  const out = path.join(dir, 'server.import.toml');
  fs.writeFileSync(out, emitToml(draft));
  assert.ok(fs.statSync(out).size > 100);
});

test('readNginxEnabled rejects retired compatibility table', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-nginx-enabled-'));
  const webserver = path.join(dir, 'deployments', 'webserver');
  fs.mkdirSync(webserver, { recursive: true });
  fs.writeFileSync(
    path.join(webserver, 'server.common.toml'),
    'specVersion = 1\nkind = "sdkwork.webserver.server"\nid = "x"\n[compatibility]\nenabled = true\n',
  );
  const result = readNginxEnabled(dir);
  assert.equal(result.enabled, false);
  assert.match(result.reason, /\[compatibility\]/);
});

test('readNginxEnabled rejects retired nginxProfile key', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-nginx-profile-'));
  const webserver = path.join(dir, 'deployments', 'webserver');
  fs.mkdirSync(webserver, { recursive: true });
  fs.writeFileSync(
    path.join(webserver, 'server.common.toml'),
    'specVersion = 1\nkind = "sdkwork.webserver.server"\nid = "x"\n[nginx]\nnginxProfile = "http-core-v1"\n',
  );
  const result = readNginxEnabled(dir);
  assert.equal(result.enabled, false);
  assert.match(result.reason, /nginxProfile/);
});

test('maps allow/deny and limit_req into typed draft + emit', () => {
  const source = `
http {
  limit_req_zone $binary_remote_addr zone=one:10m rate=1r/s;
  upstream gateway {
    least_conn;
    server 127.0.0.1:3800;
  }
  server {
    listen 80;
    server_name import-acl.local;
    location /api/ {
      allow 10.0.0.0/8;
      deny all;
      limit_req zone=one burst=5 nodelay;
      proxy_pass http://gateway;
    }
  }
}
`;
  const classified = classify(scanDirectives(source));
  assert.equal(classified.unsupported.length, 0, JSON.stringify(classified.unsupported));
  const draft = buildDraft(classified.mapped);
  assert.deepEqual(draft.http.limitReqZone, ['$binary_remote_addr zone=one:10m rate=1r/s']);
  const location = draft.http.server[0].location[0];
  assert.deepEqual(location.allow, ['10.0.0.0/8']);
  assert.deepEqual(location.deny, ['all']);
  assert.deepEqual(location.limitReq, ['zone=one burst=5 nodelay']);
  const toml = emitToml(draft, { id: 'import-acl' });
  assert.match(toml, /limitReqZone/);
  assert.match(toml, /allow = /);
  assert.match(toml, /deny = /);
  assert.match(toml, /limitReq = /);
});

test('maps auth_basic into typed draft + emit', () => {
  const source = `
http {
  server {
    listen 80;
    server_name import-auth.local;
    location /secure/ {
      auth_basic "Restricted Area";
      auth_basic_user_file /etc/nginx/.htpasswd;
      return 200 ok;
    }
  }
}
`;
  const classified = classify(scanDirectives(source));
  assert.equal(classified.unsupported.length, 0, JSON.stringify(classified.unsupported));
  const location = buildDraft(classified.mapped).http.server[0].location[0];
  assert.equal(location.authBasic, 'Restricted Area');
  assert.equal(location.authBasicUserFile, '/etc/nginx/.htpasswd');
  const toml = emitToml(buildDraft(classified.mapped), { id: 'import-auth' });
  assert.match(toml, /authBasic = /);
  assert.match(toml, /authBasicUserFile = /);
});

test('maps hash consistent into typed draft + emit', () => {
  const source = `
http {
  upstream backend {
    hash $request_uri consistent;
    server 127.0.0.1:3800;
    server 127.0.0.1:3801;
  }
  server {
    listen 80;
    server_name import-hash.local;
    location / {
      proxy_pass http://backend;
    }
  }
}
`;
  const classified = classify(scanDirectives(source));
  assert.equal(classified.unsupported.length, 0, JSON.stringify(classified.unsupported));
  const upstream = buildDraft(classified.mapped).http.upstream[0];
  assert.equal(upstream.loadBalancing, 'hash');
  assert.equal(upstream.hashKey, '$request_uri consistent');
  const toml = emitToml(buildDraft(classified.mapped), { id: 'import-hash' });
  assert.match(toml, /loadBalancing = "hash"/);
  assert.match(toml, /hashKey = /);
});

test('emitToml from product standalone conf validates as typed server.toml', () => {
  const source = fs.readFileSync(STANDALONE_CONF, 'utf8');
  const draft = buildDraft(classify(scanDirectives(source)).mapped);
  const result = validateEmittedToml(emitToml(draft, { id: 'webserver' }));
  assert.equal(result.errors.length, 0, JSON.stringify(result.errors, null, 2));
});

test('data-block bodies are data, not directives (types/charset_map/map)', () => {
  const source = `
http {
  types {
    text/html  html htm;
    text/css   css;
  }
  charset_map utf-8 iso-8859-1 {
    80 80;
  }
  map $http_user_agent $mobile {
    default 0;
    ~*iphone 1;
  }
  server {
    listen 80;
    server_name data-blocks.local;
    location / { return 200 ok; }
  }
}
`;
  const directives = scanDirectives(source);
  const names = directives.map((item) => item.name);
  for (const dataChild of ['text/html', 'text/css', '80', '~*iphone']) {
    assert.ok(!names.includes(dataChild), `${dataChild} must not be flattened as a directive`);
  }
  const classified = classify(directives);
  assert.equal(classified.unsupported.length, 0, JSON.stringify(classified.unsupported, null, 2));
  assert.ok(classified.accepted.some((item) => item.name === 'types'));
  assert.ok(classified.accepted.some((item) => item.name === 'charset_map'));
  // `map` executes on the runtime through typed `[[http.maps]]`, so it is
  // rendered as a typed declaration (never as the legacy string-array `map`
  // key, which stays accepted-but-unenforced).
  const mapItem = classified.mapped.find((item) => item.name === 'map');
  assert.ok(mapItem, 'map must be classified as a mapped executable directive');
  const draft = buildDraft(classified.mapped);
  assert.equal(draft.http.map, undefined, 'the legacy string-array map key must not be rendered');
  assert.equal(draft.http.maps.length, 1);
  assert.equal(draft.http.maps[0].source, '$http_user_agent');
  assert.equal(draft.http.maps[0].result, '$mobile');
  assert.equal(draft.http.maps[0].default, '0');
  assert.deepEqual(draft.http.maps[0].entries, [{ key: '~*iphone', value: '1' }]);
  const toml = emitToml(draft, { id: 'typed-map' });
  assert.match(toml, /\[\[http\.maps\]\]/u);
  assert.match(toml, /\[\[http\.maps\.entries\]\]/u);
});

test('executable and accepted catalogued directives are not reported unsupported', () => {
  const source = `
http {
  default_type application/octet-stream;
  server {
    listen 80;
    server_name catalog-drift.local;
    location / {
      expires 1d;
      etag off;
      if_modified_since exact;
      return 200 ok;
    }
  }
}
`;
  const classified = classify(scanDirectives(source));
  const unsupported = classified.unsupported.map((item) => item.name);
  assert.equal(unsupported.length, 0, JSON.stringify(classified.unsupported, null, 2));
  // `expires`/`etag`/`if_modified_since` execute in the Rust materializer; the
  // CLI reports them as accepted-not-rendered rather than unsupported.
  for (const name of ['expires', 'etag', 'if_modified_since']) {
    assert.ok(classified.accepted.some((item) => item.name === name),
      `${name} must be accepted (executes in the nginx materializer)`);
  }
  assert.ok(classified.accepted.some((item) => item.name === 'default_type'));
});

test('profile-excluded directives are blocked without becoming unsupported', () => {
  const source = `
http {
  server {
    listen 80;
    server_name excluded.local;
    location / { fastcgi_pass 127.0.0.1:9000; }
  }
}
`;
  const classified = classify(scanDirectives(source));
  assert.equal(classified.blocked.length, 1);
  assert.equal(classified.blocked[0].name, 'fastcgi_pass');
  assert.equal(classified.blocked[0].reason, 'notExecutableByRust');
  assert.match(classified.blocked[0].catalog, /fastcgi/);
  assert.equal(classified.unsupported.length, 0);
});

test('every CLI-rendered name is classified by the committed support catalog', () => {
  const { catalog } = loadDirectiveCatalog();
  const byName = new Map();
  for (const entry of catalog.directives) {
    const list = byName.get(entry.name) ?? [];
    list.push(entry);
    byName.set(entry.name, list);
  }
  const source = fs.readFileSync(STANDALONE_CONF, 'utf8');
  for (const item of scanDirectives(source)) {
    const entries = byName.get(item.name);
    assert.ok(entries && entries.length > 0, `${item.name} is not in the support catalog`);
  }
});

test('unenforced typed TOML keys are never rendered as enforced', () => {
  const source = `
http {
  map $http_user_agent $mobile { default 0; }
  server {
    listen 80;
    server_name unenforced.local;
    ssl_certificate cert.pem;
    ssl_certificate_key key.pem;
    ssl_prefer_server_ciphers on;
    ssl_session_cache shared:SSL:10m;
    ssl_stapling on;
    server_tokens off;
    location / {
      proxy_pass http://127.0.0.1:9001;
      proxy_buffering on;
    }
  }
}
`;
  const classified = classify(scanDirectives(source));
  for (const name of ['ssl_prefer_server_ciphers', 'ssl_session_cache', 'ssl_stapling', 'server_tokens', 'proxy_buffering']) {
    assert.ok(classified.accepted.some((item) => item.name === name),
      `${name} must be reported accepted, not rendered`);
    assert.ok(!classified.mapped.some((item) => item.name === name),
      `${name} must not be rendered into the draft`);
  }
  // `map` is executable through typed `[[http.maps]]`; it is covered by the
  // data-block test above and must render as a typed declaration.
  assert.ok(classified.mapped.some((item) => item.name === 'map'));
  const toml = emitToml(buildDraft(classified.mapped), { id: 'unenforced' });
  for (const key of ['preferServerCiphers', 'sessionCache', 'ocspStapling', 'stapling =', 'serverTokens', 'proxyBuffering', 'map =']) {
    assert.ok(!toml.includes(key), `draft must not emit unenforced typed key ${key}`);
  }
});
