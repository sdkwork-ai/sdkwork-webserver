import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SIGN_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-sign.mjs');

// A 502 from the transparency log is not a defect in the artifact, but it
// failed a whole platform job: `fetchOnConflict` settles the deterministic 409
// and says nothing about Rekor being briefly unavailable. These assertions pin
// the backoff that was added in response, so it cannot be quietly dropped.

function readNormalized(...segments) {
  return readFileSync(path.join(...segments), 'utf8').replace(/\r\n/gu, '\n');
}

// The production classifier is executed, not restated: a fixture that
// reimplemented the status-code rules would keep passing after the real one
// regressed.
function loadTransientStatusOf() {
  const source = readNormalized(SIGN_SCRIPT);
  const start = source.indexOf('const TRANSIENT_STATUS_CODES =');
  const end = source.indexOf('\n}\n', source.indexOf('function transientStatusOf('));
  assert.ok(start !== -1 && end !== -1, 'the sign script must declare the transient classifier');
  const context = vm.createContext({ Error, Number, String });
  vm.runInContext(
    `${source.slice(start, end + 3)}\nglobalThis.__classify = transientStatusOf;`,
    context,
  );
  return context.__classify;
}

test('the transient classifier separates retryable service failures from real rejections', () => {
  const classify = loadTransientStatusOf();

  for (const status of [408, 425, 429, 500, 502, 503, 504]) {
    assert.equal(
      classify({ status }),
      status,
      `${status} is a transient service failure and must be retried`,
    );
    assert.equal(
      classify(new Error(`sigstore: (${status}) Bad Gateway`)),
      status,
      `${status} also arrives as message text when fetch does not preserve the response`,
    );
  }

  // A rejected artifact, a bad identity or a missing tlog entry must fail on
  // the first attempt: retrying those only delays a real error.
  for (const status of [400, 401, 403, 404, 409, 422]) {
    assert.equal(
      classify({ status }),
      null,
      `${status} is a real rejection and must not be retried`,
    );
  }

  assert.equal(classify(new Error('certificate identity mismatch')), null);
  assert.equal(classify(undefined), null);
});

test('signing and verification both retry, and the retry budget is bounded', () => {
  const source = readNormalized(SIGN_SCRIPT);

  assert.match(
    source,
    /const SIGN_RETRY_ATTEMPTS = \d+;/u,
    'the retry budget must be an explicit constant, not an inline literal',
  );
  assert.match(
    source,
    /const SIGN_RETRY_BASE_DELAY_MS = [\d_]+;/u,
    'the backoff base delay must be an explicit constant',
  );

  // Both network calls need the backoff. Signing was what actually failed in
  // CI; verification reads the same log, so leaving it bare would just move
  // the flaky failure one stage later. The declaration is excluded so the count
  // reflects real call sites only.
  const callSites = [...source.matchAll(/(?<!function )withTransientRetry\(/gu)].length;
  assert.equal(
    callSites,
    2,
    'exactly two call sites are expected: builder.create and verify',
  );
  assert.match(
    source,
    /withTransientRetry\(\s*\(\)\s*=>\s*builder\.create\(\{ data: payload \}\)/u,
    'bundle creation must go through the retry helper',
  );
  assert.match(
    source,
    /withTransientRetry\(\s*\(\)\s*=>\s*verify\(/u,
    'bundle verification must go through the retry helper',
  );

  // Bounded budget: an unbounded or single-attempt loop turns a service blip
  // into either a hang or a pointless retry.
  assert.match(
    source,
    /attempt >= SIGN_RETRY_ATTEMPTS/u,
    'the loop must stop at the configured attempt budget',
  );
  assert.match(
    source,
    /if \(status === null \|\| attempt >= SIGN_RETRY_ATTEMPTS\) \{\s*throw error;/u,
    'a non-transient failure must be rethrown without waiting',
  );
  assert.match(
    source,
    /SIGN_RETRY_BASE_DELAY_MS \* 2 \*\* \(attempt - 1\)/u,
    'the backoff must grow exponentially so a sustained outage is not hammered',
  );
});
