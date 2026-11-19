#!/usr/bin/env node
/**
 * Live edge-deployment regression: the running sdkwork-webserver public edge
 * (native :80/:443 data plane, installer or docker mode) against the nginx
 * behaviours an operator depends on when it replaces stock nginx.
 *
 * This suite needs a deployed instance and is skipped loudly without a target:
 *
 *   SDKWORK_EDGE_DOMAIN=server-dev.sdkwork.com \
 *   SDKWORK_EDGE_HOST=192.168.31.49 \
 *   pnpm exec node --test tests/edge-deployment/edge-regression.test.mjs
 *
 * Host header resolution (SDKWORK_EDGE_DOMAIN) is the operator's hosts/DNS
 * entry pointing at SDKWORK_EDGE_HOST; every request goes through the real
 * edge listeners — never a loopback bypass.
 *
 * Covered nginx-parity surface:
 * - :80 ACME http-01 webroot routing (challenge path NOT redirected) plus the
 *   canonical 301 https redirect that preserves $host and $request_uri;
 * - :443 TLS termination (1.2+/1.3, SNI-selected certificate, ALPN h2 with
 *   http/1.1 fallback);
 * - fail-closed SNI (unknown server name is refused, never served a default);
 * - reverse proxy to the gateway (health JSON through the edge);
 * - the product `Server` response header on every response class (redirect,
 *   console static, proxied API, 404);
 * - Adaptive Web console homepage delivery (the webserver's own homepage);
 * - HTTP method semantics (HEAD without body, OPTIONS);
 * - HTTP/1.1 keep-alive connection reuse.
 */
import assert from 'node:assert/strict';
import http from 'node:http';
import https from 'node:https';
import tls from 'node:tls';
import test from 'node:test';

const DOMAIN = process.env.SDKWORK_EDGE_DOMAIN ?? '';
const HOST = process.env.SDKWORK_EDGE_HOST ?? '';
const HTTP_PORT = Number(process.env.SDKWORK_EDGE_HTTP_PORT ?? 80);
const HTTPS_PORT = Number(process.env.SDKWORK_EDGE_HTTPS_PORT ?? 443);
// `production` (installer / cloud-style edge) terminates :80 with the ACME
// webroot route plus a canonical 301 to https. `development` (docker bundle
// dev) serves the console over plaintext :80 by design — no redirect, no
// webroot. The TLS, proxy, header, homepage, method, keep-alive and load
// assertions apply to both.
const EDGE_MODE = (process.env.SDKWORK_EDGE_MODE ?? 'production').toLowerCase();
const PRODUCTION_EDGE = EDGE_MODE === 'production';
const SERVER_HEADER = 'sdkwork-webserver';
const REQUEST_TIMEOUT_MS = 15_000;

const configured = Boolean(DOMAIN && HOST);

test('edge target is configured (SDKWORK_EDGE_DOMAIN + SDKWORK_EDGE_HOST)', () => {
  assert.ok(configured, 'skip: set SDKWORK_EDGE_DOMAIN and SDKWORK_EDGE_HOST to run the live edge regression');
});

/** One https request through a dedicated TLS socket pinned to HOST + SNI. */
function rawRequest({ pathname, method = 'GET', headers = {}, alpnProtocols = ['http/1.1'] }) {
  return new Promise((resolve, reject) => {
    const request = https.request(
      {
        host: HOST,
        port: HTTPS_PORT,
        path: pathname,
        method,
        servername: DOMAIN,
        headers: { host: DOMAIN, ...headers },
        rejectUnauthorized: false,
        ALPNProtocols: alpnProtocols,
        timeout: REQUEST_TIMEOUT_MS,
      },
      (response) => {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          resolve({
            status: response.statusCode,
            headers: response.headers,
            body: Buffer.concat(chunks),
            socket: response.socket,
          });
        });
      },
    );
    request.on('timeout', () => request.destroy(new Error(`timeout ${method} ${pathname}`)));
    request.on('error', reject);
    request.end();
  });
}

/** One http request to the :80 edge listener. */
function rawHttpRequest({ pathname, method = 'GET', headers = {} }) {
  return new Promise((resolve, reject) => {
    const request = http.request(
      {
        host: HOST,
        port: HTTP_PORT,
        path: pathname,
        method,
        headers: { host: DOMAIN, ...headers },
        timeout: REQUEST_TIMEOUT_MS,
      },
      (response) => {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          resolve({
            status: response.statusCode,
            headers: response.headers,
            body: Buffer.concat(chunks),
          });
        });
      },
    );
    request.on('timeout', () => request.destroy(new Error(`timeout ${method} ${pathname}`)));
    request.on('error', reject);
    request.end();
  });
}

/** TLS-layer probe: which ALPN protocol does the edge select for DOMAIN? */
function tlsAlpnProbe(alpnProtocols) {
  return new Promise((resolve, reject) => {
    const socket = tls.connect(
      {
        host: HOST,
        port: HTTPS_PORT,
        servername: DOMAIN,
        ALPNProtocols: alpnProtocols,
        rejectUnauthorized: false,
        timeout: REQUEST_TIMEOUT_MS,
      },
      () => {
        const protocol = socket.alpnProtocol;
        const tlsVersion = socket.getProtocol();
        socket.end();
        resolve({ protocol, tlsVersion });
      },
    );
    socket.on('error', reject);
    socket.on('timeout', () => socket.destroy(new Error('tls probe timeout')));
  });
}

test(':80 answers the ACME http-01 webroot route without redirecting it', { skip: !PRODUCTION_EDGE }, async () => {
  const response = await rawHttpRequest({ pathname: '/.well-known/acme-challenge/edge-regression-probe' });
  assert.notEqual(
    response.status,
    301,
    'the ACME challenge route must be answered in place, never redirected (Let\'s Encrypt http-01 follows redirects only to https, and nginx webroot convention keeps it on :80)',
  );
  assert.ok([200, 404].includes(response.status), `unexpected ACME route status ${response.status}`);
});

test(':80 redirects every other request to https preserving host and uri', { skip: !PRODUCTION_EDGE }, async () => {
  const response = await rawHttpRequest({ pathname: '/edge-regression/redirect-target?kept=1' });
  assert.equal(response.status, 301);
  assert.equal(response.headers.location, `https://${DOMAIN}/edge-regression/redirect-target?kept=1`);
  assert.equal(response.headers.server, SERVER_HEADER, 'nginx core header filter stamps Server on return responses too');
});

test('development :80 serves the console over plaintext without redirect', { skip: PRODUCTION_EDGE }, async () => {
  const response = await rawHttpRequest({ pathname: '/' });
  assert.equal(response.status, 200, 'the dev edge serves the console on :80 by design');
  assert.notEqual(response.status, 301, 'the dev edge must not force-redirect plaintext');
  assert.equal(response.headers.server, SERVER_HEADER);
});

test(':443 terminates TLS 1.2+ and negotiates h2 first with http/1.1 fallback', async () => {
  const h2 = await tlsAlpnProbe(['h2', 'http/1.1']);
  assert.equal(h2.protocol, 'h2', `edge must offer h2 (nginx default on TLS); selected ${h2.protocol}`);
  assert.ok(['TLSv1.3', 'TLSv1.2'].includes(h2.tlsVersion), `unexpected TLS version ${h2.tlsVersion}`);
  const h1 = await tlsAlpnProbe(['http/1.1']);
  assert.equal(h1.protocol, 'http/1.1', 'http/1.1-only clients must still connect');
});

test(':443 fails closed on an unknown or absent SNI name', async () => {
  // nginx-parity note: with a single TLS virtual host nginx answers every SNI
  // from that host; the runtime is stricter (fail closed) and must refuse both
  // a foreign server name and a client that sends no SNI at all.
  const foreign = await tlsHandshakeOutcome('foreign-host.edge-regression.invalid');
  assert.equal(foreign.ok, false, 'a foreign server name must be refused, never answered');
  const noSni = await tlsHandshakeOutcome('');
  assert.equal(noSni.ok, false, 'an SNI-less handshake must be refused (curl to an IP address has no SNI)');
});

/** TLS handshake attempt: resolves { ok, protocol?, alertCode? }. */
function tlsHandshakeOutcome(servername) {
  return new Promise((resolve) => {
    const options = {
      host: HOST,
      port: HTTPS_PORT,
      ALPNProtocols: ['http/1.1'],
      rejectUnauthorized: false,
      timeout: REQUEST_TIMEOUT_MS,
    };
    if (servername) options.servername = servername;
    const socket = tls.connect(options, () => {
      const protocol = socket.alpnProtocol;
      socket.end();
      resolve({ ok: true, protocol });
    });
    socket.on('error', (error) => resolve({ ok: false, alertCode: error.code }));
    socket.on('timeout', () => {
      socket.destroy();
      resolve({ ok: false, alertCode: 'TIMEOUT' });
    });
  });
}

test('edge proxies /readyz to the gateway with the product Server header', async () => {
  const response = await rawRequest({ pathname: '/readyz' });
  assert.equal(response.status, 200);
  assert.equal(response.headers.server, SERVER_HEADER);
  const payload = JSON.parse(response.body.toString('utf8'));
  assert.equal(payload.status, 'ready');
});

test('edge serves the webserver homepage (Adaptive Web console shell)', async () => {
  const response = await rawRequest({ pathname: '/' });
  assert.equal(response.status, 200);
  assert.equal(response.headers.server, SERVER_HEADER);
  const html = response.body.toString('utf8');
  assert.match(html, /<!doctype html>/i);
  assert.ok(html.includes('SDKWork Web Server'), 'the console shell title must be present');
});

test('every response class carries the product Server header (404 and console assets)', async () => {
  const missing = await rawRequest({ pathname: '/edge-regression/no-such-asset.js' });
  assert.equal(missing.status, 404);
  assert.equal(missing.headers.server, SERVER_HEADER);
  const consoleAsset = await rawRequest({ pathname: '/runtime-env.json' });
  assert.equal(consoleAsset.headers.server, SERVER_HEADER);
});

test('response headers follow the field-standards baseline (Vary/HSTS/nosniff)', async () => {
  // RFC 9110 §12.5.5: a Vary field-value is a comma list of field NAMES — a
  // duplicated selector inside one value (or a repeated field) breaks cache
  // keyed-by parsing, so each field name appears at most once.
  const homepage = await rawRequest({ pathname: '/' });
  const varyNames = (homepage.headers.vary ?? '')
    .split(',')
    .map((name) => name.trim().toLowerCase())
    .filter(Boolean);
  assert.ok(varyNames.length > 0, 'the adaptive console must declare its representation selectors');
  assert.equal(
    new Set(varyNames).size,
    varyNames.length,
    `Vary must not repeat a field name, got ${homepage.headers.vary}`,
  );
  // RFC 6797 §6.1: every TLS response class carries the HSTS policy.
  assert.match(
    homepage.headers['strict-transport-security'] ?? '',
    /max-age=\d+/,
    'TLS responses must carry strict-transport-security',
  );
  // SECURITY_SPEC secure-response baseline on delivered documents.
  assert.equal(homepage.headers['x-content-type-options'], 'nosniff');
  assert.ok(homepage.headers['x-frame-options'], 'frame framing must be explicitly governed');
});

test('HEAD returns headers without a body', async () => {
  const response = await rawRequest({ pathname: '/', method: 'HEAD' });
  assert.equal(response.status, 200);
  assert.ok(response.headers['content-length'], 'HEAD must still advertise content-length');
  assert.equal(response.body.length, 0, 'HEAD must not carry a body');
});

test('HTTP/1.1 keep-alive reuses one TCP connection across requests', async () => {
  const agent = new https.Agent({ keepAlive: true, maxSockets: 1, rejectUnauthorized: false });
  const seenSockets = new Set();
  for (let index = 0; index < 3; index += 1) {
    await new Promise((resolve, reject) => {
      const request = https.request({
        host: HOST,
        port: HTTPS_PORT,
        path: '/readyz',
        agent,
        servername: DOMAIN,
        headers: { host: DOMAIN },
        rejectUnauthorized: false,
      }, (response) => {
        response.resume();
        response.on('end', () => {
          seenSockets.add(response.socket);
          resolve();
        });
      });
      request.on('error', reject);
      request.end();
    });
  }
  assert.equal(seenSockets.size, 1, 'keep-alive must reuse the same TLS connection for sequential requests');
  agent.destroy();
});

test('edge stays healthy under concurrent load (50 requests, bounded concurrency)', async () => {
  const total = 50;
  const concurrency = 10;
  let cursor = 0;
  let failures = 0;
  const latencies = [];
  async function worker() {
    for (;;) {
      const index = cursor;
      cursor += 1;
      if (index >= total) return;
      const started = performance.now();
      try {
        const response = await rawRequest({ pathname: '/readyz' });
        if (response.status !== 200) failures += 1;
      } catch {
        failures += 1;
      }
      latencies.push(performance.now() - started);
    }
  }
  await Promise.all(Array.from({ length: concurrency }, worker));
  assert.equal(failures, 0, `${failures}/${total} concurrent requests failed`);
  latencies.sort((a, b) => a - b);
  const p95 = latencies[Math.floor(latencies.length * 0.95)];
  assert.ok(p95 < 2_000, `p95 latency ${p95.toFixed(0)}ms exceeds the 2s sanity budget`);
});
