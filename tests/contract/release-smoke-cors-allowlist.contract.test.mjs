import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SMOKE_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-release-smoke.mjs');
const CORS_ENV = 'SDKWORK_CORS_ALLOWED_ORIGINS';

// The hermetic smoke profile runs `environment = "production"`, and the gateway
// refuses to start there without an explicit allowlist:
//
//   thread 'main' panicked at crates/sdkwork-webserver-http-host/src/lib.rs:188:33:
//   Web Framework security configuration is invalid: production-like Web HTTP
//   runtime requires SDKWORK_CORS_ALLOWED_ORIGINS
//
// web_security_policy is covered by a unit test that also rejects wildcards in
// production, so this is a deliberate guard rather than a rough edge. The smoke
// satisfies it with the one origin it can legitimately serve.

function readSmokeSource() {
  return readFileSync(SMOKE_SCRIPT, 'utf8');
}

// Run the real `standaloneManagementEnv` so a runtime fault in the original
// turns this test red. A previous contract file reimplemented the function and
// passed while the shipped code threw; the extraction is what closes that gap.
function loadStandaloneManagementEnv() {
  const source = readSmokeSource();
  const start = source.indexOf('function standaloneManagementEnv(');
  assert.notEqual(start, -1, 'the smoke script must still declare standaloneManagementEnv');
  const end = source.indexOf('\nfunction ', start + 1);
  const fn = source.slice(start, end === -1 ? undefined : end);

  // Lift the constants the function closes over instead of restating them, so the
  // harness cannot drift from the script it is exercising.
  const lifted = [...source.matchAll(/^const STANDALONE_(?:PC|H5|STATIC_FALLBACK|IAM|DRIVE)_ROOT = '[^']*';$/gmu)]
    .map((match) => match[0])
    .join('\n');
  const constants = [
    lifted,
    "const CURSOR_HMAC_KEY_ENV = 'SDKWORK_WEBSERVER_CURSOR_HMAC_KEY';",
    "const SMOKE_POSTGRES_DATABASE = 'sdkwork_ai_test_release_smoke';",
    "const SMOKE_POSTGRES_USER = 'sdkwork_ai_test';",
    'const IAM_APPLICATION_BOOTSTRAP_ENV = {};',
    'function canonicalizeWorkspaceDatabaseEnv(value) { return value; }',
    'function resolveIamDevEnv() { return {}; }',
    'function resolveSmokeCursorHmacKey() { return process.env[CURSOR_HMAC_KEY_ENV] ?? "smoke-key"; }',
  ].join('\n');
  assert.match(lifted, /STANDALONE_IAM_ROOT/u, 'the smoke must still declare its IAM root');

  const context = vm.createContext({
    Buffer,
    path: { join: (...parts) => parts.join('/').replace(/\\/gu, '/') },
    process: { env: {} },
  });
  vm.runInContext(`${constants}\n${fn}\nglobalThis.__env = standaloneManagementEnv;`, context);
  return context.__env;
}

test('the smoke gives the production-like gateway an exact CORS allowlist', () => {
  const source = readSmokeSource();

  assert.match(
    source,
    /const origins = process\.env\.SDKWORK_CORS_ALLOWED_ORIGINS\s*\n?\s*\?\? `http:\/\/127\.0\.0\.1:\$\{port\}`;/u,
    'the allowlist must default to the exact loopback origin the smoke serves',
  );
  assert.match(
    source,
    /SDKWORK_CORS_ALLOWED_ORIGINS: origins,/u,
    'standaloneManagementEnv must hand the allowlist to the gateway',
  );
  // The guard rejects wildcards in production, so a wildcard default would trade
  // one panic for another.
  assert.doesNotMatch(
    source,
    /SDKWORK_CORS_ALLOWED_ORIGINS\s*\n?\s*\?\?[^;]*\*/u,
    'the default allowlist must not contain a wildcard; production rejects those',
  );
});

test('the real standaloneManagementEnv exports a loopback origin matching its port', () => {
  const env = loadStandaloneManagementEnv();
  const port = 41234;
  const result = env('/pkg', port, '/tmp/config.toml', 'postgresql://user@127.0.0.1:5432/db');

  assert.equal(
    result[CORS_ENV],
    `http://127.0.0.1:${port}`,
    'the allowlist must name the port the smoke actually reserved',
  );
  assert.ok(
    !result[CORS_ENV].includes('*'),
    'a wildcard would be rejected by the very guard this satisfies',
  );
  assert.ok(
    result[CORS_ENV].startsWith('http://'),
    'the smoke serves plain HTTP on loopback, so the origin scheme has to match',
  );
});

test('an operator-supplied allowlist is not overridden', () => {
  const source = readSmokeSource();
  // A real deployment may already export its own list; the smoke must not silently
  // replace it with loopback, and must not silently drop it either.
  assert.match(
    source,
    /process\.env\.SDKWORK_CORS_ALLOWED_ORIGINS\s*\n?\s*\?\?/u,
    'the inherited value must win over the smoke default',
  );
  assert.doesNotMatch(
    source,
    /SDKWORK_CORS_ALLOWED_ORIGINS:\s*(?:origins\s*\?\?|process\.env)/u,
    'an empty inherited value must not erase the allowlist entirely',
  );
});
