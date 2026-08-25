#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

import { parseDotEnv } from '../../../sdkwork-specs/tools/postgres/postgres-config.mjs';

const BASE_DOMAINS = ['sdkwork.com'];
const ENVIRONMENTS = ['development', 'test', 'production'];
const DEPENDENCY_MODES = ['embedded', 'external'];
const MODULE_API_GATEWAY_DEPLOYMENTS = ['bundled', 'docker', 'external'];
const DATABASE_IDENTITIES = {
  development: 'sdkwork_ai_dev',
  test: 'sdkwork_ai_test',
  production: 'sdkwork_ai_prod',
};
const ENVIRONMENT_SUFFIX = {
  development: 'dev',
  test: 'test',
  production: '',
};
const POSTGRES_KEYS = {
  development: {
    db: 'WEBSERVER_POSTGRES_DEV_DB',
    user: 'WEBSERVER_POSTGRES_DEV_USER',
    password: 'WEBSERVER_POSTGRES_DEV_PASSWORD',
  },
  test: {
    db: 'WEBSERVER_POSTGRES_TEST_DB',
    user: 'WEBSERVER_POSTGRES_TEST_USER',
    password: 'WEBSERVER_POSTGRES_TEST_PASSWORD',
  },
  production: {
    db: 'WEBSERVER_POSTGRES_PROD_DB',
    user: 'WEBSERVER_POSTGRES_PROD_USER',
    password: 'WEBSERVER_POSTGRES_PROD_PASSWORD',
  },
};

function expectedHosts(environment) {
  const suffix = ENVIRONMENT_SUFFIX[environment];
  const role = suffix ? `server-${suffix}` : 'server';
  return BASE_DOMAINS.map((domain) => `${role}.${domain}`);
}

function csv(value) {
  return String(value ?? '')
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function requireResolved(env, key) {
  const value = String(env[key] ?? '').trim();
  if (!value || /<[^>]+>|change-me/iu.test(value)) {
    throw new Error(`${key} must be set to a resolved non-placeholder value`);
  }
  return value;
}

export function validateDeploymentEnvironment(env, mode) {
  if (!DEPENDENCY_MODES.includes(mode)) {
    throw new Error(`unsupported dependency mode: ${mode}`);
  }
  const environment = requireResolved(env, 'WEBSERVER_ENVIRONMENT');
  const expectedProfileId = `standalone.${environment}`;
  if (env.WEBSERVER_PROFILE_ID !== expectedProfileId) {
    throw new Error(`WEBSERVER_PROFILE_ID must be ${expectedProfileId}`);
  }
  if (env.SDKWORK_WEBSERVER_PROFILE_ID !== expectedProfileId) {
    throw new Error(`SDKWORK_WEBSERVER_PROFILE_ID must be ${expectedProfileId}`);
  }

  const keys = POSTGRES_KEYS[environment];
  const database = requireResolved(env, keys.db);
  const username = requireResolved(env, keys.user);
  requireResolved(env, keys.password);
  const expectedDatabaseIdentity = DATABASE_IDENTITIES[environment];
  if (database !== expectedDatabaseIdentity || username !== expectedDatabaseIdentity) {
    throw new Error(`${keys.db} and ${keys.user} must be ${expectedDatabaseIdentity}`);
  }

  const hosts = expectedHosts(environment);
  const corsOrigins = csv(env.SDKWORK_CORS_ALLOWED_ORIGINS);
  for (const host of hosts) {
    const httpOrigin = `http://${host}`;
    if (!corsOrigins.includes(httpOrigin)) {
      throw new Error(`SDKWORK_CORS_ALLOWED_ORIGINS is missing ${httpOrigin}`);
    }
  }

  if (mode === 'external') {
    requireResolved(env, 'WEBSERVER_POSTGRES_HOST');
    requireResolved(env, 'WEBSERVER_REDIS_HOST');
  }

  requireResolved(env, 'SDKWORK_SPACE_ROOT');
  requireResolved(env, 'SDKWORK_SPACE_HOST_PATH');
  requireResolved(env, 'SDKWORK_SPACE_CLONE_URL');
  if (String(env.SDKWORK_SPACE_ROOT ?? '').trim() !== '/opt/deploy') {
    throw new Error('SDKWORK_SPACE_ROOT must be /opt/deploy for docker space integration');
  }
  if (String(env.SDKWORK_SPACE_HOST_PATH ?? '').trim() !== '/opt/deploy') {
    throw new Error('SDKWORK_SPACE_HOST_PATH must be /opt/deploy for docker space integration');
  }
  if (!String(env.SDKWORK_SPACE_CLONE_URL ?? '').includes('sdkwork-space')) {
    throw new Error('SDKWORK_SPACE_CLONE_URL must reference the sdkwork-space checkout');
  }

  const gatewayDeployment = String(env.SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT ?? 'docker').trim();
  if (!MODULE_API_GATEWAY_DEPLOYMENTS.includes(gatewayDeployment)) {
    throw new Error(
      `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT must be one of ${MODULE_API_GATEWAY_DEPLOYMENTS.join(', ')}`,
    );
  }
  if (gatewayDeployment === 'external' && !String(env.SDKWORK_MODULE_API_GATEWAY_HOST ?? '').trim()) {
    throw new Error('SDKWORK_MODULE_API_GATEWAY_HOST is required when deployment mode is external');
  }

  return { environment, expectedProfileId, hosts, mode, gatewayDeployment };
}

function runComposeConfig(appRoot, envFile, mode, environment) {
  const args = [
    'compose',
    '--env-file',
    envFile,
    '-f',
    path.join(appRoot, 'deployments', 'docker', 'docker-compose.yml'),
  ];
  if (mode === 'external') {
    args.push('-f', path.join(appRoot, 'deployments', 'docker', 'docker-compose.external.yml'));
  }
  args.push('--profile', environment, 'config', '--quiet');
  const result = spawnSync('docker', args, {
    cwd: appRoot,
    encoding: 'utf8',
    shell: false,
  });
  if (result.error) {
    throw new Error(`docker compose is unavailable: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`docker compose config failed: ${result.stderr || result.stdout}`);
  }
}

export function resolveExampleEnvironment(env, mode) {
  const resolved = Object.fromEntries(
    Object.entries(env).map(([key, value]) => [
      key,
      String(value).replace(/<[^>]+>/gu, `validation-only-${key.toLowerCase()}`),
    ]),
  );
  if (mode === 'external') {
    resolved.WEBSERVER_POSTGRES_HOST = 'postgres.validation.internal';
    resolved.WEBSERVER_REDIS_HOST = 'redis.validation.internal';
  }
  return resolved;
}

export function validateDeploymentMatrix(appRoot, { compose = false } = {}) {
  const summaries = [];
  for (const environment of ENVIRONMENTS) {
    const envFile = path.join(appRoot, 'deployments', 'docker', 'env', `${environment}.env.example`);
    const source = parseDotEnv(readFileSync(envFile, 'utf8'));
    for (const mode of DEPENDENCY_MODES) {
      const resolved = resolveExampleEnvironment(source, mode);
      summaries.push(validateDeploymentEnvironment(resolved, mode));
      if (compose) {
        runComposeConfig(appRoot, envFile, mode, environment);
      }
    }
  }
  return summaries;
}

function main() {
  const { values } = parseArgs({
    args: process.argv.slice(2),
    options: {
      compose: { type: 'boolean', default: false },
      'env-file': { type: 'string' },
      matrix: { type: 'boolean', default: false },
      mode: { type: 'string', default: 'embedded' },
    },
    strict: true,
  });
  const scriptPath = fileURLToPath(import.meta.url);
  const appRoot = path.resolve(path.dirname(scriptPath), '../..');
  if (values.matrix) {
    const summaries = validateDeploymentMatrix(appRoot, { compose: values.compose });
    process.stdout.write(`${JSON.stringify({ ok: true, count: summaries.length, summaries })}\n`);
    return;
  }
  if (!values['env-file']) {
    throw new Error('--env-file is required unless --matrix is selected');
  }
  const envFile = path.resolve(values['env-file']);
  const env = parseDotEnv(readFileSync(envFile, 'utf8'));
  const summary = validateDeploymentEnvironment(env, values.mode);
  if (values.compose) {
    runComposeConfig(appRoot, envFile, values.mode, summary.environment);
  }
  process.stdout.write(`${JSON.stringify({ ok: true, ...summary })}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
