import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(REPO_ROOT, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

test('credential entry uses the PC manifest identity in every client profile', () => {
  const rootManifest = readJson('sdkwork.app.config.json');
  const pcManifest = readJson('apps/sdkwork-webserver-pc/sdkwork.app.config.json');
  const topology = readJson('specs/topology.spec.json');

  assert.equal(rootManifest.backend.appId, 'sdkwork-webserver');
  assert.equal(rootManifest.backend.tenantId, '100001');
  assert.equal(rootManifest.backend.organizationId, '0');
  assert.equal(pcManifest.backend.appId, 'sdkwork-webserver-pc');
  assert.equal(pcManifest.backend.tenantId, '100001');
  assert.equal(pcManifest.backend.organizationId, '0');

  // Standalone-only topology: every profile must be standalone, and the local
  // development profile owns the PC browser process (renamed from the retired
  // `webserver-pc-browser` id); server-only profiles do not run a browser.
  for (const profileId of Object.keys(topology.orchestration.profiles)) {
    assert.ok(
      profileId.startsWith('standalone.'),
      `profile "${profileId}" must be standalone (standalone-only deployment)`,
    );
  }
  const developmentProfile = topology.orchestration.profiles['standalone.development'];
  const client = developmentProfile.processes.find((entry) => entry.id === 'webserver-browser');
  assert.equal(client?.applicationRoot, 'apps/sdkwork-webserver-pc');
});

test('standalone startup embeds every IAM owner surface through one contribution', () => {
  const gatewayBootstrap = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/bootstrap.rs',
  );
  const profile = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/profile.rs',
  );
  const dependencyAssembly = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/dependency_assembly.rs',
  );
  const iamModuleBootstrap = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/iam_module_bootstrap.rs',
  );
  const gatewayCargo = read('crates/sdkwork-api-webserver-standalone-gateway/Cargo.toml');
  const workspaceCargo = read('Cargo.toml');

  assert.match(profile, /crate::dependency_assembly::same_origin_dependency_modules\(\)/u);
  assert.match(dependencyAssembly, /crate::iam_module_bootstrap::federated_iam_module_manifest_paths\(\)/u);
  assert.match(
    dependencyAssembly,
    /sdkwork_api_iam_assembly::assemble_owner_api_surfaces_with_pool_and_module_manifests\(\s*pool,\s*&manifest_paths,?\s*\)/u,
  );
  assert.match(profile, /compose_owner_modules/u);
  assert.match(profile, /const DEPENDENCY_UNAVAILABLE_CODE: i32 = 50301/u);
  assert.match(dependencyAssembly, /const IAM_OWNER: &str = "sdkwork-iam"/u);
  assert.match(iamModuleBootstrap, /specs\/iam\.module\.manifest\.json/u);
  assert.match(gatewayBootstrap, /assemble_standalone_profile\(\)\s*\.await/u);
  assert.match(gatewayBootstrap, /iam_web_request_context_resolver_from_env/u);
  assert.match(gatewayBootstrap, /with_problem_correlation/u);
  assert.match(gatewayCargo, /sdkwork-api-iam-assembly/u);
  assert.match(gatewayCargo, /sdkwork-api-drive-assembly/u);
  assert.match(workspaceCargo, /sdkwork-api-iam-assembly/u);
  assert.match(workspaceCargo, /sdkwork-api-drive-assembly/u);
  assert.doesNotMatch(gatewayCargo, /sdkwork-iam-standalone-gateway/u);
});

test('the gateway declares both IAM surfaces against one owner contribution', () => {
  // The platform cloud account center lives on the IAM **backend** surface
  // (`/backend/v3/api/iam/provider_accounts`, `/iam/provider_credentials/*`),
  // not the App API surface. Declaring only `app-api` left the account center
  // unmounted while the component contract still declared it as served: the
  // gateway composed the App API contribution, so startup, readiness, and the
  // static assembly gates all stayed green and the console received an empty
  // 404 from the axum fallback. Both surfaces must therefore be declared, and
  // they must resolve to the *same* executable export, because
  // API_ASSEMBLY_SPEC §4.1.1 gives one served owner exactly one contribution —
  // two exports would fail composition (or silently drop one surface).
  const component = readJson(
    'crates/sdkwork-api-webserver-standalone-gateway/specs/component.spec.json',
  );
  const iamSurfaces = component.contracts.dependencyApiSurfaces.filter(
    (surface) => surface.workspace === 'sdkwork-iam' && surface.runtimeMode === 'same-origin',
  );

  assert.deepEqual(
    iamSurfaces.map((surface) => surface.surface).sort(),
    ['app-api', 'backend-api'],
    'the standalone gateway must declare every same-origin IAM surface it serves',
  );

  const executableExports = new Set(
    iamSurfaces.map((surface) => surface.embeddedExecutableExport),
  );
  assert.equal(
    executableExports.size,
    1,
    'both IAM surfaces must arrive as one owner contribution, not one export per surface',
  );

  const [iamExport] = executableExports;
  const requiredPortExports = (component.contracts.requiredPorts ?? []).map((port) => port.export);
  assert.ok(
    requiredPortExports.includes(iamExport),
    `the IAM surface export ${iamExport} needs a matching requiredPorts entry`,
  );
});

test('standalone profiles no longer carry the temporary Drive AnyPool driver exception', () => {
  const developmentProfile = read('etc/topology/standalone.development.env');
  const productionProfile = read('etc/topology/standalone.production.env');
  const poolContract = readJson('specs/process-database-pool.spec.json');
  const processContract = poolContract.processes.find(
    (entry) => entry.id === 'sdkwork-api-webserver-standalone-gateway',
  );

  // The temporary sqlx::AnyPool mechanism was removed: no production code
  // path consumes it, so profiles and the pool contract must not re-introduce
  // the exception flags (standards-alignment: "no production code path
  // consumes the temporary AnyPool mechanism").
  for (const profile of [developmentProfile, productionProfile]) {
    assert.doesNotMatch(profile, /^SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION=/mu);
    assert.doesNotMatch(profile, /^SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT=/mu);
  }
  assert.equal(processContract.temporaryDriverPoolCountEnv, undefined);
  assert.deepEqual(processContract.temporaryDriverExceptions, []);
});

test('standalone runner injects owner runtime roots and keeps real auth enabled', () => {
  const topologyHelper = read('scripts/lib/webserver-topology.mjs');
  const devRunner = read('scripts/webserver-dev.mjs');
  const topology = readJson('specs/topology.spec.json');
  const gateway = topology.orchestration.profiles['standalone.development'].processes.find(
    (entry) => entry.id === 'application.public-ingress',
  );

  assert.equal(gateway.script, '_sdkwork:gateway:standalone');
  assert.match(topologyHelper, /SDKWORK_APP_ROOT:\s*REPO_ROOT/u);
  assert.match(topologyHelper, /SDKWORK_IAM_APP_ROOT:\s*IAM_REPO_ROOT/u);
  assert.match(topologyHelper, /SDKWORK_DRIVE_APP_ROOT:\s*DRIVE_REPO_ROOT/u);
  assert.match(devRunner, /resolveIamDevEnv/u);
  assert.match(devRunner, /IAM_APPLICATION_BOOTSTRAP_ENV/u);
  assert.doesNotMatch(devRunner, /SDKWORK_WEBSERVER_DEV_AUTH_BYPASS/u);
});
