import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function source(relativePath) {
  return readFileSync(path.join(REPO_ROOT, relativePath), 'utf8');
}

test('node sync repository streams bounded projections before materialization', () => {
  const repository = source(
    'crates/sdkwork-intelligence-webserver-repository-sqlx/src/agents.rs',
  );
  assert.match(repository, /const MAX_NODE_SYNC_ITEMS: usize = 2_048/u);
  assert.match(
    repository,
    /const MAX_NODE_SYNC_BUNDLE_BYTES: usize = 12 \* 1024 \* 1024/u,
  );
  assert.match(repository, /MAX_NODE_NGINX_CONFIG_BYTES: i64 = 1024 \* 1024/u);
  assert.match(repository, /\.fetch\(&self\.pool\)/u);
  assert.match(repository, /\.try_next\(\)/u);
  assert.match(repository, /CASE WHEN \{content_size\}/u);
  assert.match(repository, /web_listener_certificate_binding/u);
  assert.match(repository, /web_certificate_version/u);
  assert.match(repository, /v\.secret_bundle_ref/u);
  assert.match(repository, /reserve_with_additional_bytes/u);
  // Every runtime-assignment query remains bounded: the only `fetch_all` on the
  // pool is the LIMIT 5 current-assignments lookup; content projections stream
  // through `fetch` + `try_next` with hard item/byte budgets.
  assert.match(repository, /LIMIT 5/u);
  assert.doesNotMatch(repository, /fetch_all\(&self\.pool\)\.await[^;]*;\)/u);
});

test('node sync service and daemon retain independent final response bounds', () => {
  const repositorySource = source(
    'crates/sdkwork-intelligence-webserver-repository-sqlx/src/agents.rs',
  );
  const service = source(
    'crates/sdkwork-intelligence-webserver-service/src/agent_ops.rs',
  );
  const daemon = source('crates/sdkwork-webserver-agent/src/main.rs');
  assert.match(
    service,
    /const MAX_NODE_SYNC_RESPONSE_BYTES: usize = 15 \* 1024 \* 1024/u,
  );
  // Certificate/secret-bundle pairing is fail-closed at the repository: each
  // manifest certificate joins an encrypted secret bundle and decryption
  // failure rejects the sync; the service forwards the bounded manifest.
  assert.match(
    repositorySource,
    /INNER JOIN web_certificate_secret_bundle/u,
  );
  assert.match(repositorySource, /decrypt_certificate_secret_bundle/u);
  assert.match(service, /serde_json::to_vec\(manifest\)/u);
  assert.match(daemon, /const MAX_SYNC_RESPONSE_BYTES: usize = 16 \* 1024 \* 1024/u);
  assert.match(daemon, /node identity mismatch between heartbeat acknowledgement and sync manifest/u);
  assert.match(daemon, /duplicate Nginx activation domain/u);
  assert.match(daemon, /duplicate certificate activation name/u);
  assert.match(daemon, /Nginx configuration fingerprint mismatch/u);
});

test('node sync manifest contract is mandatory in root test and verify', () => {
  const packageJson = JSON.parse(source('package.json'));
  assert.equal(packageJson.scripts.test, 'pnpm exec sdkwork-app test');
  assert.equal(packageJson.scripts.verify, 'pnpm exec sdkwork-app verify');
  assert.match(packageJson.scripts['test:contracts'], /tests\/contract\/\*\.contract\.test\.mjs/u);
  assert.match(packageJson.scripts['_sdkwork:test'], /pnpm test:contracts/u);
  assert.match(packageJson.scripts['_sdkwork:verify'], /pnpm run _sdkwork:test/u);
});
