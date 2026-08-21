#!/usr/bin/env node

import { spawn } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';

import { ensureTrackedBuildSources } from './lib/build-source-integrity.mjs';
import {
  canonicalizeWorkspaceDatabaseEnv,
  IAM_APPLICATION_BOOTSTRAP_ENV,
  REPO_ROOT,
  VALID_DEPLOYMENT_PROFILES,
  VALID_ENVIRONMENTS,
  loadProfile,
  mergeRuntimeEnv,
  resolveIamDevEnv,
} from './lib/webserver-topology.mjs';

const CRITICAL_SOURCE_FILES = [
  '.env.postgres.example',
  'Cargo.toml',
  'sdkwork.app.config.json',
  'apps/sdkwork-webserver-pc/sdkwork.app.config.json',
  'crates/sdkwork-api-webserver-standalone-gateway/Cargo.toml',
  'crates/sdkwork-api-webserver-standalone-gateway/src/main.rs',
  'scripts/lib/webserver-topology.mjs',
];

function parseArgs(argv) {
  const settings = {
    database: 'postgres',
    deploymentProfile: 'standalone',
    devEnvFile: '.env.postgres',
    dryRun: false,
    environment: 'development',
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--database') {
      settings.database = argv[++index];
    } else if (argument === '--deployment-profile') {
      settings.deploymentProfile = argv[++index];
    } else if (argument === '--environment') {
      settings.environment = argv[++index];
    } else if (argument === '--dev-env-file') {
      settings.devEnvFile = argv[++index];
    } else if (argument === '--dry-run') {
      settings.dryRun = true;
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }

  if (settings.database !== 'postgres') {
    throw new Error('--database must be postgres');
  }
  if (!VALID_DEPLOYMENT_PROFILES.includes(settings.deploymentProfile)) {
    throw new Error(
      `--deployment-profile must be ${VALID_DEPLOYMENT_PROFILES.join(' or ')}`,
    );
  }
  if (!VALID_ENVIRONMENTS.includes(settings.environment)) {
    throw new Error(`--environment must be ${VALID_ENVIRONMENTS.join(' or ')}`);
  }
  return settings;
}

function printHelp() {
  console.log(`Usage: node scripts/webserver-dev.mjs [options]

Options:
  --database <postgres>                     Default: postgres
  --deployment-profile <standalone|cloud>  Default: standalone
  --environment <environment>              Default: development
  --dev-env-file <path>                    Default: .env.postgres
  --dry-run                                Print the resolved plan
  --help, -h                               Show this help`);
}

function ensureCriticalSources() {
  ensureTrackedBuildSources({
    repoRoot: REPO_ROOT,
    relativePaths: CRITICAL_SOURCE_FILES,
  });
}

function buildRuntimeEnv(settings) {
  const profileId = `${settings.deploymentProfile}.${settings.environment}`;
  const profileEnv = loadProfile(profileId);
  const baseEnv = mergeRuntimeEnv(process.env, profileEnv);
  const iamEnv = canonicalizeWorkspaceDatabaseEnv(
    resolveIamDevEnv(baseEnv, { postgresEnvFile: settings.devEnvFile }),
  );
  const databaseSource = path.relative(REPO_ROOT, path.resolve(REPO_ROOT, settings.devEnvFile));
  const autoMigrate = settings.environment === 'development' ? 'true' : 'false';

  return {
    databaseSource,
    env: {
      ...iamEnv,
      ...IAM_APPLICATION_BOOTSTRAP_ENV,
      SDKWORK_DEPLOYMENT_PROFILE: settings.deploymentProfile,
      SDKWORK_ENVIRONMENT: settings.environment,
      SDKWORK_DATABASE_AUTO_MIGRATE:
        iamEnv.SDKWORK_DATABASE_AUTO_MIGRATE ?? autoMigrate,
      SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE: settings.deploymentProfile,
      SDKWORK_WEBSERVER_ENVIRONMENT: settings.environment,
      SDKWORK_WEBSERVER_RUNTIME_TARGET: 'server',
      SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID: process.env.SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID ?? '0',
    },
  };
}

async function run() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    return;
  }

  ensureCriticalSources();
  const runtime = buildRuntimeEnv(settings);
  console.log(
    `[sdkwork-web] environment=${settings.environment} deploymentProfile=${settings.deploymentProfile} runtimeTarget=server database=${settings.database}`,
  );
  console.log(`[sdkwork-web] databaseSource=${runtime.databaseSource}`);
  console.log(
    `[sdkwork-web] managementUrl=${runtime.env.SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL}`,
  );

  const command = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
  const args = [
    'run',
    '-p',
    'sdkwork-api-webserver-standalone-gateway',
    '--bin',
    'sdkwork-api-webserver-standalone-gateway',
  ];
  if (settings.dryRun) {
    console.log(`[sdkwork-web] command=${command} ${args.join(' ')}`);
    return;
  }

  const child = spawn(command, args, {
    cwd: REPO_ROOT,
    env: runtime.env,
    stdio: 'inherit',
    windowsHide: true,
  });

  await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0 || signal === 'SIGINT' || signal === 'SIGTERM') {
        resolve();
      } else {
        reject(new Error(`sdkwork-api-webserver-standalone-gateway exited with code ${code ?? 1}`));
      }
    });
  });
}

run().catch((error) => {
  process.stderr.write(`[sdkwork-web] ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
