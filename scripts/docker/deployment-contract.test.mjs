import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';

import { parseDotEnv } from '../../../sdkwork-specs/tools/postgres/postgres-config.mjs';
import {
  resolveExampleEnvironment,
  validateDeploymentEnvironment,
  validateDeploymentMatrix,
} from './validate-docker-deployment.mjs';

const appRoot = path.resolve('.');
const environments = ['development', 'test', 'production'];
const baseDomains = ['sdkwork.com'];
const suffixes = { development: 'dev', test: 'test', production: '' };

function expectedHosts(environment) {
  const role = suffixes[environment] ? `server-${suffixes[environment]}` : 'server';
  return baseDomains.map((domain) => `${role}.${domain}`);
}

test('docker env examples bind the registered web host family', () => {
  for (const environment of environments) {
    const file = path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`);
    const env = parseDotEnv(readFileSync(file, 'utf8'));
    const resolved = resolveExampleEnvironment(env, 'embedded');
    const summary = validateDeploymentEnvironment(resolved, 'embedded');
    assert.deepEqual(summary.hosts, expectedHosts(environment));
  }
});

test('container deployment matrix covers embedded and external modes', () => {
  const summaries = validateDeploymentMatrix(appRoot);
  assert.equal(summaries.length, environments.length * 2);
  assert.deepEqual(new Set(summaries.map(({ mode }) => mode)), new Set(['embedded', 'external']));
});

test('external compose override disables embedded dependencies', () => {
  const override = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.external.yml'),
    'utf8',
  );
  assert.match(override, /profiles: \["external-disabled"\]/u);
  assert.match(override, /WEBSERVER_POSTGRES_HOST:\?/u);
  assert.match(override, /WEBSERVER_REDIS_HOST:\?/u);
  assert.match(override, /depends_on: !reset \[\]/u);
});

test('built-in compose provisions postgres and redis services', () => {
  const compose = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.yml'),
    'utf8',
  );
  assert.match(compose, /postgres:/u);
  assert.match(compose, /redis:/u);
  assert.match(compose, /WEBSERVER_POSTGRES_DEV_DB/u);
  assert.match(compose, /SDKWORK_WEBSERVER_REDIS_ENABLED: "true"/u);
});

test('standalone compose bind-mounts host /opt/deploy for sdkwork-space', () => {
  const compose = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.development.yml'),
    'utf8',
  );
  assert.match(compose, /SDKWORK_SPACE_HOST_PATH.*\/opt\/deploy/u);
  assert.match(compose, /\$\{SDKWORK_SPACE_HOST_PATH:-\/opt\/deploy\}:\/opt\/deploy:ro/u);
  assert.match(compose, /SDKWORK_SPACE_CHECKOUT_HOST_PATH.*\/opt\/deploy\/sdkwork-space/u);
  assert.doesNotMatch(compose, /webserver-opt-deploy-development/u);
});

test('embedded compose shares /opt/deploy bind mount across profiles', () => {
  const compose = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.yml'),
    'utf8',
  );
  assert.match(compose, /\$\{SDKWORK_SPACE_HOST_PATH:-\/opt\/deploy\}:\/opt\/deploy:ro/u);
  assert.match(compose, /SDKWORK_SPACE_CLONE_URL/u);
  assert.match(compose, /SDKWORK_WEBSERVER_MODULE_IMPORT_REQUIRED/u);
  assert.doesNotMatch(compose, /webserver-opt-deploy-development/u);
});

test('space clone helper script exists', () => {
  const script = path.join(
    appRoot,
    'deployments',
    'docker',
    'scripts',
    'setup-host-space-clone.sh',
  );
  assert.equal(existsSync(script), true);
  assert.match(readFileSync(script, 'utf8'), /Sdkwork-Cloud\/sdkwork-space/u);
});

test('entrypoint discovers sdkwork-space modules and writes imports', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /discover_importable_modules/u);
  assert.match(entrypoint, /\[\[webserver\.imports\]\]/u);
  assert.match(entrypoint, /materialize_module_webserver_configs/u);
  assert.match(entrypoint, /module-app-roots/u);
  assert.match(entrypoint, /Sdkwork-Cloud\/sdkwork-space/u);
});

test('docker nginx dual-authority confs are retired', () => {
  for (const name of ['development.conf', 'test.conf', 'production.conf']) {
    const retired = path.join(appRoot, 'deployments', 'docker', 'nginx', name);
    assert.equal(existsSync(retired), false, `${name} must not exist`);
  }
  assert.match(
    readFileSync(path.join(appRoot, 'deployments', 'docker', 'nginx', 'README.md'), 'utf8'),
    /deployments\/webserver\//u,
  );
});
