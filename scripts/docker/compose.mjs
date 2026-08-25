#!/usr/bin/env node

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

const VALID_ENVIRONMENTS = ['development', 'test', 'production'];

function parseArgs(argv) {
  const settings = {
    command: 'up',
    environment: 'development',
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
    if (argument === 'up' || argument === 'down' || argument === 'ps' || argument === 'logs') {
      settings.command = argument;
    } else if (argument === '--environment') {
      settings.environment = argv[++index];
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
  console.log(`Usage: node scripts/docker/compose.mjs <up|down|ps|logs> [options]

Options:
  --environment <development|test|production|all>   Default: development
  --external                                      External PostgreSQL/Redis mode
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

function composeArgs(settings, envFile, project, profiles) {
  const args = [
    'compose',
    '--env-file',
    envFile,
    '-p',
    project,
    '-f',
    COMPOSE_FILE,
  ];
  if (settings.external) {
    args.push('-f', COMPOSE_EXTERNAL_FILE);
  }
  if (usesPlatformGatewayAttachOverlay(envFile)) {
    args.push(
      '-f',
      path.join(DOCKER_ROOT, 'docker-compose.platform-api-gateway-attach.embedded.yml'),
    );
  } else if (usesPlatformGatewayDockerOverlay(settings, envFile)) {
    // compose.mjs drives docker-compose.yml (embedded/profiled); use the
    // embedded overlay. Standalone per-env deploys use deploy-docker-environment.sh
    // with docker-compose.platform-api-gateway.yml.
    const embedded = COMPOSE_PLATFORM_GATEWAY_EMBEDDED_FILE;
    args.push('-f', existsSync(embedded) ? embedded : COMPOSE_PLATFORM_GATEWAY_FILE);
  }
  for (const profile of profiles) {
    args.push('--profile', profile);
  }
  return args;
}

function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    return;
  }
  if (settings.environment !== 'all' && !VALID_ENVIRONMENTS.includes(settings.environment)) {
    throw new Error(`--environment must be ${VALID_ENVIRONMENTS.join(', ')} or all`);
  }

  const targets = settings.environment === 'all'
    ? VALID_ENVIRONMENTS
    : [settings.environment];

  if (settings.shared && (settings.external || settings.environment !== 'all')) {
    throw new Error('--shared requires embedded mode with --environment all');
  }

  if (settings.shared) {
    const envFile = settings.envFile ?? ensureEnvFile('development');
    if (settings.validate && settings.command === 'up') {
      for (const environment of VALID_ENVIRONMENTS) {
        run('node', [
          path.join(REPO_ROOT, 'scripts', 'docker', 'validate-docker-deployment.mjs'),
          '--env-file',
          settings.envFile ?? ensureEnvFile(environment),
          '--mode',
          'embedded',
        ]);
      }
    }
    const args = composeArgs(settings, envFile, 'sdkwork-webserver-shared', VALID_ENVIRONMENTS);
    args.push(settings.command);
    if (settings.command === 'up' && settings.detach) {
      args.push('-d');
    }
    console.log(`docker ${args.join(' ')}`);
    if (!settings.dryRun) {
      run('docker', args);
    }
    return;
  }

  for (const environment of targets) {
    const envFile = settings.envFile ?? ensureEnvFile(environment);
    if (settings.validate && settings.command === 'up') {
      run('node', [
        path.join(REPO_ROOT, 'scripts', 'docker', 'validate-docker-deployment.mjs'),
        '--env-file',
        envFile,
        '--mode',
        settings.external ? 'external' : 'embedded',
      ]);
    }
    const args = composeArgs(
      settings,
      envFile,
      `sdkwork-webserver-${environment}`,
      [environment],
    );
    args.push(settings.command);
    if (settings.command === 'up' && settings.detach) {
      args.push('-d');
    }
    console.log(`docker ${args.join(' ')}`);
    if (!settings.dryRun) {
      run('docker', args);
    }
  }
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
