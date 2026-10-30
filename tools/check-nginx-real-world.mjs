#!/usr/bin/env node
/**
 * Real-world nginx configuration gate.
 *
 * Loads the committed production configuration tree
 * (`tests/nginx/real-world/production-api-birdcoder`) through the gateway's
 * `validate-nginx` command with the operator compatibility settings the nginx
 * grammar cannot express (the request-time document-root allowlist) and fails
 * the gate unless:
 *
 * - the command exits 0 (the whole tree materializes, validates, and compiles);
 * - the tree reports `skipped=0` (no site file was dropped because of a
 *   fail-closed directive);
 * - the loaded surface is non-empty (virtual hosts, listeners, resources,
 *   upstreams);
 * - no error-level diagnostic or fail-closed marker appears in the output.
 *
 * The gate exists because the synthetic differential slices only cover the
 * directives they declare; the production tree is the counterweight that keeps
 * the bounded adapter honest about real nginx configurations.
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const FIXTURE = path.join(root, 'tests', 'nginx', 'real-world', 'production-api-birdcoder', 'nginx.conf');
const BINARY = path.join(root, 'target', 'debug', 'sdkwork-api-webserver-standalone-gateway');
const DOCUMENT_ROOT_ALLOWLIST = '/opt/vuesource';

const log = (message) => process.stderr.write(`[nginx-real-world] ${message}\n`);

if (staleness(BINARY, root).stale) {
  log(stalenessMessage(BINARY, root, 'node tools/check-nginx-real-world.mjs'));
  process.exit(1);
}

for (const [label, target] of [
  ['fixture', FIXTURE],
  ['gateway binary', BINARY],
]) {
  if (!fs.existsSync(target)) {
    log(`${label} does not exist: ${path.relative(root, target)}`);
    process.exit(1);
  }
}

const result = spawnSync(
  BINARY,
  ['validate-nginx', FIXTURE],
  {
    cwd: root,
    encoding: 'utf8',
    env: {
      ...process.env,
      SDKWORK_WEBSERVER_DOCUMENT_ROOT_ALLOWLIST: DOCUMENT_ROOT_ALLOWLIST,
    },
    timeout: 180_000,
  },
);
const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;

if (result.error) {
  log(`validate-nginx could not run: ${result.error.message}`);
  process.exit(1);
}
if (result.status !== 0) {
  for (const line of output.split('\n')) {
    if (line.trim()) log(line.trim());
  }
  log(`validate-nginx exited ${result.status}`);
  process.exit(1);
}

const validated = output.match(
  /validated nginx compatibility: .*?virtualHosts=(\d+) listeners=(\d+) resources=(\d+) upstreams=(\d+) .*?skipped=(\d+)/,
);
if (!validated) {
  log('validate-nginx did not report the expected materialized surface');
  process.exit(1);
}
const [, virtualHosts, listeners, resources, upstreams, skipped] = validated.map(Number);
const failures = [];
if (virtualHosts < 1) failures.push(`virtualHosts=${virtualHosts}`);
if (listeners < 1) failures.push(`listeners=${listeners}`);
if (resources < 1) failures.push(`resources=${resources}`);
if (upstreams < 1) failures.push(`upstreams=${upstreams}`);
if (skipped !== 0) failures.push(`skipped=${skipped}`);

const failClosedPatterns = [
  /\bERROR\b/,
  /\bfail(?:ed|s)?[- ]closed\b/i,
  /is not executable/i,
  /is not supported by the runtime/i,
  /unsupported (?:server|http|location) directive/i,
];
for (const pattern of failClosedPatterns) {
  if (pattern.test(output)) {
    failures.push(`output matches ${pattern}`);
  }
}
if (failures.length > 0) {
  for (const line of output.split('\n')) {
    if (line.trim()) log(line.trim());
  }
  log(`real-world nginx configuration failed: ${failures.join('; ')}`);
  process.exit(1);
}

log(
  `ok: virtualHosts=${virtualHosts} listeners=${listeners} resources=${resources} upstreams=${upstreams} skipped=${skipped}`,
);
process.exit(0);
