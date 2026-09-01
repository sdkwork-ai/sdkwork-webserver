#!/usr/bin/env node

// Single compose entry point for every sdkwork-webserver Docker stack
// (PNPM_SCRIPT_SPEC.md §4.2 / DEPLOYMENT_SPEC.md §7: one reusable driver,
// thin wrappers only). Both pnpm scripts and shell deploy scripts route
// through this file; layouts differ only in the compose file set:
//
// - embedded (default): deployments/docker/docker-compose.yml with compose
//   profiles (development/test/production) plus built-in postgres/redis and
//   the embedded platform-api-gateway overlay.
// - external: deployments/docker/docker-compose.<environment>.yml (one file
//   per lifecycle environment) with external PostgreSQL/Redis and the
//   standalone platform-api-gateway overlay.

import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { parseDotEnv } from '../../../sdkwork-specs/tools/postgres/postgres-config.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const DOCKER_ROOT = path.join(REPO_ROOT, 'deployments', 'docker');
const COMPOSE_FILE = path.join(DOCKER_ROOT, 'docker-compose.yml');
const COMPOSE_EXTERNAL_FILE = path.join(DOCKER_ROOT, 'docker-compose.external.yml');
const COMPOSE_PLATFORM_GATEWAY_FILE = path.join(
  DOCKER_ROOT,
  'docker-compose.platform-api-gateway.yml',
);
const COMPOSE_PLATFORM_GATEWAY_EMBEDDED_FILE = path.join(
  DOCKER_ROOT,
  'docker-compose.platform-api-gateway.embedded.yml',
);
const COMPOSE_PLATFORM_GATEWAY_ATTACH_FILE = path.join(
  DOCKER_ROOT,
  'docker-compose.platform-api-gateway-attach.yml',
);
const COMPOSE_PLATFORM_GATEWAY_ATTACH_EMBEDDED_FILE = path.join(
  DOCKER_ROOT,
  'docker-compose.platform-api-gateway-attach.embedded.yml',
);

const VALID_ENVIRONMENTS = ['development', 'test', 'production'];
// staging is a first-class deployment environment (DEPLOYMENT_SPEC §2,
// "production-like rehearsal") with its own compose file and env file, but it
// is excluded from `all`/`--shared` sweeps: those target the local
// dev/test/production trio on one host.
const DEPLOYABLE_ENVIRONMENTS = [...VALID_ENVIRONMENTS, 'staging'];
const VALID_LAYOUTS = ['embedded', 'external'];

function parseArgs(argv) {
  const settings = {
    command: 'up',
    environment: 'development',
    layout: 'embedded',
    detach: true,
    external: false,
    shared: false,
    platformApiGatewayDocker: false,
    validate: false,
    envFile: undefined,
    dryRun: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === 'up' || argument === 'down' || argument === 'ps' || argument === 'logs' || argument === 'pull') {
      settings.command = argument;
    } else if (argument === '--environment') {
      settings.environment = argv[++index];
    } else if (argument === '--layout') {
      settings.layout = argv[++index];
      if (!VALID_LAYOUTS.includes(settings.layout)) {
        throw new Error(`--layout must be ${VALID_LAYOUTS.join(', ')}`);
      }
    } else if (argument === '--external') {
      settings.external = true;
    } else if (argument === '--shared') {
      settings.shared = true;
    } else if (argument === '--platform-api-gateway-docker') {
      settings.platformApiGatewayDocker = true;
    } else if (argument === '--validate') {
      settings.validate = true;
    } else if (argument === '--env-file') {
      settings.envFile = path.resolve(argv[++index]);
    } else if (argument === '--no-detach') {
      settings.detach = false;
    } else if (argument === '--dry-run') {
      settings.dryRun = true;
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  return settings;
}

function printHelp() {
  console.log(`Usage: node scripts/docker/compose.mjs <up|down|ps|logs|pull> [options]

Options:
  --environment <development|test|staging|production|all>   Default: development
                                                  (all sweeps development, test,
                                                  and production)
  --layout <embedded|external>                    embedded: docker-compose.yml
                                                  with profiles; external: one
                                                  docker-compose.<env>.yml per
                                                  environment. Default: embedded
  --external                                      External PostgreSQL/Redis mode (embedded layout)
  --shared                                        All profiles in one project (embedded only)
  --platform-api-gateway-docker                   Add platform-api-gateway sibling container overlay
  --validate                                      Validate env before compose up
  --env-file <path>                               Override env file path
  --no-detach                                     Foreground mode for "up"
  --dry-run                                       Print the resolved compose command
  --help, -h                                      Show this help`);
}

function ensureEnvFile(environment) {
  const envFile = path.join(DOCKER_ROOT, 'env', `${environment}.env`);
  const exampleFile = path.join(DOCKER_ROOT, 'env', `${environment}.env.example`);
  if (!existsSync(envFile)) {
    if (!existsSync(exampleFile)) {
      throw new Error(`missing env template: ${exampleFile}`);
    }
    copyFileSync(exampleFile, envFile);
    console.log(`created ${envFile} from example`);
  }
  return envFile;
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    stdio: 'inherit',
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message ?? `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
}

function usesPlatformGatewayAttachOverlay(envFile) {
  const env = parseDotEnv(readFileSync(envFile, 'utf8'));
  return String(env.SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK ?? '').trim().length > 0;
}

function usesPlatformGatewayDockerOverlay(settings, envFile) {
  if (usesPlatformGatewayAttachOverlay(envFile)) {
    return false;
  }
  if (settings.platformApiGatewayDocker) {
    return true;
  }
  const env = parseDotEnv(readFileSync(envFile, 'utf8'));
  return String(env.SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT ?? 'docker').trim() === 'docker';
}

/** Compose file set for one layout + environment. */
function composeFiles(settings, envFile, environment) {
  const files = [];
  if (settings.layout === 'external') {
    files.push(path.join(DOCKER_ROOT, `docker-compose.${environment}.yml`));
    if (usesPlatformGatewayAttachOverlay(envFile)) {
      files.push(COMPOSE_PLATFORM_GATEWAY_ATTACH_FILE);
    } else if (usesPlatformGatewayDockerOverlay(settings, envFile)) {
      files.push(COMPOSE_PLATFORM_GATEWAY_FILE);
    }
    return files;
  }
  files.push(COMPOSE_FILE);
  if (settings.external) {
    files.push(COMPOSE_EXTERNAL_FILE);
  }
  if (usesPlatformGatewayAttachOverlay(envFile)) {
    files.push(COMPOSE_PLATFORM_GATEWAY_ATTACH_EMBEDDED_FILE);
  } else if (usesPlatformGatewayDockerOverlay(settings, envFile)) {
    // compose.mjs drives docker-compose.yml (embedded/profiled); use the
    // embedded overlay. Standalone per-env deploys use the external layout.
    files.push(
      existsSync(COMPOSE_PLATFORM_GATEWAY_EMBEDDED_FILE)
        ? COMPOSE_PLATFORM_GATEWAY_EMBEDDED_FILE
        : COMPOSE_PLATFORM_GATEWAY_FILE,
    );
  }
  return files;
}

function composeArgs(settings, envFile, project, profiles, environment) {
  const args = [
    'compose',
    '--env-file',
    envFile,
    '-p',
    project,
  ];
  for (const file of composeFiles(settings, envFile, environment)) {
    args.push('-f', file);
  }
  // External layout compose files carry no profiles; embedded files select
  // services through them.
  if (settings.layout === 'embedded') {
    for (const profile of profiles) {
      args.push('--profile', profile);
    }
  }
  return args;
}

function validateEnvironmentFile(settings, envFile, mode) {
  run('node', [
    path.join(REPO_ROOT, 'scripts', 'docker', 'validate-docker-deployment.mjs'),
    '--env-file',
    envFile,
    '--mode',
    mode,
  ]);
}

function runCompose(settings, envFile, project, profiles, environment) {
  const args = composeArgs(settings, envFile, project, profiles, environment);
  args.push(settings.command);
  if (settings.command === 'up' && settings.detach) {
    args.push('-d');
  }
  console.log(`docker ${args.join(' ')}`);
  if (!settings.dryRun) {
    run('docker', args);
  }
}

function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    return;
  }
  if (settings.environment !== 'all' && !DEPLOYABLE_ENVIRONMENTS.includes(settings.environment)) {
    throw new Error(`--environment must be ${DEPLOYABLE_ENVIRONMENTS.join(', ')} or all`);
  }
  if (settings.shared && (settings.external || settings.environment !== 'all' || settings.layout !== 'embedded')) {
    throw new Error('--shared requires embedded mode with --environment all');
  }

  const targets = settings.environment === 'all'
    ? VALID_ENVIRONMENTS
    : [settings.environment];

  if (settings.shared) {
    const envFile = settings.envFile ?? ensureEnvFile('development');
    if (settings.validate && settings.command === 'up') {
      for (const environment of VALID_ENVIRONMENTS) {
        validateEnvironmentFile(settings, settings.envFile ?? ensureEnvFile(environment), 'embedded');
      }
    }
    runCompose(settings, envFile, 'sdkwork-webserver-shared', VALID_ENVIRONMENTS, 'development');
    return;
  }

  for (const environment of targets) {
    const envFile = settings.envFile ?? ensureEnvFile(environment);
    if (settings.validate && settings.command === 'up') {
      validateEnvironmentFile(
        settings,
        envFile,
        // External layout stacks run against host-managed PostgreSQL/Redis.
        settings.layout === 'external' || settings.external ? 'external' : 'embedded',
      );
    }
    runCompose(settings, envFile, `sdkwork-webserver-${environment}`, [environment], environment);
  }
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
