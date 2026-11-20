#!/usr/bin/env node
/**
 * Run the formal deployment capability regression on a remote edge host.
 *
 * Thin SSH orchestrator for `deployment-regression.py`: the repository is
 * pulled on the edge host first (the deployment under test is built from that
 * checkout), so this wrapper only forwards the selected mode and targets and
 * propagates the remote exit code. Evidence lives where the deployment lives;
 * pass --json-out to also mirror the JSON verdict locally.
 *
 * Usage:
 *   node tests/nginx/edge-parity/run-remote-regression.mjs \
 *       --host api-dev.birdcoder.com --mode full-edge \
 *       [--repo /opt/source/sdkwork-space/sdkwork-webserver] \
 *       [--environment production] [--domain server.sdkwork.com] \
 *       [--json-out target/evidence/full-edge.json]
 *
 * Exit codes mirror the remote runner: 0 aligned, 1 drift, 2 usage error.
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));

const { values } = parseArgs({
  options: {
    host: { type: 'string' },
    mode: { type: 'string' },
    repo: { type: 'string' },
    environment: { type: 'string', default: 'production' },
    domain: { type: 'string' },
    'http-target': { type: 'string' },
    'tls-target': { type: 'string' },
    gateway: { type: 'string' },
    'json-out': { type: 'string' },
  },
  strict: true,
});

if (!values.host || !values.mode) {
  console.error('usage: run-remote-regression.mjs --host <host> --mode <installer|full-edge> [options]');
  process.exit(2);
}
if (!['installer', 'full-edge'].includes(values.mode)) {
  console.error(`unknown mode: ${values.mode}`);
  process.exit(2);
}

const repo = values.repo ?? '/opt/source/sdkwork-space/sdkwork-webserver';
const remoteScript = `${repo}/tests/nginx/edge-parity/deployment-regression.py`;

const argv = [
  '-o',
  'BatchMode=yes',
  '-o',
  'ConnectTimeout=15',
  values.host,
  `python3 ${remoteScript} --mode ${values.mode} --environment ${values.environment}`,
];
if (values.domain) argv.push(`--domain ${values.domain}`);
if (values['http-target']) argv.push(`--http ${values['http-target']}`);
if (values['tls-target']) argv.push(`--tls ${values['tls-target']}`);
if (values.gateway) argv.push(`--gateway ${values.gateway}`);

let jsonTmpPath;
if (values['json-out']) {
  const remoteJson = '/tmp/sdkwork-deployment-regression-verdict.json';
  argv.push(`--json ${remoteJson}`);
  jsonTmpPath = remoteJson;
}

console.log(`[remote-regression] ssh ${values.host}: mode=${values.mode} repo=${repo}`);
const result = spawnSync('ssh', argv, { stdio: 'inherit' });

if (result.error) {
  console.error(`[remote-regression] ssh failed: ${result.error.message}`);
  process.exit(2);
}

if (jsonTmpPath && result.status === 0 && values['json-out']) {
  const fetch = spawnSync(
    'scp',
    ['-q', `${values.host}:${jsonTmpPath}`, values['json-out']],
    { stdio: 'inherit' },
  );
  if (fetch.status !== 0) {
    console.error('[remote-regression] warning: verdict JSON copy failed');
  } else {
    const verdict = JSON.parse(fs.readFileSync(values['json-out'], 'utf8'));
    console.log(`[remote-regression] verdict: ${verdict.verdict}`);
  }
}

process.exit(result.status ?? 1);
