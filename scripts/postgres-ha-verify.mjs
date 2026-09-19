#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_POSTGRES_IMAGE =
  'postgres:16.9-alpine3.22@sha256:7c688148e5e156d0e86df7ba8ae5a05a2386aaec1e2ad8e6d11bdf10504b1fb7';
const DATABASE_NAME = 'sdkwork_web_ha';
const DATABASE_USER = 'sdkwork_ha';
const DATABASE_PASSWORD = 'sdkwork-ha-test-only-password';
const REPLICATION_SLOT = 'sdkwork_web_ha_slot';
const PGDATA = '/var/lib/postgresql/data';
const BASELINE_PATH = 'database/ddl/baseline/postgres/0001_webserver_baseline.sql';
const READY_ATTEMPTS = 60;
const CONVERGENCE_ATTEMPTS = 60;
const POLL_INTERVAL_MS = 1_000;
const OPERATION_TIMEOUT_MS = 10 * 60 * 1_000;
const COMMAND_TIMEOUT_MS = 2 * 60 * 1_000;
const MAX_CAPTURE_BYTES = 64 * 1024;
const MAX_BASELINE_BYTES = 2 * 1024 * 1024;
const CANARY_ID = '9600001';
const PROMOTED_CANARY_ID = '9600002';
const CANARY_TENANT_ID = '9600';

const CRITICAL_SOURCE_FILES = Object.freeze([
  'package.json',
  'sdkwork.workflow.json',
  BASELINE_PATH,
]);

export function createPostgresHaPlan({
  image = process.env.SDKWORK_WEBSERVER_POSTGRES_CI_IMAGE || DEFAULT_POSTGRES_IMAGE,
} = {}) {
  if (!/^postgres:[a-zA-Z0-9_.-]+@sha256:[a-f0-9]{64}$/u.test(image)) {
    throw new Error('PostgreSQL HA image must use a tag plus sha256 digest');
  }
  return Object.freeze({
    image,
    limits: Object.freeze({
      operationTimeoutMs: OPERATION_TIMEOUT_MS,
      readyAttempts: READY_ATTEMPTS,
      convergenceAttempts: CONVERGENCE_ATTEMPTS,
      maxCaptureBytes: MAX_CAPTURE_BYTES,
      maxBaselineBytes: MAX_BASELINE_BYTES,
      maxContainers: 2,
      maxNetworks: 1,
      memoryBytesPerContainer: 256 * 1024 * 1024,
      cpusPerContainer: 1,
      pidsPerContainer: 256,
      dataTmpfsBytesPerContainer: 128 * 1024 * 1024,
    }),
    baseBackup: Object.freeze([
      'pg_basebackup',
      '--host=primary',
      '--port=5432',
      `--username=${DATABASE_USER}`,
      `--pgdata=${PGDATA}`,
      '--write-recovery-conf',
      '--wal-method=stream',
      `--slot=${REPLICATION_SLOT}`,
      '--no-password',
      '--checkpoint=fast',
    ]),
    promote: Object.freeze([
      'pg_ctl',
      `--pgdata=${PGDATA}`,
      'promote',
      '--wait',
      '--timeout=30',
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
  console.log(`Usage: node scripts/postgres-ha-verify.mjs [options]

Options:
  --dry-run  Print the bounded PostgreSQL streaming failover plan
  --help     Show this help`);
}

function createDeadline() {
  const expiresAt = Date.now() + OPERATION_TIMEOUT_MS;
  return (requestedMs = COMMAND_TIMEOUT_MS) => {
    const remainingMs = expiresAt - Date.now();
    if (remainingMs <= 0) {
      throw new Error(`PostgreSQL HA verification exceeded ${OPERATION_TIMEOUT_MS}ms`);
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
    input: options.input,
    maxBuffer: capture ? MAX_CAPTURE_BYTES : undefined,
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
    console.log(`[sdkwork-web-postgres-ha] recovered ${relativePath} from git`);
  }
  const baselineBytes = statSync(path.join(REPO_ROOT, BASELINE_PATH)).size;
  if (baselineBytes <= 0 || baselineBytes > MAX_BASELINE_BYTES) {
    throw new Error(`PostgreSQL baseline must be within 1..${MAX_BASELINE_BYTES} bytes`);
  }
}

function dockerExec(containerName, args, remainingTimeout, options = {}) {
  const interactiveArgs = options.input === undefined ? [] : ['--interactive'];
  const userArgs = options.user ? ['--user', options.user] : [];
  const envArgs = (options.env ?? []).flatMap((value) => ['--env', value]);
  return run(
    'docker',
    ['exec', ...interactiveArgs, ...userArgs, ...envArgs, containerName, ...args],
    {
      capture: options.capture,
      input: options.input,
      timeoutMs: remainingTimeout(options.timeoutMs ?? COMMAND_TIMEOUT_MS),
    },
  );
}

function psql(containerName, sql, remainingTimeout, capture = false) {
  return dockerExec(
    containerName,
    [
      'psql',
      `--username=${DATABASE_USER}`,
      `--dbname=${DATABASE_NAME}`,
      '--set',
      'ON_ERROR_STOP=1',
      '--no-psqlrc',
      ...(capture ? ['--tuples-only', '--no-align'] : []),
      '--command',
      sql,
    ],
    remainingTimeout,
    { capture, env: [`PGPASSWORD=${DATABASE_PASSWORD}`] },
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
        `--username=${DATABASE_USER}`,
        `--dbname=${DATABASE_NAME}`,
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
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, POLL_INTERVAL_MS);
    }
  }
  throw new Error(`${containerName} did not become ready within ${READY_ATTEMPTS} seconds`);
}

function waitForSqlValue(containerName, sql, expected, remainingTimeout, label) {
  for (let attempt = 1; attempt <= CONVERGENCE_ATTEMPTS; attempt += 1) {
    try {
      const result = psql(containerName, sql, remainingTimeout, true);
      if (String(result.stdout).trim() === expected) {
        return;
      }
    } catch (error) {
      if (attempt === CONVERGENCE_ATTEMPTS) {
        throw error;
      }
    }
    if (attempt < CONVERGENCE_ATTEMPTS) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, POLL_INTERVAL_MS);
    }
  }
  throw new Error(`${label} did not converge within ${CONVERGENCE_ATTEMPTS} seconds`);
}

function containerResourceArgs() {
  return [
    '--memory',
    '256m',
    '--cpus',
    '1.0',
    '--pids-limit',
    '256',
    '--shm-size',
    '32m',
    '--tmpfs',
    `${PGDATA}:rw,nosuid,size=128m`,
    '--tmpfs',
    '/tmp:rw,noexec,nosuid,size=16m',
    '--tmpfs',
    '/var/run/postgresql:rw,nosuid,size=1m',
  ];
}

function insertCanarySql(id, name) {
  return `INSERT INTO webserver_site (`
    + `id, uuid, tenant_id, organization_id, data_scope, user_id, name, slug, description, `
    + `site_type, status, runtime_config, metadata, created_at, updated_at, version, deleted_at, deleted_by`
    + `) VALUES (`
    + `${id}, 'ha-${id}', ${CANARY_TENANT_ID}, 0, 1, NULL, '${name}', 'ha-${id}', NULL, `
    + `1, 1, '{}', '{}', '2026-07-19T00:00:00Z', '2026-07-19T00:00:00Z', `
    + `0, NULL, NULL);`;
}

function configurePrimary(primaryName, remainingTimeout) {
  const baseline = readFileSync(path.join(REPO_ROOT, BASELINE_PATH));
  const hbaResult = psql(primaryName, 'SHOW hba_file;', remainingTimeout, true);
  const hbaFile = String(hbaResult.stdout).trim();
  if (!hbaFile.startsWith(`${PGDATA}/`) || !hbaFile.endsWith('/pg_hba.conf')) {
    throw new Error(`unexpected PostgreSQL HBA path: ${hbaFile}`);
  }
  dockerExec(
    primaryName,
    ['tee', '-a', hbaFile],
    remainingTimeout,
    {
      input: Buffer.from(`host replication ${DATABASE_USER} all scram-sha-256\n`, 'utf8'),
      user: 'postgres',
    },
  );
  psql(primaryName, 'SELECT pg_reload_conf();', remainingTimeout);
  psql(
    primaryName,
    `SELECT * FROM pg_create_physical_replication_slot('${REPLICATION_SLOT}');`,
    remainingTimeout,
  );
  dockerExec(
    primaryName,
    [
      'psql',
      `--username=${DATABASE_USER}`,
      `--dbname=${DATABASE_NAME}`,
      '--set',
      'ON_ERROR_STOP=1',
      '--no-psqlrc',
      '--file',
      '-',
    ],
    remainingTimeout,
    { input: baseline, env: [`PGPASSWORD=${DATABASE_PASSWORD}`] },
  );
  psql(primaryName, insertCanarySql(CANARY_ID, 'ha-before-failover'), remainingTimeout);
}

function startStandby(standbyName, networkName, plan, remainingTimeout) {
  run(
    'docker',
    [
      'run',
      '--detach',
      '--rm',
      '--name',
      standbyName,
      '--hostname',
      'standby',
      '--network',
      networkName,
      '--network-alias',
      'standby',
      ...containerResourceArgs(),
      '--entrypoint',
      'sleep',
      plan.image,
      'infinity',
    ],
    { capture: true, timeoutMs: remainingTimeout(5 * 60 * 1_000) },
  );
  dockerExec(
    standbyName,
    ['chown', '-R', 'postgres:postgres', PGDATA, '/var/run/postgresql'],
    remainingTimeout,
  );
  dockerExec(standbyName, plan.baseBackup, remainingTimeout, {
    user: 'postgres',
    env: [`PGPASSWORD=${DATABASE_PASSWORD}`],
  });
  dockerExec(standbyName, ['chmod', '0700', PGDATA], remainingTimeout, {
    user: 'postgres',
  });
  try {
    dockerExec(
      standbyName,
      [
        'pg_ctl',
        `--pgdata=${PGDATA}`,
        `--log=${PGDATA}/standby.log`,
        '--options=-c hot_standby=on -c listen_addresses=*',
        'start',
        '--wait',
        '--timeout=30',
      ],
      remainingTimeout,
      { user: 'postgres', env: [`PGPASSWORD=${DATABASE_PASSWORD}`] },
    );
  } catch (error) {
    const logResult = dockerExec(
      standbyName,
      ['cat', `${PGDATA}/standby.log`],
      remainingTimeout,
      { capture: true, user: 'postgres', timeoutMs: 30_000 },
    );
    throw new Error(
      `standby start failed: ${error instanceof Error ? error.message : error}: `
        + String(logResult.stdout).trim(),
    );
  }
  waitForPostgres(standbyName, remainingTimeout);
}

function verifyReplication(primaryName, standbyName, remainingTimeout) {
  waitForSqlValue(
    primaryName,
    "SELECT COUNT(*)::text FROM pg_stat_replication WHERE state = 'streaming';",
    '1',
    remainingTimeout,
    'streaming replication',
  );
  waitForSqlValue(
    standbyName,
    'SELECT pg_is_in_recovery()::text;',
    'true',
    remainingTimeout,
    'standby recovery state',
  );
  psql(
    primaryName,
    `UPDATE webserver_site SET name = 'ha-replicated-before-failover' WHERE id = ${CANARY_ID};`,
    remainingTimeout,
  );
  const lsnResult = psql(
    primaryName,
    'SELECT pg_current_wal_flush_lsn()::text;',
    remainingTimeout,
    true,
  );
  const lsn = String(lsnResult.stdout).trim();
  if (!/^[A-F0-9]+\/[A-F0-9]+$/u.test(lsn)) {
    throw new Error(`invalid primary WAL LSN: ${lsn}`);
  }
  waitForSqlValue(
    standbyName,
    `SELECT (pg_last_wal_replay_lsn() >= '${lsn}'::pg_lsn)::text;`,
    'true',
    remainingTimeout,
    'standby WAL replay',
  );
  waitForSqlValue(
    standbyName,
    `SELECT name FROM webserver_site WHERE id = ${CANARY_ID} AND tenant_id = ${CANARY_TENANT_ID};`,
    'ha-replicated-before-failover',
    remainingTimeout,
    'tenant canary replication',
  );
  return lsn;
}

function promoteStandby(primaryName, standbyName, plan, remainingTimeout) {
  run('docker', ['stop', '--time', '10', primaryName], {
    capture: true,
    timeoutMs: remainingTimeout(30_000),
  });
  dockerExec(standbyName, plan.promote, remainingTimeout, { user: 'postgres' });
  waitForSqlValue(
    standbyName,
    'SELECT pg_is_in_recovery()::text;',
    'false',
    remainingTimeout,
    'standby promotion',
  );
  psql(
    standbyName,
    insertCanarySql(PROMOTED_CANARY_ID, 'ha-after-promotion'),
    remainingTimeout,
  );
  const result = psql(
    standbyName,
    `SELECT COUNT(*)::text FROM webserver_site WHERE tenant_id = ${CANARY_TENANT_ID} `
      + `AND id IN (${CANARY_ID}, ${PROMOTED_CANARY_ID});`,
    remainingTimeout,
    true,
  );
  if (String(result.stdout).trim() !== '2') {
    throw new Error('promoted PostgreSQL did not preserve and accept tenant data');
  }
}

export function runPostgresHa(plan = createPostgresHaPlan()) {
  const remainingTimeout = createDeadline();
  ensureCriticalSources(remainingTimeout);
  run('docker', ['version', '--format', '{{.Server.Version}}'], {
    capture: true,
    timeoutMs: remainingTimeout(30_000),
  });

  const suffix = `${process.pid}-${Date.now()}`;
  const networkName = `sdkwork-web-ha-${suffix}`;
  const primaryName = `sdkwork-web-ha-primary-${suffix}`;
  const standbyName = `sdkwork-web-ha-standby-${suffix}`;
  try {
    run('docker', ['network', 'create', '--internal', '--driver', 'bridge', networkName], {
      capture: true,
      timeoutMs: remainingTimeout(30_000),
    });
    run(
      'docker',
      [
        'run',
        '--detach',
        '--rm',
        '--name',
        primaryName,
        '--hostname',
        'primary',
        '--network',
        networkName,
        '--network-alias',
        'primary',
        ...containerResourceArgs(),
        '--env',
        `POSTGRES_DB=${DATABASE_NAME}`,
        '--env',
        `POSTGRES_USER=${DATABASE_USER}`,
        '--env',
        `POSTGRES_PASSWORD=${DATABASE_PASSWORD}`,
        plan.image,
        '-c',
        'wal_level=replica',
        '-c',
        'max_wal_senders=4',
        '-c',
        'max_replication_slots=4',
        '-c',
        'hot_standby=on',
      ],
      { capture: true, timeoutMs: remainingTimeout(5 * 60 * 1_000) },
    );
    waitForPostgres(primaryName, remainingTimeout);
    configurePrimary(primaryName, remainingTimeout);
    startStandby(standbyName, networkName, plan, remainingTimeout);
    const replayedLsn = verifyReplication(primaryName, standbyName, remainingTimeout);
    promoteStandby(primaryName, standbyName, plan, remainingTimeout);
    console.log(
      `[sdkwork-web-postgres-ha] standby replayed ${replayedLsn}, promoted, and accepted tenant writes`,
    );
  } finally {
    spawnSync('docker', ['rm', '--force', primaryName, standbyName], {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      maxBuffer: MAX_CAPTURE_BYTES,
      stdio: 'pipe',
      timeout: 30_000,
      windowsHide: true,
    });
    spawnSync('docker', ['network', 'rm', networkName], {
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
      const plan = createPostgresHaPlan();
      if (options.dryRun) {
        console.log(JSON.stringify(plan, null, 2));
      } else {
        runPostgresHa(plan);
      }
    }
  } catch (error) {
    console.error(`[sdkwork-web-postgres-ha] ${error instanceof Error ? error.message : error}`);
    process.exitCode = 1;
  }
}
