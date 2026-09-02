import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { parse as parseYaml } from 'yaml';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const surfaces = [
  {
    name: 'app-api',
    source: 'apis/app-api/web/openapi.yaml',
    authority: 'apis/app-api/web/sdkwork-webserver-app-api.openapi.json',
    manifest: 'sdks/_route-manifests/app-api/sdkwork-routes-webserver-app-api.route-manifest.json',
    typescriptApi: 'sdks/sdkwork-webserver-app-sdk/sdkwork-webserver-app-sdk-typescript/generated/server-openapi/src/api',
    // 15 marked operations after listener_certificate_bindings and health_checks
    // idempotency markings were added to apis/app-api/web/openapi.yaml and materialized.
    expectedIdempotentOperations: 15,
  },
  {
    name: 'backend-api',
    source: 'apis/backend-api/web/openapi.yaml',
    authority: 'apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json',
    manifest: 'sdks/_route-manifests/backend-api/sdkwork-routes-webserver-backend-api.route-manifest.json',
    typescriptApi: 'sdks/sdkwork-webserver-backend-sdk/sdkwork-webserver-backend-sdk-typescript/generated/server-openapi/src/api',
    expectedIdempotentOperations: 32,
  },
  {
    name: 'internal-api',
    source: 'apis/internal-api/web/sdkwork-webserver-internal-api.openapi.yaml',
    authority: 'apis/internal-api/web/sdkwork-webserver-internal-api.openapi.json',
    manifest: 'sdks/_route-manifests/internal-api/sdkwork-routes-webserver-internal-api.route-manifest.json',
    typescriptApi: 'sdks/sdkwork-webserver-internal-sdk/sdkwork-webserver-internal-sdk-typescript/generated/server-openapi/src/api',
    expectedIdempotentOperations: 2,
  },
];

function read(relativePath) {
  return fs.readFileSync(path.join(ROOT, relativePath), 'utf8');
}

function operationEntries(document) {
  const entries = [];
  for (const [routePath, pathItem] of Object.entries(document.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) continue;
      entries.push({ method: method.toUpperCase(), operation, pathItem, routePath });
    }
  }
  return entries;
}

function resolveParameter(document, parameter) {
  const match = parameter?.$ref?.match(/^#\/components\/parameters\/([^/]+)$/u);
  if (!match) return parameter;
  return document.components?.parameters?.[match[1]];
}

function assertIdempotencyHeader(document, entry, label) {
  const parameters = [
    ...(Array.isArray(entry.pathItem.parameters) ? entry.pathItem.parameters : []),
    ...(Array.isArray(entry.operation.parameters) ? entry.operation.parameters : []),
  ].map((parameter) => resolveParameter(document, parameter));
  const header = parameters.find((parameter) => parameter?.in === 'header' && parameter.name === 'Idempotency-Key');
  assert.ok(header, `${label} is missing Idempotency-Key`);
  assert.equal(header.required, true, `${label} Idempotency-Key must be required`);
  assert.deepEqual(
    { type: header.schema?.type, minLength: header.schema?.minLength, maxLength: header.schema?.maxLength },
    { type: 'string', minLength: 1, maxLength: 128 },
    `${label} Idempotency-Key must remain bounded`,
  );
}

function readTypeScriptApi(relativeDirectory) {
  const directory = path.join(ROOT, relativeDirectory);
  return fs.readdirSync(directory)
    .filter((name) => name.endsWith('.ts'))
    .map((name) => fs.readFileSync(path.join(directory, name), 'utf8'))
    .join('\n');
}

for (const surface of surfaces) {
  test(`${surface.name} preserves idempotency across API, route, and SDK contracts`, () => {
    const source = parseYaml(read(surface.source));
    const authority = JSON.parse(read(surface.authority));
    const manifest = JSON.parse(read(surface.manifest));
    const sourceEntries = operationEntries(source);
    const authorityEntries = operationEntries(authority);
    const sourceByRoute = new Map(sourceEntries.map((entry) => [`${entry.method} ${entry.routePath}`, entry]));
    const authorityByRoute = new Map(authorityEntries.map((entry) => [`${entry.method} ${entry.routePath}`, entry]));
    const manifestByRoute = new Map(manifest.routes.map((route) => [`${route.method} ${route.path}`, route]));
    const marked = sourceEntries.filter((entry) => entry.operation['x-sdkwork-idempotent'] === true);

    assert.equal(marked.length, surface.expectedIdempotentOperations);
    for (const entry of sourceEntries) {
      const key = `${entry.method} ${entry.routePath}`;
      const sourceMarked = entry.operation['x-sdkwork-idempotent'] === true;
      const authorityEntry = authorityByRoute.get(key);
      const route = manifestByRoute.get(key);
      assert.ok(authorityEntry, `${surface.name} authority is missing ${key}`);
      assert.ok(route, `${surface.name} route manifest is missing ${key}`);
      assert.equal(authorityEntry.operation['x-sdkwork-idempotent'] === true, sourceMarked, `${key} authority marker drift`);
      assert.equal(route.idempotent, sourceMarked, `${key} route idempotency drift`);
      if (sourceMarked) {
        assertIdempotencyHeader(source, sourceByRoute.get(key), `${surface.name} source ${key}`);
        assertIdempotencyHeader(authority, authorityEntry, `${surface.name} authority ${key}`);
      }
    }

    const generated = readTypeScriptApi(surface.typescriptApi);
    assert.equal((generated.match(/idempotencyKey: string;/gu) ?? []).length, marked.length);
    assert.equal((generated.match(/'Idempotency-Key': \{ value: params\.idempotencyKey/gu) ?? []).length, marked.length);
    assert.doesNotMatch(generated, /idempotencyKey\?: string;/u);
  });
}

test('deployment idempotency is Header-owned and consumers do not assemble it manually', () => {
  const appAuthority = JSON.parse(read('apis/app-api/web/sdkwork-webserver-app-api.openapi.json'));
  const backendAuthority = JSON.parse(read('apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json'));
  assert.equal(appAuthority.components.schemas.CreateDeploymentRequest.properties.idempotencyKey, undefined);
  assert.equal(backendAuthority.components.schemas.CreateApplicationDeploymentRequest.properties.idempotencyKey, undefined);

  const consumerFiles = [
    'apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-core/src/index.tsx',
    'apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-core/src/index.tsx',
    'apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-applications/src/data-source.ts',
  ];
  for (const file of consumerFiles) {
    assert.doesNotMatch(read(file), /Idempotency-Key/u, `${file} must use generated SDK params`);
  }
});
