import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SMOKE_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-release-smoke.mjs');

function readSmokeSource() {
  return readFileSync(SMOKE_SCRIPT, 'utf8');
}

function readMaxResponseBytes() {
  const source = readSmokeSource();
  const match = source.match(/^const MAX_RESPONSE_BYTES = (.+);$/mu);
  assert.ok(match, 'the smoke must declare MAX_RESPONSE_BYTES');
  // Evaluate only the arithmetic, never arbitrary source.
  assert.doesNotMatch(match[1], /[^\d\s*+()]/u, 'the bound must be plain arithmetic');
  // eslint-disable-next-line no-new-func
  return Function(`return (${match[1]});`)();
}

test('a response-limit failure names the route that produced it', () => {
  const source = readSmokeSource();

  // "smoke response exceeds 65536 bytes" cannot be attributed to any of the
  // same-origin routes, which is what left the last failure unattributable.
  assert.match(
    source,
    /`smoke response for \$\{requestPath\} exceeds \$\{MAX_RESPONSE_BYTES\} bytes`/u,
    'the HTTP helper must name the requested path',
  );
  assert.match(
    source,
    /`smoke \$\{protocol\} health response exceeds \$\{MAX_RESPONSE_BYTES\} bytes`/u,
    'the health helper must name the protocol it was probing',
  );
  assert.doesNotMatch(
    source,
    /new Error\(`smoke response exceeds \$\{MAX_RESPONSE_BYTES\} bytes`\)/u,
    'the unattributable message must be gone from both helpers',
  );
});

test('the response bound can hold the composite OpenAPI document', () => {
  const bound = readMaxResponseBytes();

  // The smoke JSON.parses /openapi.json, which the gateway composes from every
  // mounted owner surface. A 64 KiB ceiling truncated it into a parse failure
  // rather than a readable diagnostic.
  assert.ok(
    bound >= 1024 * 1024,
    `the bound must leave room for a multi-surface OpenAPI document, got ${bound}`,
  );
  assert.ok(
    bound <= 64 * 1024 * 1024,
    `the bound must stay a bound rather than accepting an unbounded stream, got ${bound}`,
  );
  assert.equal(bound, 8 * 1024 * 1024, 'the documented 8 MiB is the intended value');
});

test('the OpenAPI assertion still needs a whole document', () => {
  const source = readSmokeSource();
  // If a future change makes the smoke stop parsing the document, the larger
  // bound stops being justified and this test should be revisited.
  assert.match(
    source,
    /const openapiDocument = JSON\.parse\(openapi\.body\);/u,
    'the smoke parses the composite OpenAPI, which is why the bound had to grow',
  );
});
