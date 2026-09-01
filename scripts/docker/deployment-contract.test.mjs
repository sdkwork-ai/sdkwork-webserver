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
const environments = ['development', 'test', 'staging', 'production'];
const baseDomains = ['sdkwork.com'];
const suffixes = { development: 'dev', test: 'test', staging: 'staging', production: '' };

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

test('entrypoint imports the platform API gateway sidecar for api*.brand hosts', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /is_platform_api_gateway_module/u);
  // Checkout-direct import (§17.3): api*.brand reverse proxy comes from the
  // sibling module's own cloud sidecar, never from entrypoint-rewritten conf.
  assert.match(entrypoint, /ensure_platform_api_gateway_module_checkout/u);
  assert.match(entrypoint, /ensure_platform_api_gateway_import_listed/u);
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
    'docker-compose.staging.yml',
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
  assert.match(entrypoint, /discover_module_api_gateway_allowed_hosts/u);
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

test('entrypoint includes module nginx sidecars checkout-direct without rewriting', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  // §17.3: sibling sidecars stay the single source of truth; the aggregator
  // includes each checkout conf directly and never sed-rewrites upstreams.
  assert.match(entrypoint, /module_nginx_sidecar_abs_path/u);
  assert.match(entrypoint, /High-cohesion import: the module's own checkout sidecar/u);
  assert.doesNotMatch(entrypoint, /rewrite_module_gateway_upstream/u);
  assert.match(entrypoint, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT/u);
  assert.match(entrypoint, /prepare_module_api_gateway/u);
  assert.match(entrypoint, /start_bundled_module_api_gateway/u);
  assert.match(entrypoint, /sdkwork-api-cloud-gateway/u);
  // Every environment branch the entrypoint can take must be explicit: the
  // platform API host allowlist and CORS defaults cover dev/test/staging/prod
  // (APP_RUNTIME_TOPOLOGY_NAMING §9 — api-dev/api-test/api-staging/api).
  assert.match(entrypoint, /api-staging\.\$\{brand\}/u);
  assert.match(entrypoint, /server-staging\.\$\{domain\}/u);
});

test('platform API gateway overlays align with the canonical sibling upstream port', () => {
  const overlay = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.platform-api-gateway.yml'),
    'utf8',
  );
  const embedded = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.platform-api-gateway.embedded.yml'),
    'utf8',
  );
  // Cloud-mode sidecars dial sdkwork-api-cloud-gateway:8080 checkout-direct;
  // the container must answer on that port (§17.3).
  for (const compose of [overlay, embedded]) {
    assert.match(compose, /SDKWORK_MODULE_API_GATEWAY_UPSTREAM_PORT:-8080/u);
    assert.match(compose, /SDKWORK_API_CLOUD_GATEWAY_BIND: 0\.0\.0\.0:\$\{SDKWORK_MODULE_API_GATEWAY_UPSTREAM_PORT:-8080\}/u);
  }
  assert.match(overlay, /aliases:\s*\n\s*- sdkwork-api-cloud-gateway/u);
  // Attach mode documents the alias + declared-port requirement.
  for (const name of [
    'docker-compose.platform-api-gateway-attach.yml',
    'docker-compose.platform-api-gateway-attach.embedded.yml',
  ]) {
    const attach = readFileSync(path.join(appRoot, 'deployments', 'docker', name), 'utf8');
    assert.match(attach, /SDKWORK_MODULE_API_GATEWAY_PORT:-8080/u);
    assert.match(attach, /sdkwork-api-cloud-gateway:8080/u);
  }
  assert.match(overlay, /platform-api-gateway/u);
  assert.match(overlay, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT: docker/u);
  for (const environment of environments) {
    const envExample = readFileSync(
      path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`),
      'utf8',
    );
    assert.match(envExample, /SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker/u);
    assert.match(envExample, /SDKWORK_MODULE_API_GATEWAY_PORT=8080/u);
  }
});

test('docker compose files expose module API gateway deployment env', () => {
  for (const file of [
    'docker-compose.yml',
    'docker-compose.development.yml',
    'docker-compose.test.yml',
    'docker-compose.staging.yml',
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
  assert.match(entrypoint, /materialize_product_edge_nginx_conf/u);
  assert.match(entrypoint, /webserver_adaptive_shell/u);
  assert.match(entrypoint, /product-edge-nginx\.conf/u);
  assert.match(entrypoint, /sdkwork-ai\/sdkwork-space/u);
  assert.match(entrypoint, /environment_dist_alias/u);
  assert.match(entrypoint, /dist\/\$\{dist_alias\}/u);
  // Adaptive Web by_environment prefers discovered PC/H5 before static-fallback.
  assert.match(entrypoint, /pc_dev:-\$\{pc_root:-\$\{static_fallback\}\}/u);
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

test('entrypoint materializes sibling Adaptive Web static roots in-container', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  // §13.6/§17: sidecar @pc/@h5 named locations dispatch to package roots that
  // the image does not own; the entrypoint links them to checkout dist trees.
  assert.match(entrypoint, /materialize_module_web_static_roots/u);
  assert.match(entrypoint, /module_adaptive_named_root/u);
  assert.match(entrypoint, /location @\$\{surface\}/u);
  assert.match(entrypoint, /SDKWORK_WEBSERVER_MODULE_WEB_ROOT:-\/usr\/share\/sdkwork/u);
  assert.match(entrypoint, /ln -sfn "\$\{src\}" "\$\{root\}"/u);
  // Non-symlink content is never replaced (fail-safe materialization).
  assert.match(entrypoint, /refusing to replace non-symlink content/u);
  // Serving profile follows the active import set unless explicitly pinned.
  assert.match(entrypoint, /SDKWORK_WEBSERVER_STATIC_SOURCE_PROFILE:-\$\(webserver_import_profile\)/u);
});

test('imported sidecar TLS certificates bootstrap into the canonical ACME layout', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /ensure_imported_sidecar_certificates \|\| true/u);
  assert.match(entrypoint, /lets_encrypt_certs_root/u);
  assert.match(entrypoint, /SDKWORK_WEBSERVER_CERTS_LETS_ENCRYPT_DIR:-\/etc\/sdkwork\/certs\/letsencrypt/u);
  assert.match(entrypoint, /fullchain\.pem/u);
  assert.match(entrypoint, /privkey\.pem/u);
});

test('product edge emits production TLS blocks with health probe locations', () => {
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  // Production: one TLS server block per brand domain (W11/W25/W26).
  assert.match(entrypoint, /listen 443 ssl;/u);
  assert.match(entrypoint, /ssl_protocols TLSv1\.2 TLSv1\.3;/u);
  assert.match(entrypoint, /ssl_certificate \$\{lets_root\}\/\$\{hostdom\}\/fullchain\.pem;/u);
  // Every environment: health/ready probes terminate on this edge.
  assert.match(entrypoint, /location = \/healthz \{/u);
  assert.match(entrypoint, /location = \/readyz \{/u);
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
    'docker-compose.staging.yml',
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
    'docker-compose.staging.yml',
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
  for (const environment of ['development', 'test', 'staging', 'production']) {
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

test('drive delivery cache mount and env contract is complete (DRIVE_SPEC §17)', () => {
  // Dockerfile: /opt/deploy/drive exists, is owned, and carries cache defaults.
  const dockerfile = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'Dockerfile.standalone'),
    'utf8',
  );
  assert.match(dockerfile, /^\s*\/opt\/deploy\/drive \\/mu);
  assert.match(dockerfile, /chown sdkwork:sdkwork \/opt\/deploy \/opt\/deploy\/drive/u);
  assert.match(dockerfile, /SDKWORK_DRIVE_WEBSITE_CACHE_ROOT=\/opt\/deploy\/drive\/website-cache/u);
  assert.match(dockerfile, /SDKWORK_DRIVE_WEBSITE_CACHE_MAX_TOTAL_BYTES=8589934592/u);
  assert.match(dockerfile, /SDKWORK_DRIVE_WEBSITE_CACHE_MAX_ENTRIES=100000/u);

  // Entrypoint: shared cache root bootstrap is fail-safe (warn, never fail).
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  assert.match(entrypoint, /ensure_drive_delivery_cache_root/u);
  assert.match(entrypoint, /\/opt\/deploy\/drive\/website-cache/u);

  // Every compose layout binds the shared host mount and per-env cache vars.
  const perEnvironment = {
    'docker-compose.yml': /SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT:\s*(development|test|staging|production)/u,
    'docker-compose.development.yml': /SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT:\s*development/u,
    'docker-compose.test.yml': /SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT:\s*test/u,
    'docker-compose.staging.yml': /SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT:\s*staging/u,
    'docker-compose.production.yml': /SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT:\s*production/u,
  };
  for (const [file, environmentPattern] of Object.entries(perEnvironment)) {
    const compose = readFileSync(path.join(appRoot, 'deployments', 'docker', file), 'utf8');
    assert.match(compose, /\$\{SDKWORK_DRIVE_CACHE_HOST_PATH:-\/opt\/deploy\/drive\}:\/opt\/deploy\/drive:rw/u);
    assert.match(compose, /SDKWORK_DRIVE_WEBSITE_CACHE_ENABLED/u);
    assert.match(compose, /SDKWORK_DRIVE_WEBSITE_CACHE_ROOT/u);
    assert.match(compose, /SDKWORK_DRIVE_WEBSITE_CACHE_MAX_TOTAL_BYTES/u);
    assert.match(compose, /SDKWORK_DRIVE_WEBSITE_CACHE_MAX_ENTRIES/u);
    assert.match(compose, environmentPattern);
  }

  // Multi-instance bundle compose (DEPLOYMENT_SPEC.md §6) is environment
  // neutral: it must bind the same shared cache mount and cache bounds, and
  // must NOT pin the environment segment (the runtime falls back to
  // SDKWORK_WEBSERVER_ENVIRONMENT per instance).
  const bundleCompose = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.bundle.yml'),
    'utf8',
  );
  assert.match(bundleCompose, /\$\{SDKWORK_DRIVE_CACHE_HOST_PATH:-\/opt\/deploy\/drive\}:\/opt\/deploy\/drive:rw/u);
  assert.match(bundleCompose, /SDKWORK_DRIVE_WEBSITE_CACHE_ENABLED/u);
  assert.match(bundleCompose, /SDKWORK_DRIVE_WEBSITE_CACHE_ROOT/u);
  assert.match(bundleCompose, /SDKWORK_DRIVE_WEBSITE_CACHE_MAX_TOTAL_BYTES/u);
  assert.match(bundleCompose, /SDKWORK_DRIVE_WEBSITE_CACHE_MAX_ENTRIES/u);
  assert.doesNotMatch(bundleCompose, /SDKWORK_DRIVE_WEBSITE_CACHE_ENVIRONMENT/u);

  // Env examples document the deployment inputs for every lifecycle env.
  for (const environment of environments) {
    const envExample = readFileSync(
      path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`),
      'utf8',
    );
    assert.match(envExample, /SDKWORK_DRIVE_WEBSITE_CACHE_ENABLED=/u);
    assert.match(envExample, /SDKWORK_DRIVE_CACHE_HOST_PATH=\/opt\/deploy\/drive/u);
  }
});

test('unified install bundle ships every lifecycle environment (DEPLOYMENT_SPEC §2/§6)', () => {
  // deploy.sh accepts all four environments; the installer copies each
  // env example into the bundle so operators never hand-craft one.
  const deployScript = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'bundle', 'deploy.sh'),
    'utf8',
  );
  assert.match(deployScript, /development\|test\|staging\|production/u);

  // Port-resolution case matrix must cover every accepted environment
  // (DEPLOYMENT_SPEC §6 bundle deploy port-key contract): a missing branch
  // leaves PORT_BASE unset and fails under `set -u` mid-deploy.
  for (const [environment, keys] of Object.entries({
    development: [
      'SDKWORK_WEBSERVER_DEV_HOST_PORT',
      'SDKWORK_WEBSERVER_DEV_IMPORT_HTTP_HOST_PORT',
      'SDKWORK_WEBSERVER_DEV_HTTPS_HOST_PORT',
    ],
    test: [
      'SDKWORK_WEBSERVER_TEST_HOST_PORT',
      'SDKWORK_WEBSERVER_TEST_IMPORT_HTTP_HOST_PORT',
      'SDKWORK_WEBSERVER_TEST_HTTPS_HOST_PORT',
    ],
    staging: [
      'SDKWORK_WEBSERVER_STAGING_HOST_PORT',
      'SDKWORK_WEBSERVER_STAGING_IMPORT_HTTP_HOST_PORT',
      'SDKWORK_WEBSERVER_STAGING_HTTPS_HOST_PORT',
    ],
    production: [
      'SDKWORK_WEBSERVER_PROD_HOST_PORT',
      'SDKWORK_WEBSERVER_PROD_IMPORT_HTTP_HOST_PORT',
      'SDKWORK_WEBSERVER_PROD_HTTPS_HOST_PORT',
    ],
  })) {
    for (const key of keys) {
      assert.ok(
        deployScript.includes(`env_key ${key}`),
        `deploy.sh must resolve ${key} for environment ${environment}`,
      );
    }
  }

  const installer = readFileSync(
    path.join(appRoot, 'scripts', 'docker', 'package-install-bundle.mjs'),
    'utf8',
  );
  assert.match(
    installer,
    /DEFAULT_ENVIRONMENTS = \['development', 'test', 'staging', 'production'\]/u,
  );
  for (const environment of environments) {
    assert.equal(
      existsSync(path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`)),
      true,
      `${environment}.env.example must exist for bundle shipping`,
    );
  }
});

test('every environment CORS allowlist covers registered client origins (WEB_FRAMEWORK_SPEC §12)', () => {
  // Desktop WebView custom schemes and the mini program runtime are first-party
  // client origins: every environment's default SDKWORK_CORS_ALLOWED_ORIGINS
  // must include them so desktop shells and mini programs never fail CORS.
  const registeredOrigins = [
    'app://dsh',
    'app://birdcoder',
    'app://sdkwork',
    'app://dtupay',
    'tauri://localhost',
    'https://servicewechat.com',
  ];

  const composeDefaults = {
    'docker-compose.yml': 4,
    'docker-compose.development.yml': 1,
    'docker-compose.test.yml': 1,
    'docker-compose.staging.yml': 1,
    'docker-compose.production.yml': 1,
  };
  for (const [file, expectedCount] of Object.entries(composeDefaults)) {
    const compose = readFileSync(path.join(appRoot, 'deployments', 'docker', file), 'utf8');
    const defaults = [...compose.matchAll(/SDKWORK_CORS_ALLOWED_ORIGINS:\s*\$\{SDKWORK_CORS_ALLOWED_ORIGINS:-([^}]*)\}/gu)];
    assert.strictEqual(
      defaults.length,
      expectedCount,
      `${file} must carry ${expectedCount} CORS default(s)`,
    );
    for (const [, value] of defaults) {
      for (const origin of registeredOrigins) {
        assert.ok(
          value.split(',').map((entry) => entry.trim()).includes(origin),
          `${file} CORS default must include registered client origin ${origin}`,
        );
      }
    }
  }

  for (const environment of environments) {
    for (const fileName of [`${environment}.env.example`, `${environment}.env`]) {
      const file = path.join(appRoot, 'deployments', 'docker', 'env', fileName);
      const parsed = parseDotEnv(readFileSync(file, 'utf8'));
      const origins = String(parsed.SDKWORK_CORS_ALLOWED_ORIGINS ?? '')
        .split(',')
        .map((entry) => entry.trim())
        .filter(Boolean);
      for (const origin of registeredOrigins) {
        assert.ok(
          origins.includes(origin),
          `${fileName} SDKWORK_CORS_ALLOWED_ORIGINS must include ${origin}`,
        );
      }
    }
  }

  // Entrypoint fallback defaults (no env file at all) also carry the origins.
  const entrypoint = readFileSync(
    path.join(appRoot, 'deployments', 'docker', 'scripts', 'entrypoint-standalone.sh'),
    'utf8',
  );
  for (const origin of registeredOrigins) {
    assert.ok(
      entrypoint.includes(origin),
      `entrypoint default_docker_cors_allowed_origins must include ${origin}`,
    );
  }
});

test('release smoke is hermetic against host runtime config (RUNTIME_DIRECTORY_SPEC §4.1)', () => {
  // Packaged binaries fall back to the host canonical config
  // (/etc/sdkwork/webserver/config.toml) when SDKWORK_WEBSERVER_CONFIG_FILE is
  // unset. On a host that already carries a native install of another
  // environment, that file injects its [database]/[ingress]/[app_roots] values
  // into the process and makes the release verification non-reproducible
  // (observed: schema "sdkwork_ai_test" overriding a dev database URL). The
  // smoke must always pin a self-owned config file.
  const smoke = readFileSync(path.join(appRoot, 'scripts', 'webserver-release-smoke.mjs'), 'utf8');

  assert.match(
    smoke,
    /const HERMETIC_RUNTIME_CONFIG = `/u,
    'release smoke must declare a hermetic runtime config template',
  );
  assert.match(
    smoke,
    /SDKWORK_WEBSERVER_CONFIG_FILE: runtimeConfigFile/u,
    'standaloneManagementEnv must pin SDKWORK_WEBSERVER_CONFIG_FILE',
  );
  assert.match(
    smoke,
    /function writeHermeticRuntimeConfig\(/u,
    'release smoke must write its own runtime config file',
  );
  assert.match(
    smoke,
    /function hermeticEnv\(/u,
    'release smoke must build a hermetic env for every packaged invocation',
  );

  // The hermetic config declares the profile only: any [database] section would
  // re-introduce exactly the cross-environment override it exists to prevent.
  const template = smoke.slice(
    smoke.indexOf('const HERMETIC_RUNTIME_CONFIG = `') + 'const HERMETIC_RUNTIME_CONFIG = `'.length,
  );
  const body = template.slice(0, template.indexOf('`;'));
  assert.doesNotMatch(body, /^\s*\[database\]/mu, 'hermetic config must not declare [database]');
  assert.doesNotMatch(body, /^\s*\[app_roots\]/mu, 'hermetic config must not declare [app_roots]');
  assert.doesNotMatch(body, /^\s*\[ingress\]/mu, 'hermetic config must not declare [ingress]');
  assert.match(body, /^\[profile\]/mu, 'hermetic config must declare [profile]');
  assert.match(body, /environment = "production"/u, 'hermetic config pins the production profile');
});
