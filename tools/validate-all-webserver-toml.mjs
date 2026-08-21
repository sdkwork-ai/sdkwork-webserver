#!/usr/bin/env node
/**
 * Validate every deployments/webserver layout under sdkwork-space.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const checker = path.join(workspaceRoot, 'sdkwork-specs', 'tools', 'check-webserver-toml-standard.mjs');

function listRoots() {
  const result = spawnSync(
    'rg',
    [
      '-l',
      'kind = .sdkwork.webserver.server.',
      workspaceRoot,
      '--glob',
      '**/deployments/webserver/server.common.toml',
      '--glob',
      '!**/node_modules/**',
    ],
    { encoding: 'utf8' },
  );
  return (result.stdout || '')
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((file) => path.resolve(file, '..', '..', '..'));
}

const roots = [...new Set(listRoots())].sort();
const failures = [];
let ok = 0;
for (const root of roots) {
  const run = spawnSync(process.execPath, [checker, '--root', root], {
    encoding: 'utf8',
  });
  const name = path.basename(root);
  if (run.status === 0) {
    ok += 1;
    console.log(`ok  ${name}`);
  } else {
    failures.push({ name, root, stdout: run.stdout, stderr: run.stderr, status: run.status });
    console.log(`FAIL ${name}`);
    if (run.stdout) process.stdout.write(run.stdout);
    if (run.stderr) process.stderr.write(run.stderr);
  }
}

console.log(JSON.stringify({ roots: roots.length, ok, failed: failures.length }, null, 2));
if (failures.length) {
  process.exitCode = 1;
}
