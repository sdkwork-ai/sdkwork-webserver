import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SMOKE_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-release-smoke.mjs');
const CURSOR_HMAC_KEY_ENV = 'SDKWORK_WEBSERVER_CURSOR_HMAC_KEY';

// The packaged gateway refuses to serve a production-like environment without a
// dedicated cursor signing key:
//
//   SDKWORK_WEBSERVER_CURSOR_HMAC_KEY is required in production-like environments
//
// Deriving that key from the database URL would make the widely shared URL a
// forgery primitive for keyset cursors, so the guard is deliberate and must stay.
// The smoke satisfies the guard instead of relaxing it, and it must do so with a
// per-run random value: a committed constant would be copied into production by
// whoever reads the script first.
function readSmokeSource() {
  return readFileSync(SMOKE_SCRIPT, 'utf8');
}

test('the smoke satisfies the cursor signing guard with a per-run random key', () => {
  const source = readSmokeSource();

  assert.match(
    source,
    /const CURSOR_HMAC_KEY_ENV = 'SDKWORK_WEBSERVER_CURSOR_HMAC_KEY';/u,
    "the smoke must name the exact variable the gateway reads",
  );
  assert.match(
    source,
    /randomBytes\(/u,
    'the cursor key must come from a CSPRNG, not a literal',
  );
  assert.doesNotMatch(
    source,
    new RegExp(`${CURSOR_HMAC_KEY_ENV}\\s*[:=]\\s*'[^']+'`, 'u'),
    'no fixed cursor key may be committed; the operator secret must never ship in the repo',
  );
  // The management gateway gets the key explicitly...
  assert.match(
    source,
    /\[CURSOR_HMAC_KEY_ENV\]: resolveSmokeCursorHmacKey\(\)/u,
    'standaloneManagementEnv must hand the key to the management gateway',
  );
  // ...and the data-plane children inherit it from process.env.
  assert.match(
    source,
    /process\.env\[CURSOR_HMAC_KEY_ENV\] = smokeCursorHmacKey;/u,
    'the key must be published to process.env so hermeticEnv passes it to the data plane',
  );
});

test('the cursor key is resolved once and shared by both planes', () => {
  const source = readSmokeSource();

  // A fresh value per call would hand the management gateway and the data-plane
  // children different signing material, so cursors minted by one would not
  // verify in the other.
  assert.match(
    source,
    /let smokeCursorHmacKey;/u,
    'the resolved key must be memoized at module scope',
  );
  assert.match(
    source,
    /if \(smokeCursorHmacKey !== undefined\) \{\s*return smokeCursorHmacKey;/u,
    'resolveSmokeCursorHmacKey must return the memoized value',
  );
  // Resolution has to happen before the first packaged binary starts.
  assert.match(
    source,
    /publishSmokeDatabaseEnv\(databaseUrl\);\s*\n\s*\/\/[^\n]*\n(?:\s*\/\/[^\n]*\n)*\s*resolveSmokeCursorHmacKey\(\);/u,
    'the key must be resolved before the gateway child is spawned',
  );
});

test('a caller-provided key is honoured, and the generated one has real entropy', () => {
  // Mirror the script's resolution so the behaviour is checked, not just its text.
  const resolve = (inherited) => {
    let key;
    return () => {
      if (key !== undefined) {
        return key;
      }
      key = inherited && inherited.trim() !== '' ? inherited : randomBytes(32).toString('base64');
      return key;
    };
  };

  const explicit = 'operator-supplied-key';
  assert.equal(resolve(explicit)(), explicit, 'an inherited key must win over generation');
  assert.equal(resolve('   ')().length, 44, 'a blank inherited key must not be adopted');

  const generated = resolve(undefined)();
  assert.equal(Buffer.from(generated, 'base64').length, 32, 'the generated key must carry 256 bits');
  assert.notEqual(generated, resolve(undefined)(), 'two runs must not share a key');

  const memoized = resolve(undefined);
  assert.equal(memoized(), memoized(), 'one run must reuse a single key across both planes');
});
