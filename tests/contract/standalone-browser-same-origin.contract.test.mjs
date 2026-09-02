import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const PC_ROOT = path.join(REPO_ROOT, 'apps', 'sdkwork-webserver-pc');
const SDK_BASE_URL_KEYS = [
  'appApiBaseUrl',
  'backendApiBaseUrl',
  'driveAppApiBaseUrl',
  'appbaseAppApiBaseUrl',
];

function read(relativePath) {
  return readFileSync(path.join(REPO_ROOT, ...relativePath.split('/')), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

test('standalone topology declares one canonical browser origin in both lifecycle profiles', () => {
  const topology = readJson('specs/topology.spec.json');
  const development = topology.orchestration.profiles['standalone.development'];
  const production = topology.orchestration.profiles['standalone.production'];

  // Development: one adaptive-web delivery serves PC and H5 renderers behind
  // the single same-origin gateway ingress.
  assert.equal(development.browserDeliveries.length, 1);
  const devDelivery = development.browserDeliveries[0];
  assert.equal(devDelivery.id, 'webserver-adaptive-web');
  assert.equal(devDelivery.applicationRoot, 'apps/sdkwork-webserver-pc');
  assert.deepEqual(devDelivery.clientArchitectures, ['pc-web', 'h5']);
  assert.equal(devDelivery.originMode, 'same-origin');
  assert.equal(devDelivery.deliveryMode, 'dev-server-proxy');
  assert.equal(devDelivery.apiSurfaceId, 'application.public-ingress');
  assert.equal(devDelivery.clientProcessId, 'webserver-browser');
  assert.equal(devDelivery.preserveCanonicalPaths, true);

  // Production: the gateway statically hosts every browser bundle same-origin.
  assert.ok(production.browserDeliveries.length > 0);
  for (const delivery of production.browserDeliveries) {
    assert.equal(delivery.originMode, 'same-origin');
    assert.equal(delivery.deliveryMode, 'gateway-static');
    assert.equal(delivery.apiSurfaceId, 'application.public-ingress');
    assert.equal(delivery.hostProcessId, 'application.public-ingress');
    assert.match(delivery.buildOutput, /^apps\/sdkwork-webserver-(pc|h5)\/dist$/u);
  }
});

test('standalone browser runtime sources expose only canonical same-origin SDK roots', () => {
  const deployment = readJson(
    'apps/sdkwork-webserver-pc/etc/sdkwork.deployment.config.json',
  );
  for (const profileId of ['standalone.development', 'standalone.production']) {
    const source = deployment.profiles[profileId]?.source;
    assert.equal(typeof source, 'string', `${profileId} source`);
    const runtime = JSON.parse(
      readFileSync(path.resolve(PC_ROOT, 'etc', source), 'utf8'),
    );
    assert.equal(runtime.profileId, profileId);
    assert.equal(runtime.browserOriginMode, 'same-origin');
    for (const key of SDK_BASE_URL_KEYS) assert.equal(runtime[key], '/', `${profileId}.${key}`);
    assert.doesNotMatch(JSON.stringify(runtime), /:(?:3800|3900)\b/u);
  }
});

test('Vite proxies every canonical infrastructure path and keeps one React instance', () => {
  const browserTopology = read(
    'apps/sdkwork-webserver-pc/scripts/browser-topology.mjs',
  );
  for (const canonicalPath of [
    '/app/v3/api',
    '/backend/v3/api',
    '/openapi.json',
    '/healthz',
    '/readyz',
    '/livez',
    '/metrics',
  ]) {
    assert.match(browserTopology, new RegExp(`['\"]${canonicalPath.replaceAll('/', '\\/')}['\"]`, 'u'));
  }
  assert.match(browserTopology, /preserveCanonicalPaths !== true/u);
  assert.doesNotMatch(browserTopology, /rewrite\s*:/u);

  const vite = read('apps/sdkwork-webserver-pc/vite.config.ts');
  // react/react-dom must be deduped; the configuration may list additional
  // framework packages after them.
  assert.match(vite, /dedupe:\s*\["react",\s*"react-dom"[^\]]*\]/u);
  assert.match(vite, /createCanonicalApiProxyConfig/u);
});

test('standalone package and gateway resolve PC and dependency assets from one package root', () => {
  const productionEnv = read('etc/topology/standalone.production.env');
  for (const declaration of [
    'SDKWORK_APP_ROOT=/usr/lib/sdkwork/webserver',
    'SDKWORK_WEBSERVER_APP_ROOT=/usr/lib/sdkwork/webserver',
    'SDKWORK_WEBSERVER_SERVER_APP_ROOT=/usr/lib/sdkwork/webserver',
    'SDKWORK_IAM_APP_ROOT=/usr/share/sdkwork/webserver/iam',
    'SDKWORK_DRIVE_APP_ROOT=/usr/share/sdkwork/webserver/drive',
    'SDKWORK_WEBSERVER_PC_STATIC_ROOT=/usr/share/sdkwork/webserver/web/pc',
  ]) {
    assert.match(productionEnv, new RegExp(`^${declaration.replaceAll('/', '\\/').replace('.', '\\.')}$$`, 'mu'));
  }

  const release = read('scripts/webserver-release.mjs');
  assert.match(release, /build-browser-client\.mjs/u);
  assert.match(release, /share\/sdkwork\/webserver-pc/u);
  assert.match(release, /share\/sdkwork\/iam/u);
  assert.match(release, /share\/sdkwork\/drive/u);

  const gatewayMain = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/main.rs',
  );
  assert.match(gatewayMain, /configure_packaged_runtime_roots_from_env\(\)/u);
});
