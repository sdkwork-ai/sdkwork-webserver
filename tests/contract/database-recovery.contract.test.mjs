import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('database recovery drill is PostgreSQL-native, real, pinned, and bounded', () => {
  const result = spawnSync(process.execPath, ['scripts/database-recovery-verify.mjs', '--dry-run'], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.match(plan.image, /^postgres:16\.9-alpine3\.22@sha256:[a-f0-9]{64}$/u);
  assert.deepEqual(plan.limits, {
    readyAttempts: 60,
    operationTimeoutMs: 600_000,
    maxCaptureBytes: 65_536,
    maxBaselineBytes: 2_097_152,
    maxBackupBytes: 67_108_864,
    maxContainers: 1,
    containerMemoryBytes: 268_435_456,
    containerCpus: 1,
    containerPids: 256,
  });
  assert.equal(plan.sqliteTest, undefined);
  assert.ok(plan.postgres.dump.includes('pg_dump'));
  assert.ok(plan.postgres.dump.includes('--format=custom'));
  assert.ok(plan.postgres.dump.includes('--username=sdkwork_recovery'));
  assert.ok(plan.postgres.restore.includes('pg_restore'));
  assert.ok(plan.postgres.restore.includes('--exit-on-error'));
  assert.ok(plan.postgres.restore.includes('--username=sdkwork_recovery'));

  const source = readFileSync(
    path.join(REPO_ROOT, 'scripts/database-recovery-verify.mjs'),
    'utf8',
  );
  assert.match(source, /'--rm'/u);
  assert.doesNotMatch(source, /'--publish'/u);
  assert.match(source, /'--network',\s*'none'/u);
  assert.match(source, /'--memory',\s*'256m'/u);
  assert.match(source, /'--pids-limit',\s*'256'/u);
  assert.match(source, /\/var\/lib\/postgresql\/data:rw,nosuid,size=128m/u);
  assert.match(source, /options\.input === undefined \? \[\] : \['--interactive'\]/u);
  assert.match(source, /sha256sum/u);
  assert.match(source, /\['stat', '-c', '%s'/u);
  assert.match(source, /finally\s*\{/u);
  assert.match(source, /docker', \['rm', '--force'/u);
});

test('database recovery drill is mandatory in root and release verification', () => {
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));
  assert.equal(
    packageJson.scripts['test:database:recovery'],
    'node scripts/database-recovery-verify.mjs',
  );
  assert.equal(packageJson.scripts.test, 'pnpm exec sdkwork-app test');
  assert.equal(packageJson.scripts.verify, 'pnpm exec sdkwork-app verify');
  assert.match(packageJson.scripts['test:contracts'], /tests\/contract\/\*\.contract\.test\.mjs/u);
  assert.match(packageJson.scripts['_sdkwork:test'], /pnpm test:contracts/u);
  assert.match(packageJson.scripts['_sdkwork:verify'], /pnpm run _sdkwork:test/u);

  const workflow = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'agent.workflow.json'), 'utf8'),
  );
  assert.ok(
    workflow.lifecycle.validate.some((step) => step.run === 'pnpm test:database:recovery'),
  );
  const recoveryIndex = workflow.lifecycle.validate.findIndex(
    (step) => step.run === 'pnpm test:database:recovery',
  );
  const driftIndex = workflow.lifecycle.validate.findIndex(
    (step) => step.run === 'git diff --exit-code -- .',
  );
  assert.ok(recoveryIndex >= 0 && driftIndex > recoveryIndex);
});
