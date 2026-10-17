import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SMOKE_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-release-smoke.mjs');

// A packaged gateway prints one `sqlx::postgres::notice` line per migration
// statement. On this release that filled the whole 256 KiB budget with "already
// exists, skipping" before the process printed why it died, so the captured
// output ended mid-notice and the Validate step failed with a cause nobody could
// read. Retention has to keep the tail; the head alone is a diagnostic dead end.
function readSmokeSource() {
  return readFileSync(SMOKE_SCRIPT, 'utf8');
}

// Re-implemented from the script so the behaviour is exercised, not just matched
// as text. Keep in sync with captureBounded.
function captureBounded(stream, head = 64 * 1024, tail = 192 * 1024) {
  let headBuffer = Buffer.alloc(0);
  let tailBuffer = Buffer.alloc(0);
  stream.on('data', (chunk) => {
    let incoming = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    if (headBuffer.length < head) {
      const room = head - headBuffer.length;
      headBuffer = Buffer.concat([headBuffer, incoming.subarray(0, room)]);
      incoming = incoming.subarray(room);
    }
    if (incoming.length === 0) {
      return;
    }
    tailBuffer = Buffer.concat([tailBuffer, incoming]);
    if (tailBuffer.length > tail) {
      tailBuffer = tailBuffer.subarray(tailBuffer.length - tail);
    }
  });
  return () => {
    if (tailBuffer.length === 0) {
      return headBuffer.toString('utf8');
    }
    return `${headBuffer.toString('utf8')}\n${tailBuffer.toString('utf8')}`;
  };
}

test('the smoke budgets process output so the failure reason survives', () => {
  const source = readSmokeSource();

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

test('a chatty process still reports why it exited', () => {
  // 20k notices is roughly what the migration emits, comfortably over the budget.
  const notices = 20_000;
  const stream = Readable.from(
    (function* emit() {
      for (let index = 0; index < notices; index += 1) {
        yield `migration notice line ${index} already exists, skipping\n`;
      }
      yield 'ERROR: the real reason the process died\n';
    })(),
  );

  const read = captureBounded(stream);

  return new Promise((resolve) => {
    setTimeout(() => {
      const output = read();
      assert.ok(
        output.includes('ERROR: the real reason the process died'),
        'the last line the process printed is the diagnosis and must be retained',
      );
      assert.ok(output.includes('migration notice line 0 '), 'the head is kept for context');
      assert.ok(output.includes(`migration notice line ${notices - 1} `), 'the tail is kept whole');
      assert.ok(
        Buffer.byteLength(output) <= 64 * 1024 + 192 * 1024 + 64,
        'retention stays within the declared budget',
      );
      resolve();
    }, 50);
  });
});
