#!/usr/bin/env node
/**
 * nginx config-acceptance probe for `http2_chunk_size`.
 *
 * The HTTP/1 differential harness cannot observe an HTTP/2 DATA-frame
 * boundary, so the wire evidence for `http2_chunk_size` lives in the gateway
 * frame-capture test (`tests/http2_tuning.rs`). This probe covers the other
 * half of the contract: a real nginx must accept the directive in every
 * supported context (http, server, location) and reject a zero size, exactly
 * as the Rust materializer does.
 *
 * `--with-http_v2_module` is required for `http2_chunk_size` to exist at all
 * (it is declared by `ngx_http_v2_module`). A package binary built without
 * that module answers `unknown directive "http2_chunk_size"`, so the probe
 * skips loudly instead of pretending the directive was accepted; the
 * environment's local oracles may be such builds.
 *
 * Exit codes: 0 accepted, 1 rejected/misbehaving, 3 no v2-enabled nginx.
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const work = path.join(root, 'tmp', 'nginx-http2-chunk-acceptance');

const candidates = [
  process.env.NGINX_BIN,
  '/tmp/nginx-v2-src/objs/nginx',
  '/tmp/nginx-deb/root/usr/sbin/nginx',
  'nginx',
].filter((candidate) => typeof candidate === 'string' && candidate.length > 0);

function hasHttp2Module(binary) {
  const probe = spawnSync(binary, ['-V'], { encoding: 'utf8' });
  if (probe.error || probe.status !== 0) return false;
  return `${probe.stdout ?? ''}${probe.stderr ?? ''}`.includes('--with-http_v2_module');
}

function findOracle() {
  for (const candidate of candidates) {
    if (candidate !== 'nginx' && !fs.existsSync(candidate)) continue;
    if (hasHttp2Module(candidate)) return candidate;
  }
  return null;
}

function renderConfig(extraHttp) {
  return `daemon off;
master_process off;
error_log ${work}/error.log notice;
pid ${work}/nginx.pid;

events {}

http {
    access_log off;
    client_body_temp_path ${work}/tmp/client_body;
    proxy_temp_path ${work}/tmp/proxy;
    fastcgi_temp_path ${work}/tmp/fastcgi;
    uwsgi_temp_path ${work}/tmp/uwsgi;
    scgi_temp_path ${work}/tmp/scgi;
    http2_chunk_size 4k;
${extraHttp}
    server {
        listen 127.0.0.1:20999;
        server_name chunk.example.test;
        http2_chunk_size 8k;
        location = /chunk {
            http2_chunk_size 1k;
            return 200 "ok";
        }
    }
}
`;
}

function testConfig(binary, text) {
  fs.mkdirSync(work, { recursive: true });
  // nginx opens its compile-time default error/access log under
  // `<prefix>/logs/` before the rendered config takes effect.
  fs.mkdirSync(path.join(work, 'logs'), { recursive: true });
  for (const directory of ['client_body', 'proxy', 'fastcgi', 'uwsgi', 'scgi']) {
    fs.mkdirSync(path.join(work, 'tmp', directory), { recursive: true });
  }
  const configPath = path.join(work, 'nginx.conf');
  fs.writeFileSync(configPath, text);
  const result = spawnSync(binary, ['-t', '-c', configPath, '-p', `${work}/`], {
    encoding: 'utf8',
    env: { ...process.env, TZ: 'UTC' },
  });
  return {
    ok: result.status === 0,
    status: result.status,
    output: `${result.stdout ?? ''}${result.stderr ?? ''}`,
  };
}

const binary = findOracle();
if (!binary) {
  process.stdout.write(
    '[nginx-http2-chunk-acceptance] SKIP: no nginx built with --with-http_v2_module ' +
      '(http2_chunk_size is declared by ngx_http_v2_module); wire evidence is the ' +
      'gateway h2 frame-capture test\n',
  );
  process.exit(3);
}

const accepted = testConfig(binary, renderConfig(''));
if (!accepted.ok) {
  process.stdout.write(
    `[nginx-http2-chunk-acceptance] FAIL: ${binary} rejected http2_chunk_size at ` +
      `http/server/location:\n${accepted.output}\n`,
  );
  process.exit(1);
}
if (!/syntax is ok/.test(accepted.output) || !/test is successful/.test(accepted.output)) {
  process.stdout.write(
    `[nginx-http2-chunk-acceptance] FAIL: unexpected nginx -t output:\n${accepted.output}\n`,
  );
  process.exit(1);
}

const rejected = testConfig(
  binary,
  renderConfig('').replace('http2_chunk_size 4k;', 'http2_chunk_size 0;'),
);
if (rejected.ok || !/chunk size cannot be zero/.test(rejected.output)) {
  process.stdout.write(
    `[nginx-http2-chunk-acceptance] FAIL: nginx must reject http2_chunk_size 0 with a ` +
      `"chunk size cannot be zero" diagnostic; got exit ${rejected.status}:\n${rejected.output}\n`,
  );
  process.exit(1);
}

process.stdout.write(
  `[nginx-http2-chunk-acceptance] PASS: ${binary} accepts http2_chunk_size at ` +
    'http/server/location and rejects a zero size\n',
);
process.exit(0);
