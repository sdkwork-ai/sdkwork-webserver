import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';
import vm from 'node:vm';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SMOKE_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-release-smoke.mjs');

// A packaged gateway prints one `sqlx::postgres::notice` line per migration
// statement. On this release that filled the whole 256 KiB budget with "already
// exists, skipping" before the process printed why it died, so the captured
// output ended mid-notice and Validate failed with a cause nobody could read.
// Retention has to keep the tail; the head alone is a diagnostic dead end.
//
// These tests evaluate the real `captureBounded` lifted out of the script rather
// than a copy. An earlier version of this file reimplemented the function and
// passed, while the real one threw `TypeError: Assignment to constant variable`
// on the second chunk and took the release down. A duplicate cannot drift from
// its original, but an extracted original still can, so it is executed.
function loadCaptureBounded() {
  const source = readFileSync(SMOKE_SCRIPT, 'utf8');
  const constants = [...source.matchAll(/^const (MAX_PROCESS_OUTPUT_BYTES|PROCESS_OUTPUT_HEAD_BYTES|PROCESS_OUTPUT_TAIL_BYTES) = (.*);$/gmu)]
    .map((match) => `${match[1]} = ${match[2]};`)
    .join('\n');
  const start = source.indexOf('function captureBounded(stream) {');
  assert.notEqual(start, -1, 'the smoke script must still declare captureBounded');
  const end = source.indexOf('\nfunction ', start + 1);
  const fn = source.slice(start, end === -1 ? undefined : end);

  const context = vm.createContext({ Buffer, console });
  vm.runInContext(`${constants}\n${fn}\nglobalThis.__captureBounded = captureBounded;`, context);
  return {
    captureBounded: context.__captureBounded,
    maxBytes: context.MAX_PROCESS_OUTPUT_BYTES,
  };
}

function chattyStream(notices) {
  return Readable.from(
    (function* emit() {
      for (let index = 0; index < notices; index += 1) {
        yield `migration notice line ${index} already exists, skipping\n`;
      }
      yield 'ERROR: the real reason the process died\n';
    })(),
  );
}

test('the smoke budgets process output so the failure reason survives', () => {
  const source = readFileSync(SMOKE_SCRIPT, 'utf8');

  assert.match(
    source,
    /const PROCESS_OUTPUT_TAIL_BYTES = MAX_PROCESS_OUTPUT_BYTES - PROCESS_OUTPUT_HEAD_BYTES;/u,
    'the tail budget must be derived from the total, not a second magic number',
  );
  assert.match(
    source,
    /tail = tail\.subarray\(tail\.length - PROCESS_OUTPUT_TAIL_BYTES\);/u,
    'the tail must be trimmed from the front, which is the only way to keep the last lines',
  );
  assert.doesNotMatch(
    source,
    /if \(retainedBytes >= MAX_PROCESS_OUTPUT_BYTES\) \{\s*return;/u,
    'the head-only retention that dropped the failure reason must be gone',
  );
});

test('the real captureBounded runs and keeps the tail of a chatty process', async () => {
  const { captureBounded, maxBytes } = loadCaptureBounded();
  const notices = 20_000;

  const read = captureBounded(chattyStream(notices));
  await new Promise((resolve) => setTimeout(resolve, 100));

  const output = read();
  assert.ok(
    output.includes('ERROR: the real reason the process died'),
    'the last line the process printed is the diagnosis and must be retained',
  );
  assert.ok(output.includes('migration notice line 0 '), 'the head is kept for context');
  assert.ok(output.includes(`migration notice line ${notices - 1} `), 'the tail is kept whole');
  assert.ok(
    Buffer.byteLength(output) <= maxBytes + 128,
    `retention must stay within the declared budget (${Buffer.byteLength(output)} bytes)`,
  );
  assert.match(output, /output truncated/u, 'a truncated capture has to admit it');
});

test('the real captureBounded never reassigns a binding it declared const', async () => {
  // Regression guard for the exact defect that shipped: `const incoming` was
  // reassigned when the head had room, which threw on the first chunk that
  // overflowed the head budget and killed the smoke before it could report
  // anything. Exercising the real function is what makes this catch it.
  const { captureBounded } = loadCaptureBounded();
  const read = captureBounded(chattyStream(1));
  await new Promise((resolve) => setTimeout(resolve, 50));
  assert.doesNotThrow(() => read(), 'reading the captured output must not throw');
});
