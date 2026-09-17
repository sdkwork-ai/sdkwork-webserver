#!/usr/bin/env node

// Reset the bootstrap administrator password of one Web Server environment.
//
// Mirrors `sdkwork-cloudrouter`'s `scripts/reset-admin-account.mjs`: this
// driver resolves the target environment, then runs the gateway's `reset-admin`
// operation with the new secret supplied through the environment, so the
// password never appears in the process argument list.
//
// `pnpm admin:reset:dev` targets the workspace PostgreSQL development profile
// (`.env.postgres`); `pnpm admin:reset:<lifecycle>` targets an installed
// deployment through the canonical runtime configuration file. Every script
// requires the caller to pass `--password`, or to export
// `SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD`.

import { spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPOSITORY_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const GATEWAY_PACKAGE = 'sdkwork-api-webserver-standalone-gateway';
const GATEWAY_BINARY = 'sdkwork-api-webserver-standalone-gateway';
const PASSWORD_ENV = 'SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD';
const DEFAULT_DEV_ENV_FILE = '.env.postgres';
const FALLBACK_DEV_ENV_FILE = '.env.postgres.example';
const MAX_DEV_ENV_BYTES = 256 * 1024;

// ENVIRONMENT_SPEC lifecycle set. `dev` / `prod` are accepted aliases that the
// workspace profile vocabulary maps to the lifecycle name.
const LIFECYCLE_ALIASES = new Map([
  ['dev', 'development'],
  ['development', 'development'],
  ['test', 'test'],
  ['staging', 'staging'],
  ['demo', 'demo'],
  ['prod', 'production'],
  ['production', 'production'],
]);

export function parseResetAdminArgs(argv = []) {
  const settings = {
    help: false,
    dryRun: false,
    release: false,
    mode: 'dev',
    environment: null,
    username: null,
    tenantId: null,
    password: null,
    configFile: null,
    devEnvFile: null,
    databaseUrl: null,
    databaseMaxConnections: null,
    gatewayBinary: null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') continue;
    if (arg === '--help' || arg === '-h') {
      settings.help = true;
    } else if (arg === '--dry-run') {
      settings.dryRun = true;
    } else if (arg === '--release') {
      settings.release = true;
    } else if (arg === '--mode') {
      settings.mode = normalizeMode(valueOf(argv, (index += 1), arg));
    } else if (arg === '--environment') {
      settings.environment = normalizeEnvironment(valueOf(argv, (index += 1), arg));
    } else if (arg === '--username') {
      settings.username = trimOrNull(valueOf(argv, (index += 1), arg));
    } else if (arg === '--tenant-id') {
      settings.tenantId = trimOrNull(valueOf(argv, (index += 1), arg));
    } else if (arg === '--password') {
      settings.password = valueOf(argv, (index += 1), arg);
    } else if (arg === '--config-file') {
      settings.configFile = valueOf(argv, (index += 1), arg);
    } else if (arg === '--dev-env-file') {
      settings.devEnvFile = valueOf(argv, (index += 1), arg);
    } else if (arg === '--database-url') {
      settings.databaseUrl = valueOf(argv, (index += 1), arg);
    } else if (arg === '--database-max-connections') {
      settings.databaseMaxConnections = valueOf(argv, (index += 1), arg);
    } else if (arg === '--gateway-binary') {
      settings.gatewayBinary = valueOf(argv, (index += 1), arg);
    } else {
      throw new Error(`unsupported admin reset option: ${arg}`);
    }
  }

  settings.environment ??= settings.mode === 'release' ? 'production' : 'development';
  return settings;
}

function valueOf(argv, index, flag) {
  const value = argv[index];
  if (value === undefined || value.startsWith('--')) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function trimOrNull(value) {
  const trimmed = String(value ?? '').trim();
  return trimmed === '' ? null : trimmed;
}

function normalizeMode(value) {
  const mode = String(value ?? '').trim().toLowerCase();
  if (mode !== 'dev' && mode !== 'release') {
    throw new Error('--mode must be dev or release');
  }
  return mode;
}

export function normalizeEnvironment(value) {
  const key = String(value ?? '').trim().toLowerCase();
  const environment = LIFECYCLE_ALIASES.get(key);
  if (!environment) {
    throw new Error(
      `--environment must be one of ${[...new Set(LIFECYCLE_ALIASES.values())].join(', ')}`,
    );
  }
  return environment;
}

/**
 * Minimal dotenv reader for the workspace PostgreSQL profile. Values may be
 * quoted; `export ` prefixes and `#` comments are ignored; a malformed line
 * fails closed instead of silently dropping a database setting.
 */
export function parseDevEnvFile(text, source) {
  const values = {};
  for (const [offset, rawLine] of text.split(/\r?\n/u).entries()) {
    const line = rawLine.trim();
    if (line === '' || line.startsWith('#')) continue;
    const assignment = line.startsWith('export ') ? line.slice('export '.length) : line;
    const separator = assignment.indexOf('=');
    if (separator <= 0) {
      throw new Error(`${source}:${offset + 1}: expected KEY=VALUE`);
    }
    const key = assignment.slice(0, separator).trim();
    let value = assignment.slice(separator + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"') && value.length >= 2) ||
      (value.startsWith("'") && value.endsWith("'") && value.length >= 2)
    ) {
      value = value.slice(1, -1);
    }
    values[key] = value;
  }
  return values;
}

function resolveDevEnvFile(settings, root) {
  if (settings.devEnvFile) {
    const explicit = path.resolve(root, settings.devEnvFile);
    if (!existsSync(explicit)) {
      throw new Error(`--dev-env-file does not exist: ${explicit}`);
    }
    return explicit;
  }
  for (const candidate of [DEFAULT_DEV_ENV_FILE, FALLBACK_DEV_ENV_FILE]) {
    const resolved = path.join(root, candidate);
    if (existsSync(resolved)) return resolved;
  }
  throw new Error(
    `no development database profile found: expected ${DEFAULT_DEV_ENV_FILE} (or ` +
      `${FALLBACK_DEV_ENV_FILE}) in ${root}; pass --dev-env-file <path>`,
  );
}

function readDevEnvFile(devEnvFile) {
  const text = readFileSync(devEnvFile, 'utf8');
  if (text.length > MAX_DEV_ENV_BYTES) {
    throw new Error(`development database profile exceeds ${MAX_DEV_ENV_BYTES} bytes`);
  }
  return parseDevEnvFile(text, devEnvFile);
}

/**
 * Build the executable plan for one environment. Exported so the composition
 * can be exercised without touching a database.
 */
export function createResetAdminPlan({
  settings = parseResetAdminArgs([]),
  root = REPOSITORY_ROOT,
  env = process.env,
  platform = process.platform,
} = {}) {
  const password = settings.password ?? env[PASSWORD_ENV] ?? '';
  if (password.trim() === '') {
    throw new Error(
      `the admin reset password is required: pass --password or set ${PASSWORD_ENV}`,
    );
  }

  // The Web Server is standalone-only (SDKWORK_WEBSERVER_SPEC.md §17.4); the
  // profile is pinned so a stray SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE in the
  // caller's shell cannot retarget the command.
  const stepEnv = {
    ...env,
    SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE: 'standalone',
    SDKWORK_WEBSERVER_ENVIRONMENT: settings.environment,
    [PASSWORD_ENV]: password,
  };

  const details = { devEnvFile: null, runtimeConfigFile: null };

  if (settings.mode === 'dev') {
    const devEnvFile = resolveDevEnvFile(settings, root);
    const devEnv = readDevEnvFile(devEnvFile);
    Object.assign(stepEnv, devEnv);
    details.devEnvFile = devEnvFile;
  } else if (settings.configFile) {
    const configFile = path.resolve(root, settings.configFile);
    if (!existsSync(configFile)) {
      throw new Error(`--config-file does not exist: ${configFile}`);
    }
    stepEnv.SDKWORK_WEBSERVER_CONFIG_FILE = configFile;
    details.runtimeConfigFile = configFile;
  }

  if (settings.databaseUrl) stepEnv.SDKWORK_DATABASE_URL = settings.databaseUrl;
  if (settings.databaseMaxConnections) {
    stepEnv.SDKWORK_DATABASE_MAX_CONNECTIONS = settings.databaseMaxConnections;
  }

  const resetArguments = ['reset-admin'];
  if (settings.username) resetArguments.push('--username', settings.username);
  if (settings.tenantId) resetArguments.push('--tenant-id', settings.tenantId);

  const step = settings.gatewayBinary
    ? {
        command: path.resolve(root, settings.gatewayBinary),
        args: resetArguments,
      }
    : {
        command: platform === 'win32' ? 'cargo.exe' : 'cargo',
        args: [
          'run',
          ...(settings.release ? ['--release'] : []),
          '-p',
          GATEWAY_PACKAGE,
          '--bin',
          GATEWAY_BINARY,
          '--',
          ...resetArguments,
        ],
      };

  return {
    mode: settings.mode,
    environment: settings.environment,
    details,
    steps: [
      {
        name: 'reset-admin',
        ...step,
        cwd: root,
        env: stepEnv,
        shell: false,
        windowsHide: platform === 'win32',
      },
    ],
  };
}

function formatCommand(step) {
  return `${step.command} ${step.args.join(' ')}`;
}

// Never echo the secret, not even in --dry-run output: only the resolved
// SDKWORK database/runtime keys are shown, and every value that could carry a
// credential is redacted.
function formatEnvironment(stepEnv) {
  return Object.keys(stepEnv)
    .filter(
      (key) =>
        key.startsWith('SDKWORK_DATABASE_') || key.startsWith('SDKWORK_WEBSERVER_'),
    )
    .sort()
    .map((key) => `${key}=${redactEnvironmentValue(key, stepEnv[key])}`);
}

function redactEnvironmentValue(key, value) {
  if (/PASSWORD|SECRET|TOKEN/u.test(key)) return '<redacted>';
  return String(value).replace(/\/\/[^/@\s]*@/u, '//<credentials>@');
}

async function runStep(step) {
  await new Promise((resolve, reject) => {
    const child = spawn(step.command, step.args, {
      cwd: step.cwd,
      env: step.env,
      stdio: 'inherit',
      shell: step.shell ?? false,
      windowsHide: step.windowsHide ?? process.platform === 'win32',
    });
    child.on('error', reject);
    child.on('exit', (code, signal) => {
      if (signal) {
        reject(new Error(`${step.name} exited with signal ${signal}`));
      } else if ((code ?? 1) !== 0) {
        reject(new Error(`${step.name} exited with code ${code}`));
      } else {
        resolve();
      }
    });
  });
}

function printHelp() {
  console.log(`Usage: pnpm admin:reset:<target> -- --password <password> [options]

Reset the bootstrap administrator password of one Web Server environment.

Targets:
  dev          Workspace PostgreSQL development profile (.env.postgres)
  test         Installed deployment, test lifecycle
  staging      Installed deployment, staging lifecycle
  release      Installed deployment, production lifecycle

Options:
  --mode <dev|release>            dev reads a dotenv profile, release reads the runtime TOML
  --environment <lifecycle>       development, test, staging, demo, or production
  --password <password>           New password (>= 8 characters); may also be set with
                                  ${PASSWORD_ENV}
  --username <username>           Administrator username (default: admin)
  --tenant-id <tenant-id>         Tenant id (default: the platform tenant 100001)
  --dev-env-file <path>           Development dotenv profile (default: .env.postgres)
  --config-file <path>            Runtime TOML path for --mode release
  --database-url <url>            PostgreSQL URL override
  --database-max-connections <n>  Pool size override
  --gateway-binary <path>         Run an installed gateway binary instead of cargo
  --release                       Build the gateway with cargo --release
  --dry-run                       Print the resolved command without executing it
  -h, --help                      Show this help

The reset verifies the stored credential before reporting success; the new
password is never written to the process argument list.
`);
}

async function main(argv = process.argv.slice(2)) {
  const settings = parseResetAdminArgs(argv);
  if (settings.help) {
    printHelp();
    return;
  }
  const plan = createResetAdminPlan({ settings });
  for (const step of plan.steps) {
    console.error(`[reset-admin] ${formatCommand(step)}`);
    if (settings.dryRun) {
      console.error(`[reset-admin] environment:\n  ${formatEnvironment(step.env).join('\n  ')}`);
      continue;
    }
    await runStep(step);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`[reset-admin] ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}
