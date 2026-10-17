import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SMOKE_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-release-smoke.mjs');

function readSmokeSource() {
  return readFileSync(SMOKE_SCRIPT, 'utf8');
}

test('release smoke provisions its own PostgreSQL when none is configured', () => {
  const source = readSmokeSource();
  // CI has no `.env.postgres`; the only PostgreSQL the workflow starts belongs to
  // a later validate step and is destroyed when that step ends. Without a
  // self-provisioned database the smoke failed with
  // "serve-time database lifecycle bootstrap failed: ... pool timed out".
  assert.match(
    source,
    /async function bindSmokeDatabase\(\)/u,
    'the smoke must expose a database provisioning entry point',
  );
  assert.match(
    source,
    /'docker',[\s\S]{0,120}'run', '--detach', '--rm'/u,
    'the smoke must start a detached disposable PostgreSQL container',
  );
  assert.match(
    source,
    /function removeSmokePostgres\(\)/u,
    'the smoke must remove the container it started',
  );
  assert.match(
    source,
    /removeSmokePostgres\(\);[\s\S]{0,200}rmSync\(temporaryRoot/u,
    'the disposable database must be removed in the smoke cleanup block',
  );
});

test('release smoke prefers an externally configured database', () => {
  const source = readSmokeSource();
  // A developer machine or an operator-provided URL must win, so the smoke never
  // shadows a real database with a throwaway container.
  assert.match(
    source,
    /SDKWORK_DATABASE_TEST_POSTGRES_URL[\s\S]{0,200}SDKWORK_SMOKE_POSTGRES_URL/u,
    'the smoke must honour an externally supplied database URL',
  );
  const bind = source.slice(source.indexOf('async function bindSmokeDatabase'));
  assert.match(
    bind.slice(0, bind.indexOf("'docker', ['version'")),
    /return activeSmokeDatabase;/u,
    'a configured database must be returned before any container is started',
  );
});

test('release smoke does not mistake the derived fallback URL for a real database', () => {
  const source = readSmokeSource();
  // `resolveIamDevEnv` synthesises a hard-coded URL when `.env.postgres` is
  // absent, so a non-empty check alone reports a database with nothing listening
  // on it. That regressed the smoke into skipping its own PostgreSQL and failing
  // with a pool timeout, so the derived URL has to survive a TCP probe.
  assert.match(
    source,
    /function probeReachableDatabase\(databaseUrl\)/u,
    'the derived database URL must be probed before it is trusted',
  );
  assert.match(
    source,
    /net\.connect\(\{ host, port \}\)/u,
    'the probe must open a TCP connection to the database endpoint',
  );
  const resolve = source.slice(source.indexOf('async function resolveSmokeDatabaseUrl'));
  assert.match(
    resolve,
    /await probeReachableDatabase\(derived\.trim\(\)\)/u,
    'the derived URL must be probed for reachability',
  );
  const explicitResolver = source.slice(
    source.indexOf('function resolveExplicitSmokeDatabaseUrl'),
    source.indexOf('function probeReachableDatabase'),
  );
  assert.match(
    explicitResolver,
    /const explicit = process\.env\.SDKWORK_DATABASE_TEST_POSTGRES_URL[\s\S]*\|\|[\s\S]*process\.env\.SDKWORK_SMOKE_POSTGRES_URL/u,
    'the explicit resolver must read both exported URL variables',
  );
  assert.match(
    source.slice(source.indexOf('async function resolveSmokeDatabaseUrl')),
    /const explicit = resolveExplicitSmokeDatabaseUrl\(\);\s*if \(explicit\) \{\s*return explicit;/u,
    'an exported URL must be returned without a probe',
  );
  assert.match(
    resolve,
    /return undefined;/u,
    'an unreachable derived URL must fall through to the disposable database',
  );
});

test('release smoke points the packaged gateway at the provisioned database', () => {
  const source = readSmokeSource();
  // A provisioned database that is not handed to the gateway is useless: the
  // child boots with SDKWORK_DATABASE_AUTO_MIGRATE and would still time out.
  assert.match(
    source,
    /const databaseUrl = await bindSmokeDatabase\(\);/u,
    'the smoke must bind the database before booting the gateway',
  );
  assert.match(
    source,
    /env: standaloneManagementEnv\(packageRoot, port, runtimeConfigFile, databaseUrl\)/u,
    'the serve-management child must receive the provisioned database URL',
  );
  assert.match(
    source,
    /publishSmokeDatabaseEnv\(databaseUrl\);/u,
    'the data-plane child must also resolve the provisioned database',
  );
  assert.match(
    source,
    /SDKWORK_DATABASE_URL: databaseUrl/u,
    'the provisioned URL must use the supported SDKWORK_DATABASE_URL key',
  );
});
