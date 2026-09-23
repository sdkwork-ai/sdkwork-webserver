#!/usr/bin/env node
/**
 * Validate every deployments/webserver layout under sdkwork-space.
 *
 * Discovery used to shell out to `rg`. When ripgrep is not installed spawnSync
 * fails with ENOENT, the file list comes back empty, and the script reported
 * `{ roots: 0, ok: 0, failed: 0 }` with exit code 0 — a workspace-wide gate
 * that green-lit having validated nothing. Discovery is now a plain directory
 * walk, and an empty result is a failure, never a pass.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const checker = path.join(workspaceRoot, 'sdkwork-specs', 'tools', 'check-webserver-toml-standard.mjs');

const SKIP_DIRS = new Set([
  'node_modules',
  'target',
  '.git',
  'dist',
  'build',
  'tmp',
  '.venv',
  '.next',
  'coverage',
  // Frozen historical snapshots (pin-freeze/README.md): 37 archived repo copies,
  // NOT live workspace members. They are intentionally never modernized, so
  // validating them reports stale findings against repos that are already
  // correct — and their `sdkwork-*` basenames collide with the live modules,
  // misattributing the failure. Exclusion is the caller's job by design.
  'pin-freeze',
]);
const COMMON_REL = path.join('deployments', 'webserver', 'server.common.toml');
const KIND_MARKER = 'kind = "sdkwork.webserver.server"';

function listRoots() {
  const found = [];
  const walk = (dir, depth) => {
    if (depth > 6) return;
    let entries;
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.isDirectory()) {
        if (SKIP_DIRS.has(entry.name) || entry.isSymbolicLink()) continue;
        walk(path.join(dir, entry.name), depth + 1);
        continue;
      }
      if (!entry.isFile()) continue;
      const full = path.join(dir, entry.name);
      if (!full.endsWith(COMMON_REL)) continue;
      let body = '';
      try {
        body = fs.readFileSync(full, 'utf8');
      } catch {
        continue;
      }
      if (body.includes(KIND_MARKER)) found.push(path.resolve(full, '..', '..', '..'));
    }
  };
  walk(workspaceRoot, 0);
  return found;
}

const roots = [...new Set(listRoots())].sort();
if (roots.length === 0) {
  console.error(
    `validate-all-webserver-toml: discovered 0 webserver layout roots under ${workspaceRoot}\n`
      + 'An empty discovery is a failure, not a pass — check the workspace layout and this walker.',
  );
  process.exit(1);
}

const failures = [];
let ok = 0;
for (const root of roots) {
  const run = spawnSync(process.execPath, [checker, '--root', root], { encoding: 'utf8' });
  // Report the workspace-relative path, not the basename: a bare `sdkwork-x`
  // cannot distinguish the live module from a same-named copy elsewhere in the
  // workspace, which previously blamed the wrong repo.
  const label = path.relative(workspaceRoot, root).split(path.sep).join('/');
  if (run.status === 0) {
    ok += 1;
    console.log(`ok   ${label}`);
  } else {
    failures.push({ label, root, status: run.status });
    console.log(`FAIL ${label}`);
    if (run.stdout) process.stdout.write(run.stdout);
    if (run.stderr) process.stderr.write(run.stderr);
  }
}

console.log(JSON.stringify({ roots: roots.length, ok, failed: failures.length }, null, 2));
if (failures.length) {
  process.exitCode = 1;
}
