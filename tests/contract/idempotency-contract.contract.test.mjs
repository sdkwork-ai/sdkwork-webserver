import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { parse as parseYaml } from 'yaml';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// The `app-api` surface was retired: the application / domain / certificate
// lifecycle is owned by sdkwork-deployments and served through its own
// `/app/v3/api` authority, so this repository only verifies the two surfaces it
// still owns.
const surfaces = [
  {
    name: 'backend-api',
    source: 'apis/backend-api/web/openapi.yaml',
    authority: 'apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json',
    manifest: 'sdks/_route-manifests/backend-api/sdkwork-routes-webserver-backend-api.route-manifest.json',
    typescriptApi: 'sdks/sdkwork-webserver-backend-sdk/sdkwork-webserver-backend-sdk-typescript/generated/server-openapi/src/api',
    // 33 marked operations after the webserver_configs update (PUT) idempotency
    // marking was added to apis/backend-api/web/openapi.yaml and materialized.
    expectedIdempotentOperations: 33,
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
  // The webserver app-api authority was retired together with the surface, so
  // the request under guard is the backend-admin one.
  const backendAuthority = JSON.parse(read('apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json'));
  assert.equal(
    backendAuthority.components.schemas.CreateApplicationDeploymentRequest.properties.idempotencyKey,
    undefined,
  );

  // Every authored PC surface must reach the header through generated SDK
  // params. The authored tree is walked explicitly, skipping `node_modules` and
  // `dist`: `fs.readdirSync(..., { recursive: true })` cannot prune those and
  // dies on the broken `.bin` entries pnpm leaves inside nested dependency
  // hoists. The scan replaces a hand-kept file list that had gone stale — it
  // still pointed at `…-admin-applications`, a package that no longer exists,
  // so the assertion died on ENOENT instead of guarding.
  const offenders = [];
  const walkAuthored = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        if (entry.name === 'node_modules' || entry.name === 'dist') continue;
        walkAuthored(path.join(directory, entry.name));
        continue;
      }
      if (!entry.isFile() || !/\.tsx?$/u.test(entry.name)) continue;
      const absolute = path.join(directory, entry.name);
      if (/Idempotency-Key/u.test(fs.readFileSync(absolute, 'utf8'))) {
        offenders.push(path.relative(ROOT, absolute).replace(/\\/gu, '/'));
      }
    }
  };
  walkAuthored(path.join(ROOT, 'apps/sdkwork-webserver-pc/packages'));
  assert.deepEqual(offenders, [], 'authored PC surfaces must use generated SDK params');
});
