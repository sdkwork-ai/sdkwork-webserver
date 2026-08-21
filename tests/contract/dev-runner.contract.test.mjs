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
    packageJson.scripts['dev:cloud'],
    'pnpm exec sdkwork-app dev --deployment-profile cloud',
  );
  assert.equal(
    packageJson.scripts['dev:server'],
    'pnpm exec sdkwork-app dev --runtime-target server --deployment-profile standalone',
  );
  assert.equal(packageJson.scripts['dev:browser'], undefined);
  assert.equal(packageJson.scripts['dev:browser:postgres'], undefined);
  assert.equal(packageJson.scripts['dev:browser:postgres:standalone'], undefined);
});

test('deployment index owns all supported Web Server profiles', () => {
  const deployment = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'etc', 'sdkwork.deployment.config.json'), 'utf8'),
  );
  assert.equal(deployment.application, 'sdkwork-webserver');
  assert.equal(deployment.topology, '../specs/topology.spec.json');
  assert.equal(deployment.defaultProfile, 'standalone.development');
  assert.deepEqual(Object.keys(deployment.profiles).sort(), [
    'cloud.development',
    'cloud.production',
    'standalone.development',
    'standalone.production',
  ]);
  assert.equal(deployment.environments.development.cloudApiBaseUrl, 'https://api-dev.sdkwork.com');
  assert.equal(deployment.environments.production.applicationOrigin, 'https://server.sdkwork.com');
});

test('standalone development supplies an explicit single-node Snowflake id', () => {
  const profile = parseEnv(
    readFileSync(
      path.join(REPO_ROOT, 'etc', 'topology', 'standalone.development.env'),
      'utf8',
    ),
  );

  assert.equal(profile.SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID, '0');
});
