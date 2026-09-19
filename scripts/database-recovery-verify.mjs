#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_POSTGRES_IMAGE =
  'postgres:16.9-alpine3.22@sha256:7c688148e5e156d0e86df7ba8ae5a05a2386aaec1e2ad8e6d11bdf10504b1fb7';
const SOURCE_DATABASE = 'sdkwork_web_recovery_source';
const RESTORE_DATABASE = 'sdkwork_web_recovery_restore';
const DATABASE_USER = 'sdkwork_recovery';
const DATABASE_PASSWORD = 'sdkwork-recovery-test-only-password';
const READY_ATTEMPTS = 60;
const READY_INTERVAL_MS = 1_000;
const OPERATION_TIMEOUT_MS = 10 * 60 * 1_000;
const COMMAND_TIMEOUT_MS = 2 * 60 * 1_000;
const MAX_CAPTURE_BYTES = 64 * 1024;
const MAX_BASELINE_BYTES = 2 * 1024 * 1024;
const MAX_BACKUP_BYTES = 64 * 1024 * 1024;
const BASELINE_PATH = 'database/ddl/baseline/postgres/0001_webserver_baseline.sql';
const CONTAINER_BACKUP_PATH = '/tmp/sdkwork-web-recovery.dump';
const CANARY_ID = '9500001';
const CANARY_TENANT_ID = '9500';

const CRITICAL_SOURCE_FILES = Object.freeze([
  'Cargo.toml',
  'Cargo.lock',
  BASELINE_PATH,
]);

export function createDatabaseRecoveryPlan({
  image = process.env.SDKWORK_WEBSERVER_POSTGRES_CI_IMAGE || DEFAULT_POSTGRES_IMAGE,
} = {}) {
  if (!/^postgres:[a-zA-Z0-9_.-]+@sha256:[a-f0-9]{64}$/u.test(image)) {
    throw new Error('PostgreSQL recovery image must use a tag plus sha256 digest');
  }
  return Object.freeze({
    image,
    limits: Object.freeze({
      readyAttempts: READY_ATTEMPTS,
      operationTimeoutMs: OPERATION_TIMEOUT_MS,
      maxCaptureBytes: MAX_CAPTURE_BYTES,
      maxBaselineBytes: MAX_BASELINE_BYTES,
      maxBackupBytes: MAX_BACKUP_BYTES,
      maxContainers: 1,
      containerMemoryBytes: 256 * 1024 * 1024,
      containerCpus: 1,
      containerPids: 256,
    }),
    postgres: Object.freeze({
      dump: Object.freeze([
        'pg_dump',
        '--format=custom',
        '--compress=6',
        '--no-owner',
        '--no-acl',
        `--username=${DATABASE_USER}`,
        `--file=${CONTAINER_BACKUP_PATH}`,
        `--dbname=${SOURCE_DATABASE}`,
      ]),
      restore: Object.freeze([
        'pg_restore',
        '--exit-on-error',
        '--no-owner',
        '--no-acl',
        `--username=${DATABASE_USER}`,
        `--dbname=${RESTORE_DATABASE}`,
        CONTAINER_BACKUP_PATH,
      ]),
    }),
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
  console.log(`Usage: node scripts/database-recovery-verify.mjs [options]

Options:
  --dry-run  Print the bounded PostgreSQL recovery verification plan
  --help     Show this help`);
}

function createDeadline() {
  const expiresAt = Date.now() + OPERATION_TIMEOUT_MS;
  return (requestedMs = COMMAND_TIMEOUT_MS) => {
    const remainingMs = expiresAt - Date.now();
    if (remainingMs <= 0) {
      throw new Error(`database recovery verification exceeded ${OPERATION_TIMEOUT_MS}ms`);
    }
    return Math.max(1, Math.min(requestedMs, remainingMs));
  };
}

function run(command, args, options = {}) {
  const capture = options.capture === true;
  const stdio = capture
    ? 'pipe'
    : options.input === undefined
      ? 'inherit'
      : ['pipe', 'inherit', 'inherit'];
  const result = spawnSync(command, args, {
    cwd: REPO_ROOT,
    encoding: capture ? 'utf8' : undefined,
    env: options.env ?? process.env,
    maxBuffer: capture ? MAX_CAPTURE_BYTES : undefined,
    input: options.input,
    stdio,
    timeout: options.timeoutMs ?? COMMAND_TIMEOUT_MS,
    windowsHide: true,
  });
  if (result.error) {
    throw new Error(`${command} failed to start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const detail = capture ? String(result.stderr || result.stdout || '').trim() : '';
    throw new Error(`${command} exited with status ${result.status}${detail ? `: ${detail}` : ''}`);
  }
  return result;
}

function ensureCriticalSources(remainingTimeout) {
  for (const relativePath of CRITICAL_SOURCE_FILES) {
    const absolutePath = path.join(REPO_ROOT, relativePath);
    if (existsSync(absolutePath)) {
      continue;
    }
    const tracked = run('git', ['ls-files', '--error-unmatch', '--', relativePath], {
      capture: true,
      timeoutMs: remainingTimeout(30_000),
    });
    if (!String(tracked.stdout).trim()) {
      throw new Error(`missing build-critical source ${relativePath}`);
    }
    run('git', ['checkout', 'HEAD', '--', relativePath], {
      timeoutMs: remainingTimeout(30_000),
    });
    if (!existsSync(absolutePath)) {
      throw new Error(`failed to recover build-critical source ${relativePath}`);
    }
    console.log(`[sdkwork-web-recovery] recovered ${relativePath} from git`);
  }
  const baselineBytes = statSync(path.join(REPO_ROOT, BASELINE_PATH)).size;
  if (baselineBytes <= 0 || baselineBytes > MAX_BASELINE_BYTES) {
    throw new Error(`PostgreSQL baseline must be within 1..${MAX_BASELINE_BYTES} bytes`);
  }
}

function dockerExec(containerName, args, remainingTimeout, options = {}) {
  const interactiveArgs = options.input === undefined ? [] : ['--interactive'];
  return run(
    'docker',
    [
      'exec',
      ...interactiveArgs,
      '--env',
      `PGPASSWORD=${DATABASE_PASSWORD}`,
      containerName,
      ...args,
    ],
    {
      capture: options.capture,
      input: options.input,
      timeoutMs: remainingTimeout(options.timeoutMs ?? COMMAND_TIMEOUT_MS),
    },
  );
}

function waitForPostgres(containerName, remainingTimeout) {
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
        SOURCE_DATABASE,
      ],
      {
        cwd: REPO_ROOT,
        encoding: 'utf8',
        maxBuffer: MAX_CAPTURE_BYTES,
        stdio: 'pipe',
        timeout: remainingTimeout(10_000),
        windowsHide: true,
      },
    );
    if (result.status === 0) {
      return;
    }
    if (result.error && result.error.code !== 'ETIMEDOUT') {
      throw new Error(`PostgreSQL readiness check failed: ${result.error.message}`);
    }
    if (attempt < READY_ATTEMPTS) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, READY_INTERVAL_MS);
    }
  }
  throw new Error(`PostgreSQL did not become ready within ${READY_ATTEMPTS} seconds`);
}

function psql(containerName, database, sql, remainingTimeout, capture = false) {
  return dockerExec(
    containerName,
    [
      'psql',
      '--username',
      DATABASE_USER,
      '--dbname',
      database,
      '--set',
      'ON_ERROR_STOP=1',
      '--no-psqlrc',
      ...(capture ? ['--tuples-only', '--no-align'] : []),
      '--command',
      sql,
    ],
    remainingTimeout,
    { capture },
  );
}

function verifyPostgresRecovery(containerName, plan, remainingTimeout) {
  const baseline = readFileSync(path.join(REPO_ROOT, BASELINE_PATH));
  dockerExec(
    containerName,
    [
      'psql',
      '--username',
      DATABASE_USER,
      '--dbname',
      SOURCE_DATABASE,
      '--set',
      'ON_ERROR_STOP=1',
      '--no-psqlrc',
      '--file',
      '-',
    ],
    remainingTimeout,
    { input: baseline },
  );
  psql(
    containerName,
    SOURCE_DATABASE,
    `INSERT INTO webserver_site (`
      + `id, uuid, tenant_id, organization_id, data_scope, user_id, name, slug, description, `
      + `site_type, status, runtime_config, metadata, created_at, updated_at, version, deleted_at, deleted_by`
      + `) VALUES (`
      + `${CANARY_ID}, 'recovery-canary-uuid', ${CANARY_TENANT_ID}, 0, 1, NULL, `
      + `'recovery-canary', 'recovery-canary', NULL, 1, 1, '{}', '{}', `
      + `'2026-07-19T00:00:00Z', '2026-07-19T00:00:00Z', 0, NULL, NULL);`,
    remainingTimeout,
  );

  dockerExec(containerName, plan.postgres.dump, remainingTimeout);
  const sizeResult = dockerExec(
    containerName,
    ['stat', '-c', '%s', CONTAINER_BACKUP_PATH],
    remainingTimeout,
    { capture: true, timeoutMs: 30_000 },
  );
  const backupBytes = Number(String(sizeResult.stdout).trim());
  if (!Number.isSafeInteger(backupBytes) || backupBytes <= 0 || backupBytes > MAX_BACKUP_BYTES) {
    throw new Error(`PostgreSQL backup must be within 1..${MAX_BACKUP_BYTES} bytes`);
  }
  const checksumResult = dockerExec(
    containerName,
    ['sha256sum', CONTAINER_BACKUP_PATH],
    remainingTimeout,
    { capture: true, timeoutMs: 30_000 },
  );
  if (!/^[a-f0-9]{64}\s/u.test(String(checksumResult.stdout))) {
    throw new Error('PostgreSQL backup checksum is missing or malformed');
  }

  psql(
    containerName,
    SOURCE_DATABASE,
    `UPDATE webserver_site SET name = 'source-mutated-after-backup' WHERE id = ${CANARY_ID};`,
    remainingTimeout,
  );
  dockerExec(
    containerName,
    ['createdb', '--username', DATABASE_USER, RESTORE_DATABASE],
    remainingTimeout,
  );
  dockerExec(containerName, plan.postgres.restore, remainingTimeout);
  const canaryResult = psql(
    containerName,
    RESTORE_DATABASE,
    `SELECT name || ':' || tenant_id::text FROM webserver_site `
      + `WHERE id = ${CANARY_ID} AND tenant_id = ${CANARY_TENANT_ID};`,
    remainingTimeout,
    true,
  );
  if (String(canaryResult.stdout).trim() !== `recovery-canary:${CANARY_TENANT_ID}`) {
    throw new Error('restored PostgreSQL canary does not match the isolated backup generation');
  }
  const tableCountResult = psql(
    containerName,
    RESTORE_DATABASE,
    "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name LIKE 'web_%';",
    remainingTimeout,
    true,
  );
  const tableCount = Number(String(tableCountResult.stdout).trim());
  if (!Number.isSafeInteger(tableCount) || tableCount < 10) {
    throw new Error('restored PostgreSQL Web schema is incomplete');
  }
  console.log(
    `[sdkwork-web-recovery] PostgreSQL restored ${tableCount} Web tables from a ${backupBytes}-byte checksummed backup`,
  );
}

export function runDatabaseRecovery(plan = createDatabaseRecoveryPlan()) {
  const remainingTimeout = createDeadline();
  ensureCriticalSources(remainingTimeout);
  run('docker', ['version', '--format', '{{.Server.Version}}'], {
    capture: true,
    timeoutMs: remainingTimeout(30_000),
  });

  const containerName = `sdkwork-web-recovery-${process.pid}-${Date.now()}`;
  try {
    run(
      'docker',
      [
        'run',
        '--detach',
        '--rm',
        '--name',
        containerName,
        '--network',
        'none',
        '--memory',
        '256m',
        '--cpus',
        '1.0',
        '--pids-limit',
        '256',
        '--tmpfs',
        '/var/lib/postgresql/data:rw,nosuid,size=128m',
        '--tmpfs',
        '/tmp:rw,noexec,nosuid,size=80m',
        '--env',
        `POSTGRES_DB=${SOURCE_DATABASE}`,
        '--env',
        `POSTGRES_USER=${DATABASE_USER}`,
        '--env',
        `POSTGRES_PASSWORD=${DATABASE_PASSWORD}`,
        plan.image,
      ],
      { capture: true, timeoutMs: remainingTimeout(5 * 60 * 1_000) },
    );
    waitForPostgres(containerName, remainingTimeout);
    console.log('[sdkwork-web-recovery] verifying PostgreSQL custom-format backup/restore');
    verifyPostgresRecovery(containerName, plan, remainingTimeout);
  } finally {
    spawnSync('docker', ['rm', '--force', containerName], {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      maxBuffer: MAX_CAPTURE_BYTES,
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
      const plan = createDatabaseRecoveryPlan();
      if (options.dryRun) {
        console.log(JSON.stringify(plan, null, 2));
      } else {
        runDatabaseRecovery(plan);
      }
    }
  } catch (error) {
    console.error(`[sdkwork-web-recovery] ${error instanceof Error ? error.message : error}`);
    process.exitCode = 1;
  }
}
