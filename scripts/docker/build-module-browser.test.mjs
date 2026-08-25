#!/usr/bin/env node

import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import {
  buildModuleBrowser,
  DEPLOYMENT_ENVIRONMENT_ALIASES,
  resolveArchitectures,
} from './build-module-browser.mjs';
import { resolveModuleBrowserBuildPlan } from './run-module-browser-build.mjs';

const WORKSPACE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const IM_ROOT = path.join(WORKSPACE_ROOT, 'sdkwork-im');

test('DEPLOYMENT_ENVIRONMENT_ALIASES maps lifecycle tiers to dist aliases', () => {
  assert.equal(DEPLOYMENT_ENVIRONMENT_ALIASES.development, 'dev');
  assert.equal(DEPLOYMENT_ENVIRONMENT_ALIASES.production, 'prod');
});

test('resolveArchitectures all discovers pc and h5 for sdkwork-im', () => {
  const architectures = resolveArchitectures(IM_ROOT, 'all');
  assert.deepEqual(new Set(architectures), new Set(['pc', 'h5']));
});

test('buildModuleBrowser dry-run plans both surfaces for sdkwork-im dev', () => {
  const result = buildModuleBrowser({
    architecture: 'all',
    deploymentEnvironment: 'development',
    dryRun: true,
    module: 'sdkwork-im',
    spaceCheckoutRoot: WORKSPACE_ROOT,
  });
  assert.equal(result.module, 'sdkwork-im');
  assert.equal(result.plans.length, 2);
  assert.ok(result.plans.every((plan) => plan.outDir === 'dist/standalone/dev'));
});

test('resolveModuleBrowserBuildPlan defaults to host mode', () => {
  const plan = resolveModuleBrowserBuildPlan({
    architecture: 'pc',
    deploymentEnvironment: 'development',
    deploymentProfile: 'standalone',
    dryRun: true,
    module: 'sdkwork-im',
  }, {
    spaceCheckoutRoot: WORKSPACE_ROOT,
  });
  assert.equal(plan.mode, 'host');
  assert.equal(plan.plans.length, 1);
  assert.equal(plan.plans[0].architecture, 'pc');
});
