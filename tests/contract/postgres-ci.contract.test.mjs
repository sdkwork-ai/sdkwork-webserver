import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { parse } from 'yaml';

import { isPostgresReady } from '../../scripts/postgres-ci-verify.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('PostgreSQL readiness ignores the temporary initialization server', () => {
  assert.equal(isPostgresReady('database system is ready to accept connections', 0), false);
  assert.equal(
    isPostgresReady('PostgreSQL init process complete; ready for start up.', 0),
    true,
  );
  assert.equal(
    isPostgresReady('PostgreSQL init process complete; ready for start up.', 1),
    false,
  );
});

test('PostgreSQL CI runner is pinned, bounded, and executes both ignored suites', () => {
  const result = spawnSync(process.execPath, ['scripts/postgres-ci-verify.mjs', '--dry-run'], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.match(plan.image, /^postgres:16\.9-alpine3\.22@sha256:[a-f0-9]{64}$/u);
  assert.deepEqual(plan.lifecycleTest.slice(0, 3), [
    'cargo',
    'test',
    '-p',
  ]);
  assert.ok(plan.lifecycleTest.includes('postgres_baseline_seed_and_drift_are_clean'));
  assert.ok(
    plan.repositoryTest.includes(
      'postgres_repository_transactions_tenants_idempotency_and_pagination_are_bounded',
    ),
  );
  assert.ok(plan.lifecycleTest.includes('--ignored'));
  assert.ok(plan.repositoryTest.includes('--ignored'));
});

test('workflow contract makes PostgreSQL verification a PR and release gate', () => {
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));
  assert.equal(
    packageJson.scripts['test:postgres:required'],
    'node scripts/postgres-ci-verify.mjs',
  );

  const manifest = JSON.parse(readFileSync(path.join(REPO_ROOT, 'sdkwork.workflow.json'), 'utf8'));
  assert.ok(manifest.dependencies.some((dependency) => dependency.id === 'sdkwork-iam'));
  assert.ok(
    manifest.lifecycle.install.some((step) => step.run === 'pnpm install --frozen-lockfile'),
  );
  assert.ok(
    manifest.lifecycle.validate.some((step) => step.run === 'pnpm test:postgres:required'),
  );

  const workflow = parse(
    readFileSync(path.join(REPO_ROOT, '.github/workflows/package.yml'), 'utf8'),
  );
  assert.ok(Object.hasOwn(workflow.on, 'pull_request'));
  assert.ok(workflow.on.push.branches.includes('main'));
  assert.equal(
    workflow.jobs.package.uses,
    'sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@bfe420b79d3c584d246f9691c8374b24534c3365',
  );
  assert.equal(workflow.jobs.package.with.config_path, 'sdkwork.workflow.json');
  assert.equal(Object.keys(workflow.jobs).length, 1);
  assert.ok(
    manifest.lifecycle.validate.some((step) => step.run === 'git diff --exit-code -- .'),
  );
});
