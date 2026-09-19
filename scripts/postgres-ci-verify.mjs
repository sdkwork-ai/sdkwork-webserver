#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_POSTGRES_IMAGE =
  'postgres:16.9-alpine3.22@sha256:7c688148e5e156d0e86df7ba8ae5a05a2386aaec1e2ad8e6d11bdf10504b1fb7';
const DATABASE_NAME = 'sdkwork_ai_test_run_web_ci';
const DATABASE_USER = 'sdkwork_ai_test';
const DATABASE_PASSWORD = 'sdkwork-ci-only-password';
const READY_ATTEMPTS = 60;
const READY_INTERVAL_MS = 1_000;
const COMMAND_TIMEOUT_MS = 15 * 60 * 1_000;
const POSTGRES_INITIALIZATION_COMPLETE =
  'PostgreSQL init process complete; ready for start up.';
const CRITICAL_SOURCE_FILES = Object.freeze([
  'Cargo.toml',
  'Cargo.lock',
  'database/ddl/baseline/postgres/0001_webserver_baseline.sql',
  'crates/sdkwork-webserver-database-host/tests/postgres_lifecycle.rs',
  'crates/sdkwork-intelligence-webserver-repository-sqlx/tests/repository_parity.rs',
]);

export function createPostgresCiPlan({
  image = process.env.SDKWORK_WEBSERVER_POSTGRES_CI_IMAGE || DEFAULT_POSTGRES_IMAGE,
} = {}) {
  if (!/^postgres:[a-zA-Z0-9_.-]+@sha256:[a-f0-9]{64}$/u.test(image)) {
    throw new Error('PostgreSQL CI image must use a tag plus sha256 digest');
  }
  return Object.freeze({
    image,
    lifecycleTest: Object.freeze([
      'cargo',
      'test',
      '-p',
      'sdkwork-webserver-database-host',
      '--test',
      'postgres_lifecycle',
      'postgres_baseline_seed_and_drift_are_clean',
      '--',
      '--ignored',
      '--exact',
      '--nocapture',
    ]),
    repositoryTest: Object.freeze([
      'cargo',
      'test',
      '-p',
      'sdkwork-intelligence-webserver-repository-sqlx',
      '--test',
      'repository_parity',
      'postgres_repository_transactions_tenants_idempotency_and_pagination_are_bounded',
      '--',
      '--ignored',
      '--exact',
      '--nocapture',
    ]),
  });
}

function parseArgs(argv) {
  const options = { dryRun: false };
  for (const argument of argv) {
    if (argument === '--dry-run') {
      options.dryRun = true;
    } else if (argument === '--help' || argument === '-h') {
      options.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  return options;
}

function printHelp() {
  console.log(`Usage: node scripts/postgres-ci-verify.mjs [options]

Options:
  --dry-run  Print the bounded disposable PostgreSQL verification plan
  --help     Show this help`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: REPO_ROOT,
    encoding: options.capture ? 'utf8' : undefined,
    env: options.env ?? process.env,
    maxBuffer: options.capture ? 64 * 1024 : undefined,
    stdio: options.capture ? 'pipe' : 'inherit',
    timeout: options.timeoutMs ?? COMMAND_TIMEOUT_MS,
    windowsHide: true,
  });
  if (result.error) {
    throw new Error(`${command} failed to start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const detail = options.capture ? String(result.stderr || result.stdout || '').trim() : '';
    throw new Error(`${command} exited with status ${result.status}${detail ? `: ${detail}` : ''}`);
  }
  return result;
}

function ensureCriticalSources() {
  for (const relativePath of CRITICAL_SOURCE_FILES) {
    const absolutePath = path.join(REPO_ROOT, relativePath);
    if (existsSync(absolutePath)) {
      continue;
    }
    const tracked = run('git', ['ls-files', '--error-unmatch', '--', relativePath], {
      capture: true,
      timeoutMs: 30_000,
    });
    if (!String(tracked.stdout).trim()) {
      throw new Error(`missing build-critical source ${relativePath}`);
    }
    run('git', ['checkout', 'HEAD', '--', relativePath], { timeoutMs: 30_000 });
    if (!existsSync(absolutePath)) {
      throw new Error(
        `failed to recover build-critical source ${relativePath}; run: git checkout HEAD -- ${relativePath}`,
      );
    }
    console.log(`[sdkwork-web-postgres-ci] recovered ${relativePath} from git`);
  }
}

export function isPostgresReady(initializationLogs, probeStatus) {
  return (
    probeStatus === 0 && String(initializationLogs).includes(POSTGRES_INITIALIZATION_COMPLETE)
  );
}

function waitForPostgres(containerName) {
  for (let attempt = 1; attempt <= READY_ATTEMPTS; attempt += 1) {
    const result = spawnSync(
      'docker',
      [
        'exec',
        containerName,
        'pg_isready',
        '--username',
        DATABASE_USER,
        '--dbname',
        DATABASE_NAME,
      ],
      {
        cwd: REPO_ROOT,
        encoding: 'utf8',
        maxBuffer: 64 * 1024,
        stdio: 'pipe',
        timeout: 10_000,
        windowsHide: true,
      },
    );
    const logs = spawnSync('docker', ['logs', containerName], {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      maxBuffer: 256 * 1024,
      stdio: 'pipe',
      timeout: 10_000,
      windowsHide: true,
    });
    const initializationLogs = `${logs.stdout || ''}\n${logs.stderr || ''}`;
    if (isPostgresReady(initializationLogs, result.status)) {
      return;
    }
    if (attempt < READY_ATTEMPTS) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, READY_INTERVAL_MS);
    }
  }
  throw new Error(`PostgreSQL did not become ready within ${READY_ATTEMPTS} seconds`);
}

function resolvePublishedPort(containerName) {
  const result = run('docker', ['port', containerName, '5432/tcp'], {
    capture: true,
    timeoutMs: 30_000,
  });
  const match = /:(?<port>[0-9]{1,5})\s*$/u.exec(String(result.stdout).trim());
  const port = Number(match?.groups?.port);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error(`cannot resolve bounded PostgreSQL host port: ${String(result.stdout).trim()}`);
  }
  return port;
}

function resetCanonicalSchema(containerName) {
  run(
    'docker',
    [
      'exec',
      '--env',
      `PGPASSWORD=${DATABASE_PASSWORD}`,
      containerName,
      'psql',
      '--username',
      DATABASE_USER,
      '--dbname',
      DATABASE_NAME,
      '--set',
      'ON_ERROR_STOP=1',
      '--command',
      `DROP SCHEMA IF EXISTS ${DATABASE_NAME} CASCADE; CREATE SCHEMA ${DATABASE_NAME};`,
    ],
    { timeoutMs: 60_000 },
  );
}

export function runPostgresCi(plan = createPostgresCiPlan()) {
  ensureCriticalSources();
  run('docker', ['version', '--format', '{{.Server.Version}}'], {
    capture: true,
    timeoutMs: 30_000,
  });

  const containerName = `sdkwork-web-postgres-ci-${process.pid}-${Date.now()}`;
  try {
    run(
      'docker',
      [
        'run',
        '--detach',
        '--rm',
        '--name',
        containerName,
        '--env',
        `POSTGRES_DB=${DATABASE_NAME}`,
        '--env',
        `POSTGRES_USER=${DATABASE_USER}`,
        '--env',
        `POSTGRES_PASSWORD=${DATABASE_PASSWORD}`,
        '--publish',
        '127.0.0.1::5432',
        plan.image,
      ],
      { capture: true, timeoutMs: 5 * 60 * 1_000 },
    );
    waitForPostgres(containerName);
    resetCanonicalSchema(containerName);
    const port = resolvePublishedPort(containerName);
    const databaseUrl =
      `postgresql://${DATABASE_USER}:${DATABASE_PASSWORD}@127.0.0.1:${port}/${DATABASE_NAME}`
      + '?sslmode=disable&application_name=sdkwork_web_ci';
    const testEnv = {
      ...process.env,
      SDKWORK_DATABASE_TEST_POSTGRES_URL: databaseUrl,
    };

    console.log('[sdkwork-web-postgres-ci] verifying PostgreSQL lifecycle, seed, and drift');
    run(plan.lifecycleTest[0], plan.lifecycleTest.slice(1), { env: testEnv });
    resetCanonicalSchema(containerName);
    console.log('[sdkwork-web-postgres-ci] verifying PostgreSQL Repository parity');
    run(plan.repositoryTest[0], plan.repositoryTest.slice(1), { env: testEnv });
  } finally {
    spawnSync('docker', ['rm', '--force', containerName], {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      maxBuffer: 64 * 1024,
      stdio: 'pipe',
      timeout: 30_000,
      windowsHide: true,
    });
  }
}

function isMainModule() {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
}

if (isMainModule()) {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options.help) {
      printHelp();
    } else {
      const plan = createPostgresCiPlan();
      if (options.dryRun) {
        console.log(JSON.stringify(plan, null, 2));
      } else {
        runPostgresCi(plan);
      }
    }
  } catch (error) {
    console.error(`[sdkwork-web-postgres-ci] ${error instanceof Error ? error.message : error}`);
    process.exitCode = 1;
  }
}
