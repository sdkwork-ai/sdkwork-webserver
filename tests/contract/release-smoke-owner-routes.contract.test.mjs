import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SMOKE_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-release-smoke.mjs');
const GATEWAY_ASSEMBLY = path.join(
  REPO_ROOT,
  'crates',
  'sdkwork-api-webserver-standalone-gateway',
  'src',
  'dependency_assembly.rs',
);
const WORKSPACE_ROOT = path.resolve(REPO_ROOT, '..');
const DRIVE_ROUTE_MANIFEST = path.join(
  WORKSPACE_ROOT,
  'sdkwork-drive',
  'crates',
  'sdkwork-routes-drive-app-api',
  'src',
  'http_route_manifest.rs',
);

/// Sources are read with CRLF normalised to LF. A checkout on Windows carries
/// CRLF and a checkout in CI carries LF; anchoring a regex at `$` without
/// tolerating the carriage return made this gate pass locally and fail in CI
/// (or the reverse) for a reason that has nothing to do with the behaviour.
function readNormalized(...segments) {
  return readFileSync(path.join(...segments), 'utf8').replace(/\r\n/gu, '\n');
}

const readSmokeSource = () => readNormalized(SMOKE_SCRIPT);

/// The owners the standalone gateway actually composes, read from the assembly
/// source rather than restated here. A probe may only target a surface one of
/// these owners mounts: an owner the gateway never composes can never answer,
/// so probing it fails the release over a route that does not exist.
function readComposedOwners() {
  const source = readNormalized(GATEWAY_ASSEMBLY);
  const owners = new Set(
    [...source.matchAll(/^const ([A-Z0-9_]+_OWNER): &str = "([^"]+)";$/gmu)].map((match) => match[2]),
  );
  assert.ok(owners.size > 0, 'the gateway must declare its owner ids as string constants');
  return owners;
}

/// Every `dual_token` / `access_token` GET route the drive app-api manifest
/// mounts, as `[path, operationId]` pairs.
function readDriveGetRoutes() {
  const source = readNormalized(DRIVE_ROUTE_MANIFEST);
  return [
    ...source.matchAll(
      /HttpRoute::(?:dual_token|access_token)\(\s*HttpMethod::Get,\s*"([^"]+)",\s*"[^"]*",\s*"([^"]+)",?\s*\)/gmu,
    ),
  ].map((match) => [match[1], match[2]]);
}

test('the smoke probes only surfaces a composed owner can mount', () => {
  const owners = readComposedOwners();
  const source = readSmokeSource();

  // The regression: `/app/v3/api/assets` is owned by the `sdkwork-assets`
  // workspace. The drive assembly deliberately excludes it and this gateway
  // composes no assets module, so the probe could only ever fail.
  assert.ok(
    !owners.has('sdkwork-assets'),
    'this assertion encodes the gateway owner list; if assets is ever composed, revisit it',
  );
  assert.doesNotMatch(
    source,
    /'\/app\/v3\/api\/assets'/u,
    'the smoke must not probe a route owned by an uncomposed workspace',
  );
  assert.doesNotMatch(source, /assets\.list/u, 'the assets operationId must be gone from the smoke');

  const probePaths = new Set(
    [...source.matchAll(/^\s{2}\w+: '(\/[^']+)',$/gmu)].map((match) => match[1]),
  );
  // One probe per composed dependency owner that answers an unauthenticated
  // request with problem+json 401: deployments, iam, drive.
  for (const required of [
    '/app/v3/api/apps',
    '/app/v3/api/auth/sessions/current',
    '/app/v3/api/drive/favorites',
  ]) {
    assert.ok(probePaths.has(required), `the same-origin path table must probe ${required}`);
  }
});

test(
  'the drive probe targets a route the drive app-api manifest actually mounts',
  { skip: !existsSync(DRIVE_ROUTE_MANIFEST) && 'sdkwork-drive checkout not present' },
  () => {
    const source = readSmokeSource();
    const match = source.match(/^\s{2}drive\w+: '(\/app\/v3\/api\/drive[^']*)',$/mu);
    assert.ok(match, 'the smoke must probe exactly one drive-owned route');
    const probedPath = match[1];

    const driveGetRoutes = readDriveGetRoutes();
    assert.ok(driveGetRoutes.length > 0, 'the drive manifest must expose GET routes');
    const mounted = driveGetRoutes.find(([routePath]) => routePath === probedPath);
    assert.ok(
      mounted,
      `${probedPath} is not mounted by the drive app-api manifest; candidates: ${driveGetRoutes
        .map(([routePath]) => routePath)
        .join(', ')}`,
    );
    assert.ok(
      source.includes(`operationId: '${mounted[1]}'`),
      `the probed operationId must be the drive manifest's own id for ${probedPath} (${mounted[1]})`,
    );
  },
);
