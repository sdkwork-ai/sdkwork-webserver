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
