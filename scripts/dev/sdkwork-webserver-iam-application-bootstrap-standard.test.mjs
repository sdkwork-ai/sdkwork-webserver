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

  assert.equal(rootManifest.backend.appId, 'sdkwork-web');
  assert.equal(rootManifest.backend.tenantId, '100001');
  assert.equal(rootManifest.backend.organizationId, '0');
  assert.equal(pcManifest.backend.appId, 'sdkwork-webserver-pc');
  assert.equal(pcManifest.backend.tenantId, '100001');
  assert.equal(pcManifest.backend.organizationId, '0');

  for (const profileId of ['standalone.development', 'cloud.development']) {
    const client = topology.orchestration.profiles[profileId].processes.find(
      (entry) => entry.id === 'webserver-pc-browser',
    );
    assert.equal(client.applicationRoot, 'apps/sdkwork-webserver-pc');
  }
});

test('standalone startup embeds IAM App API through its owner assembly', () => {
  const gatewayBootstrap = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/bootstrap.rs',
  );
  const profile = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/profile.rs',
  );
  const iamModuleBootstrap = read(
    'crates/sdkwork-api-webserver-standalone-gateway/src/iam_module_bootstrap.rs',
  );
  const gatewayCargo = read('crates/sdkwork-api-webserver-standalone-gateway/Cargo.toml');
  const workspaceCargo = read('Cargo.toml');

  assert.match(profile, /crate::iam_module_bootstrap::web_iam_module_manifest_path\(\)/u);
  assert.match(
    profile,
    /sdkwork_api_iam_assembly::assemble_app_api_contribution_with_module_manifests\(&\[\s*web_iam_manifest,\s*\]\)/u,
  );
  assert.match(profile, /sdkwork_api_drive_assembly::assemble_app_api_contribution\(\)/u);
  assert.match(profile, /compose_owner_contributions/u);
  assert.match(profile, /const DEPENDENCY_UNAVAILABLE_CODE: i32 = 50301/u);
  assert.match(profile, /assembly_unavailable\("sdkwork-iam"/u);
  assert.match(iamModuleBootstrap, /specs\/iam\.module\.manifest\.json/u);
  assert.match(gatewayBootstrap, /assemble_standalone_profile\(\)\s*\.await/u);
  assert.match(gatewayBootstrap, /with_web_request_context/u);
  assert.match(gatewayCargo, /sdkwork-api-iam-assembly/u);
  assert.match(gatewayCargo, /sdkwork-api-drive-assembly/u);
  assert.match(workspaceCargo, /sdkwork-api-iam-assembly/u);
  assert.match(workspaceCargo, /sdkwork-api-drive-assembly/u);
  assert.doesNotMatch(gatewayCargo, /sdkwork-iam-standalone-gateway/u);
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
