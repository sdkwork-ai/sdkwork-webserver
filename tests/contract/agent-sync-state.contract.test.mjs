import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('node daemon sync state is durable, checksummed, bounded, and fail-closed', () => {
  const state = readFileSync(
    path.join(REPO_ROOT, 'crates/sdkwork-webserver-agent/src/state.rs'),
    'utf8',
  );
  assert.match(state, /const MAX_STATE_BYTES: u64 = 1024 \* 1024/u);
  assert.match(state, /SYNC_VERSION_PREFIX: &str = "sv1:"/u);
  assert.match(state, /struct StateChecksumPayload/u);
  assert.match(state, /sdkwork_utils_rust::crypto::sha256_hash/u);
  assert.match(state, /NamedTempFile::new_in/u);
  assert.match(state, /staged\.as_file\(\)\.sync_all\(\)/u);
  assert.match(state, /reject_symlink_ancestors/u);
  assert.match(state, /permissions\.set_mode\(0o600\)/u);
  assert.match(state, /sdkwork-webserver-node-daemon\.lock/u);
  assert.match(state, /file\.try_lock\(\)/u);
  assert.match(state, /another Web Node Daemon already owns this state directory/u);
  assert.match(state, /\/var\/lib\/sdkwork\/webserver\/edge/u);
  assert.doesNotMatch(state, /std::env::temp_dir/u);
  assert.doesNotMatch(state, /unwrap_or_default\(\)/u);
});

test('node daemon persists desired before apply and observed only after real reload', () => {
  const source = readFileSync(
    path.join(REPO_ROOT, 'crates/sdkwork-webserver-agent/src/main.rs'),
    'utf8',
  );
  const desiredIndex = source.indexOf('local_state.with_desired');
  const desiredSaveIndex = source.indexOf('desired_state.save');
  const deployIndex = source.indexOf('activate_deployment_async');
  const observedIndex = source.indexOf('local_state.with_observed');
  const observedSaveIndex = source.indexOf('observed_state.save');
  const processLockIndex = source.indexOf('NodeDaemonLock::acquire');
  const stateLoadIndex = source.indexOf('NodeDaemonState::load');
  assert.ok(processLockIndex >= 0 && processLockIndex < stateLoadIndex);
  assert.ok(desiredIndex >= 0 && desiredIndex < desiredSaveIndex);
  assert.ok(desiredSaveIndex < deployIndex);
  assert.ok(deployIndex < observedIndex && observedIndex < observedSaveIndex);
  assert.match(
    source,
    /last_sync_version: local_state\.observed_sync_version\(\)/u,
  );
  assert.match(source, /MAX_SYNC_RESPONSE_BYTES: usize = 16 \* 1024 \* 1024/u);
  assert.match(source, /MAX_NGINX_CONFIGS_PER_SYNC: usize = 2_048/u);
  assert.match(source, /SdkworkBackendClient/u);
  assert.match(source, /config\.max_response_body_bytes = maximum_response_bytes/u);
  assert.match(source, /client\.set_agent_token/u);
  assert.match(source, /\.heartbeat\(&heartbeat\)\.await/u);
  assert.match(source, /\.retrieve\(local_state\.observed_sync_version\(\)\)/u);
  assert.doesNotMatch(source, /reqwest::/u);
  assert.doesNotMatch(source, /response\.json\(\)\.await/u);
  assert.doesNotMatch(
    source,
    /let manifest: AgentSyncResponse\s*=\s*serde_json::from_slice/u,
  );
});

test('node daemon state contract is present in root verification and component metadata', () => {
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));
  assert.equal(packageJson.scripts.test, 'pnpm exec sdkwork-app test');
  assert.equal(packageJson.scripts.verify, 'pnpm exec sdkwork-app verify');
  assert.match(packageJson.scripts['test:contracts'], /tests\/contract\/\*\.contract\.test\.mjs/u);
  assert.match(packageJson.scripts['_sdkwork:test'], /pnpm test:contracts/u);
  assert.match(packageJson.scripts['_sdkwork:verify'], /pnpm run _sdkwork:test/u);

  const component = JSON.parse(
    readFileSync(
      path.join(REPO_ROOT, 'crates/sdkwork-webserver-agent/specs/component.spec.json'),
      'utf8',
    ),
  );
  assert.ok(component.contracts.configKeys.includes('SDKWORK_WEBSERVER_AGENT_STATE_PATH'));
  assert.ok(component.contracts.configKeys.includes('SDKWORK_WEBSERVER_AGENT_STATE_DIR'));
  assert.ok(component.contracts.configKeys.includes('SDKWORK_WEBSERVER_NODE_TOKEN'));
  assert.ok(component.contracts.configKeys.includes('SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS'));
  assert.ok(component.contracts.configKeys.includes('SDKWORK_WEBSERVER_NODE_STATE_PATH'));
  assert.ok(component.contracts.configKeys.includes('SDKWORK_WEBSERVER_NODE_STATE_DIR'));
  assert.ok(component.contracts.runtimeEntrypoints.includes('binary#sdkwork-webserver-node-daemon'));
  assert.ok(component.contracts.runtimeEntrypoints.includes('binary#sdkwork-webserver-agent'));
  assert.ok(component.contracts.configKeys.includes('SDKWORK_WEBSERVER_EDGE_ROOT'));
});

test('web node daemon terminology is preferred without breaking v3 aliases', () => {
  const main = readFileSync(
    path.join(REPO_ROOT, 'crates/sdkwork-webserver-agent/src/main.rs'),
    'utf8',
  );
  const state = readFileSync(
    path.join(REPO_ROOT, 'crates/sdkwork-webserver-agent/src/state.rs'),
    'utf8',
  );
  const env = readFileSync(
    path.join(REPO_ROOT, 'etc/node-daemon/development.env.example'),
    'utf8',
  );
  assert.match(main, /SDKWORK_WEBSERVER_NODE_TOKEN/u);
  assert.match(main, /SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS/u);
  assert.match(main, /conflicts with legacy alias/u);
  assert.match(state, /SDKWORK_WEBSERVER_NODE_STATE_PATH/u);
  assert.match(state, /SDKWORK_WEBSERVER_NODE_STATE_DIR/u);
  assert.match(env, /^SDKWORK_WEBSERVER_NODE_TOKEN=/mu);
  assert.match(env, /^SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS=/mu);
  assert.doesNotMatch(env, /^SDKWORK_WEBSERVER_AGENT_TOKEN=/mu);
  assert.doesNotMatch(env, /^SDKWORK_WEBSERVER_AGENT_SYNC_INTERVAL_SECS=/mu);
});
