import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { ensureTrackedBuildSources } from '../../scripts/lib/build-source-integrity.mjs';
import { collectGenerationPlans } from '../../tools/generate_web_sdks.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function readJson(relativePath) {
  return JSON.parse(readFileSync(path.join(REPO_ROOT, relativePath), 'utf8'));
}

function runNode(args, cwd = REPO_ROOT) {
  return spawnSync(process.execPath, args, {
    cwd,
    encoding: 'utf8',
    windowsHide: true,
  });
}

test('root pnpm surface exposes every owned capability through canonical action-first names', () => {
  const scripts = readJson('package.json').scripts;
  for (const name of [
    'dev', 'dev:standalone', 'stop', 'build', 'test', 'check', 'verify', 'clean',
    'api:materialize', 'api:materialize:check', 'api:check',
    'sdk:generate', 'sdk:generate:check', 'sdk:check',
    'db:postgres:plan', 'db:postgres:init', 'db:postgres:migrate',
    'gateway:run:standalone', 'gateway:plan:standalone', 'gateway:build:standalone',
    'gateway:package:standalone', 'gateway:validate:standalone', 'gateway:matrix',
    'release:plan:standalone',
    'release:build:standalone',
    'release:package:standalone',
    'release:validate:standalone',
    'deploy:validate', 'deploy:plan:standalone',
    'deploy:apply:standalone',
    'deploy:rollback:standalone',
    'topology:validate', 'topology:plan', 'sbom:generate', 'sbom:check',
  ]) {
    assert.equal(typeof scripts[name], 'string', `missing canonical root script ${name}`);
  }
  // Standalone-only refactor: cloud-prefixed canonical scripts no longer exist.
  for (const name of Object.keys(scripts)) {
    assert.doesNotMatch(name, /:cloud$/u, `retired cloud script must not return: ${name}`);
  }
  assert.equal(scripts.dev, 'pnpm dev:standalone');
  assert.equal(scripts['dev:standalone'], 'pnpm exec sdkwork-app dev --deployment-profile standalone');
  assert.equal(scripts['_sdkwork:build'], 'node scripts/build.mjs --release');
  assert.equal(scripts['_sdkwork:clean'], 'node scripts/clean.mjs');
  assert.equal(
    scripts['gateway:route-composition:audit'],
    'node ../sdkwork-specs/tools/audit-gateway-route-composition-workspace.mjs --workspace .. --prefix sdkwork-webserver',
  );
  assert.match(scripts['_sdkwork:check'], /pnpm api:check/u);
  assert.match(scripts['_sdkwork:check'], /pnpm sdk:check/u);
  assert.match(scripts['_sdkwork:check'], /pnpm deploy:validate/u);
});

test('SDK generation covers every materialized manifest language', () => {
  const plans = collectGenerationPlans();
  const expected = [
    'sdkwork-webserver-app-sdk',
    'sdkwork-webserver-backend-sdk',
    'sdkwork-webserver-internal-sdk',
  ].flatMap((familyName) => {
    const manifest = readJson(`sdks/${familyName}/sdk-manifest.json`);
    return manifest.languages
      .filter((language) => language.generationState === 'materialized')
      .map((language) => `${familyName}/${language.language}`);
  });
  assert.deepEqual(
    plans.map((plan) => `${plan.sdkName}/${plan.language}`),
    expected,
  );
  assert.equal(plans.length, 26);
});

test('PC app surface delegates dev and stop while keeping its local lifecycle scoped', () => {
  const appRoot = 'apps/sdkwork-webserver-pc';
  const deployment = readJson(`${appRoot}/etc/sdkwork.deployment.config.json`);
  // Standalone-only refactor: one runtime-env per standalone lifecycle profile.
  const standaloneRuntimeEnvs = Object.entries(deployment.profiles).map(([profileId, profile]) => ({
    profileId,
    runtimeConfig: readJson(`${appRoot}/etc/${profile.source}`),
  }));
  const scripts = readJson(`${appRoot}/package.json`).scripts;
  assert.equal(deployment.kind, 'sdkwork.component-deployment');
  assert.equal(deployment.parentDeploymentConfig, '../../../etc/sdkwork.deployment.config.json');
  assert.equal(deployment.parentTopologySpec, '../../../specs/topology.spec.json');
  for (const { profileId, runtimeConfig } of standaloneRuntimeEnvs) {
    assert.equal(runtimeConfig.profileId, profileId);
    assert.equal(runtimeConfig.deploymentProfile, 'standalone');
    assert.equal(runtimeConfig.browserOriginMode, 'same-origin');
    for (const key of [
      'appApiBaseUrl',
      'backendApiBaseUrl',
      'driveAppApiBaseUrl',
      'appbaseAppApiBaseUrl',
    ]) {
      assert.equal(runtimeConfig[key], '/', `${profileId}.${key}`);
    }
    assert.doesNotMatch(JSON.stringify(runtimeConfig), /:(?:3800|3900)\b/u);
  }
  assert.equal(scripts.dev, 'pnpm dev:standalone');
  assert.equal(
    scripts['dev:standalone'],
    'pnpm exec sdkwork-app dev --root ../.. --deployment-profile standalone',
  );
  assert.equal(scripts.stop, 'pnpm exec sdkwork-app stop --root ../..');
  assert.match(scripts['build:standalone'], /--deployment-profile standalone/u);
  assert.doesNotMatch(scripts.build, /sdkwork-app/u);
  assert.doesNotMatch(scripts.test, /sdkwork-app/u);
  assert.doesNotMatch(scripts.clean, /sdkwork-app/u);
});

test('standalone profile exposes only the application gateway to browser clients', () => {
  const topology = readJson('specs/topology.spec.json');
  const env = readFileSync(
    path.join(REPO_ROOT, topology.profileFiles['standalone.development']),
    'utf8',
  );
  assert.doesNotMatch(env, /PLATFORM_API_GATEWAY_HTTP_URL/u);
  assert.match(env, /SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL=http:\/\/127\.0\.0\.1:3800/u);

  // Development: one adaptive-web delivery proxies both PC and H5 renderers
  // behind the single same-origin gateway ingress.
  const development = topology.orchestration.profiles['standalone.development'];
  assert.equal(development.browserDeliveries.length, 1);
  const delivery = development.browserDeliveries[0];
  assert.equal(delivery.id, 'webserver-adaptive-web');
  assert.equal(delivery.applicationRoot, 'apps/sdkwork-webserver-pc');
  assert.deepEqual(delivery.clientArchitectures, ['pc-web', 'h5']);
  assert.equal(delivery.originMode, 'same-origin');
  assert.equal(delivery.deliveryMode, 'dev-server-proxy');
  assert.equal(delivery.apiSurfaceId, 'application.public-ingress');
  assert.equal(delivery.clientProcessId, 'webserver-browser');
  assert.equal(delivery.preserveCanonicalPaths, true);

  // Production: the same gateway statically serves the PC and H5 bundles.
  const production = topology.orchestration.profiles['standalone.production'];
  assert.ok(production.browserDeliveries.length > 0);
  for (const staticDelivery of production.browserDeliveries) {
    assert.equal(staticDelivery.originMode, 'same-origin');
    assert.equal(staticDelivery.deliveryMode, 'gateway-static');
    assert.equal(staticDelivery.apiSurfaceId, 'application.public-ingress');
    assert.equal(staticDelivery.hostProcessId, 'application.public-ingress');
    assert.match(staticDelivery.buildOutput, /^apps\/sdkwork-webserver-(pc|h5)\/dist$/u);
  }
  const productionEnv = readFileSync(
    path.join(REPO_ROOT, topology.profileFiles['standalone.production']),
    'utf8',
  );
  assert.match(productionEnv, /SDKWORK_WEBSERVER_PC_STATIC_ROOT=\/usr\/share\/sdkwork\/webserver\/web\/pc/u);
  assert.match(productionEnv, /SDKWORK_WEBSERVER_H5_STATIC_ROOT=\/usr\/share\/sdkwork\/webserver\/web\/h5/u);
});

test('parent topology starts the browser client only in the standalone development profile', () => {
  const topology = readJson('specs/topology.spec.json');
  const development = topology.orchestration.profiles['standalone.development'].processes;
  const client = development.find((entry) => entry.id === 'webserver-browser');
  assert.ok(client, 'standalone development must start the browser client');
  assert.deepEqual(client.runtimeTargets, ['browser']);
  assert.deepEqual(client.clientArchitectures, ['pc-web', 'h5']);
  for (const profileId of ['standalone.test', 'standalone.production', 'standalone.staging']) {
    const processes = topology.orchestration.profiles[profileId].processes;
    assert.equal(
      processes.some((entry) => entry.id === 'webserver-browser'),
      false,
      `${profileId} must not start a browser client process`,
    );
  }
});

test('build source integrity restores a missing tracked source before continuing', () => {
  let exists = false;
  const calls = [];
  ensureTrackedBuildSources({
    repoRoot: REPO_ROOT,
    relativePaths: ['tracked/Cargo.toml'],
    fileExists: () => exists,
    inspectFile: () => ({ isFile: () => true, isSymbolicLink: () => false }),
    runProcess(command, args) {
      calls.push([command, ...args]);
      if (args[0] === 'checkout') exists = true;
      return { status: 0, stdout: 'tracked/Cargo.toml\n', stderr: '' };
    },
  });
  assert.deepEqual(calls, [
    ['git', 'ls-files', '--error-unmatch', '--', 'tracked/Cargo.toml'],
    ['git', 'checkout', 'HEAD', '--', 'tracked/Cargo.toml'],
  ]);
});

test('clean dry-run enumerates only approved reproducible outputs', () => {
  const result = runNode(['scripts/clean.mjs', '--dry-run']);
  assert.equal(result.status, 0, result.stderr);
  for (const target of [
    'dist', '.runtime/dev-sites', 'node_modules/.cache', 'node_modules/.vite',
    'apps/sdkwork-webserver-pc/dist',
  ]) {
    assert.match(result.stdout, new RegExp(`would remove ${target.replaceAll('.', '\\.')}`, 'u'));
  }
  assert.doesNotMatch(result.stdout, /public\/runtime-env\.json|etc\/|specs\/|database\//u);
  assert.match(result.stdout, /would run cargo clean/u);
});

test('materialization checks are deterministic and do not rewrite tracked output', () => {
  const apiCheck = runNode(['tools/materialize_web_phase1_contracts.mjs', '--check']);
  assert.equal(apiCheck.status, 0, apiCheck.stderr);
  const pcCheck = runNode(
    ['scripts/materialize-runtime-env.mjs', '--deployment-profile', 'standalone', '--environment', 'production', '--check'],
    path.join(REPO_ROOT, 'apps', 'sdkwork-webserver-pc'),
  );
  assert.equal(pcCheck.status, 0, pcCheck.stderr);
});
