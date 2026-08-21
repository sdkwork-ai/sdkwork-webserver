#!/usr/bin/env node

import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { ensureTrackedBuildSources } from '../../../scripts/lib/build-source-integrity.mjs';

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPO_ROOT = path.resolve(APP_ROOT, '../..');
const BUILD_SOURCES = [
  'apps/sdkwork-webserver-h5/package.json',
  'apps/sdkwork-webserver-h5/tsconfig.json',
  'apps/sdkwork-webserver-h5/vite.config.ts',
  'apps/sdkwork-webserver-h5/src/main.tsx',
  'apps/sdkwork-webserver-h5/src/App.tsx',
];

ensureTrackedBuildSources({ repoRoot: REPO_ROOT, relativePaths: BUILD_SOURCES });
console.log('[sdkwork-webserver-h5] build-critical sources verified');
