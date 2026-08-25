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
  assert.match(readFileSync(script, 'utf8'), /sdkwork-ai\/sdkwork-space/u);
  assert.match(readFileSync(script, 'utf8'), /materialize-space-dist-aliases\.sh/u);
});

test('entrypoint materializes full reverse-proxy for platform API plane hosts', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /is_platform_api_gateway_module/u);
  assert.match(entrypoint, /write_platform_api_gateway_locations_docker/u);
  assert.match(entrypoint, /api-cloud-gateway\) printf '%s' "api"/u);
  assert.match(entrypoint, /api-dev\.\$\{brand\}/u);
});

test('module API gateway defaults to docker sibling deployment', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT:-\}?docker/u);
  for (const file of [
    'docker-compose.yml',
    'docker-compose.development.yml',
    'docker-compose.test.yml',
    'docker-compose.production.yml',
  ]) {
    const compose = readFileSync(path.join(appRoot, 'deployments', 'docker', file), 'utf8');
    assert.match(compose, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT:-\s*docker/u);
  }
  for (const environment of environments) {
    const envExample = readFileSync(
      path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`),
      'utf8',
    );
    assert.match(envExample, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker/u);
  }
});

test('entrypoint ensures platform API plane import and does not require gateway up', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /ensure_platform_api_gateway_import_listed/u);
  assert.match(entrypoint, /write_platform_api_gateway_locations_docker/u);
  assert.match(entrypoint, /SDKWORK_MODULE_API_GATEWAY_REQUIRED:-false/u);
  assert.match(entrypoint, /not waiting/u);
});

test('host nginx uninstall script exists and install-wsl-nginx is retired', () => {
  const uninstall = path.join(
    appRoot,
    'deployments',
    'docker',
    'scripts',
    'uninstall-wsl-nginx.sh',
  );
  const install = path.join(appRoot, 'deployments', 'docker', 'scripts', 'install-wsl-nginx.sh');
  assert.equal(existsSync(uninstall), true);
  assert.match(readFileSync(uninstall, 'utf8'), /apt-get purge/u);
  assert.match(readFileSync(install, 'utf8'), /RETIRED/u);
  assert.match(readFileSync(install, 'utf8'), /uninstall-wsl-nginx\.sh/u);
});

test('entrypoint rewrites module nginx upstream gateway to platform API gateway', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT/u);
  assert.match(entrypoint, /rewrite_module_gateway_upstream/u);
  assert.match(entrypoint, /prepare_module_api_gateway/u);
  assert.match(entrypoint, /start_bundled_module_api_gateway/u);
  assert.match(entrypoint, /sdkwork-api-cloud-gateway/u);
});

test('platform API gateway docker overlay and env examples document deployment modes', () => {
  const overlay = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.platform-api-gateway.yml'),
    'utf8',
  );
  assert.match(overlay, /sdkwork-api-cloud-gateway/u);
  assert.match(overlay, /platform-api-gateway/u);
  assert.match(overlay, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT: docker/u);
  for (const environment of environments) {
    const envExample = readFileSync(
      path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`),
      'utf8',
    );
    assert.match(envExample, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker/u);
    assert.match(envExample, /SDKWORK_MODULE_API_GATEWAY_PORT=3900/u);
  }
});

test('docker compose files expose module API gateway deployment env', () => {
  for (const file of [
    'docker-compose.yml',
    'docker-compose.development.yml',
    'docker-compose.test.yml',
    'docker-compose.production.yml',
  ]) {
    const compose = readFileSync(path.join(appRoot, 'deployments', 'docker', file), 'utf8');
    assert.match(compose, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT/u);
    assert.match(compose, /SDKWORK_MODULE_API_GATEWAY_PORT/u);
  }
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
  assert.match(entrypoint, /sdkwork-ai\/sdkwork-space/u);
  assert.match(entrypoint, /environment_dist_alias/u);
  assert.match(entrypoint, /dist\/\$\{dist_alias\}/u);
  assert.match(entrypoint, /adaptive-web\.named-locations\.docker\.conf/u);
  // imports.d/import.conf aggregates checkout nginx sidecars; layout-imports.toml
  // covers layout v3 modules without nginx sidecars.
  assert.match(entrypoint, /imports\.d\/import\.conf/u);
  assert.match(entrypoint, /imports\.d\/layout-imports\.toml/u);
  assert.match(entrypoint, /include = \$\{webserver_includes/u);
  // Startup serves the merged module-imports data plane: management API runs
  // in the background, serve-imports in the foreground.
  assert.match(entrypoint, /serve-imports/u);
  assert.match(entrypoint, /management in background/u);
  assert.match(entrypoint, /build-browser/u);
  assert.match(entrypoint, /build-module-browser\.mjs/u);
  assert.match(entrypoint, /--architecture/u);
  assert.match(entrypoint, /reload-module-static/u);
});

test('standalone image and compose publish public data plane on 80/443', () => {
  const dockerfile = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'Dockerfile.standalone'),
    'utf8',
  );
  assert.doesNotMatch(dockerfile, /SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS=80=8080/u);
  assert.match(dockerfile, /EXPOSE 80 443 3800/u);
  const development = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.development.yml'),
    'utf8',
  );
  assert.match(development, /NET_BIND_SERVICE/u);
  assert.match(development, /IMPORT_HTTP_HOST_PORT:-80\}:80"/u);
  assert.match(development, /HTTPS_HOST_PORT:-443\}:443"/u);
  for (const file of [
    'docker-compose.yml',
    'docker-compose.development.yml',
    'docker-compose.test.yml',
    'docker-compose.production.yml',
  ]) {
    const compose = readFileSync(path.join(appRoot, 'deployments', 'docker', file), 'utf8');
    assert.match(compose, /IMPORT_HTTP_HOST_PORT/u);
    assert.match(compose, /:80"/u);
    assert.match(compose, /:443"/u);
  }
});

test('standalone image build stages bundled platform API gateway by default', () => {
  const script = readFileSync(
    path.join(appRoot, 'scripts', 'docker', 'build-standalone-image.mjs'),
    'utf8',
  );
  assert.match(script, /stagePlatformGatewayInstall/u);
  assert.match(script, /CLOUD_GATEWAY_INSTALL_DIR/u);
  assert.match(script, /container-image-build/u);
  assert.match(script, /sdkwork-api-cloud-gateway/u);
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

test('module space import auto-discovery is the docker default', () => {
  // Entrypoint (SDKWORK_WEBSERVER_SPEC.md §17): every enabled sibling module
  // is imported when AUTO_DISCOVER=true; SDKWORK_SPACE_MODULES pins only when
  // AUTO_DISCOVER is false.
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /SDKWORK_SPACE_AUTO_DISCOVER/u);
  assert.match(entrypoint, /server\.\$\{env_name\}\.toml/u);
  assert.match(
    entrypoint,
    /AUTO_DISCOVER=true[\s\S]*printf '%s' "\$\{discovered\}"[\s\S]*printf '%s' "\$\{SDKWORK_SPACE_MODULES:-\}"/u,
  );
  for (const file of [
    'docker-compose.yml',
    'docker-compose.development.yml',
    'docker-compose.test.yml',
    'docker-compose.production.yml',
  ]) {
    const compose = readFileSync(path.join(appRoot, 'deployments', 'docker', file), 'utf8');
    assert.match(compose, /SDKWORK_SPACE_AUTO_DISCOVER: \$\{SDKWORK_SPACE_AUTO_DISCOVER:-true\}/u);
    assert.match(compose, /\$\{SDKWORK_SPACE_HOST_PATH:-\/opt\/deploy\}:\/opt\/deploy:ro/u);
    assert.match(
      compose,
      /\$\{SDKWORK_SPACE_CHECKOUT_HOST_PATH:-\$\{SDKWORK_SPACE_HOST_PATH:-\/opt\/deploy\}\/sdkwork-space\}:\/opt\/deploy\/sdkwork-space:rw/u,
    );
  }
  for (const environment of ['development', 'test', 'production']) {
    const envExample = readFileSync(
      path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`),
      'utf8',
    );
    assert.match(envExample, /SDKWORK_SPACE_AUTO_DISCOVER=true/u);
    const envFile = readFileSync(
      path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env`),
      'utf8',
    );
    assert.match(envFile, /SDKWORK_SPACE_AUTO_DISCOVER=true/u);
    assert.match(envFile, /SDKWORK_SPACE_MODULES=\s*$/m);
    assert.match(envFile, /SDKWORK_SPACE_CHECKOUT_HOST_PATH=\/opt\/deploy\/sdkwork-space/u);
  }
});
