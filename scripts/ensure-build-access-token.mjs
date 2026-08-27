#!/usr/bin/env node
/**
 * Thin wrapper around the canonical specs ensure-build-access-token tool so
 * webserver scripts can invoke a repo-local path.
 *
 * Usage:
 *   node scripts/ensure-build-access-token.mjs --app-root apps/sdkwork-webserver-pc --environment development
 */
export { ensureBuildAccessToken } from '../../sdkwork-specs/tools/ensure-build-access-token.mjs';

import { fileURLToPath } from 'node:url';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';

const SPECS_SCRIPT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'sdkwork-specs',
  'tools',
  'ensure-build-access-token.mjs',
);

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = spawnSync(process.execPath, [SPECS_SCRIPT, ...process.argv.slice(2)], {
    stdio: 'inherit',
    env: process.env,
  });
  process.exit(result.status ?? 1);
}
