#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import {
  buildEnvironmentAlias,
  buildModuleBrowser,
  DEPLOYMENT_ENVIRONMENT_ALIASES,
  resolveSpaceCheckoutRoot,
} from './build-module-browser.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const DOCKER_ROOT = path.join(REPO_ROOT, 'deployments', 'docker');

const VALID_DEPLOYMENT_ENVIRONMENTS = ['development', 'test', 'production', 'staging'];
const STANDALONE_COMPOSE = {
  development: path.join(DOCKER_ROOT, 'docker-compose.development.yml'),
  production: path.join(DOCKER_ROOT, 'docker-compose.production.yml'),
  staging: path.join(DOCKER_ROOT, 'docker-compose.staging.yml'),
  test: path.join(DOCKER_ROOT, 'docker-compose.test.yml'),
};

function parseSettings(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      architecture: { type: 'string', default: 'all' },
      'deployment-environment': { type: 'string', default: 'development' },
      'deployment-profile': { type: 'string', default: 'standalone' },
      'dry-run': { type: 'boolean', default: false },
      environment: { type: 'string' },
      external: { type: 'boolean', default: false },
      help: { type: 'boolean', short: 'h' },
      'host-only': { type: 'boolean', default: false },
      'in-container': { type: 'boolean', default: false },
      module: { type: 'string' },
      reload: { type: 'boolean', default: false },
    },
  });
  if (values.help) {
    console.log(`Usage: node scripts/docker/run-module-browser-build.mjs --module <sdkwork-module> [options]

Rebuild one sibling module's Adaptive Web PC/H5 bundles for a lifecycle mode and
optionally reload the matching webserver container static roots.

Options:
  --module <sdkwork-module>                 Required module repository name
  --architecture pc|h5|all                  Default: all owned surfaces
  --environment dev|test|staging|prod       Vite/build output alias; defaults from --deployment-environment
  --deployment-environment development|test|staging|production
                                            Target Docker lifecycle cluster (default: development)
  --deployment-profile standalone|cloud     Default: standalone
  --host-only                               Build on the host checkout only
  --in-container                            Force build inside the webserver container toolchain
  --external                                Use external PostgreSQL/Redis compose override
  --reload                                  Restart the target webserver service after build
  --dry-run                                 Print the resolved plan only`);
    process.exit(0);
  }
  if (!values.module) {
    throw new Error('--module is required');
  }
  if (!VALID_DEPLOYMENT_ENVIRONMENTS.includes(values['deployment-environment'])) {
    throw new Error(`--deployment-environment must be one of ${VALID_DEPLOYMENT_ENVIRONMENTS.join(', ')}`);
  }
  return {
    architecture: values.architecture,
    deploymentEnvironment: values['deployment-environment'],
    deploymentProfile: values['deployment-profile'],
    dryRun: values['dry-run'],
    environment: values.environment,
    external: values.external,
    hostOnly: values['host-only'],
    inContainer: values['in-container'],
    module: values.module,
    reload: values.reload,
  };
}

function readEnvFile(environment) {
  const envFile = path.join(DOCKER_ROOT, 'env', `${environment}.env`);
  if (!existsSync(envFile)) {
    return {};
  }
  return Object.fromEntries(
    readFileSync(envFile, 'utf8')
      .split(/\r?\n/u)
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith('#') && line.includes('='))
      .map((line) => {
        const index = line.indexOf('=');
        return [line.slice(0, index), line.slice(index + 1)];
      }),
  );
}

function resolveCheckoutHostPath(settings) {
  const env = readEnvFile(settings.deploymentEnvironment);
  return env.SDKWORK_SPACE_CHECKOUT_HOST_PATH
    ?? env.SDKWORK_SPACE_HOST_PATH
    ?? process.env.SDKWORK_SPACE_CHECKOUT_HOST_PATH
    ?? process.env.SDKWORK_SPACE_HOST_PATH
    ?? (process.platform === 'win32'
      ? path.resolve(REPO_ROOT, '..')
      : '/opt/deploy/sdkwork-space');
}

function resolveServiceName(deploymentEnvironment, composeLayout) {
  if (composeLayout === 'standalone') {
    return 'webserver';
  }
  return `webserver-${deploymentEnvironment}`;
}

function resolveComposeFiles(settings) {
  const standaloneFile = STANDALONE_COMPOSE[settings.deploymentEnvironment];
  if (existsSync(standaloneFile)) {
    return {
      composeLayout: 'standalone',
      files: settings.external
        ? [standaloneFile, path.join(DOCKER_ROOT, 'docker-compose.external.yml')]
        : [standaloneFile],
      project: `sdkwork-webserver-${settings.deploymentEnvironment}`,
    };
  }
  const files = [path.join(DOCKER_ROOT, 'docker-compose.yml')];
  if (settings.external) {
    files.push(path.join(DOCKER_ROOT, 'docker-compose.external.yml'));
  }
  return {
    composeLayout: 'unified',
    files,
    project: `sdkwork-webserver-${settings.deploymentEnvironment}`,
  };
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    env: options.env ?? process.env,
    stdio: options.stdio ?? 'inherit',
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} failed with exit ${result.status ?? 'unknown'}`);
  }
  return result;
}

function buildOnHost(settings, plan) {
  if (settings.dryRun) {
    console.log('[dry-run] host build plan:', JSON.stringify(plan, null, 2));
    return plan;
  }
  return buildModuleBrowser({
    architecture: settings.architecture,
    deploymentEnvironment: settings.deploymentEnvironment,
    deploymentProfile: settings.deploymentProfile,
    environment: settings.environment,
    module: settings.module,
  });
}

function buildInContainer(settings, plan) {
  const checkoutHostPath = resolveCheckoutHostPath(settings);
  const compose = resolveComposeFiles(settings);
  const service = resolveServiceName(settings.deploymentEnvironment, compose.composeLayout);
  const buildEnvironment = buildEnvironmentAlias(
    settings.deploymentEnvironment,
    settings.environment,
  );
  const envFile = path.join(DOCKER_ROOT, 'env', `${settings.deploymentEnvironment}.env`);
  const args = [
    'compose',
    '--project-name',
    compose.project,
  ];
  if (existsSync(envFile)) {
    args.push('--env-file', envFile);
  }
  for (const file of compose.files) {
    args.push('-f', file);
  }
  args.push(
    'run',
    '--rm',
    '--no-deps',
    '-v',
    `${checkoutHostPath}:/opt/deploy/sdkwork-space:rw`,
    '-v',
    `${path.join(REPO_ROOT, 'scripts', 'docker', 'build-module-browser.mjs')}:/app/scripts/docker/build-module-browser.mjs:ro`,
    service,
    'build-browser',
    '--module',
    settings.module,
    '--architecture',
    settings.architecture,
    '--environment',
    buildEnvironment,
    '--deployment-profile',
    settings.deploymentProfile,
  );

  console.log(`docker ${args.join(' ')}`);
  if (settings.dryRun) {
    return plan;
  }
  run('docker', args, { cwd: DOCKER_ROOT });
  return plan;
}

function reloadContainer(settings) {
  const compose = resolveComposeFiles(settings);
  const service = resolveServiceName(settings.deploymentEnvironment, compose.composeLayout);
  const envFile = path.join(DOCKER_ROOT, 'env', `${settings.deploymentEnvironment}.env`);
  const args = [
    'compose',
    '--project-name',
    compose.project,
  ];
  if (existsSync(envFile)) {
    args.push('--env-file', envFile);
  }
  for (const file of compose.files) {
    args.push('-f', file);
  }
  args.push('restart', service);
  console.log(`docker ${args.join(' ')}`);
  if (settings.dryRun) {
    return;
  }
  run('docker', args, { cwd: DOCKER_ROOT });
}

export function resolveModuleBrowserBuildPlan(settings, options = {}) {
  const spaceCheckoutRoot = resolveSpaceCheckoutRoot(options.spaceCheckoutRoot);
  const plan = buildModuleBrowser({
    architecture: settings.architecture,
    deploymentEnvironment: settings.deploymentEnvironment,
    deploymentProfile: settings.deploymentProfile,
    dryRun: true,
    environment: settings.environment,
    module: settings.module,
    spaceCheckoutRoot,
  });
  const mode = settings.inContainer
    ? 'container'
    : settings.hostOnly
      ? 'host'
      : options.defaultMode ?? 'host';
  return {
    ...plan,
    checkoutHostPath: resolveCheckoutHostPath(settings),
    compose: resolveComposeFiles(settings),
    mode,
    reload: settings.reload,
    service: resolveServiceName(settings.deploymentEnvironment, resolveComposeFiles(settings).composeLayout),
  };
}

function main() {
  const settings = parseSettings(process.argv.slice(2));
  const plan = resolveModuleBrowserBuildPlan(settings, {
    defaultMode: process.env.SDKWORK_MODULE_BROWSER_BUILD_IN_CONTAINER === 'true' ? 'container' : 'host',
  });

  if (plan.mode === 'container') {
    buildInContainer(settings, plan);
  } else {
    buildOnHost(settings, plan);
  }

  if (settings.reload) {
    reloadContainer(settings);
  }

  if (!settings.dryRun) {
    console.log(JSON.stringify({
      checkoutHostPath: plan.checkoutHostPath,
      mode: plan.mode,
      module: plan.module,
      plans: plan.plans,
      reloaded: settings.reload,
      service: plan.service,
    }, null, 2));
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}