#!/usr/bin/env node
/**
 * Aggregate nginx-compatibility release gate.
 *
 * The individual nginx gates exist as separate pnpm scripts, but nothing ran
 * them together: `_sdkwork:check` never invoked the gap catalog, corpus,
 * differential or behavioural runners, so a release could ship while every
 * compatibility claim in `specs/nginx-gap.catalog.json` was unexecuted. This
 * wrapper is the one entrypoint a release verification invokes, and it fails
 * closed:
 *
 * 1. static gates — gap-catalog shape, behavioral-corpus manifest consistency,
 *    directive support matrix drift (PRD-nginx-compatibility §3/§13);
 * 2. the in-process behavioural corpus (`tools/run-nginx-behavioral-corpus.mjs`);
 * 3. every differential slice that declares an `orchestrator` in the corpus
 *    manifest, discovered from the manifest instead of a hard-coded list so a
 *    new slice is gated automatically;
 * 4. `SDKWORK_REQUIRE_NGINX_EVIDENCE=1` for every executed child, which turns a
 *    "loud skip because this machine cannot execute the probe" into a failure.
 *    A green run therefore means the evidence actually executed against a fresh
 *    gateway build, not that the gate declined to look.
 *
 * Exit codes: 0 every step passed, 1 at least one step failed.
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'specs', 'nginx-behavioral-corpus.manifest.json');

/** Steps run with stdio inherited so a failure shows its own diagnostics. */
function run(label, command, args) {
  process.stdout.write(`\n[nginx-compat] >>> ${label}\n`);
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, SDKWORK_REQUIRE_NGINX_EVIDENCE: '1' },
  });
  if (result.error) {
    process.stdout.write(`[nginx-compat] FAIL ${label}: ${result.error.message}\n`);
    return false;
  }
  if (result.status !== 0) {
    process.stdout.write(`[nginx-compat] FAIL ${label}: exit ${result.status}\n`);
    return false;
  }
  process.stdout.write(`[nginx-compat] PASS ${label}\n`);
  return true;
}

function requireFile(relative, label) {
  if (fs.existsSync(path.join(root, relative))) return true;
  process.stdout.write(`[nginx-compat] FAIL ${label}: missing ${relative}\n`);
  return false;
}

const failures = [];

if (!requireFile('specs/nginx-behavioral-corpus.manifest.json', 'manifest')) {
  process.exit(1);
}

// 1. Static gates. The directive matrix is part of the declared compatibility
// contract (PRD §3/§13); a release without it has no reviewable support claim.
const staticGates = [
  ['gap catalog', 'tools/check-nginx-gap.mjs', ['--test', 'tools/check-nginx-gap.mjs']],
  [
    'corpus manifest',
    'tools/check-nginx-behavioral-corpus.mjs',
    ['--test', 'tools/check-nginx-behavioral-corpus.mjs'],
  ],
  [
    'directive matrix',
    'tools/check-nginx-directive-matrix.mjs',
    ['tools/check-nginx-directive-matrix.mjs'],
  ],
  [
    'directive audit',
    'tools/gen-nginx-directive-audit.mjs',
    ['tools/gen-nginx-directive-audit.mjs', '--check', '--verify-evidence'],
  ],
];
for (const [label, script, args] of staticGates) {
  if (!requireFile(script, label)) {
    failures.push(label);
    continue;
  }
  if (!run(label, 'node', args)) failures.push(label);
}

// 2. In-process behavioural regression (full-surface); the runner also enforces
// the manifest case floor so deleting cases cannot shrink coverage silently.
if (!requireFile('tools/run-nginx-behavioral-corpus.mjs', 'behavioural runner')) {
  failures.push('behavioural runner');
} else if (!run('behavioural corpus', 'node', ['tools/run-nginx-behavioral-corpus.mjs'])) {
  failures.push('behavioural corpus');
}

// 2b. Real-world acceptance gate: the committed production nginx snapshot
// (api.birdcoder.com, 33 files) must materialize with an operator document-root
// allowlist and no skipped files or fail-closed markers.
if (!requireFile('tools/check-nginx-real-world.mjs', 'real-world gate')) {
  failures.push('real-world gate');
} else if (!run('real-world config', 'node', ['tools/check-nginx-real-world.mjs'])) {
  failures.push('real-world config');
}

// 3. Differential slices are discovered from the manifest, not hard-coded.
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const orchestrators = [];
for (const slice of manifest.slices ?? []) {
  if (typeof slice.orchestrator === 'string' && !orchestrators.includes(slice.orchestrator)) {
    orchestrators.push(slice.orchestrator);
  }
}
if (orchestrators.length === 0) {
  process.stdout.write('[nginx-compat] FAIL: no differential slice orchestrator is indexed\n');
  failures.push('differential discovery');
}
for (const orchestrator of orchestrators) {
  if (!requireFile(orchestrator, `differential orchestrator ${orchestrator}`)) {
    failures.push(orchestrator);
    continue;
  }
  if (!run(orchestrator, 'node', [orchestrator])) failures.push(orchestrator);
}

if (failures.length > 0) {
  process.stdout.write(
    `\n[nginx-compat] FAILED: ${failures.length} step(s): ${failures.join(', ')}\n`,
  );
  process.exit(1);
}
process.stdout.write(
  `\n[nginx-compat] PASS: static gates, behavioural corpus and ${orchestrators.length} ` +
    'differential slice(s) executed against a fresh gateway build\n',
);
process.exit(0);
