import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('PostgreSQL HA drill is real, pinned, isolated, and bounded', () => {
  const result = spawnSync(process.execPath, ['scripts/postgres-ha-verify.mjs', '--dry-run'], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.match(plan.image, /^postgres:16\.9-alpine3\.22@sha256:[a-f0-9]{64}$/u);
  assert.deepEqual(plan.limits, {
    operationTimeoutMs: 600_000,
    readyAttempts: 60,
    convergenceAttempts: 60,
    maxCaptureBytes: 65_536,
    maxBaselineBytes: 2_097_152,
    maxContainers: 2,
    maxNetworks: 1,
    memoryBytesPerContainer: 268_435_456,
    cpusPerContainer: 1,
    pidsPerContainer: 256,
    dataTmpfsBytesPerContainer: 134_217_728,
  });
  assert.ok(plan.baseBackup.includes('pg_basebackup'));
  assert.ok(plan.baseBackup.includes('--write-recovery-conf'));
  assert.ok(plan.baseBackup.includes('--wal-method=stream'));
  assert.ok(plan.baseBackup.includes('--slot=sdkwork_web_ha_slot'));
  assert.ok(plan.promote.includes('pg_ctl'));
  assert.ok(plan.promote.includes('promote'));
  assert.ok(plan.promote.includes('--wait'));

  const source = readFileSync(path.join(REPO_ROOT, 'scripts/postgres-ha-verify.mjs'), 'utf8');
  assert.match(source, /'network', 'create', '--internal'/u);
  assert.doesNotMatch(source, /'--publish'/u);
  assert.doesNotMatch(source, /'--volume'/u);
  assert.match(source, /'--memory',\s*'256m'/u);
  assert.match(source, /'--pids-limit',\s*'256'/u);
  assert.match(source, /pg_stat_replication WHERE state = 'streaming'/u);
  assert.match(source, /pg_last_wal_replay_lsn\(\) >=/u);
  assert.match(source, /\['stop', '--time', '10', primaryName\]/u);
  assert.ok(source.indexOf("['stop', '--time', '10', primaryName]") < source.indexOf('plan.promote'));
  assert.match(source, /spawnSync\('docker', \['rm', '--force', primaryName, standbyName\]/u);
  assert.match(source, /spawnSync\('docker', \['network', 'rm', networkName\]/u);
});

test('PostgreSQL HA drill is mandatory in root and release verification', () => {
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));
  assert.equal(
    packageJson.scripts['test:postgres:ha'],
    'node scripts/postgres-ha-verify.mjs',
  );
  assert.equal(packageJson.scripts.test, 'pnpm exec sdkwork-app test');
  assert.equal(packageJson.scripts.verify, 'pnpm exec sdkwork-app verify');
  assert.match(packageJson.scripts['test:contracts'], /tests\/contract\/\*\.contract\.test\.mjs/u);
  assert.match(packageJson.scripts['_sdkwork:test'], /pnpm test:contracts/u);
  assert.match(packageJson.scripts['_sdkwork:verify'], /pnpm run _sdkwork:test/u);

  const workflow = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'agent.workflow.json'), 'utf8'),
  );
  const haIndex = workflow.lifecycle.validate.findIndex(
    (step) => step.run === 'pnpm test:postgres:ha',
  );
  const driftIndex = workflow.lifecycle.validate.findIndex(
    (step) => step.run === 'git diff --exit-code -- .',
  );
  assert.ok(haIndex >= 0 && driftIndex > haIndex);
});
