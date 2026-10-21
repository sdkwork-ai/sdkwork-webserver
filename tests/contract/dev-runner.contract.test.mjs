import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { parseEnv } from 'node:util';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('root dev commands use the SDKWork app lifecycle for this server application', () => {
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));

  assert.equal(packageJson.scripts.dev, 'pnpm dev:standalone');
  assert.equal(
    packageJson.scripts['dev:standalone'],
    'pnpm exec sdkwork-app dev --deployment-profile standalone',
  );
  assert.equal(
    packageJson.scripts['dev:server'],
    'pnpm exec sdkwork-app dev --runtime-target server --deployment-profile standalone',
  );
  assert.equal(packageJson.scripts['dev:browser'], undefined);
  assert.equal(packageJson.scripts['dev:browser:postgres'], undefined);
  assert.equal(packageJson.scripts['dev:browser:postgres:standalone'], undefined);
  // Web Server is standalone-only: no retired *:cloud dev scripts may be resurrected.
  assert.equal(packageJson.scripts['dev:cloud'], undefined);
  for (const [name] of Object.entries(packageJson.scripts)) {
    // `import:switch:*` selects the sibling-module import set (a runtime input
    // plane), not a cloud build/deploy profile, so the standalone-only rule
    // does not retire it.
    if (name === 'import:switch:cloud') {
      continue;
    }
    assert.ok(!name.endsWith(':cloud'), `script "${name}" must not exist (standalone-only)`);
  }
});

test('deployment index owns all supported Web Server profiles', () => {
  const deployment = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'etc', 'sdkwork.deployment.config.json'), 'utf8'),
  );
  assert.equal(deployment.application, 'sdkwork-webserver');
  assert.equal(deployment.topology, '../specs/topology.spec.json');
  assert.equal(deployment.defaultProfile, 'standalone.development');
  assert.deepEqual(Object.keys(deployment.profiles).sort(), [
    'standalone.demo',
    'standalone.development',
    'standalone.production',
    'standalone.staging',
    'standalone.test',
  ]);
  for (const profileName of Object.keys(deployment.profiles)) {
    assert.ok(
      profileName.startsWith('standalone.'),
      `profile "${profileName}" must be standalone (standalone-only deployment)`,
    );
  }
  assert.equal(
    deployment.environments.development.applicationOrigin,
    'http://127.0.0.1:3800',
  );
  assert.equal(deployment.environments.production.applicationOrigin, 'https://server.sdkwork.com');
});

test('standalone development leaves the Snowflake node id to the registry', () => {
  const profile = parseEnv(
    readFileSync(
      path.join(REPO_ROOT, 'etc', 'topology', 'standalone.development.env'),
      'utf8',
    ),
  );

  // `0` is the documented auto sentinel: the runtime allocates a lease from
  // sdkwork_node_registry instead of pinning node 0, so replicas of the same
  // deployment cannot collide.
  assert.equal(profile.SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID, '0');
});
