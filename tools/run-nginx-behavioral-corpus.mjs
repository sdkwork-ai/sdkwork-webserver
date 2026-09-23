#!/usr/bin/env node
/**
 * Execute the indexed nginx behavioural corpus.
 *
 * `check-nginx-behavioral-corpus.mjs` only proves the manifest and its fixture
 * files stay in sync; nothing in the repository, the pnpm scripts, or CI ever
 * *ran* the probes. That left the corpus as declared coverage: 76 full-surface
 * cases plus five recording slices that no gate executed.
 *
 * This runner closes that gap for the self-asserting slices:
 *
 * - the `full-surface` slice owns its mock upstreams, renders the staged
 *   `nginx.conf`, spawns the gateway binary and asserts 76 behaviour cases, so
 *   it is executed here and a single failing case fails the gate;
 * - `conditional-cache` asserts through its own orchestrator (it compares two
 *   endpoints and owns a captured oracle), so it is reported as delegated
 *   instead of being counted as executed here;
 * - the remaining five slices are recording tools (they print a request/response
 *   matrix against an externally started server), so they are reported as
 *   skipped instead of being counted as executed evidence.
 *
 * Binary resolution order: `SDKWORK_WEBSERVER_GATEWAY_BIN`, then the workspace
 * `target/{debug,release}` trees, then `CARGO_TARGET_DIR/{debug,release}`.
 * When none exists the runner builds the binary unless
 * `SDKWORK_WEBSERVER_SKIP_BUILD` is set, and skips with a loud notice when the
 * build itself cannot run (no cargo) rather than passing silently.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'specs', 'nginx-behavioral-corpus.manifest.json');
const SELF_ASSERTING_SLICE = 'full-surface';
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const EXECUTION_TIMEOUT_MS = 10 * 60 * 1000;

/**
 * Three kinds of slice, and the distinction is the point:
 *
 * - `executed` asserts in-process, so this runner fails the gate on a bad case;
 * - `delegated` also asserts, but through its own orchestrator (a slice that
 *   needs a second endpoint cannot be driven from here); reporting it as skipped
 *   would misdescribe working coverage as absent;
 * - `skipped` only records behaviour against an externally started server, so it
 *   can never be counted as execution evidence.
 *
 * A slice that asserts and is neither executed nor delegated would be silently
 * counted as covered, so the guard below fails instead.
 */
function classifySlices(manifest) {
  const executed = [];
  const delegated = [];
  const skipped = [];
  for (const slice of manifest.slices) {
    if (slice.id === SELF_ASSERTING_SLICE) executed.push(slice);
    else if (slice.orchestrator) delegated.push(slice);
    else skipped.push(slice);
  }
  if (executed.length !== 1) {
    throw new Error(
      `expected exactly one in-process self-asserting slice (${SELF_ASSERTING_SLICE}); ` +
        `found ${executed.length} — extend this runner before claiming execution evidence`,
    );
  }
  return { executed, delegated, skipped };
}

function candidateBinaries() {
  const candidates = [];
  if (process.env.SDKWORK_WEBSERVER_GATEWAY_BIN) {
    candidates.push(process.env.SDKWORK_WEBSERVER_GATEWAY_BIN);
  }
  const targetRoots = [path.join(root, 'target')];
  if (process.env.CARGO_TARGET_DIR) {
    targetRoots.push(path.resolve(root, process.env.CARGO_TARGET_DIR));
  }
  for (const targetRoot of targetRoots) {
    for (const profile of ['debug', 'release']) {
      for (const suffix of ['.exe', '']) {
        candidates.push(path.join(targetRoot, profile, `${BINARY_STEM}${suffix}`));
      }
    }
  }
  return candidates;
}

function resolvePython() {
  for (const candidate of ['python3', 'python']) {
    const probe = spawnSync(candidate, ['--version'], { encoding: 'utf8' });
    if (probe.status === 0) return candidate;
  }
  return null;
}

/**
 * Pick the newest existing candidate.
 *
 * Taking the first path that exists silently exercises a stale build whenever
 * `target/debug` cannot be refreshed — an operator's running server keeps that
 * executable locked on Windows — so the gate would be green while the revision
 * under test was never executed. The newest modification time always belongs to
 * the build that was just produced, and it is printed so the evidence names the
 * artefact it came from.
 */
function findBinary() {
  let best = null;
  for (const candidate of candidateBinaries()) {
    let stat;
    try {
      stat = fs.statSync(candidate);
    } catch {
      continue;
    }
    if (!stat.isFile()) continue;
    if (best === null || stat.mtimeMs > best.mtimeMs) {
      best = { path: candidate, mtimeMs: stat.mtimeMs };
    }
  }
  if (best === null) return null;
  process.stderr.write(
    `[nginx-corpus] binary: ${best.path} (mtime ${new Date(best.mtimeMs).toISOString()})\n`,
  );
  return best.path;
}

function buildBinary() {
  const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8' });
  if (cargo.status !== 0) return { ok: false, reason: 'cargo is unavailable on PATH' };
  process.stderr.write(`[nginx-corpus] building ${BINARY_STEM} (first run only)…\n`);
  const build = spawnSync(
    'cargo',
    ['build', '-p', BINARY_STEM, '--features', 'management'],
    { cwd: root, stdio: 'inherit' },
  );
  if (build.status !== 0) {
    return { ok: false, reason: `cargo build exited with ${build.status}` };
  }
  return { ok: true };
}

/**
 * Refuse to run while the slice's ports are already served.
 *
 * The probe only checks that the port accepts connections; it never verifies
 * that the process it spawned is the one listening. A stale server left on the
 * fixture port therefore makes every case pass against the *old* build — the
 * gate would be green while the revision under test was never exercised. Binding
 * each port first turns that silent false pass into a loud failure.
 */
function assertPortsFree(base, count) {
  return new Promise((resolve) => {
    const ports = Array.from({ length: count }, (_, offset) => base + offset);
    const busy = [];
    let pending = ports.length;
    for (const port of ports) {
      const server = net.createServer();
      server.once('error', () => {
        busy.push(port);
        if (--pending === 0) resolve(busy);
      });
      server.once('listening', () => {
        server.close(() => {
          if (--pending === 0) resolve(busy);
        });
      });
      server.listen(port, '127.0.0.1');
    }
  });
}

function runSlice(slice, binary, python, portBase) {
  return new Promise((resolve) => {
    const script = path.join(root, manifest.root, slice.dir, slice.probe);
    const child = spawn(python, [script, '--server-bin', binary, '--port-base', String(portBase)], {
      cwd: root,
      env: { ...process.env, PROBE_TRACEBACK: '1' },
    });
    let output = '';
    const collect = (chunk) => {
      output += chunk.toString('utf8');
    };
    child.stdout.on('data', collect);
    child.stderr.on('data', collect);
    const timer = setTimeout(() => {
      child.kill();
      resolve({ ok: false, output: `${output}\n[nginx-corpus] timed out after ${EXECUTION_TIMEOUT_MS} ms` });
    }, EXECUTION_TIMEOUT_MS);
    child.on('error', (error) => {
      clearTimeout(timer);
      resolve({ ok: false, output: `${output}\n[nginx-corpus] ${error.message}` });
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      resolve({ ok: code === 0, output });
    });
  });
}

/** `FULL-SURFACE REGRESSION: 66/66 passed` */
function parseSummary(output) {
  const match = output.match(/REGRESSION:\s*(\d+)\/(\d+)\s+passed/);
  if (!match) return null;
  return { passed: Number(match[1]), total: Number(match[2]) };
}

const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const { executed, delegated, skipped } = classifySlices(manifest);

const python = resolvePython();
if (!python) {
  process.stderr.write('[nginx-corpus] SKIP: no python3/python on PATH; cannot execute probes\n');
  process.exit(0);
}

let binary = findBinary();
if (!binary) {
  if (process.env.SDKWORK_WEBSERVER_SKIP_BUILD) {
    process.stderr.write(
      `[nginx-corpus] SKIP: no ${BINARY_STEM} binary found and SDKWORK_WEBSERVER_SKIP_BUILD is set\n`,
    );
    process.exit(0);
  }
  const built = buildBinary();
  if (!built.ok) {
    process.stderr.write(
      `[nginx-corpus] SKIP: cannot build the gateway binary (${built.reason}); ` +
        'the corpus was NOT executed\n',
    );
    process.exit(0);
  }
  binary = findBinary();
  if (!binary) {
    process.stderr.write('[nginx-corpus] FAIL: build succeeded but no binary was produced\n');
    process.exit(1);
  }
}

// A green corpus run against a stale binary proves nothing about the revision
// under test, so staleness is a failure rather than a warning.
if (staleness(binary, root).stale) {
  process.stderr.write(
    `[nginx-corpus] FAIL: ${stalenessMessage(binary, root, 'node tools/run-nginx-behavioral-corpus.mjs')}\n`,
  );
  process.exit(1);
}

const portBase = Number(
  process.env.SDKWORK_NGINX_CORPUS_PORT_BASE ?? executed[0].listenPort ?? 19890,
);

let failures = 0;
for (const slice of executed) {
  // The slice owns portBase..portBase+8 (portBase+9 stays intentionally dead).
  const busy = await assertPortsFree(portBase, 9);
  if (busy.length > 0) {
    process.stdout.write(
      `[nginx-corpus] FAIL ${slice.id}: fixture ports already served (${busy.join(', ')}). ` +
        'A stale server would make every case pass against the wrong build; stop it or ' +
        'set SDKWORK_NGINX_CORPUS_PORT_BASE to a free range.\n',
    );
    failures += 1;
    continue;
  }
  process.stdout.write(`[nginx-corpus] executing ${slice.id} (${slice.req})…\n`);
  const result = await runSlice(slice, binary, python, portBase);
  const summary = parseSummary(result.output);
  if (!summary) {
    process.stdout.write(
      `[nginx-corpus] FAIL ${slice.id}: no case summary in probe output\n${result.output}\n`,
    );
    failures += 1;
    continue;
  }
  if (!result.ok || summary.passed !== summary.total) {
    const rule = '-'.repeat(72);
    process.stdout.write(
      `[nginx-corpus] FAIL ${slice.id}: ${summary.passed}/${summary.total} cases passed\n` +
        `${rule}\n${result.output}\n${rule}\n`,
    );
    failures += 1;
    continue;
  }
  // A slice that selects zero cases reports `0/0 passed`, which must not read
  // as green: require the declared case floor so deleting or filtering cases
  // cannot silently shrink coverage.
  if (typeof slice.cases === 'number' && summary.total !== slice.cases) {
    process.stdout.write(
      `[nginx-corpus] FAIL ${slice.id}: executed ${summary.total} cases but the ` +
        `manifest declares ${slice.cases}; update the manifest when the case matrix changes\n`,
    );
    failures += 1;
    continue;
  }
  if (summary.total === 0) {
    process.stdout.write(`[nginx-corpus] FAIL ${slice.id}: executed zero cases\n`);
    failures += 1;
    continue;
  }
  process.stdout.write(
    `[nginx-corpus] PASS ${slice.id}: ${summary.passed}/${summary.total} cases passed\n`,
  );
}

for (const slice of delegated) {
  process.stdout.write(
    `[nginx-corpus] delegated ${slice.id} (${slice.req}): executed by ${slice.orchestrator}\n`,
  );
}

for (const slice of skipped) {
  process.stdout.write(
    `[nginx-corpus] skipped ${slice.id} (${slice.req}): recording slice, ` +
      'needs an operator-started server\n',
  );
}

if (failures > 0) {
  process.stderr.write(`[nginx-corpus] ${failures} slice(s) failed\n`);
  process.exit(1);
}
process.stdout.write('[nginx-corpus] nginx behavioural corpus executed and green\n');
